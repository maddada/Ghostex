use gpui::{
    Animation, AnimationExt as _, AnyElement, IntoElement, ParentElement, Styled, Window, div, px,
};
use std::time::Duration;

/// CDXC:AppModal 2026-10-31 DECISION:
/// User: "please make the accounts panels and notifications animate in slighly please they appear so quick now without animation". A short fade and a 4px rise, the ease-out the chat's cards use. Reduce Motion and the Panel animations setting's None open it at once. Each open is a new window with new animation state, so every open replays it and re-fitting an open window does not.
const POPUP_OPEN_DURATION: Duration = Duration::from_millis(140);
const POPUP_OPEN_RISE: f32 = 4.0;

/// How long a popup takes to open, `None` when it opens at once.
fn popup_open_duration(reduce_motion: bool, panel_motion: Duration) -> Option<Duration> {
    (!reduce_motion && !panel_motion.is_zero()).then_some(POPUP_OPEN_DURATION)
}

/// How far below its place the content sits at `delta` (0 to 1) of the open animation.
fn popup_open_offset(delta: f32) -> f32 {
    POPUP_OPEN_RISE * (1.0 - delta)
}

fn popup_open_animation(reduce_motion: bool) -> Option<Animation> {
    popup_open_duration(
        reduce_motion,
        crate::app::panel_motion::panel_motion_duration(),
    )
    .map(|duration| Animation::new(duration).with_easing(gpui::ease_out_quint()))
}

/// `frame` holding `content`, opening with the fade and rise unless `at_once`.
///
/// The frame's own opacity is the fade off macOS, where a window's alpha can't be set: there the
/// fill and edge fade with the content, but window glass's blur is the OS's and shows at once. On
/// macOS `fade_in_popup_window` fades the window itself, blur included, so only the rise is drawn.
pub(crate) fn popup_open_frame<F>(
    frame: F,
    content: impl IntoElement,
    at_once: bool,
    reduce_motion: bool,
) -> AnyElement
where
    F: Styled + ParentElement + IntoElement + 'static,
{
    let Some(animation) = popup_open_animation(reduce_motion).filter(|_| !at_once) else {
        return frame.child(content).into_any_element();
    };
    let frame = frame.child(
        div()
            .relative()
            .size_full()
            .min_h_0()
            .child(content)
            .with_animation("popup-open-rise", animation.clone(), |rising, delta| {
                rising.top(px(popup_open_offset(delta)))
            }),
    );
    if cfg!(target_os = "macos") {
        frame.into_any_element()
    } else {
        frame
            .with_animation("popup-open-fade", animation, |frame, delta| {
                frame.opacity(delta)
            })
            .into_any_element()
    }
}

/// Fades a popup's window in as it opens, called from its `open_window` closure (macOS).
#[cfg(target_os = "macos")]
pub(crate) fn fade_in_popup_window(window: &Window, reduce_motion: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    unsafe extern "C" {
        fn GhostexGpuiFadeInPopupWindow(native_view: *mut std::ffi::c_void, seconds: f64);
    }
    let Some(duration) = popup_open_duration(
        reduce_motion,
        crate::app::panel_motion::panel_motion_duration(),
    ) else {
        return;
    };
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        unsafe { GhostexGpuiFadeInPopupWindow(handle.ns_view.as_ptr(), duration.as_secs_f64()) };
    }
}

/// See `popup_open_frame`: elsewhere the frame's opacity is the fade.
#[cfg(not(target_os = "macos"))]
pub(crate) fn fade_in_popup_window(_window: &Window, _reduce_motion: bool) {}

/// CDXC:PlatformSupport 2026-09-23 WHY:
/// Windows menus and borderless child dialogs must be tool popups. Normal app windows are eligible for FancyZones' last-zone placement, which moved both the chat model menu and Add Worktree dialog to the main window's top-left despite correct requested bounds.
/// CDXC:PlatformSupport 2026-09-24 WHY:
/// Linux tiling managers need transient ownership before mapping; borderless Normal windows still tile. Floating preserves outside-click dismissal and session switching, which Dialog would block. Each Linux caller supplies x11_parent explicitly.
pub(crate) fn child_window_kind() -> gpui::WindowKind {
    if cfg!(target_os = "windows") {
        gpui::WindowKind::PopUp
    } else if cfg!(target_os = "linux") {
        gpui::WindowKind::Floating
    } else {
        gpui::WindowKind::Normal
    }
}

