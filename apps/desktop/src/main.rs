#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![recursion_limit = "256"]

/*
CDXC:Build 2026-06-28-17:09:
GPUI still has schema-sized privacy-boundary serde_json::json! payloads outside the removed project-workarea proof chain. Keep the crate recursion limit high enough for those explicit payloads while runtime behavior is owned by direct gates.
*/
mod app;
mod app_icon;
#[cfg(any(target_os = "linux", test))]
mod appimage_signature;
mod assets;
mod browser_history;
mod cef;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod cef_component_window;
#[cfg(any(target_os = "linux", test))]
mod cef_library_version;
mod component_store;
mod fold_motion;
mod ghostty_kit;
mod ghostty_vt;
mod hotkey_label;
#[cfg(target_os = "linux")]
mod linux_hyprland;
#[cfg(any(target_os = "linux", test))]
mod linux_updater;
#[cfg(target_os = "linux")]
mod linux_x11_scale;
mod navigation_history;
mod notification_feed;
mod plugins_modal;
mod profiling;
mod shared_settings;
mod support_logs;
mod terminal_chat_claim;
mod terminal_element;
mod terminal_environment;
mod terminal_ghostty_surface;
mod terminal_gpui_engine;
mod terminal_model;
mod terminal_osc_title;
mod terminal_scrollbar_reveal;
#[cfg(target_os = "macos")]
mod terminal_shaders;
mod terminal_wheel;
mod ui_fonts;
#[cfg(target_os = "windows")]
mod windows_cli_link;
#[cfg(target_os = "windows")]
mod windows_single_instance;
mod windows_terminal_backend;
#[cfg(target_os = "windows")]
mod windows_updater;

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.
use std::cell::RefCell;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::BCRYPT_USE_SYSTEM_PREFERRED_RNG;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::BCryptGenRandom;

use anyhow::Result;
use cef::CefBrowser;
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::Action;
use gpui::AnyElement;
use gpui::App;
use gpui::AppContext as _;
use gpui::Bounds;
use gpui::ClipboardEntry;
use gpui::ClipboardItem;
use gpui::ContentMask;
use gpui::DismissEvent;
use gpui::Element;
use gpui::ElementId;
use gpui::Entity;
use gpui::FocusHandle;
use gpui::Focusable as _;
use gpui::FontWeight;
use gpui::GlobalElementId;
use gpui::Hitbox;
use gpui::Hsla;
use gpui::Image;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyBinding;
use gpui::KeyDownEvent;
use gpui::Keystroke;
use gpui::LayoutId;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Point;
use gpui::Render;
use gpui::ScrollDelta;
use gpui::ScrollHandle;
use gpui::Size;
use gpui::StatefulInteractiveElement as _;
use gpui::Style;
use gpui::Styled as _;
use gpui::Window;
use gpui::WindowBounds;
use gpui::WindowOptions;
use gpui::canvas;
use gpui::div;
use gpui::point;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui::rgb;
use gpui::rgba;
use gpui::size;
use gpui::svg;
use gpui_component::Root;
use gpui_component::h_flex;
use gpui_component::menu::PopupMenu;
use gpui_component::scroll::Scrollbar;
use gpui_component::tooltip::Tooltip;
use gpui_component::v_flex;
use raw_window_handle::HasWindowHandle as _;
use raw_window_handle::RawWindowHandle;

#[cfg(any(target_os = "windows", target_os = "linux"))]
use gpui::WindowControlArea;

