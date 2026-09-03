/*
CDXC:CefRuntime 2026-07-04:
Windows platform adapter for the shared windowed-CEF backend (cef/shell.rs).
This module owns only the truly per-OS pieces: the helper-exe subprocess
path, a message-only HWND that turns CEF's on_schedule_message_pump_work
callbacks into main-thread cef::do_message_loop_work() steps, and child-HWND
frame/visibility/focus operations. All browser/bridge/runtime logic stays
OS-agnostic in cef/shell.rs. Handles cross this seam as opaque `*mut c_void`;
only this file treats them as HWNDs.

Written without Windows hardware (P2 best-effort bring-up): the pump-state
machine mirrors apps/desktop/native/macos/GpuiCefAppKitHooks.m semantics 1:1 except
that SetTimer/KillTimer replace the uncancellable dispatch_after generation
counter. Runtime behavior needs device verification.
*/

use anyhow::{Context as _, Result};
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GA_ROOT, GetAncestor, HWND_MESSAGE, HWND_TOP,
    IsChild, IsWindow, KillTimer, PostMessageW, RegisterClassW, SW_HIDE, SW_SHOWNA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetTimer, SetWindowPos, ShowWindow,
    USER_DEFAULT_SCREEN_DPI, WM_APP, WM_TIMER, WNDCLASSW,
};

const PUMP_WINDOW_CLASS_NAME: &str = "GhostexGpuiCefMessagePump";
const PUMP_TIMER_ID: usize = 1;
/// Private pump-window message: LPARAM carries the CEF-requested delay in ms.
const WM_GHOSTEX_SCHEDULE_PUMP_WORK: u32 = WM_APP + 0x47;
/// Matches GhostexGpuiCEFMessagePumpPlaceholderDelayMs in the macOS shim.
const PUMP_PLACEHOLDER_DELAY_MS: i64 = i32::MAX as i64;
/// Matches GhostexGpuiCEFMessagePumpImmediateTimerDelayMs in the macOS shim.
const PUMP_IMMEDIATE_TIMER_DELAY_MS: i64 = 1000 / 120;
/// Matches GhostexGpuiCEFMessagePumpMaxTimerDelayMs in the macOS shim.
const PUMP_MAX_TIMER_DELAY_MS: i64 = 1000 / 30;

/// Pump HWND as usize (0 = not created). Written on the main thread during
/// install; read from any thread by schedule_message_pump_work (PostMessageW
/// is the only cross-thread operation, and it is thread-safe by contract).
static PUMP_HWND: AtomicUsize = AtomicUsize::new(0);
static PUMP_INSTALLED: AtomicBool = AtomicBool::new(false);
static PUMP_WORK_PENDING: AtomicBool = AtomicBool::new(false);
static PUMP_WORK_ACTIVE: AtomicBool = AtomicBool::new(false);
static PUMP_REENTRANCY_DETECTED: AtomicBool = AtomicBool::new(false);
static PUMP_DISPATCH_PENDING: AtomicBool = AtomicBool::new(false);
static PUMP_DISPATCH_DELAY_MS: Mutex<i64> = Mutex::new(PUMP_PLACEHOLDER_DELAY_MS);

/// The release bootstrap starts this runtime with the verified component
/// directory on PATH, allowing the Windows loader to resolve libcef.dll before
/// Rust enters main. Development layouts keep the DLL beside the executable.
pub(super) struct PlatformCefRuntime;

pub(super) fn load_cef_runtime() -> Result<PlatformCefRuntime> {
    let runtime_dir = std::env::var_os(crate::cef_component_window::CEF_RUNTIME_DIR_ENV)
        .map(std::path::PathBuf::from)
        .context("verified CEF runtime directory is not configured")?;
    let library = runtime_dir.join("libcef.dll");
    if !library.is_file() {
        anyhow::bail!("verified CEF runtime is missing {}", library.display());
    }
    if crate::shared_settings::shared_sidebar_settings_snapshot().debugging_mode()
        && crate::support_logs::scenario_id_enabled("native.host.lifecycle")
    {
        eprintln!(
            "Ghostex CEF runtime: verified Windows component {}",
            runtime_dir.display()
        );
    }
    Ok(PlatformCefRuntime)
}

pub(super) fn prepare_application() {
    // macOS disables AppKit crash-state restoration here; Windows has no
    // equivalent process-level state to prepare before CEF touches it.
}

pub(super) fn system_uses_dark_page_appearance() -> bool {
    false
}

