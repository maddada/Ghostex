// C4 light split: CEF runtime lifecycle -- init/shutdown, the
// App/BrowserProcessHandler/FocusHandler/KeyboardHandler registrations
// (including the OnSetFocus native-focus-arbitration contract described in
// the moved comments; do not reorder those blocks), and the origin-matching
// helpers `shutdown` and the permission handler depend on. Pure move out of
// `cef/shell.rs`. See docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

pub fn initialize(cx: &gpui::App) -> Result<()> {
    let state = CEF_RUNTIME.get_or_init(|| Mutex::new(None));
    let mut state = state
        .lock()
        .expect("CEF runtime mutex should not be poisoned");
    if state.is_some() {
        return Ok(());
    }
    CEF_SHUTDOWN_IN_PROGRESS.store(false, Ordering::Release);

    let args = cef::args::Args::new();
    let platform_runtime = platform::load_cef_runtime()?;

    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    platform::install_application_hooks();

    let mut app = GhostexGpuiCefApp::new();
    let process_exit_code = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if process_exit_code >= 0 {
        std::process::exit(process_exit_code);
    }

    let root_cache_path = cef_root_cache_path()?;
    /*
    CDXC:GPUIPrivacyAudit 2026-06-23-13:18:
    The built-in Default Browser profile and first-party app UI use the app-owned persistent global CEF store, while generated Browser profiles remain memory-backed. Keep CEF file logging disabled and Chromium runtime data out of support-bundle logs.
    */
    let mut settings = cef::Settings {
        no_sandbox: 1,
        external_message_pump: 1,
        cache_path: cef::CefString::from(root_cache_path.to_string_lossy().as_ref()),
        root_cache_path: cef::CefString::from(root_cache_path.to_string_lossy().as_ref()),
        persist_session_cookies: 1,
        log_severity: cef::LogSeverity::DISABLE,
        remote_debugging_port: remote_debugging_port(),
        ..Default::default()
    };
    platform::apply_platform_settings(&mut settings);

    /*
    CDXC:GPUICefRuntime 2026-06-14-15:25:
    The GPUI shell must use Tauri's cef-rs binding path instead of the earlier GhostexCEFBridge.mm browser wrapper. Initialize CEF through cef-rs, keep GPUI as the AppKit loop owner, and scope profile data to the GPUI app so the React sidebar and main browser share a stable Chromium runtime without production-host coupling.

    CDXC:GPUICefMessagePump 2026-06-14-16:29:
    GPUI runs a blocking NSApplication loop, so the cef-rs port must use CEF's external_message_pump together with BrowserProcessHandler::on_schedule_message_pump_work. CEF schedules each pump step and the AppKit shim executes it on the main queue, avoiding the Chromium run-loop observer trap caused by an unconditional timer.

    CDXC:GPUICefMessagePump 2026-06-14-16:54:
    CEF can call on_schedule_message_pump_work during cef::initialize before the first browser is created. Install the GPUI pump gate before initialization so those startup callbacks reach the main queue instead of leaving Chromium partially initialized with only helper processes alive.
    */
    platform::install_message_pump(cx);
    let initialized = cef::initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if initialized != 1 {
        platform::invalidate_message_pump();
        #[cfg(target_os = "windows")]
        if cef::get_exit_code() == cef::Resultcode::NORMAL_EXIT_AUTO_DE_ELEVATED.get_raw() {
            std::process::exit(0);
        }
        anyhow::bail!("CEF initialization returned false");
    }

    *state = Some(CefRuntimeState {
        _platform: platform_runtime,
        _app: app,
    });
    Ok(())
}

pub fn context_initialized() -> bool {
    CEF_CONTEXT_INITIALIZED.load(Ordering::Acquire)
}

wrap_app! {
    pub(crate) struct GhostexGpuiCefApp;

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(GhostexGpuiBrowserProcessHandler::new())
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(GhostexGpuiRenderProcessHandler::new())
        }

        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            /*
            CDXC:GPUICefCommandLine 2026-06-14-17:00:
            The GPUI shell is a local app and must not block CEF startup on macOS Keychain prompts or locks. Match production Ghostex's CEF switch set by using Chromium's mock keychain and keeping browser subprocesses foreground-capable for embedded child views.
            */
            if let Some(command_line) = command_line {
                command_line.append_switch(Some(&CefString::from("use-mock-keychain")));
                command_line.append_switch(Some(&CefString::from("enable-fullscreen")));
                command_line.append_switch(Some(&CefString::from("allow-insecure-localhost")));
                command_line.append_switch_with_value(
                    Some(&CefString::from("remote-allow-origins")),
                    Some(&CefString::from("*")),
                );
                // Per-OS Chromium switches (e.g. Linux forcing Ozone onto
                // X11 to match the app-wide X11 embedding constraint) stay
                // behind the platform seam like every other OS-specific
                // decision.
                platform::append_platform_command_line_switches(command_line);
            }
        }
    }
}

