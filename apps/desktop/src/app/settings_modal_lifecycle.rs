//! Open, save, bridge and hydrate plumbing for the native Settings modal.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/ (the window, its pages and its decision
//! record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path),
//! docs/2026-09-28/gpui-modals-migration/SETTINGS-ARCH.md.
use crate::app::gx_store::write_client_document_value;
use crate::app::helpers::web_bridge_types::{
    AppModalHostBridgeEvent, AppModalHostBridgeEventHandler,
};
use crate::app::helpers::*;
use crate::app::window::settings_modal::catalog::settings_catalog;
use crate::app::window::settings_modal::{
    GpuiSettingsModalWindow, SETTINGS_MODAL_HEIGHT, SETTINGS_MODAL_WIDTH, SettingsModalCommand,
    SettingsModalConfig, SettingsOpenRequest,
};
use crate::app::window::settings_modal::model::SettingsTabId;
use crate::*;

/// The per-dialog prompt-agent overrides (`PROMPT_AGENT_MODAL_STORAGE_KEYS` of the React modal host):
/// the Git Commit review's and Rename's Generate Name agent choices.
const PROMPT_AGENT_OVERRIDE_KEYS: [&str; 2] = [
    "ghostex.promptAgent.gitCommit",
    "ghostex.promptAgent.renameSession",
];

/// `clearPromptAgentModalOverrides` of apps/desktop/views/modal-host.tsx (deleted 2026-10-01), run when a save changes
/// `defaultPromptAgentId` from a set value, so both dialogs show the new default next time.
///
/// CDXC:AgentLauncher 2026-09-28 SEE-ALSO:
/// The React modal host clears these keys whenever the hydrated default prompt agent changes; the native Settings clears them on its own saves (apps/desktop/src/app/git_commit_modal_lifecycle.rs reads the Git Commit key).
fn clear_prompt_agent_overrides_if_default_changed(
    next_settings: &serde_json::Map<String, serde_json::Value>,
    whole: bool,
    cx: &mut gpui::App,
) {
    const KEY: &str = "defaultPromptAgentId";
    // Stored settings leave defaults out; read a missing value as `DEFAULT_ghostex_SETTINGS`'.
    let value = |settings: &serde_json::Map<String, serde_json::Value>| {
        settings
            .get(KEY)
            .or_else(|| settings_catalog().default_value(KEY))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    if !whole && !next_settings.contains_key(KEY) {
        return;
    }
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let Some(previous) = value(snapshot.object()).filter(|previous| !previous.is_empty()) else {
        return;
    };
    if value(next_settings).as_deref() == Some(previous.as_str()) {
        return;
    }
    cx.background_executor()
        .spawn(async move {
            for key in PROMPT_AGENT_OVERRIDE_KEYS {
                let _ = write_client_document_value(key, None);
            }
        })
        .detach();
}

/// The largest body a Settings `HttpGet` reads (an extension README or screenshot).
const SETTINGS_HTTP_GET_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// `fetch(url)` of the React Extensions page: a remote catalog file, or with a leading `/` a
/// gxserver static file (`extensionStaticAssetUrl`, served without a token). Blocking.
fn settings_modal_http_get(url: &str) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let url = if url.starts_with('/') {
        format!(
            "http://{GPUI_GXSERVER_LOCAL_API_HOST}:{}{url}",
            gpui_local_gxserver_api_port()
        )
    } else {
        url.to_string()
    };
    let tls_config = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .http_status_as_error(false)
        .tls_config(tls_config)
        .build();
    let mut response = ureq::Agent::new_with_config(config)
        .get(&url)
        .call()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("The request failed with HTTP {}.", status.as_u16()));
    }
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(SETTINGS_HTTP_GET_MAX_BYTES)
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(body)
}

