//! The formatting bar's frosted window under window glass.
//!
//! CDXC:Docs 2026-09-25 DECISION:
//! User: "can we make this bottom bar also glassy like the scroll to bottom pill in the gpui chat view". Under window glass the Docs formatting bar's row is drawn in a small blurred child window of its own, tinted and bordered like the chat's scroll-to-bottom pill (`native_chat/frosted_overlay_window.rs`), because GPUI can blur only behind a whole window. The Docs view keeps an invisible twin of the row that holds its place, reports where it is, and anchors the menus and the find panel that open above it. The window never takes the keyboard, so the editor keeps it while the bar is clicked. With glass off the bar is drawn in the view as before.

use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, IntoElement, ParentElement as _, Pixels, Render,
    Styled as _, Subscription, WeakEntity, Window, WindowBounds, WindowOptions, div, px,
};
use gpui_component::Root;

use super::palette::DocsPalette;
use crate::GhostexGpuiApp;
use crate::app::native_chat::appearance::ChatAppearance;

/// The bar's corner radius, which its window's blur takes too.
const BAR_RADIUS: f32 = 10.0;

/// Whether the bar is drawn in its frosted window: under window glass, where the backend can blur
/// a child window.
pub(crate) fn format_bar_frosted(p: &DocsPalette) -> bool {
    cfg!(target_os = "macos") && p.glass
}

/// The frosted window, and the frame the Docs view last asked it to take.
#[derive(Default)]
pub(crate) struct DocsFormatBarWindow {
    handle: Option<gpui::WindowHandle<Root>>,
    /// The open window's frame, in the main window's content coordinates.
    frame: Option<Bounds<Pixels>>,
    /// The frame the placeholder last asked for; the latest request wins while a window opens.
    wanted: Option<Bounds<Pixels>>,
    busy: bool,
    /// The placeholder's bounds at its last paint (`None` while it is covered or clipped).
    measured: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The placeholder painted since the main window's last frame began.
    painted: Rc<Cell<bool>>,
}

struct DocsFormatBarView {
    app: WeakEntity<GhostexGpuiApp>,
    _observe: Subscription,
}

impl Render for DocsFormatBarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        app.update(cx, |app, cx| {
            app.render_native_docs_format_bar_window(window, cx)
        })
    }
}

