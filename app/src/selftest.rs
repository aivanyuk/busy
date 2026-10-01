//! Debug builds: `BUSY_SELFTEST=<dir>` writes what the widget and the flyout found in explorer's taskbar and
//! where they are to `<dir>\selftest.json` after every watch tick, for `tools/vm` (docs/testing.md § Windows
//! versions). Written on a worker thread, through a temp file renamed over the report, so a reader never sees
//! half a file.

use crate::worker::Worker;
use busy_core::Module;
use std::fmt::Write;
use std::path::PathBuf;
use windows::Win32::Foundation::{HWND, RECT};

/// The widget and the taskbar it is embedded in (`Taskbar::selftest`).
pub struct TaskbarInfo {
    pub tray: HWND,
    pub tray_rect: RECT,
    pub client: RECT,
    pub dpi: u32,
    /// On the left or right edge: cells stack along y.
    pub vertical: bool,
    pub landmarks: Vec<(&'static str, Option<RECT>)>,
    pub widget: HWND,
    pub parent: HWND,
    pub visible: bool,
    /// A sibling is above the widget in z-order.
    pub covered: bool,
    pub rect: RECT,
    /// `explorer::slot` as of the last layout: (anchor edge, room), taskbar client pixels.
    pub slot: (i32, i32),
    /// Each drawn cell's extent along the taskbar in widget client pixels (y when vertical), as hit-testing sees
    /// it.
    pub cells: Vec<(i32, i32, Module)>,
}

/// The flyout (`Flyout::selftest`).
pub struct FlyoutInfo {
    pub hwnd: HWND,
    pub visible: bool,
    pub module: Option<Module>,
    pub rect: RECT,
    pub backdrop: bool,
    pub work: RECT,
}

pub struct Selftest {
    writer: Worker<String>,
    seq: u64,
}

impl Selftest {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("BUSY_SELFTEST")?);
        let writer = Worker::start("busy-selftest", move |json: String| {
            let tmp = dir.join("selftest.json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, dir.join("selftest.json"));
            }
        });
        Some(Self { writer, seq: 0 })
    }

    pub fn report(&mut self, taskbar: Option<TaskbarInfo>, flyout: Option<FlyoutInfo>, configured: usize) {
        self.seq += 1;
        let mut j = format!("{{\"seq\":{},\"pid\":{},\"configured\":{configured}", self.seq, std::process::id());
        j.push_str(",\"taskbar\":");
        match taskbar {
            Some(t) => {
                let _ = write!(
                    j,
                    "{{\"hwnd\":{},\"rect\":{},\"client\":{},\"dpi\":{},\"vertical\":{},\"landmarks\":{{",
                    hwnd(t.tray),
                    rect(t.tray_rect),
                    rect(t.client),
                    t.dpi,
                    t.vertical
                );
                for (i, (name, r)) in t.landmarks.iter().enumerate() {
                    let _ = write!(j, "{}\"{name}\":{}", if i > 0 { "," } else { "" }, r.map_or("null".into(), rect));
                }
                let _ = write!(
                    j,
                    "}}}},\"widget\":{{\"hwnd\":{},\"parent\":{},\"visible\":{},\"covered\":{},\"rect\":{},\
                     \"slot\":[{},{}],\"cells\":[",
                    hwnd(t.widget),
                    hwnd(t.parent),
                    t.visible,
                    t.covered,
                    rect(t.rect),
                    t.slot.0,
                    t.slot.1
                );
                for (i, (l, r, m)) in t.cells.iter().enumerate() {
                    let sep = if i > 0 { "," } else { "" };
                    let _ = write!(j, "{sep}{{\"module\":\"{m:?}\",\"left\":{l},\"right\":{r}}}");
                }
                j.push_str("]}");
            }
            None => j.push_str("null,\"widget\":null"),
        }
        j.push_str(",\"flyout\":");
        match flyout {
            Some(f) => {
                let module = f.module.map_or("null".into(), |m| format!("\"{m:?}\""));
                let _ = write!(
                    j,
                    "{{\"hwnd\":{},\"visible\":{},\"module\":{module},\"rect\":{},\"backdrop\":{},\"work\":{}}}",
                    hwnd(f.hwnd),
                    f.visible,
                    rect(f.rect),
                    f.backdrop,
                    rect(f.work)
                );
            }
            None => j.push_str("null"),
        }
        j.push('}');
        self.writer.submit(j);
    }
}

fn hwnd(h: HWND) -> isize {
    h.0 as isize
}

fn rect(r: RECT) -> String {
    format!("[{},{},{},{}]", r.left, r.top, r.right, r.bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_json_with_every_field() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut s = Selftest { writer: Worker::start("test-selftest", move |j: String| tx.send(j).unwrap()), seq: 0 };
        let r = RECT { left: 0, top: 1040, right: 1920, bottom: 1080 };
        let tb = TaskbarInfo {
            tray: HWND(1 as _),
            tray_rect: r,
            client: r,
            dpi: 96,
            vertical: false,
            landmarks: vec![("TrayNotifyWnd", Some(r)), ("Start", None)],
            widget: HWND(2 as _),
            parent: HWND(1 as _),
            visible: true,
            covered: false,
            rect: r,
            slot: (1500, 600),
            cells: vec![(-1, 40, Module::Cpu), (40, 80, Module::Memory)],
        };
        let fly = FlyoutInfo { hwnd: HWND(3 as _), visible: false, module: None, rect: r, backdrop: true, work: r };
        // One at a time: the writer skips a report superseded before its turn.
        s.report(Some(tb), Some(fly), 4);
        let first: serde_json::Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        assert_eq!(first["taskbar"]["landmarks"]["Start"], serde_json::Value::Null);
        assert_eq!(first["widget"]["cells"][1]["module"], "Memory");
        assert_eq!(first["widget"]["cells"][0]["left"], -1);
        assert_eq!(first["flyout"]["module"], serde_json::Value::Null);
        assert_eq!((first["configured"].as_u64(), first["taskbar"]["vertical"].as_bool()), (Some(4), Some(false)));
        s.report(None, None, 0);
        let second: serde_json::Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        assert_eq!((second["seq"].as_u64(), second["widget"].is_null()), (Some(2), true));
    }
}
