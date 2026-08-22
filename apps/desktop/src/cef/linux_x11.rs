/*
CDXC:GPUICefPlatformSeam 2026-07-04:
Linux (X11) platform adapter for the shared windowed-CEF backend
(cef/shell.rs). This module owns only the truly per-OS pieces: turning CEF's
on_schedule_message_pump_work callbacks into main-thread
cef::do_message_loop_work() steps via gpui's foreground executor, child
X11-window frame/visibility/focus operations through x11rb, the helper-exe
subprocess path, and the `--ozone-platform=x11` Chromium switch. All
browser/bridge/runtime logic stays OS-agnostic in cef/shell.rs. Handles
cross this seam as opaque `*mut c_void`; only this file treats them as X11
window ids.

X11 is an app-wide constraint on Linux, not a per-pane choice: CEF child
windows can only be reparented into an X11 window, so the GPUI shell itself
must run gpui's X11 backend (forced in main.rs before Application creation)
and Chromium's Ozone layer must match. Under Wayland desktops everything
runs through XWayland, which trades away fractional-scaling sharpness and
some IME fidelity — accepted v1 trade-offs until browser OSR unlocks a
native-Wayland shell (plan Phase 4).

x11rb is the deliberate X library choice: gpui's own X11 backend already
pulls it into the Linux dependency tree (same 0.13 major), it speaks the X
protocol directly over its own connection (no libX11/libxcb link-time
dependency), and the four requests this adapter needs (ConfigureWindow,
MapWindow, UnmapWindow, SetInputFocus) are core protocol.

Written without Linux hardware (P3 best-effort bring-up): the pump-state
machine mirrors gpui/native/macos/GpuiCefAppKitHooks.m semantics 1:1 except
that a gpui foreground task with a cancellable deadline replaces the
uncancellable dispatch_after generation counter.

CDXC:GPUILinuxX11Backend 2026-07-05: device-verified on Ubuntu 26.04
(XWayland/KDE). Two Linux-only requirements surfaced and live in this file:
CEF child browsers need a depth-matched intermediate embed-host window under
gpui's 32-bit ARGB window (see child_window_info), and the external message
pump must dispatch the default GMainContext because Chromium's UI-thread X11
event source is glib-hosted (see dispatch_pending_glib_main_context_sources).
*/

