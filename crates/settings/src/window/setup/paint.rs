//! Drawing setup (design onboarding): title bar with Close, the hero text, the reading cards, the two
//! position cards and the footer with Start with Windows, Skip and Start monitoring.

use super::{BOX, HEADLINE, Layout, POSITION, READINGS, SKIP, START, STARTUP, SUB, Target, View, W};
use crate::window::controls::button;
use crate::window::frame;
use crate::window::layout::{self, LINE_12, LINE_14};
use crate::window::paint::State;
use busy_ui::render::{Align, Canvas, Rect};
use busy_ui::theme::{Theme, alpha};
use busy_ui::tone::Tone;

pub(in crate::window) fn paint(cv: &Canvas, v: &View, s: &State) {
    let (t, f) = (s.theme, s.fonts);
    // SAFETY: the render target is a live COM object inside BeginDraw/EndDraw.
    unsafe { cv.rt.Clear(Some(&t.win)) };
    let hover = v.hover.and_then(|h| (h == Target::Close).then_some(layout::Target::Close));
    let bar = frame::Bar { title: "busy", buttons: frame::CLOSE, hover, maximized: false };
    frame::draw(cv, W, &bar, t, f);
    let Some(l) = &v.layout else { return };
    cv.text(HEADLINE, &f.title, l.headline, t.fg, Align::Left);
    cv.text(SUB, &f.body_wrap, l.sub, t.fg2, Align::Left);
    cv.text(READINGS, &f.strong, l.readings, t.fg, Align::Left);
    for (i, ((m, on), r)) in super::cards(s.cfg).into_iter().zip(&l.cards).enumerate() {
        let hot = v.hover == Some(Target::Card(i));
        cv.round(*r, 6.0, if hot { t.active } else { t.card });
        cv.round_outline(*r, 6.0, if on { t.accent } else { t.card_line });
        let bx = Rect::new(r.x + 14.0, r.y + (r.h - BOX) / 2.0, BOX, BOX);
        check(cv, bx, on, true, t);
        let name_x = bx.right() + 12.0;
        cv.text(
            busy_ui::i18n::t().common.module(m),
            &f.body,
            Rect::new(name_x, r.y, r.right() - 14.0 - name_x, r.h),
            t.fg,
            Align::Left,
        );
        let sample = v.samples.get(i).cloned().flatten().unwrap_or_default();
        let color = t.color(Tone::Pal(s.cfg.module(m).map_or(m.default_color(), busy_core::ModuleCfg::color_index)));
        cv.text(&sample, &f.small_strong, Rect::new(name_x, r.y, r.right() - 14.0 - name_x, r.h), color, Align::Right);
    }
    cv.text(POSITION, &f.strong, l.position, t.fg, Align::Left);
    let chosen = super::place(s.cfg);
    for (i, ((_, label, desc), r)) in super::places(s.edge.is_vertical()).iter().zip(&l.places).enumerate() {
        let hot = v.hover == Some(Target::Place(i));
        cv.round(*r, 6.0, if hot { t.active } else { t.card });
        cv.round_outline(*r, 6.0, t.card_line);
        radio(cv, Rect::new(r.x + 14.0, r.y + 14.0, BOX, BOX), i == chosen, t);
        let tx = r.x + 14.0 + BOX + 12.0;
        let tw = r.right() - 14.0 - tx;
        cv.text(label, &f.body, Rect::new(tx, r.y + 14.0, tw, LINE_14), t.fg, Align::Left);
        cv.text(desc, &f.small, Rect::new(tx, r.y + 14.0 + LINE_14 + 2.0, tw, LINE_12), t.fg2, Align::Left);
    }
    footer(cv, l, v, s);
    // Keyboard focus (WinUI's focus visual): a 2-DIP `--fg` ring 3 outside the target.
    if v.focus_visible
        && let Some(r) = v.focus.and_then(|f| l.rect(f))
    {
        let radius = if matches!(v.focus, Some(Target::Card(_) | Target::Place(_))) { 9.0 } else { 7.0 };
        cv.round_stroke(r.inset(-3.0, -3.0), radius, 2.0, t.fg);
    }
}

fn footer(cv: &Canvas, l: &Layout, v: &View, s: &State) {
    let (t, f) = (s.theme, s.fonts);
    cv.fill(l.footer, t.footer);
    cv.fill(Rect::new(0.0, l.footer.y, W, 1.0), t.line);
    // Waits (drawn disabled) while the registry is read or written, like the Settings toggle.
    let busy = s.autostart_busy;
    check(cv, l.check, s.cfg.autostart, !busy, t);
    let label = Rect::new(l.check.right() + 10.0, l.startup.y, l.startup.right() - l.check.right() - 10.0, l.startup.h);
    cv.text(STARTUP, &f.body, label, if busy { t.fg3 } else { t.fg }, Align::Left);
    button::draw(cv, l.skip, SKIP, v.hover == Some(Target::Skip), t, f);
    let start = if v.hover == Some(Target::Start) { alpha(t.accent, 0.9) } else { t.accent };
    cv.round(l.start, 4.0, start);
    cv.text(START, &f.body, l.start, t.on_accent, Align::Center);
}

/// A 20-DIP checkbox (radius 4): checked is `--accent` with an `--on-accent` check mark, unchecked a
/// `--ctl-strong` outline; disabled, `--fg3` instead of either.
fn check(cv: &Canvas, r: Rect, on: bool, enabled: bool, t: &Theme) {
    if on {
        cv.round(r, 4.0, if enabled { t.accent } else { t.fg3 });
        let (x, y) = (r.x, r.y);
        cv.line((x + 5.5, y + 10.5), (x + 8.5, y + 13.5), 1.5, t.on_accent);
        cv.line((x + 8.5, y + 13.5), (x + 14.5, y + 7.0), 1.5, t.on_accent);
    } else {
        cv.round_outline(r.inset(0.5, 0.5), 3.5, if enabled { t.ctl_strong } else { t.fg3 });
    }
}

/// A 20-DIP radio: chosen is an `--accent` ring 5 wide around an `--on-accent` dot, else a `--ctl-strong` outline.
fn radio(cv: &Canvas, r: Rect, on: bool, t: &Theme) {
    if on {
        cv.round(r, BOX / 2.0, t.accent);
        cv.round(r.inset(5.0, 5.0), BOX / 2.0 - 5.0, t.on_accent);
    } else {
        cv.round_outline(r.inset(0.5, 0.5), BOX / 2.0 - 0.5, t.ctl_strong);
    }
}
