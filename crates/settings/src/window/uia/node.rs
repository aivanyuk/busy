//! What the window shows to UI Automation, pure: the elements under the window (`Node`s) in reading order,
//! each with a role, a name and the state a screen reader announces, read from the `View` as painted. A
//! `Tree` (`tree.rs`) is one frame's worth, which the providers answer from.

use crate::window::frame;
use crate::window::layout::{PAGE_TITLE_H, Target, View};
use crate::window::model::{self, Control, Flag, Item, Page};
use busy_core::Config;
use busy_ui::render::Rect;
use std::hash::{DefaultHasher, Hash, Hasher};

pub(in crate::window) use super::tree::{Entry, Tree};

/// An element under the window (the root, which the window's own provider stands for).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::window) enum Node {
    Min,
    Max,
    Close,
    Search,
    /// A nav item, by its page (the nav's order follows the taskbar order).
    Nav(Page),
    /// The page title (or "Search results").
    Title,
    /// Content item `i`: a section header, a row's control, the taskbar order's ↑/↓ of its `i`th module.
    Header(usize),
    Ctl(usize),
    Up(usize),
    Down(usize),
    /// The open dropdown of item `i`, and its option `k`.
    Popup(usize),
    Opt(usize, usize),
    /// Setup: a reading card, a position, Start with Windows, Skip, Start monitoring.
    Card(usize),
    Place(usize),
    Startup,
    Skip,
    Start,
}

/// The element's UIA control type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::window) enum Role {
    Button,
    CheckBox,
    ComboBox,
    Edit,
    Group,
    List,
    ListItem,
    Text,
    RadioButton,
}

/// What a screen reader is told about an element.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::window) struct Info {
    pub(in crate::window) role: Role,
    pub(in crate::window) name: String,
    /// A row's description, the page's subtitle.
    pub(in crate::window) help: String,
    /// A nav item's On/Off, "Selected" for the current page or option.
    pub(in crate::window) status: String,
    /// Search query, a dropdown's, segments' or swatches' current choice (UIA Value pattern).
    pub(in crate::window) value: Option<String>,
    /// A toggle's state (Toggle pattern).
    pub(in crate::window) toggle: Option<bool>,
    /// A radio button's state (SelectionItem pattern).
    pub(in crate::window) selected: Option<bool>,
    /// Can be invoked (Invoke pattern): buttons, nav items, options.
    pub(in crate::window) invoke: bool,
    pub(in crate::window) focusable: bool,
    pub(in crate::window) enabled: bool,
}

impl Node {
    /// Whether it is part of the content, which scrolls and changes with the page.
    pub(in crate::window) fn in_content(self) -> bool {
        matches!(
            self,
            Node::Title | Node::Header(_) | Node::Ctl(_) | Node::Up(_) | Node::Down(_) | Node::Popup(_) | Node::Opt(..)
        )
    }

    /// Three numbers that tell elements apart, for the runtime id (with the key).
    pub(in crate::window) fn id(self) -> [i32; 3] {
        let page = |p: Page| match p {
            Page::General => 0,
            Page::Module(m) => 1 + busy_core::Module::ALL.iter().position(|&a| a == m).unwrap_or(0) as i32,
            Page::Advanced => 99,
        };
        match self {
            Node::Min => [1, 0, 0],
            Node::Max => [2, 0, 0],
            Node::Close => [3, 0, 0],
            Node::Search => [4, 0, 0],
            Node::Nav(p) => [5, page(p), 0],
            Node::Title => [6, 0, 0],
            Node::Header(i) => [7, i as i32, 0],
            Node::Ctl(i) => [8, i as i32, 0],
            Node::Up(i) => [9, i as i32, 0],
            Node::Down(i) => [10, i as i32, 0],
            Node::Popup(i) => [11, i as i32, 0],
            Node::Opt(i, k) => [12, i as i32, k as i32],
            Node::Card(i) => [13, i as i32, 0],
            Node::Place(i) => [14, i as i32, 0],
            Node::Startup => [15, 0, 0],
            Node::Skip => [16, 0, 0],
            Node::Start => [17, 0, 0],
        }
    }

