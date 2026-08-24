#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![recursion_limit = "256"]

/*
CDXC:GPUIBuildSchemaPayloads 2026-06-28-17:09:
GPUI still has schema-sized privacy-boundary serde_json::json! payloads outside the removed project-workarea proof chain. Keep the crate recursion limit high enough for those explicit payloads while runtime behavior is owned by direct gates.
*/
mod app;
mod app_icon;
mod assets;
mod cef;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod cef_component_window;
mod component_store;
mod ghostty_kit;
mod ghostty_vt;
mod navigation_history;
mod plugins_modal;
mod shared_settings;
mod support_logs;
mod terminal_element;
mod terminal_environment;
mod terminal_ghostty_surface;
mod terminal_gpui_engine;
mod terminal_model;
mod terminal_native_view;
mod terminal_osc_title;
mod terminal_surface_host;
mod terminal_surface_lifecycle;
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
use std::sync::OnceLock;
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
use gpui::Modifiers;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Point;
use gpui::PressureStage;
use gpui::Render;
use gpui::RenderOnce;
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
use gpui_component::Selectable;
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
pub(crate) use crate::app::ffi::*;
pub(crate) use crate::app::hotkeys::*;
pub(crate) use crate::app::model::*;
// C1 wave-4 extraction: the god object itself now lives in app/core.rs; the
// crate-root re-export keeps `crate::GhostexGpuiApp` resolving for main(),
// the sibling modules that hold `Entity<GhostexGpuiApp>`, and the FFI bridge.
pub(crate) use crate::app::core::*;

