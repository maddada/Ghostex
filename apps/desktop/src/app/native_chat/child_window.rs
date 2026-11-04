//! Moving the chat's open child windows after the pane they cover or anchor to moved.

use super::state::NativeChatView;
use gpui::{Bounds, Context, Pixels};

impl NativeChatView {
    pub(crate) fn active_child_window_source(&self, cx: &gpui::App) -> Option<gpui::WindowId> {
        let source = self.main_window?.window_id();
        let children = [
            self.image_viewer.handle.map(|handle| handle.window_id()),
            self.table_preview.handle.map(|handle| handle.window_id()),
            self.save_markdown_window
                .handle
                .map(|handle| handle.window_id()),
            self.rewind_window.handle.map(|handle| handle.window_id()),
            self.context_editor_window
                .handle
                .map(|handle| handle.window_id()),
            self.maximized_window.map(|handle| handle.window_id()),
            self.reply_popover_window(),
            self.draft_project_picker_window(),
        ];
        if let Some(menu_source) = self.active_option_menu_source(cx)
            && (menu_source == source || children.contains(&Some(menu_source)))
        {
            return Some(source);
        }
        children
            .contains(&Some(cx.active_window()?.window_id()))
            .then_some(source)
    }

    /// CDXC:SessionChat 2026-09-24 DECISION:
    /// User: nothing the chat pops up may stay on screen once the user switches to another session, and a modal the session still owns (the image preview, the rewind confirmation and its "could not be rewound" message, Save as Markdown, the context editor, the maximized composer) comes back when the user returns to that session, sized to the pane it is in by then. Menus, the `@` list and the floating pills are dismissed for good instead, because reopening a dropdown nobody asked for would be its own surprise. This supersedes the 2026-09-19 decision, which took the image preview and the model picker down without bringing them back.
    pub(crate) fn dismiss_windows_for_hidden_pane(&mut self, cx: &mut Context<Self>) {
        if self.pane_hidden {
            return;
        }
        self.pane_hidden = true;
        self.close_own_menu(cx);
        self.close_reply_popover(cx);
        self.close_draft_project_picker(cx);
        self.pending_model_menu = None;
        self.hide_frosted_overlays(cx);
        self.sync_pane_modal_windows(cx);
        self.schedule_hidden_image_trim(cx);
    }

    /// Reopens the modals the session still owns, on the pane's current frame.
    pub(crate) fn restore_windows_for_shown_pane(&mut self, cx: &mut Context<Self>) {
        if !self.pane_hidden {
            return;
        }
        self.pane_hidden = false;
        self.cancel_hidden_image_trim();
        self.sync_pane_modal_windows(cx);
    }

    fn sync_pane_modal_windows(&mut self, cx: &mut Context<Self>) {
        self.sync_owned_modal_windows(cx);
        self.sync_suggestion_window(cx);
    }

    fn sync_owned_modal_windows(&mut self, cx: &mut Context<Self>) {
        self.sync_image_viewer_window(cx);
        self.sync_table_preview_window(cx);
        self.sync_rewind_window(cx);
        self.sync_save_markdown_window(cx);
        self.sync_context_editor_window(cx);
        self.sync_maximized_window(cx);
    }

    /// The window the pane was last drawn in, forgotten once that window has closed.
    pub(super) fn open_main_window(&mut self, cx: &gpui::App) -> Option<gpui::AnyWindowHandle> {
        if self
            .main_window
            .is_some_and(|window| !cx.windows().contains(&window))
        {
            self.main_window = None;
        }
        self.main_window
    }

