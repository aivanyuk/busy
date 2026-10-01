//! The widget's context menu: built from the config, it returns the chosen command for the router to apply.

use busy_core::{Anchor, Config, Module};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const ID_SETTINGS: u32 = 1;
const ID_EXIT: u32 = 2;
const ID_ANCHOR_TRAY: u32 = 3;
const ID_ANCHOR_LEFT: u32 = 4;
const ID_TOGGLE: u32 = 100;

pub enum Command {
    Settings,
    Exit,
    Anchor(Anchor),
    /// Show or hide the module's taskbar cell.
    Toggle(Module),
}

/// Shows the menu at the cursor, owned by `owner`, and returns the choice. `TrackPopupMenu` spins a modal
/// loop, so call it outside the app state borrow.
pub fn show(owner: HWND, cfg: &Config) -> Option<Command> {
    // SAFETY: the menus are created, used and destroyed here (the submenus with the menu that holds them);
    // the label buffers outlive the AppendMenuW calls that copy them.
    unsafe {
        let (Ok(menu), Ok(sub_mods), Ok(sub_pos)) = (CreatePopupMenu(), CreatePopupMenu(), CreatePopupMenu()) else {
            return None;
        };
        let checked = |b: bool| if b { MF_CHECKED } else { MF_UNCHECKED };
        for mc in cfg.modules.iter().filter(|mc| !mc.module.allowed_styles().is_empty()) {
            let idx = mc.module.index() as u32;
            let label = busy_win::wide(mc.module.label());
            let _ = AppendMenuW(
                sub_mods,
                MF_STRING | checked(mc.taskbar),
                (ID_TOGGLE + idx) as usize,
                PCWSTR(label.as_ptr()),
            );
        }
        let _ = AppendMenuW(
            sub_pos,
            MF_STRING | checked(cfg.anchor == Anchor::NearTray),
            ID_ANCHOR_TRAY as usize,
            w!("Next to notification area"),
        );
        // Down a vertical taskbar (design `isVert()`) the far end from the tray is its top.
        let left = if busy_win::taskbar_edge().is_vertical() { w!("Top") } else { w!("Left edge") };
        let _ = AppendMenuW(sub_pos, MF_STRING | checked(cfg.anchor == Anchor::Left), ID_ANCHOR_LEFT as usize, left);
        let _ = AppendMenuW(menu, MF_STRING, ID_SETTINGS as usize, w!("Settings…"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_POPUP, sub_mods.0 as usize, w!("Show on taskbar"));
        let _ = AppendMenuW(menu, MF_POPUP, sub_pos.0 as usize, w!("Position"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, ID_EXIT as usize, w!("Exit"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when clicking elsewhere.
        let _ = SetForegroundWindow(owner);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
            pt.x,
            pt.y,
            None,
            owner,
            None,
        );
        let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match cmd.0 as u32 {
            ID_SETTINGS => Some(Command::Settings),
            ID_EXIT => Some(Command::Exit),
            ID_ANCHOR_TRAY => Some(Command::Anchor(Anchor::NearTray)),
            ID_ANCHOR_LEFT => Some(Command::Anchor(Anchor::Left)),
            id => id.checked_sub(ID_TOGGLE).and_then(|i| Module::ALL.get(i as usize)).map(|&m| Command::Toggle(m)),
        }
    }
}
