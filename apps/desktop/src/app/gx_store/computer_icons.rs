//! Every computer's icon as this app knows it: what each computer's gxserver says was picked for it
//! (server/src/computer_icon.rs), and the mark the sidebar draws for it (CDXC:RemoteMachines
//! 2026-11-12 in gx-core computer_icons.rs).
//!
//! This computer's is read when the app starts, a connected computer's each time its client starts,
//! and either is set from Settings > Remote. A computer that is away is drawn with the icon this app
//! last read there, kept on its saved machine (`remoteMachines[].icon`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use ghostex_gx_core::LOCAL_MACHINE_ID;

/// What one computer's gxserver answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComputerIconFacts {
    /// The picked icon's id, `None` when none was picked.
    pub(crate) icon: Option<String>,
    /// `laptop` or `desktop`, `None` when that computer cannot tell (or runs an older Ghostex).
    pub(crate) kind: Option<String>,
    /// `macos`, `windows` or `linux`.
    pub(crate) platform: Option<String>,
}

impl ComputerIconFacts {
    fn from_answer(answer: &Value) -> Self {
        let text = |key: &str| {
            answer
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        Self {
            icon: text("icon"),
            kind: text("kind"),
            platform: text("platform"),
        }
    }
}

/// A computer's mark in the sidebar: its icon's drawing and its full name for the tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComputerMark {
    pub(crate) asset: &'static str,
    pub(crate) name: String,
}

fn facts() -> &'static Mutex<HashMap<String, ComputerIconFacts>> {
    static FACTS: OnceLock<Mutex<HashMap<String, ComputerIconFacts>>> = OnceLock::new();
    FACTS.get_or_init(Default::default)
}

/// What that computer's gxserver last answered, when it has.
pub(crate) fn computer_icon_facts(machine_id: &str) -> Option<ComputerIconFacts> {
    facts().lock().ok()?.get(machine_id).cloned()
}

fn store_facts(machine_id: &str, next: ComputerIconFacts) {
    if let Ok(mut facts) = facts().lock() {
        facts.insert(machine_id.to_string(), next);
    }
}