// C1 wave-1 extraction: brings the stateless helper fns moved into
// app/helpers/* back into scope for all of main.rs's existing call sites.
use crate::app::helpers::*;
// C1 wave-2 extraction: brings the window entities (app/window/*) and the
// CefSurface/CefElement gpui Element impls (app/element/*) back into scope.
use crate::app::element::*;
use crate::app::window::*;
// C1 wave-3 extraction: brings Region A (actions, hotkeys, FFI callback
// bridge, consts, and sub-model/value types) back into scope. These are
// `pub(crate) use` (not plain `use`) because a handful of items
// (TerminalSurfaceMountSlotKey, AgentsTerminalBodyMountSlotId,
// AgentsTerminalRuntimeSessionId) are referenced by sibling modules via
// `crate::ItemName`, which requires a crate-root re-export, not just a
// private import for main.rs's own call sites.
pub(crate) use crate::app::actions::*;
pub(crate) use crate::app::consts::*;
pub(crate) use crate::app::extensions::*;
pub(crate) use crate::app::ffi::*;
pub(crate) use crate::app::hotkeys::*;
pub(crate) use crate::app::model::*;
// C1 wave-4 extraction: the god object itself now lives in app/core.rs; the
// crate-root re-export keeps `crate::GhostexGpuiApp` resolving for main(),
// the sibling modules that hold `Entity<GhostexGpuiApp>`, and the FFI bridge.
pub(crate) use crate::app::core::*;
// CDXC:Extensions 2026-09-18 SEE-ALSO:
// The per-view scope keys are read from workarea.rs and the titlebar modules.
pub(crate) use crate::app::view_scopes::*;

