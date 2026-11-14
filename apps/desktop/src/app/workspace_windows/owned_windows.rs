//! Keeping the windows a workspace window owns on it while the user moves it.

use gpui::Window;

/// Shifts the windows `window` owns by the distance it moved since it last reported its frame.
/// Called from the workspace window's bounds observer and once when it opens, which records the
/// starting position.
///
/// CDXC:AppModal 2026-10-04 DECISION:
/// User: "I just moved the window while maximize was enabled in the gpui chat view composer and the maximize window stayed in its place even though i moved the app around ... Do same for other similar cases in the app." Every window drawn over a workspace window moves with it. macOS already does this for windows attached with `addChildWindow:`. A Windows owned window keeps its screen position when its owner moves, so every window whose owner chain reaches the moved workspace window is shifted by the same distance: GPUI owns each popup by the window that was active when it opened (the maximized composer and the chat's other dialogs and menus, the app modals and, through them, their corner X, the toasts, the Docs drawer, the floating sidebar panel, the hidden New Thread picker). The windows Ghostex Capture detaches from their owner stay where they are.
/// CDXC:AppModal 2026-10-04 WHY:
/// The shift runs from a task, outside the app update, the way GPUI's own `Window::resize` does: each shifted window's GPUI state takes its new origin from WM_MOVE without being drawn again, which a moved callback arriving during an update would refuse. The Windows move loop runs foreground tasks on its timer, so the windows follow during the drag, not only once it ends. A minimized window's parked frame is not a move, so restoring it moves nothing.
#[cfg(target_os = "windows")]
pub(super) fn follow_owner_window_frame(window: &Window, cx: &gpui::App) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::cell::RefCell;
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic};

    thread_local! {
        /// Each workspace window's top-left corner when it last reported, in physical pixels.
        static OWNER_ORIGINS: RefCell<Vec<(isize, (i32, i32))>> = const { RefCell::new(Vec::new()) };
    }

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let owner = handle.hwnd.get();
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `owner` is the live window this observer belongs to.
    if unsafe { IsIconic(owner as HWND) } != 0
        || unsafe { GetWindowRect(owner as HWND, &mut rect) } == 0
    {
        return;
    }
    let origin = (rect.left, rect.top);
    let previous = OWNER_ORIGINS.with(|origins| {
        let mut origins = origins.borrow_mut();
        match origins.iter_mut().find(|(hwnd, _)| *hwnd == owner) {
            Some((_, last)) => Some(std::mem::replace(last, origin)),
            None => {
                origins.push((owner, origin));
                None
            }
        }
    });
    let Some(previous) = previous.filter(|previous| *previous != origin) else {
        return;
    };
    let (dx, dy) = (origin.0 - previous.0, origin.1 - previous.1);
    cx.foreground_executor()
        .spawn(async move { shift_owned_windows(owner, dx, dy) })
        .detach();
}

/// The X11 side of the rule above.
///
/// CDXC:AppModal 2026-11-06 WHY:
/// Discord report (Ubuntu 24.04, two displays): on first launch the onboarding opened centred on the main window, the user moved the main window to the other screen, and the onboarding stayed behind with no way to move it. An X11 window that is WM_TRANSIENT_FOR the main window (`x11_parent`, or GPUI's own parent for a Floating window) keeps its screen position when the main window moves, exactly like a Windows owned window; Mutter only drags along transients that are modal, and the app modals deliberately are not. So every Ghostex client window whose transient chain reaches the moved workspace window is shifted by the same distance: the app modals (onboarding included), the chat's dialogs and menus, the toasts. Positions are read from the server in physical pixels, as `set_bounds` in gpui_linux's X11 window does, and the shift runs from a task outside the app update, as on Windows. Wayland gives no client a position to read or set, so nothing happens there.
#[cfg(target_os = "linux")]
pub(super) fn follow_owner_window_frame(window: &Window, cx: &gpui::App) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::cell::RefCell;

    thread_local! {
        /// Each workspace window's top-left corner when it last reported, in physical pixels.
        static OWNER_ORIGINS: RefCell<Vec<(u32, (i32, i32))>> = const { RefCell::new(Vec::new()) };
    }

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let owner = match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        _ => return,
    };
    let Some(origin) = x11::client_origin(owner) else {
        return;
    };
    let previous = OWNER_ORIGINS.with(|origins| {
        let mut origins = origins.borrow_mut();
        match origins.iter_mut().find(|(window, _)| *window == owner) {
            Some((_, last)) => Some(std::mem::replace(last, origin)),
            None => {
                origins.push((owner, origin));
                None
            }
        }
    });
    let Some(previous) = previous.filter(|previous| *previous != origin) else {
        return;
    };
    let (dx, dy) = (origin.0 - previous.0, origin.1 - previous.1);
    cx.foreground_executor()
        .spawn(async move { x11::shift_transient_windows(owner, dx, dy) })
        .detach();
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub(super) fn follow_owner_window_frame(_: &Window, _: &gpui::App) {}

