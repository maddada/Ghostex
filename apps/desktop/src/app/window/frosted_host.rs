//! The frosted child windows the app's menus and tooltips draw in while the main window is glass.
//!
//! CDXC:Theming 2026-09-25 DECISION:
//! User, of the sidebar menu and the header's ⋯ menu reading as near-opaque dark boxes: "is it possible to make context menu and these menus in the app match the look of the app when transparency is enabled better?", then "ok do ur best option / apply this to all the menus and tooltips like the ... and sidebar menu etc". Under window glass every menu and tooltip is a frosted surface: it draws in a blurred window of its own, filled with the frosted menu fill (`frosted_menu_fill`), with a faint ink border and the soft ink wash on its highlighted row. Menus that already had a window (the header's dropdowns, context menus, the chat's menus) keep it; the sidebar's menus and every tooltip, which were drawn inside the main window where nothing can blur, move into the windows kept here. Glass off, they all look as before.
//!
//! CDXC:Theming 2026-09-25 WHY:
//! Only one menu and one tooltip are ever up, so each kind keeps one window and hides it between uses rather than opening a new one each time (a GPUI window pays for a new Metal surface; tooltips come and go on every hover). A window's blur is limited to the rounded frames its content reports (`Window::report_frosted_region`), so a menu with a submenu, or a tooltip bubble with its margins, blurs only its panels. A window that is replaced is closed first, and nothing here holds its owner alive, so a leftover can never outlive what it showed (the lesson of the stacked scroll pills).

use std::{cell::RefCell, rc::Rc};

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, EntityId, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, Subscription, Window, WindowBounds,
    WindowHandle, WindowOptions, div,
};

use crate::app::helpers::window_glass_active;

/// What a frosted host window shows, rendered in that window.
pub(crate) type FrostedContent = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrostedHostKind {
    Tooltip,
    /// One panel of the sidebar's menu: the menu itself at 0, each submenu stacked above it.
    SidebarMenu(u8),
    /// The Docs toolbar over selected text (`native_docs/notes_windows.rs`).
    DocsSelectionToolbar,
    /// A Quick Access filter's picker (`window/quick_access/chrome.rs`), drawn over the Quick
    /// Access window rather than the main one.
    QuickAccessPicker,
    /// A Quick Access row's actions menu or its Actions panel (`window/quick_access/actions_menu.rs`).
    QuickAccessActions,
    /// The sidebar's account usage strip while it peeks over the list unpinned
    /// (`native_sidebar/usage.rs`).
    SidebarUsage,
}

/// How many stacked sidebar menu panels get a window of their own; deeper ones share none.
pub(crate) const SIDEBAR_MENU_HOST_LEVELS: u8 = 4;

/// The corner radius of a sidebar menu panel's window (the panel's own 8px at 100% zoom).
pub(crate) const SIDEBAR_MENU_HOST_RADIUS: f32 = 8.0;

/// The corner radius of the sidebar's peeking usage strip, which its window's blur takes too.
pub(crate) const SIDEBAR_USAGE_HOST_RADIUS: f32 = 8.0;

/// The corner radius of the Docs selection toolbar, which its window's blur takes too.
pub(crate) const DOCS_SELECTION_TOOLBAR_RADIUS: f32 = 8.0;

/// Whether menus and tooltips drawn inside the main window move into frosted host windows now.
/// macOS only: a host's blur is limited to its content's frames, which only the macOS window
/// backend can do; elsewhere they stay in the window with their solid fill.
pub(crate) fn frosted_hosting_active() -> bool {
    cfg!(target_os = "macos") && window_glass_active()
}

#[derive(Default)]
struct HostSlot {
    handle: Option<WindowHandle<FrostedHostView>>,
    /// The window the open host is attached to.
    parent: Option<AnyWindowHandle>,
    /// The frame on screen, in the parent's content coordinates, or `None` while hidden.
    shown: Option<Bounds<Pixels>>,
    /// Where the latest request wants it (`None` hides it).
    wanted: Option<(AnyWindowHandle, Bounds<Pixels>)>,
    content: Option<FrostedContent>,
    /// Identifies the content, so the same tooltip reported again changes nothing.
    content_key: Option<EntityId>,
    /// Content that must repaint when this entity notifies (a menu drawn from the app's state).
    observe: Option<gpui::Entity<crate::GhostexGpuiApp>>,
    busy: bool,
}