wrap_browser_process_handler! {
    pub(crate) struct GhostexGpuiBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            /*
            CEF's global request context and synchronous browser creation are
            valid only after this callback. cef::initialize returning does not
            imply that Chromium has reached that point, especially with a new
            profile on first launch.
            */
            CEF_CONTEXT_INITIALIZED.store(true, Ordering::Release);
        }

        fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
            if let Some(command_line) = command_line {
                command_line.append_switch(Some(&CefString::from("disable-background-mode")));
                command_line.append_switch(Some(&CefString::from(
                    "disable-backgrounding-occluded-windows",
                )));
            }
        }

        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            platform::schedule_message_pump_work(delay_ms);
        }
    }
}

/*
CDXC:GPUICefNativeFocus 2026-07-09:
Renderer-initiated focus requests (page JS focus()/re-render focus recovery)
must never move AppKit first-responder to a CEF child view: the shared
sidebar re-renders on every gxserver presentation delta, and without this
handler each render pulled key focus away from the active terminal for a few
milliseconds (dropped keystrokes; occasionally permanently until a click).
Native focus for CEF surfaces is exclusively app-owned: user mouse-down via
the AppKit focus subclass, or explicit Rust `focus_native_view` +
`host.set_focus` calls, both of which arrive here as FOCUS_SOURCE_SYSTEM and
stay allowed. Canceling NAVIGATION-source requests does not affect the
page's internal DOM focus, only the native first-responder transfer.
*/
wrap_focus_handler! {
    pub(crate) struct GhostexGpuiCefFocusHandler;

    impl FocusHandler {
        fn on_set_focus(
            &self,
            browser: Option<&mut cef::Browser>,
            source: FocusSource,
        ) -> c_int {
            /*
            CDXC:GPUICefNativeFocus 2026-07-10:
            NAVIGATION-source cancellation alone proved insufficient: a hidden
            titlebar-host page (Tips stays alive after close) requested native
            focus every ~30s through its keep-awake poll, and Chromium
            delivered that renderer-driven request as FOCUS_SOURCE_SYSTEM —
            stranding keyboard focus on an invisible surface until the next
            terminal click. A surface the app has hidden has no focus claim
            from any source, so cancel those requests outright.

            CDXC:GPUICefNativeFocus 2026-07-10-14:30:
            The hidden-only SYSTEM guard was still insufficient: visible
            background surfaces (a non-active project-workarea page plus the
            sidebar, each running the shared app's blur-recovery focus())
            stole first responder from each other as allowed SYSTEM requests,
            producing a millisecond-cadence focus war that saturated the main
            thread (2026-07-10 beach ball). Every app-owned grant — user
            mouse-down via the focus subclass, Rust `focus_native_view` +
            `host.set_focus`, and the edit-command responder shim — makes the
            CEF view first responder *before* Chromium raises OnSetFocus, so
            the app-owned arbitration rule is: a CEF surface may complete a
            native focus transfer only if it already contains the window's
            first responder. Requests from any surface that does not own the
            responder are renderer-initiated steals and are canceled
            regardless of source or visibility.
            */
            let (native_view, browser_id) = browser
                .map(|browser| {
                    (
                        browser
                            .host()
                            .map(|host| platform::native_view_ptr(host.window_handle())),
                        Some(browser.identifier()),
                    )
                })
                .unwrap_or((None, None));
            let hidden = native_view.is_some_and(cef_native_view_is_hidden);
            let responder_outside = native_view
                .is_some_and(|native_view| !platform::native_view_owns_first_responder(native_view));
            /*
            CDXC:GPUICefExplicitNativeFocusOwnership 2026-07-15:
            `native_view_owns_first_responder` cannot prove a SYSTEM request is
            app-owned: Chromium moves its NSView into first-responder position
            before invoking OnSetFocus, so a renderer `focus()` call satisfies
            that check while stealing input from a GPUI terminal. The existing
            active-native-view registry is the non-circular authority. AppKit
            CEF mouseDown and explicit Rust `focus_native_view` calls mark the
            exact browser root before requesting focus; returning focus to the
            GPUI root clears the registry. Reject every request that lacks that
            explicit current ownership, even if Chromium already changed the
            responder underneath us.
            */
            let explicitly_active = native_view
                .is_some_and(|native_view| {
                    active_cef_native_view() == Some(native_view as usize)
                        || (cfg!(target_os = "windows")
                            && platform::native_view_owns_first_responder(native_view))
                });
            let sidebar_editable_focus_granted = native_view.is_some_and(|native_view| {
                SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.load(Ordering::Acquire)
                    == native_view as usize
            });
            #[cfg(target_os = "windows")]
            let cancel = hidden
                || (source == FocusSource::NAVIGATION && !sidebar_editable_focus_granted);
            #[cfg(not(target_os = "windows"))]
            let cancel = hidden
                || responder_outside
                || !explicitly_active
                || source == FocusSource::NAVIGATION;
            #[cfg(target_os = "windows")]
            if !cancel {
                if let Some(native_view) = native_view {
                    set_active_cef_native_view(native_view as usize);
                }
            }
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.terminalFocus.cefNativeFocusRequest",
                serde_json::json!({
                    "browserId": browser_id,
                    "source": format!("{source:?}"),
                    "surfaceHidden": hidden,
                    "responderOutside": responder_outside,
                    "explicitlyActive": explicitly_active,
                    "sidebarEditableFocusGranted": sidebar_editable_focus_granted,
                    "canceled": cancel,
                }),
            );
            cancel as c_int
        }
    }
}

