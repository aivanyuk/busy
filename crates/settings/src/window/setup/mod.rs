//! Onboarding (design "FIRST RUN": pick readings, pick a side, done), the settings window's setup mode: one
//! fixed-size page with the reading cards, the two positions, Start with Windows, and Skip / Start monitoring.
//! Every choice applies live, so the taskbar behind the window follows it; Skip, Start monitoring and Close
//! all finish the same way, keeping what was chosen. This file is the pure part: what is shown, where, and
//! what is under the pointer; `input.rs` acts on it and `paint.rs` draws it.

use super::frame;
use super::layout::{Fonts, LINE_12, LINE_14, TITLE_H};
use busy_core::{Anchor, Config, Module};
use busy_ui::render::{Gfx, Rect};

pub(super) mod input;
mod paint;

pub(super) use paint::paint;

/// Client width (design: 680 wide, content 32 in from each side).
pub(super) const W: f32 = 680.0;
const PAD_X: f32 = 32.0;
const PAD_T: f32 = 12.0;
const PAD_B: f32 = 28.0;
/// Between the hero, the readings and the position; under a section heading; between cards.
const SECTION_GAP: f32 = 24.0;
const HEAD_GAP: f32 = 8.0;
const GAP: f32 = 6.0;
pub(super) const HEADLINE_H: f32 = 36.0;
const CARD_H: f32 = 52.0;
const COLS: usize = 3;
/// Radio card: padding 14 around a label and a description 2 apart.
const PLACE_H: f32 = 14.0 + LINE_14 + 2.0 + LINE_12 + 14.0;
/// Footer: a 1-DIP top line, padding 20 around 32-high buttons.
const FOOTER_H: f32 = 1.0 + 20.0 + 32.0 + 20.0;
pub(super) const BOX: f32 = 20.0;

pub(super) const HEADLINE: &str = "Your PC\u{2019}s vitals, right on the taskbar";
pub(super) const SUB: &str =
    "Pick what to show. Click any reading for details \u{2014} you can change all of this later in Settings.";
pub(super) const READINGS: &str = "Show on taskbar";
pub(super) const POSITION: &str = "Position";
pub(super) const STARTUP: &str = "Start with Windows";
pub(super) const SKIP: &str = "Skip";
pub(super) const START: &str = "Start monitoring";
/// The positions offered: anchor, label, description.
pub(super) const PLACES: [(Anchor, &str, &str); 2] = [
    (Anchor::NearTray, "Next to the system tray", "Right side, beside the clock"),
    (Anchor::Left, "Left edge of the taskbar", "Where Widgets usually sits"),
];

/// Something the pointer or the keyboard acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Close,
    /// A reading card, by its index in `cards`.
    Card(usize),
    /// A position, by its index in `PLACES`.
    Place(usize),
    Startup,
    Skip,
    Start,
}

/// The readings offered: every module with a taskbar cell, in taskbar order, and whether it is on.
pub(super) fn cards(cfg: &Config) -> Vec<(Module, bool)> {
    cfg.modules.iter().filter(|c| c.module != Module::Processes).map(|c| (c.module, c.taskbar)).collect()
}

/// The position chosen, as an index into `PLACES`.
pub(super) fn place(cfg: &Config) -> usize {
    PLACES.iter().position(|p| p.0 == cfg.anchor).unwrap_or(0)
}

/// Where everything sits, in client DIPs. Fixed once made: the window doesn't resize in setup.
pub(super) struct Layout {
    pub(super) headline: Rect,
    pub(super) sub: Rect,
    pub(super) readings: Rect,
    pub(super) cards: Vec<Rect>,
    pub(super) position: Rect,
    pub(super) places: [Rect; 2],
    pub(super) footer: Rect,
    /// The Start with Windows checkbox, and the checkbox with its label (what a click hits).
    pub(super) check: Rect,
    pub(super) startup: Rect,
    pub(super) skip: Rect,
    pub(super) start: Rect,
    /// Client height.
    pub(super) h: f32,
}

