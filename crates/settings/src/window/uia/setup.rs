//! Setup's elements for UI Automation, pure: Close, the headline and headings, a checkbox per reading card
//! (named by its module, the live reading as its status), the two positions as radio buttons (SelectionItem),
//! Start with Windows, Skip and Start monitoring. Setup's page doesn't change shape, so every key is 0.

use super::node::{Entry, Info, Node, Role, Tree};
use crate::window::layout::TITLE_H;
use crate::window::setup::{self, PLACES, POSITION, READINGS, SKIP, START, STARTUP, SUB, Target, View, W};
use busy_core::Config;
use busy_ui::render::Rect;

pub(in crate::window) fn node(t: Target) -> Node {
    match t {
        Target::Close => Node::Close,
        Target::Card(i) => Node::Card(i),
        Target::Place(i) => Node::Place(i),
        Target::Startup => Node::Startup,
        Target::Skip => Node::Skip,
        Target::Start => Node::Start,
    }
}

pub(in crate::window) fn target(n: Node) -> Option<Target> {
    Some(match n {
        Node::Close => Target::Close,
        Node::Card(i) => Target::Card(i),
        Node::Place(i) => Target::Place(i),
        Node::Startup => Target::Startup,
        Node::Skip => Target::Skip,
        Node::Start => Target::Start,
        _ => return None,
    })
}

fn info(role: Role, name: &str) -> Info {
    Info {
        role,
        name: name.into(),
        help: String::new(),
        status: String::new(),
        value: None,
        toggle: None,
        selected: None,
        invoke: false,
        focusable: true,
        enabled: true,
    }
}

/// Setup's elements as its view and the config show them.
pub(in crate::window) fn tree(v: &View, cfg: &Config, autostart_busy: bool) -> Tree {
    let entry =
        |node: Node, info: Info, rect: Option<Rect>| Entry { node, key: 0, parent: None, info, rect, offscreen: false };
    let l = v.layout.as_ref();
    let rect = |t: Target| l.and_then(|l| l.rect(t));
    let close = Info { invoke: true, focusable: false, ..info(Role::Button, "Close") };
    let mut entries = vec![entry(Node::Close, close, rect(Target::Close))];
    let headline = Info { help: SUB.into(), focusable: false, ..info(Role::Text, setup::HEADLINE) };
    entries.push(entry(Node::Title, headline, l.map(|l| l.headline)));
    let heading = |name: &str| Info { focusable: false, ..info(Role::Text, name) };
    entries.push(entry(Node::Header(0), heading(READINGS), l.map(|l| l.readings)));
    for (i, (m, on)) in setup::cards(cfg).into_iter().enumerate() {
        let status = v.samples.get(i).cloned().flatten().unwrap_or_default();
        let card = Info { status, toggle: Some(on), ..info(Role::CheckBox, m.label()) };
        entries.push(entry(Node::Card(i), card, rect(Target::Card(i))));
    }
    entries.push(entry(Node::Header(1), heading(POSITION), l.map(|l| l.position)));
    let chosen = setup::place(cfg);
    for (i, (_, label, desc)) in PLACES.iter().enumerate() {
        let place =
            Info { help: (*desc).into(), selected: Some(i == chosen), invoke: true, ..info(Role::RadioButton, label) };
        entries.push(entry(Node::Place(i), place, rect(Target::Place(i))));
    }
    let startup = Info { toggle: Some(cfg.autostart), enabled: !autostart_busy, ..info(Role::CheckBox, STARTUP) };
    entries.push(entry(Node::Startup, startup, rect(Target::Startup)));
    for (n, t, name) in [(Node::Skip, Target::Skip, SKIP), (Node::Start, Target::Start, START)] {
        entries.push(entry(n, Info { invoke: true, ..info(Role::Button, name) }, rect(t)));
    }
    let h = l.map_or(TITLE_H, |l| l.h);
    Tree { entries, focus: v.focus.map(|t| (node(t), 0)), pane: Rect::new(0.0, 0.0, W, h), popup: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_reads_as_checkboxes_radios_and_buttons() {
        let mut cfg = Config::default();
        let v = View { focus: Some(Target::Card(1)), ..View::default() };
        let t = tree(&v, &cfg, false);
        let named = |t: &Tree, name: &str| t.entries.iter().find(|e| e.info.name == name).cloned();
        let cpu = named(&t, "CPU").map(|e| (e.node, e.info.role, e.info.toggle));
        assert_eq!(cpu, Some((Node::Card(0), Role::CheckBox, Some(true))));
        let tray = named(&t, "Next to the system tray").map(|e| (e.info.role, e.info.selected));
        assert_eq!(tray, Some((Role::RadioButton, Some(true))));
        assert!(named(&t, "Start with Windows").is_some_and(|e| e.info.toggle == Some(false)));
        assert!(named(&t, "Start monitoring").is_some_and(|e| e.info.invoke));
        assert_eq!(t.focus, Some((Node::Card(1), 0)));
        // Nodes and targets map both ways; the busy autostart checkbox is disabled.
        assert!(t.entries.iter().all(|e| target(e.node).is_none_or(|x| node(x) == e.node)));
        cfg.autostart = true;
        assert!(!tree(&v, &cfg, true).entries.iter().any(|e| e.node == Node::Startup && e.info.enabled));
    }
}