pub(super) fn install_application_hooks() {
    // The macOS CefAppProtocol/sendEvent swizzle and Edit-menu install have
    // no Windows counterpart: Chromium's Windows message pump needs no host
    // application protocol, and edit-command dispatch (Ctrl+A/C/V/X) reaches
    // the focused Chromium child HWND through normal Win32 key routing.
}

pub(super) fn install_message_pump(_cx: &gpui::App) {
    // The GPUI app context is unused here: PostMessageW to the message-only
    // pump HWND is the OS-level main-thread scheduler, so the pump needs no
    // gpui executor (unlike Linux, where gpui's foreground executor is the
    // only way into the main event loop).
    if PUMP_INSTALLED.load(Ordering::SeqCst) {
        return;
    }

    ensure_pump_window();
    PUMP_WORK_PENDING.store(false, Ordering::SeqCst);
    PUMP_WORK_ACTIVE.store(false, Ordering::SeqCst);
    PUMP_REENTRANCY_DETECTED.store(false, Ordering::SeqCst);
    PUMP_DISPATCH_PENDING.store(false, Ordering::SeqCst);
    PUMP_INSTALLED.store(true, Ordering::SeqCst);
}

pub(super) fn invalidate_message_pump() {
    PUMP_INSTALLED.store(false, Ordering::SeqCst);
    PUMP_WORK_PENDING.store(false, Ordering::SeqCst);
    PUMP_DISPATCH_PENDING.store(false, Ordering::SeqCst);
    let hwnd = pump_hwnd();
    if !hwnd.is_null() {
        unsafe {
            KillTimer(hwnd, PUMP_TIMER_ID);
        }
    }
}

pub(super) fn schedule_message_pump_work(delay_ms: i64) {
    let Ok(mut pending_delay_ms) = PUMP_DISPATCH_DELAY_MS.lock() else {
        return;
    };
    let should_post = !PUMP_DISPATCH_PENDING.load(Ordering::SeqCst);
    if should_post {
        *pending_delay_ms = delay_ms;
        PUMP_DISPATCH_PENDING.store(true, Ordering::SeqCst);
    } else if delay_ms != PUMP_PLACEHOLDER_DELAY_MS {
        *pending_delay_ms = delay_ms;
    }
    drop(pending_delay_ms);
    if !should_post {
        return;
    }

    let hwnd = pump_hwnd();
    if hwnd.is_null() {
        PUMP_DISPATCH_PENDING.store(false, Ordering::SeqCst);
        return;
    }
    unsafe {
        PostMessageW(
            hwnd,
            WM_GHOSTEX_SCHEDULE_PUMP_WORK,
            0 as WPARAM,
            0 as LPARAM,
        );
    }
}

pub(super) fn apply_platform_settings(settings: &mut cef::Settings) {
    /*
    On macOS the bundle layout discovers the helper apps; on Windows CEF
    re-launches the main executable for subprocesses unless
    browser_subprocess_path points at the dedicated helper, so the packaged
    layout must place ghostex-gpui-cef-helper.exe beside the main exe.
    */
    let executable =
        std::env::current_exe().expect("failed to resolve GPUI executable path for CEF helper");
    let helper = executable
        .parent()
        .expect("GPUI executable path has no parent directory")
        .join("ghostex-gpui-cef-helper.exe");
    settings.browser_subprocess_path = cef::CefString::from(helper.to_string_lossy().as_ref());
    let runtime_dir = std::env::var_os(crate::cef_component_window::CEF_RUNTIME_DIR_ENV)
        .map(std::path::PathBuf::from)
        .expect("verified CEF runtime directory must be configured before CEF initialization");
    settings.resources_dir_path = cef::CefString::from(runtime_dir.to_string_lossy().as_ref());
    settings.locales_dir_path =
        cef::CefString::from(runtime_dir.join("locales").to_string_lossy().as_ref());
}

pub(super) fn append_platform_command_line_switches(_command_line: &mut cef::CommandLine) {
    // Windows needs no OS-specific Chromium switches beyond the shared set
    // in cef/shell.rs; Ozone platform selection is a Linux-only concern.
}

pub(super) fn child_window_info(
    parent_native_view: *mut c_void,
    bounds: &cef::Rect,
) -> cef::WindowInfo {
    /*
    CEF's default runtime style is Chrome when the Chrome runtime is enabled.
    On Windows that style ignores the client-provided child HWND contract and
    creates a separate top-level Chromium window with browser chrome. Embedded
    Ghostex panes are Alloy child windows, so select that style explicitly
    rather than relying on CEF's platform-dependent default.
    */
    let mut window_info =
        cef::WindowInfo::default().set_as_child(cef::sys::HWND(parent_native_view.cast()), bounds);
    window_info.runtime_style = cef::RuntimeStyle::ALLOY;
    window_info
}

