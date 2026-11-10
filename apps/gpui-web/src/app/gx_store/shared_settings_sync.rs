//! The page's copy of this computer's settings: read from gxserver when the event stream opens and
//! again on each `sharedSettingsChanged` frame (server/src/server/shared_settings_watch.rs), then
//! handed to what the desktop refreshes after its own settings poll
//! (sidebar_dispatch/runtime_settings_and_host_messages.rs): the theme colours, the fonts, the HUD
//! and every window. The chats pick the change up on their host's own settings pass (gx_chat/world.rs).
use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;

const READ_SHARED_SETTINGS_PATH: &str = "/api/readSharedSettings";
/// A failed read waits this long before its second try, twice as long before each later one, and
/// never longer than the cap.
const SETTINGS_READ_RETRY_FIRST_MS: u64 = 500;
const SETTINGS_READ_RETRY_MAX_MS: u64 = 15_000;
pub(super) const SHARED_SETTINGS_CHANGED_FRAME: &str = "\"type\":\"sharedSettingsChanged\"";

impl GhostexGpuiApp {
    pub(super) fn gx_store_read_shared_settings(&mut self, cx: &mut gpui::Context<Self>) {
        let read = crate::shared_settings::begin_saved_read();
        cx.spawn(async move |app, cx| {
            let mut attempt: u32 = 0;
            let settings = loop {
                match super::gx_rpc(None, READ_SHARED_SETTINGS_PATH, json!({})).await {
                    Ok(result) => match result.get("settings") {
                        Some(Value::Object(settings)) => break settings.clone(),
                        _ => break Map::new(),
                    },
                    // An older gxserver has no settings read (`notFound`), a client it does not
                    // serve settings to is refused (`badRequest`), and so are a token or a
                    // protocol it does not take: asking again changes nothing, and the page keeps
                    // the settings it has.
                    Err(error)
                        if matches!(
                            error.code.as_deref(),
                            Some(
                                "notFound"
                                    | "badRequest"
                                    | "unauthorized"
                                    | "forbidden"
                                    | "protocolMismatch"
                                    | "methodNotAllowed"
                            )
                        ) =>
                    {
                        log::warn!("could not read the settings: {}", error.message);
                        return;
                    }
                    // A refused file read or a dropped request: tried again, since the change
                    // frame that started this read is not sent twice. A later read takes over.
                    Err(error) => {
                        log::warn!(
                            "could not read the settings, trying again: {}",
                            error.message
                        );
                        // 0.5s, 1s, 2s, 4s, 8s, then 15s.
                        let delay = SETTINGS_READ_RETRY_FIRST_MS
                            .saturating_mul(1 << attempt.min(5))
                            .min(SETTINGS_READ_RETRY_MAX_MS);
                        attempt += 1;
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(delay))
                            .await;
                        if !crate::shared_settings::is_latest_saved_read(read) {
                            return;
                        }
                    }
                }
            };
            let _ = app.update(cx, |app, cx| {
                if crate::shared_settings::install_saved(read, settings) {
                    app.web_shared_settings_changed(cx);
                }
            });
        })
        .detach();
    }

    fn web_shared_settings_changed(&mut self, cx: &mut gpui::Context<Self>) {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        crate::app::helpers::apply_settings_theme_colours();
        crate::app::helpers::apply_gpui_component_theme(cx);
        cx.global_mut::<gpui_component::Theme>().mono_font_family = "JetBrains Mono".into();
        gpui_component::Theme::sync_base(cx);
        crate::ui_fonts::apply_fonts(settings.object(), cx);
        self.sidebar_runtime_settings_snapshot = crate::cef::SidebarRuntimeSettingsSnapshot {
            debugging_mode: settings.debugging_mode(),
            saved_settings_json: serde_json::to_string(settings.object()).unwrap_or_default(),
            ..self.sidebar_runtime_settings_snapshot.clone()
        };
        self.gx_store_hud_settings_changed(cx);
        cx.refresh_windows();
    }
}
