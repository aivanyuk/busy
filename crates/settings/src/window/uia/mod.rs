//! UI Automation for screen readers (plan: basic roles and names): the window answers `WM_GETOBJECT` with a
//! root provider whose elements are the title-bar buttons, the search box, the nav items, the page's headers
//! and controls and the open popup's options (`node`), served by COM providers (`provider`). Once a client
//! has asked, each frame publishes the elements for the providers and announces what changed: the focus as
//! it moves, the focused toggle or value, and the content's structure.

use super::Ui;
use super::layout::Target;
use node::{Node, Tree};
use provider::Shared;
use std::cell::RefCell;
use std::sync::Arc;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Variant::{VARIANT, VariantClear};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsZoomed, WM_APP};

mod node;
mod provider;
mod setup;
mod tree;

/// Posted to the window when UIA queued a request.
pub(super) const WM_APP_UIA: u32 = WM_APP + 2;

pub(super) enum Action {
    /// What a click (or Space) on it does; also a toggle's Toggle.
    Invoke,
    Focus,
    /// The search box's new query.
    SetQuery(String),
}

/// An action on an element, named as the tree had it; dropped if that element is gone by the time it runs.
pub(super) struct Request {
    node: Node,
    key: u32,
    action: Action,
}

/// Nothing until a UIA client first asks for the window, so without one it costs nothing.
#[derive(Default)]
pub(super) struct State {
    shared: RefCell<Option<Arc<Shared>>>,
    root: RefCell<Option<IRawElementProviderSimple>>,
    /// The tree last published, to announce what changed.
    last: RefCell<Option<Arc<Tree>>>,
}

impl Ui {
    pub(super) fn maximized(&self) -> bool {
        // SAFETY: our live window.
        unsafe { IsZoomed(self.hwnd) }.as_bool()
    }

    /// The elements as the view shows them now; `None` while it is being changed.
    fn uia_tree(&self) -> Option<Tree> {
        let cfg = self.cfg.try_borrow().ok()?;
        if self.in_setup() {
            let vertical = self.choices.try_borrow().ok()?.edge.is_vertical();
            return Some(setup::tree(&*self.setup.try_borrow().ok()?, &cfg, self.autostart_busy(), vertical));
        }
        let v = self.view.try_borrow().ok()?;
        Some(node::tree(&v, &cfg, self.maximized(), self.autostart_busy()))
    }

    /// `WM_GETOBJECT` for UIA's root object id.
    pub(super) fn uia_object(&self, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
        if l.0 as i32 != UiaRootObjectId {
            return None;
        }
        if self.uia.shared.borrow().is_none() {
            let shared = Arc::new(Shared::new(self.hwnd));
            *self.uia.root.borrow_mut() = Some(provider::root(&shared));
            *self.uia.shared.borrow_mut() = Some(shared);
            self.uia_frame();
        }
        let root = self.uia.root.borrow().clone()?;
        // SAFETY: our window, the message's own parameters and a live provider.
        Some(unsafe { UiaReturnRawElementProvider(self.hwnd, w, l, &root) })
    }

    /// `WM_DESTROY`: the providers go dark and UIA lets go of them.
    pub(super) fn uia_destroy(&self) {
        if let Some(shared) = self.uia.shared.take() {
            shared.publish(None);
            if let Ok(mut q) = shared.requests.lock() {
                q.clear();
            }
        }
        self.uia.root.take();
        // SAFETY: our window; a null provider with zero parameters is the documented release call. The window's
        // providers are the process's only ones, so all are disconnected.
        unsafe {
            UiaReturnRawElementProvider(self.hwnd, WPARAM(0), LPARAM(0), None);
            let _ = UiaDisconnectAllProviders();
        }
    }

