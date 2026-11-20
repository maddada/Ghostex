//! Screenshot mode: one switch that hides what is private before a screenshot or a screen share
//! (account names and emails, the sidebar's account tiles, the model pill's account mark, and the
//! other workspaces), with a small indicator in the window while it is on.
//!
//! CDXC:ScreenshotMode 2026-11-20 DECISION:
//! User (picked on the 11.12.0 smoke improvements page): "One switch that hides account names, tiles and other workspaces would make post shots safe by default". Off by default, in Settings > Accounts and the command palette (and `ghostex settings`), never synced to other computers (it is in no Settings > Sync category), and clearly shown as on so it is not left on by mistake. It turns on everything Hide emails does, so every place that masks emails reads `account_emails_hidden` here (gxserver reads the same two keys, `session_chat_notice::account_emails_hidden`).
//! SEE-ALSO: packages/gx-chat-core/src/event.rs (`ChatSettings::screenshot_mode`), apps/desktop/src/app/native_sidebar/workspace_tile.rs, apps/desktop/src/app/gx_store/workspaces.rs (`gx_store_workspace_menu`).

use serde_json::Value;

/// The settings key.
pub(crate) const SCREENSHOT_MODE_KEY: &str = "screenshotMode";

/// Whether screenshot mode is on.
pub(crate) fn screenshot_mode_on() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get(SCREENSHOT_MODE_KEY)
        .and_then(Value::as_bool)
        == Some(true)
}

/// Whether account emails are masked: Hide emails, or screenshot mode.
pub(crate) fn account_emails_hidden() -> bool {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let object = settings.object();
    let on = |key: &str| object.get(key).and_then(Value::as_bool) == Some(true);
    on("hideAccountEmails") || on(SCREENSHOT_MODE_KEY)
}

impl crate::GhostexGpuiApp {
    /// The command palette's Toggle Screenshot Mode: saves the opposite of what is on now. The web
    /// build's palette does not run it (a page saves no settings).
    #[cfg_attr(ghostex_web, allow(dead_code))]
    pub(crate) fn toggle_screenshot_mode(&mut self, cx: &mut gpui::Context<Self>) {
        self.handle_gpui_app_modal_update_settings_patch_message(
            &serde_json::json!({
                "patch": { SCREENSHOT_MODE_KEY: !screenshot_mode_on() },
                "source": "commandPalette:screenshotMode",
                "type": "updateSettingsPatch",
            }),
            cx,
        );
    }
}