pub(super) fn native_view_ptr(handle: cef::sys::cef_window_handle_t) -> *mut c_void {
    handle.0.cast()
}

pub(super) fn prepare_native_view_for_focus(_native_view: *mut c_void) {
    // The macOS focus subclass exists to route AppKit first-responder and
    // command-key dispatch into the exact CEF NSView. Win32 keyboard focus
    // already follows the clicked Chromium child HWND, and select-all runs
    // inside Chromium's own accelerator handling, so no per-view setup is
    // needed here.
}

pub(super) fn set_native_view_mouse_focus_passive(_native_view: *mut c_void, _passive: bool) {
    // Mouse-focus passivity is an AppKit first-responder policy; Win32
    // keyboard focus routing for the sidebar is not implemented here yet.
}

pub(super) fn set_native_view_passive_focus_grant(_native_view: *mut c_void, _granted: bool) {}

pub(super) fn return_focus_to_gpui_root(native_view: *mut c_void) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.is_null() {
        return;
    }
    super::shell::clear_active_native_view();
    unsafe {
        SetFocus(root);
    }
}

pub(super) fn set_native_view_frame(
    native_view: *mut c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    _scale_factor: f32,
) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    /*
    The shared shell passes gpui logical pixels with a top-left origin. Child
    HWND placement is physical pixels (also top-left origin), so scale by the
    window's DPI; the browser HWND inherits its parent window's DPI. The
    GPUI-provided scale factor is intentionally unused: GetDpiForWindow is
    the authoritative per-window value on Windows and gpui derives its own
    scale from the same source.
    */
    let scale = unsafe { GetDpiForWindow(hwnd) } as f64 / USER_DEFAULT_SCREEN_DPI as f64;
    let scale = if scale > 0.0 { scale } else { 1.0 };
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            (x * scale).round() as i32,
            (y * scale).round() as i32,
            ((width * scale).round().max(0.0)) as i32,
            ((height * scale).round().max(0.0)) as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub(super) fn log_resize_diagnostic(
    _browser_id: i32,
    _width: i32,
    _height: i32,
    _frame_us: u64,
    _was_resized_us: u64,
    _total_us: u64,
) {
}

pub(super) fn set_native_view_visible(native_view: *mut c_void, visible: bool) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    // SW_SHOWNA mirrors NSView.hidden = NO: reveal without stealing
    // activation or keyboard focus from GPUI chrome.
    unsafe {
        ShowWindow(hwnd, if visible { SW_SHOWNA } else { SW_HIDE });
    }
}

pub(super) fn order_native_view_front(native_view: *mut c_void) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    // Mirrors the macOS reorder above all current siblings: dropdown CEF
    // panels are reused across opens while other child windows keep being
    // created, so showing one must re-assert its top position.
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub(super) fn focus_native_view(native_view: *mut c_void) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    // Mirrors makeFirstResponder on macOS; the shared shell follows up with
    // host.set_focus(1) so Chromium moves focus to its inner widget HWND.
    // Record the exact registered browser root before SetFocus so the shared
    // focus guard recognizes this app-owned Windows handoff just as it does
    // the AppKit mouse/focus hook on macOS.
    super::shell::mark_native_view_focused(native_view);
    unsafe {
        SetFocus(hwnd);
    }
}

pub(super) fn focus_gpui_root_view(native_view: *mut c_void) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    super::shell::clear_active_native_view();
    unsafe {
        SetFocus(hwnd);
    }
}

pub(super) fn native_view_owns_first_responder(native_view: *mut c_void) -> bool {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return false;
    }
    let focused = unsafe { GetFocus() };
    !focused.is_null() && (focused == hwnd || unsafe { IsChild(hwnd, focused) } != 0)
}

pub(super) fn native_view_has_direct_focus(native_view: *mut c_void) -> bool {
    let hwnd: HWND = native_view.cast();
    !hwnd.is_null() && unsafe { GetFocus() == hwnd }
}