impl GhostexGpuiApp {
    /// Opens the native Settings modal on the page `kind` (or the `open` message) names. The
    /// message carries the same deep links the React host read (`initialTab`, `initialSection`,
    /// `initialSearchQuery`, ...), and its `latestSidebarStateMessage` when the launcher attached one.
    pub(crate) fn open_gpui_settings_modal(
        &mut self,
        kind: GpuiAppModalKind,
        open_message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let sidebar_state = open_message
            .get("latestSidebarStateMessage")
            .filter(|message| message.is_object())
            .cloned()
            .unwrap_or_else(|| self.gpui_app_modal_sidebar_state_message_for_open(kind, cx));
        let mut request = SettingsOpenRequest::from_open_message(kind.modal_id(), open_message);
        // Settings > Projects opens on this window's project unless the opener named one.
        if request.initial_project_id.is_none() {
            request.initial_project_id = self.active_project_id_for_view_scope();
        }
        // The React page got its gxserver RPC from the page bootstrap; here it is the local token.
        request.gxserver_rpc_available = read_gpui_gxserver_auth_token().is_ok();
        let config = SettingsModalConfig {
            palette: self.gpui_native_modal_palette(),
            request,
            sidebar_state,
        };
        let bridge = self.app_modal_host_bridge_event_handler(cx);
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_settings_modal_command(kind, command, &bridge, cx);
        });
        self.open_native_app_modal(
            kind,
            SETTINGS_MODAL_WIDTH,
            SETTINGS_MODAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiSettingsModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    /// Runs what the modal asked for through the handlers the React Settings modal's messages
    /// reach: saves go to the settings service like `updateSettingsPatch` / `updateSettings`, and a
    /// posted message goes through the same app-modal bridge as the React host's
    /// `{ type: "sidebarCommand", message }`.
    fn handle_gpui_settings_modal_command(
        &mut self,
        kind: GpuiAppModalKind,
        command: SettingsModalCommand,
        bridge: &AppModalHostBridgeEventHandler,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            SettingsModalCommand::SavePatch { patch, source } => {
                clear_prompt_agent_overrides_if_default_changed(&patch, false, cx);
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &serde_json::json!({
                        "patch": patch,
                        "source": source,
                        "type": "updateSettingsPatch",
                    }),
                    cx,
                );
            }
            SettingsModalCommand::SaveSettings { settings, source } => {
                clear_prompt_agent_overrides_if_default_changed(&settings, true, cx);
                self.handle_gpui_app_modal_update_settings_message(
                    &serde_json::json!({
                        "settings": settings,
                        "source": source,
                        "type": "updateSettings",
                    }),
                    cx,
                );
            }
            SettingsModalCommand::PostMessage(message) => {
                bridge(AppModalHostBridgeEvent::Message(
                    serde_json::json!({ "message": message, "type": "sidebarCommand" }).to_string(),
                ));
            }
            SettingsModalCommand::Toast {
                level,
                title,
                description,
            } => {
                self.dispatch_gpui_workspace_action_toast(&level, &title, &description, cx);
            }
            SettingsModalCommand::PickSystemColor { key, initial } => {
                self.open_settings_system_color_panel(kind, key, &initial, cx);
            }
            SettingsModalCommand::GxserverRpc {
                path,
                params,
                timeout,
                reply,
            } => {
                // A Cloud Boxes status read also refreshes the app's own agentbox cache, so a
                // provider set up there shows in the next launcher and picker (gx_store/agentbox.rs).
                let agentbox_status = path == "/api/agentbox" && params["action"] == "status";
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move { gpui_gxserver_rpc_result(&path, &params, timeout) })
                        .await;
                    if agentbox_status && let Ok(status) = &result {
                        let _ = this.update(cx, |this, cx| {
                            this.note_agentbox_status_answer(status, cx);
                        });
                    }
                    let _ = cx.update(|cx| reply(result, cx));
                })
                .detach();
            }
            SettingsModalCommand::CopyToClipboard(text) => {
                gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(text), cx);
            }
            SettingsModalCommand::HttpGet { url, reply } => {
                let background = cx.background_executor().clone();
                cx.spawn(async move |_, cx| {
                    let result = background
                        .spawn(async move { settings_modal_http_get(&url) })
                        .await;
                    let _ = cx.update(|cx| reply(result, cx));
                })
                .detach();
            }
            SettingsModalCommand::HostMessage(message) => {
                bridge(AppModalHostBridgeEvent::Message(message.to_string()));
            }
            SettingsModalCommand::Close => {
                self.release_native_app_modal_window(kind, cx);
            }
            SettingsModalCommand::OpenAccounts => {
                // Settings still open on another page switches to Accounts in place.
                let switched = self.update_native_app_modal::<GpuiSettingsModalWindow, _>(
                    GpuiAppModalKind::Settings,
                    cx,
                    |settings, window, cx| {
                        settings.select_page(SettingsTabId::Accounts, window, cx);
                        window.activate_window();
                    },
                );
                if switched.is_none() {
                    self.open_gpui_settings_accounts_page(None, cx);
                }
            }
        }
    }

    /// `<input type="color">` in CEF: the system colour panel owned by the Settings window
    /// (ChooseColorW on Windows, NSColorPanel on macOS); every colour it reports is saved through
    /// the modal like the input's `onChange`. Where there is no system panel to show, the field
    /// opens the in-app Pick Color dialog instead.
    fn open_settings_system_color_panel(
        &mut self,
        kind: GpuiAppModalKind,
        key: String,
        initial: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let owner = self
            .update_native_app_modal(kind, cx, |_: &mut GpuiSettingsModalWindow, window, _| {
                system_color_panel::owner(window)
            })
            .flatten();
        let Some(mut picked) = system_color_panel::open(owner, initial) else {
            self.update_native_app_modal(kind, cx, |modal: &mut GpuiSettingsModalWindow, _, cx| {
                modal.open_in_app_color_picker(&key, cx);
            });
            return;
        };
        cx.spawn(async move |this, cx| {
            use futures::StreamExt as _;
            while let Some(hex) = picked.next().await {
                let key = key.clone();
                let delivered = this
                    .update(cx, |app, cx| {
                        app.update_native_app_modal(
                            kind,
                            cx,
                            |modal: &mut GpuiSettingsModalWindow, _, cx| {
                                modal.receive_system_color(&key, hex, cx);
                            },
                        )
                        .is_some()
                    })
                    .unwrap_or(false);
                if !delivered {
                    break;
                }
            }
        })
        .detach();
    }

    fn native_settings_modal_kind(&self) -> Option<GpuiAppModalKind> {
        self.native_app_modal_kind()
            .filter(|kind| kind.is_settings_modal_entry())
    }

    /// A fresh `sidebarState` hydrate (after a save anywhere) for the open native Settings modal.
    /// Returns false when it is not open.
    pub(crate) fn refresh_native_settings_modal_sidebar_state(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(kind) = self.native_settings_modal_kind() else {
            return false;
        };
        self.update_native_app_modal(
            kind,
            cx,
            move |modal: &mut GpuiSettingsModalWindow, _window, cx| {
                modal.receive_sidebar_state(message, cx);
            },
        )
        .is_some()
    }

    /// A transient payload (`agentHookStatus`, `ghostexCliStatus`, `settingsActionStatus`, ...)
    /// or a modal message for the open native Settings modal. Returns false when it is not open.
    pub(crate) fn receive_native_settings_modal_payload(
        &mut self,
        payload: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(kind) = self.native_settings_modal_kind() else {
            return false;
        };
        let payload = payload.clone();
        self.update_native_app_modal(
            kind,
            cx,
            move |modal: &mut GpuiSettingsModalWindow, _window, cx| {
                modal.receive_host_payload(payload, cx);
            },
        )
        .is_some()
    }
}

