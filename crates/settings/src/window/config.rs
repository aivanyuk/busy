//! Filling the controls from a `Config`, collecting them back, and Apply.

use super::controls::{checked, combo_fill, combo_set, combo_value, send, set_check, set_text, window_string};
use super::worker::{Job, Reply};
use super::{Ui, show};
use busy_core::{Anchor, CellStyle, Config, TempUnit, ThemeMode};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PWSTR, w};

const OFFSET_RANGE: i32 = 2000;
pub(super) const STYLES: [(CellStyle, &str); 3] =
    [(CellStyle::Text, "Text"), (CellStyle::Graph, "Graph"), (CellStyle::Bar, "Bar")];

impl Ui {
    pub(super) fn init(&self, cfg: &Config) {
        send(self.ctl.list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize);
        for (i, name) in ["Module", "Taskbar", "Flyout", "Style"].into_iter().enumerate() {
            let text = HSTRING::from(name);
            let col = LVCOLUMNW {
                mask: LVCF_TEXT | LVCF_FMT,
                fmt: if i == 1 || i == 2 { LVCFMT_CENTER } else { LVCFMT_LEFT },
                pszText: PWSTR(text.as_ptr() as _),
                ..Default::default()
            };
            send(self.ctl.list, LVM_INSERTCOLUMNW, i, &col as *const _ as isize);
        }
        for i in 0..cfg.modules.len() {
            let item = LVITEMW {
                mask: LVIF_TEXT,
                iItem: i as i32,
                pszText: PWSTR(w!("").as_ptr() as _),
                ..Default::default()
            };
            send(self.ctl.list, LVM_INSERTITEMW, 0, &item as *const _ as isize);
            self.fill_row(i);
        }

        combo_fill(self.ctl.style, STYLES.iter().enumerate().map(|(i, (_, s))| (*s, i as isize)));
        combo_fill(self.ctl.anchor, [("Near tray", 0), ("Left edge", 1)]);
        combo_set(self.ctl.anchor, (cfg.anchor == Anchor::Left) as isize, String::new);
        combo_fill(
            self.ctl.interval,
            [500, 1000, 2000, 5000].map(|v| (fmt_ms(v), v as isize)).iter().map(|(s, v)| (s.as_str(), *v)),
        );
        combo_set(self.ctl.interval, cfg.interval_ms as isize, || fmt_ms(cfg.interval_ms));
        combo_fill(
            self.ctl.history,
            [60, 120, 300, 600].map(|v| (fmt_secs(v), v as isize)).iter().map(|(s, v)| (s.as_str(), *v)),
        );
        combo_set(self.ctl.history, cfg.history_secs as isize, || fmt_secs(cfg.history_secs));
        combo_fill(self.ctl.theme, [("System", 0), ("Light", 1), ("Dark", 2)]);
        combo_set(self.ctl.theme, cfg.theme as isize, String::new);
        combo_fill(self.ctl.unit, [("Celsius (\u{b0}C)", 0), ("Fahrenheit (\u{b0}F)", 1)]);
        combo_set(self.ctl.unit, (cfg.temp_unit == TempUnit::Fahrenheit) as isize, String::new);

        send(self.ctl.spin, UDM_SETBUDDY, self.ctl.offset.0 as usize, 0);
        send(self.ctl.spin, UDM_SETRANGE32, -OFFSET_RANGE as usize, OFFSET_RANGE as isize);
        send(self.ctl.spin, UDM_SETPOS32, 0, cfg.offset_px.clamp(-OFFSET_RANGE, OFFSET_RANGE) as isize);
        set_text(self.ctl.offset, &cfg.offset_px.to_string());
        set_text(self.ctl.sensor, &cfg.pinned_sensor);
        let cue = HSTRING::from("hardware/name \u{2014} empty = hottest CPU");
        send(self.ctl.sensor, EM_SETCUEBANNER, 0, cue.as_ptr() as isize);
        // Shows the saved setting until the worker has read the registry.
        set_check(self.ctl.autostart, cfg.autostart);
        self.enable(self.ctl.autostart, false);

        self.select(0);
        self.sync_editors();
        self.changed();
    }

