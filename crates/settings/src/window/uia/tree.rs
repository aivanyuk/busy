//! One frame's worth of the window's elements, which the providers answer from: each element's node, key,
//! parent, what a screen reader is told and where it is.

use super::node::{Info, Node};
use busy_ui::render::Rect;

/// One element of a `Tree`.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::window) struct Entry {
    pub(in crate::window) node: Node,
    /// Tells this element from one that took its place: content ids are indexes, which name another row after
    /// a page switch, a search or a row coming or going. 0 outside the content.
    pub(in crate::window) key: u32,
    pub(in crate::window) parent: Option<Node>,
    pub(in crate::window) info: Info,
    /// In window DIPs; `None` when not shown (an option scrolled out of the popup).
    pub(in crate::window) rect: Option<Rect>,
    pub(in crate::window) offscreen: bool,
}

/// The window's elements as one frame shows them.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::window) struct Tree {
    /// Top-level elements in reading order, then the popup's options.
    pub(in crate::window) entries: Vec<Entry>,
    /// The element with the keyboard focus.
    pub(in crate::window) focus: Option<(Node, u32)>,
    /// The content pane and the open popup, in window DIPs, for hit-testing.
    pub(in crate::window) pane: Rect,
    pub(in crate::window) popup: Option<Rect>,
}

impl Tree {
    pub(in crate::window) fn get(&self, n: Node, key: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.node == n && e.key == key)
    }

    /// The children of `parent` (`None`: the window), in order.
    pub(in crate::window) fn children(&self, parent: Option<Node>) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.parent == parent).collect()
    }

    /// The element at a window-DIP point: an option or the popup while one is open, else a control, a nav
    /// item, the search box or a caption button.
    pub(in crate::window) fn at(&self, x: f32, y: f32) -> Option<&Entry> {
        let hit = |e: &&Entry| e.rect.is_some_and(|r| r.contains(x, y));
        if self.popup.is_some_and(|p| p.contains(x, y)) {
            return self
                .entries
                .iter()
                .filter(|e| matches!(e.node, Node::Opt(..)))
                .find(hit)
                .or_else(|| self.entries.iter().find(|e| matches!(e.node, Node::Popup(_))));
        }
        let content = self.pane.contains(x, y);
        self.entries
            .iter()
            .filter(|e| e.parent.is_none() && (e.info.invoke || e.info.focusable))
            .find(|e| (content || !e.node.in_content()) && hit(e))
    }
}
