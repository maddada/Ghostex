//! The frame every native app modal window draws: the modal itself, plus a round close button
//! centred on the modal's top-right corner while the pointer is over the top of the modal.
//!
//! CDXC:AppModal 2026-10-04 DECISION:
//! User: "lets make it actually show on the top right corner when the user is at the top of the modal anywhere not just if near top right corner, so top 80px of top of the modal", then "can we make it appear ON the top right corner not inside the modal?". The X is centred on the modal's top-right corner, half outside the card, and appears while the pointer is anywhere in the modal's top 80px, full width (or on the X itself). This supersedes the 2026-10-02 "show the x only when hovering very close to the top right" rule; the rest of the 2026-10-02 and 2026-09-30 decisions stands: "the x needs to be ON the top right corner of the modals", and "let's not make the modals close when i click away anymore", for "all the big modals that appear center of the app". The 2026-09-30 decision superseded the 2026-09-27 click-away list (Rename Session, Rename Worktree, Delete Worktree closed when the main window became key again) and the focus-loss close of Quick Access, Browser History and the Markdown table and Mermaid viewers: no app modal closes on a click outside it now, except those the user asked on 2026-10-08 to close on click-away again: Quick Access (CDXC:AppModal 2026-10-08 in window/quick_access/window.rs), and every Settings page kind, Search by Prompt, Browser History, the Markdown table and Mermaid diagram viewers, the Git file diff, the Export transcript result, the Agents Hub, and (asked 2026-10-09) Rename Session and Rename Worktree (CDXC:AppModal 2026-10-09 on `close_app_modal_on_click_away` in window/popup_dismissal.rs). The button runs the modal's own Escape close (`ModalCornerClose::close_from_corner`), so it cancels exactly what Escape cancels.
//! SEE-ALSO: apps/desktop/src/app/native_app_modal_lifecycle.rs and apps/gpui-web/src/app/web_host/modals.rs (the two openers that put every native modal in this frame).
use super::native_modal_kit::*;
use super::popup_frame::PopupOwner;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyView, AnyWindowHandle, App, AppContext as _, Bounds, Context, DispatchPhase, Entity,
    InteractiveElement as _, IntoElement, MouseExitEvent, MouseMoveEvent, ParentElement as _,
    Render, Styled as _, Subscription, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, canvas, div, point, px, size,
};
use gpui_component::Root;
use std::rc::Rc;

/// Whether the corner close button is drawn: shared by the modal's frame, which reports the
/// pointer in its top band, and the button's own window, which reports the pointer on the button.
#[derive(Default)]
struct CornerCloseReveal {
    enabled: bool,
    pointer_in_band: bool,
    pointer_on_button: bool,
}

impl CornerCloseReveal {
    fn visible(&self) -> bool {
        self.enabled && (self.pointer_in_band || self.pointer_on_button)
    }
}

fn set_reveal(
    reveal: &Entity<CornerCloseReveal>,
    cx: &mut App,
    change: impl FnOnce(&mut CornerCloseReveal) -> &mut bool,
    value: bool,
) {
    reveal.update(cx, |reveal, cx| {
        let field = change(reveal);
        if *field != value {
            *field = value;
            cx.notify();
        }
    });
}

pub(crate) struct ModalWindowFrame {
    content: AnyView,
    dismiss: Rc<dyn Fn(&mut Window, &mut App)>,
    shows_corner_close: Rc<dyn Fn(&App) -> bool>,
    palette: ModalPalette,
    /// Created on the first render, which also opens the button's window.
    reveal: Option<Entity<CornerCloseReveal>>,
}

impl ModalWindowFrame {
    pub(crate) fn new<V: ModalCornerClose>(content: Entity<V>, palette: ModalPalette) -> Self {
        let dismiss_target = content.clone();
        let corner_close_target = content.clone();
        Self {
            content: content.into(),
            dismiss: Rc::new(move |window, cx| {
                dismiss_target.update(cx, |modal, cx| modal.close_from_corner(window, cx));
            }),
            shows_corner_close: Rc::new(move |cx| {
                corner_close_target.read(cx).shows_corner_close(cx)
            }),
            palette,
            reveal: None,
        }
    }

    /**
     * CDXC:AppModal 2026-10-04 WHY:
     * The X sits half outside the modal, and a native window draws nothing outside its own
     * frame, so the X is a small window of its own, owned by the modal's window (stacked above
     * it, moving with it) and centred on its corner. Growing the modal's window by a transparent
     * margin instead would have that margin swallow clicks meant for the main window behind it
     * on Windows, short of a hit-test override.
     */
    fn corner_close_reveal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<CornerCloseReveal> {
        if let Some(reveal) = &self.reveal {
            return reveal.clone();
        }
        let reveal = cx.new(|_| CornerCloseReveal::default());
        self.reveal = Some(reveal.clone());
        let modal = window.window_handle();
        let frame = cx.entity();
        let dismiss = self.dismiss.clone();
        let palette = self.palette;
        let button_reveal = reveal.clone();
        // The modal's window must exist (and on Windows be the active window) first.
        cx.defer(move |cx| {
            open_corner_close_window(modal, frame, button_reveal, dismiss, palette, cx);
        });
        reveal
    }