use anyhow::{Context as _, Result};
use futures::{FutureExt as _, StreamExt as _, channel::mpsc};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{
    ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, InputFocus, StackMode,
    Window as X11Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// Matches GhostexGpuiCEFMessagePumpPlaceholderDelayMs in the macOS shim.
const PUMP_PLACEHOLDER_DELAY_MS: i64 = i32::MAX as i64;
/// Matches GhostexGpuiCEFMessagePumpImmediateTimerDelayMs in the macOS shim.
const PUMP_IMMEDIATE_TIMER_DELAY_MS: i64 = 1000 / 120;
/// Matches GhostexGpuiCEFMessagePumpMaxTimerDelayMs in the macOS shim.
const PUMP_MAX_TIMER_DELAY_MS: i64 = 1000 / 30;

/// Requested pump delays, sent from any thread by CEF's
/// on_schedule_message_pump_work and consumed by the main-thread driver
/// task. The sender is the only cross-thread pump entry point.
static PUMP_SENDER: OnceLock<mpsc::UnboundedSender<()>> = OnceLock::new();
static PUMP_DISPATCH_PENDING: AtomicBool = AtomicBool::new(false);
static PUMP_DISPATCH_DELAY_MS: Mutex<i64> = Mutex::new(PUMP_PLACEHOLDER_DELAY_MS);
static PUMP_INSTALLED: AtomicBool = AtomicBool::new(false);
static PUMP_WORK_PENDING: AtomicBool = AtomicBool::new(false);
static PUMP_WORK_ACTIVE: AtomicBool = AtomicBool::new(false);
static PUMP_REENTRANCY_DETECTED: AtomicBool = AtomicBool::new(false);

/// The release bootstrap starts this runtime with the verified component
/// directory on LD_LIBRARY_PATH, allowing the ELF loader to resolve libcef.so
/// before Rust enters main. Development layouts keep the library beside the
/// executable and use the existing $ORIGIN rpath.
pub(super) struct PlatformCefRuntime;

pub(super) fn load_cef_runtime() -> Result<PlatformCefRuntime> {
    let runtime_dir = std::env::var_os(crate::cef_component_window::CEF_RUNTIME_DIR_ENV)
        .map(std::path::PathBuf::from)
        .context("verified CEF runtime directory is not configured")?;
    let library = runtime_dir.join("libcef.so");
    if !library.is_file() {
        anyhow::bail!("verified CEF runtime is missing {}", library.display());
    }
    if crate::shared_settings::shared_sidebar_settings_snapshot().debugging_mode()
        && crate::support_logs::scenario_id_enabled("native.host.lifecycle")
    {
        eprintln!(
            "Ghostex CEF runtime: verified Linux component {}",
            runtime_dir.display()
        );
    }
    Ok(PlatformCefRuntime)
}

pub(super) fn prepare_application() {
    // macOS disables AppKit crash-state restoration here. The Linux
    // process-level preparation — forcing gpui's X11 backend before the
    // Application exists — lives in main.rs next to the
    // gpui_platform::application() call it steers, so nothing remains to do
    // at the CEF layer.
}

pub(super) fn system_uses_dark_page_appearance() -> bool {
    false
}

pub(super) fn install_application_hooks() {
    // The macOS CefAppProtocol/sendEvent swizzle and Edit-menu install have
    // no Linux counterpart: Chromium integrates with X11 directly (it opens
    // its own display connection and installs its own X error handlers), and
    // edit-command dispatch reaches the focused Chromium child window
    // through normal X11 key routing.
}

pub(super) fn install_message_pump(cx: &gpui::App) {
    if PUMP_INSTALLED.load(Ordering::SeqCst) {
        return;
    }

    /*
    Unlike macOS (GCD main queue) and Windows (message-only HWND), Linux has
    no OS-level "run this on the main thread" primitive: the main thread sits
    inside gpui's calloop event loop. The only sanctioned way in is gpui's
    own foreground executor, so the pump is a detached foreground task that
    owns the whole pump-state machine on the main thread and receives
    requested delays over a channel. It is spawned once per process; a
    reinstall after invalidate just re-arms the flags.
    */
    if PUMP_SENDER.get().is_none() {
        let (sender, receiver) = mpsc::unbounded();
        let _ = PUMP_SENDER.set(sender);
        let background_executor = cx.background_executor().clone();
        cx.foreground_executor()
            .spawn(drive_message_pump(receiver, background_executor))
            .detach();
    }

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
}

pub(super) fn schedule_message_pump_work(delay_ms: i64) {
    let Ok(mut pending_delay_ms) = PUMP_DISPATCH_DELAY_MS.lock() else {
        return;
    };
    if !PUMP_DISPATCH_PENDING.load(Ordering::SeqCst) {
        *pending_delay_ms = delay_ms;
        PUMP_DISPATCH_PENDING.store(true, Ordering::SeqCst);
        drop(pending_delay_ms);
        if let Some(sender) = PUMP_SENDER.get() {
            let _ = sender.unbounded_send(());
        }
    } else if delay_ms != PUMP_PLACEHOLDER_DELAY_MS {
        // New real CEF work replaces the pending deadline. The host-owned
        // placeholder never overwrites work already queued by Chromium.
        *pending_delay_ms = delay_ms;
    }
}

enum PumpEvent {
    Scheduled(Option<()>),
    DeadlineReached,
}

async fn drive_message_pump(
    mut scheduled_delays: mpsc::UnboundedReceiver<()>,
    background_executor: gpui::BackgroundExecutor,
) {
    // `deadline` is this platform's SetTimer/KillTimer: a pending one-shot
    // that any newly scheduled delay replaces (making the macOS shim's
    // generation counter unnecessary, same as Windows).
    let mut deadline: Option<Instant> = None;
    loop {
        let event = match deadline {
            Some(at) => {
                let timer = background_executor
                    .timer(at.saturating_duration_since(Instant::now()))
                    .fuse();
                futures::pin_mut!(timer);
                futures::select_biased! {
                    scheduled = scheduled_delays.next() => PumpEvent::Scheduled(scheduled),
                    _ = timer => PumpEvent::DeadlineReached,
                }
            }
            None => PumpEvent::Scheduled(scheduled_delays.next().await),
        };

        match event {
            // The process-wide sender lives in a static and is never
            // dropped; a closed channel means process teardown.
            PumpEvent::Scheduled(None) => return,
            PumpEvent::Scheduled(Some(())) => {
                let delay_ms = if let Ok(delay_ms) = PUMP_DISPATCH_DELAY_MS.lock() {
                    let delay_ms = *delay_ms;
                    PUMP_DISPATCH_PENDING.store(false, Ordering::SeqCst);
                    delay_ms
                } else {
                    PUMP_DISPATCH_PENDING.store(false, Ordering::SeqCst);
                    PUMP_PLACEHOLDER_DELAY_MS
                };
                on_schedule_message_pump_work(&mut deadline, delay_ms);
            }
            PumpEvent::DeadlineReached => {
                deadline = None;
                if PUMP_INSTALLED.load(Ordering::SeqCst) && PUMP_WORK_PENDING.load(Ordering::SeqCst)
                {
                    PUMP_WORK_PENDING.store(false, Ordering::SeqCst);
                    run_scheduled_message_pump_work();
                }
            }
        }
    }
}

fn on_schedule_message_pump_work(deadline: &mut Option<Instant>, delay_ms: i64) {
    if !PUMP_INSTALLED.load(Ordering::SeqCst) {
        return;
    }

    if delay_ms == PUMP_PLACEHOLDER_DELAY_MS && PUMP_WORK_PENDING.load(Ordering::SeqCst) {
        return;
    }

    PUMP_WORK_PENDING.store(false, Ordering::SeqCst);
    *deadline = None;

    let clamped_delay_ms = if delay_ms <= 0 {
        PUMP_IMMEDIATE_TIMER_DELAY_MS
    } else {
        delay_ms.min(PUMP_MAX_TIMER_DELAY_MS)
    };
    PUMP_WORK_PENDING.store(true, Ordering::SeqCst);
    *deadline = Some(Instant::now() + Duration::from_millis(clamped_delay_ms as u64));
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
    dispatch_pending_glib_main_context_sources();
    cef::do_message_loop_work();
    dispatch_pending_glib_main_context_sources();
    PUMP_WORK_ACTIVE.store(false, Ordering::SeqCst);

    PUMP_REENTRANCY_DETECTED.load(Ordering::SeqCst)
}

/*
CDXC:GPUILinuxX11Backend 2026-07-05 (device-verified root cause):
CEF's external message pump (MessagePumpExternal, libcef browser_message_loop)
only drains Chromium task queues; on Linux Chromium's UI-thread event
machinery — the X11 event source, fd watchers, and everything else registered
on the thread-default GMainContext — is dispatched by the embedder's glib
main loop. cefclient gets this for free from its GTK loop; GPUI runs calloop,
so nothing dispatched glib sources and Chromium never read its X connection:
CreateWindow errors went unseen, MapNotify/ConfigureNotify never arrived,
browser widgets never became visible, and compositor frames were never
produced (windows stayed black while the DOM ran fine). Draining the default
context around each pump step is the same integration contract cefclient's
external pump documents, expressed without GTK.

glib symbols are resolved from the already-loaded libglib-2.0 (libcef.so
hard-depends on it), so this adds no link-time or packaging dependency.
*/
fn dispatch_pending_glib_main_context_sources() {
    struct GlibMainContext {
        context: *mut std::ffi::c_void,
        pending: unsafe extern "C" fn(*mut std::ffi::c_void) -> std::ffi::c_int,
        iteration: unsafe extern "C" fn(*mut std::ffi::c_void, std::ffi::c_int) -> std::ffi::c_int,
    }
    // The pump driver task and CEF callbacks all run on the GPUI main
    // thread, which owns the default GMainContext; the value never crosses
    // threads.
    unsafe impl Send for GlibMainContext {}
    unsafe impl Sync for GlibMainContext {}

    static GLIB: OnceLock<GlibMainContext> = OnceLock::new();
    let glib = GLIB.get_or_init(|| {
        unsafe extern "C" {
            fn dlopen(
                filename: *const std::ffi::c_char,
                flag: std::ffi::c_int,
            ) -> *mut std::ffi::c_void;
            fn dlsym(
                handle: *mut std::ffi::c_void,
                symbol: *const std::ffi::c_char,
            ) -> *mut std::ffi::c_void;
        }
        const RTLD_NOW: std::ffi::c_int = 2;
        unsafe {
            let handle = dlopen(c"libglib-2.0.so.0".as_ptr(), RTLD_NOW);
            assert!(
                !handle.is_null(),
                "libglib-2.0.so.0 must be loadable: libcef.so links it and CEF is initialized before the pump runs"
            );
            let default_context = dlsym(handle, c"g_main_context_default".as_ptr());
            let pending = dlsym(handle, c"g_main_context_pending".as_ptr());
            let iteration = dlsym(handle, c"g_main_context_iteration".as_ptr());
            assert!(
                !default_context.is_null() && !pending.is_null() && !iteration.is_null(),
                "glib main-context symbols must resolve from libglib-2.0.so.0"
            );
            let default_context: unsafe extern "C" fn() -> *mut std::ffi::c_void =
                std::mem::transmute(default_context);
            GlibMainContext {
                context: default_context(),
                pending: std::mem::transmute(pending),
                iteration: std::mem::transmute(iteration),
            }
        }
    });

    // Bounded drain: dispatch what is ready without blocking, and stop after
    // a fixed number of iterations so a misbehaving source cannot wedge the
    // GPUI frame loop.
    for _ in 0..64 {
        if unsafe { (glib.pending)(glib.context) } == 0 {
            break;
        }
        unsafe { (glib.iteration)(glib.context, 0) };
    }
}

pub(super) fn apply_platform_settings(settings: &mut cef::Settings) {
    /*
    On macOS the bundle layout discovers the helper apps; on Linux CEF
    re-launches the main executable for subprocesses unless
    browser_subprocess_path points at the dedicated helper, so the packaged
    layout must place ghostex-gpui-cef-helper beside the main executable.
    */
    let executable =
        std::env::current_exe().expect("failed to resolve GPUI executable path for CEF helper");
    let helper = executable
        .parent()
        .expect("GPUI executable path has no parent directory")
        .join("ghostex-gpui-cef-helper");
    settings.browser_subprocess_path = cef::CefString::from(helper.to_string_lossy().as_ref());
    let runtime_dir = std::env::var_os(crate::cef_component_window::CEF_RUNTIME_DIR_ENV)
        .map(std::path::PathBuf::from)
        .expect("verified CEF runtime directory must be configured before CEF initialization");
    settings.resources_dir_path = cef::CefString::from(runtime_dir.to_string_lossy().as_ref());
    settings.locales_dir_path =
        cef::CefString::from(runtime_dir.join("locales").to_string_lossy().as_ref());
}

pub(super) fn append_platform_command_line_switches(command_line: &mut cef::CommandLine) {
    use cef::ImplCommandLine as _;
    // The whole app runs X11 (see the module header), so Chromium's Ozone
    // backend must be pinned to X11 as well; letting Ozone auto-pick Wayland
    // would make windowed child-browser creation impossible. Chromium
    // propagates the switch to its subprocesses itself.
    command_line.append_switch_with_value(
        Some(&cef::CefString::from("ozone-platform")),
        Some(&cef::CefString::from("x11")),
    );
}

/*
CDXC:GPUILinuxX11Backend 2026-07-05 (device-verified root cause):
gpui's X11 windows use a 32-bit ARGB visual, but CEF's CefWindowX11 creates
its child at the server default depth (24) with the default visual and no
colormap. X11 requires an explicit colormap (and border pixel) whenever a
child's depth differs from its parent's, so CEF's CreateWindow fails with
BadMatch — silently, because with the external message pump Chromium's UI
thread never dispatches its X error queue. Verified on device: an identical
CreateWindow without a colormap under the GPUI window fails with error code
8 (BadMatch); with an explicit colormap it succeeds.

The adapter therefore owns one real intermediate "embed host" X11 window per
CEF surface: default visual, default depth, explicit colormap, created as a
child of the GPUI window. CEF parents into that depth-matched host, so its
own default-visual CreateWindow is valid again. The host is normal layout
ownership (the actual container for the browser region), not an overlay or
hit-test shim: frame operations move/size the host and keep the CEF window
at the host's full size; visibility maps/unmaps the host; focus still goes
to the CEF window itself.

The host id is recorded against the CEF window right after synchronous
browser creation (prepare_native_view_for_focus, which runs on the single
creating thread) and destroyed in release_native_view when the owning
CefBrowser drops.
*/
static PENDING_EMBED_HOST: Mutex<Option<X11Window>> = Mutex::new(None);
static EMBED_HOST_BY_CEF_WINDOW: Mutex<Option<HashMap<X11Window, X11Window>>> = Mutex::new(None);

fn embed_host_colormap(
    connection: &RustConnection,
    screen: &x11rb::protocol::xproto::Screen,
) -> u32 {
    static COLORMAP: OnceLock<u32> = OnceLock::new();
    *COLORMAP.get_or_init(|| {
        let colormap = connection
            .generate_id()
            .expect("failed to allocate an X11 colormap id for CEF embed hosts");
        let _ = connection.create_colormap(
            ColormapAlloc::NONE,
            colormap,
            screen.root,
            screen.root_visual,
        );
        colormap
    })
}

fn create_embed_host_window(parent: X11Window, bounds: &cef::Rect) -> X11Window {
    let (connection, screen_index) = x11_connection();
    let screen = &connection.setup().roots[*screen_index];
    let host = connection
        .generate_id()
        .expect("failed to allocate an X11 window id for the CEF embed host");
    let values = CreateWindowAux::new()
        .background_pixel(0)
        .border_pixel(0)
        .colormap(embed_host_colormap(connection, screen));
    let _ = connection.create_window(
        screen.root_depth,
        host,
        parent,
        bounds.x as i16,
        bounds.y as i16,
        (bounds.width.max(1)) as u16,
        (bounds.height.max(1)) as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &values,
    );
    // Mapped immediately to match the macOS adapter, where the CEF child
    // NSView starts visible; the shared shell hides collapsed surfaces
    // through set_native_view_visible.
    let _ = connection.map_window(host);
    let _ = connection.flush();
    host
}

fn embed_host_for_cef_window(cef_window: X11Window) -> Option<X11Window> {
    let registry = EMBED_HOST_BY_CEF_WINDOW
        .lock()
        .expect("CEF embed-host registry mutex should not be poisoned");
    registry
        .as_ref()
        .and_then(|hosts| hosts.get(&cef_window).copied())
}

pub(super) fn child_window_info(
    parent_native_view: *mut c_void,
    bounds: &cef::Rect,
) -> cef::WindowInfo {
    // cef_window_handle_t is the X11 window id (c_ulong) on Linux; the
    // opaque pointer from cef_parent_native_view carries that id.
    let Some(parent) = x11_window(parent_native_view) else {
        return cef::WindowInfo::default().set_as_child(
            parent_native_view as usize as cef::sys::cef_window_handle_t,
            bounds,
        );
    };
    let host = create_embed_host_window(parent, bounds);
    *PENDING_EMBED_HOST
        .lock()
        .expect("pending CEF embed-host mutex should not be poisoned") = Some(host);
    // The CEF window fills the embed host; the host carries the surface
    // position within the GPUI window.
    let cef_bounds = cef::Rect {
        x: 0,
        y: 0,
        width: bounds.width.max(1),
        height: bounds.height.max(1),
    };
    cef::WindowInfo::default().set_as_child(host as cef::sys::cef_window_handle_t, &cef_bounds)
}

pub(super) fn native_view_ptr(handle: cef::sys::cef_window_handle_t) -> *mut c_void {
    handle as usize as *mut c_void
}

pub(super) fn set_native_view_mouse_focus_passive(_native_view: *mut c_void, _passive: bool) {
    // Mouse-focus passivity is an AppKit first-responder policy; X11 focus
    // routing for the sidebar is not implemented here yet.
}

pub(super) fn set_native_view_passive_focus_grant(_native_view: *mut c_void, _granted: bool) {}

pub(super) fn return_focus_to_gpui_root(_native_view: *mut c_void) {}

pub(super) fn prepare_native_view_for_focus(native_view: *mut c_void) {
    // The macOS focus subclass exists to route AppKit first-responder and
    // command-key dispatch into the exact CEF NSView. On X11 keyboard focus
    // follows SetInputFocus/click on the Chromium child window, and
    // select-all runs inside Chromium's own accelerator handling. The only
    // per-view setup is adopting the embed host created for this browser:
    // creation is synchronous on this thread, so the pending host is the one
    // this CEF window was just parented into.
    let pending = PENDING_EMBED_HOST
        .lock()
        .expect("pending CEF embed-host mutex should not be poisoned")
        .take();
    let (Some(cef_window), Some(host)) = (x11_window(native_view), pending) else {
        return;
    };
    EMBED_HOST_BY_CEF_WINDOW
        .lock()
        .expect("CEF embed-host registry mutex should not be poisoned")
        .get_or_insert_with(HashMap::new)
        .insert(cef_window, host);
}

pub(super) fn release_native_view(native_view: *mut c_void) {
    let Some(cef_window) = x11_window(native_view) else {
        return;
    };
    let host = EMBED_HOST_BY_CEF_WINDOW
        .lock()
        .expect("CEF embed-host registry mutex should not be poisoned")
        .as_mut()
        .and_then(|hosts| hosts.remove(&cef_window));
    let Some(host) = host else {
        return;
    };
    let (connection, _) = x11_connection();
    let _ = connection.destroy_window(host);
    let _ = connection.flush();
}

/// Adapter-owned X connection for child-window placement. CEF and gpui each
/// hold their own display connections; requests on separate connections are
/// serialized by the X server, so this needs no coordination with them.
fn x11_connection() -> &'static (RustConnection, usize) {
    static X11_CONNECTION: OnceLock<(RustConnection, usize)> = OnceLock::new();
    X11_CONNECTION.get_or_init(|| {
        x11rb::connect(None)
            .expect("failed to connect to the X11 display for CEF child-window placement")
    })
}

