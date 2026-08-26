use std::{
    collections::{HashMap, HashSet, VecDeque},
    env,
    ffi::{CStr, CString, c_char, c_int, c_void},
    fmt,
    mem::{self, ManuallyDrop},
    ptr::{self, NonNull},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicPtr, AtomicU8, Ordering},
    },
};

#[cfg(target_os = "macos")]
use std::{os::unix::ffi::OsStrExt as _, path::Path};

use gpui::{Bounds, Pixels};

use crate::{
    AgentsTerminalBodyMountSlotId, AgentsTerminalRuntimeSessionId, AgentsTerminalStartupBodySlotId,
    TerminalSurfaceMountSlotKey,
    ghostty_kit::ffi,
    ghostty_vt::VtOptionAsAlt,
    shared_settings::SharedTerminalConfirmCloseSurface,
    terminal_element::{
        TerminalConfiguredColor, TerminalCursorShape, TerminalMetricAdjustment,
        TerminalMouseShiftCapture, TerminalViewSettings,
    },
    terminal_gpui_engine::{
        GpuiTerminalColorDefaults, GpuiTerminalEngineConfig,
        gpui_engine_terminal_font_config_from_parts,
    },
    terminal_model::Rgb,
};

#[cfg(target_os = "macos")]
use crate::terminal_native_view::RealTerminalNativeViewHandle;

