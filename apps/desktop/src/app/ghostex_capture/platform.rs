//! What GPUI cannot do for Ghostex Capture's windows, per platform: move an open window, keep it
//! above other apps, read the pointer anywhere on screen, and remember the app the user was in.
//!
//! CDXC:GhostexCapture 2026-09-30 WHY:
//! GPUI's only geometry call on an open window is `Window::resize`, and its pop-ups are owned by
//! (Windows) or transient for (X11) whatever Ghostex window was active, so they would hide with the
//! main window. Each backend here does the one native thing it needs. Wayland lets no client place
//! its own windows or read the pointer, so the floating button is not offered there.
//!
//! SEE-ALSO: apps/desktop/native/macos/GpuiGhostexCapture.m.

use gpui::{Bounds, Pixels, Point, point, px};

/// A native window reference: the NSView on macOS, the HWND on Windows, the X11 window id on Linux.
pub(crate) type NativeWindow = usize;

/// The app the user was in before Ghostex Capture was used, to capture as "current app".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrontmostApp {
    /// macOS process id, Windows HWND, or X11 window id; 0 when unknown.
    pub(crate) id: i64,
}

/// Whether this desktop session can host the floating button.
pub(crate) fn supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn GhostexGpuiCapturePrepareFloatingWindow(native_view: *mut std::ffi::c_void, keyable: bool);
    fn GhostexGpuiCaptureSetWindowFrame(
        native_view: *mut std::ffi::c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    );
    fn GhostexGpuiCaptureCoverScreen(
        native_view: *mut std::ffi::c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    );
    fn GhostexGpuiCaptureFocusWindow(native_view: *mut std::ffi::c_void);
    fn GhostexGpuiCaptureHideWindow(native_view: *mut std::ffi::c_void) -> bool;
    fn GhostexGpuiCaptureShowWindow(native_view: *mut std::ffi::c_void, focus: bool);
    fn GhostexGpuiCaptureMakeResizable(
        native_view: *mut std::ffi::c_void,
        min_width: f64,
        min_height: f64,
    );
    fn GhostexGpuiCaptureWindowFrame(
        native_view: *mut std::ffi::c_void,
        x: *mut f64,
        y: *mut f64,
        width: *mut f64,
        height: *mut f64,
    ) -> bool;
    fn GhostexGpuiCaptureFrontmostPid() -> i32;
    fn GhostexGpuiScreensHaveSeparateSpaces() -> bool;
    fn GhostexGpuiPrimaryMouseButtonDown() -> bool;
    fn GhostexGpuiDetachChildWindow(native_view: *mut std::ffi::c_void);
    fn GhostexGpuiPointerScreenLocation(x: *mut f64, y: *mut f64) -> bool;
}

/// Makes a freshly opened window float over other apps and every desktop; `keyable` windows take
/// the keyboard.
pub(crate) fn prepare_floating_window(native: NativeWindow, keyable: bool) {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiCapturePrepareFloatingWindow(native as *mut std::ffi::c_void, keyable)
    };
    #[cfg(target_os = "windows")]
    windows::prepare(native, keyable);
    #[cfg(target_os = "linux")]
    x11::prepare(native, keyable);
}

/// Moves and sizes a window to `frame` in GPUI's global space. `scale` is the window's scale
/// factor (device pixels per point) for the backends that place windows in device pixels.
pub(crate) fn set_window_frame(native: NativeWindow, frame: Bounds<Pixels>, scale: f32) {
    #[cfg(target_os = "macos")]
    {
        let _ = scale;
        unsafe {
            GhostexGpuiCaptureSetWindowFrame(
                native as *mut std::ffi::c_void,
                f32::from(frame.origin.x) as f64,
                f32::from(frame.origin.y) as f64,
                f32::from(frame.size.width) as f64,
                f32::from(frame.size.height) as f64,
            )
        };
    }
    #[cfg(target_os = "windows")]
    windows::set_frame(native, frame, scale);
    #[cfg(target_os = "linux")]
    x11::set_frame(native, frame, scale);
}