/// The icon this app last read on a saved computer, kept to draw while it is away.
fn saved_icon(settings: &serde_json::Map<String, Value>, machine_id: &str) -> Option<String> {
    settings
        .get("remoteMachines")
        .and_then(Value::as_array)?
        .iter()
        .find(|machine| machine.get("id").and_then(Value::as_str) == Some(machine_id))?
        .get("icon")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// This computer's system as its own build knows it.
fn this_system() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

impl GhostexGpuiApp {
    /// Every computer tab's mark, by machine id; empty with only this computer, which draws none.
    pub(super) fn gx_store_computer_marks(
        &self,
        tabs: &[ghostex_gx_core::MachineTabInput],
    ) -> HashMap<String, ComputerMark> {
        if tabs.len() <= 1 {
            return HashMap::new();
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let systems = self.remote_machine_systems();
        tabs.iter()
            .map(|tab| {
                let local = tab.machine_id == LOCAL_MACHINE_ID;
                let facts = computer_icon_facts(&tab.machine_id);
                let picked = facts
                    .as_ref()
                    .map(|facts| facts.icon.clone())
                    .unwrap_or_else(|| {
                        (!local)
                            .then(|| saved_icon(settings.object(), &tab.machine_id))
                            .flatten()
                    });
                let kind = facts.as_ref().and_then(|facts| facts.kind.clone());
                let system = facts
                    .as_ref()
                    .and_then(|facts| facts.platform.clone())
                    .or_else(|| {
                        if local {
                            Some(this_system().to_string())
                        } else {
                            systems
                                .get(&tab.machine_id)
                                .map(|system| system.to_string())
                        }
                    });
                let name = if local {
                    crate::host_name::local_computer_name()
                } else {
                    tab.label.clone()
                };
                (
                    tab.machine_id.clone(),
                    ComputerMark {
                        asset: ghostex_gx_core::computer_icons::computer_icon_asset(
                            picked.as_deref(),
                            kind.as_deref(),
                            system.as_deref(),
                        ),
                        name,
                    },
                )
            })
            .collect()
    }

    /// Reads the icon of this computer and of every connected computer not read since its client
    /// started.
    pub(super) fn gx_store_ask_computer_icons(&mut self, cx: &mut gpui::Context<Self>) {
        let wanted: Vec<String> = std::iter::once(LOCAL_MACHINE_ID.to_string())
            .chain(self.remote_gxserver_connections.keys().cloned())
            .filter(|machine_id| !self.gx_store.remote.icons_asked.contains(machine_id))
            .collect();
        for machine_id in wanted {
            let remote = if machine_id == LOCAL_MACHINE_ID {
                None
            } else {
                match self.gpui_remote_gxserver_request_target(&machine_id) {
                    Some(target) => Some(target),
                    None => continue,
                }
            };
            self.gx_store.remote.icons_asked.insert(machine_id.clone());
            cx.spawn(async move |this, cx| {
                let answer = super::rpc::gx_rpc(remote, "/api/computerIcon", json!({})).await;
                // A computer whose Ghostex predates the icon answers nothing; it keeps the icon
                // last read there, else its system's mark.
                let next = answer
                    .as_ref()
                    .map(ComputerIconFacts::from_answer)
                    .unwrap_or_default();
                let _ = this.update(cx, |this, cx| {
                    if answer.is_ok() {
                        this.note_computer_icon(&machine_id, next, cx);
                    }
                });
            })
            .detach();
        }
    }

    /// Takes a computer's answer: keeps it, saves a remote one's pick on its saved machine so it is
    /// drawn while that computer is away, and redraws.
    fn note_computer_icon(
        &mut self,
        machine_id: &str,
        next: ComputerIconFacts,
        cx: &mut gpui::Context<Self>,
    ) {
        let picked = next.icon.clone();
        store_facts(machine_id, next);
        if machine_id != LOCAL_MACHINE_ID {
            self.save_remote_computer_icon(machine_id, picked.as_deref(), cx);
        }
        self.gx_store_sync_remote_clients(true, cx);
        cx.notify();
    }

    /// Writes `icon` on that saved machine when it differs from what is there.
    fn save_remote_computer_icon(
        &mut self,
        machine_id: &str,
        icon: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        if saved_icon(settings.object(), machine_id).as_deref() == icon {
            return;
        }
        let Some(machines) = settings
            .object()
            .get("remoteMachines")
            .and_then(Value::as_array)
        else {
            return;
        };
        let machines: Vec<Value> = machines
            .iter()
            .map(|machine| {
                let mut machine = machine.clone();
                if machine.get("id").and_then(Value::as_str) == Some(machine_id)
                    && let Some(fields) = machine.as_object_mut()
                {
                    match icon {
                        Some(icon) => {
                            fields.insert("icon".to_string(), json!(icon));
                        }
                        None => {
                            fields.remove("icon");
                        }
                    }
                }
                machine
            })
            .collect();
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({
                "type": "updateSettingsPatch",
                "patch": { "remoteMachines": machines },
                "source": "settings:computerIcon",
            }),
            cx,
        );
    }

    /// Settings > Remote picked `icon` (an id, or empty to go back to the default) for that
    /// computer: it is saved there, by that computer's gxserver.
    pub(crate) fn set_computer_icon(
        &mut self,
        machine_id: String,
        icon: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote = if machine_id == LOCAL_MACHINE_ID {
            None
        } else {
            match self.gpui_remote_gxserver_request_target(&machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Icon not changed",
                        "Connect to that computer first, then pick its icon again.",
                        cx,
                    );
                    return;
                }
            }
        };
        cx.spawn(async move |this, cx| {
            let answer =
                super::rpc::gx_rpc(remote, "/api/setComputerIcon", json!({ "icon": icon })).await;
            let _ = this.update(cx, |this, cx| match answer {
                Ok(answer) => {
                    this.note_computer_icon(&machine_id, ComputerIconFacts::from_answer(&answer), cx);
                    this.publish_computer_icons(cx);
                }
                Err(_) => this.dispatch_gpui_workspace_action_toast(
                    "error",
                    "Icon not changed",
                    "That computer's Ghostex is too old to keep an icon. Update it there, then try again.",
                    cx,
                ),
            });
        })
        .detach();
    }

    /// `computerIcon` from Settings > Remote: `op` `read` (send every computer's pick) or `set`
    /// (`machineId`, `icon`, empty for the default).
    pub(crate) fn handle_computer_icon_command(
        &mut self,
        command: &serde_json::Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if command.get("op").and_then(Value::as_str) == Some("set")
            && let Some(machine_id) = command.get("machineId").and_then(Value::as_str)
        {
            let icon = command
                .get("icon")
                .and_then(Value::as_str)
                .unwrap_or_default();
            self.set_computer_icon(machine_id.to_string(), icon.to_string(), cx);
            return;
        }
        self.publish_computer_icons(cx);
    }

    /// Hands an open Settings window the icon every computer wears and which one was picked
    /// there, for Settings > Remote's pickers.
    pub(crate) fn publish_computer_icons(&mut self, cx: &mut gpui::Context<Self>) {
        let picked: serde_json::Map<String, Value> = facts()
            .lock()
            .map(|facts| {
                facts
                    .iter()
                    .map(|(machine_id, facts)| (machine_id.clone(), json!(facts.icon)))
                    .collect()
            })
            .unwrap_or_default();
        let local = computer_icon_facts(LOCAL_MACHINE_ID).unwrap_or_default();
        let default_local = ghostex_gx_core::computer_icons::computer_icon_asset(
            None,
            local.kind.as_deref(),
            Some(local.platform.as_deref().unwrap_or(this_system())),
        );
        self.dispatch_open_gpui_app_modal_message(
            json!({
                "type": "computerIcons",
                "picked": picked,
                "localDefaultAsset": default_local,
                "connected": self.remote_gxserver_connections.keys().collect::<Vec<_>>(),
            }),
            cx,
        );
    }

    /// The mark of the computer a sidebar row runs on, while more than one computer is in the
    /// sidebar: its icon's drawing and the computer's full name.
    pub(crate) fn sidebar_computer_mark(&self, session_id: &str) -> Option<(&'static str, &str)> {
        let marks = &self.gx_store.remote.marks;
        if marks.is_empty() {
            return None;
        }
        let key = ghostex_gx_core::SessionKey::parse_sidebar_session_id(session_id)?;
        marks
            .get(key.machine.remote_id().unwrap_or(LOCAL_MACHINE_ID))
            .map(|mark| (mark.asset, mark.name.as_str()))
    }

    /// The icon a notification from that computer ends its title with: the sidebar's rule
    /// (native_sidebar/computer_mark.rs), so every computer's, this one's included, while more than
    /// one is in the sidebar, and none with only this computer. A remote computer without a mark
    /// keeps the plain computer icon.
    pub(crate) fn notification_computer_icon(
        &self,
        machine: &ghostex_gx_core::MachineId,
    ) -> Option<&'static str> {
        let marks = &self.gx_store.remote.marks;
        match marks.get(machine.remote_id().unwrap_or(LOCAL_MACHINE_ID)) {
            Some(mark) => Some(mark.asset),
            None => (!machine.is_local()).then_some("titlebar/device-desktop.svg"),
        }
    }

    /// The icon a computer tile in the sidebar's top row draws.
    pub(crate) fn sidebar_machine_icon_asset(&self, machine_id: &str) -> Option<&'static str> {
        self.gx_store
            .remote
            .marks
            .get(machine_id)
            .map(|mark| mark.asset)
    }
}