#[cfg(target_os = "linux")]
mod x11 {
    use std::sync::OnceLock;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt, Window};
    use x11rb::rust_connection::RustConnection;

    /// This module's own display connection; the X server orders its requests with GPUI's.
    fn connection() -> Option<&'static (RustConnection, Window)> {
        static CONNECTION: OnceLock<Option<(RustConnection, Window)>> = OnceLock::new();
        CONNECTION
            .get_or_init(|| {
                let (connection, screen) = x11rb::connect(None).ok()?;
                let root = connection.setup().roots.get(screen)?.root;
                Some((connection, root))
            })
            .as_ref()
    }

    /// Where `window`'s client area sits on the root window, in physical pixels.
    pub(super) fn client_origin(window: Window) -> Option<(i32, i32)> {
        let (connection, root) = connection()?;
        let reply = connection
            .translate_coordinates(window, *root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        Some((reply.dst_x as i32, reply.dst_y as i32))
    }

    fn window_property(connection: &RustConnection, window: Window, name: &[u8]) -> Vec<u32> {
        let Some(atom) = connection
            .intern_atom(true, name)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom)
            .filter(|atom| *atom != 0)
        else {
            return Vec::new();
        };
        connection
            .get_property(false, window, atom, AtomEnum::ANY, 0, 4096)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
    }

    /// Where a following window should be: where Ghostex last moved it, and where it was just before.
    #[derive(Clone, Copy)]
    struct Followed {
        placed: (i32, i32),
        before: (i32, i32),
    }

    /// The windows that follow their owner, and the ones the user moved away, which no longer do.
    #[derive(Default)]
    struct FollowState {
        followed: std::collections::HashMap<Window, Followed>,
        let_go: std::collections::HashSet<Window>,
    }

    fn follow_state() -> &'static std::sync::Mutex<FollowState> {
        static STATE: OnceLock<std::sync::Mutex<FollowState>> = OnceLock::new();
        STATE.get_or_init(Default::default)
    }

    /// A window manager can put a moved window a pixel off where it was asked to go.
    const PLACEMENT_SLACK: i32 = 4;

    fn near(a: (i32, i32), b: (i32, i32)) -> bool {
        (a.0 - b.0).abs() <= PLACEMENT_SLACK && (a.1 - b.1).abs() <= PLACEMENT_SLACK
    }

    fn frame(connection: &RustConnection, window: Window) -> Option<(i32, i32, i32, i32)> {
        let (x, y) = client_origin(window)?;
        let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
        Some((x, y, geometry.width as i32, geometry.height as i32))
    }

    fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }

    /// Moves every managed window of this process whose WM_TRANSIENT_FOR chain reaches `owner` by
    /// `(dx, dy)` physical pixels, keeping its size and stacking, unless the user moved it away.
    ///
    /// CDXC:AppModal 2026-11-14 DECISION:
    /// User (Omarchy audit Q9): "dialogs follow Ghostex until the user drags one, then stay where put." On Hyprland any floating window can be dragged, so a dialog pulled off Ghostex kept jumping along whenever Ghostex moved. A window Ghostex has moved before is let go once it is no longer where Ghostex put it; one Ghostex has not moved yet is let go when it no longer overlaps Ghostex. A let-go window stays put until it closes.
    pub(super) fn shift_transient_windows(owner: Window, dx: i32, dy: i32) {
        let Some((connection, root)) = connection() else {
            return;
        };
        let Ok(mut state) = follow_state().lock() else {
            return;
        };
        // Where the owner was before this move, to tell which windows were still on it.
        let owner_before =
            frame(connection, owner).map(|(x, y, width, height)| (x - dx, y - dy, width, height));
        let pid = std::process::id();
        let transient_for = |window: Window| {
            window_property(connection, window, b"WM_TRANSIENT_FOR")
                .first()
                .copied()
                .filter(|parent| *parent != 0)
        };
        let owned_by_owner = |mut window: Window| {
            // Transient chains are short (a menu, its modal, the workspace window); the bound guards a cycle.
            for _ in 0..8 {
                let Some(parent) = transient_for(window) else {
                    return false;
                };
                if parent == owner {
                    return true;
                }
                window = parent;
            }
            false
        };
        let clients = window_property(connection, *root, b"_NET_CLIENT_LIST");
        // Window ids are reused once a window closes.
        state.followed.retain(|window, _| clients.contains(window));
        state.let_go.retain(|window| clients.contains(window));
        for window in clients {
            if window == owner
                || state.let_go.contains(&window)
                || window_property(connection, window, b"_NET_WM_PID").first() != Some(&pid)
                || !owned_by_owner(window)
            {
                continue;
            }
            let Some((x, y, width, height)) = frame(connection, window) else {
                continue;
            };
            let from = match state.followed.get(&window) {
                // Still where Ghostex put it, or not there yet during a quick drag of the owner.
                Some(followed) if near((x, y), followed.placed) => followed.placed,
                Some(followed) if near((x, y), followed.before) => followed.before,
                Some(_) => {
                    state.let_go.insert(window);
                    continue;
                }
                None if owner_before
                    .is_some_and(|owner_before| !overlaps((x, y, width, height), owner_before)) =>
                {
                    state.let_go.insert(window);
                    continue;
                }
                None => (x, y),
            };
            let placed = (from.0 + dx, from.1 + dy);
            let _ = connection
                .configure_window(window, &ConfigureWindowAux::new().x(placed.0).y(placed.1));
            state.followed.insert(
                window,
                Followed {
                    placed,
                    before: from,
                },
            );
        }
        let _ = connection.flush();
    }
}