impl Layout {
    pub(super) fn new(gfx: &Gfx, f: &Fonts, cards: usize) -> Self {
        let cw = W - 2.0 * PAD_X;
        let x = PAD_X;
        let mut y = TITLE_H + PAD_T;
        let headline = Rect::new(x, y, cw, HEADLINE_H);
        y += HEADLINE_H + 6.0;
        let sub = Rect::new(x, y, cw, gfx.metrics(&f.body_wrap, SUB, cw).1.max(LINE_14));
        y = sub.bottom() + SECTION_GAP;
        let readings = Rect::new(x, y, cw, LINE_14);
        y += LINE_14 + HEAD_GAP;
        let col = (cw - GAP * (COLS - 1) as f32) / COLS as f32;
        let cell = |i: usize| {
            let (c, r) = ((i % COLS) as f32, (i / COLS) as f32);
            Rect::new(x + c * (col + GAP), y + r * (CARD_H + GAP), col, CARD_H)
        };
        let card_rects: Vec<Rect> = (0..cards).map(cell).collect();
        y = card_rects.last().map_or(y, Rect::bottom) + SECTION_GAP;
        let position = Rect::new(x, y, cw, LINE_14);
        y += LINE_14 + HEAD_GAP;
        let pw = (cw - GAP) / 2.0;
        let places = [Rect::new(x, y, pw, PLACE_H), Rect::new(x + pw + GAP, y, pw, PLACE_H)];
        y += PLACE_H + PAD_B;
        let footer = Rect::new(0.0, y, W, FOOTER_H);
        let by = y + 1.0 + 20.0;
        let button = |label: &str| gfx.text_width(&f.body, label) + 40.0;
        let start = Rect::new(W - 24.0 - button(START), by, button(START), 32.0);
        let skip = Rect::new(start.x - 8.0 - button(SKIP), by, button(SKIP), 32.0);
        let check = Rect::new(24.0, by + (32.0 - BOX) / 2.0, BOX, BOX);
        let startup = Rect::new(24.0, by, BOX + 10.0 + gfx.text_width(&f.body, STARTUP), 32.0);
        Layout {
            headline,
            sub,
            readings,
            cards: card_rects,
            position,
            places,
            footer,
            check,
            startup,
            skip,
            start,
            h: y + FOOTER_H,
        }
    }

    pub(super) fn hit(&self, x: f32, y: f32) -> Option<Target> {
        if y < TITLE_H {
            return frame::button_at(W, x, frame::CLOSE).map(|_| Target::Close);
        }
        if let Some(i) = self.cards.iter().position(|r| r.contains(x, y)) {
            return Some(Target::Card(i));
        }
        if let Some(i) = self.places.iter().position(|r| r.contains(x, y)) {
            return Some(Target::Place(i));
        }
        [(self.startup, Target::Startup), (self.skip, Target::Skip), (self.start, Target::Start)]
            .into_iter()
            .find_map(|(r, t)| r.contains(x, y).then_some(t))
    }

    /// What the focus ring surrounds.
    pub(super) fn rect(&self, t: Target) -> Option<Rect> {
        match t {
            Target::Close => Some(Rect::new(W - 46.0, 0.0, 46.0, TITLE_H)),
            Target::Card(i) => self.cards.get(i).copied(),
            Target::Place(i) => self.places.get(i).copied(),
            Target::Startup => Some(self.startup),
            Target::Skip => Some(self.skip),
            Target::Start => Some(self.start),
        }
    }
}

/// Tab order: every card, the chosen position (arrows move within the group), Start with Windows, Skip, Start.
pub(super) fn stops(cfg: &Config) -> Vec<Target> {
    let mut out: Vec<Target> = (0..cards(cfg).len()).map(Target::Card).collect();
    out.extend([Target::Place(place(cfg)), Target::Startup, Target::Skip, Target::Start]);
    out
}

/// Setup's own view state; the choices themselves are the config's.
#[derive(Default)]
pub(super) struct View {
    pub(super) layout: Option<Layout>,
    pub(super) hover: Option<Target>,
    pub(super) pressed: Option<Target>,
    pub(super) focus: Option<Target>,
    pub(super) focus_visible: bool,
    /// Each card's reading as last drawn (`busy_ui::cell::sample`), redrawn when one changes.
    pub(super) samples: Vec<Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use busy_core::CellStyle;

    #[test]
    fn cards_and_places_follow_the_config() {
        let mut cfg = Config::default();
        let c = cards(&cfg);
        assert_eq!(c.len(), Module::ALL.len() - 1);
        assert!(c.iter().all(|&(m, _)| m != Module::Processes));
        assert_eq!(c[0], (Module::Cpu, true));
        assert_eq!(place(&cfg), 0);
        cfg.anchor = Anchor::Left;
        cfg.modules.iter_mut().for_each(|m| m.style = CellStyle::Text);
        assert_eq!(place(&cfg), 1);
        let s = stops(&cfg);
        assert_eq!(s[c.len()..], [Target::Place(1), Target::Startup, Target::Skip, Target::Start]);
    }
}
