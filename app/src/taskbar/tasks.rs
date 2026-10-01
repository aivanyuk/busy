//! Where the task buttons are, read through UI Automation. On 26H2 explorer's legacy task list window
//! (`ReBarWindow32`) no longer follows the XAML buttons: with left- or top-aligned icons it understates their
//! extent, and a widget placed after it covers the last ones. The buttons' automation peers have their real
//! rects. UIA calls go into explorer and can block, so they run on a worker thread, never the UI thread.

use crate::worker::Worker;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::Accessibility::*;
use windows::core::{Interface, Result};

/// The peer holding the taskbar's buttons, and the classes of those that form the icon group: Start, Task View
/// and Search are toggle buttons, apps are task list buttons. Widgets and Copilot sit apart from the group.
const FRAME: &str = "Taskbar.TaskbarFrameAutomationPeer";
const BUTTONS: [&str; 2] = ["ToggleButton", "Taskbar.TaskListButtonAutomationPeer"];
/// Bounds a call into a hung explorer, which would otherwise hold up exit (the worker is joined).
const CONNECTION_TIMEOUT_MS: u32 = 1000;
const TRANSACTION_TIMEOUT_MS: u32 = 2000;

pub(super) struct Tasks {
    /// Scans the taskbar window it is given.
    worker: Worker<isize>,
    /// The icon group's screen rect from the last scan that found one.
    latest: Arc<Mutex<Option<RECT>>>,
}

impl Tasks {
    pub(super) fn start() -> Self {
        let latest = Arc::new(Mutex::new(None));
        let out = latest.clone();
        let worker = Worker::start("busy-tasks", move |tray: isize| {
            let found = UIA.with_borrow_mut(|uia| {
                if uia.is_none() {
                    *uia = Uia::new().ok();
                }
                uia.as_mut().and_then(|u| u.scan(HWND(tray as _)))
            });
            // A failed scan keeps the last rect: failures are mostly transient (a timeout while explorer is
            // busy, the frame looked up again), and dropping back to the stale ReBarWindow32 would move the
            // widget onto the icons for a tick. A new taskbar window comes with a new `Tasks`, so a rect never
            // outlives the taskbar it was read from.
            if let (Some(r), Ok(mut latest)) = (found, out.lock()) {
                *latest = Some(r);
            }
        });
        Self { worker, latest }
    }

    /// Queues a scan of `tray`; never blocks. Its result shows in `latest` once done.
    pub(super) fn request(&self, tray: HWND) {
        self.worker.submit(tray.0 as isize);
    }

    /// The icon group's screen rect, if a scan found it.
    pub(super) fn latest(&self) -> Option<RECT> {
        self.latest.lock().ok().and_then(|l| *l)
    }
}

thread_local! {
    /// The worker's UIA client; COM objects stay on the thread that made them.
    static UIA: RefCell<Option<Uia>> = const { RefCell::new(None) };
}

struct Uia {
    automation: IUIAutomation,
    /// Fetches class names and rects with the elements, in one cross-process call.
    cache: IUIAutomationCacheRequest,
    all: IUIAutomationCondition,
    /// The buttons' parent, found once per taskbar window and kept while it answers.
    frame: Option<(HWND, IUIAutomationElement)>,
}

impl Uia {
    fn new() -> Result<Self> {
        // SAFETY: COM initialization and object creation on this (worker) thread; it stays in the MTA for its
        // whole life, so nothing balances it.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            if let Ok(a2) = automation.cast::<IUIAutomation2>() {
                let _ = a2.SetConnectionTimeout(CONNECTION_TIMEOUT_MS);
                let _ = a2.SetTransactionTimeout(TRANSACTION_TIMEOUT_MS);
            }
            let cache = automation.CreateCacheRequest()?;
            cache.AddProperty(UIA_ClassNamePropertyId)?;
            cache.AddProperty(UIA_BoundingRectanglePropertyId)?;
            let all = automation.CreateTrueCondition()?;
            Ok(Self { automation, cache, all, frame: None })
        }
    }

    /// The union of the icon group's buttons in screen pixels; `None` if the taskbar has no such peers (an
    /// older Windows) or the scan failed.
    fn scan(&mut self, tray: HWND) -> Option<RECT> {
        if self.frame.as_ref().is_none_or(|f| f.0 != tray) {
            self.frame = self.find_frame(tray).map(|f| (tray, f));
        }
        let (_, frame) = self.frame.as_ref()?;
        // SAFETY: calls on live COM objects; a stale frame fails and is looked up again next time.
        let kids = unsafe { frame.FindAllBuildCache(TreeScope_Children, &self.all, &self.cache) };
        let Ok(kids) = kids else {
            self.frame = None;
            return None;
        };
        let mut group: Option<RECT> = None;
        for kid in elements(&kids) {
            // SAFETY: cached properties of a live element, read without a cross-process call.
            let (class, r) = unsafe { (kid.CachedClassName(), kid.CachedBoundingRectangle()) };
            let (Ok(class), Ok(r)) = (class, r) else { continue };
            if r.right <= r.left || r.bottom <= r.top || !BUTTONS.iter().any(|b| class == *b) {
                continue;
            }
            group = Some(group.map_or(r, |g| RECT {
                left: g.left.min(r.left),
                top: g.top.min(r.top),
                right: g.right.max(r.right),
                bottom: g.bottom.max(r.bottom),
            }));
        }
        group
    }

    fn find_frame(&self, tray: HWND) -> Option<IUIAutomationElement> {
        // SAFETY: calls on live COM objects; `tray` may be stale, which fails the call.
        let found = unsafe {
            let root = self.automation.ElementFromHandle(tray).ok()?;
            root.FindAllBuildCache(TreeScope_Descendants, &self.all, &self.cache).ok()?
        };
        // SAFETY: a cached property of a live element.
        elements(&found).find(|e| unsafe { e.CachedClassName() }.is_ok_and(|c| c == FRAME))
    }
}

fn elements(a: &IUIAutomationElementArray) -> impl Iterator<Item = IUIAutomationElement> + '_ {
    // SAFETY: indexes stay below the array's own length.
    let n = unsafe { a.Length() }.unwrap_or(0);
    (0..n).filter_map(move |i| unsafe { a.GetElement(i) }.ok())
}

#[cfg(test)]
mod tests {
    use super::Uia;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::w;

    /// Real taskbar: a found icon group has a real rect, and a second scan of an unchanged taskbar agrees.
    /// Skipped without a shell (CI services), without UIA, or on a taskbar without the XAML peers (older
    /// Windows), like the other hardware-dependent tests.
    #[test]
    fn finds_the_icon_group() {
        // SAFETY: looks a window up by class; no pointers are passed.
        let Ok(tray) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else { return };
        let Ok(mut uia) = Uia::new() else { return };
        let t = std::time::Instant::now();
        let first = uia.scan(tray);
        let cold = t.elapsed();
        let t = std::time::Instant::now();
        let again = uia.scan(tray);
        println!("icon group {first:?}: first scan {cold:?}, then {:?}", t.elapsed());
        let Some(g) = first else { return };
        assert!(g.right > g.left && g.bottom > g.top);
        assert_eq!(again, first);
    }
}
