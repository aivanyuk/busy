//! The COM side of UI Automation: the window's root provider (a fragment root over the `HWND`'s own
//! provider) and one element provider per element, with the Invoke, Toggle and Value patterns. UIA calls
//! providers on its own threads, so they never touch the window's state: they answer from the `Tree` the
//! window publishes after each frame (`Shared`), and queue what they are asked to do (invoke, toggle, focus,
//! typing) for the window, which runs it from its own message loop. An element whose node and key are no
//! longer in the tree, or a window that is gone, answers `UIA_E_ELEMENTNOTAVAILABLE`.

#![allow(non_upper_case_globals, reason = "matches on the SDK's UIA constants, which keep their SDK names")]

use super::node::{Entry, Node, Role, Tree};
use super::{Action, Request, WM_APP_UIA};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{E_OUTOFMEMORY, HWND, LPARAM, POINT, VARIANT_FALSE, VARIANT_TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, ScreenToClient};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement};
use windows::Win32::System::Variant::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{BOOL, BSTR, Error, HRESULT, IUnknown, IUnknownImpl, PCWSTR, Result, implement};

/// What the window shares with its providers.
pub(super) struct Shared {
    /// The window, as a number (a handle is not `Send`); every call on it here is safe from any thread.
    hwnd: isize,
    /// The last frame's elements and the window's scale; `None` once the window is gone. The lock only
    /// covers swapping or cloning the `Arc`: the tree is built before publishing and read after.
    tree: Mutex<Option<(Arc<Tree>, f32)>>,
    /// Requests for the window's message loop.
    pub(super) requests: Mutex<Vec<Request>>,
}

impl Shared {
    pub(super) fn new(hwnd: HWND) -> Self {
        Self { hwnd: hwnd.0 as isize, tree: Mutex::new(None), requests: Mutex::new(Vec::new()) }
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    /// Makes `tree` the one providers answer from (`None`: the window is gone).
    pub(super) fn publish(&self, tree: Option<(Arc<Tree>, f32)>) {
        let old = match self.tree.lock() {
            Ok(mut t) => std::mem::replace(&mut *t, tree),
            Err(_) => None,
        };
        // Freed here, after the guard is gone.
        drop(old);
    }

    /// Runs `f` on the published tree; fails when the window is gone.
    fn read<R>(&self, f: impl FnOnce(&Tree, f32) -> Option<R>) -> Result<R> {
        let snap = self.tree.lock().map_err(|_| gone())?.clone();
        let (tree, scale) = snap.ok_or_else(gone)?;
        f(&tree, scale).ok_or_else(gone)
    }
}

fn gone() -> Error {
    Error::from(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

fn err(code: u32) -> Error {
    Error::from(HRESULT(code as i32))
}

/// A null result with success (an absent sibling, an unsupported pattern).
fn none<T>() -> Result<T> {
    Err(Error::empty())
}

pub(super) fn variant_i4(n: i32) -> VARIANT {
    variant(VT_I4, VARIANT_0_0_0 { lVal: n })
}

pub(super) fn variant_bool(b: bool) -> VARIANT {
    variant(VT_BOOL, VARIANT_0_0_0 { boolVal: if b { VARIANT_TRUE } else { VARIANT_FALSE } })
}

/// Owns a `BSTR`: the receiver frees it (UIA for a property value; the caller, with `VariantClear`, for an
/// event's old and new values).
pub(super) fn variant_str(s: &str) -> VARIANT {
    variant(VT_BSTR, VARIANT_0_0_0 { bstrVal: ManuallyDrop::new(BSTR::from(s)) })
}

fn variant(vt: VARENUM, value: VARIANT_0_0_0) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: value,
            }),
        },
    }
}

fn control_type(r: Role) -> UIA_CONTROLTYPE_ID {
    match r {
        Role::Button => UIA_ButtonControlTypeId,
        Role::CheckBox => UIA_CheckBoxControlTypeId,
        Role::ComboBox => UIA_ComboBoxControlTypeId,
        Role::Edit => UIA_EditControlTypeId,
        Role::Group => UIA_GroupControlTypeId,
        Role::List => UIA_ListControlTypeId,
        Role::ListItem => UIA_ListItemControlTypeId,
        Role::Text => UIA_TextControlTypeId,
    }
}