fn x11_window(native_view: *mut c_void) -> Option<X11Window> {
    let id = native_view as usize;
    if id == 0 {
        return None;
    }
    // X11 window ids are 32-bit resource ids even though the C handle type
    // is c_ulong.
    Some(id as X11Window)
}

pub(super) fn set_native_view_frame(
    native_view: *mut c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f32,
) {
    let Some(window) = x11_window(native_view) else {
        return;
    };
    /*
    The shared shell passes gpui logical pixels with a top-left origin. X11
    child-window placement is physical pixels relative to the parent's
    top-left, and X11 has no per-window scale query, so the conversion uses
    the scale factor GPUI computed for the parent window. Zero extents are
    clamped to 1: a zero-size X window is a BadValue protocol error, and the
    shared shell hides collapsed surfaces via set_native_view_visible rather
    than zero-sizing them.
    */
    let scale = if scale_factor > 0.0 {
        scale_factor as f64
    } else {
        1.0
    };
    let scaled_x = (x * scale).round() as i32;
    let scaled_y = (y * scale).round() as i32;
    let scaled_width = (width * scale).round().max(1.0) as u32;
    let scaled_height = (height * scale).round().max(1.0) as u32;
    let (connection, _) = x11_connection();
    // The embed host carries the position inside the GPUI window; the CEF
    // window stays pinned at the host's origin with the host's full size so
    // CefWindowX11's ConfigureNotify handling resizes the browser widget.
    if let Some(host) = embed_host_for_cef_window(window) {
        let host_values = ConfigureWindowAux::new()
            .x(scaled_x)
            .y(scaled_y)
            .width(scaled_width)
            .height(scaled_height);
        let _ = connection.configure_window(host, &host_values);
        let cef_values = ConfigureWindowAux::new()
            .x(0)
            .y(0)
            .width(scaled_width)
            .height(scaled_height);
        let _ = connection.configure_window(window, &cef_values);
    } else {
        let values = ConfigureWindowAux::new()
            .x(scaled_x)
            .y(scaled_y)
            .width(scaled_width)
            .height(scaled_height);
        let _ = connection.configure_window(window, &values);
    }
    let _ = connection.flush();
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
    let Some(window) = x11_window(native_view) else {
        return;
    };
    // Map/unmap mirrors NSView.hidden: the window keeps its geometry and
    // browser state, it just stops being composited. Neither request moves
    // keyboard focus; the shared shell's blur() handles focus release.
    // With an embed host the host is the mapped/unmapped unit so the CEF
    // window's own mapped state (owned by CefWindowX11) stays untouched.
    let (connection, _) = x11_connection();
    let target = embed_host_for_cef_window(window).unwrap_or(window);
    if visible {
        let _ = connection.map_window(target);
    } else {
        let _ = connection.unmap_window(target);
    }
    let _ = connection.flush();
}