impl GhostexGpuiApp {
    /// The invisible reporter the placeholder carries: each paint hands its bounds (or `None`
    /// while something drawn in the view covers it, or it is clipped) to the frosted window.
    pub(crate) fn native_docs_format_bar_reporter(&self, cx: &Context<Self>) -> AnyElement {
        let state = &self.native_docs.format_bar_window;
        let (measured, painted) = (state.measured.clone(), state.painted.clone());
        // The notes list and the note composer are drawn in the view and would sit under the
        // bar's window, so it steps aside while they are open.
        let covered = self.native_docs.composer.is_some() || self.native_docs.notes_list_open;
        let app = cx.weak_entity();
        gpui::canvas(
            move |bounds, window, cx| {
                painted.set(true);
                let control = (!covered
                    && window.content_mask().bounds.intersect(&bounds) == bounds)
                    .then_some(bounds);
                if measured.replace(control) != control {
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.native_docs_place_format_bar_window(control, cx);
                        });
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    /// Where the bar's frosted window sits in the main window, when `window` is it.
    pub(crate) fn native_docs_format_bar_window_origin(
        &self,
        window: &Window,
    ) -> Option<gpui::Point<Pixels>> {
        let state = &self.native_docs.format_bar_window;
        state
            .handle
            .filter(|handle| handle.window_id() == window.window_handle().window_id())
            .and(state.frame)
            .map(|frame| frame.origin)
    }

    /// Runs as the main window's frame begins: a placeholder that did not paint last frame (another
    /// view, another kind of file, glass turned off) takes its window down.
    pub(crate) fn native_docs_drop_unseen_format_bar(&mut self, cx: &mut Context<Self>) {
        let state = &self.native_docs.format_bar_window;
        if state.painted.replace(false) || (state.handle.is_none() && state.wanted.is_none()) {
            return;
        }
        state.measured.set(None);
        self.native_docs_place_format_bar_window(None, cx);
    }

    fn native_docs_place_format_bar_window(
        &mut self,
        control: Option<Bounds<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.native_docs.format_bar_window;
        state.wanted = control;
        if state.busy || state.frame == state.wanted {
            return;
        }
        state.busy = true;
        let app = cx.entity();
        cx.defer(move |cx| apply_format_bar_window(app, cx));
    }

    fn render_native_docs_format_bar_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The window is as small as the bar, so its tooltips go to the frosted tooltip windows.
        crate::app::window::frosted_host::sync_frosted_tooltip_presenter(window, cx);
        let Some(p) = self.native_docs.palette.clone() else {
            return div().into_any_element();
        };
        let pill = ChatAppearance::current(&serde_json::Value::Null).on_window_glass(true);
        let frosted = Some((pill.composer_background, pill.composer_border));
        let Some(row) = self.native_docs_format_bar_row(&p, frosted, cx) else {
            return div().into_any_element();
        };
        div().size_full().child(row).into_any_element()
    }
}

fn apply_format_bar_window(app: gpui::Entity<GhostexGpuiApp>, cx: &mut gpui::App) {
    let (wanted, handle, parent, main) = app.update(cx, |app, _| {
        let state = &mut app.native_docs.format_bar_window;
        (
            state.wanted,
            state.handle.take(),
            app.parent_ns_view,
            app.main_window_handle,
        )
    });
    // An open bar follows its place (a resize, a collapse) instead of being closed and reopened.
    if let (Some(frame), Some(handle)) = (wanted, handle)
        && crate::app::native_chat::child_window::move_child_window(
            handle.into(),
            parent,
            frame,
            cx,
        )
    {
        finish(&app, Some(handle), wanted, cx);
        return;
    }
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    let placement = wanted.and_then(|frame| {
        main?
            .update(cx, |_, window, cx| {
                (
                    crate::app::native_chat::child_window::content_bounds(window).origin,
                    window.display(cx).map(|display| display.id()),
                )
            })
            .ok()
            .map(|(origin, display_id)| (frame, origin, display_id))
    });
    let Some((frame, origin, display_id)) = placement else {
        finish(&app, None, None, cx);
        return;
    };
    let screen = Bounds::new(origin + frame.origin, frame.size);
    let observed = app.clone();
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(screen)),
            display_id: crate::app::window::popup_frame::display_at(screen.center(), cx)
                .or(display_id),
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
            window.set_background_corner_radius(px(BAR_RADIUS));
            attach_bar_window(window, parent);
            let view = cx.new(|cx| DocsFormatBarView {
                app: observed.downgrade(),
                _observe: cx.observe(&observed, |_, _, cx| cx.notify()),
            });
            cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
        },
    );
    match result {
        Ok(handle) => finish(&app, Some(handle), Some(frame), cx),
        Err(_) => finish(&app, None, None, cx),
    }
}

/// Records what is on screen now and catches up with a request that arrived meanwhile.
fn finish(
    app: &gpui::Entity<GhostexGpuiApp>,
    handle: Option<gpui::WindowHandle<Root>>,
    frame: Option<Bounds<Pixels>>,
    cx: &mut gpui::App,
) {
    app.update(cx, |app, cx| {
        let state = &mut app.native_docs.format_bar_window;
        state.handle = handle;
        state.frame = frame;
        state.busy = false;
        if state.frame != state.wanted {
            state.busy = true;
            let app = cx.entity();
            cx.defer(move |cx| apply_format_bar_window(app, cx));
        }
    });
}

/// Attaches the bar above the main window without ever taking key status from it (the chat's
/// floating controls attach the same way), so typing stays in the editor.
#[cfg(target_os = "macos")]
fn attach_bar_window(window: &mut Window, parent: *mut std::ffi::c_void) {
    unsafe extern "C" {
        fn GhostexGpuiAttachComposerSuggestionsWindow(
            view: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
        );
    }
    if let Ok(view) = crate::app::helpers::cef_parent_native_view(window) {
        unsafe { GhostexGpuiAttachComposerSuggestionsWindow(view, parent) };
    }
}

#[cfg(not(target_os = "macos"))]
fn attach_bar_window(_: &mut Window, _: *mut std::ffi::c_void) {}