/// CDXC:PlatformSupport 2026-09-29 WHY:
/// GPUI gives a Linux window server-side decorations unless it asks otherwise, so KWin drew a title bar with minimize and close buttons on the chat menus, the account popover and the app modals, which all draw their own panel. Child windows opened with `child_window_kind()` and no titlebar ask for client decorations, which on X11 means no window-manager frame, the borderless window they are on macOS and Windows. Their `Root` is built with `bordered(false)`: gpui-component otherwise wraps a client-decorated window in its Linux shadow and resize edges, which shrank the menus and pushed the account popover's Add account button out of its window.
pub(crate) fn child_window_decorations() -> Option<gpui::WindowDecorations> {
    cfg!(target_os = "linux").then_some(gpui::WindowDecorations::Client)
}

/// The window a menu, dialog, toast or other child window belongs to: its frame and the display
/// that frame is measured on, read together. Child windows take their `WindowOptions::display_id`
/// from here, never from a point alone.
///
/// CDXC:PlatformSupport 2026-10-01 WHY:
/// A frame given to `WindowOptions::window_bounds` means something only together with its
/// `display_id`, and what it means differs by platform. On macOS every display's GPUI bounds start
/// at (0, 0), `Window::bounds` is measured from the display the window is on, and a new window is
/// placed against `display_id`, falling back to the menu-bar display; a frame measured from its
/// owner but opened with no display (or the display "under" a point, which on macOS was always the
/// menu-bar display) landed on the menu-bar display at the owner's offset whenever Ghostex sat on
/// another monitor, so the chat composer's menus, the toasts and the modal close button read as
/// never opening, and the Files comment box hid the selection toolbar until a restart (10.8.1).
/// On macOS a child window therefore always opens on its owner's display, whatever monitor its own
/// frame reaches. On Windows and X11 frames are global and displays report their real origin, but
/// Windows replaces bounds whose centre is not on the named display with that display's default
/// spot (the chat menus opened "behind" the main window, 2026-09-23), so there the display is the
/// one under the frame's centre, else the owner's. Windows placed from the OS pointer use
/// `place_global` instead. Supersedes the 2026-09-23 and 2026-10-01 notes on `display_at`, which
/// let a caller pick a display without naming the window its frame was measured from.
/// SEE-ALSO: GhostexGpuiPointerScreenLocation and GhostexGpuiCaptureWindowFrame (native/macos), which
/// report the shared space `place_global` reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PopupOwner {
    /// The owner's frame, in the space `Window::bounds` reports.
    pub(crate) frame: gpui::Bounds<gpui::Pixels>,
    pub(crate) display_id: Option<gpui::DisplayId>,
}

impl PopupOwner {
    pub(crate) fn of(window: &Window, cx: &gpui::App) -> Self {
        Self::new(
            window.bounds(),
            window.display(cx).map(|display| display.id()),
        )
    }

    /// An owner known from a frame and display read together earlier (the main window's, cached
    /// by the app on every draw).
    pub(crate) fn new(
        frame: gpui::Bounds<gpui::Pixels>,
        display_id: Option<gpui::DisplayId>,
    ) -> Self {
        Self { frame, display_id }
    }

    /// `WindowOptions::display_id` for a child window at `frame`, measured in the owner's space.
    pub(crate) fn display_for(
        &self,
        frame: gpui::Bounds<gpui::Pixels>,
        cx: &gpui::App,
    ) -> Option<gpui::DisplayId> {
        if cfg!(target_os = "macos") {
            return self.display_id;
        }
        cx.displays()
            .into_iter()
            .find(|display| display.bounds().contains(&frame.center()))
            .map(|display| display.id())
            .or(self.display_id)
    }

