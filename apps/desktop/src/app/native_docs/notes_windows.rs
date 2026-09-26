//! The toolbar over selected text and the note composer under window glass, each in a frosted
//! window of its own.
//!
//! CDXC:Docs 2026-09-26 DECISION:
//! User: "pls make these elements in the docs use glass effect when the app has transparency enabled like we did for other similar elements", of the toolbar over selected text and the note composer. Under window glass (macOS) both are frosted like the app's menus and popovers: each draws in a blurred window of its own with the frosted menu fill and outline (`frosted_menu_fill`, `titlebar_popup_menu_border_color`), because GPUI can blur only behind a whole window. The toolbar uses the shared frosted host (`window/frosted_host.rs`) and never takes the keyboard, so the selection stays live while its buttons are clicked. The composer's window takes the keyboard for its text field and gives it back to the editor when the note is added or dismissed. With glass off both draw in the Docs view as before.

use std::rc::Rc;

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, IntoElement, Pixels, Render, Styled as _,
    Subscription, WeakEntity, Window, WindowBounds, WindowOptions, div, px,
};
use gpui_component::Root;

use crate::GhostexGpuiApp;
use crate::app::window::frosted_host::{
    FrostedHostKind, frosted_hosting_active, hide_frosted_host, show_frosted_host,
    sync_frosted_tooltip_presenter,
};

/// The composer's corner radius, which its window's blur takes too.
pub(crate) const COMPOSER_RADIUS: f32 = 10.0;

/// Whether the toolbar and the composer draw in frosted windows now: under window glass, where
/// the macOS backend can blur a child window.
pub(crate) fn notes_frosted() -> bool {
    frosted_hosting_active()
}

/// The toolbar's host and the composer's window, and the frames the Docs view last asked for.
#[derive(Default)]
pub(crate) struct DocsNotesWindows {
    /// The toolbar is up in its frosted host.
    toolbar_hosted: bool,
    /// The Docs view placed the toolbar and the composer since the main window's frame began.
    seen: bool,
    composer: Option<gpui::WindowHandle<Root>>,
    /// The open composer window's frame, in the main window's content coordinates.
    composer_frame: Option<Bounds<Pixels>>,
    /// The frame last asked for; the latest request wins while a window opens.
    composer_wanted: Option<Bounds<Pixels>>,
    composer_busy: bool,
}

struct DocsComposerWindowView {
    app: WeakEntity<GhostexGpuiApp>,
    _observe: Subscription,
}

impl Render for DocsComposerWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        app.update(cx, |app, cx| {
            app.render_native_docs_composer_window(window, cx)
        })
    }
}

impl GhostexGpuiApp {
    /// Shows the toolbar in its frosted host at `frame` (in the main window's coordinates), or
    /// takes it down with `None`. Runs on every draw of the Docs view.
    pub(crate) fn native_docs_sync_toolbar_host(
        &mut self,
        frame: Option<Bounds<Pixels>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.native_docs.notes_windows.seen = true;
        let Some(frame) = frame else {
            self.native_docs_hide_toolbar_host(cx);
            return;
        };
        self.native_docs.notes_windows.toolbar_hosted = true;
        let app = cx.weak_entity();
        show_frosted_host(
            FrostedHostKind::DocsSelectionToolbar,
            window.window_handle(),
            frame,
            Some(cx.entity_id()),
            Rc::new(move |window, cx| {
                // The window is as small as the toolbar, so its tooltips go to the frosted
                // tooltip window.
                sync_frosted_tooltip_presenter(window, cx);
                app.update(cx, |app, cx| app.render_native_docs_hosted_toolbar(cx))
                    .unwrap_or_else(|_| div().into_any_element())
            }),
            Some(cx.entity()),
            cx,
        );
    }