    pub(in crate::window) fn from_target(v: &View, t: Target) -> Option<Node> {
        Some(match t {
            Target::Min => Node::Min,
            Target::Max => Node::Max,
            Target::Close => Node::Close,
            Target::Search => Node::Search,
            Target::Nav(i) => Node::Nav(*v.nav.get(i)?),
            Target::Ctl(i) | Target::Part(i, _) => Node::Ctl(i),
            Target::Up(i) => Node::Up(i),
            Target::Down(i) => Node::Down(i),
            Target::Opt(k) => Node::Opt(v.popup.as_ref()?.item, k),
        })
    }

    /// What input on this element acts on; `None` for text, the popup and elements that are gone.
    pub(in crate::window) fn target(self, v: &View) -> Option<Target> {
        let t = match self {
            Node::Min => Target::Min,
            Node::Max => Target::Max,
            Node::Close => Target::Close,
            Node::Search => Target::Search,
            Node::Nav(p) => Target::Nav(v.nav.iter().position(|&q| q == p)?),
            Node::Ctl(i) => Target::Ctl(i),
            Node::Up(i) => Target::Up(i),
            Node::Down(i) => Target::Down(i),
            Node::Opt(_, k) => Target::Opt(k),
            Node::Title | Node::Header(_) | Node::Popup(_) => return None,
            // Setup's: see `uia::setup::target`.
            Node::Card(_) | Node::Place(_) | Node::Startup | Node::Skip | Node::Start => return None,
        };
        exists(v, self).then_some(t)
    }
}

/// The window's children in reading order: caption buttons, search, nav, the page title, then the content;
/// the open popup last.
pub(in crate::window) fn children(v: &View) -> Vec<Node> {
    let mut out = vec![Node::Min, Node::Max, Node::Close, Node::Search];
    out.extend(v.nav.iter().map(|&p| Node::Nav(p)));
    out.push(Node::Title);
    for (i, item) in v.items.iter().enumerate() {
        match item {
            Item::Header(_) => out.push(Node::Header(i)),
            Item::Row(_) => out.push(Node::Ctl(i)),
            // The taskbar order: its usable buttons, as its Tab stops.
            Item::Order(_) => out.extend(v.stops().into_iter().filter_map(|t| match t {
                Target::Up(j) => Some(Node::Up(j)),
                Target::Down(j) => Some(Node::Down(j)),
                _ => None,
            })),
            Item::Preview(_) => {}
        }
    }
    if let Some(p) = &v.popup {
        out.push(Node::Popup(p.item));
    }
    out
}

/// A node's children: the open popup's options; nothing for the rest.
pub(in crate::window) fn children_of(v: &View, n: Node) -> Vec<Node> {
    match (n, &v.popup, v.options()) {
        (Node::Popup(i), Some(p), Some((opts, _))) if p.item == i => (0..opts.len()).map(|k| Node::Opt(i, k)).collect(),
        _ => Vec::new(),
    }
}

pub(in crate::window) fn parent(n: Node) -> Option<Node> {
    match n {
        Node::Opt(i, _) => Some(Node::Popup(i)),
        _ => None,
    }
}

pub(in crate::window) fn exists(v: &View, n: Node) -> bool {
    match parent(n) {
        Some(p) => children_of(v, p).contains(&n),
        None => children(v).contains(&n),
    }
}

/// The element with the keyboard focus: the popup's cursor while it is open, else the focused target.
pub(in crate::window) fn focused(v: &View) -> Option<Node> {
    match &v.popup {
        Some(p) => Some(Node::Opt(p.item, p.cursor)),
        None => Node::from_target(v, v.focus?),
    }
}