    /// CDXC:SessionChat 2026-09-25 WHY:
    /// The floating sessions panel draws the chat in a window of its own and closes it when it goes away, and the chat's owned modals are restored when the pane is next shown, which comes before the chat is drawn in the window that shows it now. Opening them then read the closed panel window's frame, and GPUI's "window not found" landed in the chat's error banner and stayed there. A modal owed to a pane whose window is gone therefore waits for the pane's next draw, which names the window it belongs to, and opens once that draw has measured the pane there.
    pub(super) fn note_drawn_in(&mut self, window: &gpui::Window, cx: &mut Context<Self>) {
        self.drawn_native_view = window_native_view(window);
        self.drawn_window_size = window.viewport_size();
        let window = window.window_handle();
        if self.main_window.replace(window) == Some(window) || self.pane_hidden {
            return;
        }
        let chat = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = chat.update(cx, |chat, cx| {
                chat.hide_frosted_overlays(cx);
                chat.close_suggestion_window(cx);
                chat.sync_owned_modal_windows(cx);
                cx.notify();
            });
        });
    }

    /// CDXC:SessionChat 2026-09-26 WHY:
    /// The chat's popups (the `$`, `@` and `/` list, the scroll-to-bottom pill, menus and pane modals) take their frames in the coordinates of the window the chat is drawn in, and moving one in place measured those frames against the parent it is attached to. That parent was always the main window, so while the chat was drawn in the floating sessions panel every moved popup landed the panel's offset away, over the sidebar. They attach to, and are placed in, the window that draws the chat, and the ones already open are closed when the chat moves to another window so its next paint reopens them there.
    pub(in crate::app::native_chat) fn child_window_parent(
        &self,
        cx: &gpui::App,
    ) -> *mut std::ffi::c_void {
        // The floating panel's window is closed when the panel goes away; its view is never reused.
        self.drawn_native_view
            .filter(|_| {
                self.main_window
                    .is_some_and(|window| cx.windows().contains(&window))
            })
            .unwrap_or(self.config.parent_native_view)
    }

    /// CDXC:SessionChat 2026-10-01 DECISION:
    /// User: "When I click expand for a table or a diagram, or any of the things we expand out, we should show it large, outside the chat view itself, not constrained inside it." The table preview and the image viewer open over the whole window the chat is drawn in, not over the chat pane; the Mermaid viewer already opens as an app modal centred on the main window (mermaid_diagram_modal_lifecycle.rs). Supersedes the pane-sized frames of 2026-09-18 (image viewer) and 2026-09-24 (table preview).
    pub(in crate::app::native_chat) fn expanded_area(&self) -> Bounds<Pixels> {
        let size = self.drawn_window_size;
        if size.width <= gpui::px(0.0) || size.height <= gpui::px(0.0) {
            return self.bounds.get();
        }
        Bounds::new(gpui::Point::default(), size)
    }

    pub(super) fn pane_windows_open(&self) -> bool {
        self.image_viewer.handle.is_some()
            || self.table_preview.handle.is_some()
            || self.save_markdown_window.handle.is_some()
            || self.rewind_window.handle.is_some()
            || self.context_editor_window.handle.is_some()
            || self.maximized_window.is_some()
    }

    /// CDXC:SessionChat 2026-09-24 DECISION:
    /// User: a modal that is still open when the pane is resized or moved to another pane is resized with it. An anchored menu cannot follow that way (its trigger moved under it), so it closes instead, which is what the 2026-09-17 model-picker rule did for the picker the model pop-up replaced.
    pub(super) fn pane_frame_changed(&mut self, cx: &mut Context<Self>) {
        // A menu whose first panel is still opening was asked for by this very frame.
        if self
            .option_menu
            .as_ref()
            .is_some_and(|menu| !menu.read(cx).is_opening())
        {
            self.close_own_menu(cx);
        }
        self.follow_pane_windows(cx);
    }

    /// Closes the open menu, unless it is anchored to another surface and this view is only hosting it.
    fn close_own_menu(&mut self, cx: &mut Context<Self>) {
        if self
            .option_menu
            .as_ref()
            .is_some_and(|menu| menu.read(cx).is_outside_pane())
        {
            return;
        }
        if let Some(menu) = self.option_menu.take() {
            menu.update(cx, |menu, cx| menu.close_with_focus(None, false, cx));
            cx.notify();
        }
    }

    /// Keep every pane-covering child window on the pane after the pane was resized or moved;
    /// React draws these as overlays inside the pane, so they follow it for free.
    pub(super) fn follow_pane_windows(&mut self, cx: &mut Context<Self>) {
        let pane = self.bounds.get();
        let expanded = self.expanded_area();
        let parent = self.child_window_parent(cx);
        let scale = super::appearance::ChatAppearance::current(&self.snapshot).scale;
        let windows: [Option<(gpui::AnyWindowHandle, Bounds<Pixels>)>; 6] = [
            self.image_viewer
                .handle
                .map(|handle| (handle.into(), expanded)),
            self.table_preview.handle.map(|handle| {
                (
                    handle.into(),
                    super::table_preview::table_preview_frame(expanded),
                )
            }),
            self.save_markdown_window
                .handle
                .map(|handle| (handle.into(), pane)),
            self.rewind_window
                .handle
                .map(|handle| (handle.into(), pane)),
            self.context_editor_window.handle.map(|handle| {
                (
                    handle.into(),
                    super::context_editor::context_editor_frame(pane, scale),
                )
            }),
            self.maximized_window.map(|handle| (handle.into(), pane)),
        ];
        for (handle, frame) in windows.into_iter().flatten() {
            if !move_child_window(handle, parent, frame, cx) {
                // GPUI has no cross-platform way to move a window, so elsewhere it keeps its
                // origin and only takes the pane's size.
                let _ = handle.update(cx, |_, window, _| window.resize(frame.size));
            }
        }
    }
}