thread_local! {
    static TOOLTIP_HOST: RefCell<HostSlot> = RefCell::default();
    static DOCS_SELECTION_TOOLBAR_HOST: RefCell<HostSlot> = RefCell::default();
    static QUICK_ACCESS_PICKER_HOST: RefCell<HostSlot> = RefCell::default();
    static QUICK_ACCESS_ACTIONS_HOST: RefCell<HostSlot> = RefCell::default();
    static SIDEBAR_USAGE_HOST: RefCell<HostSlot> = RefCell::default();
    static SIDEBAR_MENU_HOSTS: RefCell<Vec<HostSlot>> = RefCell::default();
}

fn with_slot<R>(kind: FrostedHostKind, f: impl FnOnce(&mut HostSlot) -> R) -> R {
    match kind {
        FrostedHostKind::Tooltip => TOOLTIP_HOST.with(|slot| f(&mut slot.borrow_mut())),
        FrostedHostKind::DocsSelectionToolbar => {
            DOCS_SELECTION_TOOLBAR_HOST.with(|slot| f(&mut slot.borrow_mut()))
        }
        FrostedHostKind::QuickAccessPicker => {
            QUICK_ACCESS_PICKER_HOST.with(|slot| f(&mut slot.borrow_mut()))
        }
        FrostedHostKind::QuickAccessActions => {
            QUICK_ACCESS_ACTIONS_HOST.with(|slot| f(&mut slot.borrow_mut()))
        }
        FrostedHostKind::SidebarUsage => SIDEBAR_USAGE_HOST.with(|slot| f(&mut slot.borrow_mut())),
        FrostedHostKind::SidebarMenu(level) => SIDEBAR_MENU_HOSTS.with(|slots| {
            let mut slots = slots.borrow_mut();
            let level = usize::from(level);
            if slots.len() <= level {
                slots.resize_with(level + 1, HostSlot::default);
            }
            f(&mut slots[level])
        }),
    }
}

/// Shows `content` in `kind`'s host at `frame` (in `parent`'s content coordinates). `key`
/// identifies the content: a request with the same key and frame as the last one does nothing.
/// `observe` repaints the host whenever that entity notifies. Safe to call while drawing.
pub(crate) fn show_frosted_host(
    kind: FrostedHostKind,
    parent: AnyWindowHandle,
    frame: Bounds<Pixels>,
    key: Option<EntityId>,
    content: FrostedContent,
    observe: Option<gpui::Entity<crate::GhostexGpuiApp>>,
    cx: &mut App,
) {
    let schedule = with_slot(kind, |slot| {
        let same = key.is_some()
            && slot.content_key == key
            && slot.wanted == Some((parent, frame))
            && slot.content.is_some();
        if same {
            return false;
        }
        slot.content = Some(content);
        slot.content_key = key;
        slot.observe = observe;
        slot.wanted = Some((parent, frame));
        !std::mem::replace(&mut slot.busy, true)
    });
    if schedule {
        cx.defer(move |cx| apply(kind, cx));
    }
}

/// Hides `kind`'s host, keeping its window for the next use. Safe to call while drawing, and
/// cheap when it is already hidden.
pub(crate) fn hide_frosted_host(kind: FrostedHostKind, cx: &mut App) {
    let schedule = with_slot(kind, |slot| {
        if slot.wanted.is_none() && slot.content.is_none() {
            return false;
        }
        slot.wanted = None;
        slot.content = None;
        slot.content_key = None;
        slot.observe = None;
        !std::mem::replace(&mut slot.busy, true)
    });
    if schedule {
        cx.defer(move |cx| apply(kind, cx));
    }
}

/// Hides `kind`'s host only while it sits over `parent`, leaving a host over another window alone.
pub(crate) fn hide_frosted_host_over(kind: FrostedHostKind, parent: AnyWindowHandle, cx: &mut App) {
    if with_slot(kind, |slot| {
        slot.wanted.is_some_and(|(over, _)| over == parent)
    }) {
        hide_frosted_host(kind, cx);
    }
}