    /// After each frame: publishes the elements as drawn and announces what changed since the last frame.
    pub(super) fn uia_frame(&self) {
        let Some(shared) = self.uia.shared.borrow().clone() else { return };
        let Some(tree) = self.uia_tree().map(Arc::new) else { return };
        shared.publish(Some((tree.clone(), self.scale())));
        let Some(last) = self.uia.last.replace(Some(tree.clone())) else { return };
        // SAFETY: no arguments; reads UIA's state.
        if last == tree || !unsafe { UiaClientsAreListening() }.as_bool() {
            return;
        }
        let top = |t: &Tree| t.children(None).iter().map(|e| (e.node, e.key)).collect::<Vec<_>>();
        if top(&last) != top(&tree)
            && let Some(root) = self.uia.root.borrow().clone()
        {
            // SAFETY: a provider we made; no runtime id is needed for the window's own children.
            let _ = unsafe {
                UiaRaiseStructureChangedEvent(&root, StructureChangeType_ChildrenInvalidated, std::ptr::null_mut(), 0)
            };
        }
        // SAFETY: no arguments.
        if unsafe { GetForegroundWindow() } != self.hwnd {
            return;
        }
        let Some(now) = tree.focus.and_then(|(n, k)| tree.get(n, k)) else { return };
        let el = provider::element(&shared, now);
        if last.focus != tree.focus {
            // SAFETY: a provider we made.
            let _ = unsafe { UiaRaiseAutomationEvent(&el, UIA_AutomationFocusChangedEventId) };
            return;
        }
        let Some(was) = last.get(now.node, now.key) else { return };
        let (old, new) = (&was.info, &now.info);
        if old.toggle != new.toggle {
            let state = |t: Option<bool>| {
                provider::variant_i4(if t == Some(true) { ToggleState_On.0 } else { ToggleState_Off.0 })
            };
            raise_changed(&el, UIA_ToggleToggleStatePropertyId, state(old.toggle), state(new.toggle));
        }
        if old.value != new.value {
            let value = |v: &Option<String>| provider::variant_str(v.as_deref().unwrap_or(""));
            raise_changed(&el, UIA_ValueValuePropertyId, value(&old.value), value(&new.value));
        }
    }

    /// `WM_APP_UIA`: runs the queued requests whose element is still there, as the keyboard would.
    pub(super) fn uia_requests(&self) {
        let Some(shared) = self.uia.shared.borrow().clone() else { return };
        let requests = shared.requests.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default();
        for r in requests {
            // An earlier request may have closed the window (Close, Skip, Start monitoring).
            // SAFETY: only checks whether the handle still names a window.
            if !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(self.hwnd)) }.as_bool() {
                break;
            }
            // Checked against the view as it is now: an earlier request may have changed it.
            let here = self.uia_tree().is_some_and(|t| t.get(r.node, r.key).is_some());
            if self.in_setup() {
                match (r.action, setup::target(r.node).filter(|_| here)) {
                    (Action::Invoke, Some(t)) => self.setup_activate(t),
                    (Action::Focus, Some(t)) => self.setup_focus(t),
                    _ => {}
                }
                continue;
            }
            let Some(t) = here.then(|| r.node.target(&self.view.borrow())).flatten() else { continue };
            match (r.action, t) {
                (Action::Invoke, t) => self.press(t),
                (Action::Focus, Target::Opt(k)) => {
                    if let Some(p) = self.view.borrow_mut().popup.as_mut() {
                        p.cursor = k;
                        // Keep the cursor among the options shown.
                        p.first = p.first.min(k).max((k + 1).saturating_sub(p.visible));
                    }
                    self.invalidate();
                }
                (Action::Focus, t) => {
                    let mut v = self.view.borrow_mut();
                    v.popup = None;
                    v.set_focus(t);
                    drop(v);
                    self.invalidate();
                }
                (Action::SetQuery(q), Target::Search) => {
                    let mut v = self.view.borrow_mut();
                    v.popup = None;
                    v.query = q.chars().filter(|c| !c.is_control()).take(64).collect();
                    v.scroll = 0.0;
                    drop(v);
                    self.rebuild();
                }
                _ => {}
            }
        }
    }
}

/// Raises a property change, then frees the two values.
fn raise_changed(el: &IRawElementProviderSimple, id: UIA_PROPERTY_ID, mut old: VARIANT, mut new: VARIANT) {
    // SAFETY: a provider we made and two initialized VARIANTs, which we own and clear after the call.
    unsafe {
        let _ = UiaRaiseAutomationPropertyChangedEvent(el, id, &old, &new);
        let _ = VariantClear(&mut old);
        let _ = VariantClear(&mut new);
    }
}