/// Where it is, in window DIPs; `None` when it isn't shown (an option scrolled out of the popup).
fn rect(v: &View, n: Node) -> Option<Rect> {
    match n {
        Node::Min | Node::Max | Node::Close => Some(frame::button_rect(v.w, n.target(v)?)),
        Node::Search => Some(v.search_rect()),
        Node::Nav(_) | Node::Ctl(_) | Node::Up(_) | Node::Down(_) => v.focus_rect(n.target(v)?),
        Node::Title => {
            let (x, y) = v.origin();
            Some(Rect::new(x, y, (v.pane().right() - x).max(0.0), PAGE_TITLE_H))
        }
        Node::Header(i) => v.placed.get(i).map(|p| v.to_window(p.rect)),
        Node::Popup(_) => v.popup.as_ref().map(|p| p.rect),
        Node::Opt(_, k) => v.option_rect(k),
        // Setup's are in `uia::setup`.
        Node::Card(_) | Node::Place(_) | Node::Startup | Node::Skip | Node::Start => None,
    }
}

/// Content scrolled out of the pane, or an option scrolled out of the popup.
fn offscreen(v: &View, n: Node, r: Option<Rect>) -> bool {
    let Some(r) = r else { return true };
    let pane = v.pane();
    n.in_content() && !matches!(n, Node::Popup(_) | Node::Opt(..)) && (r.bottom() <= pane.y || r.y >= pane.bottom())
}

fn plain(role: Role, name: impl Into<String>) -> Info {
    Info {
        role,
        name: name.into(),
        help: String::new(),
        status: String::new(),
        value: None,
        toggle: None,
        selected: None,
        invoke: false,
        focusable: false,
        enabled: true,
    }
}

/// A caption button: clicked, never focused (they aren't Tab stops).
fn caption(name: &str) -> Info {
    Info { invoke: true, ..plain(Role::Button, name) }
}

fn button(name: impl Into<String>) -> Info {
    Info { invoke: true, focusable: true, ..plain(Role::Button, name) }
}

/// `maximized` names the Maximize button Restore; `autostart_busy` disables that toggle, as painted.
pub(in crate::window) fn info(v: &View, cfg: &Config, n: Node, maximized: bool, autostart_busy: bool) -> Option<Info> {
    if !exists(v, n) {
        return None;
    }
    let order_name = |j: usize, dir: &str| {
        let m = v.items.iter().find_map(|it| match it {
            Item::Order(list) => list.get(j).map(|&(m, _)| m),
            _ => None,
        })?;
        Some(format!("Move {} {dir}", m.label()))
    };
    Some(match n {
        Node::Min => caption("Minimize"),
        Node::Max => caption(if maximized { "Restore" } else { "Maximize" }),
        Node::Close => caption("Close"),
        Node::Search => Info { value: Some(v.query.clone()), focusable: true, ..plain(Role::Edit, "Find a setting") },
        Node::Nav(p) => {
            let mut status = match p {
                Page::Module(m) if model::is_on(cfg, m) => "On".to_string(),
                Page::Module(_) => "Off".to_string(),
                _ => String::new(),
            };
            if p == v.page && !v.searching() {
                status = if status.is_empty() { "Selected".into() } else { format!("{status}, selected") };
            }
            Info { status, invoke: true, focusable: true, ..plain(Role::ListItem, p.title()) }
        }
        Node::Title => {
            let (title, sub) = v.heading();
            Info { help: sub, ..plain(Role::Text, title) }
        }
        Node::Header(i) => match v.items.get(i)? {
            Item::Header(h) => plain(Role::Text, *h),
            _ => return None,
        },
        Node::Up(j) => button(order_name(j, "up")?),
        Node::Down(j) => button(order_name(j, "down")?),
        Node::Ctl(i) => {
            let Item::Row(row) = v.items.get(i)? else { return None };
            let base = Info { help: row.desc.clone(), focusable: true, ..plain(Role::Group, row.title) };
            match &row.control {
                Control::Toggle(flag, on) => Info {
                    role: Role::CheckBox,
                    toggle: Some(*on),
                    enabled: !(*flag == Flag::Autostart && autostart_busy),
                    ..base
                },
                Control::Dropdown(opts, sel) => {
                    Info { role: Role::ComboBox, value: opts.get(*sel).map(|o| o.label.clone()), ..base }
                }
                // Picked with ←/→, like a radio group.
                Control::Segmented(opts, sel) => Info { value: opts.get(*sel).map(|o| o.label.clone()), ..base },
                Control::Button(c) => Info { role: Role::Button, name: c.label().into(), invoke: true, ..base },
                Control::Swatches(_, sel) => {
                    Info { value: Some(format!("Color {} of {}", sel + 1, busy_core::PALETTE_LEN)), ..base }
                }
            }
        }
        Node::Popup(i) => {
            let Some(Item::Row(row)) = v.items.get(i) else { return None };
            plain(Role::List, row.title)
        }
        Node::Opt(_, k) => {
            let (opts, sel) = v.options()?;
            let status = if k == sel { "Selected" } else { "" };
            Info { status: status.into(), invoke: true, focusable: true, ..plain(Role::ListItem, &opts.get(k)?.label) }
        }
        Node::Card(_) | Node::Place(_) | Node::Startup | Node::Skip | Node::Start => return None,
    })
}

