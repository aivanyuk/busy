//! Drawing the whole window from its `View`: title bar, nav, the page's cards, then the open popup on top.

use super::controls::{dropdown, nav, order, preview, segmented, swatch, toggle};
use super::frame;
use super::layout::{CARD_PAD_L, CARD_PAD_Y, Fonts, LINE_14, PAGE_SUB_H, PAGE_TITLE_H, Target, View};
use super::model::{self, Control, Flag, Item, Page};
use busy_core::{Config, Module, ModuleCfg};
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::Theme;

/// What painting needs besides the view.
pub(super) struct State<'a> {
    pub(super) cfg: &'a Config,
    pub(super) theme: &'a Theme,
    pub(super) fonts: &'a Fonts,
    pub(super) gfx: &'a Gfx,
    pub(super) cell_fonts: &'a busy_ui::cell::Fonts,
    /// The host's readings, for the preview; `None` when it couldn't lend them for this frame.
    pub(super) data: Option<preview::Data<'a>>,
    pub(super) maximized: bool,
    /// The autostart toggle waits for the registry (first read or a write in flight).
    pub(super) autostart_busy: bool,
}

pub(super) fn paint(cv: &Canvas, v: &View, s: &State) {
    let (t, f) = (s.theme, s.fonts);
    // SAFETY: the render target is a live COM object inside BeginDraw/EndDraw.
    unsafe { cv.rt.Clear(Some(&t.win)) };
    frame::draw(cv, v.w, v.hover, s.maximized, t, f);
    nav::header(cv, &model::readings(s.cfg), t, f);
    nav::search(cv, v.search_rect(), t, f);
    // `Theme::color` clamps the index: a host's config need not be normalized.
    let dot = |m: Module| {
        t.color(busy_ui::tone::Tone::Pal(s.cfg.module(m).map_or(m.default_color(), ModuleCfg::color_index)))
    };
    for (i, &p) in v.nav.iter().enumerate() {
        let (dot, status) = match p {
            Page::General => (t.fg3, ""),
            Page::Module(m) => (dot(m), if model::is_on(s.cfg, m) { "On" } else { "Off" }),
        };
        let item =
            nav::Item { label: p.title(), dot, status, selected: p == v.page, hover: v.hover == Some(Target::Nav(i)) };
        nav::item(cv, v.nav_rect(i), &item, t, f);
    }

    let pane = v.pane();
    cv.clip(pane);
    let (x, y) = v.origin();
    let w = pane.w - (x - pane.x) - 32.0;
    cv.text(v.page.title(), &f.title, Rect::new(x, y, w, PAGE_TITLE_H), t.fg, Align::Left);
    cv.text(v.page.sub(), &f.sub, Rect::new(x, y + PAGE_TITLE_H + 4.0, w, PAGE_SUB_H), t.fg2, Align::Left);
    for (i, (item, p)) in v.items.iter().zip(&v.placed).enumerate() {
        let r = v.to_window(p.rect);
        if r.bottom() < pane.y || r.y > pane.bottom() {
            continue;
        }
        match item {
            Item::Header(title) => cv.text(title, &f.strong, Rect::new(r.x, r.y, r.w, LINE_14), t.fg, Align::Left),
            Item::Order(list) => order::draw(cv, r, list, dot, v.hover, t, f),
            Item::Preview(m) => preview::draw(cv, r, *m, s.cfg, s.data.as_ref(), (s.gfx, s.cell_fonts), t, f),
            Item::Row(row) => {
                cv.round(r, 4.0, t.card);
                cv.round_outline(r, 4.0, t.card_line);
                let tx = r.x + CARD_PAD_L;
                cv.text(row.title, &f.body, Rect::new(tx, r.y + CARD_PAD_Y, p.text_w, LINE_14), t.fg, Align::Left);
                let desc = Rect::new(tx, r.y + CARD_PAD_Y + LINE_14 + 2.0, p.text_w, r.h);
                cv.text(&row.desc, &f.desc, desc, t.fg2, Align::Left);
                let ctl = v.to_window(p.ctl);
                let hover = v.hover == Some(Target::Ctl(i));
                match &row.control {
                    Control::Toggle(flag, on) => {
                        let enabled = !(*flag == Flag::Autostart && s.autostart_busy);
                        toggle::draw(cv, ctl, *on, enabled, t, f);
                    }
                    Control::Dropdown(opts, sel) => {
                        let label = opts.get(*sel).map_or("", |o| o.label.as_str());
                        dropdown::draw(cv, ctl, label, hover, t, f);
                    }
                    Control::Segmented(opts, sel) => segmented::draw(cv, s.gfx, ctl, opts, *sel, t, f),
                    Control::Swatches(_, sel) => swatch::draw(cv, ctl, *sel, t),
                }
            }
        }
    }
    cv.unclip();
    // Content taller than the pane: a thin thumb, as the flyout draws it.
    if v.max_scroll() > 0.0 {
        let track = pane.h - 8.0;
        let thumb = (track * pane.h / v.height).max(24.0);
        let ty = pane.y + 4.0 + (track - thumb) * v.scroll / v.max_scroll();
        cv.round(Rect::new(pane.right() - 6.0, ty, 3.0, thumb), 1.5, t.fg3);
    }
    dropdown::draw_popup(cv, v, t, f);
}