    /**
     * Tracks whether the pointer is in the modal's top band. Window-wide listeners rather than an
     * element or group hover: the modal's own controls there (text fields, occluding buttons)
     * would otherwise swallow the moves, and a canvas registers no hitbox, so nothing under it
     * loses input.
     *
     * CDXC:AppModal 2026-10-04 WHY:
     * The tracker must be pinned to the frame's top-left (`inset_0`). An absolute element with no
     * insets sits at its static position, after the modal's content, so `bounds` started at the
     * window's bottom edge, no move inside the window ever counted as near the top, and the X
     * never appeared on any platform. The window's mouse-exit event hides the X once the pointer
     * leaves through the top edge, unless it left onto the X itself.
     */
    fn top_band_tracker(reveal: Entity<CornerCloseReveal>) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let move_reveal = reveal.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let position = event.position;
                    let in_band = bounds.contains(&position)
                        && f32::from(position.y - bounds.top()) <= MODAL_CORNER_CLOSE_REVEAL_HEIGHT;
                    set_reveal(&move_reveal, cx, |r| &mut r.pointer_in_band, in_band);
                });
                let exit_reveal = reveal.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    set_reveal(&exit_reveal, cx, |r| &mut r.pointer_in_band, false);
                });
            },
        )
        .absolute()
        .inset_0()
    }
}

impl Render for ModalWindowFrame {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // An open modal follows the Menu blur slider (CDXC:Theming 2026-10-15 on
        // `apply_frosted_menu_blur`).
        #[cfg(target_os = "macos")]
        crate::app::helpers::apply_frosted_menu_blur(window);
        let show_close = (self.shows_corner_close)(cx);
        let reveal = self.corner_close_reveal(window, cx);
        set_reveal(&reveal, cx, |r| &mut r.enabled, show_close);
        div()
            .id("app-modal-window-frame")
            .relative()
            .size_full()
            .child(self.content.clone())
            .when(show_close, |this| {
                this.child(Self::top_band_tracker(reveal))
            })
    }
}

/// Opens the corner close button's window centred on the modal window's top-right corner.
fn open_corner_close_window(
    modal: AnyWindowHandle,
    frame: Entity<ModalWindowFrame>,
    reveal: Entity<CornerCloseReveal>,
    dismiss: Rc<dyn Fn(&mut Window, &mut App)>,
    palette: ModalPalette,
    cx: &mut App,
) {
    // CDXC:AppModal 2026-10-27 WHY:
    // A page's windows are canvases that the web platform always draws opaque (it ignores a transparent background), so the X's own 24px window showed as a black square on every app modal's corner in the GPUI web build. The page has no X there; its modals close with Escape, their buttons or a click away.
    if cfg!(target_arch = "wasm32") {
        return;
    }
    let Ok((modal_bounds, owner, modal_native)) = modal.update(cx, |_, window, cx| {
        (
            window.bounds(),
            PopupOwner::of(window, cx),
            native_window_ref(window),
        )
    }) else {
        return;
    };
    let half = MODAL_CORNER_CLOSE_SIZE / 2.0;
    let corner = modal_bounds.top_right();
    let screen = Bounds::new(
        point(corner.x - px(half), corner.y - px(half)),
        size(px(MODAL_CORNER_CLOSE_SIZE), px(MODAL_CORNER_CLOSE_SIZE)),
    );
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(screen)),
        display_id: owner.display_for(screen, cx),
        titlebar: None,
        kind: WindowKind::PopUp,
        // Clicking the X must not take keyboard focus from the modal.
        focus: false,
        show: true,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let _ = cx.open_window(options, move |window, cx| {
        attach_corner_close_window(window, modal_native);
        let view = cx.new(|cx| ModalCornerCloseButton {
            _subscriptions: vec![
                cx.observe(&reveal, |_, _, cx| cx.notify()),
                // The button's window goes with the modal's.
                cx.observe_release_in(&frame, window, |_, _, window, _| window.remove_window()),
            ],
            reveal,
            modal,
            dismiss,
            palette,
        });
        cx.new(|cx| {
            Root::new(view, window, cx)
                .bordered(false)
                .bg(gpui::transparent_black())
        })
    });
}

