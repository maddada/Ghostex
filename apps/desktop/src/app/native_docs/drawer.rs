//! The floating files list (a drawer or a peek) in a native child window of its own, laid over the
//! right edge of the Docs view.
//!
//! CDXC:Docs 2026-09-25 DECISION:
//! User: "make the sidebar for the files list work like the sessions sidebar. it has glass effect and it's able to appear on top of cef panes without any issue". The floating list is drawn in a child window over the Docs view, exactly as the floating sessions sidebar is, so it shows over an HTML file or a drawing without hiding the page, and under glass its backdrop is the main window's glass picture (`sync_overlay_window_glass`) with the sidebar's own tint over it. Docked, the list stays in the main window beside the document. On macOS AppKit slides the window (`native/macos/GpuiDocsDrawer.m`); elsewhere it opens and closes in one step.

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    WeakEntity, Window, WindowBackgroundAppearance, div, point, px, size,
};

use super::render::SIDEBAR_WIDTH;
use crate::GhostexGpuiApp;
use crate::app::floating_reveal::model::FLOATING_PANEL_CORNER_RADIUS;
use crate::app::helpers::{
    cef_parent_native_view, set_docs_drawer_glass_window, sync_overlay_window_glass,
    window_glass_active,
};

/// The drawer's window, kept between uses once opened.
pub(crate) struct DocsDrawerHost {
    pub(crate) window: gpui::WindowHandle<gpui_component::Root>,
    /// The window's GPUI view, which AppKit slides.
    native_view: *mut std::ffi::c_void,
    /// The list is out (or sliding out); false while it is hidden or sliding away.
    shown: bool,
}

/// The drawer window's root. It borrows the app entity and draws the same files list the docked
/// layout draws.
pub(crate) struct DocsDrawerWindow {
    app: WeakEntity<GhostexGpuiApp>,
    _subscription: Subscription,
}

impl Render for DocsDrawerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        app.update(cx, |app, cx| app.render_native_docs_drawer(window, cx))
    }
}

impl GhostexGpuiApp {
    /// The window this Docs element is drawn in is the drawer's.
    pub(crate) fn native_docs_in_drawer(&self, window: &Window) -> bool {
        self.native_docs
            .drawer
            .as_ref()
            .is_some_and(|drawer| drawer.window.window_id() == window.window_handle().window_id())
    }

