//! When the one-time Native mode announcement modal opens on desktop and what its primary button
//! does (`Q4773`/`Q4774`). The window itself is `app/window/native_mode_intro_modal.rs`.
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! User picked Option F ("Bold minimal") showing once after updating on desktop only (phone has
//! its own bottom sheet). Closing with Not now, Esc, or the corner × marks it done immediately so
//! it never opens again. Clicking "Turn on Native mode" sets `agentSessionMode` to `"non-cli"`
//! for Antigravity, Claude Code, and Codex, calls `/api/switchEligibleSessionsToAcp` to switch
//! idle sessions and idle coordinators right away (skipping worker threads and mid-turn sessions),
//! and shows a short result toast.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{AppContext as _, Context};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::model::GpuiAppModalKind;
use crate::app::window::{
    GpuiNativeModeIntroModalWindow, NATIVE_MODE_INTRO_MODAL_INITIAL_HEIGHT,
    NATIVE_MODE_INTRO_MODAL_WIDTH, NativeModeIntroCommand, NativeModeIntroHost,
    NativeModeIntroModalConfig,
};

const SETTINGS_INTRO_KEY: &str = "nativeModeIntro";
const NATIVE_FAMILIES: [&str; 3] = ["antigravity", "claude", "codex"];

/// Opened in this run already (by any window), so a second window never opens it twice before the
/// settings file catches up.
static NATIVE_MODE_INTRO_OPENED: AtomicBool = AtomicBool::new(false);

pub(crate) fn format_bulk_switch_summary(switched: usize, deferred: usize) -> String {
    let plural = |count: usize| if count == 1 { "chat" } else { "chats" };
    match (switched, deferred) {
        (s, d) if s > 0 && d > 0 => format!(
            "Switched {s} {}; {d} will switch when they finish or reload.",
            plural(s)
        ),
        (s, 0) if s > 0 => format!(
            "Switched {s} {} to Native mode. New chats will use Native.",
            plural(s)
        ),
        (0, d) if d > 0 => format!(
            "Native mode is on for new chats. {d} active {} will switch when they finish or reload.",
            plural(d)
        ),
        _ => "Native mode is on for new chats with Antigravity, Claude Code and Codex.".to_string(),
    }
}

impl GhostexGpuiApp {
    /// Opens the Native mode announcement modal once after updating, when nothing else is open.
    pub(crate) fn maybe_open_native_mode_intro(&mut self, cx: &mut Context<Self>) {
        if NATIVE_MODE_INTRO_OPENED.load(Ordering::Relaxed)
            || self.native_sidebar.snapshot.is_none()
            || self.native_app_modal_kind().is_some()
        {
            return;
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        if settings
            .object()
            .get(SETTINGS_INTRO_KEY)
            .and_then(Value::as_str)
            != Some("pending")
        {
            return;
        }
        NATIVE_MODE_INTRO_OPENED.store(true, Ordering::Relaxed);
        // Shown is answered: it never opens again, however this modal closes.
        self.write_native_mode_intro_settings(|settings| {
            settings.insert(SETTINGS_INTRO_KEY.to_string(), "done".into());
        });
        let config = NativeModeIntroModalConfig {
            palette: self.gpui_native_dialog_palette(),
        };
        let host: NativeModeIntroHost =
            self.native_app_modal_host(cx, |app, command, cx| match command {
                NativeModeIntroCommand::TurnOn => {
                    app.release_native_app_modal_window(GpuiAppModalKind::NativeModeIntro, cx);
                    app.turn_on_native_mode_from_intro(cx);
                }
                NativeModeIntroCommand::Dismiss => {
                    app.release_native_app_modal_window(GpuiAppModalKind::NativeModeIntro, cx);
                    cx.notify();
                }
            });
        self.open_native_mode_intro_window(config, host, cx);
    }

    /// `ghostex debug show-modal native-mode-intro`: opens the announcement modal as a safe
    /// preview without changing `nativeModeIntro`, `agentSessionMode`, or running sessions.
    #[cfg_attr(ghostex_web, allow(dead_code))]
    pub(crate) fn open_native_mode_intro_preview(&mut self, cx: &mut Context<Self>) {
        let config = NativeModeIntroModalConfig {
            palette: self.gpui_native_dialog_palette(),
        };
        let host: NativeModeIntroHost =
            self.native_app_modal_host(cx, |app, command, cx| match command {
                NativeModeIntroCommand::TurnOn | NativeModeIntroCommand::Dismiss => {
                    app.release_native_app_modal_window(GpuiAppModalKind::NativeModeIntro, cx);
                    cx.notify();
                }
            });
        self.open_native_mode_intro_window(config, host, cx);
    }

    fn open_native_mode_intro_window(
        &mut self,
        config: NativeModeIntroModalConfig,
        host: NativeModeIntroHost,
        cx: &mut Context<Self>,
    ) {
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                app.open_native_app_modal(
                    GpuiAppModalKind::NativeModeIntro,
                    NATIVE_MODE_INTRO_MODAL_WIDTH,
                    NATIVE_MODE_INTRO_MODAL_INITIAL_HEIGHT,
                    move |window, cx| {
                        cx.new(|cx| GpuiNativeModeIntroModalWindow::new(config, host, window, cx))
                    },
                    cx,
                );
            });
        });
    }

    fn turn_on_native_mode_from_intro(&mut self, cx: &mut Context<Self>) {
        self.write_native_mode_intro_settings(|settings| {
            let mut modes = settings
                .get("agentSessionMode")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for family in NATIVE_FAMILIES {
                modes.insert(family.to_string(), json!("non-cli"));
            }
            settings.insert("agentSessionMode".to_string(), Value::Object(modes));
        });
        cx.notify();
        let request = cx.background_executor().spawn(async move {
            crate::app::helpers::gpui_gxserver_rpc_result(
                "/api/switchEligibleSessionsToAcp",
                &json!({}),
                std::time::Duration::from_secs(60),
            )
        });
        cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |app, cx| match result {
                Ok(value) => {
                    let switched =
                        value.get("switched").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let deferred =
                        value.get("deferred").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let detail = format_bulk_switch_summary(switched, deferred);
                    app.dispatch_gpui_workspace_action_toast(
                        "success",
                        "Native mode is on",
                        &detail,
                        cx,
                    );
                }
                Err(error) => {
                    app.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Native mode is on for new chats",
                        &format!("Could not switch existing chats right now: {error}"),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn write_native_mode_intro_settings(
        &mut self,
        edit: impl FnOnce(&mut serde_json::Map<String, Value>),
    ) {
        let mut settings = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        edit(&mut settings);
        if let Err(error) = crate::shared_settings::write_shared_sidebar_settings_object(settings) {
            log::error!("Native mode intro: could not save settings: {error:?}");
        }
    }
}