/// Moves every top-level window of this thread whose owner chain reaches `owner` by `(dx, dy)`
/// physical pixels, keeping its size, stacking and activation.
#[cfg(target_os = "windows")]
fn shift_owned_windows(owner: isize, dx: i32, dy: i32) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GW_OWNER, GetWindow, GetWindowRect, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
        SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `windows` vector below, alive for the whole EnumThreadWindows call.
        let windows = unsafe { &mut *(lparam as *mut Vec<HWND>) };
        windows.push(hwnd);
        1
    }
    let owner = owner as HWND;
    let owned_by_owner = |mut hwnd: HWND| {
        // Owner chains are short (a menu, its modal, the workspace window); the bound guards a cycle.
        for _ in 0..8 {
            // SAFETY: GetWindow only reads the window's owner.
            hwnd = unsafe { GetWindow(hwnd, GW_OWNER) };
            if hwnd.is_null() {
                return false;
            }
            if hwnd == owner {
                return true;
            }
        }
        false
    };
    let mut windows: Vec<HWND> = Vec::new();
    // SAFETY: `collect` only appends to the vector the pointer names.
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(collect),
            &mut windows as *mut Vec<HWND> as LPARAM,
        )
    };
    for hwnd in windows {
        if hwnd == owner || !owned_by_owner(hwnd) {
            continue;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: `hwnd` was enumerated on this thread just above.
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            continue;
        }
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                rect.left + dx,
                rect.top + dy,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
            )
        };
    }
}
