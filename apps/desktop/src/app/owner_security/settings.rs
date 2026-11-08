//! The Settings window's saves that loosen security wait for the system prompt (Touch ID or the
//! Mac password, Windows Hello, the polkit password) before they are written, like the same change
//! made through gxserver's API waits for a signed approval.

use std::cell::{Cell, RefCell};

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

/*
CDXC:RemoteMachines 2026-11-08 WHY:
User, on 11.1.1 (Windows): "after the new update i cant connect with remote. it requires windows hello pin, but then SSH says to give it the actual password (which i have) but it doesnt progress further". The machine dialog's Save sends the machine (a Remote setting, so it waits for Windows Hello since #495) and then the password typed beside it (`saveRemoteMachinePassword`). The password went on at once, found no such machine yet (or had its `sshPasswordSaved` mark written over by the machine once Hello was answered), and was never kept, so the connect logged in without it and the machine refused it. A password sent while a Settings save waits for the prompt now waits behind that save: it is kept once the save is written, and dropped with it when the prompt is declined. One prompt, the password typed once.
*/
/// A Settings save waiting for the system prompt, and the password saves sent after it.
struct HeldSave {
    id: u64,
    password_saves: Vec<Map<String, Value>>,
}

thread_local! {
    /// The saves waiting for the prompt, oldest first (the app's main thread only).
    static HELD_SAVES: RefCell<Vec<HeldSave>> = const { RefCell::new(Vec::new()) };
    static NEXT_HELD_SAVE: Cell<u64> = const { Cell::new(0) };
}

/// Puts a `saveRemoteMachinePassword` from the Settings window behind the newest save waiting for
/// the prompt; `false` when nothing waits (or it is another message), and it goes on at once.
pub(crate) fn wait_behind_held_save(message: &Value) -> bool {
    if message.get("type").and_then(Value::as_str) != Some("saveRemoteMachinePassword") {
        return false;
    }
    let Some(command) = message.as_object() else {
        return false;
    };
    HELD_SAVES.with_borrow_mut(|held| match held.last_mut() {
        Some(save) => {
            save.password_saves.push(command.clone());
            true
        }
        None => false,
    })
}

/// The password saves that waited on save `id`, which the prompt has now answered.
fn release_held_save(id: u64) -> Vec<Map<String, Value>> {
    HELD_SAVES.with_borrow_mut(|held| {
        held.iter()
            .position(|save| save.id == id)
            .map(|index| held.remove(index).password_saves)
            .unwrap_or_default()
    })
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
        let held_id = NEXT_HELD_SAVE.with(|next| next.replace(next.get() + 1));
        HELD_SAVES.with_borrow_mut(|saves| {
            saves.push(HeldSave {
                id: held_id,
                password_saves: Vec::new(),
            })
        });
        let confirmed = cx
            .background_executor()
            .spawn(async move { ghostex_owner_keys::confirm_presence(&reason) });
        cx.spawn(async move |this, cx| {
            let answer = confirmed.await;
            let password_saves = release_held_save(held_id);
            let _ = this.update(cx, |this, cx| match answer {
                Ok(_) => {
                    this.save_gpui_settings_modal_command(command, cx);
                    // The machine is written now, so its password finds it and keeps its mark.
                    for password_save in password_saves {
                        this.handle_gpui_save_remote_machine_password_message(&password_save, cx);
                    }
                }
                Err(error) => {
                    // The machine wasn't saved: neither is the password typed with it.
                    drop(password_saves);
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