    /// Runs `f` against the main window when `window` is another one (the files list's drawer or
    /// the formatting bar's frosted window): a prompt belongs over the app, and the editor and the
    /// find field take focus in the window that draws them. False when `window` is already the
    /// main window and the caller goes on itself.
    pub(crate) fn native_docs_defer_to_main_window(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        let Some(main) = self.main_window_handle else {
            return false;
        };
        if main.window_id() == window.window_handle().window_id() {
            return false;
        }
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                app.update(cx, |this, cx| f(this, window, cx));
            });
        });
        true
    }

    /// Where one of Docs' child windows (the drawer, the formatting bar's frosted window) sits in
    /// the main window's content coordinates; `None` for any other window.
    pub(crate) fn native_docs_child_window_origin(
        &self,
        window: &Window,
    ) -> Option<gpui::Point<Pixels>> {
        if self.native_docs_in_drawer(window) {
            return Some(
                self.native_docs
                    .drawer_frame
                    .map_or(point(px(0.0), px(0.0)), |frame| frame.origin),
            );
        }
        self.native_docs_format_bar_window_origin(window)
    }

    /// Focuses a field of the files list in whichever window draws it. A floating list is in the
    /// drawer's window, which takes the keyboard and focuses the field when it next draws.
    pub(crate) fn native_docs_focus_list_input(
        &mut self,
        input: &gpui::Entity<gpui_component::input::InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.native_docs_sidebar_layout().overlay && !self.native_docs_in_drawer(window) {
            self.native_docs.drawer_focus = Some(input.clone());
            self.native_docs_notify(cx);
            return;
        }
        super::actions::focus_and_select_all(input, window);
    }

    /// Shows, moves or hides the drawer window to match the list's state. Runs in the main
    /// window's render, after the layout that decided whether the list floats.
    pub(crate) fn native_docs_sync_drawer(
        &mut self,
        floating: bool,
        slide_away: bool,
        view: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !floating || view.size.width <= px(0.0) {
            self.native_docs.drawer_focus = None;
            self.native_docs_hide_drawer_window(slide_away, cx);
            return;
        }
        self.native_docs.drawer_synced = true;
        let width = SIDEBAR_WIDTH.min(view.size.width.as_f32());
        let frame = Bounds::new(
            point(view.right() - px(width), view.top()),
            size(px(width), view.size.height),
        );
        let moved = self.native_docs.drawer_frame != Some(frame);
        self.native_docs.drawer_frame = Some(frame);
        let parent = self.parent_ns_view;
        let Some(drawer) = self.native_docs.drawer.as_mut() else {
            if !self.native_docs.drawer_opening {
                self.native_docs_open_drawer_window(cx);
            }
            return;
        };
        if drawer.shown && !moved && drawer_window_visible(drawer.native_view) {
            return;
        }
        drawer.shown = true;
        let (handle, native_view) = (drawer.window, drawer.native_view);
        let slide = Self::native_docs_slide_duration().as_secs_f64();
        self.native_docs_watch_outside_clicks(cx);
        spawn_show_drawer_window(handle, native_view, parent, frame, slide, cx);
    }

    /// Whether the drawer's window is out, or on its way out.
    pub(crate) fn native_docs_drawer_shown(&self) -> bool {
        self.native_docs
            .drawer
            .as_ref()
            .is_some_and(|drawer| drawer.shown)
    }

    /// Takes the drawer down: sliding away, or at once when the docked list has just taken its
    /// place.
    fn native_docs_hide_drawer_window(&mut self, slide_away: bool, cx: &mut Context<Self>) {
        let Some(drawer) = self.native_docs.drawer.as_mut() else {
            return;
        };
        if !drawer.shown {
            return;
        }
        drawer.shown = false;
        let (handle, native_view) = (drawer.window, drawer.native_view);
        let slide = if slide_away {
            Self::native_docs_slide_duration().as_secs_f64()
        } else {
            0.0
        };
        #[cfg(not(target_os = "macos"))]
        {
            self.native_docs.drawer = None;
        }
        cx.defer(move |cx| hide_drawer_window(handle, native_view, slide, cx));
    }

    /// Runs as the main window's frame begins: a drawer the Docs view did not draw last frame
    /// belongs to a view that went away (another view, a closed panel), so it goes too.
    pub(crate) fn native_docs_drop_unseen_drawer(&mut self, cx: &mut Context<Self>) {
        let seen = std::mem::replace(&mut self.native_docs.drawer_synced, false);
        if seen
            || !self
                .native_docs
                .drawer
                .as_ref()
                .is_some_and(|drawer| drawer.shown)
        {
            return;
        }
        self.native_docs.transient = None;
        self.native_docs.slide = None;
        self.native_docs.peek_timer = None;
        self.native_docs.drawer_focus = None;
        self.native_docs_hide_drawer_window(true, cx);
    }

    /// Opening a window draws its root at once, and the root updates this entity, so the window is
    /// opened on a deferred task outside this update (the floating reveal panel does the same).
    fn native_docs_open_drawer_window(&mut self, cx: &mut Context<Self>) {
        let Some(main) = self.main_window_handle else {
            return;
        };
        self.native_docs.drawer_opening = true;
        let parent = self.parent_ns_view;
        let app = cx.weak_entity();
        gpui::App::defer(cx, move |cx| {
            let Some(app) = app.upgrade() else {
                return;
            };
            let finish = |app: &gpui::Entity<GhostexGpuiApp>, cx: &mut gpui::App| {
                app.update(cx, |app, _| app.native_docs.drawer_opening = false);
            };
            let Some(frame) = app.read(cx).native_docs.drawer_frame else {
                finish(&app, cx);
                return;
            };
            let Ok((origin, display_id)) = main.update(cx, |_, window, cx| {
                (
                    crate::app::native_chat::child_window::content_bounds(window).origin,
                    window.display(cx).map(|display| display.id()),
                )
            }) else {
                finish(&app, cx);
                return;
            };
            let screen = Bounds::new(origin + frame.origin, frame.size);
            let options = gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(screen)),
                display_id: crate::app::window::popup_frame::display_at(screen.center(), cx)
                    .or(display_id),
                focus: false,
                // AppKit orders the window in as it slides it out; elsewhere the platform shows it
                // as it opens.
                show: !cfg!(target_os = "macos"),
                kind: gpui::WindowKind::PopUp,
                // Clear outside its rounded corners when the window is not glass.
                window_background: if window_glass_active() {
                    WindowBackgroundAppearance::Blurred
                } else {
                    WindowBackgroundAppearance::Transparent
                },
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                titlebar: None,
                app_id: crate::gpui_platform_window_app_id(),
                icon: crate::gpui_platform_window_icon(),
                ..Default::default()
            };
            let observed = app.clone();
            let result = cx.open_window(options, move |window, cx| {
                window.set_background_corner_radius(px(FLOATING_PANEL_CORNER_RADIUS));
                let view = cx.new(|cx| DocsDrawerWindow {
                    app: observed.downgrade(),
                    _subscription: cx.observe(&observed, |_, _, cx| cx.notify()),
                });
                cx.new(|cx| {
                    gpui_component::Root::new(view, window, cx).bg(gpui::transparent_black())
                })
            });
            let Ok(handle) = result else {
                finish(&app, cx);
                return;
            };
            let native_view = handle
                .update(cx, |_, window, _| cef_parent_native_view(window))
                .ok()
                .and_then(Result::ok);
            let Some(native_view) = native_view else {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
                finish(&app, cx);
                return;
            };
            round_floating_panel(native_view);
            set_docs_drawer_glass_window(Some(handle.into()));
            let slide = GhostexGpuiApp::native_docs_slide_duration().as_secs_f64();
            spawn_show_drawer_window(handle, native_view, parent, frame, slide, cx);
            app.update(cx, |app, cx| {
                app.native_docs.drawer_opening = false;
                app.native_docs.drawer = Some(DocsDrawerHost {
                    window: handle,
                    native_view,
                    shown: true,
                });
                app.native_docs_watch_outside_clicks(cx);
                // The list may have closed while the window was opening; the next sync hides it.
                cx.notify();
            });
        });
    }

    /// A click anywhere else in the main window closes a drawer or a peek, as a click on the
    /// document does. AppKit records it (a browser page under an HTML file takes its clicks
    /// natively, so no GPUI handler sees them) and this picks it up while the list is out.
    fn native_docs_watch_outside_clicks(&mut self, cx: &mut Context<Self>) {
        if !cfg!(target_os = "macos") || self.native_docs.drawer_click_watch {
            return;
        }
        self.native_docs.drawer_click_watch = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
                let watching = this.update(cx, |this, cx| {
                    let Some(native_view) = this
                        .native_docs
                        .drawer
                        .as_ref()
                        .filter(|drawer| drawer.shown)
                        .map(|drawer| drawer.native_view)
                    else {
                        this.native_docs.drawer_click_watch = false;
                        return false;
                    };
                    if take_outside_click(native_view) {
                        this.native_docs_close_transient(cx);
                    }
                    true
                });
                if !matches!(watching, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// The pointer left the drawer window or came back to it: a peek closes after the grace once
    /// the pointer is gone, wherever it went (a browser page under it reports nothing to Docs).
    fn native_docs_drawer_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.native_docs.transient == Some(super::state::DocsTransient::Peek) {
                self.native_docs.peek_timer = None;
            }
            return;
        }
        self.native_docs_leave_peek(cx);
    }

    fn render_native_docs_drawer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (Some(p), Some(frame)) = (
            self.native_docs.palette.clone(),
            self.native_docs.drawer_frame,
        ) else {
            return div().into_any_element();
        };
        sync_overlay_window_glass(window, point(-frame.origin.x, -frame.origin.y));
        if let Some(input) = self.native_docs.drawer_focus.take() {
            window.activate_window();
            super::actions::focus_and_select_all(&input, window);
        }
        let layout = self.native_docs_sidebar_layout();
        let list = self.render_native_docs_files_list(&p, layout, true, window, cx);
        // Laid out at the full width against the window's left edge, which is where AppKit pins
        // the drawn frame while the window slides.
        div()
            .id("native-docs-drawer")
            .size_full()
            .key_context("NativeDocs")
            .on_action(cx.listener(Self::handle_native_docs_action))
            .on_key_down(cx.listener(Self::native_docs_key_down))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.native_docs_drawer_hovered(*hovered, cx)
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(frame.size.width)
                    .child(list),
            )
            .into_any_element()
    }
}

