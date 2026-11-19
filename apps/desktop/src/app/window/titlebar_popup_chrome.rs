//! Native window chrome for the titlebar dropdown popup windows.
//!
//! The dropdown is an owned popup window that must never take activation from
//! the main window: the menus are mouse driven, Escape is handled by the main
//! window, and the main window's deactivation observer closes whichever
//! dropdown is open. A panel that steals activation therefore destroys itself
//! on the very click it was opened for.

use gpui::Window;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

#[cfg(target_os = "macos")]
pub(crate) fn prepare_gpui_titlebar_popup_window_chrome(window: &mut Window) {
    use crate::app::helpers::os_cli::native_event_queue::GhostexGpuiPrepareTitlebarPopupWindow;

    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        unsafe { GhostexGpuiPrepareTitlebarPopupWindow(handle.ns_view.as_ptr()) };
    }
}

/// CDXC:Titlebar 2026-09-20 WHY:
/// The Windows counterpart of the macOS `becomesKeyOnlyIfNeeded` panel. GPUI creates these popups with `focus: false`, which gives them WS_EX_NOACTIVATE and SW_SHOWNOACTIVATE, but its shared window procedure answers every WM_MOUSEACTIVATE with SetActiveWindow(handle) to keep `active_window` current. That activated the popup on the mouse-down before the click, deactivated the main window, and let the main window's observer close the popup before the click reached a row, so every titlebar dropdown row (Quick Actions "Configure" included) did nothing on Windows. Answering WM_MOUSEACTIVATE with MA_NOACTIVATE ahead of GPUI keeps the mouse message and leaves activation on the main window.
#[cfg(target_os = "windows")]
pub(crate) fn prepare_gpui_titlebar_popup_window_chrome(window: &mut Window) {
    make_gpui_popup_window_non_activating(window);
}

/// Keeps a pointer-only pop-up (a titlebar dropdown or a frosted host) from taking activation from
/// its owner when it is clicked, for the reason above.
#[cfg(target_os = "windows")]
pub(crate) fn make_gpui_popup_window_non_activating(window: &mut Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    windows_chrome::make_popup_window_non_activating(handle.hwnd.get() as windows_chrome::Hwnd);
}

/// Makes a pop-up the pointer passes through (a tooltip): it never takes the hover, a click or
/// activation, so the window under it keeps them.
///
/// CDXC:Tooltips 2026-10-09 WHY:
/// The frosted tooltip host is a real window, and on Windows it caught the pointer when a tooltip opened under it: the floating files list read that as the pointer leaving and closed ("hovering over this tooltip in files list causing the floating files list to go away"), and the tooltip blocked clicks on the rows it covered. It answers hit tests with HTTRANSPARENT, which hands the pointer to the window beneath on the same thread (every GPUI window is on the main thread), as the macOS host's `ignoresMouseEvents` does.
#[cfg(target_os = "windows")]
pub(crate) fn make_gpui_popup_window_click_through(window: &mut Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    windows_chrome::make_popup_window_click_through(handle.hwnd.get() as windows_chrome::Hwnd);
}

/// CDXC:PlatformSupport 2026-10-04 WHY:
/// GPUI owns a Windows pop-up by whichever window is active when it opens, and keeps an unowned pop-up topmost. The chat's scroll-to-bottom pill (with its Escape and "Agent was interrupted" toasts) and the frosted tooltips can open while Ghostex is in the background (streaming text, a wheel or hover over the inactive window); they then had no owner and floated over the app the user had switched to. These overlays belong to the window they are drawn over, so they are owned by it and leave the topmost band, sitting just above it.
#[cfg(target_os = "windows")]
pub(crate) fn own_gpui_popup_window(window: &mut Window, owner: *mut std::ffi::c_void) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    own_popup_hwnd(handle.hwnd.get() as *mut std::ffi::c_void, owner);
}