fn main() {
    support_logs::note_launch();
    #[cfg(windows)]
    if gpui_run_windows_remote_ssh_askpass() {
        return;
    }
    #[cfg(target_os = "windows")]
    windows_updater::run_startup_hooks();

    // Strip inherited color/session blockers before GPUI, gxserver,
    // GhosttyKit, or the PTY engine can snapshot the process environment.
    //
    // SAFETY: called before GPUI starts background threads or framework-owned
    // environment readers.
    unsafe {
        terminal_environment::remove_color_disabling_from_current_process();
        terminal_environment::remove_session_identity_from_current_process();
        /*
        CDXC:OsIntegration 2026-07-24:
        LaunchServices gives packaged macOS apps a system-only PATH. Normalize
        it once at the process boundary so CLI status, bundled-skill installs,
        Cua Driver checks, and other fixed local-tool actions see the same
        standard user locations as gxserver. This is startup environment
        ownership, not a per-action fallback.
        2026-10-31: this asks the login shell for its PATH and waits at most
        `STARTUP_PROBE_WAIT`; the probe's thread only uses std's environment
        calls, which are locked against this `set_var`.
        */
        #[cfg(target_os = "macos")]
        {
            let current_path = env::var("PATH").ok();
            env::set_var("PATH", gpui_startup_user_tool_path(current_path.as_deref()));
        }
    }
    // A second launch brings the running Ghostex forward and exits (see
    // windows_single_instance.rs for why this no longer waits for CEF).
    #[cfg(target_os = "windows")]
    if windows_single_instance::hand_off_to_running_instance() {
        return;
    }
    // Install missing `ghostex`/`gx` PATH wrappers and refresh stale
    // Ghostex-owned ones off the main thread so filesystem probing cannot
    // delay first paint, and only after the PATH normalization above so the
    // scan sees the user's standard tool directories.
    profiling::start();
    thread::spawn(gpui_auto_install_ghostex_cli_wrappers);
    // Tunnels a crashed or force-quit Ghostex left running (app/helpers/remote/tunnel_registry.rs).
    thread::spawn(gpui_reap_orphaned_remote_tunnels);
    // Only finds the CEF runtime on disk; nothing starts or downloads it here
    // (CDXC:CefRuntime 2026-09-28 in app/helpers/web_runtime.rs).
    // A runtime the user uninstalled (or, on Windows, reinstalled) while it ran is removed first,
    // before anything can load it (app/helpers/web_runtime_pending.rs).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let web_runtime_usable =
        app::helpers::web_runtime_pending::apply_pending_web_runtime_change_at_launch();
    #[cfg(target_os = "linux")]
    let web_runtime_usable = true;
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    if web_runtime_usable && cef_component_window::configure_cef_framework_path_for_process() {
        app::helpers::web_runtime::set_web_runtime_state(
            app::helpers::web_runtime::WebRuntimeState::Installed,
        );
    }
    // The Linux app is X11-only for v1 (CEF child-window embedding requires
    // an X11 host window), so backend selection must happen before framework
    // initialization or background work can read the environment.
    #[cfg(target_os = "linux")]
    force_gpui_x11_backend_for_windowed_cef();
    // Omarchy's Super+C / Super+V reach Ghostex as Ctrl+Insert / Shift+Insert (linux_hyprland.rs).
    #[cfg(target_os = "linux")]
    linux_hyprland::start_terminal_tag();
    /*
    CDXC:PlatformSupport 2026-07-25:
    Windowed CEF children are normal child HWNDs. GPUI's DirectComposition
    top-level uses WS_EX_NOREDIRECTIONBITMAP, so DWM cannot composite those
    children and browser/sidebar surfaces remain black. Force GPUI's normal
    redirection-surface path before framework initialization so CEF child
    windows and terminal content share one correctly composited hierarchy.
    */
    #[cfg(target_os = "windows")]
    unsafe {
        std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1");
    }
    // Crash reports must capture panics from the very start of the process
    // (GPUI previously lost panics to stderr; macOS counterpart:
    // NativeCrashDiagnostics).
    support_logs::install_panic_hook();
    support_logs::install_uncaught_exception_handler();
    support_logs::install_log_records_sink();
    app::gx_store::initialize_client_storage_at_start();
    // Before anything reads or writes terminal settings, so the first config Ghostex
    // terminals load is the copied one.
    shared_settings::ghostty_config_migration::migrate_terminal_config_from_ghostty_once();
    cef::prepare_application();
    #[cfg(target_os = "macos")]
    reconcile_gpui_managed_ghostty_config();
    // Workspace chrome background follows the `background` color of
    // Ghostex's terminal config (macOS defaultWorkspaceBackgroundColor parity). Read once
    // before the window opens so first paint already uses the real color.
    initialize_workspace_background_color_from_ghostty_config();
    refresh_gpui_visual_settings(&shared_settings::shared_sidebar_settings_snapshot());
    app::view_retention::apply_view_retention_default(
        &shared_settings::shared_sidebar_settings_snapshot(),
    );

    let application = gpui_platform::application().with_assets(assets::GhostexAssets);
    // OS-integration URL/file opens (ghostex:// + Finder Open With) hook the
    // platform's application:openURLs: delegate before the run loop starts so
    // launch-time opens are buffered until the app entity registers.
    #[cfg(target_os = "macos")]
    application.on_open_urls(queue_gpui_os_integration_urls);
    application.run(move |cx| {
        #[cfg(target_os = "windows")]
        cef::register_windows_shutdown(cx);
        gpui_component::init(cx);
        crate::app::native_docs::grammars::register();
        // CDXC:Clipboard 2026-10-28 WHY: the CUA clipboard keys work in every text field on
        // Windows and Linux as they do in the terminal (Ctrl+Insert / Shift+Insert there).
        // Omarchy's Super+C / Super+V send exactly these to windows it tags as terminals, the
        // only way its universal copy reaches Ghostex without sending Ctrl+C to a running command.
        #[cfg(not(target_os = "macos"))]
        cx.bind_keys([KeyBinding::new(
            "ctrl-insert",
            gpui_component::input::Copy,
            Some("Input"),
        )]);
        cx.bind_keys([KeyBinding::new(
            "shift-insert",
            gpui_component::input::Paste,
            Some("Input"),
        )]);
        ui_fonts::register(cx);
        crate::app::window::frosted_host::register_frosted_tooltip_root_plugin(cx);
        crate::app::window::modal_popover_host::install_frosted_modal_popover_host();
        apply_gpui_component_theme(cx);
        #[cfg(target_os = "macos")]
        {
            let source_id = app_icon::source_id_from_settings(
                shared_settings::shared_sidebar_settings_snapshot().object(),
            );
            let _ = app_icon::apply_persisted_source_id(&source_id);
        }
        // The GPUI terminal engine draws with the vendored JetBrains Mono
        // Nerd Font faces; register them before any window renders.
        terminal_gpui_engine::register_gpui_terminal_engine_fonts(cx);
        // Native app menu bar (macOS installMainMenu parity); menu actions
        // dispatch through the focused window's normal action chain.
        cx.set_menus(ghostex_gpui_main_menus_for_source_focus(false));
        /*
        CDXC:Hotkeys 2026-08-03:
        Do not bind Cmd+A in CEF_KEY_CONTEXT. Source must receive the original
        trusted chord so Monaco can run its editor-owned Select All command;
        Browser and other CEF surfaces keep their native AppKit selectAll:
        bridge in GpuiCefAppKitHooks.m.
        */
        let shell_key_bindings = vec![
            KeyBinding::new("f12", OpenCommandPane, None),
            KeyBinding::new("f5", ReloadFocusedBrowser, Some(BROWSER_KEY_CONTEXT)),
            KeyBinding::new(
                "escape",
                TitlebarDropdownCancel,
                Some(TITLEBAR_DROPDOWN_KEY_CONTEXT),
            ),
            KeyBinding::new("escape", TitlebarDropdownCancel, Some(CEF_KEY_CONTEXT)),
            KeyBinding::new(
                "tab",
                gpui::NoAction {},
                Some(terminal_element::TERMINAL_KEY_CONTEXT),
            ),
            KeyBinding::new(
                "shift-tab",
                gpui::NoAction {},
                Some(terminal_element::TERMINAL_KEY_CONTEXT),
            ),
        ];
        #[cfg(target_os = "macos")]
        let shell_key_bindings = {
            let mut bindings = shell_key_bindings;
            bindings.extend([
                KeyBinding::new("cmd-q", QuitGhostexGpui, None),
                KeyBinding::new("cmd-h", HideGhostexGpui, None),
                KeyBinding::new("alt-cmd-h", HideGhostexGpuiOthers, None),
                KeyBinding::new("cmd-m", MinimizeGhostexGpuiWindow, None),
                KeyBinding::new("cmd-`", CycleGhostexGpuiWindows, None),
            ]);
            bindings
        };
        cx.bind_keys(shell_key_bindings);
        // Paste, find, zoom, close and the other app-action keys bind at the chord Settings gives
        // them (app/shell_hotkeys.rs, which also keeps Ctrl+Shift+V terminal paste), then the
        // user's configured hotkey table binds after them so configured chords win conflicts.
        // Ids dispatch through the shared runGhostexHotkeyAction route regardless of which
        // surface has focus.
        crate::app::shell_hotkeys::gpui_bind_hotkeys_from_settings(cx);
        gpui_prewarm_ghostex_editor_daemon();
        crate::app::helpers::note_main_window_background(window_glass_background_appearance());
        cx.on_action(|_: &NewGhostexGpuiWindow, cx| {
            // A key press dispatches while its window is mid-update; the new window reads that
            // window's frame and project, so it opens on the next turn.
            cx.defer(crate::app::workspace_windows::open_new_workspace_window);
        });
        cx.on_action(|action: &ActivateGhostexGpuiWindow, cx| {
            let number = action.number;
            cx.defer(move |cx| {
                crate::app::workspace_windows::activate_workspace_window(number, cx)
            });
        });
        cx.on_action(|_: &CycleGhostexGpuiWindows, cx| {
            cx.defer(crate::app::workspace_windows::activate_next_workspace_window);
        });
        // Window frame persistence (macOS persistMainWindowChrome parity): every
        // window open at the last quit reopens at its saved frame with the
        // multi-monitor rules, else the historical centered default.
        crate::app::workspace_windows::open_saved_workspace_windows(cx);
        /*
        CDXC:PlatformSupport 2026-10-01 WHY:
        The workspace windows own application lifetime. App-modal, toast,
        and titlebar child windows can still be registered when the user closes
        the last workspace window, so waiting for `cx.windows()` to become empty
        leaves a headless Ghostex process holding CEF's persistent-profile
        singleton. A subsequent Ghostex.exe then reaches `cef_initialize` while
        that stale owner is alive and exits with "CEF initialization returned
        false". Quit when the last workspace window closes (File > New Window
        can open more than one, app/workspace_windows/); keep the
        empty-window arm for defensive parity if a platform closes every child
        before this observer. Supersedes the 2026-08-02 rule that quit on the
        main window's close.
        */
        cx.on_window_closed(move |cx, window_id| {
            #[cfg(target_os = "linux")]
            cef::detach_native_views_of_closing_window(window_id);
            let closed = crate::app::workspace_windows::workspace_window_closed(window_id, cx);
            if matches!(
                closed,
                crate::app::workspace_windows::WorkspaceWindowClosed::Last
            ) || cx.windows().is_empty()
            {
                GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
                cx.quit();
            }
        })
        .detach();
    });
    cef::shutdown();
}