/// Whether `bounds` lies wholly inside the current content mask, so a floating control's own window
/// can stand in for it without poking out of its pane.
///
/// CDXC:SessionChat 2026-10-04 WHY:
/// This used to be `mask.intersect(&bounds) == bounds`. At a fractional display scale (Windows at 125% or 150%), layout lands on thirds and fifths of a pixel, and `intersect` rebuilds the size as `(origin + size) - origin`, which in f32 often differs from `size` in the last bit. The test then failed on most frames, so under window glass the "Scroll to bottom" pill and the Escape and "Agent was interrupted" toasts never opened their window and did not show at all. At 2x (macOS) the values are exact. The edges are now compared with a hundredth-of-a-pixel tolerance.
pub(crate) fn bounds_inside_content_mask(window: &gpui::Window, bounds: Bounds<Pixels>) -> bool {
    let mask = window.content_mask().bounds;
    let slack = gpui::px(0.01);
    bounds.left() >= mask.left() - slack
        && bounds.top() >= mask.top() - slack
        && bounds.right() <= mask.right() + slack
        && bounds.bottom() <= mask.bottom() + slack
}

/// The chat window's content area in screen coordinates, which is what element bounds and child
/// window frames are measured from. Chat Lab's regular macOS titlebar sits outside it; the app's
/// own windows draw under their titlebar, so there the two are the same.
pub(crate) fn content_bounds(window: &gpui::Window) -> Bounds<Pixels> {
    let bounds = window.bounds();
    #[cfg(target_os = "macos")]
    let bounds = Bounds::from_corners(
        bounds.origin
            + gpui::point(
                gpui::px(0.0),
                (bounds.size.height - window.viewport_size().height).max(gpui::px(0.0)),
            ),
        bounds.bottom_right(),
    );
    bounds
}

/// The window's root native view (its HWND on Windows), which the chat's child windows attach to
/// and are moved against. Elsewhere they keep the parent the chat was created with.
#[cfg(target_os = "macos")]
pub(crate) fn window_native_view(window: &gpui::Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn window_native_view(window: &gpui::Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn window_native_view(_: &gpui::Window) -> Option<*mut std::ffi::c_void> {
    None
}

/// Move an open child window to `frame`, given in the chat window's content coordinates. False
/// when the window is gone or this platform cannot move it.
///
/// CDXC:SessionChat 2026-09-19 WHY:
/// AppKit reports the new size to GPUI synchronously from `setFrame:`, and GPUI drops that report while the app is borrowed, which left a child window moved from inside an update painting at its old size inside its new frame. The frame is therefore set from a task that runs outside any app update, the way GPUI's own `Window::resize` does.
#[cfg(target_os = "macos")]
pub(crate) fn move_child_window(
    handle: gpui::AnyWindowHandle,
    parent: *mut std::ffi::c_void,
    frame: Bounds<Pixels>,
    cx: &mut gpui::App,
) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    unsafe extern "C" {
        fn GhostexGpuiSetChildWindowContentFrame(
            child_native_view: *mut std::ffi::c_void,
            main_native_view: *mut std::ffi::c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        );
    }
    fn native_view(window: &gpui::Window) -> Option<*mut std::ffi::c_void> {
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
            _ => None,
        }
    }
    if !matches!(
        handle.update(cx, |_, window, _| native_view(window)),
        Ok(Some(_))
    ) {
        return false;
    }
    cx.spawn(async move |cx| {
        let Ok(Some(view)) = handle.update(cx, |_, window, _| native_view(window)) else {
            return;
        };
        unsafe {
            GhostexGpuiSetChildWindowContentFrame(
                view,
                parent,
                f64::from(frame.origin.x.as_f32()),
                f64::from(frame.origin.y.as_f32()),
                f64::from(frame.size.width.as_f32()),
                f64::from(frame.size.height.as_f32()),
            );
        }
    })
    .detach();
    true
}

