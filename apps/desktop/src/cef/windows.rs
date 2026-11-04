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

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateSolidBrush, DCX_CACHE, DeleteObject, FillRect, GetDCEx, HBRUSH, ReleaseDC,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, GetFocus, SetFocus};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GA_ROOT, GWL_EXSTYLE, GetAncestor,
    GetClientRect, GetWindowLongPtrW, HWND_MESSAGE, HWND_TOP, IsChild, IsWindow, KillTimer,
    LWA_ALPHA, PostMessageW, RegisterClassW, SW_HIDE, SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, USER_DEFAULT_SCREEN_DPI, WM_APP, WM_NCDESTROY, WM_SIZE, WM_TIMER,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP,
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
/// Subclass id of the page-background painter on a CEF host window.
const HOST_BACKGROUND_SUBCLASS_ID: usize = 0x4758_4247;

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

/// libcef.dll, loaded from the verified runtime folder for the rest of the process.
pub(super) struct PlatformCefRuntime;

/// CDXC:CefRuntime 2026-09-28 WHY:
/// ghostex-gpui.exe delay-loads libcef.dll (apps/desktop/build.rs) so it starts without the optional web runtime. Before the first CEF call this loads the installed component's libcef.dll by full path with LOAD_WITH_ALTERED_SEARCH_PATH, so its own imports (chrome_elf.dll) resolve from that folder and the delay-load helper then finds the module already loaded. The folder also goes first on PATH, for the helper subprocess (load-time linked) and anything Chromium loads by name, which is exactly what the bootstrap arranged before it stopped installing CEF; SetDllDirectoryW is not used because it would put Chromium's DLLs ahead of System32 for the whole app. Development layouts keep the DLL beside the executable, which is the same path.
pub(super) fn load_cef_runtime() -> Result<PlatformCefRuntime> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::System::LibraryLoader::{
        LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
    };

    let runtime_dir = std::env::var_os(crate::cef_component_window::CEF_RUNTIME_DIR_ENV)
        .map(std::path::PathBuf::from)
        .context("verified CEF runtime directory is not configured")?;
    let library = runtime_dir.join("libcef.dll");
    if !library.is_file() {
        anyhow::bail!("verified CEF runtime is missing {}", library.display());
    }
    let library_wide = library
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    // SAFETY: the path is a NUL-terminated UTF-16 string that outlives the call; the module
    // handle is never freed, because CEF stays loaded until the process exits.
    unsafe {
        if LoadLibraryExW(
            library_wide.as_ptr(),
            std::ptr::null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
        .is_null()
        {
            anyhow::bail!(
                "Windows could not load {}: {}",
                library.display(),
                std::io::Error::last_os_error()
            );
        }
    }
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    if !std::env::split_paths(&current_path).any(|path| path == runtime_dir) {
        let joined = std::env::join_paths(
            std::iter::once(runtime_dir.clone()).chain(std::env::split_paths(&current_path)),
        )
        .context("the CEF runtime folder cannot be added to PATH")?;
        // SAFETY: runs on the main thread inside cef::initialize, before CEF starts its own
        // threads, and Windows serialises environment reads and writes itself.
        unsafe { std::env::set_var("PATH", joined) };
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

pub(super) fn install_application_hooks() {
    // The macOS CefAppProtocol/sendEvent swizzle and Edit-menu install have
    // no Windows counterpart: Chromium's Windows message pump needs no host
    // application protocol, and edit-command dispatch (Ctrl+A/C/V/X) reaches
    // the focused Chromium child HWND through normal Win32 key routing.
}

/// Brings this process's main window to the front when Ghostex is launched while it already runs: the largest
/// visible top-level window without an owner, which skips app modals attached to the main window.
pub(super) fn activate_running_app() {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, SW_RESTORE, SetForegroundWindow,
    };
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `best` tuple below, alive for the whole EnumWindows call.
        let best = unsafe { &mut *(lparam as *mut (HWND, i64)) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid != std::process::id()
            || unsafe { IsWindowVisible(hwnd) } == 0
            || !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null()
        {
            return 1;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe { GetWindowRect(hwnd, &mut rect) };
        let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
        if area > best.1 {
            *best = (hwnd, area);
        }
        1
    }
    let mut best: (HWND, i64) = (std::ptr::null_mut(), 0);
    // SAFETY: `visit` only reads windows and writes through the pointer to `best`.
    unsafe { EnumWindows(Some(visit), &mut best as *mut (HWND, i64) as LPARAM) };
    if best.0.is_null() {
        return;
    }
    // SAFETY: `best.0` is a live top-level window of this process found above.
    unsafe {
        if IsIconic(best.0) != 0 {
            ShowWindow(best.0, SW_RESTORE);
        }
        SetForegroundWindow(best.0);
    }
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

/// Runs CEF's start now: the external pump installed before it drives CEF here. Only macOS defers
/// it into Chromium's own message loop.
pub(super) fn start_cef(start: Box<dyn FnOnce() -> anyhow::Result<()>>) -> anyhow::Result<()> {
    start()
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

/// CDXC:CefRuntime 2026-10-07 WHY:
/// Without overlay scrollbars Chromium on Windows draws a scrollbar with a solid track and arrow buttons, and an element scroller on a dark page that does not declare `color-scheme: dark` (most HTML files opened in Files, most websites) gets a white track. Chromium's `OverlayScrollbar` feature (chrome://flags#overlay-scrollbars) draws the thin Fluent overlay thumb with no track instead, close to macOS, for every CEF page at once (Files, browser tabs, the Code view, extension views) without injecting CSS into anyone's page. The old `FluentScrollbar` / `FluentOverlayScrollbar` feature names are gone in Chromium 154 (Fluent is always on), so this is the only switch that does it. SEE-ALSO: cef/linux_x11.rs sets the same feature.
pub(super) fn append_platform_command_line_switches(command_line: &mut cef::CommandLine) {
    super::shell::append_chromium_feature(command_line, "enable-features", "OverlayScrollbar");
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
    ensure_child_composited_by_dwm(hwnd);
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

/// CDXC:Theming 2026-09-25 WHY:
/// A main window opened with window glass presents through DirectComposition (`WS_EX_NOREDIRECTIONBITMAP`), which has no redirection surface for ordinary child HWNDs to draw into, so windowed CEF pages would stay black (the 2026-07-25 PlatformSupport note in main.rs). Since Windows 8 a child window may be layered, and a layered child gets a redirection surface of its own that DWM composites above its parent, so each CEF child is made layered and fully opaque when its root window has no redirection surface. Opaque main windows keep plain children.
fn ensure_child_composited_by_dwm(hwnd: HWND) {
    unsafe {
        let root = GetAncestor(hwnd, GA_ROOT);
        if root.is_null() {
            return;
        }
        let root_style = GetWindowLongPtrW(root, GWL_EXSTYLE) as u32;
        if root_style & WS_EX_NOREDIRECTIONBITMAP == 0 {
            return;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_EX_LAYERED != 0 {
            return;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (style | WS_EX_LAYERED) as isize);
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
    }
}

/// CDXC:CefRuntime 2026-10-26 WHY:
/// The layered surface `ensure_child_composited_by_dwm` gives a CEF host is seeded with whatever is on screen under the window each time it grows, and CEF never paints its host (`CefBrowserWindow` has no class brush and answers WM_ERASEBKGND with 0), so on window glass a new Browser tab showed those stale screen pixels (a stray frame over an empty area) until Chromium's first frame covered it (seen 2026-10-26 on Windows). The host paints itself in the page's own background colour (the colour Chromium starts from, `background_color` in `CefBrowser::new`) after every size change. It paints on WM_SIZE with a DC that ignores child windows because Chromium's widget covers the whole host: WM_ERASEBKGND never reaches the host, and a style-clipped DC would paint nothing. Chromium's own frames stay above this surface. Hosts of an opaque main window draw into the window's own surface and are left alone.
pub(super) fn set_native_view_background_color(native_view: *mut c_void, argb: u32) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    let colorref = ((argb >> 16) & 0xff) | (argb & 0xff00) | ((argb & 0xff) << 16);
    unsafe {
        let brush = CreateSolidBrush(colorref);
        if brush.is_null() {
            return;
        }
        if SetWindowSubclass(
            hwnd,
            Some(host_background_subclass_proc),
            HOST_BACKGROUND_SUBCLASS_ID,
            brush as usize,
        ) == 0
        {
            DeleteObject(brush);
        }
    }
}

unsafe extern "system" fn host_background_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    brush: usize,
) -> LRESULT {
    match message {
        WM_SIZE => {
            // CEF resizes its widget to the new client size first.
            let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            paint_host_background(hwnd, brush as HBRUSH);
            result
        }
        WM_NCDESTROY => unsafe {
            RemoveWindowSubclass(
                hwnd,
                Some(host_background_subclass_proc),
                HOST_BACKGROUND_SUBCLASS_ID,
            );
            DeleteObject(brush as HBRUSH);
            DefSubclassProc(hwnd, message, wparam, lparam)
        },
        _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
    }
}

fn paint_host_background(hwnd: HWND, brush: HBRUSH) {
    unsafe {
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED == 0 {
            return;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetClientRect(hwnd, &mut rect);
        let dc = GetDCEx(hwnd, std::ptr::null_mut(), DCX_CACHE);
        if dc.is_null() {
            return;
        }
        FillRect(dc, &rect, brush);
        ReleaseDC(hwnd, dc);
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

/// A panel slide only needs the page out of sight for its few frames; it comes back without a fade.
pub(super) fn set_native_view_motion_hidden(
    native_view: *mut c_void,
    hidden: bool,
    _fade_in_seconds: f64,
) {
    set_native_view_visible(native_view, !hidden);
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

/// Not read on Windows: a Code window switch there waits out its fixed hold.
pub(super) fn native_view_content_identity(_native_view: *mut c_void) -> u64 {
    0
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
    // CDXC:FocusRouting 2026-09-14 WHY:
    // A mouse click already focuses Chromium's inner widget on Windows. Moving focus to its browser root and back emits a window blur that immediately dismisses sidebar agent menus.
    // Preserve the focused descendant while recording the app-owned grant.
    if native_view_owns_first_responder(native_view) {
        return;
    }
    unsafe {
        SetFocus(hwnd);
    }
}

pub(super) fn focus_gpui_root_view(native_view: *mut c_void) {
    let hwnd: HWND = native_view.cast();
    if hwnd.is_null() {
        return;
    }
    /*
    CDXC:FocusRouting 2026-10-09 WHY:
    `SetFocus` on a window that is not active activates it, so this in-window handoff (meant to pull the keys out of a Chromium child) moved the keyboard across windows. With an app modal open, the chat composer the closing sidebar menu had refocused reported its focus a moment later and this call activated the main window: Rename Session's selected title lost the caret to the chat box, and History (Quick Access), which closes when it loses focus, vanished after a split second. Like macOS's `makeFirstResponder`, it now only moves focus inside a window that is already active (or when no window of the app is); the window's own activation (a click, the modal closing) brings the keys back.
    */
    let active = unsafe { GetActiveWindow() };
    if !active.is_null() && active != unsafe { GetAncestor(hwnd, GA_ROOT) } {
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
