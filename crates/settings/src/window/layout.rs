//! Where everything sits, in DIPs (design `Settings` screen): the title bar, the nav column, the page's cards
//! and the open dropdown. One `View` holds the page being shown and its layout; painting and hit-testing both
//! read it, so what is drawn is what is clicked.

use super::choices::Opt;
use super::controls::{dropdown, order, preview, segmented, swatch, toggle};
use super::model::{Control, Item, Page};
use busy_ui::render::{Gfx, Rect};
use std::cell::RefCell;
use std::collections::HashMap;
use windows::Win32::Graphics::DirectWrite::{DWRITE_FONT_WEIGHT_SEMI_BOLD, IDWriteTextFormat};
use windows::core::Result;

/// Logical window size (design: 1000 × 700) and the smallest it may be resized to.
pub(super) const WIN_W: f32 = 1000.0;
pub(super) const WIN_H: f32 = 700.0;
pub(super) const MIN_W: f32 = 760.0;
pub(super) const MIN_H: f32 = 520.0;
pub(super) const TITLE_H: f32 = 40.0;
pub(super) const NAV_W: f32 = 272.0;
/// Nav column: padding 4 12 16 16, 3 between children.
const NAV_X: f32 = 16.0;
const NAV_ITEM_W: f32 = NAV_W - 16.0 - 12.0;
const NAV_GAP: f32 = 3.0;
/// App header (padding 8 4 16 around a 48-DIP tile), search box (32 high, 8 below).
pub(super) const HEADER_Y: f32 = TITLE_H + 4.0;
pub(super) const HEADER_H: f32 = 8.0 + 48.0 + 16.0;
pub(super) const SEARCH_Y: f32 = HEADER_Y + HEADER_H + NAV_GAP;
const NAV_Y: f32 = SEARCH_Y + 32.0 + 8.0 + NAV_GAP;
pub(super) const NAV_ITEM_H: f32 = 36.0;
/// Content pane: padding 12 32 32 20, children 4 apart.
const PAD_L: f32 = 20.0;
const PAD_R: f32 = 32.0;
const PAD_T: f32 = 12.0;
const PAD_B: f32 = 32.0;
const GAP: f32 = 4.0;
pub(super) const PAGE_TITLE_H: f32 = 36.0;
pub(super) const PAGE_SUB_H: f32 = 18.0;
/// Section header: 14 px semibold with margin 14 0 4.
pub(super) const HEADER_TOP: f32 = 14.0;
pub(super) const LINE_14: f32 = 20.0;
pub(super) const LINE_12: f32 = 16.0;
/// Setting card: min-height 68, padding 12 16 12 20, 16 between text and control; the text wraps the
/// control under it below 220 DIPs.
const CARD_MIN_H: f32 = 68.0;
pub(super) const CARD_PAD_L: f32 = 20.0;
const CARD_PAD_R: f32 = 16.0;
pub(super) const CARD_PAD_Y: f32 = 12.0;
const TEXT_MIN_W: f32 = 220.0;
/// Popup: 4 below its button, padding 4, 32-DIP options 2 apart.
pub(super) const OPT_H: f32 = 32.0;
pub(super) const OPT_GAP: f32 = 2.0;
pub(super) const POP_PAD: f32 = 4.0;

pub(super) struct Fonts {
    pub(super) caption: IDWriteTextFormat,
    pub(super) body: IDWriteTextFormat,
    pub(super) strong: IDWriteTextFormat,
    pub(super) small: IDWriteTextFormat,
    /// Card descriptions: 12 px, wrapping.
    pub(super) desc: IDWriteTextFormat,
    pub(super) sub: IDWriteTextFormat,
    pub(super) title: IDWriteTextFormat,
    /// Segment labels: 13 px.
    pub(super) seg: IDWriteTextFormat,
    /// Widths of segment labels, which are constant: measured once, not on every paint or pointer move.
    seg_widths: RefCell<HashMap<String, f32>>,
}

