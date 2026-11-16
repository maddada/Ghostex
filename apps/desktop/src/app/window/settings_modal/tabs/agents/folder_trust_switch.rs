//! Settings > Agents > Defaults: Trust project folders automatically. Turning it on asks for the
//! system's own prompt on this computer first (Touch ID or the password, Windows Hello or the
//! Windows password, polkit), the same `ghostex_owner_keys::confirm_presence` Settings already uses
//! before Hex's installer; turning it off never asks.
//!
//! CDXC:AgentProviders 2026-11-14 DECISION: User: the switch "can't be enabled without confirmation with pin/fingerprint etc on the machine i'm on when i change that setting". The confirmed time is saved beside the switch, and only both together count as on (packages/settings-catalog/src/folder_trust.rs); gxserver's API refuses every write that would turn it on, so the phone, another computer, the CLI and agents can only turn it off.
//! SEE-ALSO: server/src/settings_agent_writable.rs, server/src/agent_folder_trust/launch.rs.

use super::super::super::fields::{RowSpec, toggle_field_with};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::SettingsValues;
use super::AgentsTab;
use ghostex_settings_catalog::folder_trust::{
    TRUST_PROJECT_FOLDERS_CONFIRMED_SETTING, TRUST_PROJECT_FOLDERS_SETTING,
    trust_project_folders_on,
};
use gpui::{AnyElement, Context};
use serde_json::{Map, Value, json};

/// What the system prompt says Ghostex is trying to do ("Ghostex is trying to …" on macOS).
const PRESENCE_REASON: &str = "turn on Trust project folders automatically, so agents work in your project folders without asking first";

const DESCRIPTION: &str = "Mark each project and worktree folder as trusted before its agent starts, so no session stops at a \"Do you trust this folder?\" question. Off by default: the chat shows that question as a card with Trust folder and Don't trust. Turning it on asks for Touch ID, Windows Hello or your password on this computer.";

/// Where the switch stands while it is being turned on.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) enum FolderTrustSwitch {
    #[default]
    Idle,
    /// The system prompt is up.
    Asking,
    /// The last try did not turn it on, and why.
    NotTurnedOn(String),
}

fn patch(on: bool) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert(TRUST_PROJECT_FOLDERS_SETTING.to_string(), json!(on));
    patch.insert(
        TRUST_PROJECT_FOLDERS_CONFIRMED_SETTING.to_string(),
        json!(if on {
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        } else {
            String::new()
        }),
    );
    patch
}

impl AgentsTab {
    pub(super) fn folder_trust_row(
        &mut self,
        p: &SettingsPalette,
        values: &SettingsValues,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = trust_project_folders_on(values.map());
        let description = match &self.folder_trust_switch {
            FolderTrustSwitch::Idle => DESCRIPTION.to_string(),
            FolderTrustSwitch::Asking => {
                "Confirm with Touch ID, Windows Hello or your password to turn it on.".to_string()
            }
            FolderTrustSwitch::NotTurnedOn(why) => format!("{DESCRIPTION} Not turned on: {why}"),
        };
        toggle_field_with(
            p,
            TRUST_PROJECT_FOLDERS_SETTING,
            RowSpec::new("Trust project folders automatically").description(description),
            on || self.folder_trust_switch == FolderTrustSwitch::Asking,
            None,
            |page: &mut Self, next, _window, cx| page.set_folder_trust(next, cx),
            cx,
        )
    }

    fn set_folder_trust(&mut self, next: bool, cx: &mut Context<Self>) {
        if !next {
            self.folder_trust_switch = FolderTrustSwitch::Idle;
            self.store.update(cx, |store, cx| {
                store.apply_patch(patch(false), "settings:control", cx)
            });
            cx.notify();
            return;
        }
        if self.folder_trust_switch == FolderTrustSwitch::Asking {
            return;
        }
        self.folder_trust_switch = FolderTrustSwitch::Asking;
        cx.notify();
        let background = cx.background_executor().clone();
        cx.spawn(async move |page, cx| {
            let presence = background
                .spawn(async move { ghostex_owner_keys::confirm_presence(PRESENCE_REASON) })
                .await;
            let _ = page.update(cx, |page, cx| {
                match presence {
                    Ok(_) => {
                        page.folder_trust_switch = FolderTrustSwitch::Idle;
                        page.store.update(cx, |store, cx| {
                            store.apply_patch(patch(true), "settings:control", cx)
                        });
                    }
                    Err(error) => {
                        page.folder_trust_switch = FolderTrustSwitch::NotTurnedOn(match error {
                            ghostex_owner_keys::KeyError::Cancelled => {
                                "the confirmation was cancelled.".to_string()
                            }
                            other => format!("this computer could not confirm it's you ({other})."),
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
