//! The window following the host: configs it applied (`sync`) and new readings (`refresh`), which feed the
//! module page's live preview and the dropdowns that list the machine's drives, adapters and sensors.

use super::choices::Choices;
use super::clock;
use super::model::Page;
use super::{Ui, ui};
use busy_core::{Config, Snapshot};
use busy_ui::cell::{self, Key};
use busy_ui::ctx::Ctx;
use busy_ui::history::History;

/// What the preview last showed: its cell and the clock beside it.
#[derive(PartialEq)]
pub(super) struct Shown {
    cell: Option<Key>,
    clock: (String, String),
}

pub(crate) fn sync(cfg: &Config) {
    let Some(u) = ui() else { return };
    if *u.cfg.borrow() == *cfg {
        return;
    }
    let theme = u.cfg.borrow().theme != cfg.theme;
    *u.cfg.borrow_mut() = cfg.clone();
    if theme {
        u.apply_theme();
    }
    u.rebuild();
}

pub(crate) fn refresh(snap: &Snapshot, hist: &History) {
    let Some(u) = ui() else { return };
    u.update_choices(Choices::from_snapshot(snap));
    let Page::Module(m) = u.view.borrow().page else { return };
    let shown = {
        let (cfg, theme) = (u.cfg.borrow(), u.theme());
        let ctx = Ctx { cfg: &cfg, snap, hist, theme: &theme, gfx: &u.gfx };
        Shown { cell: cfg.module(m).and_then(|mc| cell::cell(&ctx, mc)).map(|c| c.key()), clock: clock::now() }
    };
    if u.shown.borrow().as_ref() != Some(&shown) {
        *u.shown.borrow_mut() = Some(shown);
        u.invalidate();
    }
}

impl Ui {
    /// Takes the machine's lists from new readings. A list that came back empty (its module isn't sampled
    /// right now) keeps the last one, and nothing changes under an open popup.
    fn update_choices(&self, new: Choices) {
        if self.view.borrow().popup.is_some() {
            return;
        }
        let mut next = self.choices.borrow().clone();
        if !new.volumes.is_empty() {
            next.volumes = new.volumes;
        }
        if !new.adapters.is_empty() {
            next.adapters = new.adapters;
        }
        if !new.sensors.is_empty() {
            next.sensors = new.sensors;
        }
        if next != *self.choices.borrow() {
            *self.choices.borrow_mut() = next;
            self.rebuild();
        }
    }
}