    /// The visible frame (without the menu bar, notch, Dock or taskbar) of the display a child
    /// window at `frame` opens on, in the owner's space.
    pub(crate) fn visible_frame(
        &self,
        frame: gpui::Bounds<gpui::Pixels>,
        cx: &gpui::App,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        let display = cx.find_display(self.display_for(frame, cx)?)?;
        Some(display.visible_bounds())
    }

    /// `point`, measured in the owner's space, in the space every display shares (`place_global`).
    pub(crate) fn to_global(
        &self,
        point: gpui::Point<gpui::Pixels>,
        cx: &gpui::App,
    ) -> gpui::Point<gpui::Pixels> {
        self.display_id
            .and_then(|id| cx.find_display(id))
            .map_or(point, |display| point + global_shift(&*display))
    }
}

/// How far `display`'s GPUI frame sits from its place in the shared space: logical pixels from the
/// menu-bar display's top-left corner, y down, where the OS reports the pointer. Windows and X11
/// report displays there already; GPUI's macOS displays all start at (0, 0), so their origin comes
/// from Core Graphics, whose global display space is that same space.
fn global_shift(display: &dyn gpui::PlatformDisplay) -> gpui::Point<gpui::Pixels> {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        #[allow(dead_code)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        #[repr(C)]
        #[allow(dead_code)]
        struct CGSize {
            width: f64,
            height: f64,
        }
        #[repr(C)]
        #[allow(dead_code)]
        struct CGRect {
            origin: CGPoint,
            size: CGSize,
        }
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGDisplayBounds(display: u32) -> CGRect;
        }
        // GPUI's macOS display ids are CGDirectDisplayIDs.
        let rect = unsafe { CGDisplayBounds(u64::from(display.id()) as u32) };
        gpui::point(
            gpui::px(rect.origin.x as f32),
            gpui::px(rect.origin.y as f32),
        ) - display.bounds().origin
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = display;
        gpui::Point::default()
    }
}

/// `display`'s frame and visible frame in the shared space (`place_global`).
pub(crate) fn global_display_frames(
    display: &dyn gpui::PlatformDisplay,
) -> (gpui::Bounds<gpui::Pixels>, gpui::Bounds<gpui::Pixels>) {
    let shift = global_shift(display);
    let (bounds, visible) = (display.bounds(), display.visible_bounds());
    (
        gpui::Bounds::new(bounds.origin + shift, bounds.size),
        gpui::Bounds::new(visible.origin + shift, visible.size),
    )
}

/// A window not owned by another window (the "Copied!" bubble, Ghostex Capture's windows) placed
/// from the OS pointer or a saved screen position: `frame` in the shared space, as
/// `WindowOptions::window_bounds` and `display_id` take it. The display is the one under the frame's
/// centre; a frame on no display opens against the menu-bar display, whose GPUI frame starts where
/// the shared space does.
pub(crate) fn place_global(
    frame: gpui::Bounds<gpui::Pixels>,
    cx: &gpui::App,
) -> (gpui::Bounds<gpui::Pixels>, Option<gpui::DisplayId>) {
    cx.displays()
        .into_iter()
        .find_map(|display| {
            let shift = global_shift(&*display);
            let bounds = display.bounds();
            gpui::Bounds::new(bounds.origin + shift, bounds.size)
                .contains(&frame.center())
                .then(|| {
                    (
                        gpui::Bounds::new(frame.origin - shift, frame.size),
                        Some(display.id()),
                    )
                })
        })
        .unwrap_or((frame, None))
}