/// The round X in its own window (`open_corner_close_window`).
struct ModalCornerCloseButton {
    reveal: Entity<CornerCloseReveal>,
    modal: AnyWindowHandle,
    dismiss: Rc<dyn Fn(&mut Window, &mut App)>,
    palette: ModalPalette,
    _subscriptions: Vec<Subscription>,
}

impl ModalCornerCloseButton {
    /// Keeps the X shown while the pointer is on it, also where it overhangs the modal.
    fn pointer_tracker(reveal: Entity<CornerCloseReveal>) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let move_reveal = reveal.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let on_button = bounds.contains(&event.position);
                        set_reveal(&move_reveal, cx, |r| &mut r.pointer_on_button, on_button);
                    }
                });
                let exit_reveal = reveal.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        set_reveal(&exit_reveal, cx, |r| &mut r.pointer_on_button, false);
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }
}

impl Render for ModalCornerCloseButton {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.reveal.read(cx).visible();
        div()
            .relative()
            .size_full()
            .child(Self::pointer_tracker(self.reveal.clone()))
            .child(
                modal_corner_close_button(
                    &self.palette,
                    "app-modal-corner-close",
                    if visible {
                        ModalCornerCloseReveal::Shown
                    } else {
                        ModalCornerCloseReveal::Hidden
                    },
                    move |this: &mut Self, _window, cx| {
                        cx.stop_propagation();
                        if !this.reveal.read(cx).enabled {
                            return;
                        }
                        let modal = this.modal;
                        let dismiss = this.dismiss.clone();
                        // The modal closes from its own window, outside this one's event.
                        cx.defer(move |cx| {
                            let _ = modal.update(cx, |_, window, cx| dismiss(window, cx));
                        });
                    },
                    cx,
                )
                .size_full(),
            )
    }
}

/// The modal window's native handle, which the button's window is attached to.
#[derive(Clone, Copy)]
struct NativeWindowRef(Option<isize>);

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn native_window_ref(window: &Window) -> NativeWindowRef {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    NativeWindowRef(
        match HasWindowHandle::window_handle(window)
            .ok()
            .map(|handle| handle.as_raw())
        {
            #[cfg(target_os = "macos")]
            Some(RawWindowHandle::AppKit(handle)) => Some(handle.ns_view.as_ptr() as isize),
            #[cfg(target_os = "windows")]
            Some(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get()),
            _ => None,
        },
    )
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_window_ref(_: &Window) -> NativeWindowRef {
    NativeWindowRef(None)
}

/// Stacks the button's window above the modal's for as long as both are open, without ever
/// taking key status from it.
#[cfg(target_os = "macos")]
fn attach_corner_close_window(window: &mut Window, modal: NativeWindowRef) {
    unsafe extern "C" {
        fn GhostexGpuiAttachComposerSuggestionsWindow(
            view: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
        );
    }
    let (NativeWindowRef(Some(parent)), NativeWindowRef(Some(view))) =
        (modal, native_window_ref(window))
    else {
        return;
    };
    unsafe {
        GhostexGpuiAttachComposerSuggestionsWindow(
            view as *mut std::ffi::c_void,
            parent as *mut std::ffi::c_void,
        )
    };
}

/// Makes the modal's window the button window's owner, so Windows keeps it above the modal and
/// minimizes and closes it with it, and drops the rounded corners and border DWM would draw
/// around its square frame.
///
/// CDXC:AppModal 2026-10-04 WHY:
/// The button window must never take activation: GPUI answers WM_MOUSEACTIVATE with MA_ACTIVATE
/// despite `focus: false`, so a click activated it, and closing the modal then destroyed the
/// active window, which flashed the main window hidden for a moment (Escape did not). It is made
/// non-activating like the titlebar dropdowns, so the modal stays active and closes as on Escape.
#[cfg(target_os = "windows")]
fn attach_corner_close_window(window: &mut Window, modal: NativeWindowRef) {
    super::make_gpui_popup_window_non_activating(window);
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
        DwmSetWindowAttribute,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_HWNDPARENT, SetWindowLongPtrW};
    /// `DWMWA_COLOR_NONE`: no border.
    const NO_BORDER: u32 = 0xFFFF_FFFE;
    let (NativeWindowRef(Some(owner)), NativeWindowRef(Some(hwnd))) =
        (modal, native_window_ref(window))
    else {
        return;
    };
    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    let corner = DWMWCP_DONOTROUND;
    let border = NO_BORDER;
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, owner);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const corner).cast(),
            std::mem::size_of_val(&corner) as u32,
        );
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            (&raw const border).cast(),
            std::mem::size_of_val(&border) as u32,
        );
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn attach_corner_close_window(_: &mut Window, _: NativeWindowRef) {}