#[cfg(target_os = "linux")]
static LINUX_INHERITED_WAYLAND_DISPLAY: std::sync::OnceLock<Option<String>> =
    std::sync::OnceLock::new();

/// The Wayland socket name the process inherited before main() removed it
/// from the environment to force gpui's X11 backend. Terminal child
/// processes get it back (terminal_gpui_engine spawn env) so user-launched
/// GUI apps keep running native Wayland even though this app is X11.
#[cfg(target_os = "linux")]
pub(crate) fn linux_inherited_wayland_display() -> Option<&'static str> {
    LINUX_INHERITED_WAYLAND_DISPLAY
        .get()
        .and_then(|value| value.as_deref())
}

/// CDXC:PlatformSupport 2026-10-28 WHY:
/// The Linux app hides `WAYLAND_DISPLAY` from its own environment to run as an X11 client (`force_gpui_x11_backend_for_windowed_cef` below), and every program it started inherited the gap. On Omarchy `xdg-open` started Chromium, which is set to run on Wayland only, and it quit at once with "Failed to connect to Wayland display" while `xdg-open` still reported success: the onboarding's YouTube link, Settings links, Open In and Reveal did nothing, and gxserver handed the gap to every agent session (browser sign-ins, `wl-copy`). Programs Ghostex starts get the inherited value back, as terminals already do (terminal_gpui_engine.rs); the app process itself stays X11.
pub(crate) fn restore_wayland_display_for_process_command(command: &mut std::process::Command) {
    #[cfg(target_os = "linux")]
    if let Some(wayland_display) = linux_inherited_wayland_display() {
        command.env("WAYLAND_DISPLAY", wayland_display);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = command;
}

#[cfg(target_os = "linux")]
fn force_gpui_x11_backend_for_windowed_cef() {
    /*
    CDXC:PlatformSupport 2026-07-04:
    Linux v1 runs the whole app as an X11 client (XWayland on Wayland
    desktops): CEF child-browser windows can only be reparented into an X11
    window, and that constraint is app-wide because the host GPUI window
    itself must be X11. gpui exposes no explicit backend constructor
    (LinuxPlatform/X11Client are crate-private); its selection input is
    guess_compositor()'s WAYLAND_DISPLAY/DISPLAY environment probe, the same
    mechanism Zed documents as `WAYLAND_DISPLAY='' zed`. Removing the
    variable here — as the first statement of main(), before any thread can
    read the environment — is therefore the intended API, not a workaround.
    Chromium's Ozone side of the same constraint lives in
    cef/linux_x11.rs (`--ozone-platform=x11`). Accepted v1 trade-offs,
    revisited when browser OSR unlocks native Wayland (plan Phase 4):
    fractional-scaling sharpness and weaker IME under XWayland.
    */
    let inherited = env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|value| !value.is_empty());
    let _ = LINUX_INHERITED_WAYLAND_DISPLAY.set(inherited);

    let has_x11_display = env::var("DISPLAY").is_ok_and(|value| !value.is_empty());
    if !has_x11_display {
        // Without X11/XWayland the app cannot run at all in v1; failing
        // loudly here beats gpui silently picking its headless client after
        // WAYLAND_DISPLAY disappears.
        eprintln!("ghostex-gpui requires an X11 display (Xorg or XWayland): DISPLAY is not set");
        std::process::exit(1);
    }

    // SAFETY: called before GPUI starts background threads or framework-owned
    // environment readers, so no concurrent environment access is possible.
    unsafe { env::remove_var("WAYLAND_DISPLAY") };

    linux_x11_scale::pin_gpui_x11_scale_factor();
}

