//! The Settings window's saves that loosen security wait for the system prompt (Touch ID or the
//! Mac password, Windows Hello, the polkit password) before they are written, like the same change
//! made through gxserver's API waits for a signed approval.

use serde_json::{Map, Value};

use crate::GhostexGpuiApp;
use crate::app::model::GpuiAppModalKind;
use crate::app::window::settings_modal::catalog::settings_catalog;
use crate::app::window::settings_modal::{GpuiSettingsModalWindow, SettingsModalCommand};
use ghostex_settings_catalog::security_relaxing::needs_owner_presence;

/// The saved value of every setting in `changes` that loosens security and would change, keyed
/// by setting: what to show again when the owner doesn't confirm.
fn held_back(changes: &Map<String, Value>) -> Map<String, Value> {
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let saved = snapshot.object();
    changes
        .iter()
        .filter(|(key, value)| needs_owner_presence(key, value))
        .filter_map(|(key, value)| {
            let current = saved
                .get(key)
                .cloned()
                .or_else(|| settings_catalog().default_value(key).cloned());
            (current.as_ref() != Some(value)).then(|| (key.clone(), current.unwrap_or(Value::Null)))
        })
        .collect()
}

/// The prompt's line: "Change Easy Connect and Sync coordinator playbooks in Settings".
fn reason(keys: &[&String]) -> String {
    let catalog = ghostex_settings_catalog::help::catalog();
    let titles: Vec<String> = keys
        .iter()
        .map(|key| {
            catalog
                .settings
                .iter()
                .find(|entry| &entry.key == *key)
                .map(|entry| entry.title.clone())
                .unwrap_or_else(|| key.to_string())
        })
        .collect();
    format!("change {} in Ghostex Settings", titles.join(", "))
}

impl GhostexGpuiApp {
    /*
    CDXC:Security 2026-11-08 DECISION:
    User (#495 follow-up): the desktop's own Settings window must ask for the system prompt (Touch ID / Hello / polkit) when it saves a security-relaxing setting, same as the API path, because agents can drive the GUI. The rule is the API's (`ghostex_settings_catalog::security_relaxing`); a save the owner doesn't confirm is not written and the window shows the saved values again.
    */
    /// `Some(command)` to save now; `None` when the save waits for the prompt (and is saved, or
    /// put back, once it is answered).
    pub(crate) fn owner_security_hold_settings_save(
        &mut self,
        kind: GpuiAppModalKind,
        command: SettingsModalCommand,
        cx: &mut gpui::Context<Self>,
    ) -> Option<SettingsModalCommand> {
        let held = match &command {
            SettingsModalCommand::SavePatch { patch, .. } => held_back(patch),
            SettingsModalCommand::SaveSettings { settings, .. } => held_back(settings),
            _ => return Some(command),
        };
        if held.is_empty() {
            return Some(command);
        }
        let mut keys: Vec<&String> = held.keys().collect();
        keys.sort();
        let reason = reason(&keys);
        let confirmed = cx
            .background_executor()
            .spawn(async move { ghostex_owner_keys::confirm_presence(&reason) });
        cx.spawn(async move |this, cx| {
            let answer = confirmed.await;
            let _ = this.update(cx, |this, cx| match answer {
                Ok(_) => this.save_gpui_settings_modal_command(command, cx),
                Err(error) => {
                    this.update_native_app_modal(
                        kind,
                        cx,
                        move |modal: &mut GpuiSettingsModalWindow, _window, cx| {
                            modal.revert_settings(held, cx);
                        },
                    );
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Setting not changed",
                        &error.to_string(),
                        cx,
                    );
                }
            });
        })
        .detach();
        None
    }
}