/// The system colour panel Chromium showed for `<input type="color">`, reporting `#rrggbb` picks.
mod system_color_panel {
    use futures::channel::mpsc::UnboundedReceiver;

    /// The Settings window's native handle, for owning the panel.
    pub(super) fn owner(window: &gpui::Window) -> Option<isize> {
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let handle = HasWindowHandle::window_handle(window).ok()?;
            match handle.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = window;
            None
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    fn rgb_of(hex: &str) -> u32 {
        u32::from_str_radix(hex.trim().trim_start_matches('#'), 16).unwrap_or(0) & 0xff_ffff
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    fn hex_of(rgb: u32) -> String {
        format!("#{:06x}", rgb & 0xff_ffff)
    }

    /// Opens the panel at `initial`; `None` where this platform has none.
    pub(super) fn open(owner: Option<isize>, initial: &str) -> Option<UnboundedReceiver<String>> {
        #[cfg(target_os = "windows")]
        {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            let initial = rgb_of(initial);
            // ChooseColorW runs its own modal loop; like GPUI's file dialogs it gets its own
            // thread so the app keeps pumping the owner's messages.
            std::thread::Builder::new()
                .name("settings-color-dialog".into())
                .spawn(move || {
                    if let Some(rgb) = windows::choose_color(owner, initial) {
                        let _ = sender.unbounded_send(hex_of(rgb));
                    }
                })
                .ok()?;
            Some(receiver)
        }
        #[cfg(target_os = "macos")]
        {
            let _ = owner;
            Some(macos::show_color_panel(rgb_of(initial)))
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let _ = (owner, initial);
            None
        }
    }

    #[cfg(target_os = "windows")]
    mod windows {
        use std::sync::Mutex;
        use windows_sys::Win32::UI::Controls::Dialogs::{
            CC_ANYCOLOR, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW,
        };

        /// The dialog's sixteen custom colours, kept for the session like Chromium's
        /// `g_custom_colors`.
        static CUSTOM_COLORS: Mutex<[u32; 16]> = Mutex::new([0; 16]);

        /// COLORREF is 0x00BBGGRR (and swapping back is the same operation).
        fn colorref(rgb: u32) -> u32 {
            ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
        }

        /// Chromium's `ColorChooserDialog`: `CC_ANYCOLOR | CC_FULLOPEN | CC_RGBINIT`, owned by the
        /// Settings window. `None` when cancelled.
        pub(super) fn choose_color(owner: Option<isize>, initial: u32) -> Option<u32> {
            let mut custom = CUSTOM_COLORS
                .lock()
                .map(|colors| *colors)
                .unwrap_or([0; 16]);
            let mut dialog = CHOOSECOLORW {
                lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
                hwndOwner: owner.unwrap_or(0) as _,
                rgbResult: colorref(initial),
                lpCustColors: custom.as_mut_ptr(),
                Flags: CC_ANYCOLOR | CC_FULLOPEN | CC_RGBINIT,
                ..Default::default()
            };
            let chosen = unsafe { ChooseColorW(&mut dialog) } != 0;
            if let Ok(mut colors) = CUSTOM_COLORS.lock() {
                *colors = custom;
            }
            chosen.then(|| colorref(dialog.rgbResult))
        }
    }

    #[cfg(target_os = "macos")]
    mod macos {
        use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
        use std::ffi::c_void;

        type Callback = extern "C" fn(context: *mut c_void, rgb: u32, finished: bool);

        unsafe extern "C" {
            /// native/macos/GpuiColorPanel.m
            fn GhostexGpuiShowColorPanel(rgb: u32, context: *mut c_void, callback: Callback);
        }

        extern "C" fn color_panel_event(context: *mut c_void, rgb: u32, finished: bool) {
            if context.is_null() {
                return;
            }
            if finished {
                drop(unsafe { Box::from_raw(context as *mut UnboundedSender<String>) });
                return;
            }
            let sender = unsafe { &*(context as *const UnboundedSender<String>) };
            let _ = sender.unbounded_send(super::hex_of(rgb));
        }

        /// NSColorPanel reports every change while it is open, as Chromium's `ColorPanelCocoa`
        /// did; the stream ends when the panel closes or another field takes it over.
        pub(super) fn show_color_panel(initial: u32) -> UnboundedReceiver<String> {
            let (sender, receiver) = unbounded();
            let context = Box::into_raw(Box::new(sender)) as *mut c_void;
            unsafe { GhostexGpuiShowColorPanel(initial, context, color_panel_event) };
            receiver
        }
    }
}