/// Strips the system frame and shadow from a popup's own window (`GpuiChatDialogWindow.m`).
/// Only for windows with a transparent background whose content draws its own panel.
pub(crate) fn strip_gpui_popup_window_frame(window: &mut Window) {
    #[cfg(target_os = "macos")]
    {
        use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
        unsafe extern "C" {
            fn GhostexGpuiStripPopupWindowFrame(view: *mut std::ffi::c_void);
        }
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
        {
            unsafe { GhostexGpuiStripPopupWindowFrame(handle.ns_view.as_ptr()) };
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

/// The radius a popup window's own card is drawn with when it asked for `radius`, so the card's
/// corner and its window's corner are one curve (see `round_popup_window`).
pub(crate) fn popup_card_radius(radius: gpui::Pixels, blurred: bool) -> gpui::Pixels {
    #[cfg(target_os = "windows")]
    if blurred {
        // The radii DWM gives `DWMWCP_ROUNDSMALL` and `DWMWCP_ROUND`, picked the way
        // gpui_windows' `set_background_corner_radius` picks between them.
        return gpui::px(if radius <= gpui::px(0.0) {
            0.0
        } else if radius < gpui::px(6.0) {
            4.0
        } else {
            8.0
        });
    }
    let _ = blurred;
    radius
}

/// Rounds a popup window whose content draws its own rounded, bordered card: the chat composer's
/// `/`, `$` and `@` list and the chat's menus. `blurred` is whether the window was opened with
/// `WindowBackgroundAppearance::Blurred`; the card must be drawn at
/// `popup_card_radius(radius, blurred)`.
///
/// CDXC:PlatformSupport 2026-10-20 WHY:
/// Windows 11 DWM rounds every popup window itself, at a fixed 8px (4px small), and draws its own 1px border along that curve, while the card inside was drawn at the chat's radius (18px for the `/` list) with a border of its own, so each corner showed two curves that did not meet, with and without window glass. DWM's corner cannot take another radius, and the blur of a blurred window fills the window's rounded rectangle whatever its region (gpui_windows' frosted_backdrop.rs), so under glass the card takes DWM's radius; with glass off the window is transparent, so DWM leaves it square and the card's own corner is the only shape. DWM's border is dropped in both cases: the card draws the border. macOS and Linux round the window to the card's radius, unchanged.
pub(crate) fn round_popup_window(window: &mut Window, radius: gpui::Pixels, blurred: bool) {
    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmSetWindowAttribute};
        /// `DWMWA_COLOR_NONE`: no border.
        const NO_BORDER: u32 = 0xFFFF_FFFE;
        // Zero asks DWM for `DWMWCP_DONOTROUND`.
        window.set_background_corner_radius(if blurred { radius } else { gpui::px(0.0) });
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        let border = NO_BORDER;
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR as u32,
                (&raw const border).cast(),
                std::mem::size_of_val(&border) as u32,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = blurred;
        window.set_background_corner_radius(radius);
    }
}

/// CDXC:AppModal 2026-09-25 DECISION:
/// User: on Windows the modals shown without decoration (Add a project and the like) "blend with the bg of the app", so they need a border. macOS draws a rim and shadow around these borderless windows; a Windows PopUp gets neither, and the modal surface is tinted from the app chrome. Windows 11 DWM gives the modal window rounded corners, its shadow and a border in `border`, the same frame for the React and native modal hosts. Windows 10 ignores both attributes.
pub(crate) fn frame_app_modal_window(window: &mut Window, border: gpui::Rgba) {
    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::{
            DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
        };
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
        // COLORREF is 0x00BBGGRR.
        let color: u32 = channel(border.r) | channel(border.g) << 8 | channel(border.b) << 16;
        let corner = DWMWCP_ROUND;
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&raw const corner).cast(),
                std::mem::size_of_val(&corner) as u32,
            );
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR as u32,
                (&raw const color).cast(),
                std::mem::size_of_val(&color) as u32,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, border);
}

#[cfg(test)]
mod popup_open_tests {
    use super::*;

    const NORMAL: Duration = Duration::from_millis(200);

    #[test]
    fn it_opens_briefly_unless_motion_is_turned_off() {
        let opens = popup_open_duration(false, NORMAL).unwrap();
        assert!((120..=160).contains(&opens.as_millis()), "{opens:?}");
        assert_eq!(popup_open_duration(true, NORMAL), None);
        assert_eq!(popup_open_duration(false, Duration::ZERO), None);
    }

    #[test]
    fn the_content_rises_a_little_into_place() {
        assert_eq!(popup_open_offset(0.0), POPUP_OPEN_RISE);
        assert_eq!(popup_open_offset(1.0), 0.0);
        assert!(POPUP_OPEN_RISE <= 6.0);
    }
}
