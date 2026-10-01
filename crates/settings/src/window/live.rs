//! The window following the host: configs it applied (`sync`), new readings (`refresh`), which feed the
//! module page's live preview and the dropdowns that list the machine's drives, adapters and sensors, and
//! the update check's answer (`release`); and following the taskbar to another screen edge (`follow_edge`).

use super::choices::Choices;
use super::clock;
use super::controls::preview;
use super::model::Page;
use super::{Ui, ui};
use busy_core::release::Release;
use busy_core::{Config, Snapshot};
use std::cell::RefCell;

thread_local! {
    /// The update check's last answer, kept while the window is closed for the next one to open with.
    static NEWER: RefCell<Option<Release>> = const { RefCell::new(None) };
}

pub(super) fn newer() -> Option<Release> {
    NEWER.with_borrow(Clone::clone)
}

pub(crate) fn release(newer: Option<&Release>) {
    NEWER.set(newer.cloned());
    let Some(u) = ui() else { return };
    if u.choices.borrow().newer.as_ref() == newer {
        return;
    }
    u.choices.borrow_mut().newer = newer.cloned();
    // Under an open popup the rows stay as they are; the next rebuild shows it.
    if u.view.borrow().popup.is_none() {
        u.rebuild();
    }
}
use busy_ui::cell::{self, Key};
use busy_ui::ctx::Ctx;
use busy_ui::history::History;
use busy_win::Edge;

/// What the preview last showed: its cell, the clock beside it, and the taskbar edge it was drawn for (a strip,
/// or a column on the left or right).
#[derive(PartialEq)]
pub(super) struct Shown {
    cell: Option<Key>,
    clock: (String, String),
    edge: Edge,
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
    u.follow_edge();
    u.update_choices(Choices::from_snapshot(snap));
    if u.in_setup() {
        return u.setup_samples(snap);
    }
    let Page::Module(m) = u.view.borrow().page else { return };
    let shown = {
        let (cfg, theme, edge) = (u.cfg.borrow(), u.theme(), u.choices.borrow().edge);
        let ctx = Ctx { cfg: &cfg, snap, hist, theme: &theme, gfx: &u.gfx };
        let cell = cfg.module(m).and_then(|mc| cell::cell(&ctx, mc)).map(|c| c.key());
        Shown { cell, clock: if edge.is_vertical() { clock::now_short() } else { clock::now() }, edge }
    };
    if u.shown.borrow().as_ref() != Some(&shown) {
        *u.shown.borrow_mut() = Some(shown);
        u.invalidate();
    }
}

impl Ui {
    /// Follows the taskbar to another screen edge (Windows 26H2 lets it stand on the left or right): "left"
    /// becomes "top" in the words, and the preview turns into a column. `taskbar_edge` only reads window
    /// rects, so this runs before every frame and with every refresh: a change shows by the next paint or
    /// sample, whichever comes first. An open popup closes, as its options' words and place change.
    pub(super) fn follow_edge(&self) {
        let edge = busy_win::taskbar_edge();
        if self.choices.borrow().edge == edge {
            return;
        }
        self.choices.borrow_mut().edge = edge;
        self.view.borrow_mut().popup = None;
        self.rebuild();
    }

    /// Down a vertical taskbar the preview card is as tall as its cell's column, which the style, the label and
    /// the readings decide: measured from the readings lent for a frame, and laid out again before the frame is
    /// drawn when it changed.
    pub(super) fn fit_preview(&self, d: &preview::Data) {
        let edge = self.choices.borrow().edge;
        if self.in_setup() || !edge.is_vertical() {
            return;
        }
        let Page::Module(m) = self.view.borrow().page else { return };
        let h = preview::column_height(d, m, &self.cfg.borrow(), &self.theme(), &self.gfx);
        if self.cell_h.replace(h) != h {
            let mut v = self.view.borrow_mut();
            v.preview_h = preview::height(edge, h);
            v.lay_out(&self.gfx, &self.fonts);
        }
    }

    /// Setup's cards: each module's reading, redrawn when one changes.
    pub(super) fn setup_samples(&self, snap: &Snapshot) {
        let samples: Vec<_> = {
            let cfg = self.cfg.borrow();
            super::setup::cards(&cfg).into_iter().map(|(m, _)| cell::sample(snap, &cfg, m)).collect()
        };
        let mut v = self.setup.borrow_mut();
        if v.samples != samples {
            v.samples = samples;
            drop(v);
            self.invalidate();
        }
    }

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