fn apply(kind: FrostedHostKind, cx: &mut App) {
    loop {
        let (wanted, handle, parent, shown) = with_slot(kind, |slot| {
            (slot.wanted, slot.handle, slot.parent, slot.shown)
        });
        let handle = handle.filter(|handle| handle.update(cx, |_, _, _| ()).is_ok());
        match wanted {
            None => {
                if let Some(handle) = handle
                    && shown.is_some()
                {
                    set_visible(handle.into(), None, false, cx);
                }
                with_slot(kind, |slot| {
                    slot.handle = handle;
                    slot.shown = None;
                });
            }
            Some((target, frame)) => {
                if let Some(handle) = handle
                    && parent == Some(target)
                {
                    if shown != Some(frame) {
                        let parent_view = native_view_of(target, cx);
                        crate::app::native_chat::child_window::move_child_window(
                            handle.into(),
                            parent_view.unwrap_or(std::ptr::null_mut()),
                            frame,
                            cx,
                        );
                    }
                    if shown.is_none() {
                        // After the move above, which also runs from a task, so a reused window
                        // never shows at its last spot first.
                        cx.spawn(async move |cx| {
                            let _ =
                                cx.update(|cx| set_visible(handle.into(), Some(target), true, cx));
                        })
                        .detach();
                    }
                    let observe = with_slot(kind, |slot| slot.observe.clone());
                    let _ = handle.update(cx, |view, _, cx| {
                        view.observe(observe, cx);
                        cx.notify();
                    });
                    with_slot(kind, |slot| slot.shown = Some(frame));
                } else {
                    if let Some(handle) = handle {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    let opened = open_host(kind, target, frame, cx);
                    with_slot(kind, |slot| {
                        slot.handle = opened;
                        slot.parent = opened.map(|_| target);
                        slot.shown = opened.map(|_| frame);
                    });
                }
            }
        }
        // A request that arrived while this one was applied is applied next.
        let done = with_slot(kind, |slot| {
            let done = slot.wanted == wanted;
            if done {
                slot.busy = false;
            }
            done
        });
        if done {
            return;
        }
    }
}

fn open_host(
    kind: FrostedHostKind,
    parent: AnyWindowHandle,
    frame: Bounds<Pixels>,
    cx: &mut App,
) -> Option<WindowHandle<FrostedHostView>> {
    let parent_view = native_view_of(parent, cx)?;
    let (origin, display_id) = parent
        .update(cx, |_, window, cx| {
            (
                crate::app::native_chat::child_window::content_bounds(window).origin,
                window.display(cx).map(|display| display.id()),
            )
        })
        .ok()?;
    let screen_frame = Bounds::new(origin + frame.origin, frame.size);
    let display_id =
        crate::app::window::popup_frame::display_at(screen_frame.center(), cx).or(display_id);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(screen_frame)),
            display_id,
            titlebar: None,
            kind: gpui::WindowKind::PopUp,
            focus: false,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            app_id: crate::gpui_platform_window_app_id(),
            icon: crate::gpui_platform_window_icon(),
            window_background: gpui::WindowBackgroundAppearance::Blurred,
            ..Default::default()
        },
        move |window, cx| {
            crate::app::helpers::apply_frosted_menu_blur(window);
            match kind {
                // A tooltip bubble sits inside margins, so its window's blur follows the bubble.
                FrostedHostKind::Tooltip => window.set_frosted_surface(true),
                // A menu panel fills its window exactly, the way a header dropdown does, so the
                // window is simply rounded to the panel's corners.
                FrostedHostKind::SidebarMenu(_) => {
                    window.set_background_corner_radius(gpui::px(SIDEBAR_MENU_HOST_RADIUS))
                }
                FrostedHostKind::DocsSelectionToolbar => {
                    window.set_background_corner_radius(gpui::px(DOCS_SELECTION_TOOLBAR_RADIUS))
                }
                FrostedHostKind::QuickAccessPicker => {
                    window.set_background_corner_radius(gpui::px(
                        crate::app::window::quick_access::palette::QUICK_ACCESS_RADIUS_CONTROL,
                    ))
                }
                FrostedHostKind::QuickAccessActions => {
                    window.set_background_corner_radius(gpui::px(
                        crate::app::window::quick_access::palette::QUICK_ACCESS_RADIUS_ACTIONS_MENU,
                    ))
                }
                FrostedHostKind::SidebarUsage => {
                    window.set_background_corner_radius(gpui::px(SIDEBAR_USAGE_HOST_RADIUS))
                }
            }
            attach_host_window(window, parent_view, kind);
            let observe = with_slot(kind, |slot| slot.observe.clone());
            cx.new(|cx| {
                let mut view = FrostedHostView {
                    kind,
                    _observe: None,
                };
                view.observe(observe, cx);
                view
            })
        },
    )
    .ok()
}

pub(crate) struct FrostedHostView {
    kind: FrostedHostKind,
    _observe: Option<Subscription>,
}