/// Lays a prepared window over the whole of `screen` (GPUI's global space), above the menu bar and
/// the Dock: the area-selection overlay. Elsewhere the window already opened at that frame.
pub(crate) fn cover_screen(native: NativeWindow, screen: Bounds<Pixels>) {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiCaptureCoverScreen(
            native as *mut std::ffi::c_void,
            f32::from(screen.origin.x) as f64,
            f32::from(screen.origin.y) as f64,
            f32::from(screen.size.width) as f64,
            f32::from(screen.size.height) as f64,
        )
    };
    #[cfg(not(target_os = "macos"))]
    let _ = (native, screen);
}

/// Lets a window be resized from its edges, no smaller than `min`. GPUI gives a window with no
/// title bar no resize edges on macOS, whatever `is_resizable` says; elsewhere that option does it.
pub(crate) fn make_resizable(native: NativeWindow, min: gpui::Size<Pixels>) {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiCaptureMakeResizable(
            native as *mut std::ffi::c_void,
            f32::from(min.width) as f64,
            f32::from(min.height) as f64,
        )
    };
    #[cfg(not(target_os = "macos"))]
    let _ = (native, min);
}

/// Where a window is and how big, in the space `set_window_frame` places it in, read from the OS
/// so it works from inside the window's own event handlers too. `None` where GPUI's own
/// `Window::bounds` is used instead.
pub(crate) fn window_frame(native: NativeWindow) -> Option<Bounds<Pixels>> {
    #[cfg(target_os = "macos")]
    {
        let [mut x, mut y, mut width, mut height] = [0f64; 4];
        unsafe {
            GhostexGpuiCaptureWindowFrame(
                native as *mut std::ffi::c_void,
                &mut x,
                &mut y,
                &mut width,
                &mut height,
            )
        }
        .then(|| {
            Bounds::new(
                point(px(x as f32), px(y as f32)),
                gpui::size(px(width as f32), px(height as f32)),
            )
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = native;
        None
    }
}

/// Whether each display has its own Spaces, where a window that belongs to another (a child
/// window) can only be shown on its owner's display. False where displays share one desktop.
pub(crate) fn screens_have_separate_spaces() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { GhostexGpuiScreensHaveSeparateSpaces() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Whether the primary mouse button is held, so a window drag may still be going. False where it
/// cannot be read.
pub(crate) fn primary_mouse_button_down() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { GhostexGpuiPrimaryMouseButtonDown() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Takes a child window off the window it belongs to (macOS), so the owner coming to the front
/// never shows it while it is hidden. Elsewhere a hidden window stays hidden on its own.
pub(crate) fn detach_from_parent(native: NativeWindow) {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiDetachChildWindow(native as *mut std::ffi::c_void)
    };
    #[cfg(not(target_os = "macos"))]
    let _ = native;
}

/// Gives a keyable window the keyboard again.
pub(crate) fn focus_window(native: NativeWindow) {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiCaptureFocusWindow(native as *mut std::ffi::c_void)
    };
    #[cfg(target_os = "windows")]
    windows::focus(native);
    #[cfg(target_os = "linux")]
    let _ = native;
}

/// Hides a window for a capture, so no part of it can show in the picture. Returns whether it had
/// the keyboard, for `show_window` to give it back.
pub(crate) fn hide_window(native: NativeWindow) -> bool {
    #[cfg(target_os = "macos")]
    return unsafe { GhostexGpuiCaptureHideWindow(native as *mut std::ffi::c_void) };
    #[cfg(target_os = "windows")]
    return windows::hide(native);
    #[cfg(target_os = "linux")]
    return x11::hide(native);
}

/// Shows a window `hide_window` hid, giving it the keyboard when `focus`.
pub(crate) fn show_window(native: NativeWindow, focus: bool) {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiCaptureShowWindow(native as *mut std::ffi::c_void, focus)
    };
    #[cfg(target_os = "windows")]
    windows::show(native, focus);
    #[cfg(target_os = "linux")]
    x11::show(native, focus);
}

/// The pointer in GPUI's global space, read from the OS.
pub(crate) fn pointer(scale: f32) -> Option<Point<Pixels>> {
    #[cfg(target_os = "macos")]
    {
        let _ = scale;
        let (mut x, mut y) = (0f64, 0f64);
        unsafe { GhostexGpuiPointerScreenLocation(&mut x, &mut y) }
            .then(|| point(px(x as f32), px(y as f32)))
    }
    #[cfg(target_os = "windows")]
    {
        windows::pointer(scale)
    }
    #[cfg(target_os = "linux")]
    {
        x11::pointer(scale)
    }
}

/// The app the user is in right now, unless it is Ghostex itself: then the last other app.
pub(crate) fn frontmost_app() -> FrontmostApp {
    #[cfg(target_os = "macos")]
    {
        FrontmostApp {
            id: i64::from(unsafe { GhostexGpuiCaptureFrontmostPid() }),
        }
    }
    #[cfg(target_os = "windows")]
    {
        windows::frontmost()
    }
    #[cfg(target_os = "linux")]
    {
        x11::frontmost()
    }
}

/// Starts remembering the last app in front that is not Ghostex, where the OS cannot tell it after
/// a click on the floating button activated Ghostex.
pub(crate) fn start_frontmost_tracking() {
    #[cfg(target_os = "windows")]
    windows::start_foreground_tracking();
}

#[cfg(target_os = "windows")]
mod windows {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    use gpui::{Bounds, Pixels, Point, point, px};
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWLP_HWNDPARENT, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, HWND_TOPMOST,
        SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    };

    use super::FrontmostApp;

    static LAST_FOREGROUND: AtomicIsize = AtomicIsize::new(0);
    static TRACKING: AtomicBool = AtomicBool::new(false);

    pub(super) fn prepare(native: usize, _keyable: bool) {
        let hwnd = native as HWND;
        unsafe {
            // GPUI makes a pop-up owned by the Ghostex window that was active when it opened;
            // an owned window hides whenever that window is minimized.
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }

    pub(super) fn set_frame(native: usize, frame: Bounds<Pixels>, scale: f32) {
        let device = |value: Pixels| (f32::from(value) * scale).round() as i32;
        unsafe {
            SetWindowPos(
                native as HWND,
                HWND_TOPMOST,
                device(frame.origin.x),
                device(frame.origin.y),
                device(frame.size.width),
                device(frame.size.height),
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    pub(super) fn focus(native: usize) {
        unsafe { SetForegroundWindow(native as HWND) };
    }

    pub(super) fn hide(native: usize) -> bool {
        let hwnd = native as HWND;
        let had_focus = unsafe { GetForegroundWindow() } == hwnd;
        unsafe { ShowWindow(hwnd, SW_HIDE) };
        had_focus
    }

    pub(super) fn show(native: usize, focus: bool) {
        let hwnd = native as HWND;
        unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        if focus {
            unsafe { SetForegroundWindow(hwnd) };
        }
    }

    pub(super) fn pointer(scale: f32) -> Option<Point<Pixels>> {
        let mut position = POINT { x: 0, y: 0 };
        (unsafe { GetCursorPos(&mut position) } != 0)
            .then(|| point(px(position.x as f32 / scale), px(position.y as f32 / scale)))
    }

    fn is_own(hwnd: HWND) -> bool {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        pid == unsafe { GetCurrentProcessId() }
    }

    pub(super) fn frontmost() -> FrontmostApp {
        let hwnd = unsafe { GetForegroundWindow() };
        if !hwnd.is_null() && !is_own(hwnd) {
            return FrontmostApp { id: hwnd as i64 };
        }
        FrontmostApp {
            id: LAST_FOREGROUND.load(Ordering::Relaxed) as i64,
        }
    }

    pub(super) fn start_foreground_tracking() {
        if TRACKING.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = std::thread::Builder::new()
            .name("ghostex-capture-foreground".into())
            .spawn(|| {
                loop {
                    let hwnd = unsafe { GetForegroundWindow() };
                    if !hwnd.is_null() && !is_own(hwnd) {
                        LAST_FOREGROUND.store(hwnd as isize, Ordering::Relaxed);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            });
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use std::sync::OnceLock;

    use gpui::{Bounds, Pixels, Point, point, px};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask, InputFocus,
    };
    use x11rb::rust_connection::RustConnection;

    use super::FrontmostApp;

    fn connection() -> Option<&'static (RustConnection, u32)> {
        static CONNECTION: OnceLock<Option<(RustConnection, u32)>> = OnceLock::new();
        CONNECTION
            .get_or_init(|| {
                let (connection, screen) = x11rb::connect(None).ok()?;
                let root = connection.setup().roots.get(screen)?.root;
                Some((connection, root))
            })
            .as_ref()
    }

    fn atom(connection: &RustConnection, name: &[u8]) -> Option<u32> {
        Some(connection.intern_atom(false, name).ok()?.reply().ok()?.atom)
    }

    pub(super) fn prepare(native: usize, keyable: bool) {
        // The button is an override-redirect pop-up the window manager leaves alone; keyable
        // windows are managed, so they ask to stay above and on every desktop.
        if !keyable {
            return;
        }
        let Some((connection, root)) = connection() else {
            return;
        };
        let (Some(state), Some(above), Some(sticky), Some(skip_taskbar)) = (
            atom(connection, b"_NET_WM_STATE"),
            atom(connection, b"_NET_WM_STATE_ABOVE"),
            atom(connection, b"_NET_WM_STATE_STICKY"),
            atom(connection, b"_NET_WM_STATE_SKIP_TASKBAR"),
        ) else {
            return;
        };
        for pair in [[above, sticky], [skip_taskbar, 0]] {
            let event =
                ClientMessageEvent::new(32, native as u32, state, [1, pair[0], pair[1], 1, 0]);
            let _ = connection.send_event(
                false,
                *root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            );
        }
        let _ = connection.flush();
    }

    pub(super) fn set_frame(native: usize, frame: Bounds<Pixels>, scale: f32) {
        let Some((connection, _)) = connection() else {
            return;
        };
        let device = |value: Pixels| (f32::from(value) * scale).round() as i32;
        let aux = ConfigureWindowAux::new()
            .x(device(frame.origin.x))
            .y(device(frame.origin.y))
            .width(device(frame.size.width).max(1) as u32)
            .height(device(frame.size.height).max(1) as u32);
        let _ = connection.configure_window(native as u32, &aux);
        let _ = connection.flush();
    }

    pub(super) fn hide(native: usize) -> bool {
        let Some((connection, _)) = connection() else {
            return false;
        };
        let had_focus = connection
            .get_input_focus()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| reply.focus == native as u32);
        let _ = connection.unmap_window(native as u32);
        let _ = connection.flush();
        had_focus
    }

    pub(super) fn show(native: usize, focus: bool) {
        let Some((connection, _)) = connection() else {
            return;
        };
        let _ = connection.map_window(native as u32);
        if focus {
            let _ =
                connection.set_input_focus(InputFocus::PARENT, native as u32, x11rb::CURRENT_TIME);
        }
        let _ = connection.flush();
    }

    pub(super) fn pointer(scale: f32) -> Option<Point<Pixels>> {
        let (connection, root) = connection()?;
        let reply = connection.query_pointer(*root).ok()?.reply().ok()?;
        Some(point(
            px(reply.root_x as f32 / scale),
            px(reply.root_y as f32 / scale),
        ))
    }

    pub(super) fn frontmost() -> FrontmostApp {
        let Some((connection, root)) = connection() else {
            return FrontmostApp::default();
        };
        let Some(active) = atom(connection, b"_NET_ACTIVE_WINDOW") else {
            return FrontmostApp::default();
        };
        let id = connection
            .get_property(false, *root, active, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().and_then(|mut values| values.next()))
            .unwrap_or(0);
        FrontmostApp { id: i64::from(id) }
    }
}