pub(super) fn release_native_view(native_view: *mut c_void) {
    /*
    CDXC:CefRuntime 2026-08-24:
    GhostexGpuiLifeSpanHandler::do_close returns handled, so per
    cef_life_span_handler.h's DoClose docs the app is still required to complete
    the close by proceeding with window-hierarchy tear-down. Without it the
    browser stays partially closed, on_before_close never fires, and every
    closed surface keeps a live renderer subprocess. DestroyWindow on the CEF
    child HWND is that tear-down: WM_DESTROY reaches Chromium's own window
    procedure and lets CEF finish the close, mirroring what macOS does by
    removing the CEF child NSView from its superview and Linux by destroying its
    embed-host X window. `native_view` is the CEF-created child window CEF
    reported through native_view_ptr — the same handle every other per-surface
    function in this file drives — so this destroys the browser's child tree
    (DestroyWindow tears down child windows depth-first) and never the parent
    GPUI window.

    Threading: this runs from `impl Drop for CefBrowser` on the main/UI thread,
    which is the thread that owns this window — the same thread assumption the
    pump window above documents. DestroyWindow must be called from the window's
    creating thread, and that requirement is satisfied here.
    */
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    unsafe {
        // Win32 recycles HWND values. If the parent GPUI window was destroyed
        // first, Win32 already destroyed this child and the handle may now name
        // an unrelated window; destroy only a handle that is still a live
        // window.
        if IsWindow(hwnd) == 0 {
            return;
        }
        DestroyWindow(hwnd);
    }
}

fn pump_hwnd() -> HWND {
    PUMP_HWND.load(Ordering::SeqCst) as HWND
}

fn ensure_pump_window() {
    if !pump_hwnd().is_null() {
        return;
    }

    let class_name: Vec<u16> = PUMP_WINDOW_CLASS_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let window_class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(pump_window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        // Registration fails harmlessly if the class already exists (pump
        // reinstall after invalidate); CreateWindowExW resolves it by name.
        RegisterClassW(&window_class);
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        PUMP_HWND.store(hwnd as usize, Ordering::SeqCst);
    }
}

unsafe extern "system" fn pump_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_GHOSTEX_SCHEDULE_PUMP_WORK => {
            let delay_ms = if let Ok(delay_ms) = PUMP_DISPATCH_DELAY_MS.lock() {
                let delay_ms = *delay_ms;
                PUMP_DISPATCH_PENDING.store(false, Ordering::SeqCst);
                delay_ms
            } else {
                PUMP_DISPATCH_PENDING.store(false, Ordering::SeqCst);
                PUMP_PLACEHOLDER_DELAY_MS
            };
            on_schedule_message_pump_work(hwnd, delay_ms);
            0
        }
        WM_TIMER if wparam == PUMP_TIMER_ID => {
            unsafe {
                KillTimer(hwnd, PUMP_TIMER_ID);
            }
            if PUMP_INSTALLED.load(Ordering::SeqCst) && PUMP_WORK_PENDING.load(Ordering::SeqCst) {
                PUMP_WORK_PENDING.store(false, Ordering::SeqCst);
                run_scheduled_message_pump_work();
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn on_schedule_message_pump_work(hwnd: HWND, delay_ms: i64) {
    if !PUMP_INSTALLED.load(Ordering::SeqCst) {
        return;
    }

    if delay_ms == PUMP_PLACEHOLDER_DELAY_MS && PUMP_WORK_PENDING.load(Ordering::SeqCst) {
        return;
    }

    // Unlike dispatch_after on macOS, a pending SetTimer is cancellable, so
    // the generation counter from the AppKit shim is unnecessary here.
    PUMP_WORK_PENDING.store(false, Ordering::SeqCst);
    unsafe {
        KillTimer(hwnd, PUMP_TIMER_ID);
    }

    let clamped_delay_ms = if delay_ms <= 0 {
        PUMP_IMMEDIATE_TIMER_DELAY_MS
    } else {
        delay_ms.min(PUMP_MAX_TIMER_DELAY_MS)
    };
    PUMP_WORK_PENDING.store(true, Ordering::SeqCst);
    unsafe {
        SetTimer(hwnd, PUMP_TIMER_ID, clamped_delay_ms as u32, None);
    }
}

fn run_scheduled_message_pump_work() {
    if !PUMP_INSTALLED.load(Ordering::SeqCst) {
        return;
    }

    let was_reentrant = perform_message_loop_work();
    if was_reentrant {
        schedule_message_pump_work(0);
    } else if !PUMP_WORK_PENDING.load(Ordering::SeqCst) {
        schedule_message_pump_work(PUMP_PLACEHOLDER_DELAY_MS);
    }
}

fn perform_message_loop_work() -> bool {
    if PUMP_WORK_ACTIVE.load(Ordering::SeqCst) {
        PUMP_REENTRANCY_DETECTED.store(true, Ordering::SeqCst);
        return false;
    }

    PUMP_REENTRANCY_DETECTED.store(false, Ordering::SeqCst);
    PUMP_WORK_ACTIVE.store(true, Ordering::SeqCst);
    cef::do_message_loop_work();
    PUMP_WORK_ACTIVE.store(false, Ordering::SeqCst);

    PUMP_REENTRANCY_DETECTED.load(Ordering::SeqCst)
}