/*
CDXC:GPUIGhosttySurfaceRuntime 2026-06-22-22:45:
Phase 2 crosses the real GhosttyKit/libghostty boundary for visible running Agents mount slots. The runtime owners may initialize Ghostty, create a finalized default config, share one Ghostty app, create/drop/update real surfaces from App-owned host NSViews, and mirror shell-derived terminal focus idempotently; they must not add command/cwd/env/session lifecycle, persistent IDs, terminal content, stdout/stderr, logs, fake handles, fallback success paths, overlays, hidden hit regions, broad hit-test routing, or synthetic input routing.

CDXC:GPUITerminalRuntimeIdentity 2026-06-22-23:24:
Ghostty surface owners carry the private runtime session id separately from the pane/body mount slot. Mount slots remain layout attachments keyed by pane plus shell session, while runtime ids are process-local session identity and must not be persisted, logged, or shown as terminal titles.

CDXC:GPUITerminalLaunchPayload 2026-06-22-23:58:
Phase 3 startup may carry cwd, command, env vars, initial input, and wait-after-command only as runtime launch data on a prepared Ghostty surface request. Reject interior-NUL strings before FFI and keep CString/env-var storage scoped to ghostty_surface_new so private launch values never enter Debug output, logs, shell state, titles, or returned configs with dangling pointers.

CDXC:GPUITerminalStartupGhosttySurface 2026-06-23-03:33:
Mounting startup surfaces need a startup-owned Ghostty boundary keyed by `AgentsTerminalStartupBodySlotId` plus process-local runtime id. This owner may create, resize, and free a hidden Ghostty surface from an already-prepared config request, but it must not require a Running mount slot, show or focus AppKit hosts, set Ghostty app/surface focus, apply Ready/Failed, persist, log, or expose launch/private terminal payloads.

CDXC:GPUITerminalStartupGhosttySurface 2026-06-23-04:13:
Startup readiness may inspect Ghostty surface metadata only as redacted runtime facts: process-exited, foreground-process-id-present, and tty-name-present. Raw tty names and process ids must be freed or discarded at the FFI boundary and must not enter Debug output, shell state, logs, titles, launch payloads, or persistence.

CDXC:GPUITerminalStartupHandoff 2026-06-23-04:25:
Ready Mounting startup surfaces must be re-owned by the Running surface path instead of being dropped and recreated. The conversion consumes the startup owner without freeing the Ghostty surface, changes only the map key identity from startup body slot to Running body mount slot, and keeps raw process, tty, launch, and terminal content data out of logs, shell state, and Debug output.

CDXC:GPUITerminalGhosttyClose 2026-06-23-04:49:
Running Ghostty close parity must ask the embedded surface to close and wait for the runtime close callback before shell-tab removal. Each surface owner passes a process-memory close token as surface userdata, keeps the AppKit NSView available only through `platform.macos.nsview`, and records only confirmation-needed or confirmed-close state without logging, persistence, runtime ids, raw paths, command text, environment, stdout/stderr, tty names, process ids, or terminal content.

CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
Running Ghostty surface ownership is generic over a typed body mount slot so command-pane terminals can use the same App-owned NSView and GhosttyKit surface pipeline without entering Agents workspace/startup maps. Command owners use command group/session ids, explicit launch-payload sources only, no title/status/path parsing, no logs, no persistence, and no input routing changes.

CDXC:GPUICommandTerminalLaunchPayload 2026-06-27-01:25:
Plain command terminals may carry only the active project cwd supplied by the app's exact-slot command launch source, while Action terminals may carry their separate command payload. The Ghostty surface owner must remain ignorant of titles, shell state, terminal content, fallback cwd inference, and persisted project paths.

CDXC:GPUITerminalProcessExit 2026-06-23-05:30:
Mounted Running Agents and command terminals need a runtime-only process-exited query that returns only a redacted boolean. Callers that only need exit state must not use the richer metadata snapshot because that would unnecessarily cross the tty/pid FFI boundary and increase the chance of exposing raw terminal/process details.

CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
Close-confirm parity needs the Ghostty callback token to hand confirmation-needed events to GPUI exactly once so App-owned runtime state can hold the pending prompt identity. The token remains process memory only and must not expose terminal content, command text, paths, runtime ids, durable ids, logs, shell-state fields, or launch payload data.

CDXC:GPUITerminalCloseConfirm 2026-06-23-05:47:
Canceling a pending close-confirm prompt must reset only the owner-local close-request latch after exact GPUI surface matching. That lets a later user close action ask Ghostty again without inventing a fallback close path or persisting prompt state.

CDXC:GPUITerminalInputABI 2026-06-23-05:53:
Real terminal input parity begins with narrow owner wrappers over the existing embedded Ghostty input exports. These wrappers accept already-sanitized primitive values or borrowed byte slices, do not translate GPUI keyboard/mouse events yet, and must not store, log, persist, or expose terminal input text through Debug or shell-state JSON.

CDXC:GPUITerminalInputABI 2026-06-23-05:58:
Zero-length text and preedit are distinct FFI edge cases. Text uses a stable non-null empty pointer because Ghostty slices the pointer unconditionally, while preedit clear follows Ghostty's AppKit path and passes a null pointer with length zero.

CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
Slice 237 binds GhosttyKit's real `ghostty_surface_needs_confirm_quit` query so close-confirm prompts can be backed by source-side ABI evidence. Surface owners may expose only a boolean for the current mounted surface; they must not log, persist, or reveal process ids, tty names, commands, paths, runtime ids, or terminal content.

CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
Mounted GPUI terminal host NSViews own native AppKit key events because GPUI's root `KeyDownEvent` drops the macOS native keycode Ghostty needs for Return, Backspace, arrows, modifiers, and bindings. Register only the exact host-view to Ghostty-surface pairing while a real surface is mounted, keep the registry runtime-only, and never store typed text beyond the synchronous FFI call.

CDXC:GPUITerminalFileDropInsertion 2026-06-27-03:34:
Terminal file drops are transient text insertion to the exact mounted AppKit host view registered for native key forwarding. Dispatch the borrowed bytes only through that matched Ghostty surface, reject null, unregistered, or empty input, and do not add focused-surface fallback routing, logging, persistence, overlays, or hit-test routing.

CDXC:GPUITerminalNativeImeBridge 2026-06-27-03:46:
AppKit IME committed text, marked preedit text, and candidate-window geometry must route only through the exact mounted terminal host view registered for Ghostty native input, including command-pane terminals. Borrow callback bytes only for the synchronous Ghostty call, reject empty committed text, allow empty preedit to clear via the null/zero Ghostty convention, and do not store raw IME text or fall back to focused surfaces.
*/

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct GhosttyNativeKeyTarget {
    surface: usize,
    functions: GhosttyKitFunctionTable,
    mount_slot_sort_key: (u8, u64, u64),
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
pub(crate) struct GhosttyNativeKeyTargetDiagnostic {
    pub(crate) surface_kind: u8,
    pub(crate) container_id: u64,
    pub(crate) session_id: u64,
}

#[cfg(target_os = "macos")]
fn ghostty_native_key_targets()
-> &'static Mutex<std::collections::HashMap<usize, GhosttyNativeKeyTarget>> {
    static TARGETS: OnceLock<Mutex<std::collections::HashMap<usize, GhosttyNativeKeyTarget>>> =
        OnceLock::new();
    TARGETS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

#[cfg(target_os = "macos")]
pub(crate) fn register_native_key_target<SlotId>(
    native_view: RealTerminalNativeViewHandle,
    surface: &GhosttySurfaceOwner<SlotId>,
) where
    SlotId: TerminalSurfaceMountSlotKey,
{
    let target = GhosttyNativeKeyTarget {
        surface: surface.as_raw() as usize,
        functions: surface.functions,
        mount_slot_sort_key: surface.mount_slot_id().terminal_surface_sort_key(),
    };
    if let Ok(mut targets) = ghostty_native_key_targets().lock() {
        targets.insert(native_view.as_ptr() as usize, target);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_native_key_target(native_view: RealTerminalNativeViewHandle) {
    if let Ok(mut targets) = ghostty_native_key_targets().lock() {
        targets.remove(&(native_view.as_ptr() as usize));
    }
}

#[cfg(target_os = "macos")]
fn native_key_target_for_view(native_view: *mut c_void) -> Option<GhosttyNativeKeyTarget> {
    let native_view = NonNull::new(native_view)?;
    ghostty_native_key_targets()
        .lock()
        .ok()
        .and_then(|targets| targets.get(&(native_view.as_ptr() as usize)).copied())
}

#[cfg(target_os = "macos")]
pub(crate) fn native_key_target_diagnostic_for_view(
    native_view: *mut c_void,
) -> Option<GhosttyNativeKeyTargetDiagnostic> {
    let target = native_key_target_for_view(native_view)?;
    let (surface_kind, container_id, session_id) = target.mount_slot_sort_key;
    Some(GhosttyNativeKeyTargetDiagnostic {
        surface_kind,
        container_id,
        session_id,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn native_key_translation_mods_for_view(
    native_view: *mut c_void,
    mods: ffi::ghostty_input_mods_e,
) -> ffi::ghostty_input_mods_e {
    let Some(target) = native_key_target_for_view(native_view) else {
        return mods;
    };
    unsafe { (target.functions.surface_key_translation_mods)(target.surface as *mut c_void, mods) }
}

#[cfg(target_os = "macos")]
pub(crate) fn send_native_key_event_for_view(
    native_view: *mut c_void,
    event: ffi::ghostty_input_key_s,
) -> bool {
    let Some(target) = native_key_target_for_view(native_view) else {
        return false;
    };
    unsafe { (target.functions.surface_key)(target.surface as *mut c_void, event) }
}

#[cfg(target_os = "macos")]
pub(crate) fn native_key_event_is_binding_for_view(
    native_view: *mut c_void,
    event: ffi::ghostty_input_key_s,
) -> bool {
    let Some(target) = native_key_target_for_view(native_view) else {
        return false;
    };
    let mut flags = 0;
    unsafe {
        (target.functions.surface_key_is_binding)(target.surface as *mut c_void, event, &mut flags)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn send_native_dropped_text_for_view(native_view: *mut c_void, bytes: &[u8]) -> bool {
    send_native_surface_text_for_view(native_view, bytes)
}

#[cfg(target_os = "macos")]
pub(crate) fn send_native_prompt_editor_shortcut_for_view(native_view: *mut c_void) -> bool {
    send_native_surface_text_for_view(native_view, b"\x07")
}

#[cfg(target_os = "macos")]
fn send_native_surface_text_for_view(native_view: *mut c_void, bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let Some(target) = native_key_target_for_view(native_view) else {
        return false;
    };
    unsafe {
        (target.functions.surface_text)(
            target.surface as *mut c_void,
            bytes.as_ptr() as *const c_char,
            bytes.len(),
        );
    }
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn set_native_preedit_text_for_view(native_view: *mut c_void, bytes: &[u8]) -> bool {
    let Some(target) = native_key_target_for_view(native_view) else {
        return false;
    };
    unsafe {
        (target.functions.surface_preedit)(
            target.surface as *mut c_void,
            ghostty_surface_preedit_ptr(bytes),
            bytes.len(),
        );
    }
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn native_ime_point_for_view(
    native_view: *mut c_void,
) -> Option<GhosttySurfaceImePoint> {
    let target = native_key_target_for_view(native_view)?;
    let mut x = 0.0;
    let mut y = 0.0;
    let mut width = 0.0;
    let mut height = 0.0;
    unsafe {
        (target.functions.surface_ime_point)(
            target.surface as *mut c_void,
            &mut x,
            &mut y,
            &mut width,
            &mut height,
        );
    }
    Some(GhosttySurfaceImePoint {
        x,
        y,
        width,
        height,
    })
}

#[derive(Clone, Copy)]
pub(crate) struct GhosttyKitFunctionTable {
    init: unsafe fn(usize, *mut *mut c_char) -> c_int,
    config_new: unsafe fn() -> ffi::ghostty_config_t,
    config_free: unsafe fn(ffi::ghostty_config_t),
    config_load_default_files: unsafe fn(ffi::ghostty_config_t),
    config_load_file: unsafe fn(ffi::ghostty_config_t, *const c_char),
    config_load_string: unsafe fn(ffi::ghostty_config_t, *const c_char, usize),
    config_load_recursive_files: unsafe fn(ffi::ghostty_config_t),
    config_finalize: unsafe fn(ffi::ghostty_config_t),
    app_new: unsafe fn(
        *const ffi::ghostty_runtime_config_s,
        ffi::ghostty_config_t,
    ) -> ffi::ghostty_app_t,
    app_free: unsafe fn(ffi::ghostty_app_t),
    app_tick: unsafe fn(ffi::ghostty_app_t),
    app_set_focus: unsafe fn(ffi::ghostty_app_t, bool),
    string_free: unsafe fn(ffi::ghostty_string_s),
    surface_config_new: unsafe fn() -> ffi::ghostty_surface_config_s,
    surface_new: unsafe fn(
        ffi::ghostty_app_t,
        *const ffi::ghostty_surface_config_s,
    ) -> ffi::ghostty_surface_t,
    surface_free: unsafe fn(ffi::ghostty_surface_t),
    surface_set_content_scale: unsafe fn(ffi::ghostty_surface_t, f64, f64),
    surface_set_size: unsafe fn(ffi::ghostty_surface_t, u32, u32),
    surface_set_focus: unsafe fn(ffi::ghostty_surface_t, bool),
    surface_set_occlusion: unsafe fn(ffi::ghostty_surface_t, bool),
    surface_size: unsafe fn(ffi::ghostty_surface_t) -> ffi::ghostty_surface_size_s,
    surface_needs_confirm_quit: unsafe fn(ffi::ghostty_surface_t) -> bool,
    surface_binding_action: unsafe fn(ffi::ghostty_surface_t, *const c_char, usize) -> bool,
    surface_process_exited: unsafe fn(ffi::ghostty_surface_t) -> bool,
    #[allow(dead_code)]
    // ghostty surface FFI vtable entry kept complete; nothing reads this metadata back today
    surface_foreground_pid: unsafe fn(ffi::ghostty_surface_t) -> u64,
    #[allow(dead_code)]
    // ghostty surface FFI vtable entry kept complete; nothing reads this metadata back today
    surface_tty_name: unsafe fn(ffi::ghostty_surface_t) -> ffi::ghostty_string_s,
    surface_key_translation_mods:
        unsafe fn(ffi::ghostty_surface_t, ffi::ghostty_input_mods_e) -> ffi::ghostty_input_mods_e,
    surface_key: unsafe fn(ffi::ghostty_surface_t, ffi::ghostty_input_key_s) -> bool,
    surface_key_is_binding: unsafe fn(
        ffi::ghostty_surface_t,
        ffi::ghostty_input_key_s,
        *mut ffi::ghostty_binding_flags_e,
    ) -> bool,
    surface_text: unsafe fn(ffi::ghostty_surface_t, *const c_char, usize),
    surface_preedit: unsafe fn(ffi::ghostty_surface_t, *const c_char, usize),
    surface_mouse_captured: unsafe fn(ffi::ghostty_surface_t) -> bool,
    surface_mouse_button: unsafe fn(
        ffi::ghostty_surface_t,
        ffi::ghostty_input_mouse_state_e,
        ffi::ghostty_input_mouse_button_e,
        ffi::ghostty_input_mods_e,
    ) -> bool,
    surface_mouse_pos: unsafe fn(ffi::ghostty_surface_t, f64, f64, ffi::ghostty_input_mods_e),
    surface_mouse_scroll:
        unsafe fn(ffi::ghostty_surface_t, f64, f64, ffi::ghostty_input_scroll_mods_t),
    surface_mouse_pressure: unsafe fn(ffi::ghostty_surface_t, u32, f64),
    surface_ime_point: unsafe fn(ffi::ghostty_surface_t, *mut f64, *mut f64, *mut f64, *mut f64),
    surface_request_close: unsafe fn(ffi::ghostty_surface_t),
    surface_complete_clipboard_request:
        unsafe fn(ffi::ghostty_surface_t, *const ffi::ghostty_clipboard_complete_s, *mut c_void),
    surface_deny_clipboard_request: unsafe fn(ffi::ghostty_surface_t, *mut c_void),
}

impl GhosttyKitFunctionTable {
    /*
    CDXC:GPUILinuxX11Backend 2026-07-05:
    The production table binds the real GhosttyKit exports, which exist only
    in the macOS static archive (gpui/build.rs). Non-macOS terminals run the
    libghostty-vt GPUI engine instead, so the table constructor and every
    production_* binding below are macOS-only; the table type itself stays
    cross-platform because owner structs carry it by value.
    */
    #[cfg(target_os = "macos")]
    const fn production() -> Self {
        Self {
            init: production_ghostty_init,
            config_new: production_ghostty_config_new,
            config_free: production_ghostty_config_free,
            config_load_default_files: production_ghostty_config_load_default_files,
            config_load_file: production_ghostty_config_load_file,
            config_load_string: production_ghostty_config_load_string,
            config_load_recursive_files: production_ghostty_config_load_recursive_files,
            config_finalize: production_ghostty_config_finalize,
            app_new: production_ghostty_app_new,
            app_free: production_ghostty_app_free,
            app_tick: production_ghostty_app_tick,
            app_set_focus: production_ghostty_app_set_focus,
            string_free: production_ghostty_string_free,
            surface_config_new: production_ghostty_surface_config_new,
            surface_new: production_ghostty_surface_new,
            surface_free: production_ghostty_surface_free,
            surface_set_content_scale: production_ghostty_surface_set_content_scale,
            surface_set_size: production_ghostty_surface_set_size,
            surface_set_focus: production_ghostty_surface_set_focus,
            surface_set_occlusion: production_ghostty_surface_set_occlusion,
            surface_size: production_ghostty_surface_size,
            surface_needs_confirm_quit: production_ghostty_surface_needs_confirm_quit,
            surface_binding_action: production_ghostty_surface_binding_action,
            surface_process_exited: production_ghostty_surface_process_exited,
            surface_foreground_pid: production_ghostty_surface_foreground_pid,
            surface_tty_name: production_ghostty_surface_tty_name,
            surface_key_translation_mods: production_ghostty_surface_key_translation_mods,
            surface_key: production_ghostty_surface_key,
            surface_key_is_binding: production_ghostty_surface_key_is_binding,
            surface_text: production_ghostty_surface_text,
            surface_preedit: production_ghostty_surface_preedit,
            surface_mouse_captured: production_ghostty_surface_mouse_captured,
            surface_mouse_button: production_ghostty_surface_mouse_button,
            surface_mouse_pos: production_ghostty_surface_mouse_pos,
            surface_mouse_scroll: production_ghostty_surface_mouse_scroll,
            surface_mouse_pressure: production_ghostty_surface_mouse_pressure,
            surface_ime_point: production_ghostty_surface_ime_point,
            surface_request_close: production_ghostty_surface_request_close,
            surface_complete_clipboard_request:
                production_ghostty_surface_complete_clipboard_request,
            surface_deny_clipboard_request: production_ghostty_surface_deny_clipboard_request,
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_init(argc: usize, argv: *mut *mut c_char) -> c_int {
    unsafe { ffi::ghostty_init(argc, argv) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_new() -> ffi::ghostty_config_t {
    unsafe { ffi::ghostty_config_new() }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_free(config: ffi::ghostty_config_t) {
    unsafe { ffi::ghostty_config_free(config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_load_default_files(config: ffi::ghostty_config_t) {
    unsafe { ffi::ghostty_config_load_default_files(config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_load_file(config: ffi::ghostty_config_t, path: *const c_char) {
    unsafe { ffi::ghostty_config_load_file(config, path) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_load_string(
    config: ffi::ghostty_config_t,
    source: *const c_char,
    len: usize,
) {
    unsafe { ffi::ghostty_config_load_string(config, source, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_load_recursive_files(config: ffi::ghostty_config_t) {
    unsafe { ffi::ghostty_config_load_recursive_files(config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_config_finalize(config: ffi::ghostty_config_t) {
    unsafe { ffi::ghostty_config_finalize(config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_new(
    runtime_config: *const ffi::ghostty_runtime_config_s,
    config: ffi::ghostty_config_t,
) -> ffi::ghostty_app_t {
    unsafe { ffi::ghostty_app_new(runtime_config, config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_free(app: ffi::ghostty_app_t) {
    unsafe { ffi::ghostty_app_free(app) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_tick(app: ffi::ghostty_app_t) {
    unsafe { ffi::ghostty_app_tick(app) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_app_set_focus(app: ffi::ghostty_app_t, focused: bool) {
    unsafe { ffi::ghostty_app_set_focus(app, focused) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_string_free(value: ffi::ghostty_string_s) {
    unsafe { ffi::ghostty_string_free(value) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_config_new() -> ffi::ghostty_surface_config_s {
    unsafe { ffi::ghostty_surface_config_new() }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_new(
    app: ffi::ghostty_app_t,
    config: *const ffi::ghostty_surface_config_s,
) -> ffi::ghostty_surface_t {
    unsafe { ffi::ghostty_surface_new(app, config) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_free(surface: ffi::ghostty_surface_t) {
    unsafe { ffi::ghostty_surface_free(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_content_scale(
    surface: ffi::ghostty_surface_t,
    x: f64,
    y: f64,
) {
    unsafe { ffi::ghostty_surface_set_content_scale(surface, x, y) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_size(
    surface: ffi::ghostty_surface_t,
    width: u32,
    height: u32,
) {
    unsafe { ffi::ghostty_surface_set_size(surface, width, height) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_focus(surface: ffi::ghostty_surface_t, focused: bool) {
    unsafe { ffi::ghostty_surface_set_focus(surface, focused) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_set_occlusion(surface: ffi::ghostty_surface_t, visible: bool) {
    unsafe { ffi::ghostty_surface_set_occlusion(surface, visible) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_size(
    surface: ffi::ghostty_surface_t,
) -> ffi::ghostty_surface_size_s {
    unsafe { ffi::ghostty_surface_size(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_process_exited(surface: ffi::ghostty_surface_t) -> bool {
    unsafe { ffi::ghostty_surface_process_exited(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_needs_confirm_quit(surface: ffi::ghostty_surface_t) -> bool {
    unsafe { ffi::ghostty_surface_needs_confirm_quit(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_binding_action(
    surface: ffi::ghostty_surface_t,
    action: *const c_char,
    len: usize,
) -> bool {
    unsafe { ffi::ghostty_surface_binding_action(surface, action, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_foreground_pid(surface: ffi::ghostty_surface_t) -> u64 {
    unsafe { ffi::ghostty_surface_foreground_pid(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_tty_name(
    surface: ffi::ghostty_surface_t,
) -> ffi::ghostty_string_s {
    unsafe { ffi::ghostty_surface_tty_name(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_key_translation_mods(
    surface: ffi::ghostty_surface_t,
    mods: ffi::ghostty_input_mods_e,
) -> ffi::ghostty_input_mods_e {
    unsafe { ffi::ghostty_surface_key_translation_mods(surface, mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_key(
    surface: ffi::ghostty_surface_t,
    event: ffi::ghostty_input_key_s,
) -> bool {
    unsafe { ffi::ghostty_surface_key(surface, event) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_key_is_binding(
    surface: ffi::ghostty_surface_t,
    event: ffi::ghostty_input_key_s,
    flags: *mut ffi::ghostty_binding_flags_e,
) -> bool {
    unsafe { ffi::ghostty_surface_key_is_binding(surface, event, flags) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_text(
    surface: ffi::ghostty_surface_t,
    ptr: *const c_char,
    len: usize,
) {
    unsafe { ffi::ghostty_surface_text(surface, ptr, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_preedit(
    surface: ffi::ghostty_surface_t,
    ptr: *const c_char,
    len: usize,
) {
    unsafe { ffi::ghostty_surface_preedit(surface, ptr, len) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_captured(surface: ffi::ghostty_surface_t) -> bool {
    unsafe { ffi::ghostty_surface_mouse_captured(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_button(
    surface: ffi::ghostty_surface_t,
    action: ffi::ghostty_input_mouse_state_e,
    button: ffi::ghostty_input_mouse_button_e,
    mods: ffi::ghostty_input_mods_e,
) -> bool {
    unsafe { ffi::ghostty_surface_mouse_button(surface, action, button, mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_pos(
    surface: ffi::ghostty_surface_t,
    x: f64,
    y: f64,
    mods: ffi::ghostty_input_mods_e,
) {
    unsafe { ffi::ghostty_surface_mouse_pos(surface, x, y, mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_scroll(
    surface: ffi::ghostty_surface_t,
    x: f64,
    y: f64,
    scroll_mods: ffi::ghostty_input_scroll_mods_t,
) {
    unsafe { ffi::ghostty_surface_mouse_scroll(surface, x, y, scroll_mods) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_mouse_pressure(
    surface: ffi::ghostty_surface_t,
    stage: u32,
    pressure: f64,
) {
    unsafe { ffi::ghostty_surface_mouse_pressure(surface, stage, pressure) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_ime_point(
    surface: ffi::ghostty_surface_t,
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) {
    unsafe { ffi::ghostty_surface_ime_point(surface, x, y, width, height) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_request_close(surface: ffi::ghostty_surface_t) {
    unsafe { ffi::ghostty_surface_request_close(surface) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_complete_clipboard_request(
    surface: ffi::ghostty_surface_t,
    complete: *const ffi::ghostty_clipboard_complete_s,
    state: *mut c_void,
) {
    unsafe { ffi::ghostty_surface_complete_clipboard_request(surface, complete, state) }
}

#[cfg(target_os = "macos")]
unsafe fn production_ghostty_surface_deny_clipboard_request(
    surface: ffi::ghostty_surface_t,
    state: *mut c_void,
) {
    unsafe { ffi::ghostty_surface_deny_clipboard_request(surface, state) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GhosttySurfaceRuntimeError {
    InitFailed(c_int),
    ConfigCreateReturnedNull,
    ConfigPathContainsInteriorNul,
    ConfigOptionInvalid,
    AppCreateReturnedNull,
    SurfaceCreateReturnedNull,
    InvalidScaleFactor(f64),
    InvalidBounds {
        field: GhosttySurfaceBoundsField,
        value: f64,
    },
    LaunchPayloadContainsInteriorNul {
        field: GhosttySurfaceLaunchPayloadField,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GhosttySurfaceBoundsField {
    Width,
    Height,
}

impl From<GhosttySurfaceConfigRequestError> for GhosttySurfaceRuntimeError {
    fn from(error: GhosttySurfaceConfigRequestError) -> Self {
        match error {
            GhosttySurfaceConfigRequestError::InvalidScaleFactor(value) => {
                Self::InvalidScaleFactor(value)
            }
            GhosttySurfaceConfigRequestError::LaunchPayloadContainsInteriorNul { field } => {
                Self::LaunchPayloadContainsInteriorNul { field }
            }
        }
    }
}

static PRODUCTION_GHOSTTY_INIT_RESULT: OnceLock<Result<(), GhosttySurfaceRuntimeError>> =
    OnceLock::new();

fn initialize_production_ghostty_once(
    functions: GhosttyKitFunctionTable,
) -> Result<(), GhosttySurfaceRuntimeError> {
    *PRODUCTION_GHOSTTY_INIT_RESULT.get_or_init(|| initialize_ghostty_runtime(functions))
}

fn initialize_ghostty_runtime(
    functions: GhosttyKitFunctionTable,
) -> Result<(), GhosttySurfaceRuntimeError> {
    let (argc, argv) = leaked_ghostty_process_argv();
    let result = unsafe { (functions.init)(argc, argv) };
    if result == ffi::GHOSTTY_SUCCESS {
        Ok(())
    } else {
        Err(GhosttySurfaceRuntimeError::InitFailed(result))
    }
}

fn leaked_ghostty_process_argv() -> (usize, *mut *mut c_char) {
    let mut argv_storage = env::args()
        .map(|arg| CString::new(arg).expect("process argv strings cannot contain interior NUL"))
        .collect::<Vec<_>>();
    if argv_storage.is_empty() {
        argv_storage.push(CString::new("ghostex-gpui").expect("static argv is NUL-free"));
    }

    let argv_ptrs = argv_storage
        .iter()
        .map(|arg| arg.as_ptr() as *mut c_char)
        .collect::<Vec<_>>();
    let argc = argv_ptrs.len();
    let argv = Box::leak(argv_ptrs.into_boxed_slice()).as_mut_ptr();
    let _argv_storage = Box::leak(argv_storage.into_boxed_slice());
    (argc, argv)
}

#[cfg(target_os = "macos")]
pub(crate) fn load_default_ghostty_background_color() -> Option<ffi::ghostty_config_color_s> {
    let functions = GhosttyKitFunctionTable::production();
    initialize_production_ghostty_once(functions).ok()?;
    let config = GhosttyConfigOwner::load_default_finalized_with_functions(functions).ok()?;

    let key = b"background";
    let mut color = ffi::ghostty_config_color_s { r: 0, g: 0, b: 0 };
    let has_value = unsafe {
        ffi::ghostty_config_get(
            config.as_raw(),
            (&mut color as *mut ffi::ghostty_config_color_s).cast::<c_void>(),
            key.as_ptr().cast::<c_char>(),
            key.len(),
        )
    };

    has_value.then_some(color)
}

/// Load the finalized terminal-relevant configuration through Ghostty itself.
/// The exact path matters because the GPUI bundle identifier differs from
/// Ghostty's while both apps intentionally share the user's Ghostty config.
/// Themes, defaults, and recursive `config-file` entries are resolved before
/// the canonical snapshot crosses into Rust.
#[cfg(target_os = "macos")]
pub(crate) fn load_ghostty_terminal_engine_config_from_path(
    path: &Path,
    selected_theme_source: Option<&str>,
) -> Result<GpuiTerminalEngineConfig, GhosttySurfaceRuntimeError> {
    let functions = GhosttyKitFunctionTable::production();
    initialize_production_ghostty_once(functions)?;

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| GhosttySurfaceRuntimeError::ConfigPathContainsInteriorNul)?;
    let config = unsafe { (functions.config_new)() };
    let config =
        NonNull::new(config).ok_or(GhosttySurfaceRuntimeError::ConfigCreateReturnedNull)?;
    let owner = GhosttyConfigOwner { config, functions };
    unsafe {
        // Seed the selected embedded theme before the user's config. Ghostty
        // defines theme colors as the base layer and explicitly configured
        // foreground/background/palette values as overrides. Loading this
        // source after the config reverses that precedence and, for example,
        // replaces a user's white foreground with GitHub Dark's gray one.
        // Keeping the embedded source first also makes the theme available
        // when Ghostex is running without Ghostty.app's resource directory.
        if let Some(source) = selected_theme_source {
            (functions.config_load_string)(
                owner.as_raw(),
                source.as_ptr().cast::<c_char>(),
                source.len(),
            );
        }
        (functions.config_load_file)(owner.as_raw(), path.as_ptr());
        (functions.config_load_recursive_files)(owner.as_raw());
        (functions.config_finalize)(owner.as_raw());
    }

    let formatted = unsafe { ffi::ghostty_config_to_string(owner.as_raw()) };
    let bytes = if formatted.ptr.is_null() {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(formatted.ptr.cast::<u8>(), formatted.len) }.to_vec()
    };
    unsafe { (functions.string_free)(formatted) };
    let formatted =
        String::from_utf8(bytes).map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
    parse_ghostty_terminal_engine_config(&formatted)
}

#[cfg(target_os = "macos")]
fn parse_ghostty_terminal_engine_config(
    formatted: &str,
) -> Result<GpuiTerminalEngineConfig, GhosttySurfaceRuntimeError> {
    let value = |key: &str| canonical_config_values(formatted, key).into_iter().last();
    /*
    Ghostty's canonical formatter represents an unset optional value as an
    empty right-hand side (for example `cursor-text = `). That is valid typed
    configuration, not a malformed color or enum. Keep ordinary string
    settings capable of carrying an intentional empty value, but decode the
    nullable terminal settings through this explicit boundary so a valid
    finalized config cannot abort creation of a newly selected session.
    */
    let optional_value = |key: &str| value(key).filter(|value| !value.is_empty());
    let font_family = canonical_config_values(formatted, "font-family")
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or("JetBrains Mono");
    let font_size = parse_config_f32(value("font-size"))?;
    let font_weight = canonical_config_values(formatted, "font-variation")
        .into_iter()
        .filter_map(|value| value.strip_prefix("wght="))
        .filter_map(|value| value.parse::<f32>().ok())
        .next_back()
        .unwrap_or(400.0);
    let mut font = gpui_engine_terminal_font_config_from_parts(font_family, font_size, font_weight);
    font.cell_width_adjustment = parse_metric_adjustment(optional_value("adjust-cell-width"))?;
    font.cell_height_adjustment = parse_metric_adjustment(optional_value("adjust-cell-height"))?;

    let foreground = parse_rgb(value("foreground"))?;
    let background = parse_rgb(value("background"))?;
    let cursor = optional_value("cursor-color")
        .map(parse_rgb_value)
        .transpose()?;
    let mut palette = [Rgb::default(); 256];
    let mut palette_count = 0usize;
    for entry in canonical_config_values(formatted, "palette") {
        let Some((index, color)) = entry.split_once('=') else {
            return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid);
        };
        let index = index
            .trim()
            .parse::<usize>()
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
        let slot = palette
            .get_mut(index)
            .ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
        *slot = parse_rgb_value(color.trim())?;
        palette_count += 1;
    }
    if palette_count != 256 {
        return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid);
    }

    let cursor_shape = match value("cursor-style").unwrap_or("block") {
        "bar" => TerminalCursorShape::Bar,
        "underline" => TerminalCursorShape::Underline,
        "block" | "block_hollow" => TerminalCursorShape::Block,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let option_as_alt = match optional_value("macos-option-as-alt").unwrap_or("false") {
        "false" => VtOptionAsAlt::False,
        "true" => VtOptionAsAlt::True,
        "left" => VtOptionAsAlt::Left,
        "right" => VtOptionAsAlt::Right,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let confirm_close_surface = match value("confirm-close-surface").unwrap_or("false") {
        "false" => SharedTerminalConfirmCloseSurface::False,
        "true" => SharedTerminalConfirmCloseSurface::True,
        "always" => SharedTerminalConfirmCloseSurface::Always,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let mouse_shift_capture = match value("mouse-shift-capture").unwrap_or("false") {
        "false" => TerminalMouseShiftCapture::False,
        "true" => TerminalMouseShiftCapture::True,
        "always" => TerminalMouseShiftCapture::Always,
        "never" => TerminalMouseShiftCapture::Never,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let (mouse_scroll_precision, mouse_scroll_discrete) =
        parse_mouse_scroll_multiplier(value("mouse-scroll-multiplier"))?;

    Ok(GpuiTerminalEngineConfig {
        font,
        view: TerminalViewSettings {
            cursor_shape,
            // The GhosttyKit surface path is not selected at runtime; the
            // composited engine owns background images.
            background_image: None,
            cursor_blink: parse_config_bool(
                optional_value("cursor-style-blink").unwrap_or("false"),
            )?,
            cursor_opacity: parse_config_f32(value("cursor-opacity"))?,
            cursor_text: optional_value("cursor-text")
                .map(parse_terminal_configured_color)
                .transpose()?,
            selection_background: optional_value("selection-background")
                .map(parse_terminal_configured_color)
                .transpose()?,
            selection_clear_on_copy: parse_config_bool(
                value("selection-clear-on-copy").unwrap_or("false"),
            )?,
            selection_clear_on_typing: parse_config_bool(
                value("selection-clear-on-typing").unwrap_or("true"),
            )?,
            selection_word_chars: value("selection-word-chars")
                .unwrap_or(" \t'\"│`|:;,()[]{}<>$")
                .to_string(),
            copy_on_select: matches!(value("copy-on-select"), Some("clipboard")),
            selection_clipboard_enabled: matches!(
                value("copy-on-select"),
                Some("true" | "clipboard")
            ),
            clipboard_trim_trailing_spaces: parse_config_bool(
                value("clipboard-trim-trailing-spaces").unwrap_or("true"),
            )?,
            mouse_hide_while_typing: parse_config_bool(
                value("mouse-hide-while-typing").unwrap_or("false"),
            )?,
            mouse_scroll_precision,
            mouse_scroll_discrete,
            mouse_shift_capture,
            scrollbar_visible: value("scrollbar").unwrap_or("system") != "never",
            // This is app-owned rather than a Ghostty config key. Callers
            // overwrite it from shared Settings after loading the finalized
            // Ghostty config.
            scroll_to_bottom_when_typing: true,
        },
        colors: Some(GpuiTerminalColorDefaults {
            foreground,
            background,
            cursor,
            palette,
        }),
        // Ghostty 1.4 renamed `scrollback-limit` to `scrollback-limit-bytes`;
        // the canonical formatter emits only the new key, either as a byte
        // count or as the `unlimited` sentinel (integer max upstream).
        scrollback_limit_bytes: match value("scrollback-limit-bytes")
            .ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?
        {
            "unlimited" => u64::MAX,
            value => value
                .parse::<u64>()
                .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?,
        },
        option_as_alt,
        confirm_close_surface,
    })
}

#[cfg(target_os = "macos")]
fn canonical_config_values<'a>(formatted: &'a str, key: &str) -> Vec<&'a str> {
    formatted
        .lines()
        .filter_map(|line| {
            let (candidate, value) = line.split_once(" = ")?;
            (candidate == key).then_some(value)
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn parse_rgb(value: Option<&str>) -> Result<Rgb, GhosttySurfaceRuntimeError> {
    parse_rgb_value(value.ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?)
}

#[cfg(target_os = "macos")]
fn parse_rgb_value(value: &str) -> Result<Rgb, GhosttySurfaceRuntimeError> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 {
        return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid);
    }
    let component = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&value[range], 16)
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)
    };
    Ok(Rgb {
        r: component(0..2)?,
        g: component(2..4)?,
        b: component(4..6)?,
    })
}

#[cfg(target_os = "macos")]
fn parse_terminal_configured_color(
    value: &str,
) -> Result<TerminalConfiguredColor, GhosttySurfaceRuntimeError> {
    match value {
        "cell-foreground" => Ok(TerminalConfiguredColor::CellForeground),
        "cell-background" => Ok(TerminalConfiguredColor::CellBackground),
        value => parse_rgb_value(value).map(TerminalConfiguredColor::Rgb),
    }
}

#[cfg(target_os = "macos")]
fn parse_config_bool(value: &str) -> Result<bool, GhosttySurfaceRuntimeError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    }
}

#[cfg(target_os = "macos")]
fn parse_config_f32(value: Option<&str>) -> Result<f32, GhosttySurfaceRuntimeError> {
    value
        .ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?
        .parse::<f32>()
        .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)
}

#[cfg(target_os = "macos")]
fn parse_metric_adjustment(
    value: Option<&str>,
) -> Result<TerminalMetricAdjustment, GhosttySurfaceRuntimeError> {
    let Some(value) = value else {
        return Ok(TerminalMetricAdjustment::None);
    };
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .parse::<f32>()
            .map(|value| TerminalMetricAdjustment::Percent(value / 100.0))
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid);
    }
    value
        .parse::<f32>()
        .map(TerminalMetricAdjustment::Absolute)
        .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)
}

#[cfg(target_os = "macos")]
fn parse_mouse_scroll_multiplier(
    value: Option<&str>,
) -> Result<(f32, f32), GhosttySurfaceRuntimeError> {
    let mut precision = 1.0;
    let mut discrete = 3.0;
    for part in value.unwrap_or("precision:1,discrete:3").split(',') {
        let Some((key, value)) = part.split_once(':') else {
            let value = part
                .parse::<f32>()
                .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
            return Ok((value, value));
        };
        let value = value
            .parse::<f32>()
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
        match key {
            "precision" => precision = value,
            "discrete" => discrete = value,
            _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
        }
    }
    Ok((precision, discrete))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GhosttySurfaceNsViewHandle {
    nsview: NonNull<c_void>,
}

impl GhosttySurfaceNsViewHandle {
    /// # Safety
    ///
    /// `nsview` must be an existing real AppKit `NSView` that remains valid until the eventual
    /// Ghostty surface config consumer finishes using the produced FFI struct.
    #[allow(dead_code)] // no caller: the surface host owns NSView creation now
    pub(crate) unsafe fn from_existing_nsview(nsview: NonNull<c_void>) -> Self {
        Self { nsview }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn from_terminal_native_view(native_view: RealTerminalNativeViewHandle) -> Self {
        Self {
            nsview: native_view.as_non_null(),
        }
    }

    fn as_ptr(self) -> *mut c_void {
        self.nsview.as_ptr()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GhosttySurfaceScaleFactor(f64);

impl GhosttySurfaceScaleFactor {
    pub(crate) fn new(scale_factor: f64) -> Result<Self, GhosttySurfaceConfigRequestError> {
        if scale_factor.is_finite() && scale_factor > 0.0 {
            Ok(Self(scale_factor))
        } else {
            Err(GhosttySurfaceConfigRequestError::InvalidScaleFactor(
                scale_factor,
            ))
        }
    }

    fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GhosttySurfaceConfigRequestError {
    InvalidScaleFactor(f64),
    LaunchPayloadContainsInteriorNul {
        field: GhosttySurfaceLaunchPayloadField,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GhosttySurfaceLaunchPayloadField {
    WorkingDirectory,
    Command,
    EnvVarKey,
    EnvVarValue,
    InitialInput,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GhosttySurfaceLaunchEnvVar {
    key: String,
    value: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GhosttySurfaceLaunchPayload {
    working_directory: Option<String>,
    command: Option<String>,
    env_vars: Vec<GhosttySurfaceLaunchEnvVar>,
    initial_input: Option<String>,
    wait_after_command: bool,
}

impl GhosttySurfaceLaunchPayload {
    pub(crate) fn try_new(
        working_directory: Option<String>,
        command: Option<String>,
        env_vars: Vec<(String, String)>,
        initial_input: Option<String>,
        wait_after_command: bool,
    ) -> Result<Self, GhosttySurfaceConfigRequestError> {
        validate_optional_launch_string(
            GhosttySurfaceLaunchPayloadField::WorkingDirectory,
            working_directory.as_deref(),
        )?;
        validate_optional_launch_string(
            GhosttySurfaceLaunchPayloadField::Command,
            command.as_deref(),
        )?;
        validate_optional_launch_string(
            GhosttySurfaceLaunchPayloadField::InitialInput,
            initial_input.as_deref(),
        )?;

        let env_vars = crate::terminal_environment::color_capable_terminal_env_vars(env_vars)
            .into_iter()
            .map(|(key, value)| {
                validate_launch_string(GhosttySurfaceLaunchPayloadField::EnvVarKey, &key)?;
                validate_launch_string(GhosttySurfaceLaunchPayloadField::EnvVarValue, &value)?;
                Ok(GhosttySurfaceLaunchEnvVar { key, value })
            })
            .collect::<Result<Vec<_>, GhosttySurfaceConfigRequestError>>()?;

        Ok(Self {
            working_directory,
            command,
            env_vars,
            initial_input,
            wait_after_command,
        })
    }

    fn env_var_count(&self) -> usize {
        self.env_vars.len()
    }
}

impl fmt::Debug for GhosttySurfaceLaunchPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GhosttySurfaceLaunchPayload")
            .field("has_working_directory", &self.working_directory.is_some())
            .field("has_command", &self.command.is_some())
            .field("env_var_count", &self.env_var_count())
            .field("has_initial_input", &self.initial_input.is_some())
            .field("wait_after_command", &self.wait_after_command)
            .finish()
    }
}

fn validate_optional_launch_string(
    field: GhosttySurfaceLaunchPayloadField,
    value: Option<&str>,
) -> Result<(), GhosttySurfaceConfigRequestError> {
    if let Some(value) = value {
        validate_launch_string(field, value)?;
    }
    Ok(())
}

fn validate_launch_string(
    field: GhosttySurfaceLaunchPayloadField,
    value: &str,
) -> Result<(), GhosttySurfaceConfigRequestError> {
    if value.as_bytes().contains(&0) {
        Err(GhosttySurfaceConfigRequestError::LaunchPayloadContainsInteriorNul { field })
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GhosttySurfaceTerminalConfig {
    font_size: f32,
}

impl GhosttySurfaceTerminalConfig {
    pub(crate) fn unmanaged() -> Self {
        Self { font_size: 0.0 }
    }

    pub(crate) fn with_font_size(font_size: f32) -> Self {
        Self { font_size }
    }

    fn font_size(self) -> f32 {
        self.font_size
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct GhosttySurfaceConfigRequest {
    nsview: GhosttySurfaceNsViewHandle,
    scale_factor: GhosttySurfaceScaleFactor,
    terminal_config: GhosttySurfaceTerminalConfig,
    launch_payload: Option<GhosttySurfaceLaunchPayload>,
}

impl GhosttySurfaceConfigRequest {
    pub(crate) fn new(
        nsview: GhosttySurfaceNsViewHandle,
        scale_factor: GhosttySurfaceScaleFactor,
    ) -> Self {
        Self {
            nsview,
            scale_factor,
            terminal_config: GhosttySurfaceTerminalConfig::unmanaged(),
            launch_payload: None,
        }
    }

    pub(crate) fn try_new(
        nsview: GhosttySurfaceNsViewHandle,
        scale_factor: f64,
    ) -> Result<Self, GhosttySurfaceConfigRequestError> {
        Ok(Self::new(
            nsview,
            GhosttySurfaceScaleFactor::new(scale_factor)?,
        ))
    }

    pub(crate) fn with_launch_payload(
        mut self,
        launch_payload: GhosttySurfaceLaunchPayload,
    ) -> Self {
        self.launch_payload = Some(launch_payload);
        self
    }

    pub(crate) fn with_terminal_config(
        mut self,
        terminal_config: GhosttySurfaceTerminalConfig,
    ) -> Self {
        self.terminal_config = terminal_config;
        self
    }

    pub(crate) fn set_terminal_config(&mut self, terminal_config: GhosttySurfaceTerminalConfig) {
        self.terminal_config = terminal_config;
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn try_from_terminal_native_view(
        native_view: RealTerminalNativeViewHandle,
        scale_factor: f64,
    ) -> Result<Self, GhosttySurfaceConfigRequestError> {
        Self::try_new(
            GhosttySurfaceNsViewHandle::from_terminal_native_view(native_view),
            scale_factor,
        )
    }

    #[allow(dead_code)] // no caller: the live path builds the FFI surface config through the surface host
    pub(crate) fn to_ffi_config(&self) -> ffi::ghostty_surface_config_s {
        assert!(
            self.launch_payload.is_none(),
            "launch-bearing Ghostty configs require scoped preparation"
        );
        let mut config = empty_ffi_surface_config();
        self.apply_base_to_ffi_config(&mut config);
        config
    }

    pub(crate) fn scale_factor(&self) -> f64 {
        self.scale_factor.get()
    }

    fn prepare_ffi_config(
        &self,
        mut config: ffi::ghostty_surface_config_s,
    ) -> GhosttySurfacePreparedConfig {
        self.apply_base_to_ffi_config(&mut config);
        GhosttySurfacePreparedConfig::new(config, self.launch_payload.as_ref())
    }

    fn apply_base_to_ffi_config(&self, config: &mut ffi::ghostty_surface_config_s) {
        /*
        CDXC:GPUITerminalSettings 2026-06-27-10:10:
        Embedded Ghostty surface requests can carry only the GhosttyKit-supported live/recreate FFI typography field, `font_size`; config-file-backed settings such as font family, theme, cursor, scrollback, clipboard, and mouse are intentionally not represented in `ghostty_surface_config_s`. A `font_size` of 0.0 remains the unmanaged Ghostty default for generic callers, while GPUI-owned request builders attach the shared Settings `terminalFontSize` value before creating Agents, command, or startup surfaces; live surface reload is not claimed here.
        */
        let nsview = self.nsview.as_ptr();

        config.platform_tag = ffi::GHOSTTY_PLATFORM_MACOS;
        config.platform = ffi::ghostty_platform_u {
            macos: ffi::ghostty_platform_macos_s { nsview },
        };
        config.userdata = nsview;
        config.scale_factor = self.scale_factor.get();
        config.font_size = self.terminal_config.font_size();
        config.working_directory = ptr::null();
        config.command = ptr::null();
        config.env_vars = ptr::null_mut();
        config.env_var_count = 0;
        config.initial_input = ptr::null();
        config.wait_after_command = false;
        config.context = ffi::GHOSTTY_SURFACE_CONTEXT_WINDOW;
    }
}

impl fmt::Debug for GhosttySurfaceConfigRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GhosttySurfaceConfigRequest")
            .field("scale_factor", &self.scale_factor.get())
            .field("has_launch_payload", &self.launch_payload.is_some())
            .field(
                "launch_env_var_count",
                &self
                    .launch_payload
                    .as_ref()
                    .map_or(0, GhosttySurfaceLaunchPayload::env_var_count),
            )
            .finish()
    }
}

struct GhosttySurfacePreparedConfig {
    config: ffi::ghostty_surface_config_s,
    _working_directory: Option<CString>,
    _command: Option<CString>,
    _env_keys: Vec<CString>,
    _env_values: Vec<CString>,
    _env_vars: Vec<ffi::ghostty_env_var_s>,
    _initial_input: Option<CString>,
}

impl GhosttySurfacePreparedConfig {
    fn new(
        mut config: ffi::ghostty_surface_config_s,
        launch_payload: Option<&GhosttySurfaceLaunchPayload>,
    ) -> Self {
        let Some(launch_payload) = launch_payload else {
            return Self {
                config,
                _working_directory: None,
                _command: None,
                _env_keys: Vec::new(),
                _env_values: Vec::new(),
                _env_vars: Vec::new(),
                _initial_input: None,
            };
        };

        let working_directory = launch_payload
            .working_directory
            .as_deref()
            .map(cstring_from_validated_launch_string);
        let command = launch_payload
            .command
            .as_deref()
            .map(cstring_from_validated_launch_string);
        let initial_input = launch_payload
            .initial_input
            .as_deref()
            .map(cstring_from_validated_launch_string);
        let env_keys = launch_payload
            .env_vars
            .iter()
            .map(|env_var| cstring_from_validated_launch_string(&env_var.key))
            .collect::<Vec<_>>();
        let env_values = launch_payload
            .env_vars
            .iter()
            .map(|env_var| cstring_from_validated_launch_string(&env_var.value))
            .collect::<Vec<_>>();
        let mut env_vars = env_keys
            .iter()
            .zip(env_values.iter())
            .map(|(key, value)| ffi::ghostty_env_var_s {
                key: key.as_ptr(),
                value: value.as_ptr(),
            })
            .collect::<Vec<_>>();

        config.working_directory = working_directory
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr());
        config.command = command.as_ref().map_or(ptr::null(), |value| value.as_ptr());
        config.initial_input = initial_input
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr());
        config.wait_after_command = launch_payload.wait_after_command;
        config.env_var_count = env_vars.len();
        config.env_vars = if env_vars.is_empty() {
            ptr::null_mut()
        } else {
            env_vars.as_mut_ptr()
        };

        Self {
            config,
            _working_directory: working_directory,
            _command: command,
            _env_keys: env_keys,
            _env_values: env_values,
            _env_vars: env_vars,
            _initial_input: initial_input,
        }
    }

    fn as_ptr(&self) -> *const ffi::ghostty_surface_config_s {
        &self.config
    }

    fn set_surface_userdata(&mut self, userdata: *mut c_void) {
        self.config.userdata = userdata;
    }
}

fn cstring_from_validated_launch_string(value: &str) -> CString {
    CString::new(value).expect("launch payload strings are validated before FFI preparation")
}

fn empty_ffi_surface_config() -> ffi::ghostty_surface_config_s {
    ffi::ghostty_surface_config_s {
        platform_tag: ffi::GHOSTTY_PLATFORM_INVALID,
        platform: ffi::ghostty_platform_u {
            macos: ffi::ghostty_platform_macos_s {
                nsview: ptr::null_mut(),
            },
        },
        userdata: ptr::null_mut(),
        scale_factor: 1.0,
        font_size: 0.0,
        working_directory: ptr::null(),
        command: ptr::null(),
        env_vars: ptr::null_mut(),
        env_var_count: 0,
        initial_input: ptr::null(),
        wait_after_command: false,
        context: ffi::GHOSTTY_SURFACE_CONTEXT_WINDOW,
    }
}

pub(crate) struct GhosttyConfigOwner {
    config: NonNull<c_void>,
    functions: GhosttyKitFunctionTable,
}

impl GhosttyConfigOwner {
    fn load_default_finalized_with_functions(
        functions: GhosttyKitFunctionTable,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        let config = unsafe { (functions.config_new)() };
        let config =
            NonNull::new(config).ok_or(GhosttySurfaceRuntimeError::ConfigCreateReturnedNull)?;
        let owner = Self { config, functions };

        unsafe {
            (functions.config_load_default_files)(owner.as_raw());
            (functions.config_finalize)(owner.as_raw());
        }

        Ok(owner)
    }

    fn as_raw(&self) -> ffi::ghostty_config_t {
        self.config.as_ptr()
    }
}

impl Drop for GhosttyConfigOwner {
    fn drop(&mut self) {
        unsafe {
            (self.functions.config_free)(self.as_raw());
        }
    }
}

struct GhosttyRuntimeCallbackState {
    app: AtomicPtr<c_void>,
    wakeup_requested: AtomicBool,
}

impl GhosttyRuntimeCallbackState {
    fn new() -> Self {
        Self {
            app: AtomicPtr::new(ptr::null_mut()),
            wakeup_requested: AtomicBool::new(false),
        }
    }

    fn mark_app_ready(&self, app: ffi::ghostty_app_t) {
        self.app.store(app, Ordering::SeqCst);
    }
}

pub(crate) struct GhosttyAppOwner {
    app: NonNull<c_void>,
    #[allow(dead_code)]
    // ownership handle: held so the ghostty app keeps its config/runtime-config alive for the C side, never read from Rust
    config: GhosttyConfigOwner,
    runtime_state: Box<GhosttyRuntimeCallbackState>,
    #[allow(dead_code)]
    // ownership handle: held so the ghostty app keeps its config/runtime-config alive for the C side, never read from Rust
    runtime_config: ffi::ghostty_runtime_config_s,
    functions: GhosttyKitFunctionTable,
    latest_focus_state: Option<bool>,
}

impl GhosttyAppOwner {
    #[cfg(target_os = "macos")]
    pub(crate) fn new() -> Result<Self, GhosttySurfaceRuntimeError> {
        let functions = GhosttyKitFunctionTable::production();
        initialize_production_ghostty_once(functions)?;
        Self::new_after_runtime_init(functions)
    }

    fn new_after_runtime_init(
        functions: GhosttyKitFunctionTable,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        let config = GhosttyConfigOwner::load_default_finalized_with_functions(functions)?;
        let runtime_state = Box::new(GhosttyRuntimeCallbackState::new());
        let runtime_config = runtime_config_for_state(&runtime_state);
        let app = unsafe { (functions.app_new)(&runtime_config, config.as_raw()) };
        let app = NonNull::new(app).ok_or(GhosttySurfaceRuntimeError::AppCreateReturnedNull)?;
        runtime_state.mark_app_ready(app.as_ptr());

        Ok(Self {
            app,
            config,
            runtime_state,
            runtime_config,
            functions,
            latest_focus_state: None,
        })
    }

    fn as_raw(&self) -> ffi::ghostty_app_t {
        self.app.as_ptr()
    }

    pub(crate) fn tick(&self) {
        unsafe {
            (self.functions.app_tick)(self.as_raw());
        }
    }

    pub(crate) fn tick_if_woken(&self) {
        if self
            .runtime_state
            .wakeup_requested
            .swap(false, Ordering::SeqCst)
        {
            self.tick();
        }
    }

    pub(crate) fn set_focus(&mut self, focused: bool) {
        if self.latest_focus_state == Some(focused) {
            return;
        }
        unsafe {
            (self.functions.app_set_focus)(self.as_raw(), focused);
        }
        self.latest_focus_state = Some(focused);
    }
}

impl Drop for GhosttyAppOwner {
    fn drop(&mut self) {
        self.runtime_state
            .app
            .store(ptr::null_mut(), Ordering::SeqCst);
        unsafe {
            (self.functions.app_free)(self.as_raw());
        }
    }
}

/*
CDXC:GPUITerminalClipboard 2026-06-27-04:10:
Ghostty runtime clipboard callbacks must never touch GPUI App clipboard APIs directly. Embedded Ghostty passes surface userdata to clipboard callbacks, so GPUI may accept only registered surface close tokens with mounted surfaces, enqueue owner-local standard-clipboard operations, and let the app-thread drain perform explicit clipboard access for the exact Agents or command surface owner.

CDXC:GPUITerminalClipboard 2026-06-27-04:10:
The low-level Ghostty clipboard path is surface-scoped only. Runtime app userdata, null request state, selection clipboard requests, missing `text/plain` write payloads, or focused-surface inference must not authorize clipboard access. Initial reads complete as unconfirmed, confirm callbacks borrow the original content pointer only for synchronous completion, and callbacks must not log, persist, or retain raw clipboard diagnostics beyond the owner-local queue.
*/
const GHOSTTY_RUNTIME_SUPPORTS_SELECTION_CLIPBOARD: bool = false;
const GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_MIME: &[u8] = b"text/plain";
const GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_C_STRING: &[u8] = b"text/plain\0";

fn runtime_config_for_state(state: &GhosttyRuntimeCallbackState) -> ffi::ghostty_runtime_config_s {
    ffi::ghostty_runtime_config_s {
        userdata: state as *const GhosttyRuntimeCallbackState as *mut c_void,
        supports_selection_clipboard: GHOSTTY_RUNTIME_SUPPORTS_SELECTION_CLIPBOARD,
        wakeup_cb: Some(ghostty_runtime_wakeup_cb),
        action_cb: Some(ghostty_runtime_action_cb),
        read_clipboard_cb: Some(ghostty_runtime_read_clipboard_cb),
        confirm_read_clipboard_cb: Some(ghostty_runtime_confirm_read_clipboard_cb),
        write_clipboard_cb: Some(ghostty_runtime_write_clipboard_cb),
        close_surface_cb: Some(ghostty_runtime_close_surface_cb),
    }
}

unsafe extern "C" fn ghostty_runtime_wakeup_cb(userdata: *mut c_void) {
    let Some(state) = NonNull::new(userdata as *mut GhosttyRuntimeCallbackState) else {
        return;
    };
    unsafe {
        state
            .as_ref()
            .wakeup_requested
            .store(true, Ordering::SeqCst);
    }
}

/// Dispatches Ghostty runtime actions to the owning surface's app-thread
/// queue. Only surface-targeted, product-relevant tags are handled; returning
/// false leaves the remaining tags to Ghostty's default behavior, matching the
/// macOS host's dispatcher in TerminalWorkspaceView.
unsafe extern "C" fn ghostty_runtime_action_cb(
    _app: ffi::ghostty_app_t,
    target: ffi::ghostty_target_s,
    action: ffi::ghostty_action_s,
) -> bool {
    let Some(event) = (unsafe { runtime_action_event_from_action(action) }) else {
        return false;
    };
    let Some(token) = registered_surface_close_token_for_action_target(target) else {
        return false;
    };
    unsafe {
        token.as_ref().enqueue_runtime_action_event(event);
    }
    true
}

unsafe fn runtime_action_event_from_action(
    action: ffi::ghostty_action_s,
) -> Option<GhosttyRuntimeActionEvent> {
    match action.tag {
        ffi::GHOSTTY_ACTION_OPEN_URL => {
            let open_url = unsafe { action.action.open_url };
            let url = unsafe { runtime_action_sized_string(open_url.url, open_url.len) }?;
            Some(GhosttyRuntimeActionEvent::OpenUrl { url })
        }
        ffi::GHOSTTY_ACTION_RING_BELL => Some(GhosttyRuntimeActionEvent::RingBell),
        ffi::GHOSTTY_ACTION_SET_TITLE => {
            let title = unsafe { runtime_action_c_string(action.action.set_title.title) }?;
            Some(GhosttyRuntimeActionEvent::SetTitle { title })
        }
        ffi::GHOSTTY_ACTION_PWD => {
            let pwd = unsafe { runtime_action_c_string(action.action.pwd.pwd) }?;
            Some(GhosttyRuntimeActionEvent::Pwd { pwd })
        }
        ffi::GHOSTTY_ACTION_MOUSE_OVER_LINK => {
            let link = unsafe { action.action.mouse_over_link };
            let url = unsafe { runtime_action_sized_string(link.url, link.len) };
            Some(GhosttyRuntimeActionEvent::MouseOverLink { url })
        }
        ffi::GHOSTTY_ACTION_START_SEARCH => {
            let needle = unsafe { runtime_action_c_string(action.action.start_search.needle) };
            Some(GhosttyRuntimeActionEvent::StartSearch { needle })
        }
        ffi::GHOSTTY_ACTION_END_SEARCH => Some(GhosttyRuntimeActionEvent::EndSearch),
        ffi::GHOSTTY_ACTION_SEARCH_TOTAL => {
            let total = unsafe { action.action.search_total.total };
            Some(GhosttyRuntimeActionEvent::SearchTotal {
                total: (total >= 0).then_some(total as u64),
            })
        }
        ffi::GHOSTTY_ACTION_SEARCH_SELECTED => {
            let selected = unsafe { action.action.search_selected.selected };
            Some(GhosttyRuntimeActionEvent::SearchSelected {
                selected: (selected >= 0).then_some(selected as u64),
            })
        }
        _ => None,
    }
}

unsafe fn runtime_action_c_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    if text.is_empty() { None } else { Some(text) }
}

unsafe fn runtime_action_sized_string(value: *const c_char, len: usize) -> Option<String> {
    if value.is_null() || len == 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), len) };
    let text = String::from_utf8_lossy(bytes).into_owned();
    if text.is_empty() { None } else { Some(text) }
}

/*
CDXC:GPUITerminalClipboard 2026-06-27-04:10:
Ghostty's embedded runtime calls clipboard callbacks with `SurfaceUD`, while wakeup/action still use app-level runtime userdata. Validate the pointer against registered surface tokens before casting, enqueue only standard reads and explicit `text/plain` standard writes, complete initial reads as unconfirmed so Ghostty paste protection can ask back through `confirm_read_clipboard_cb`, and mirror native Ghostex by confirming the borrowed callback content synchronously without storing or logging it.
*/
unsafe extern "C" fn ghostty_runtime_read_clipboard_cb(
    userdata: *mut c_void,
    clipboard: ffi::ghostty_clipboard_e,
    state: *mut c_void,
    mimes: *const *const c_char,
    mimes_len: usize,
    list: bool,
) -> ffi::ghostty_clipboard_read_result_e {
    if clipboard != ffi::GHOSTTY_CLIPBOARD_STANDARD || state.is_null() {
        return ffi::GHOSTTY_CLIPBOARD_READ_UNSUPPORTED;
    }
    if list || !(unsafe { runtime_clipboard_requests_text_plain(mimes, mimes_len) }) {
        return ffi::GHOSTTY_CLIPBOARD_READ_UNAVAILABLE;
    }
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return ffi::GHOSTTY_CLIPBOARD_READ_UNSUPPORTED;
    };
    if unsafe { token.as_ref().enqueue_runtime_clipboard_read(state) } {
        ffi::GHOSTTY_CLIPBOARD_READ_STARTED
    } else {
        ffi::GHOSTTY_CLIPBOARD_READ_UNAVAILABLE
    }
}

unsafe extern "C" fn ghostty_runtime_confirm_read_clipboard_cb(
    userdata: *mut c_void,
    confirm: *const ffi::ghostty_clipboard_confirm_s,
    state: *mut c_void,
    request: ffi::ghostty_clipboard_request_e,
) {
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return;
    };
    if confirm.is_null()
        || state.is_null()
        || !runtime_clipboard_confirm_read_request_supported(request)
    {
        unsafe { token.as_ref().deny_runtime_clipboard_request(state) };
        return;
    }
    unsafe {
        token
            .as_ref()
            .complete_runtime_clipboard_confirmation(confirm, state)
    };
}

unsafe extern "C" fn ghostty_runtime_write_clipboard_cb(
    userdata: *mut c_void,
    clipboard: ffi::ghostty_clipboard_e,
    content: *const ffi::ghostty_clipboard_content_s,
    len: usize,
    _confirm: bool,
) {
    if clipboard != ffi::GHOSTTY_CLIPBOARD_STANDARD {
        return;
    }
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return;
    };
    let Some(text) = (unsafe { runtime_clipboard_text_plain_content(content, len) }) else {
        return;
    };
    unsafe {
        token.as_ref().enqueue_runtime_clipboard_write(text);
    }
}

const GHOSTTY_SURFACE_CLOSE_STATE_NONE: u8 = 0;
const GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED: u8 = 1;
const GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED: u8 = 2;

/*
CDXC:GPUITerminalClipboard 2026-06-27-04:10:
Owner-local clipboard operations are the only bridge from Ghostty callbacks to GPUI clipboard APIs. A denied drain completes pending reads with empty data and drops writes without invoking clipboard closures; an allowed drain may read/write only for the exact mounted owner that holds this token, never through focus, app-level userdata, logs, persistence, or selection clipboard routing.
*/
enum GhosttyRuntimeClipboardOperation {
    ReadStandard { state: *mut c_void },
    WriteStandardText { text: String },
}

/// Bound on undrained per-surface runtime action events. The app thread drains
/// every frame while a surface is mounted, so the cap only matters for surfaces
/// that emit actions while hidden; oldest events drop first.
const GHOSTTY_RUNTIME_ACTION_EVENT_QUEUE_LIMIT: usize = 128;

/// Ghostty runtime actions surfaced to the app thread. Strings are copied out
/// of the callback-scoped C pointers before crossing threads.
#[derive(Clone, Debug)]
pub(crate) enum GhosttyRuntimeActionEvent {
    OpenUrl { url: String },
    RingBell,
    SetTitle { title: String },
    Pwd { pwd: String },
    MouseOverLink { url: Option<String> },
    StartSearch { needle: Option<String> },
    EndSearch,
    SearchTotal { total: Option<u64> },
    SearchSelected { selected: Option<u64> },
}

struct GhosttySurfaceCloseToken {
    close_state: AtomicU8,
    surface: AtomicPtr<c_void>,
    surface_complete_clipboard_request:
        unsafe fn(ffi::ghostty_surface_t, *const ffi::ghostty_clipboard_complete_s, *mut c_void),
    surface_deny_clipboard_request: unsafe fn(ffi::ghostty_surface_t, *mut c_void),
    runtime_clipboard_operations: Mutex<VecDeque<GhosttyRuntimeClipboardOperation>>,
    runtime_action_events: Mutex<VecDeque<GhosttyRuntimeActionEvent>>,
}

impl GhosttySurfaceCloseToken {
    fn boxed(functions: GhosttyKitFunctionTable) -> Box<Self> {
        let token = Box::new(Self::new(functions));
        token.register_surface_userdata();
        token
    }

    fn new(functions: GhosttyKitFunctionTable) -> Self {
        Self {
            close_state: AtomicU8::new(GHOSTTY_SURFACE_CLOSE_STATE_NONE),
            surface: AtomicPtr::new(ptr::null_mut()),
            surface_complete_clipboard_request: functions.surface_complete_clipboard_request,
            surface_deny_clipboard_request: functions.surface_deny_clipboard_request,
            runtime_clipboard_operations: Mutex::new(VecDeque::new()),
            runtime_action_events: Mutex::new(VecDeque::new()),
        }
    }

    fn as_userdata(&self) -> *mut c_void {
        self as *const GhosttySurfaceCloseToken as *mut c_void
    }

    fn userdata_key(&self) -> usize {
        self.as_userdata() as usize
    }

    fn register_surface_userdata(&self) {
        if let Ok(mut tokens) = ghostty_surface_close_token_registry().lock() {
            tokens.insert(self.userdata_key());
        }
    }

    fn unregister_surface_userdata(&self) {
        if let Ok(mut tokens) = ghostty_surface_close_token_registry().lock() {
            tokens.remove(&self.userdata_key());
        }
    }

    fn set_surface(&self, surface: ffi::ghostty_surface_t) {
        self.surface.store(surface, Ordering::SeqCst);
        // Runtime action callbacks identify surfaces by pointer (not userdata),
        // so keep a surface-pointer index alongside the userdata registry.
        if !surface.is_null() {
            if let Ok(mut surfaces) = ghostty_surface_action_token_registry().lock() {
                surfaces.insert(surface as usize, self.userdata_key());
            }
        }
    }

    fn clear_surface(&self) {
        let previous = self.surface.swap(ptr::null_mut(), Ordering::SeqCst);
        if previous.is_null() {
            return;
        }
        if let Ok(mut surfaces) = ghostty_surface_action_token_registry().lock() {
            surfaces.remove(&(previous as usize));
        }
    }

    fn runtime_surface(&self) -> Option<ffi::ghostty_surface_t> {
        NonNull::new(self.surface.load(Ordering::SeqCst)).map(NonNull::as_ptr)
    }

    fn record_close_callback(&self, confirmation_needed: bool) {
        let state = if confirmation_needed {
            GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED
        } else {
            GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED
        };
        self.close_state.store(state, Ordering::SeqCst);
    }

    fn enqueue_runtime_action_event(&self, event: GhosttyRuntimeActionEvent) {
        if self.runtime_surface().is_none() {
            return;
        }
        if let Ok(mut events) = self.runtime_action_events.lock() {
            if events.len() >= GHOSTTY_RUNTIME_ACTION_EVENT_QUEUE_LIMIT {
                events.pop_front();
            }
            events.push_back(event);
        }
    }

    fn take_runtime_action_events(&self) -> VecDeque<GhosttyRuntimeActionEvent> {
        self.runtime_action_events
            .lock()
            .map(|mut events| mem::take(&mut *events))
            .unwrap_or_default()
    }

    fn enqueue_runtime_clipboard_read(&self, state: *mut c_void) -> bool {
        if self.runtime_surface().is_none() {
            return false;
        }
        let Ok(mut operations) = self.runtime_clipboard_operations.lock() else {
            return false;
        };
        operations.push_back(GhosttyRuntimeClipboardOperation::ReadStandard { state });
        true
    }

    fn enqueue_runtime_clipboard_write(&self, text: String) {
        if self.runtime_surface().is_none() || text.is_empty() {
            return;
        }
        if let Ok(mut operations) = self.runtime_clipboard_operations.lock() {
            operations.push_back(GhosttyRuntimeClipboardOperation::WriteStandardText { text });
        }
    }

    fn drain_runtime_clipboard_operations(
        &self,
        allow_standard_clipboard: bool,
        mut read_standard_text: impl FnMut() -> Option<String>,
        mut write_standard_text: impl FnMut(String),
    ) {
        let operations = self.take_runtime_clipboard_operations();
        for operation in operations {
            match operation {
                GhosttyRuntimeClipboardOperation::ReadStandard { state } => {
                    let text = if allow_standard_clipboard {
                        read_standard_text()
                    } else {
                        None
                    };
                    self.complete_runtime_clipboard_read(state, text);
                }
                GhosttyRuntimeClipboardOperation::WriteStandardText { text } => {
                    if allow_standard_clipboard {
                        write_standard_text(text);
                    }
                }
            }
        }
    }

    fn deny_pending_runtime_clipboard_operations(&self) {
        let operations = self.take_runtime_clipboard_operations();
        for operation in operations {
            if let GhosttyRuntimeClipboardOperation::ReadStandard { state } = operation {
                self.deny_runtime_clipboard_request(state);
            }
        }
    }

    fn take_runtime_clipboard_operations(&self) -> VecDeque<GhosttyRuntimeClipboardOperation> {
        self.runtime_clipboard_operations
            .lock()
            .map(|mut operations| mem::take(&mut *operations))
            .unwrap_or_default()
    }

    fn complete_runtime_clipboard_read(&self, state: *mut c_void, text: Option<String>) {
        let text = text.map(String::into_bytes);
        let content = text.as_ref().map(|bytes| ffi::ghostty_clipboard_content_s {
            mime: GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_C_STRING
                .as_ptr()
                .cast(),
            data: bytes.as_ptr().cast(),
            len: bytes.len(),
        });
        let complete = ffi::ghostty_clipboard_complete_s {
            contents: content
                .as_ref()
                .map_or(ptr::null(), |content| content as *const _),
            contents_len: usize::from(content.is_some()),
            available: ptr::null(),
            available_len: 0,
            confirmed: false,
            remember: false,
        };
        self.complete_runtime_clipboard_request(&complete, state);
    }

    fn complete_runtime_clipboard_request(
        &self,
        complete: *const ffi::ghostty_clipboard_complete_s,
        state: *mut c_void,
    ) {
        let Some(surface) = self.runtime_surface() else {
            return;
        };
        unsafe {
            (self.surface_complete_clipboard_request)(surface, complete, state);
        }
    }

    unsafe fn complete_runtime_clipboard_confirmation(
        &self,
        confirm: *const ffi::ghostty_clipboard_confirm_s,
        state: *mut c_void,
    ) {
        let Some(confirm) = (unsafe { confirm.as_ref() }) else {
            self.deny_runtime_clipboard_request(state);
            return;
        };
        let complete = ffi::ghostty_clipboard_complete_s {
            contents: confirm.contents,
            contents_len: confirm.contents_len,
            available: confirm.available,
            available_len: confirm.available_len,
            confirmed: true,
            remember: false,
        };
        self.complete_runtime_clipboard_request(&complete, state);
    }

    fn deny_runtime_clipboard_request(&self, state: *mut c_void) {
        let Some(surface) = self.runtime_surface() else {
            return;
        };
        unsafe { (self.surface_deny_clipboard_request)(surface, state) };
    }

    fn consume_confirmed_close_requested(&self) -> bool {
        self.close_state
            .compare_exchange(
                GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED,
                GHOSTTY_SURFACE_CLOSE_STATE_NONE,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }

    fn consume_confirmation_needed_close_requested(&self) -> bool {
        self.close_state
            .compare_exchange(
                GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED,
                GHOSTTY_SURFACE_CLOSE_STATE_NONE,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }

    fn confirmed_close_pending(&self) -> bool {
        self.close_state.load(Ordering::SeqCst) == GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMED
    }

    fn clear_confirmation_needed_close_requested(&self) {
        let _ = self.close_state.compare_exchange(
            GHOSTTY_SURFACE_CLOSE_STATE_CONFIRMATION_NEEDED,
            GHOSTTY_SURFACE_CLOSE_STATE_NONE,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
}

impl Drop for GhosttySurfaceCloseToken {
    fn drop(&mut self) {
        self.clear_surface();
        self.unregister_surface_userdata();
    }
}

fn ghostty_surface_action_token_registry() -> &'static Mutex<HashMap<usize, usize>> {
    static SURFACES: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    SURFACES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn registered_surface_close_token_for_action_target(
    target: ffi::ghostty_target_s,
) -> Option<NonNull<GhosttySurfaceCloseToken>> {
    if target.tag != ffi::GHOSTTY_TARGET_SURFACE {
        return None;
    }
    let surface = unsafe { target.target.surface };
    if surface.is_null() {
        return None;
    }
    let userdata = ghostty_surface_action_token_registry()
        .lock()
        .ok()?
        .get(&(surface as usize))
        .copied()?;
    registered_surface_close_token_from_userdata(userdata as *mut c_void)
}

fn ghostty_surface_close_token_registry() -> &'static Mutex<HashSet<usize>> {
    static TOKENS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    TOKENS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn registered_surface_close_token_from_userdata(
    userdata: *mut c_void,
) -> Option<NonNull<GhosttySurfaceCloseToken>> {
    let token = NonNull::new(userdata as *mut GhosttySurfaceCloseToken)?;
    let is_registered = ghostty_surface_close_token_registry()
        .lock()
        .map(|tokens| tokens.contains(&(userdata as usize)))
        .unwrap_or(false);
    if is_registered { Some(token) } else { None }
}

fn runtime_clipboard_confirm_read_request_supported(
    request: ffi::ghostty_clipboard_request_e,
) -> bool {
    request == ffi::GHOSTTY_CLIPBOARD_REQUEST_PASTE
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_KITTY_READ
        || request == ffi::GHOSTTY_CLIPBOARD_REQUEST_KITTY_WRITE
}

unsafe fn runtime_clipboard_requests_text_plain(mimes: *const *const c_char, len: usize) -> bool {
    if mimes.is_null() || len == 0 {
        return false;
    }
    unsafe { std::slice::from_raw_parts(mimes, len) }
        .iter()
        .copied()
        .filter(|mime| !mime.is_null())
        .any(|mime| {
            unsafe { CStr::from_ptr(mime) }.to_bytes() == GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_MIME
        })
}

unsafe fn runtime_clipboard_text_plain_content(
    content: *const ffi::ghostty_clipboard_content_s,
    len: usize,
) -> Option<String> {
    if content.is_null() || len == 0 {
        return None;
    }
    for entry in unsafe { std::slice::from_raw_parts(content, len) } {
        if entry.mime.is_null() || entry.data.is_null() {
            continue;
        }
        let mime = unsafe { CStr::from_ptr(entry.mime) };
        if mime.to_bytes() != GHOSTTY_RUNTIME_CLIPBOARD_TEXT_PLAIN_MIME {
            continue;
        }
        let data = unsafe { std::slice::from_raw_parts(entry.data.cast::<u8>(), entry.len) };
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        return Some(text.to_string());
    }
    None
}

unsafe extern "C" fn ghostty_runtime_close_surface_cb(
    userdata: *mut c_void,
    confirmation_needed: bool,
) {
    /*
    CDXC:GPUITerminalGhosttyClose 2026-06-27-04:25:
    Ghostty close callbacks use the same surface userdata channel as clipboard callbacks. Validate the pointer against registered surface tokens before mutating owner-local close state so app-level runtime userdata and stale pointers cannot be treated as terminal owners.
    */
    let Some(token) = registered_surface_close_token_from_userdata(userdata) else {
        return;
    };
    unsafe {
        token.as_ref().record_close_callback(confirmation_needed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GhosttySurfacePixelSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl GhosttySurfacePixelSize {
    pub(crate) fn from_gpui_bounds(
        bounds: Bounds<Pixels>,
        scale_factor: f64,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        let scale_factor = GhosttySurfaceScaleFactor::new(scale_factor)?;
        Ok(Self {
            width: scaled_pixel_dimension(
                GhosttySurfaceBoundsField::Width,
                f64::from(bounds.size.width.as_f32()),
                scale_factor,
            )?,
            height: scaled_pixel_dimension(
                GhosttySurfaceBoundsField::Height,
                f64::from(bounds.size.height.as_f32()),
                scale_factor,
            )?,
        })
    }
}

fn scaled_pixel_dimension(
    field: GhosttySurfaceBoundsField,
    value: f64,
    scale_factor: GhosttySurfaceScaleFactor,
) -> Result<u32, GhosttySurfaceRuntimeError> {
    if !value.is_finite() || value < 0.0 {
        return Err(GhosttySurfaceRuntimeError::InvalidBounds { field, value });
    }

    let scaled = (value * scale_factor.get()).floor().max(1.0);
    if !scaled.is_finite() || scaled > f64::from(u32::MAX) {
        return Err(GhosttySurfaceRuntimeError::InvalidBounds { field, value });
    }

    Ok(scaled as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // no constructor: key-binding probing is done by ghostty itself now
pub(crate) struct GhosttySurfaceKeyBindingStatus {
    binding: bool,
    flags: ffi::ghostty_binding_flags_e,
}

impl GhosttySurfaceKeyBindingStatus {
    #[allow(dead_code)] // no live caller: key-binding probing is only reachable from the superseded native key path
    fn from_ffi_result(binding: bool, flags: ffi::ghostty_binding_flags_e) -> Self {
        Self {
            binding,
            flags: if binding { flags } else { 0 },
        }
    }

    #[allow(dead_code)] // no live caller: key-binding probing is only reachable from the superseded native key path
    pub(crate) fn binding(self) -> bool {
        self.binding
    }

    #[allow(dead_code)] // no live caller: key-binding probing is only reachable from the superseded native key path
    pub(crate) fn flags(self) -> ffi::ghostty_binding_flags_e {
        self.flags
    }
}

static GHOSTTY_SURFACE_EMPTY_TEXT_SENTINEL: [u8; 1] = [0];

fn ghostty_surface_text_ptr(bytes: &[u8]) -> *const c_char {
    if bytes.is_empty() {
        GHOSTTY_SURFACE_EMPTY_TEXT_SENTINEL.as_ptr() as *const c_char
    } else {
        bytes.as_ptr() as *const c_char
    }
}

fn ghostty_surface_preedit_ptr(bytes: &[u8]) -> *const c_char {
    if bytes.is_empty() {
        ptr::null()
    } else {
        bytes.as_ptr() as *const c_char
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GhosttySurfaceImePoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GhosttySurfaceMetadataSnapshot {
    process_exited: bool,
    foreground_process_id_present: bool,
    tty_name_present: bool,
}

impl GhosttySurfaceMetadataSnapshot {
    pub(crate) fn from_redacted_presence(
        process_exited: bool,
        foreground_process_id_present: bool,
        tty_name_present: bool,
    ) -> Self {
        Self {
            process_exited,
            foreground_process_id_present,
            tty_name_present,
        }
    }

    pub(crate) fn process_exited(self) -> bool {
        self.process_exited
    }

    #[allow(dead_code)] // no live caller: only the superseded startup-host reconcile read surface metadata presence
    pub(crate) fn foreground_process_id_present(self) -> bool {
        self.foreground_process_id_present
    }

    #[allow(dead_code)] // no live caller: only the superseded startup-host reconcile read surface metadata presence
    pub(crate) fn tty_name_present(self) -> bool {
        self.tty_name_present
    }

    pub(crate) fn indicates_ready_metadata(self) -> bool {
        !self.process_exited && self.foreground_process_id_present && self.tty_name_present
    }
}

#[allow(dead_code)] // no live caller: only reachable from the superseded startup-host reconcile
fn ghostty_surface_metadata_snapshot(
    functions: GhosttyKitFunctionTable,
    surface: ffi::ghostty_surface_t,
) -> GhosttySurfaceMetadataSnapshot {
    let process_exited = unsafe { (functions.surface_process_exited)(surface) };
    let foreground_process_id_present = unsafe { (functions.surface_foreground_pid)(surface) } != 0;
    let tty_name = unsafe { (functions.surface_tty_name)(surface) };
    let tty_name_present = !tty_name.ptr.is_null() && tty_name.len > 0;
    unsafe {
        (functions.string_free)(tty_name);
    }

    GhosttySurfaceMetadataSnapshot::from_redacted_presence(
        process_exited,
        foreground_process_id_present,
        tty_name_present,
    )
}

pub(crate) struct GhosttySurfaceOwner<SlotId = AgentsTerminalBodyMountSlotId> {
    surface: NonNull<c_void>,
    mount_slot_id: SlotId,
    runtime_session_id: AgentsTerminalRuntimeSessionId,
    functions: GhosttyKitFunctionTable,
    close_token: Box<GhosttySurfaceCloseToken>,
    close_requested: bool,
    latest_scale_factor: Option<GhosttySurfaceScaleFactor>,
    latest_pixel_size: Option<GhosttySurfacePixelSize>,
    latest_focus_state: Option<bool>,
    latest_occlusion_state: bool,
}

impl<SlotId> GhosttySurfaceOwner<SlotId>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    pub(crate) fn new(
        app: &GhosttyAppOwner,
        mount_slot_id: SlotId,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        request: &GhosttySurfaceConfigRequest,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        let close_token = GhosttySurfaceCloseToken::boxed(app.functions);
        let (surface, functions) =
            create_ghostty_surface_from_request(app, request, close_token.as_userdata())?;
        close_token.set_surface(surface.as_ptr());
        Ok(Self {
            surface,
            mount_slot_id,
            runtime_session_id,
            functions,
            close_token,
            close_requested: false,
            latest_scale_factor: None,
            latest_pixel_size: None,
            latest_focus_state: None,
            latest_occlusion_state: true,
        })
    }

    pub(crate) fn mount_slot_id(&self) -> SlotId {
        self.mount_slot_id
    }

    pub(crate) fn runtime_session_id(&self) -> AgentsTerminalRuntimeSessionId {
        self.runtime_session_id
    }

    pub(crate) fn can_rekey_to_mount_slot(
        &self,
        mount_slot_id: SlotId,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> bool {
        self.mount_slot_id == mount_slot_id && self.runtime_session_id == runtime_session_id
    }

    pub(crate) fn can_move_to_mount_slot(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> bool {
        self.runtime_session_id == runtime_session_id
    }

    pub(crate) fn into_rekeyed_surface_owner(
        self,
        mount_slot_id: SlotId,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Self {
        /*
        CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
        Parked Running owners must reattach by moving the same Ghostty surface back under the current body slot. `ManuallyDrop` prevents `ghostty_surface_free` during the rekey and keeps the close/clipboard token with the surface userdata so reattach cannot recreate the process or lose owner-scoped runtime callbacks.
        */
        let owner = ManuallyDrop::new(self);
        let close_token = unsafe { ptr::read(&owner.close_token) };
        Self {
            surface: owner.surface,
            mount_slot_id,
            runtime_session_id,
            functions: owner.functions,
            close_token,
            close_requested: owner.close_requested,
            latest_scale_factor: owner.latest_scale_factor,
            latest_pixel_size: owner.latest_pixel_size,
            latest_focus_state: None,
            latest_occlusion_state: owner.latest_occlusion_state,
        }
    }

    pub(crate) fn update_content_scale_and_size(
        &mut self,
        bounds: Bounds<Pixels>,
        scale_factor: f64,
    ) -> Result<(), GhosttySurfaceRuntimeError> {
        let scale_factor = GhosttySurfaceScaleFactor::new(scale_factor)?;
        let pixel_size = GhosttySurfacePixelSize::from_gpui_bounds(bounds, scale_factor.get())?;
        self.set_content_scale(scale_factor);
        self.set_size_pixels(pixel_size);
        Ok(())
    }

    pub(crate) fn set_focus(&mut self, focused: bool) {
        if self.latest_focus_state == Some(focused) {
            return;
        }
        unsafe {
            (self.functions.surface_set_focus)(self.as_raw(), focused);
        }
        self.latest_focus_state = Some(focused);
    }

    pub(crate) fn set_occlusion(&mut self, visible: bool) {
        if self.latest_occlusion_state == visible {
            return;
        }
        unsafe {
            (self.functions.surface_set_occlusion)(self.as_raw(), visible);
        }
        self.latest_occlusion_state = visible;
    }

    pub(crate) fn surface_size(&self) -> ffi::ghostty_surface_size_s {
        unsafe { (self.functions.surface_size)(self.as_raw()) }
    }

    #[allow(dead_code)] // no live caller: the native ghostty surface host owns this today
    pub(crate) fn metadata_snapshot(&self) -> GhosttySurfaceMetadataSnapshot {
        ghostty_surface_metadata_snapshot(self.functions, self.as_raw())
    }

    pub(crate) fn process_exited(&self) -> bool {
        unsafe { (self.functions.surface_process_exited)(self.as_raw()) }
    }

    pub(crate) fn needs_confirm_quit(&self) -> bool {
        unsafe { (self.functions.surface_needs_confirm_quit)(self.as_raw()) }
    }

    /// Performs a named Ghostty keybind action (e.g. `start_search`,
    /// `search:<needle>`, `navigate_search:next`, `end_search`) on this
    /// surface, mirroring the macOS host's `performBindingAction`.
    pub(crate) fn perform_binding_action(&self, action: &str) -> bool {
        unsafe {
            (self.functions.surface_binding_action)(
                self.as_raw(),
                action.as_ptr().cast(),
                action.len(),
            )
        }
    }

    #[allow(dead_code)] // no live caller: the native ghostty surface host owns this today
    pub(crate) fn key_translation_mods(
        &self,
        mods: ffi::ghostty_input_mods_e,
    ) -> ffi::ghostty_input_mods_e {
        unsafe { (self.functions.surface_key_translation_mods)(self.as_raw(), mods) }
    }

    pub(crate) fn send_key(&self, event: ffi::ghostty_input_key_s) -> bool {
        unsafe { (self.functions.surface_key)(self.as_raw(), event) }
    }

    #[allow(dead_code)] // no live caller: the native ghostty surface host owns this today
    pub(crate) fn key_is_binding(
        &self,
        event: ffi::ghostty_input_key_s,
    ) -> GhosttySurfaceKeyBindingStatus {
        let mut flags = 0;
        let binding =
            unsafe { (self.functions.surface_key_is_binding)(self.as_raw(), event, &mut flags) };
        GhosttySurfaceKeyBindingStatus::from_ffi_result(binding, flags)
    }

    pub(crate) fn send_text_bytes(&self, bytes: &[u8]) {
        unsafe {
            (self.functions.surface_text)(
                self.as_raw(),
                ghostty_surface_text_ptr(bytes),
                bytes.len(),
            );
        }
    }

    pub(crate) fn set_preedit_bytes(&self, bytes: &[u8]) {
        unsafe {
            (self.functions.surface_preedit)(
                self.as_raw(),
                ghostty_surface_preedit_ptr(bytes),
                bytes.len(),
            );
        }
    }

    pub(crate) fn mouse_captured(&self) -> bool {
        unsafe { (self.functions.surface_mouse_captured)(self.as_raw()) }
    }

    pub(crate) fn mouse_button(
        &self,
        action: ffi::ghostty_input_mouse_state_e,
        button: ffi::ghostty_input_mouse_button_e,
        mods: ffi::ghostty_input_mods_e,
    ) -> bool {
        unsafe { (self.functions.surface_mouse_button)(self.as_raw(), action, button, mods) }
    }

    pub(crate) fn mouse_pos(&self, x: f64, y: f64, mods: ffi::ghostty_input_mods_e) {
        unsafe {
            (self.functions.surface_mouse_pos)(self.as_raw(), x, y, mods);
        }
    }

    pub(crate) fn mouse_scroll(
        &self,
        x: f64,
        y: f64,
        scroll_mods: ffi::ghostty_input_scroll_mods_t,
    ) {
        unsafe {
            (self.functions.surface_mouse_scroll)(self.as_raw(), x, y, scroll_mods);
        }
    }

    pub(crate) fn mouse_pressure(&self, stage: u32, pressure: f64) {
        unsafe {
            (self.functions.surface_mouse_pressure)(self.as_raw(), stage, pressure);
        }
    }

    pub(crate) fn ime_point(&self) -> GhosttySurfaceImePoint {
        let mut x = 0.0;
        let mut y = 0.0;
        let mut width = 0.0;
        let mut height = 0.0;
        unsafe {
            (self.functions.surface_ime_point)(
                self.as_raw(),
                &mut x,
                &mut y,
                &mut width,
                &mut height,
            );
        }
        GhosttySurfaceImePoint {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn request_close(&mut self) -> bool {
        if self.close_requested {
            return false;
        }
        self.close_requested = true;
        unsafe {
            (self.functions.surface_request_close)(self.as_raw());
        }
        true
    }

    pub(crate) fn consume_confirmed_close_requested(&self) -> bool {
        self.close_token.consume_confirmed_close_requested()
    }

    pub(crate) fn consume_confirmation_needed_close_requested(&self) -> bool {
        self.close_token
            .consume_confirmation_needed_close_requested()
    }

    pub(crate) fn cancel_pending_close_request(&mut self) -> bool {
        if !self.close_requested || self.close_token.confirmed_close_pending() {
            return false;
        }
        self.close_token.clear_confirmation_needed_close_requested();
        self.close_requested = false;
        true
    }

    pub(crate) fn drain_runtime_clipboard_requests(
        &self,
        allow_standard_clipboard: bool,
        read_standard_text: impl FnMut() -> Option<String>,
        write_standard_text: impl FnMut(String),
    ) {
        self.close_token.drain_runtime_clipboard_operations(
            allow_standard_clipboard,
            read_standard_text,
            write_standard_text,
        );
    }

    pub(crate) fn drain_runtime_action_events(&self) -> Vec<GhosttyRuntimeActionEvent> {
        self.close_token
            .take_runtime_action_events()
            .into_iter()
            .collect()
    }

    fn as_raw(&self) -> ffi::ghostty_surface_t {
        self.surface.as_ptr()
    }

    fn set_content_scale(&mut self, scale_factor: GhosttySurfaceScaleFactor) {
        if self.latest_scale_factor == Some(scale_factor) {
            return;
        }
        unsafe {
            (self.functions.surface_set_content_scale)(
                self.as_raw(),
                scale_factor.get(),
                scale_factor.get(),
            );
        }
        self.latest_scale_factor = Some(scale_factor);
    }

    fn set_size_pixels(&mut self, pixel_size: GhosttySurfacePixelSize) {
        if self.latest_pixel_size == Some(pixel_size) {
            return;
        }
        unsafe {
            (self.functions.surface_set_size)(self.as_raw(), pixel_size.width, pixel_size.height);
        }
        self.latest_pixel_size = Some(pixel_size);
    }
}

impl<SlotId> Drop for GhosttySurfaceOwner<SlotId> {
    fn drop(&mut self) {
        self.close_token.deny_pending_runtime_clipboard_operations();
        unsafe {
            (self.functions.surface_free)(self.surface.as_ptr());
        }
        self.close_token.clear_surface();
    }
}

pub(crate) struct StartupGhosttySurfaceOwner {
    surface: NonNull<c_void>,
    startup_body_slot_id: AgentsTerminalStartupBodySlotId,
    runtime_session_id: AgentsTerminalRuntimeSessionId,
    functions: GhosttyKitFunctionTable,
    close_token: Box<GhosttySurfaceCloseToken>,
    latest_scale_factor: Option<GhosttySurfaceScaleFactor>,
    latest_pixel_size: Option<GhosttySurfacePixelSize>,
}

impl StartupGhosttySurfaceOwner {
    pub(crate) fn new(
        app: &GhosttyAppOwner,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        request: &GhosttySurfaceConfigRequest,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        let close_token = GhosttySurfaceCloseToken::boxed(app.functions);
        let (surface, functions) =
            create_ghostty_surface_from_request(app, request, close_token.as_userdata())?;
        close_token.set_surface(surface.as_ptr());
        Ok(Self {
            surface,
            startup_body_slot_id,
            runtime_session_id,
            functions,
            close_token,
            latest_scale_factor: None,
            latest_pixel_size: None,
        })
    }

    pub(crate) fn startup_body_slot_id(&self) -> AgentsTerminalStartupBodySlotId {
        self.startup_body_slot_id
    }

    pub(crate) fn runtime_session_id(&self) -> AgentsTerminalRuntimeSessionId {
        self.runtime_session_id
    }

    pub(crate) fn update_content_scale_and_size(
        &mut self,
        bounds: Bounds<Pixels>,
        scale_factor: f64,
    ) -> Result<(), GhosttySurfaceRuntimeError> {
        let scale_factor = GhosttySurfaceScaleFactor::new(scale_factor)?;
        let pixel_size = GhosttySurfacePixelSize::from_gpui_bounds(bounds, scale_factor.get())?;
        self.set_content_scale(scale_factor);
        self.set_size_pixels(pixel_size);
        Ok(())
    }

    pub(crate) fn metadata_snapshot(&self) -> GhosttySurfaceMetadataSnapshot {
        ghostty_surface_metadata_snapshot(self.functions, self.as_raw())
    }

    pub(crate) fn into_running_surface_owner(
        self,
        mount_slot_id: AgentsTerminalBodyMountSlotId,
    ) -> GhosttySurfaceOwner {
        /*
        CDXC:GPUITerminalStartupHandoff 2026-06-23-04:25:
        Promotion must transfer the exact startup Ghostty surface into the Running owner without calling `ghostty_surface_free`. `ManuallyDrop` keeps the surface alive while the new owner takes the same raw handle and runtime id; focus starts unset because startup owners never focus hidden hosts.

        CDXC:GPUITerminalGhosttyClose 2026-06-23-04:49:
        The surface userdata is the owner-held close token, so Ready handoff must move that token with the raw Ghostty surface. Replacing it would leave the embedded close callback pointing at stale process memory.

        CDXC:GPUITerminalClipboard 2026-06-27-04:10:
        The surface userdata carries the registered close/clipboard token, so Ready handoff must move that token with the Ghostty surface. Clipboard callbacks can then keep enqueueing owner-local operations for the promoted Running owner without recreating the process, using focus, or falling back to app-level runtime userdata.
        */
        let startup_owner = ManuallyDrop::new(self);
        let close_token = unsafe { ptr::read(&startup_owner.close_token) };
        GhosttySurfaceOwner {
            surface: startup_owner.surface,
            mount_slot_id,
            runtime_session_id: startup_owner.runtime_session_id,
            functions: startup_owner.functions,
            close_token,
            close_requested: false,
            latest_scale_factor: startup_owner.latest_scale_factor,
            latest_pixel_size: startup_owner.latest_pixel_size,
            latest_focus_state: None,
            latest_occlusion_state: true,
        }
    }

    fn as_raw(&self) -> ffi::ghostty_surface_t {
        self.surface.as_ptr()
    }

    fn set_content_scale(&mut self, scale_factor: GhosttySurfaceScaleFactor) {
        if self.latest_scale_factor == Some(scale_factor) {
            return;
        }
        unsafe {
            (self.functions.surface_set_content_scale)(
                self.as_raw(),
                scale_factor.get(),
                scale_factor.get(),
            );
        }
        self.latest_scale_factor = Some(scale_factor);
    }

    fn set_size_pixels(&mut self, pixel_size: GhosttySurfacePixelSize) {
        if self.latest_pixel_size == Some(pixel_size) {
            return;
        }
        unsafe {
            (self.functions.surface_set_size)(self.as_raw(), pixel_size.width, pixel_size.height);
        }
        self.latest_pixel_size = Some(pixel_size);
    }
}

impl Drop for StartupGhosttySurfaceOwner {
    fn drop(&mut self) {
        self.close_token.deny_pending_runtime_clipboard_operations();
        unsafe {
            (self.functions.surface_free)(self.as_raw());
        }
        self.close_token.clear_surface();
    }
}

fn create_ghostty_surface_from_request(
    app: &GhosttyAppOwner,
    request: &GhosttySurfaceConfigRequest,
    surface_userdata: *mut c_void,
) -> Result<(NonNull<c_void>, GhosttyKitFunctionTable), GhosttySurfaceRuntimeError> {
    let functions = app.functions;
    let config = unsafe { (functions.surface_config_new)() };
    let mut prepared_config = request.prepare_ffi_config(config);
    prepared_config.set_surface_userdata(surface_userdata);
    let surface = unsafe { (functions.surface_new)(app.as_raw(), prepared_config.as_ptr()) };
    let surface =
        NonNull::new(surface).ok_or(GhosttySurfaceRuntimeError::SurfaceCreateReturnedNull)?;
    Ok((surface, functions))
}