fn gpui_platform_window_app_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        Some("ghostex".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn gpui_platform_window_icon() -> Option<Arc<image::RgbaImage>> {
    #[cfg(target_os = "linux")]
    {
        Some(
            GPUI_LINUX_WINDOW_ICON
                .get_or_init(|| {
                    let source = image::load_from_memory_with_format(
                        assets::APP_ICON_PNG,
                        image::ImageFormat::Png,
                    )
                    .expect("the embedded Ghostex Linux window icon must be a valid PNG")
                    .into_rgba8();
                    let (mut left, mut top) = (source.width(), source.height());
                    let (mut right, mut bottom) = (0, 0);
                    let mut found_visible_pixel = false;
                    for (x, y, pixel) in source.enumerate_pixels() {
                        if pixel.0[3] == 0 {
                            continue;
                        }
                        found_visible_pixel = true;
                        left = left.min(x);
                        top = top.min(y);
                        right = right.max(x);
                        bottom = bottom.max(y);
                    }
                    assert!(
                        found_visible_pixel,
                        "the embedded Ghostex Linux window icon must contain visible pixels"
                    );
                    Arc::new(
                        image::imageops::crop_imm(
                            &source,
                            left,
                            top,
                            right - left + 1,
                            bottom - top + 1,
                        )
                        .to_image(),
                    )
                })
                .clone(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn reconcile_gpui_managed_ghostty_config() {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let settings = snapshot.gpui_terminal_engine_settings();
    /*
    CDXC:Clipboard 2026-08-06:
    Older Ghostex managed blocks set `mouse-shift-capture = always`. When a
    full-screen application enables mouse reporting, that sends Shift-drag to
    the PTY too, leaving no gesture that can create the local selection Cmd-C
    requires. Reconcile the managed key to `false` so Shift keeps the standard
    terminal selection override for local and SSH-attached sessions. The same
    startup pass repairs the historical explicit theme color overrides while
    preserving user-authored lines outside Ghostex's marked block.
    */
    let mut keys = vec!["mouse-shift-capture"];
    if !settings.ghostty_theme.is_empty() {
        keys.push("theme");
    }
    let _ = shared_settings::write_ghostty_terminal_config_from_settings_object(
        snapshot.object(),
        &keys,
    );
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
#[derive(Clone, Copy)]
enum GpuiWindowCaptionControl {
    Minimize,
    Maximize,
    Restore,
    Close,
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
impl GpuiWindowCaptionControl {
    fn element_id(self) -> &'static str {
        match self {
            Self::Minimize => "ghostex-gpui-titlebar-window-minimize",
            Self::Maximize => "ghostex-gpui-titlebar-window-maximize",
            Self::Restore => "ghostex-gpui-titlebar-window-restore",
            Self::Close => "ghostex-gpui-titlebar-window-close",
        }
    }

    fn icon_path(self) -> &'static str {
        match self {
            Self::Minimize => TITLEBAR_ICON_WINDOW_MINIMIZE,
            Self::Maximize => TITLEBAR_ICON_WINDOW_MAXIMIZE,
            Self::Restore => TITLEBAR_ICON_WINDOW_RESTORE,
            Self::Close => TITLEBAR_ICON_WINDOW_CLOSE,
        }
    }

    fn icon_size(self) -> f32 {
        match self {
            Self::Close => 14.0,
            Self::Minimize | Self::Maximize | Self::Restore => 12.0,
        }
    }

    #[cfg(target_os = "windows")]
    fn window_control_area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}