/// CDXC:PlatformSupport 2026-09-24 WHY:
/// Floating X11 windows are still independent frames; owner-relative placement keeps previews and editors aligned when their chat pane moves. The backend uses the explicit creation-time owner rather than keyboard focus.
#[cfg(target_os = "linux")]
pub(crate) fn move_child_window(
    handle: gpui::AnyWindowHandle,
    _: *mut std::ffi::c_void,
    frame: Bounds<Pixels>,
    cx: &mut gpui::App,
) -> bool {
    handle
        .update(cx, |_, window, _| window.set_x11_frame_in_parent(frame))
        .unwrap_or(false)
}

/// CDXC:PlatformSupport 2026-10-22 WHY:
/// This used to answer "cannot move" on Windows, and every caller then closed the window and opened it again (a chat menu that changed height, the `$`/`@`/`/` list, the Docs note composer and format bar, the "Scroll to bottom" pill) or only resized it where it stood (the chat's pane dialogs). Removing an active pop-up handed activation to another app, so clicking the Threads menu's "N more" closed the menu and brought another app's window in front of Ghostex. `parent` is the HWND of the window `frame` is measured in (`window_native_view`), and the move runs from a task: `SetWindowPos` delivers `WM_SIZE` at once and GPUI's resize callback needs the app free (CDXC:ContextMenus 2026-10-02 in frosted_host.rs).
#[cfg(target_os = "windows")]
pub(crate) fn move_child_window(
    handle: gpui::AnyWindowHandle,
    parent: *mut std::ffi::c_void,
    frame: Bounds<Pixels>,
    cx: &mut gpui::App,
) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };
    if parent.is_null()
        || !matches!(
            handle.update(cx, |_, window, _| window_native_view(window)),
            Ok(Some(_))
        )
    {
        return false;
    }
    cx.spawn(async move |cx| {
        let parent_origin = cx.update(|cx| {
            cx.windows().into_iter().find_map(|window| {
                window
                    .update(cx, |_, window, _| {
                        (window_native_view(window) == Some(parent))
                            .then(|| content_bounds(window).origin)
                    })
                    .ok()
                    .flatten()
            })
        });
        let Some(origin) = parent_origin else {
            return;
        };
        let Ok(Some((hwnd, scale))) = handle.update(cx, |_, window, _| {
            window_native_view(window).map(|hwnd| (hwnd, window.scale_factor()))
        }) else {
            return;
        };
        let screen = Bounds::new(origin + frame.origin, frame.size);
        let device = |value: Pixels| (f32::from(value) * scale).round() as i32;
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                device(screen.origin.x),
                device(screen.origin.y),
                device(screen.size.width),
                device(screen.size.height),
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            );
        }
    })
    .detach();
    true
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn move_child_window(
    _: gpui::AnyWindowHandle,
    _: *mut std::ffi::c_void,
    _: Bounds<Pixels>,
    _: &mut gpui::App,
) -> bool {
    false
}

/// CDXC:Theming 2026-09-27 DECISION:
/// User: "yes pls make all those that could be glass glass". Under window glass the chat's pane-sized dialogs (Save as Markdown, Rewind and its "could not be rewound" message, the image preview) are frosted: their window blurs the chat behind it instead of laying a dark scrim over it, and a dialog card takes the chat's frosted menu surface. Glass off, they keep the solid card and the dark scrim.
pub(super) fn pane_dialog_window_background(glass: bool) -> gpui::WindowBackgroundAppearance {
    if glass {
        gpui::WindowBackgroundAppearance::Blurred
    } else {
        gpui::WindowBackgroundAppearance::Transparent
    }
}

/// Gives a pane dialog's blurred window the frosted menus' blur (see `pane_dialog_window_background`).
pub(super) fn prepare_pane_dialog_window(window: &gpui::Window, glass: bool) {
    if glass {
        crate::app::helpers::apply_frosted_menu_blur(window);
    }
}

/// The dim a pane dialog lays over the chat: `opaque` over the solid chat, a light one over the
/// frosted pane (the blur already sets the dialog apart).
pub(super) fn pane_dialog_scrim(glass: bool, light: bool, opaque: f32) -> gpui::Hsla {
    if !glass {
        return gpui::black().opacity(opaque);
    }
    if light {
        gpui::black().opacity(0.04)
    } else {
        gpui::black().opacity(0.18)
    }
}