/// A runtime id: UIA's "append to the window's id" marker, then ours.
fn runtime_id(ids: &[i32]) -> Result<*mut SAFEARRAY> {
    // SAFETY: creates a new VT_I4 vector; no pointer is passed in.
    let sa = unsafe { SafeArrayCreateVector(VT_I4, 0, ids.len() as u32) };
    if sa.is_null() {
        return Err(E_OUTOFMEMORY.into());
    }
    for (i, id) in ids.iter().enumerate() {
        // SAFETY: `sa` is the vector just made, `i` an index inside it, and `id` points at one live i32.
        if let Err(e) = unsafe { SafeArrayPutElement(sa, &(i as i32), (id as *const i32).cast()) } {
            // SAFETY: frees the vector made above, which nothing else holds.
            let _ = unsafe { SafeArrayDestroy(sa) };
            return Err(e);
        }
    }
    Ok(sa)
}

/// The window's root provider.
pub(super) fn root(shared: &Arc<Shared>) -> IRawElementProviderSimple {
    Root { shared: shared.clone() }.into()
}

pub(super) fn element(shared: &Arc<Shared>, e: &Entry) -> IRawElementProviderSimple {
    Element { shared: shared.clone(), node: e.node, key: e.key }.into()
}

fn fragment(shared: &Arc<Shared>, e: Option<(Node, u32)>) -> Result<IRawElementProviderFragment> {
    match e {
        Some((node, key)) => Ok(Element { shared: shared.clone(), node, key }.into()),
        None => none(),
    }
}

fn id(e: &Entry) -> (Node, u32) {
    (e.node, e.key)
}

#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IRawElementProviderFragmentRoot)]
struct Root {
    shared: Arc<Shared>,
}

impl IRawElementProviderSimple_Impl for Root_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> Result<IUnknown> {
        none()
    }

    /// Empty: the window's own (host) provider gives its name, type and the rest.
    fn GetPropertyValue(&self, _: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(VARIANT::default())
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        // SAFETY: our window's handle; UIA checks it is live.
        unsafe { UiaHostProviderFromHwnd(self.shared.hwnd()) }
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(&self, dir: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let to = self.shared.read(|t, _| {
            let kids = t.children(None);
            Some(match dir {
                NavigateDirection_FirstChild => kids.first().map(|e| id(e)),
                NavigateDirection_LastChild => kids.last().map(|e| id(e)),
                _ => None,
            })
        })?;
        fragment(&self.shared, to)
    }

    /// None: the host provider's id stands for the window.
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    /// Empty: the host provider gives the window's.
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(UiaRect::default())
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(self.to_interface())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let mut pt = POINT { x: x as i32, y: y as i32 };
        // SAFETY: reads our window's position; `pt` is a valid in/out pointer.
        let _ = unsafe { ScreenToClient(self.shared.hwnd(), &mut pt) };
        let to = self.shared.read(|t, s| Some(t.at(pt.x as f32 / s, pt.y as f32 / s).map(id)))?;
        fragment(&self.shared, to)
    }

    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let to = self.shared.read(|t, _| Some(t.focus))?;
        fragment(&self.shared, to)
    }
}

#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IInvokeProvider, IToggleProvider, IValueProvider)]
struct Element {
    shared: Arc<Shared>,
    node: Node,
    key: u32,
}

impl Element {
    /// Runs `f` with this element as the last frame has it (and that frame's tree and scale).
    fn with<R>(&self, f: impl FnOnce(&Tree, f32, &Entry) -> Option<R>) -> Result<R> {
        self.shared.read(|t, s| f(t, s, t.get(self.node, self.key)?))
    }

    /// Queues `action` for the window's message loop.
    fn request(&self, action: Action) -> Result<()> {
        self.with(|_, _, _| Some(()))?;
        let mut q = self.shared.requests.lock().map_err(|_| gone())?;
        // Bounded: a client flooding a hung window can't grow it without end.
        if q.len() < 64 {
            q.push(Request { node: self.node, key: self.key, action });
        }
        drop(q);
        // SAFETY: posts to our window; a window that is gone just fails the post.
        let _ = unsafe { PostMessageW(Some(self.shared.hwnd()), WM_APP_UIA, WPARAM(0), LPARAM(0)) };
        Ok(())
    }

    /// Invoke and Toggle: what a click does, unless the control is disabled.
    fn press(&self) -> Result<()> {
        if !self.with(|_, _, e| Some(e.info.enabled))? {
            return Err(err(UIA_E_ELEMENTNOTENABLED));
        }
        self.request(Action::Invoke)
    }
}