/*
CDXC:GPUICefPaneZoomShortcutsWindows 2026-08-12:
Windowed CEF owns keyboard focus in its Chromium child HWND on Windows, so
GPUI's Ctrl+=, Ctrl+-, and Ctrl+0 bindings cannot observe those keystrokes.
Install this handler only on Browser, main project-workarea, and Session Chat
clients (the same surfaces registered for macOS keyboard zoom) and consume the
Windows primary-modifier chord through Chromium's browser-host zoom API.
Sidebar, modal, titlebar, companion, and DevTools clients deliberately receive
no handler. The macOS AppKit responder path remains unchanged.
*/
#[cfg(target_os = "windows")]
wrap_keyboard_handler! {
    pub(crate) struct GhostexGpuiWindowsZoomKeyboardHandler;

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut cef::Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut cef::sys::MSG>,
            _is_keyboard_shortcut: Option<&mut c_int>,
        ) -> c_int {
            const VK_0: c_int = 0x30;
            const VK_OEM_PLUS: c_int = 0xBB;
            const VK_OEM_MINUS: c_int = 0xBD;
            const CONTROL_DOWN: u32 =
                cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0 as u32;
            const ALT_DOWN: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0 as u32;
            const COMMAND_DOWN: u32 =
                cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0 as u32;

            let Some(event) = event else {
                return 0;
            };
            if event.type_ != KeyEventType::RAWKEYDOWN
                || event.modifiers & CONTROL_DOWN == 0
                || event.modifiers & (ALT_DOWN | COMMAND_DOWN) != 0
            {
                return 0;
            }
            let command = match event.windows_key_code {
                VK_OEM_PLUS => ZoomCommand::IN,
                VK_OEM_MINUS => ZoomCommand::OUT,
                VK_0 => ZoomCommand::RESET,
                _ => return 0,
            };
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return 0;
            };
            host.zoom(command);
            1
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn keyboard_zoom_handler(enabled: bool) -> Option<KeyboardHandler> {
    enabled.then(GhostexGpuiWindowsZoomKeyboardHandler::new)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn keyboard_zoom_handler(_enabled: bool) -> Option<KeyboardHandler> {
    None
}
pub(crate) fn cef_normalized_origin(value: &str) -> Option<String> {
    // Mirrors macOS `GhostexCEFNormalizedOrigin`: lowercased scheme://host with
    // the explicit port, defaulting http/https ports so "http://127.0.0.1:80"
    // and "http://127.0.0.1" compare equal; hostless/invalid values are None.
    let (scheme, rest) = value.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let mut authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if let Some((_, host)) = authority.rsplit_once('@') {
        authority = host;
    }
    let (host, explicit_port) = if let Some(bracket_end) = authority.rfind(']') {
        let (host, remainder) = authority.split_at(bracket_end + 1);
        (host, remainder.strip_prefix(':'))
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, Some(port))
    } else {
        (authority, None)
    };
    if scheme.is_empty() || host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let port = match explicit_port {
        Some(port) => port.parse::<u32>().ok()?,
        None => match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => return Some(format!("{scheme}://{host}")),
        },
    };
    Some(format!("{scheme}://{host}:{port}"))
}

pub(crate) fn cef_origins_match(lhs: &str, rhs: &str) -> bool {
    match (cef_normalized_origin(lhs), cef_normalized_origin(rhs)) {
        (Some(lhs), Some(rhs)) => lhs == rhs,
        _ => false,
    }
}

#[allow(dead_code)]
pub fn shutdown() {
    let Some(state) = CEF_RUNTIME.get() else {
        return;
    };
    let mut state = state
        .lock()
        .expect("CEF runtime mutex should not be poisoned");
    if state.is_none() {
        return;
    }
    CEF_SHUTDOWN_IN_PROGRESS.store(true, Ordering::Release);
    state.take();
    CEF_REQUEST_CONTEXTS_BY_PROFILE.with(|contexts| {
        contexts.borrow_mut().clear();
    });
    CEF_GLOBAL_REQUEST_CONTEXT.with(|context| {
        context.borrow_mut().take();
    });
    platform::invalidate_message_pump();
    cef::shutdown();
    CEF_CONTEXT_INITIALIZED.store(false, Ordering::Release);
}
