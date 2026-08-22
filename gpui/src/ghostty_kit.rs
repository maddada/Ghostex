#![allow(dead_code, non_camel_case_types)]

use std::{
    ffi::{c_char, c_int, c_void},
    path::{Path, PathBuf},
};

pub const REPO_RELATIVE_GHOSTTYKIT_HEADER: &str =
    "../.dependencies/ghostty/macos/GhosttyKit.xcframework/macos-arm64_x86_64/Headers/ghostty.h";
pub const REPO_RELATIVE_GHOSTTYKIT_ARCHIVE: &str =
    "../.dependencies/ghostty/macos/GhosttyKit.xcframework/macos-arm64_x86_64/ghostty-internal.a";

/*
CDXC:GPUIGhosttyKitAdapter 2026-06-22-20:35:
Phase 2 GPUI terminal parity needs a compile-safe boundary to the real GhosttyKit/libghostty C ABI before any native terminal surface is mounted. This module may declare real exported types and functions plus non-logging path availability helpers, but it must not synthesize project/session/process state, terminal content, command text, fallback surfaces, logging, persistence, overlays, or hidden hit regions.

CDXC:GPUIGhosttyKitAdapter 2026-06-23-04:13:
Startup readiness can only observe real GhosttyKit surface metadata through exact ABI declarations for exited state, foreground-pid presence, and tty-name presence. The FFI layer exposes `ghostty_string_s` plus its free function but does not interpret, persist, log, or print raw tty names or process ids.

CDXC:GPUITerminalInputABI 2026-06-23-05:53:
Slice 135 adds only the existing embedded Ghostty input ABI needed by future real terminal event forwarding. The Rust declarations mirror the generated GhosttyKit header and must not invent close-confirm symbols, translate GPUI events, log input payloads, persist terminal text, or route native hit testing.

CDXC:GPUITerminalMouseForwarding 2026-06-23-08:32:
Mouse input slices must use named Ghostty header values for action and button identity so GPUI callers do not encode libghostty ABI numbers as magic constants while forwarding only sanitized primitive event state.

CDXC:GPUITerminalClipboard 2026-06-23-10:33:
Clipboard blocker parity needs named Ghostty header values for standard/selection clipboards and paste/OSC 52 requests so runtime callbacks can stay explicitly disabled without magic numbers or touching raw clipboard content.

CDXC:GPUITerminalClipboard 2026-06-23-12:11:
Runtime clipboard reads are asynchronous in embedded Ghostty and completion requires a concrete surface. Bind the completion export for a future surface-scoped handoff, but keep callbacks disabled until GPUI can prove the requesting surface before any app-thread clipboard access.

CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
Slice 237 uses the real embedded GhosttyKit `ghostty_surface_needs_confirm_quit` ABI as the close-confirm evidence source. GPUI must bind the boolean query and `ghostty_surface_request_close` directly, and must not invent a separate confirm-close symbol, persist process data, log terminal content, or expose command/path/runtime details.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhosttyKitPaths {
    pub header: PathBuf,
    pub archive: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GhosttyKitAvailability {
    pub header_exists: bool,
    pub archive_exists: bool,
}

impl GhosttyKitAvailability {
    pub fn is_complete(self) -> bool {
        self.header_exists && self.archive_exists
    }
}

pub fn ghosttykit_paths(manifest_dir: impl AsRef<Path>) -> GhosttyKitPaths {
    let manifest_dir = manifest_dir.as_ref();

    GhosttyKitPaths {
        header: manifest_dir.join(REPO_RELATIVE_GHOSTTYKIT_HEADER),
        archive: manifest_dir.join(REPO_RELATIVE_GHOSTTYKIT_ARCHIVE),
    }
}

pub fn ghosttykit_availability(manifest_dir: impl AsRef<Path>) -> GhosttyKitAvailability {
    let paths = ghosttykit_paths(manifest_dir);

    GhosttyKitAvailability {
        header_exists: paths.header.is_file(),
        archive_exists: paths.archive.is_file(),
    }
}

pub mod ffi {
    use super::{c_char, c_int, c_void};

    pub const GHOSTTY_SUCCESS: c_int = 0;

    pub type ghostty_app_t = *mut c_void;
    pub type ghostty_config_t = *mut c_void;
    pub type ghostty_surface_t = *mut c_void;

    pub type ghostty_platform_e = c_int;
    pub const GHOSTTY_PLATFORM_INVALID: ghostty_platform_e = 0;
    pub const GHOSTTY_PLATFORM_MACOS: ghostty_platform_e = 1;
    pub const GHOSTTY_PLATFORM_IOS: ghostty_platform_e = 2;

    pub type ghostty_surface_context_e = c_int;
    pub const GHOSTTY_SURFACE_CONTEXT_WINDOW: ghostty_surface_context_e = 0;
    pub const GHOSTTY_SURFACE_CONTEXT_TAB: ghostty_surface_context_e = 1;
    pub const GHOSTTY_SURFACE_CONTEXT_SPLIT: ghostty_surface_context_e = 2;

    pub type ghostty_clipboard_e = c_int;
    pub const GHOSTTY_CLIPBOARD_STANDARD: ghostty_clipboard_e = 0;
    pub const GHOSTTY_CLIPBOARD_SELECTION: ghostty_clipboard_e = 1;

    pub type ghostty_clipboard_request_e = c_int;
    pub const GHOSTTY_CLIPBOARD_REQUEST_PASTE: ghostty_clipboard_request_e = 0;
    pub const GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ: ghostty_clipboard_request_e = 1;
    pub const GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE: ghostty_clipboard_request_e = 2;

    pub type ghostty_target_tag_e = c_int;
    pub type ghostty_action_tag_e = c_int;

    // Values mirror the ghostty_target_tag_e / ghostty_action_tag_e enum order in
    // the pinned GhosttyKit.xcframework's ghostty.h. Re-verify these indices
    // whenever the vendored Ghostty header changes.
    pub const GHOSTTY_TARGET_SURFACE: ghostty_target_tag_e = 1;
    pub const GHOSTTY_ACTION_DESKTOP_NOTIFICATION: ghostty_action_tag_e = 32;
    pub const GHOSTTY_ACTION_SET_TITLE: ghostty_action_tag_e = 33;
    pub const GHOSTTY_ACTION_PWD: ghostty_action_tag_e = 37;
    pub const GHOSTTY_ACTION_MOUSE_OVER_LINK: ghostty_action_tag_e = 40;
    pub const GHOSTTY_ACTION_RING_BELL: ghostty_action_tag_e = 52;
    pub const GHOSTTY_ACTION_OPEN_URL: ghostty_action_tag_e = 57;
    pub const GHOSTTY_ACTION_START_SEARCH: ghostty_action_tag_e = 62;
    pub const GHOSTTY_ACTION_END_SEARCH: ghostty_action_tag_e = 63;
    pub const GHOSTTY_ACTION_SEARCH_TOTAL: ghostty_action_tag_e = 64;
    pub const GHOSTTY_ACTION_SEARCH_SELECTED: ghostty_action_tag_e = 65;

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct ghostty_clipboard_content_s {
        pub mime: *const c_char,
        pub data: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct ghostty_env_var_s {
        pub key: *const c_char,
        pub value: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ghostty_string_s {
        pub ptr: *const c_char,
        pub len: usize,
        pub sentinel: bool,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct ghostty_platform_macos_s {
        pub nsview: *mut c_void,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct ghostty_platform_ios_s {
        pub uiview: *mut c_void,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union ghostty_platform_u {
        pub macos: ghostty_platform_macos_s,
        pub ios: ghostty_platform_ios_s,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union ghostty_target_u {
        pub surface: ghostty_surface_t,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_target_s {
        pub tag: ghostty_target_tag_e,
        pub target: ghostty_target_u,
    }

    pub type ghostty_input_action_e = c_int;
    pub type ghostty_input_mods_e = c_int;
    pub type ghostty_input_key_e = c_int;
    pub type ghostty_binding_flags_e = c_int;
    pub type ghostty_input_mouse_state_e = c_int;
    pub type ghostty_input_mouse_button_e = c_int;
    pub type ghostty_input_scroll_mods_t = c_int;
    pub type ghostty_input_trigger_tag_e = c_int;
    pub const GHOSTTY_MOUSE_RELEASE: ghostty_input_mouse_state_e = 0;
    pub const GHOSTTY_MOUSE_PRESS: ghostty_input_mouse_state_e = 1;
    pub const GHOSTTY_MOUSE_UNKNOWN: ghostty_input_mouse_button_e = 0;
    pub const GHOSTTY_MOUSE_LEFT: ghostty_input_mouse_button_e = 1;
    pub const GHOSTTY_MOUSE_RIGHT: ghostty_input_mouse_button_e = 2;
    pub const GHOSTTY_MOUSE_MIDDLE: ghostty_input_mouse_button_e = 3;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_input_key_s {
        pub action: ghostty_input_action_e,
        pub mods: ghostty_input_mods_e,
        pub consumed_mods: ghostty_input_mods_e,
        pub keycode: u32,
        pub text: *const c_char,
        pub unshifted_codepoint: u32,
        pub composing: bool,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union ghostty_input_trigger_key_u {
        pub physical: ghostty_input_key_e,
        pub unicode: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_input_trigger_s {
        pub tag: ghostty_input_trigger_tag_e,
        pub key: ghostty_input_trigger_key_u,
        pub mods: ghostty_input_mods_e,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_resize_split_s {
        pub amount: u16,
        pub direction: c_int,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_move_tab_s {
        pub amount: isize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_desktop_notification_s {
        pub title: *const c_char,
        pub body: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_set_title_s {
        pub title: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_pwd_s {
        pub pwd: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_mouse_over_link_s {
        pub url: *const c_char,
        pub len: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_size_limit_s {
        pub min_width: u32,
        pub min_height: u32,
        pub max_width: u32,
        pub max_height: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_initial_size_s {
        pub width: u32,
        pub height: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_cell_size_s {
        pub width: u32,
        pub height: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_key_sequence_s {
        pub active: bool,
        pub trigger: ghostty_input_trigger_s,
    }

    pub type ghostty_action_key_table_tag_e = c_int;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_key_table_activate_s {
        pub name: *const c_char,
        pub len: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union ghostty_action_key_table_u {
        pub activate: ghostty_action_key_table_activate_s,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_key_table_s {
        pub tag: ghostty_action_key_table_tag_e,
        pub value: ghostty_action_key_table_u,
    }

    pub type ghostty_action_color_kind_e = c_int;

    // config.Color (ghostty_config_color_s in ghostty.h); written by value
    // through `ghostty_config_get` for color-typed configuration keys.
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct ghostty_config_color_s {
        pub r: u8,
        pub g: u8,
        pub b: u8,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_color_change_s {
        pub kind: ghostty_action_color_kind_e,
        pub r: u8,
        pub g: u8,
        pub b: u8,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_config_change_s {
        pub config: ghostty_config_t,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_reload_config_s {
        pub soft: bool,
    }

    pub type ghostty_action_open_url_kind_e = c_int;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_open_url_s {
        pub kind: ghostty_action_open_url_kind_e,
        pub url: *const c_char,
        pub len: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_surface_message_childexited_s {
        pub exit_code: u32,
        pub timetime_ms: u64,
    }

    pub type ghostty_action_progress_report_state_e = c_int;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_progress_report_s {
        pub state: ghostty_action_progress_report_state_e,
        pub progress: i8,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_command_finished_s {
        pub exit_code: i16,
        pub duration: u64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_start_search_s {
        pub needle: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_search_total_s {
        pub total: isize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_search_selected_s {
        pub selected: isize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_scrollbar_s {
        pub total: u64,
        pub offset: u64,
        pub len: u64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union ghostty_action_u {
        pub new_split: c_int,
        pub toggle_fullscreen: c_int,
        pub move_tab: ghostty_action_move_tab_s,
        pub goto_tab: c_int,
        pub goto_split: c_int,
        pub goto_window: c_int,
        pub resize_split: ghostty_action_resize_split_s,
        pub size_limit: ghostty_action_size_limit_s,
        pub initial_size: ghostty_action_initial_size_s,
        pub cell_size: ghostty_action_cell_size_s,
        pub scrollbar: ghostty_action_scrollbar_s,
        pub inspector: c_int,
        pub desktop_notification: ghostty_action_desktop_notification_s,
        pub set_title: ghostty_action_set_title_s,
        pub set_tab_title: ghostty_action_set_title_s,
        pub prompt_title: c_int,
        pub pwd: ghostty_action_pwd_s,
        pub mouse_shape: c_int,
        pub mouse_visibility: c_int,
        pub mouse_over_link: ghostty_action_mouse_over_link_s,
        pub renderer_health: c_int,
        pub quit_timer: c_int,
        pub float_window: c_int,
        pub secure_input: c_int,
        pub key_sequence: ghostty_action_key_sequence_s,
        pub key_table: ghostty_action_key_table_s,
        pub color_change: ghostty_action_color_change_s,
        pub reload_config: ghostty_action_reload_config_s,
        pub config_change: ghostty_action_config_change_s,
        pub open_url: ghostty_action_open_url_s,
        pub close_tab_mode: c_int,
        pub child_exited: ghostty_surface_message_childexited_s,
        pub progress_report: ghostty_action_progress_report_s,
        pub command_finished: ghostty_action_command_finished_s,
        pub start_search: ghostty_action_start_search_s,
        pub search_total: ghostty_action_search_total_s,
        pub search_selected: ghostty_action_search_selected_s,
        pub readonly: c_int,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_action_s {
        pub tag: ghostty_action_tag_e,
        pub action: ghostty_action_u,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_surface_config_s {
        pub platform_tag: ghostty_platform_e,
        pub platform: ghostty_platform_u,
        pub userdata: *mut c_void,
        pub scale_factor: f64,
        pub font_size: f32,
        pub working_directory: *const c_char,
        pub command: *const c_char,
        pub env_vars: *mut ghostty_env_var_s,
        pub env_var_count: usize,
        pub initial_input: *const c_char,
        pub wait_after_command: bool,
        pub context: ghostty_surface_context_e,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ghostty_surface_size_s {
        pub columns: u16,
        pub rows: u16,
        pub width_px: u32,
        pub height_px: u32,
        pub cell_width_px: u32,
        pub cell_height_px: u32,
    }

    pub type ghostty_runtime_wakeup_cb = Option<unsafe extern "C" fn(*mut c_void)>;
    pub type ghostty_runtime_read_clipboard_cb =
        Option<unsafe extern "C" fn(*mut c_void, ghostty_clipboard_e, *mut c_void) -> bool>;
    pub type ghostty_runtime_confirm_read_clipboard_cb = Option<
        unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void, ghostty_clipboard_request_e),
    >;
    pub type ghostty_runtime_write_clipboard_cb = Option<
        unsafe extern "C" fn(
            *mut c_void,
            ghostty_clipboard_e,
            *const ghostty_clipboard_content_s,
            usize,
            bool,
        ),
    >;
    pub type ghostty_runtime_close_surface_cb = Option<unsafe extern "C" fn(*mut c_void, bool)>;
    pub type ghostty_runtime_action_cb =
        Option<unsafe extern "C" fn(ghostty_app_t, ghostty_target_s, ghostty_action_s) -> bool>;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ghostty_runtime_config_s {
        pub userdata: *mut c_void,
        pub supports_selection_clipboard: bool,
        pub wakeup_cb: ghostty_runtime_wakeup_cb,
        pub action_cb: ghostty_runtime_action_cb,
        pub read_clipboard_cb: ghostty_runtime_read_clipboard_cb,
        pub confirm_read_clipboard_cb: ghostty_runtime_confirm_read_clipboard_cb,
        pub write_clipboard_cb: ghostty_runtime_write_clipboard_cb,
        pub close_surface_cb: ghostty_runtime_close_surface_cb,
    }

    /*
    CDXC:GPUIGhosttyKitAdapter 2026-06-22-20:35:
    These declarations bind GPUI to the real exported GhosttyKit/libghostty symbols only. Later runtime slices may call them after native mount-slot ownership is ready; this boundary itself must stay inert and must not add speculative link flags, fake surfaces, terminal processes, command text, fallback mounts, or persistent diagnostics.

    CDXC:GPUIGhosttyKitAdapter 2026-06-23-04:49:
    Running Agents close parity needs the exported `ghostty_surface_request_close` ABI so GPUI can ask Ghostty to run its normal close flow before shell-tab removal. Keep this as a direct symbol binding only; close state belongs to the process-memory surface owner, not to persistent logs or shell state.

    CDXC:GPUITerminalInputABI 2026-06-23-05:53:
    Input parity starts as ABI scaffolding only. Bind the upstream embedded Ghostty surface input exports exactly, leave GPUI key/mouse/IME translation for a later slice, and avoid direct action-string bridges unless a future requirement can keep action payloads out of diagnostics.

    CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
    The close-confirm contract is source-side evidence for the real `ghostty_surface_needs_confirm_quit` query plus direct model removal after user consent. This boundary exposes only a redacted boolean and must not add a fake confirm ABI, command/process logging, shell-state fields, or fallback close behavior.

    CDXC:GPUILinuxX11Backend 2026-07-05:
    The exported GhosttyKit/libghostty symbols exist only in the macOS static
    archive (gpui/build.rs links GhosttyKit on macOS alone; Windows/Linux get
    libghostty-vt only, by design). The ABI types above stay cross-platform so
    shared code can name them, but the symbol bindings are macOS-only —
    non-macOS code paths that would call them must be cfg-gated at the caller,
    not satisfied with stub definitions.
    */
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        pub fn ghostty_init(argc: usize, argv: *mut *mut c_char) -> c_int;
        pub fn ghostty_string_free(value: ghostty_string_s);

        pub fn ghostty_config_new() -> ghostty_config_t;
        pub fn ghostty_config_free(config: ghostty_config_t);
        pub fn ghostty_config_load_default_files(config: ghostty_config_t);
        pub fn ghostty_config_load_file(config: ghostty_config_t, path: *const c_char);
        pub fn ghostty_config_load_string(
            config: ghostty_config_t,
            source: *const c_char,
            len: usize,
        );
        pub fn ghostty_config_load_recursive_files(config: ghostty_config_t);
        pub fn ghostty_config_finalize(config: ghostty_config_t);
        pub fn ghostty_config_get(
            config: ghostty_config_t,
            value: *mut c_void,
            key: *const c_char,
            len: usize,
        ) -> bool;
        pub fn ghostty_config_to_string(config: ghostty_config_t) -> ghostty_string_s;

        pub fn ghostty_app_new(
            runtime_config: *const ghostty_runtime_config_s,
            config: ghostty_config_t,
        ) -> ghostty_app_t;
        pub fn ghostty_app_free(app: ghostty_app_t);
        pub fn ghostty_app_tick(app: ghostty_app_t);
        pub fn ghostty_app_set_focus(app: ghostty_app_t, focused: bool);

        pub fn ghostty_surface_config_new() -> ghostty_surface_config_s;
        pub fn ghostty_surface_new(
            app: ghostty_app_t,
            config: *const ghostty_surface_config_s,
        ) -> ghostty_surface_t;
        pub fn ghostty_surface_free(surface: ghostty_surface_t);
        pub fn ghostty_surface_set_content_scale(surface: ghostty_surface_t, x: f64, y: f64);
        pub fn ghostty_surface_set_size(surface: ghostty_surface_t, width: u32, height: u32);
        pub fn ghostty_surface_set_focus(surface: ghostty_surface_t, focused: bool);
        pub fn ghostty_surface_size(surface: ghostty_surface_t) -> ghostty_surface_size_s;
        pub fn ghostty_surface_needs_confirm_quit(surface: ghostty_surface_t) -> bool;
        pub fn ghostty_surface_binding_action(
            surface: ghostty_surface_t,
            action: *const c_char,
            len: usize,
        ) -> bool;
        pub fn ghostty_surface_process_exited(surface: ghostty_surface_t) -> bool;
        pub fn ghostty_surface_foreground_pid(surface: ghostty_surface_t) -> u64;
        pub fn ghostty_surface_tty_name(surface: ghostty_surface_t) -> ghostty_string_s;
        pub fn ghostty_surface_key_translation_mods(
            surface: ghostty_surface_t,
            mods: ghostty_input_mods_e,
        ) -> ghostty_input_mods_e;
        pub fn ghostty_surface_key(surface: ghostty_surface_t, event: ghostty_input_key_s) -> bool;
        pub fn ghostty_surface_key_is_binding(
            surface: ghostty_surface_t,
            event: ghostty_input_key_s,
            flags: *mut ghostty_binding_flags_e,
        ) -> bool;
        pub fn ghostty_surface_text(surface: ghostty_surface_t, ptr: *const c_char, len: usize);
        pub fn ghostty_surface_preedit(surface: ghostty_surface_t, ptr: *const c_char, len: usize);
        pub fn ghostty_surface_mouse_captured(surface: ghostty_surface_t) -> bool;
        pub fn ghostty_surface_mouse_button(
            surface: ghostty_surface_t,
            action: ghostty_input_mouse_state_e,
            button: ghostty_input_mouse_button_e,
            mods: ghostty_input_mods_e,
        ) -> bool;
        pub fn ghostty_surface_mouse_pos(
            surface: ghostty_surface_t,
            x: f64,
            y: f64,
            mods: ghostty_input_mods_e,
        );
        pub fn ghostty_surface_mouse_scroll(
            surface: ghostty_surface_t,
            x: f64,
            y: f64,
            scroll_mods: ghostty_input_scroll_mods_t,
        );
        pub fn ghostty_surface_mouse_pressure(
            surface: ghostty_surface_t,
            stage: u32,
            pressure: f64,
        );
        pub fn ghostty_surface_ime_point(
            surface: ghostty_surface_t,
            x: *mut f64,
            y: *mut f64,
            width: *mut f64,
            height: *mut f64,
        );
        pub fn ghostty_surface_request_close(surface: ghostty_surface_t);
        pub fn ghostty_surface_complete_clipboard_request(
            surface: ghostty_surface_t,
            data: *const c_char,
            state: *mut c_void,
            confirmed: bool,
        );
    }
}