impl FrostedHostView {
    fn observe(
        &mut self,
        entity: Option<gpui::Entity<crate::GhostexGpuiApp>>,
        cx: &mut Context<Self>,
    ) {
        self._observe = entity.map(|entity| cx.observe(&entity, |_, _, cx| cx.notify()));
    }
}

impl Render for FrostedHostView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = with_slot(self.kind, |slot| slot.content.clone());
        let mut host = div().size_full();
        if let Some(content) = content {
            host = host.child(content(window, cx));
        }
        host
    }
}

fn native_view_of(window: AnyWindowHandle, cx: &mut App) -> Option<*mut std::ffi::c_void> {
    window
        .update(cx, |_, window, _| native_view(window))
        .ok()
        .flatten()
}

#[cfg(target_os = "macos")]
fn native_view(window: &Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
        _ => None,
    }
}

#[cfg(not(target_os = "macos"))]
fn native_view(_: &Window) -> Option<*mut std::ffi::c_void> {
    None
}

/// Attaches the host above its parent without ever taking key status from it (the same attachment
/// the composer's suggestions use), and lets a tooltip pass the mouse through.
#[cfg(target_os = "macos")]
fn attach_host_window(window: &mut Window, parent: *mut std::ffi::c_void, kind: FrostedHostKind) {
    let ignores_mouse = kind == FrostedHostKind::Tooltip;
    unsafe extern "C" {
        fn GhostexGpuiAttachComposerSuggestionsWindow(
            view: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
        );
        fn GhostexGpuiSetWindowIgnoresMouse(view: *mut std::ffi::c_void, ignores: bool);
        fn GhostexGpuiSetWindowIdentifier(
            view: *mut std::ffi::c_void,
            identifier: *const std::ffi::c_char,
        );
    }
    if let Some(view) = native_view(window) {
        unsafe {
            GhostexGpuiAttachComposerSuggestionsWindow(view, parent);
            // Set either way: left at its default, AppKit passes a click through any part of a
            // non-opaque window it sees as transparent, which is how clicks on the frosted menu's
            // rows fell through to the sidebar underneath.
            GhostexGpuiSetWindowIgnoresMouse(view, ignores_mouse);
            if matches!(kind, FrostedHostKind::SidebarMenu(_)) {
                // The sidebar's outside-click monitor must not read a press here as leaving the menu.
                GhostexGpuiSetWindowIdentifier(view, c"ghostex.frostedSidebarMenu".as_ptr());
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn attach_host_window(_: &mut Window, _: *mut std::ffi::c_void, _: FrostedHostKind) {}

#[cfg(target_os = "macos")]
fn set_visible(
    handle: AnyWindowHandle,
    parent: Option<AnyWindowHandle>,
    visible: bool,
    cx: &mut App,
) {
    unsafe extern "C" {
        fn GhostexGpuiSetFrostedChildWindowVisible(
            child: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
            visible: bool,
        );
    }
    let parent_view = parent.and_then(|parent| native_view_of(parent, cx));
    if let Some(view) = native_view_of(handle, cx) {
        unsafe {
            GhostexGpuiSetFrostedChildWindowVisible(
                view,
                parent_view.unwrap_or(std::ptr::null_mut()),
                visible,
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_visible(_: AnyWindowHandle, _: Option<AnyWindowHandle>, _: bool, _: &mut App) {}

/// Hands the main window's tooltips to the frosted tooltip host while glass is on, and takes them
/// back when it is off. Called from the main window's glass sync on every root render.
pub(crate) fn sync_frosted_tooltip_presenter(window: &mut Window, cx: &mut App) {
    let wanted = frosted_hosting_active();
    if window.tooltip_presenter_active() == wanted {
        return;
    }
    if !wanted {
        window.set_tooltip_presenter(None);
        hide_frosted_host(FrostedHostKind::Tooltip, cx);
        return;
    }
    let parent = window.window_handle();
    window.set_tooltip_presenter(Some(Rc::new(
        move |presentation, cx: &mut App| match presentation {
            Some((view, bounds)) => {
                let key = view.entity_id();
                show_frosted_host(
                    FrostedHostKind::Tooltip,
                    parent,
                    bounds,
                    Some(key),
                    Rc::new(move |_, _| div().size_full().child(view.clone()).into_any_element()),
                    None,
                    cx,
                );
            }
            None => hide_frosted_host(FrostedHostKind::Tooltip, cx),
        },
    )));
}