impl Fonts {
    pub(super) fn new(gfx: &Gfx) -> Result<Self> {
        Ok(Self {
            caption: gfx.format(12.0, false)?,
            body: gfx.format(14.0, false)?,
            strong: gfx.format(14.0, true)?,
            small: gfx.format(12.0, false)?,
            desc: gfx.wrapping(12.0)?,
            sub: gfx.format(13.0, false)?,
            title: gfx.format_weight(28.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            seg: gfx.format(13.0, false)?,
            seg_widths: RefCell::new(HashMap::new()),
        })
    }

    pub(super) fn seg_width(&self, gfx: &Gfx, s: &str) -> f32 {
        let mut cache = self.seg_widths.borrow_mut();
        // Bounded: the labels are a handful of constant strings.
        if cache.len() > 64 {
            cache.clear();
        }
        *cache.entry(s.to_owned()).or_insert_with(|| gfx.text_width(&self.seg, s))
    }
}

/// Something the pointer can hover or click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Min,
    Max,
    Close,
    Nav(usize),
    /// The search box.
    Search,
    /// A row's control (toggle, dropdown button), by item index.
    Ctl(usize),
    /// Part `k` of a row's control: a segment or a swatch.
    Part(usize, usize),
    /// The taskbar order's ↑ and ↓ of its `i`th module.
    Up(usize),
    Down(usize),
    /// An option of the open dropdown.
    Opt(usize),
}

/// An item's place in the content, in content coordinates (x from the content's left, y from its top before
/// scrolling).
pub(super) struct Placed {
    pub(super) rect: Rect,
    /// The control, for a row.
    pub(super) ctl: Rect,
    /// A row's text block width.
    pub(super) text_w: f32,
}

pub(super) struct Popup {
    /// The dropdown's item.
    pub(super) item: usize,
    /// In window coordinates.
    pub(super) rect: Rect,
    /// First option shown, when not all fit.
    pub(super) first: usize,
    pub(super) visible: usize,
    /// The option the arrow keys are on.
    pub(super) cursor: usize,
}

pub(super) struct View {
    pub(super) page: Page,
    pub(super) nav: Vec<Page>,
    pub(super) items: Vec<Item>,
    pub(super) placed: Vec<Placed>,
    /// Content height, padding included.
    pub(super) height: f32,
    pub(super) scroll: f32,
    /// Window size in DIPs.
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) hover: Option<Target>,
    pub(super) popup: Option<Popup>,
    /// "Find a setting": while not blank, the content shows the matching rows of all pages.
    pub(super) query: String,
    /// The keyboard focus (`Search`, `Nav`, `Ctl`, `Up` or `Down`), and whether its ring shows.
    pub(super) focus: Option<Target>,
    pub(super) focus_visible: bool,
}

impl View {
    pub(super) fn new(page: Page) -> Self {
        Self {
            page,
            nav: Vec::new(),
            items: Vec::new(),
            placed: Vec::new(),
            height: 0.0,
            scroll: 0.0,
            w: WIN_W,
            h: WIN_H,
            hover: None,
            popup: None,
            focus: None,
            focus_visible: false,
            query: String::new(),
        }
    }

    pub(super) fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// The page title and subtitle, or the search results' (design `pageTitle`, `pageSub`).
    pub(super) fn heading(&self) -> (&'static str, String) {
        if !self.searching() {
            return (self.page.title(), self.page.sub().into());
        }
        let q = self.query.trim();
        match self.items.iter().filter(|i| matches!(i, Item::Row(_))).count() {
            0 => ("Search results", format!("No settings match \u{201c}{q}\u{201d}")),
            1 => ("Search results", format!("1 setting matches \u{201c}{q}\u{201d}")),
            n => ("Search results", format!("{n} settings match \u{201c}{q}\u{201d}")),
        }
    }

    /// The content pane, in window coordinates.
    pub(super) fn pane(&self) -> Rect {
        Rect::new(NAV_W, TITLE_H, (self.w - NAV_W).max(0.0), (self.h - TITLE_H).max(0.0))
    }

    /// Where content coordinates start, in window coordinates.
    pub(super) fn origin(&self) -> (f32, f32) {
        (NAV_W + PAD_L, TITLE_H + PAD_T - self.scroll)
    }

    pub(super) fn max_scroll(&self) -> f32 {
        (self.height - self.pane().h).max(0.0)
    }

    pub(super) fn scroll_by(&mut self, dy: f32) -> bool {
        let s = (self.scroll + dy).clamp(0.0, self.max_scroll());
        let moved = s != self.scroll;
        self.scroll = s;
        moved
    }

    /// Lays out `items` for the current width; keeps the scroll within the new height.
    pub(super) fn lay_out(&mut self, gfx: &Gfx, f: &Fonts) {
        // A popup whose row is no longer a dropdown (a config synced from elsewhere) would pick for another row.
        if let Some(p) = &self.popup
            && !matches!(self.items.get(p.item), Some(Item::Row(r)) if matches!(r.control, Control::Dropdown(..)))
        {
            self.popup = None;
        }
        let width = (self.w - NAV_W - PAD_L - PAD_R).max(TEXT_MIN_W);
        let mut y = PAGE_TITLE_H + GAP + PAGE_SUB_H + 16.0 + GAP;
        self.placed = self
            .items
            .iter()
            .map(|item| {
                let p = place(item, y, width, gfx, f);
                y = p.rect.bottom() + GAP;
                p
            })
            .collect();
        self.height = PAD_T + y - GAP + PAD_B;
        self.scroll = self.scroll.min(self.max_scroll());
        // A focus on a control that is gone (another page, a row that went away) is dropped.
        if self.focus.is_some_and(|f| !matches!(f, Target::Nav(_) | Target::Search) && !self.stops().contains(&f)) {
            self.focus = None;
        }
    }