/// `own_gpui_popup_window` for a pop-up known by its HWND: a reused frosted host shown again over
/// an owner whose band may have changed while it was hidden (`frosted_host.rs`).
#[cfg(target_os = "windows")]
pub(crate) fn own_popup_hwnd(popup: *mut std::ffi::c_void, owner: *mut std::ffi::c_void) {
    windows_chrome::own_popup_window(popup as windows_chrome::Hwnd, owner as windows_chrome::Hwnd);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn prepare_gpui_titlebar_popup_window_chrome(_window: &mut Window) {}

#[cfg(target_os = "windows")]
mod windows_chrome {
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GWLP_WNDPROC, GetWindowLongPtrW, HTTRANSPARENT,
        MA_NOACTIVATE, SetWindowLongPtrW, WM_MOUSEACTIVATE, WM_NCDESTROY, WM_NCHITTEST, WNDPROC,
    };

    pub(super) type Hwnd = HWND;

    /// GPUI's own window procedure for each popup window we subclassed, kept as
    /// a raw address because a `WNDPROC` is not `Send`. Entries are added on the
    /// main thread at window creation and dropped on WM_NCDESTROY; only a few
    /// popups exist at a time, so this stays tiny.
    static CHAINED_WINDOW_PROCS: Mutex<Vec<(isize, isize)>> = Mutex::new(Vec::new());
    /// The subclassed pop-ups the pointer passes through (`make_popup_window_click_through`).
    static CLICK_THROUGH_WINDOWS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

    pub(super) fn make_popup_window_click_through(hwnd: Hwnd) {
        make_popup_window_non_activating(hwnd);
        let mut windows = click_through_windows();
        if !windows.contains(&(hwnd as isize)) {
            windows.push(hwnd as isize);
        }
    }

    fn click_through_windows() -> std::sync::MutexGuard<'static, Vec<isize>> {
        CLICK_THROUGH_WINDOWS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// CDXC:PlatformSupport 2026-11-19 WHY:
    /// An owned pop-up sits in its owner's band. Windows keeps an owned window above its owner only inside one band, so a non-topmost pop-up owned by a topmost window (a pinned session window, a capture window, or the floating page window once a move had made it topmost) stacked behind its owner: the page window's ⋯ menu opened hidden behind the page and the press seemed to do nothing on Windows. Under a topmost owner the pop-up is topmost too, at the top of that band, which is right over the owner.
    pub(super) fn own_popup_window(popup: Hwnd, owner: Hwnd) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GW_HWNDPREV, GW_OWNER, GWL_EXSTYLE, GWLP_HWNDPARENT, GetWindow, HWND_NOTOPMOST,
            HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowPos,
            WS_EX_TOPMOST,
        };
        if owner.is_null() || popup == owner {
            return;
        }
        let topmost = |hwnd: Hwnd| {
            (unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST) != 0
        };
        let owner_topmost = topmost(owner);
        let owned = unsafe { GetWindow(popup, GW_OWNER) } == owner;
        if owned && topmost(popup) == owner_topmost {
            return;
        }
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        if owner_topmost {
            unsafe {
                SetWindowLongPtrW(popup, GWLP_HWNDPARENT, owner as isize);
                SetWindowPos(popup, HWND_TOPMOST, 0, 0, 0, 0, flags);
            }
            return;
        }
        // The window just above the owner, read before the pop-up moves, so the pop-up can be
        // stacked right over the owner instead of over whatever app is in front. When that window
        // is topmost (or there is none), the owner leads the normal band and the top of that band,
        // where `HWND_NOTOPMOST` puts the pop-up, is already right over it.
        let above_owner = unsafe { GetWindow(owner, GW_HWNDPREV) };
        unsafe {
            SetWindowLongPtrW(popup, GWLP_HWNDPARENT, owner as isize);
            SetWindowPos(popup, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
            if !above_owner.is_null() && above_owner != popup && !topmost(above_owner) {
                SetWindowPos(popup, above_owner, 0, 0, 0, 0, flags);
            }
        }
    }

    pub(super) fn make_popup_window_non_activating(hwnd: Hwnd) {
        let ours: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
            ghostex_titlebar_popup_window_proc;
        let ours = ours as usize as isize;
        if unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) } == ours {
            return;
        }
        let previous = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours) };
        if previous == 0 {
            return;
        }
        let mut chained = chained_window_procs();
        chained.retain(|(subclassed, _)| *subclassed != hwnd as isize);
        chained.push((hwnd as isize, previous));
    }

    unsafe extern "system" fn ghostex_titlebar_popup_window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_MOUSEACTIVATE {
            return MA_NOACTIVATE as LRESULT;
        }
        if msg == WM_NCHITTEST && click_through_windows().contains(&(hwnd as isize)) {
            return HTTRANSPARENT as LRESULT;
        }
        if msg == WM_NCDESTROY {
            click_through_windows().retain(|window| *window != hwnd as isize);
        }
        let previous = take_or_read_chained_window_proc(hwnd, msg == WM_NCDESTROY);
        match previous {
            Some(previous) => unsafe { CallWindowProcW(Some(previous), hwnd, msg, wparam, lparam) },
            None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    /// Resolves the chained procedure, releasing the lock before it is called so
    /// a re-entrant message from inside GPUI's procedure cannot deadlock.
    fn take_or_read_chained_window_proc(
        hwnd: HWND,
        forget: bool,
    ) -> Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT> {
        let raw = {
            let mut chained = chained_window_procs();
            let index = chained
                .iter()
                .position(|(subclassed, _)| *subclassed == hwnd as isize)?;
            if forget {
                chained.swap_remove(index).1
            } else {
                chained[index].1
            }
        };
        let proc: WNDPROC = unsafe { std::mem::transmute::<isize, WNDPROC>(raw) };
        proc
    }

    fn chained_window_procs() -> std::sync::MutexGuard<'static, Vec<(isize, isize)>> {
        CHAINED_WINDOW_PROCS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