pub(super) fn order_native_view_front(native_view: *mut c_void) {
    let Some(window) = x11_window(native_view) else {
        return;
    };
    // Mirrors the macOS reorder above all current siblings: dropdown CEF
    // panels are reused across opens while other child windows keep being
    // created, so showing one must re-assert its top stacking position.
    let (connection, _) = x11_connection();
    let target = embed_host_for_cef_window(window).unwrap_or(window);
    let values = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
    let _ = connection.configure_window(target, &values);
    let _ = connection.flush();
}

pub(super) fn focus_native_view(native_view: *mut c_void) {
    let Some(window) = x11_window(native_view) else {
        return;
    };
    // Mirrors makeFirstResponder on macOS: give the CEF child window X input
    // focus so key events route to Chromium; the shared shell follows up
    // with host.set_focus(1) so Chromium moves focus to its inner widget.
    // RevertTo=Parent returns focus to the GPUI window if the child unmaps.
    let (connection, _) = x11_connection();
    let _ = connection.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME);
    let _ = connection.flush();
}

pub(super) fn focus_gpui_root_view(native_view: *mut c_void) {
    focus_native_view(native_view);
}

pub(super) fn native_view_owns_first_responder(_native_view: *mut c_void) -> bool {
    // First-responder arbitration is an AppKit concern; X11 input focus is
    // already granted explicitly through focus_native_view, so renderer
    // focus requests keep their pre-existing allow behavior here.
    true
}