fn main() {
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
        CDXC:GPUIUserToolPath 2026-07-24:
        LaunchServices gives packaged macOS apps a system-only PATH. Normalize
        it once at the process boundary so CLI status, bundled-skill installs,
        Cua Driver checks, and other fixed local-tool actions see the same
        standard user locations as gxserver. This is startup environment
        ownership, not a per-action fallback.
        */
        #[cfg(target_os = "macos")]
        {
            let current_path = env::var("PATH").ok();
            env::set_var(
                "PATH",
                gpui_normalized_user_tool_path(current_path.as_deref()),
            );
        }
    }
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    cef_component_window::configure_cef_framework_path_for_process();
    // The Linux app is X11-only for v1 (CEF child-window embedding requires
    // an X11 host window), so backend selection must happen before framework
    // initialization or background work can read the environment.
    #[cfg(target_os = "linux")]
    force_gpui_x11_backend_for_windowed_cef();
    /*
    CDXC:GPUIWindowsBringup 2026-07-25:
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
    cef::prepare_application();
    #[cfg(target_os = "macos")]
    reconcile_gpui_managed_ghostty_config();
    // Workspace chrome background follows the user's Ghostty `background`
    // config color (macOS defaultWorkspaceBackgroundColor parity). Read once
    // before the window opens so first paint already uses the real color.
    initialize_workspace_background_color_from_ghostty_config();
    refresh_gpui_visual_settings(&shared_settings::shared_sidebar_settings_snapshot());

    let application = gpui_platform::application().with_assets(assets::GhostexAssets);
    // OS-integration URL/file opens (ghostex:// + Finder Open With) hook the
    // platform's application:openURLs: delegate before the run loop starts so
    // launch-time opens are buffered until the app entity registers.
    #[cfg(target_os = "macos")]
    application.on_open_urls(queue_gpui_os_integration_urls);
    application.run(move |cx| {
        gpui_component::init(cx);
        apply_gpui_component_dark_theme(cx);
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
        CDXC:GPUISourceViewHotkeyPassthrough 2026-08-03:
        Do not bind Cmd+A in CEF_KEY_CONTEXT. Source must receive the original
        trusted chord so Monaco can run its editor-owned Select All command;
        Browser and other CEF surfaces keep their native AppKit selectAll:
        bridge in GpuiCefAppKitHooks.m.
        */
        let shell_key_bindings = vec![
            KeyBinding::new("f12", OpenCommandPane, None),
            gpui_key_binding_from_shared_hotkey("cmd+v", PasteIntoFocusedTerminal, None),
            gpui_key_binding_from_shared_hotkey("cmd+f", FindInFocusedTerminal, None),
            gpui_key_binding_from_shared_hotkey("cmd+g", FindNextInFocusedBrowser, None),
            gpui_key_binding_from_shared_hotkey("cmd+shift+g", FindPreviousInFocusedBrowser, None),
            gpui_key_binding_from_shared_hotkey("cmd+=", ZoomInFocusedSurface, None),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-+"
                } else {
                    "ctrl-+"
                },
                ZoomInFocusedSurface,
                None,
            ),
            gpui_key_binding_from_shared_hotkey("cmd+-", ZoomOutFocusedSurface, None),
            gpui_key_binding_from_shared_hotkey("cmd+0", ResetFocusedSurfaceZoom, None),
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
            KeyBinding::new("ctrl-tab", CycleFocusedTabForward, None),
            KeyBinding::new("ctrl-shift-tab", CycleFocusedTabBackward, None),
            gpui_key_binding_from_shared_hotkey("cmd+w", CloseFocusedSurface, None),
            gpui_key_binding_from_shared_hotkey("cmd+b", ToggleGpuiSidebarCollapsed, None),
            gpui_key_binding_from_shared_hotkey("cmd+alt+b", ToggleProjectEditorCompanion, None),
            KeyBinding::new(SLEEP_FOCUSED_SESSION_DEFAULT_KEY, SleepFocusedSession, None),
            gpui_key_binding_from_shared_hotkey("cmd+t", NewTerminalTab, None),
            gpui_key_binding_from_shared_hotkey("cmd+d", SplitFocusedTerminalRight, None),
            gpui_key_binding_from_shared_hotkey("cmd+shift+d", SplitFocusedTerminalDown, None),
            gpui_key_binding_from_shared_hotkey("cmd+n", NewBrowserTab, None),
            gpui_key_binding_from_shared_hotkey("cmd+ctrl+f", ToggleAgentsFocusMode, None),
            gpui_key_binding_from_shared_hotkey(
                gpui_platform_hotkey_for_action("mergeAllTabs", "ctrl+shift+m"),
                MergeAllTabs,
                None,
            ),
            KeyBinding::new("alt-1", SwitchAgentsWorkarea, None),
            KeyBinding::new("alt-2", SwitchSourceWorkarea, None),
            KeyBinding::new("alt-3", SwitchBrowserWorkarea, None),
            KeyBinding::new("alt-4", SwitchKanbanWorkarea, None),
            KeyBinding::new("alt-5", SwitchManageWorkarea, None),
            gpui_key_binding_from_shared_hotkey("cmd+alt+left", FocusWorkspaceLeft, None),
            gpui_key_binding_from_shared_hotkey("cmd+alt+right", FocusWorkspaceRight, None),
            gpui_key_binding_from_shared_hotkey("cmd+alt+up", FocusWorkspaceUp, None),
            gpui_key_binding_from_shared_hotkey("cmd+alt+down", FocusWorkspaceDown, None),
        ];
        #[cfg(target_os = "macos")]
        let shell_key_bindings = {
            let mut bindings = shell_key_bindings;
            bindings.extend([
                KeyBinding::new("cmd-q", QuitGhostexGpui, None),
                KeyBinding::new("cmd-h", HideGhostexGpui, None),
                KeyBinding::new("alt-cmd-h", HideGhostexGpuiOthers, None),
                KeyBinding::new("cmd-m", MinimizeGhostexGpuiWindow, None),
            ]);
            bindings
        };
        cx.bind_keys(shell_key_bindings);
        // The user's configured hotkey table binds after the base defaults so
        // configured chords win conflicts. Ids dispatch through the shared
        // runGhostexHotkeyAction route regardless of which surface has focus.
        cx.bind_keys(gpui_configured_hotkey_key_bindings_from_settings());
        gpui_prewarm_ghostex_editor_daemon();
        // Window frame persistence (macOS persistMainWindowChrome parity):
        // restore the saved frame with multi-monitor rules, else the
        // historical centered default.
        let window_bounds = restored_gpui_window_bounds(cx)
            .unwrap_or_else(|| WindowBounds::centered(size(px(1280.0), px(820.0)), cx));
        let options = WindowOptions {
            window_bounds: Some(window_bounds),
            window_min_size: Some(size(
                px(GPUI_WINDOW_FRAME_MIN_WIDTH),
                px(GPUI_WINDOW_FRAME_MIN_HEIGHT),
            )),
            app_id: gpui_platform_window_app_id(),
            icon: gpui_platform_window_icon(),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Ghostex".into()),
                appears_transparent: true,
                traffic_light_position: Some(gpui::point(px(11.0), px(8.0))),
            }),
            ..Default::default()
        };

        /*
        CDXC:GPUICefStartup 2026-06-14-13:10:
        CEF is mandatory for the GPUI shell, but CEF surfaces need an actual GPUI platform window before they attach native AppKit children. Create the GPUI window first, then let the CEF bridge wait for non-zero layout bounds before creating browser hosts.

        CDXC:GPUICefStartup 2026-06-14-13:09:
        CEF startup must run after GPUI completes the first frame because initializing native Chromium children during root construction can stall the GPUI launch path without producing helper processes. Schedule CEF surface creation on the next frame, then explicitly refresh the window so the sidebar and browser elements enter the normal GPUI layout pass.
        */
        let main_window = cx
            .open_window(options, |window, cx| {
                window.activate_window();
                let view = GhostexGpuiApp::new(window, cx).expect("failed to create Ghostex app");
                register_ghostex_gpui_main_menu_actions(
                    view.downgrade(),
                    gpui::Window::window_handle(window),
                    cx,
                );
                let view_for_cef = view.clone();
                window.on_next_frame(move |window, cx| {
                    view_for_cef.update(cx, |app, cx| {
                        #[cfg(any(
                            target_os = "macos",
                            target_os = "windows",
                            target_os = "linux"
                        ))]
                        app.begin_cef_startup(cx);
                        #[cfg(not(any(
                            target_os = "macos",
                            target_os = "windows",
                            target_os = "linux"
                        )))]
                        app.initialize_cef(cx);
                    });
                    window.refresh();
                });
                view.update(cx, |app, cx| {
                    app.start_gpui_support_log_maintenance(cx);
                    app.start_gpui_local_gxserver_bootstrap(cx);
                    app.start_gpui_workspace_open_target_availability_scan(cx);
                    app.start_gpui_updater(cx);
                    cx.on_app_quit(|this, cx| {
                        this.flush_gpui_quit_persistence(cx);
                        persist_gpui_window_frame_state();
                        async {}
                    })
                    .detach();
                });
                view.update(cx, |_, cx| {
                    record_gpui_window_frame_state(window, cx);
                    cx.observe_window_bounds(window, |app, window, cx| {
                        /*
                        macOS delivers bounds observer callbacks for window events
                        that do not actually change the frame (e.g. key/order
                        churn when a child panel opens). Close the anchored
                        titlebar dropdown only when the frame genuinely moved or
                        resized, otherwise every dropdown open self-closed within
                        one frame.
                        */
                        let previous_frame_state =
                            GPUI_LATEST_WINDOW_FRAME_STATE.with(|latest| latest.borrow().clone());
                        record_gpui_window_frame_state(window, cx);
                        let current_frame_state =
                            GPUI_LATEST_WINDOW_FRAME_STATE.with(|latest| latest.borrow().clone());
                        if previous_frame_state != current_frame_state {
                            if let Some(state) = app.titlebar_popup_menu.as_ref() {
                                log_gpui_titlebar_popup_repro(
                                    "gpui.titlebarPopup.mainWindowBoundsChanged",
                                    serde_json::json!({
                                        "kind": state.kind.diagnostic_label(),
                                        "mainWindowActive": window.is_window_active(),
                                        "windowBounds": gpui_titlebar_popup_bounds_diagnostic(
                                            Some(window.bounds())
                                        ),
                                    }),
                                );
                            }
                            app.close_gpui_titlebar_popup(None, window, cx);
                        }
                    })
                    .detach();
                    cx.observe_window_activation(window, |app, window, cx| {
                        log_gpui_titlebar_popup_repro(
                            "gpui.titlebarPopup.mainWindowActivationChanged",
                            serde_json::json!({
                                "kind": app
                                    .titlebar_popup_menu
                                    .as_ref()
                                    .map(|state| state.kind.diagnostic_label()),
                                "mainWindowActive": window.is_window_active(),
                            }),
                        );
                        if !window.is_window_active() {
                            app.close_gpui_titlebar_popup(None, window, cx);
                            /*
                            CDXC:GPUISidebarPointerTracking 2026-08-02:
                            Pointer-moved events stop arriving once the window is
                            no longer active, so the last crossing the observer saw
                            may have been an enter. Report the pointer as outside
                            and close any open sidebar context menu, the same way
                            leaving for another app closes a native menu.

                            CDXC:GPUISidebarPointerTracking 2026-08-20:
                            Route the "outside" report through the AppKit observer
                            instead of writing the page flag here. This used to
                            call `dispatch_gpui_sidebar_pointer_inside(false)`
                            directly, which left the observer's cache saying
                            "inside" while the page said "false"; the next real
                            crossing back into the sidebar then matched the cache
                            and was dropped as redundant, so hovering a session row
                            showed neither the row background nor the hover-only
                            Close button until the pointer left the sidebar and
                            came back. Clicking a tab in the tab strip churns window
                            activation, which is why that click was the reliable way
                            to get into the broken state.
                            */
                            #[cfg(target_os = "macos")]
                            {
                                cef::report_sidebar_pointer_outside();
                                app.dispatch_gpui_sidebar_dismiss_context_menus(cx);
                            }
                        } else {
                            /*
                            CDXC:GPUISidebarPointerTracking 2026-08-20:
                            Coming back active is the other half: the pointer can
                            already be sitting on a session row, and a pointer that
                            does not move produces no event to recompute from, so
                            resolve the crossing from the real pointer location.
                            */
                            #[cfg(target_os = "macos")]
                            cef::refresh_sidebar_pointer_inside();
                        }
                    })
                    .detach();
                });
                cx.new(|cx| Root::new(view, window, cx).bg(workspace_background_color()))
            })
            .expect("failed to open GPUI window");
        let main_window_id = main_window.window_id();
        /*
        CDXC:GPUIWindowsMainWindowQuit 2026-08-02:
        The main workspace window owns application lifetime. App-modal, toast,
        and titlebar child windows can still be registered when the user closes
        the workspace, so waiting for `cx.windows()` to become empty leaves a
        headless Ghostex process holding CEF's persistent-profile singleton.
        A subsequent Ghostex.exe then reaches `cef_initialize` while that stale
        owner is alive and exits with "CEF initialization returned false".
        Quit when the main window itself closes; keep the empty-window arm for
        defensive parity if a platform closes every child before this observer.
        */
        cx.on_window_closed(move |cx, window_id| {
            persist_gpui_window_frame_state();
            if window_id == main_window_id || cx.windows().is_empty() {
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

#[cfg(target_os = "linux")]
fn force_gpui_x11_backend_for_windowed_cef() {
    /*
    CDXC:GPUILinuxX11Backend 2026-07-04:
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
                        include_bytes!("../resources/AppIcon.appiconset/icon_256x256.png"),
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
    CDXC:GPUITerminalRemoteTuiCopy 2026-08-06:
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

#[derive(IntoElement)]
struct GpuiTitlebarTipsTrigger {
    selected: bool,
    show_badge: bool,
}

impl GpuiTitlebarTipsTrigger {
    fn new(show_badge: bool) -> Self {
        Self {
            selected: false,
            show_badge,
        }
    }
}

impl Selectable for GpuiTitlebarTipsTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for GpuiTitlebarTipsTrigger {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let icon_color = if self.selected {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };

        div()
            .id("ghostex-gpui-titlebar-button-tips")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .text_color(icon_color)
            .cursor_default()
            .when(self.selected, |this| this.bg(titlebar_button_hover_color()))
            .hover(|this| {
                this.bg(titlebar_button_hover_color())
                    .text_color(titlebar_icon_hover_color())
            })
            .child(titlebar_svg_icon(TITLEBAR_ICON_INFO, 16.0, icon_color))
            .when(self.show_badge, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(8.0))
                        .top(px(5.0))
                        .size(px(7.5))
                        .rounded_full()
                        .border_1()
                        .border_color(titlebar_background())
                        .bg(rgb(0x95d7f6)),
                )
            })
    }
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