    /// Tab order: the focused (else the selected) nav item, then each control of the page, then each usable
    /// ↑/↓ of the taskbar order.
    pub(super) fn stops(&self) -> Vec<Target> {
        let nav = self.nav.iter().position(|&p| p == self.page).unwrap_or(0);
        let mut out = vec![
            Target::Search,
            match self.focus {
                Some(Target::Nav(i)) => Target::Nav(i),
                _ => Target::Nav(nav),
            },
        ];
        for (i, item) in self.items.iter().enumerate() {
            match item {
                Item::Row(_) => out.push(Target::Ctl(i)),
                Item::Order(list) => {
                    for j in 0..list.len() {
                        if j > 0 {
                            out.push(Target::Up(j));
                        }
                        if j + 1 < list.len() {
                            out.push(Target::Down(j));
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Focuses `t` (with its ring) and scrolls it into view.
    pub(super) fn set_focus(&mut self, t: Target) {
        self.focus = Some(t);
        self.focus_visible = true;
        let Some(r) = self.focus_rect(t) else { return };
        let pane = self.pane();
        if r.y < pane.y + 8.0 {
            self.scroll_by(r.y - pane.y - 8.0);
        } else if r.bottom() > pane.bottom() - 8.0 {
            self.scroll_by(r.bottom() - pane.bottom() + 8.0);
        }
    }

    /// What the focus ring surrounds, in window coordinates.
    pub(super) fn focus_rect(&self, t: Target) -> Option<Rect> {
        match t {
            Target::Nav(i) => Some(self.nav_rect(i)),
            Target::Ctl(i) => self.placed.get(i).map(|p| self.to_window(p.ctl)),
            Target::Up(j) | Target::Down(j) => {
                let p = self
                    .items
                    .iter()
                    .zip(&self.placed)
                    .find_map(|(it, p)| matches!(it, Item::Order(_)).then_some(p))?;
                Some(self.to_window(order::button(p.rect, j, matches!(t, Target::Up(_)))))
            }
            _ => None,
        }
    }

    /// A content rect in window coordinates.
    pub(super) fn to_window(&self, r: Rect) -> Rect {
        let (x, y) = self.origin();
        Rect::new(r.x + x, r.y + y, r.w, r.h)
    }

    pub(super) fn nav_rect(&self, i: usize) -> Rect {
        Rect::new(NAV_X, NAV_Y + i as f32 * (NAV_ITEM_H + NAV_GAP), NAV_ITEM_W, NAV_ITEM_H)
    }

    pub(super) fn search_rect(&self) -> Rect {
        Rect::new(NAV_X, SEARCH_Y, NAV_ITEM_W, 32.0)
    }

    /// The popup's option `k` (an index into all options), when shown.
    pub(super) fn option_rect(&self, k: usize) -> Option<Rect> {
        let p = self.popup.as_ref()?;
        let row = k.checked_sub(p.first).filter(|&r| r < p.visible)?;
        let y = p.rect.y + POP_PAD + row as f32 * (OPT_H + OPT_GAP);
        Some(Rect::new(p.rect.x + POP_PAD, y, p.rect.w - 2.0 * POP_PAD, OPT_H))
    }

    pub(super) fn options(&self) -> Option<(&[Opt], usize)> {
        let p = self.popup.as_ref()?;
        match &self.items.get(p.item)? {
            Item::Row(r) => match &r.control {
                Control::Dropdown(opts, sel) => Some((opts, *sel)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Opens item `i`'s dropdown under (or, without room, over) its button.
    pub(super) fn open_popup(&mut self, i: usize, gfx: &Gfx, f: &Fonts) {
        let Some(Item::Row(row)) = self.items.get(i) else { return };
        let Control::Dropdown(opts, sel) = &row.control else { return };
        let ctl = self.to_window(self.placed[i].ctl);
        let w = opts.iter().map(|o| gfx.text_width(&f.body, &o.label) + 32.0 + 2.0 * POP_PAD).fold(ctl.w, f32::max);
        let fit = |avail: f32| (((avail - 2.0 * POP_PAD + OPT_GAP) / (OPT_H + OPT_GAP)).floor().max(1.0)) as usize;
        let below = self.h - 8.0 - (ctl.bottom() + 4.0);
        let above = ctl.y - 4.0 - (TITLE_H + 8.0);
        let n = opts.len();
        let (visible, down) =
            if fit(below) >= n || below >= above { (fit(below).min(n), true) } else { (fit(above).min(n), false) };
        let h = 2.0 * POP_PAD + visible as f32 * (OPT_H + OPT_GAP) - OPT_GAP;
        let y = if down { ctl.bottom() + 4.0 } else { ctl.y - 4.0 - h };
        let x = (ctl.right() - w).max(NAV_W + 8.0);
        let first = sel.saturating_sub(visible.saturating_sub(1)).min(n - visible);
        self.popup = Some(Popup { item: i, rect: Rect::new(x, y, w, h), first, visible, cursor: *sel });
    }

    pub(super) fn hit(&self, gfx: &Gfx, f: &Fonts, x: f32, y: f32) -> Option<Target> {
        if let Some(p) = &self.popup {
            // Everything under an open popup belongs to it (a click elsewhere only closes it).
            let k = (p.first..p.first + p.visible).find(|&k| self.option_rect(k).is_some_and(|r| r.contains(x, y)));
            return k.map(Target::Opt);
        }
        if y < TITLE_H {
            return super::frame::button_at(self.w, x);
        }
        if x < NAV_W {
            if self.search_rect().contains(x, y) {
                return Some(Target::Search);
            }
            return (0..self.nav.len()).find(|&i| self.nav_rect(i).contains(x, y)).map(Target::Nav);
        }
        if !self.pane().contains(x, y) {
            return None;
        }
        let (ox, oy) = self.origin();
        let (cx, cy) = (x - ox, y - oy);
        for (i, (item, p)) in self.items.iter().zip(&self.placed).enumerate() {
            if !p.rect.contains(cx, cy) {
                continue;
            }
            return match item {
                Item::Row(row) if p.ctl.contains(cx, cy) => match &row.control {
                    Control::Segmented(opts, _) => segmented::segments(gfx, f, opts, p.ctl)
                        .iter()
                        .position(|s| s.contains(cx, cy))
                        .map(|k| Target::Part(i, k)),
                    Control::Swatches(..) => swatch::hit(p.ctl, cx, cy).map(|k| Target::Part(i, k)),
                    _ => Some(Target::Ctl(i)),
                },
                Item::Row(_) => None,
                Item::Order(list) => order::hit(p.rect, list.len(), cx, cy),
                Item::Header(_) | Item::Preview(_) => None,
            };
        }
        None
    }
}

fn place(item: &Item, y: f32, width: f32, gfx: &Gfx, f: &Fonts) -> Placed {
    match item {
        Item::Header(_) => {
            Placed { rect: Rect::new(0.0, y + HEADER_TOP, width, LINE_14 + 4.0), ctl: Rect::default(), text_w: width }
        }
        Item::Preview(_) => {
            Placed { rect: Rect::new(0.0, y, width, preview::HEIGHT), ctl: Rect::default(), text_w: width }
        }
        Item::Order(list) => {
            Placed { rect: Rect::new(0.0, y, width, order::height(list.len())), ctl: Rect::default(), text_w: width }
        }
        Item::Row(row) => {
            let (cw, ch) = match &row.control {
                Control::Toggle(..) => toggle::SIZE,
                Control::Dropdown(opts, _) => dropdown::size(gfx, &f.body, opts),
                Control::Segmented(opts, _) => segmented::size(gfx, f, opts),
                Control::Swatches(..) => swatch::SIZE,
            };
            let inner = width - CARD_PAD_L - CARD_PAD_R;
            let side = inner - cw - 16.0;
            let wrap = side < TEXT_MIN_W;
            let text_w = if wrap { inner } else { side };
            let (_, desc_h) = gfx.metrics(&f.desc, &row.desc, text_w);
            let text_h = LINE_14 + 2.0 + desc_h.max(LINE_12);
            let (h, ctl) = if wrap {
                let h = CARD_PAD_Y + text_h + 8.0 + ch + CARD_PAD_Y;
                (h, Rect::new(CARD_PAD_L, y + CARD_PAD_Y + text_h + 8.0, cw, ch))
            } else {
                let h = (CARD_PAD_Y * 2.0 + text_h).max(CARD_MIN_H);
                (h, Rect::new(width - CARD_PAD_R - cw, y + (h - ch) / 2.0, cw, ch))
            };
            Placed { rect: Rect::new(0.0, y, width, h), ctl, text_w }
        }
    }
}
