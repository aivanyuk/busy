//! Onboarding (design "FIRST RUN": pick readings, pick a side, done), the settings window's setup mode: one
//! fixed-size page with the reading cards, the two positions, Start with Windows, and Skip / Start monitoring.
//! Every choice applies live, so the taskbar behind the window follows it; Skip, Start monitoring and Close
//! all finish the same way, keeping what was chosen. This file is the pure part: what is shown, where, and
//! what is under the pointer; `input.rs` acts on it and `paint.rs` draws it.

use super::frame;
use super::layout::{Fonts, LINE_12, LINE_14, TITLE_H};
use busy_core::{Anchor, Config, Module};
use busy_ui::i18n::t;
use busy_ui::render::{Gfx, Rect};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

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
/// Radio card: padding 14 around a label and a description 2 apart, one line each; the text starts right of
/// the radio, 12 from it.
const PLACE_H: f32 = 14.0 + LINE_14 + 2.0 + LINE_12 + 14.0;
pub(super) const PLACE_TEXT_X: f32 = 14.0 + BOX + 12.0;
/// Footer: a 1-DIP top line, padding 20 around 32-high buttons.
const FOOTER_H: f32 = 1.0 + 20.0 + 32.0 + 20.0;
pub(super) const BOX: f32 = 20.0;

/// The positions offered, in order.
pub(super) const ANCHORS: [Anchor; 2] = [Anchor::NearTray, Anchor::Left];

/// The positions as a taskbar standing `vertical` or not words them (design `obPos`: down a vertical taskbar
/// the far end from the tray is its top): anchor, label, description.
pub(super) fn places(vertical: bool) -> [(Anchor, &'static str, &'static str); 2] {
    let s = &t().setup;
    let [tray, left] = ANCHORS;
    if vertical {
        [(tray, s.near_tray, s.near_tray_desc_vertical), (left, s.top, s.top_desc)]
    } else {
        [(tray, s.near_tray, s.near_tray_desc), (left, s.left_edge, s.left_edge_desc)]
    }
}

/// Something the pointer or the keyboard acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Close,
    /// A reading card, by its index in `cards`.
    Card(usize),
    /// A position, by its index in `ANCHORS`.
    Place(usize),
    Startup,
    Skip,
    Start,
}

/// The readings offered: every module with a taskbar cell, in taskbar order, and whether it is on.
pub(super) fn cards(cfg: &Config) -> Vec<(Module, bool)> {
    cfg.modules.iter().filter(|c| c.module != Module::Processes).map(|c| (c.module, c.taskbar)).collect()
}

/// The position chosen, as an index into `ANCHORS`.
pub(super) fn place(cfg: &Config) -> usize {
    ANCHORS.iter().position(|&a| a == cfg.anchor).unwrap_or(0)
}

/// The height `s` takes in `wrap` at width `w`: `line`, the design's one line, unless it wraps to more lines,
/// then all of them.
fn text_h(gfx: &Gfx, wrap: &IDWriteTextFormat, s: &str, w: f32, line: f32) -> f32 {
    let (one, all) = (gfx.metrics(wrap, s, 10_000.0).1, gfx.metrics(wrap, s, w).1);
    if all > one { all.max(line) } else { line }
}

/// Where everything sits, in client DIPs. Fixed once made: the window doesn't resize in setup.
pub(super) struct Layout {
    pub(super) headline: Rect,
    pub(super) sub: Rect,
    pub(super) readings: Rect,
    pub(super) cards: Vec<Rect>,
    pub(super) position: Rect,
    pub(super) places: [Rect; 2],
    /// The heights of the radio cards' label and description, on a horizontal (0) and a vertical taskbar (1):
    /// `LINE_14` and `LINE_12`, more when one wraps.
    pub(super) place_text: [(f32, f32); 2],
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
        let (s, cw, x) = (&t().setup, W - 2.0 * PAD_X, PAD_X);
        let mut y = TITLE_H + PAD_T;
        // The design's one-line headline, or the lines a longer translation wraps to.
        let headline = Rect::new(x, y, cw, text_h(gfx, &f.title_wrap, s.headline, cw, HEADLINE_H));
        y = headline.bottom() + 6.0;
        let sub = Rect::new(x, y, cw, gfx.metrics(&f.body_wrap, s.sub, cw).1.max(LINE_14));
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
        // A radio card's label and description each take a line in the design, more when a translation wraps:
        // for each way the taskbar may stand, the most either card's takes. Both cards are as tall, and as tall
        // as the taller wording needs, so the taskbar turning doesn't resize the window.
        let tw = pw - PLACE_TEXT_X - 14.0;
        let place_text = [false, true].map(|vertical| {
            places(vertical).iter().fold((LINE_14, LINE_12), |(l, d), p| {
                (text_h(gfx, &f.body_wrap, p.1, tw, LINE_14).max(l), text_h(gfx, &f.desc, p.2, tw, LINE_12).max(d))
            })
        });
        let place_h = place_text.iter().map(|(l, d)| PLACE_H - LINE_14 - LINE_12 + l + d).fold(PLACE_H, f32::max);
        let places = [Rect::new(x, y, pw, place_h), Rect::new(x + pw + GAP, y, pw, place_h)];
        y += place_h + PAD_B;
        let footer = Rect::new(0.0, y, W, FOOTER_H);
        let by = y + 1.0 + 20.0;
        let button = |label: &str| gfx.text_width(&f.body, label) + 40.0;
        let start = Rect::new(W - 24.0 - button(s.start), by, button(s.start), 32.0);
        let skip = Rect::new(start.x - 8.0 - button(s.skip), by, button(s.skip), 32.0);
        let check = Rect::new(24.0, by + (32.0 - BOX) / 2.0, BOX, BOX);
        let startup = Rect::new(24.0, by, BOX + 10.0 + gfx.text_width(&f.body, s.startup), 32.0);
        Layout {
            headline,
            sub,
            readings,
            cards: card_rects,
            position,
            places,
            place_text,
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
        // Only the words follow the taskbar's orientation.
        assert!(places(true).iter().zip(places(false)).all(|(v, h)| v.0 == h.0));
        assert_eq!(places(true)[1].1, "Top of the taskbar");
    }
}