impl IRawElementProviderSimple_Impl for Element_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, id: UIA_PATTERN_ID) -> Result<IUnknown> {
        let (invoke, toggle, value) =
            self.with(|_, _, e| Some((e.info.invoke, e.info.toggle.is_some(), e.info.value.is_some())))?;
        match id {
            UIA_InvokePatternId if invoke => Ok(self.to_interface::<IInvokeProvider>().into()),
            UIA_TogglePatternId if toggle => Ok(self.to_interface::<IToggleProvider>().into()),
            UIA_ValuePatternId if value => Ok(self.to_interface::<IValueProvider>().into()),
            _ => none(),
        }
    }

    fn GetPropertyValue(&self, prop: UIA_PROPERTY_ID) -> Result<VARIANT> {
        self.with(|t, _, e| {
            let i = &e.info;
            let text = |s: &str| if s.is_empty() { VARIANT::default() } else { variant_str(s) };
            Some(match prop {
                UIA_ControlTypePropertyId => variant_i4(control_type(i.role).0),
                UIA_NamePropertyId => variant_str(&i.name),
                UIA_HelpTextPropertyId => text(&i.help),
                UIA_ItemStatusPropertyId => text(&i.status),
                UIA_IsKeyboardFocusablePropertyId => variant_bool(i.focusable),
                UIA_HasKeyboardFocusPropertyId => variant_bool(t.focus == Some(id(e))),
                UIA_IsEnabledPropertyId => variant_bool(i.enabled),
                UIA_IsOffscreenPropertyId => variant_bool(e.offscreen),
                _ => VARIANT::default(),
            })
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        none()
    }
}

impl IRawElementProviderFragment_Impl for Element_Impl {
    fn Navigate(&self, dir: NavigateDirection) -> Result<IRawElementProviderFragment> {
        // `None`: the window (root); `Some(None)`: nowhere.
        let to = self.with(|t, _, e| {
            let siblings = t.children(e.parent);
            let i = siblings.iter().position(|s| s.node == e.node)?;
            let kids = t.children(Some(e.node));
            Some(match dir {
                NavigateDirection_Parent => e.parent.map(|p| t.entries.iter().find(|s| s.node == p).map(id)),
                NavigateDirection_NextSibling => Some(siblings.get(i + 1).map(|s| id(s))),
                NavigateDirection_PreviousSibling => {
                    Some(i.checked_sub(1).and_then(|j| siblings.get(j)).map(|s| id(s)))
                }
                NavigateDirection_FirstChild => Some(kids.first().map(|s| id(s))),
                NavigateDirection_LastChild => Some(kids.last().map(|s| id(s))),
                _ => Some(None),
            })
        })?;
        match to {
            Some(to) => fragment(&self.shared, to),
            None => Ok(Root { shared: self.shared.clone() }.into()),
        }
    }

    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        let [a, b, c] = self.node.id();
        runtime_id(&[UiaAppendRuntimeId as i32, a, b, c, self.key as i32])
    }

    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let (r, s) = self.with(|_, s, e| Some((e.rect, s)))?;
        let Some(r) = r else { return Ok(UiaRect::default()) };
        let mut origin = POINT::default();
        // SAFETY: reads our window's position; `origin` is a valid in/out pointer.
        let _ = unsafe { ClientToScreen(self.shared.hwnd(), &mut origin) };
        let s = s as f64;
        Ok(UiaRect {
            left: origin.x as f64 + r.x as f64 * s,
            top: origin.y as f64 + r.y as f64 * s,
            width: r.w as f64 * s,
            height: r.h as f64 * s,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }

    fn SetFocus(&self) -> Result<()> {
        if !self.with(|_, _, e| Some(e.info.focusable))? {
            return Err(err(UIA_E_INVALIDOPERATION));
        }
        self.request(Action::Focus)
    }

    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(Root { shared: self.shared.clone() }.into())
    }
}

impl IInvokeProvider_Impl for Element_Impl {
    fn Invoke(&self) -> Result<()> {
        self.press()
    }
}

impl IToggleProvider_Impl for Element_Impl {
    fn Toggle(&self) -> Result<()> {
        self.press()
    }

    fn ToggleState(&self) -> Result<ToggleState> {
        self.with(|_, _, e| e.info.toggle.map(|on| if on { ToggleState_On } else { ToggleState_Off }))
    }
}

impl IValueProvider_Impl for Element_Impl {
    /// Only the search box takes a value.
    fn SetValue(&self, val: &PCWSTR) -> Result<()> {
        if self.node != Node::Search {
            return Err(err(UIA_E_INVALIDOPERATION));
        }
        // SAFETY: UIA passes a NUL-terminated string valid for the call.
        let s = unsafe { val.to_string() }.map_err(|_| err(UIA_E_INVALIDOPERATION))?;
        self.request(Action::SetQuery(s))
    }

    fn Value(&self) -> Result<BSTR> {
        self.with(|_, _, e| e.info.value.as_deref().map(BSTR::from))
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok((self.node != Node::Search).into())
    }
}