    pub(super) fn collect(&self) -> Config {
        let mut c = self.applied.borrow().clone();
        c.modules = self.modules.borrow().clone();
        c.anchor = if combo_value(self.ctl.anchor) == Some(1) { Anchor::Left } else { Anchor::NearTray };
        if let Ok(v) = window_string(self.ctl.offset).trim().parse::<i32>() {
            c.offset_px = v.clamp(-OFFSET_RANGE, OFFSET_RANGE);
        }
        c.interval_ms = combo_value(self.ctl.interval).map_or(c.interval_ms, |v| v as u32);
        c.history_secs = combo_value(self.ctl.history).map_or(c.history_secs, |v| v as u32);
        c.theme = match combo_value(self.ctl.theme) {
            Some(1) => ThemeMode::Light,
            Some(2) => ThemeMode::Dark,
            _ => ThemeMode::System,
        };
        c.temp_unit = if combo_value(self.ctl.unit) == Some(1) { TempUnit::Fahrenheit } else { TempUnit::Celsius };
        c.pinned_sensor = window_string(self.ctl.sensor).trim().to_string();
        c.autostart = checked(self.ctl.autostart);
        c
    }

    pub(super) fn changed(&self) {
        let dirty = self.collect() != *self.applied.borrow();
        self.enable(self.ctl.apply, dirty);
    }

    /// Applies the edited config; with `close`, then closes the window. An autostart change is written by the
    /// worker first, and the apply finishes in `on_reply`.
    pub(super) fn apply(&self, close: bool) {
        if let Some((_, closing)) = self.pending.borrow_mut().as_mut() {
            *closing |= close;
            return;
        }
        let mut c = self.collect();
        if let Some(reg) = self.reg_autostart.get()
            && reg != c.autostart
        {
            if self.worker.submit(Job::SetAutostart(c.autostart)) {
                *self.pending.borrow_mut() = Some((c, close));
                for h in [self.ctl.autostart, self.ctl.ok, self.ctl.apply] {
                    self.enable(h, false);
                }
                return;
            }
            c.autostart = reg;
            set_check(self.ctl.autostart, reg);
        }
        self.finish(c, close);
    }

    /// Closes the window, or once the pending apply is done.
    pub(super) fn close(&self) {
        if let Some((_, close)) = self.pending.borrow_mut().as_mut() {
            *close = true;
            return;
        }
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }

    fn finish(&self, c: Config, close: bool) {
        let changed = c != *self.applied.borrow();
        self.syncing.set(true);
        if window_string(self.ctl.offset).trim() != c.offset_px.to_string() {
            set_text(self.ctl.offset, &c.offset_px.to_string());
        }
        if window_string(self.ctl.sensor) != c.pinned_sensor {
            set_text(self.ctl.sensor, &c.pinned_sensor);
        }
        self.syncing.set(false);
        *self.applied.borrow_mut() = c.clone();
        self.changed();
        self.update_dark();
        if changed {
            (self.on_apply)(c);
        }
        if close {
            self.close();
        }
    }

    pub(super) fn on_reply(&self, r: Reply) {
        match r {
            Reply::Read { autostart, system_dark } => {
                self.system_dark.set(system_dark);
                self.update_dark();
                self.reg_autostart.set(Some(autostart));
                set_check(self.ctl.autostart, autostart);
                self.enable(self.ctl.autostart, true);
                self.changed();
                // SAFETY: `self.hwnd` is this window, alive while its procedure runs.
                if !unsafe { IsWindowVisible(self.hwnd) }.as_bool() {
                    show(self.hwnd);
                }
            }
            Reply::Theme { system_dark } => {
                self.system_dark.set(system_dark);
                self.update_dark();
            }
            Reply::SetAutostart { error, autostart } => {
                self.reg_autostart.set(Some(autostart));
                let Some((mut c, close)) = self.pending.take() else { return };
                if let Some(e) = error {
                    let msg = HSTRING::from(format!(
                        "Couldn't update the startup entry.

{e}"
                    ));
                    unsafe { MessageBoxW(Some(self.hwnd), &msg, w!("busy"), MB_ICONERROR | MB_OK) };
                }
                // Reflects the registry as read back, not the request.
                c.autostart = autostart;
                set_check(self.ctl.autostart, autostart);
                for h in [self.ctl.autostart, self.ctl.ok] {
                    self.enable(h, true);
                }
                self.finish(c, close);
            }
        }
    }
}

fn fmt_ms(ms: u32) -> String {
    format!("{} s", ms as f64 / 1000.0)
}

fn fmt_secs(secs: u32) -> String {
    if secs.is_multiple_of(60) { format!("{} min", secs / 60) } else { format!("{secs} s") }
}
