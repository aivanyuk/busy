//! Registry reads and writes, and opening links, for the window, on a thread of its own. The window runs on
//! the host's UI thread, whose input queue is attached to explorer's taskbar, so it never waits on the
//! registry or on the shell starting a browser.

use crate::{autostart, dark};
use std::sync::mpsc::{Receiver, Sender, channel};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_APP};
use windows::core::{HSTRING, w};

/// Posted to the window when replies are ready; the window drains [`Worker::replies`].
pub(super) const WM_APP_REPLY: u32 = WM_APP + 1;

pub(super) enum Job {
    /// The state the window opens with.
    Read,
    /// Re-reads the Windows app mode after a theme broadcast.
    Theme,
    SetAutostart(bool),
    /// Opens an https URL in the default browser. No reply.
    Open(String),
}

pub(super) enum Reply {
    Read {
        autostart: bool,
        system_dark: bool,
    },
    Theme {
        system_dark: bool,
    },
    /// `autostart` is the registry state read back after the write, whether or not it failed.
    SetAutostart {
        error: Option<String>,
        autostart: bool,
    },
}

fn run(job: Job) -> Option<Reply> {
    Some(match job {
        Job::Read => Reply::Read { autostart: autostart::is_enabled(), system_dark: dark::system_dark() },
        Job::Theme => Reply::Theme { system_dark: dark::system_dark() },
        Job::SetAutostart(on) => {
            let error = autostart::set(on).err().map(|e| e.message());
            Reply::SetAutostart { error, autostart: autostart::is_enabled() }
        }
        Job::Open(url) => {
            open(&url);
            return None;
        }
    })
}

/// Hands `url` to the shell, which starts the default browser; nothing else is ever opened. A failure (no
/// browser registered) is ignored: the click just does nothing.
fn open(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    // SAFETY: plain call; the shell's COM extensions want an STA without OLE1 DDE. A repeated call on this
    // thread returns S_FALSE; the thread never uninitializes, as it ends with the window.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    // SAFETY: the strings are NUL-terminated and outlive the call; no owner window, so no UI waits on it.
    unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(url), None, None, SW_SHOWNORMAL) };
}

/// One thread per open window. Dropping the worker with the window ends the thread after its current
/// job; nothing joins it, so closing the window never waits.
pub(super) struct Worker {
    jobs: Option<Sender<Job>>,
    replies: Receiver<Reply>,
}

impl Worker {
    pub(super) fn start(hwnd: HWND) -> Self {
        let (jobs, job_rx) = channel::<Job>();
        let (reply_tx, replies) = channel();
        let target = hwnd.0 as isize;
        let spawned = std::thread::Builder::new()
            .name("busy-settings".into())
            .spawn(move || {
                for job in job_rx {
                    let Some(reply) = run(job) else { continue };
                    if reply_tx.send(reply).is_err() {
                        break;
                    }
                    // SAFETY: PostMessageW only queues a message; a destroyed window's handle makes it fail.
                    let _ = unsafe { PostMessageW(Some(HWND(target as _)), WM_APP_REPLY, WPARAM(0), LPARAM(0)) };
                }
            })
            .is_ok();
        Self { jobs: spawned.then_some(jobs), replies }
    }

    /// Queues `job`; never blocks. False if the thread isn't running.
    pub(super) fn submit(&self, job: Job) -> bool {
        self.jobs.as_ref().is_some_and(|tx| tx.send(job).is_ok())
    }

    pub(super) fn replies(&self) -> impl Iterator<Item = Reply> + '_ {
        self.replies.try_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{Job, Reply, Worker};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::HWND;

    #[test]
    fn replies_arrive_without_the_caller_waiting() {
        // A null HWND posts the wake-up to the worker's own thread queue, where nothing reads it.
        let w = Worker::start(HWND::default());
        assert!(w.submit(Job::Theme));
        let deadline = Instant::now() + Duration::from_secs(5);
        let reply = loop {
            if let Some(r) = w.replies().next() {
                break r;
            }
            assert!(Instant::now() < deadline, "no reply from the worker");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(matches!(reply, Reply::Theme { .. }));
    }
}