/// Shows the drawer from a task of its own: AppKit reports the new size to GPUI from inside
/// `setFrame:`, and GPUI draws the first frame from inside `displayLayer:`, and it drops both while
/// the app is borrowed, as it is in any update. The window is marked dirty first so that frame has
/// the list to draw.
fn spawn_show_drawer_window(
    handle: gpui::WindowHandle<gpui_component::Root>,
    native_view: *mut std::ffi::c_void,
    parent: *mut std::ffi::c_void,
    frame: Bounds<Pixels>,
    slide_seconds: f64,
    cx: &mut gpui::App,
) {
    let (native_view, parent) = (native_view as usize, parent as usize);
    cx.spawn(async move |cx| {
        let _ = handle.update(cx, |_, window, _| window.refresh());
        show_drawer_window(
            handle,
            native_view as *mut std::ffi::c_void,
            parent as *mut std::ffi::c_void,
            frame,
            slide_seconds,
            cx,
        );
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn show_drawer_window(
    _: gpui::WindowHandle<gpui_component::Root>,
    native_view: *mut std::ffi::c_void,
    parent: *mut std::ffi::c_void,
    frame: Bounds<Pixels>,
    slide_seconds: f64,
    _: &mut gpui::AsyncApp,
) {
    unsafe extern "C" {
        fn GhostexGpuiDocsDrawerShow(
            drawer: *mut std::ffi::c_void,
            main: *mut std::ffi::c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            slide_seconds: f64,
        );
    }
    unsafe {
        GhostexGpuiDocsDrawerShow(
            native_view,
            parent,
            f64::from(frame.origin.x.as_f32()),
            f64::from(frame.origin.y.as_f32()),
            f64::from(frame.size.width.as_f32()),
            f64::from(frame.size.height.as_f32()),
            slide_seconds,
        );
    }
}

/// Without an AppKit slide the window is placed at its frame (where the platform can move a
/// window) and shown as it is.
#[cfg(not(target_os = "macos"))]
fn show_drawer_window(
    handle: gpui::WindowHandle<gpui_component::Root>,
    _: *mut std::ffi::c_void,
    parent: *mut std::ffi::c_void,
    frame: Bounds<Pixels>,
    _: f64,
    cx: &mut gpui::AsyncApp,
) {
    let _ = cx.update(|cx| {
        crate::app::native_chat::child_window::move_child_window(handle.into(), parent, frame, cx)
    });
}

#[cfg(target_os = "macos")]
fn hide_drawer_window(
    _: gpui::WindowHandle<gpui_component::Root>,
    native_view: *mut std::ffi::c_void,
    slide_seconds: f64,
    _: &mut gpui::App,
) {
    unsafe extern "C" {
        fn GhostexGpuiDocsDrawerHide(drawer: *mut std::ffi::c_void, slide_seconds: f64);
    }
    unsafe { GhostexGpuiDocsDrawerHide(native_view, slide_seconds) };
}

/// Without an AppKit slide there is nothing to keep the window for: it closes, and the next show
/// opens a new one.
#[cfg(not(target_os = "macos"))]
fn hide_drawer_window(
    handle: gpui::WindowHandle<gpui_component::Root>,
    _: *mut std::ffi::c_void,
    _: f64,
    cx: &mut gpui::App,
) {
    set_docs_drawer_glass_window(None);
    let _ = handle.update(cx, |_, window, _| window.remove_window());
}

/// Cuts the window's corners to the floating panels' radius (`GhostexGpuiRoundFloatingPanel`).
#[cfg(target_os = "macos")]
pub(crate) fn round_floating_panel(native_view: *mut std::ffi::c_void) {
    unsafe extern "C" {
        fn GhostexGpuiRoundFloatingPanel(view: *mut std::ffi::c_void, radius: f64);
    }
    unsafe { GhostexGpuiRoundFloatingPanel(native_view, f64::from(FLOATING_PANEL_CORNER_RADIUS)) };
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn round_floating_panel(_: *mut std::ffi::c_void) {}

/// Whether AppKit has the drawer on screen, which Docs cannot learn any other way when the system
/// orders it out.
#[cfg(target_os = "macos")]
fn drawer_window_visible(native_view: *mut std::ffi::c_void) -> bool {
    unsafe extern "C" {
        fn GhostexGpuiDocsDrawerVisible(drawer: *mut std::ffi::c_void) -> bool;
    }
    unsafe { GhostexGpuiDocsDrawerVisible(native_view) }
}

#[cfg(not(target_os = "macos"))]
fn drawer_window_visible(_: *mut std::ffi::c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
fn take_outside_click(native_view: *mut std::ffi::c_void) -> bool {
    unsafe extern "C" {
        fn GhostexGpuiDocsDrawerTakeOutsideClick(drawer: *mut std::ffi::c_void) -> bool;
    }
    unsafe { GhostexGpuiDocsDrawerTakeOutsideClick(native_view) }
}

#[cfg(not(target_os = "macos"))]
fn take_outside_click(_: *mut std::ffi::c_void) -> bool {
    false
}