    fn native_docs_hide_toolbar_host(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.native_docs.notes_windows.toolbar_hosted) {
            hide_frosted_host(FrostedHostKind::DocsSelectionToolbar, cx);
        }
    }

    fn render_native_docs_hosted_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(p) = self.native_docs.palette.clone() else {
            return div().into_any_element();
        };
        self.native_docs_selection_toolbar_panel(&p, true, cx)
            .into_any_element()
    }

    /// Opens, moves or closes the composer's frosted window to match `frame` (in the main
    /// window's coordinates). Runs on every draw of the Docs view.
    pub(crate) fn native_docs_sync_composer_window(
        &mut self,
        frame: Option<Bounds<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        self.native_docs.notes_windows.seen = true;
        self.native_docs_place_composer_window(frame, cx);
    }

    fn native_docs_place_composer_window(
        &mut self,
        frame: Option<Bounds<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let windows = &mut self.native_docs.notes_windows;
        windows.composer_wanted = frame;
        if windows.composer_busy || windows.composer_frame == windows.composer_wanted {
            return;
        }
        windows.composer_busy = true;
        let app = cx.entity();
        cx.defer(move |cx| apply_composer_window(app, cx));
    }

    fn render_native_docs_composer_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Glass turned off: the Docs view draws the composer now and this window is closing, so it
        // must not take the text field back.
        let Some(p) = self.native_docs.palette.clone().filter(|_| notes_frosted()) else {
            return div().into_any_element();
        };
        self.native_docs_composer_panel(&p, true, window, cx)
            .map_or_else(|| div().into_any_element(), IntoElement::into_any_element)
    }

    /// Runs as the main window's frame begins: when that frame does not draw the Docs view
    /// (another view took its place), the toolbar and the composer's window go once it is drawn.
    pub(crate) fn native_docs_drop_unseen_notes_windows(&mut self, cx: &mut Context<Self>) {
        let windows = &mut self.native_docs.notes_windows;
        windows.seen = false;
        if !windows.toolbar_hosted
            && windows.composer.is_none()
            && windows.composer_wanted.is_none()
        {
            return;
        }
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                if !app.native_docs.notes_windows.seen {
                    app.native_docs_hide_toolbar_host(cx);
                    app.native_docs_place_composer_window(None, cx);
                }
            });
        });
    }
}

fn apply_composer_window(app: gpui::Entity<GhostexGpuiApp>, cx: &mut gpui::App) {
    let (wanted, handle, parent, main) = app.update(cx, |app, _| {
        let windows = &mut app.native_docs.notes_windows;
        (
            windows.composer_wanted,
            windows.composer.take(),
            app.parent_ns_view,
            app.main_window_handle,
        )
    });
    // An open composer follows its place (a resize) instead of being closed and reopened.
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
        // A composer closed by the user (the note added or dismissed) gives the keyboard back to
        // the editor.
        if wanted.is_none()
            && app.read(cx).native_docs.composer.is_none()
            && let Some(main) = main
        {
            let _ = main.update(cx, |_, window, cx| {
                window.activate_window();
                let editor = app
                    .read(cx)
                    .native_docs
                    .active_document()
                    .and_then(|document| document.live.clone());
                if let Some(editor) = editor {
                    editor.update(cx, |editor, cx| editor.focus(window, cx));
                }
            });
        }
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
        give_up(&app, wanted.is_some(), cx);
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
            kind: crate::app::window::popup_frame::child_window_kind(),
            focus: true,
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
            window.set_background_corner_radius(px(COMPOSER_RADIUS));
            crate::app::helpers::apply_frosted_menu_blur(window);
            crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
            // Kept above the main window and made key, so the text field takes the keyboard.
            crate::app::window::attach_gpui_app_modal_window_to_main_window(window, parent);
            let view = cx.new(|cx| DocsComposerWindowView {
                app: observed.downgrade(),
                _observe: cx.observe(&observed, |_, _, cx| cx.notify()),
            });
            cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
        },
    );
    match result {
        Ok(handle) => finish(&app, Some(handle), Some(frame), cx),
        Err(_) => give_up(&app, true, cx),
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
        let windows = &mut app.native_docs.notes_windows;
        windows.composer = handle;
        windows.composer_frame = frame;
        windows.composer_busy = false;
        if windows.composer_frame != windows.composer_wanted {
            windows.composer_busy = true;
            let app = cx.entity();
            cx.defer(move |cx| apply_composer_window(app, cx));
        }
    });
}

/// A window that could not open drops its request, so the next draw of the Docs view asks again
/// instead of this retrying in a loop.
fn give_up(app: &gpui::Entity<GhostexGpuiApp>, failed: bool, cx: &mut gpui::App) {
    if failed {
        app.update(cx, |app, _| {
            app.native_docs.notes_windows.composer_wanted = None;
        });
    }
    finish(app, None, None, cx);
}