/// `node`'s key (see `Entry::key`): the page or search it was read from, and its name.
fn key(v: &View, n: Node, name: &str) -> u32 {
    if !n.in_content() {
        return 0;
    }
    let mut h = DefaultHasher::new();
    (v.page, v.searching().then(|| v.query.trim()), n, name).hash(&mut h);
    // Never 0, which is the chrome's.
    (h.finish() as u32).max(1)
}

/// The window's elements as the view shows them now.
pub(in crate::window) fn tree(v: &View, cfg: &Config, maximized: bool, autostart_busy: bool) -> Tree {
    let mut nodes: Vec<Node> = children(v);
    if let Some(p) = &v.popup {
        nodes.extend(children_of(v, Node::Popup(p.item)));
    }
    let entries: Vec<Entry> = nodes
        .into_iter()
        .filter_map(|n| {
            let info = info(v, cfg, n, maximized, autostart_busy)?;
            let r = rect(v, n);
            Some(Entry {
                node: n,
                key: key(v, n, &info.name),
                parent: parent(n),
                offscreen: offscreen(v, n, r),
                rect: r,
                info,
            })
        })
        .collect();
    let focus = focused(v).and_then(|n| entries.iter().find(|e| e.node == n)).map(|e| (e.node, e.key));
    Tree { entries, focus, pane: v.pane(), popup: v.popup.as_ref().map(|p| p.rect) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::choices::Choices;
    use crate::window::layout::Popup;
    use busy_core::{CellStyle, Module};

    fn view(page: Page, cfg: &Config) -> View {
        let mut v = View::new(page);
        v.nav = model::nav(cfg);
        v.items = model::items(page, cfg, &Choices::default());
        v
    }

    fn named<'a>(t: &'a Tree, name: &str) -> &'a Entry {
        t.entries.iter().find(|e| e.info.name == name).unwrap_or_else(|| panic!("{name}"))
    }

    #[test]
    fn general_reads_in_order_with_roles_and_names() {
        let cfg = Config::default();
        let v = view(Page::General, &cfg);
        let t = tree(&v, &cfg, false, false);
        let top: Vec<Node> = t.children(None).iter().map(|e| e.node).collect();
        assert_eq!(top, children(&v));
        assert_eq!(top[..5], [Node::Min, Node::Max, Node::Close, Node::Search, Node::Nav(Page::General)]);
        assert_eq!(top[4 + v.nav.len()], Node::Title);
        assert_eq!(named(&t, "Start with Windows").info.role, Role::CheckBox);
        assert_eq!(named(&t, "Start with Windows").info.toggle, Some(false));
        assert_eq!(named(&t, "Theme").info.role, Role::ComboBox);
        assert_eq!(named(&t, "Theme").info.value.as_deref(), Some("Use system setting"));
        assert_eq!(named(&t, "General").info.status, "Selected");
        assert_eq!(named(&t, "CPU").info.status, "On");
        assert_eq!(named(&t, "Disk").info.status, "Off");
        assert_eq!(named(&t, "Behavior").info.role, Role::Text);
        // Caption buttons are clicked, not focused; the order's first module has no ↑ and its last no ↓.
        assert!(named(&t, "Maximize").info.invoke && !named(&t, "Maximize").info.focusable);
        assert!(t.entries.iter().any(|e| e.info.name == "Move CPU down"));
        assert!(!t.entries.iter().any(|e| e.info.name == "Move CPU up"));
        // A busy autostart toggle is disabled.
        assert!(
            !tree(&v, &cfg, false, true).entries.iter().any(|e| e.info.name == "Start with Windows" && e.info.enabled)
        );
        assert_eq!(t.focus, None);
    }

    #[test]
    fn keys_tell_apart_what_took_an_index() {
        let mut cfg = Config::default();
        cfg.modules.iter_mut().for_each(|c| c.style = CellStyle::Text);
        let disk = view(Page::Module(Module::Disk), &cfg);
        let before = tree(&disk, &cfg, false, false);
        // Io style drops "Show label": the rows after it move up an index, and their keys tell.
        cfg.modules.iter_mut().for_each(|c| c.style = CellStyle::Io);
        let after = tree(&view(Page::Module(Module::Disk), &cfg), &cfg, false, false);
        let color = named(&before, "Widget color");
        assert!(after.get(color.node, color.key).is_none_or(|e| e.info.name == "Widget color"));
        let moved = named(&after, "Widget color");
        assert_ne!((moved.node, moved.key), (color.node, color.key));
        // The same element on the same page keeps its key; chrome keys are 0 and survive a page switch.
        assert_eq!(named(&tree(&disk, &cfg, false, false), "Style").key, named(&before, "Style").key);
        let general = tree(&view(Page::General, &cfg), &cfg, false, false);
        assert_eq!(named(&general, "Find a setting").key, 0);
        assert!(before.get(Node::Ctl(3), named(&before, "Style").key).is_none_or(|e| e.info.name == "Style"));
    }

    #[test]
    fn options_belong_to_their_dropdown() {
        let cfg = Config::default();
        let mut v = view(Page::General, &cfg);
        let theme = v.items.iter().position(|i| matches!(i, Item::Row(r) if r.title == "Theme")).unwrap_or(0);
        v.popup =
            Some(Popup { item: theme, rect: Rect::new(600.0, 300.0, 200.0, 110.0), first: 0, visible: 3, cursor: 1 });
        let t = tree(&v, &cfg, false, false);
        let opts = t.children(Some(Node::Popup(theme)));
        assert_eq!(opts.len(), 3);
        assert!(opts.iter().all(|e| matches!(e.node, Node::Opt(i, _) if i == theme)));
        assert_eq!(t.focus.map(|f| f.0), Some(Node::Opt(theme, 1)));
        assert!(!exists(&v, Node::Opt(theme + 1, 1)) && Node::Opt(theme + 1, 1).target(&v).is_none());
        // Everything over the popup belongs to it.
        assert!(matches!(t.at(610.0, 305.0).map(|e| e.node), Some(Node::Popup(_) | Node::Opt(..))));
    }

    #[test]
    fn targets_round_trip() {
        let cfg = Config::default();
        let v = view(Page::Module(Module::Cpu), &cfg);
        assert!(children(&v).iter().all(|&n| n.target(&v).is_none_or(|t| Node::from_target(&v, t) == Some(n))));
        let style = named(&tree(&v, &cfg, false, false), "Style").info.clone();
        assert_eq!((style.role, style.value.is_some()), (Role::Group, true));
    }
}
