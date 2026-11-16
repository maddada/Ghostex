//! Every computer's icon as this app knows it: what each computer's gxserver says was picked for it
//! (server/src/computer_icon.rs), and the mark the sidebar draws for it (CDXC:RemoteMachines
//! 2026-11-12 in gx-core computer_icons.rs).
//!
//! This computer's is read when the app starts, a connected computer's each time its client starts,
//! and either is set from Settings > Remote. A computer that is away is drawn with the icon this app
//! last read there, kept on its saved machine (`remoteMachines[].icon`).
//!
//! CDXC:RemoteMachines 2026-11-14 WHY:
//! A gxserver older than the system logos (its answer has no `logo`) refuses a logo pick. The pick is then kept on this app's saved machine instead, and for such a computer that local pick wins over what it answers, so the user still sees the logo they chose; once that computer runs a newer Ghostex its own answer is the truth again.
//!
//! CDXC:RemoteMachines 2026-11-16 FIX:
//! User: the icon picked for "m7-arch" (Arch Linux, Omarchy) "keeps going back to the penguin icon". Three faults made a pick last only while the app kept that computer's answer in memory: (1) the copy kept on the saved machine was never written, because the settings save lets only `settings:remoteMachines` and `sidebar:remoteMachineOrder` change the machine list and this file saved as `settings:computerIcon` (remote_conn/settings_and_install_probe.rs), so a computer that was away, not read yet, or too old to keep its pick fell back to its system's logo, the penguin for a Linux computer whose distribution wasn't known yet; (2) a read that failed (a tunnel still coming up, a server restarting) was never tried again in that run, so the fallback stayed; (3) a pick kept here for a computer too old to keep it was dropped by that computer's first answer once it updated. The saved machine now keeps the pick (`icon`), whether it was kept here (`iconKeptHere`) and the logo last read there (`logo`); a failed read is tried again a few times; a pick kept here is handed to that computer once it can keep it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use ghostex_gx_core::LOCAL_MACHINE_ID;

/// The settings save source of a computer's icon kept on its saved machine, one of the few that may
/// change `remoteMachines` (remote_conn/settings_and_install_probe.rs).
pub(crate) const COMPUTER_ICON_SETTINGS_SOURCE: &str = "settings:computerIcon";

/// What one computer's gxserver answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComputerIconFacts {
    /// The picked icon's id, `None` when none was picked.
    pub(crate) icon: Option<String>,
    /// `laptop` or `desktop`, `None` when that computer cannot tell (or runs an older Ghostex).
    pub(crate) kind: Option<String>,
    /// `macos`, `windows` or `linux`.
    pub(crate) platform: Option<String>,
    /// The logo it wears unpicked (`apple`, `windows`, a Linux distribution's, `linux`); `None`
    /// from a gxserver older than the logos.
    pub(crate) logo: Option<String>,
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
            logo: text("logo"),
        }
    }

    /// Its gxserver knows the logo icons, so its pick is the truth.
    fn knows_logos(&self) -> bool {
        self.logo.is_some()
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

/// What this app keeps of a computer's icon on its saved machine (`remoteMachines[]`), to draw
/// while it is away or not read yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SavedComputerIcon {
    /// The icon last read there, or picked here.
    icon: Option<String>,
    /// `icon` was picked here while that computer's Ghostex couldn't keep it.
    kept_here: bool,
    /// The logo that computer last said it wears unpicked.
    logo: Option<String>,
}

fn saved_machine<'a>(
    settings: &'a serde_json::Map<String, Value>,
    machine_id: &str,
) -> Option<&'a Value> {
    settings
        .get("remoteMachines")
        .and_then(Value::as_array)?
        .iter()
        .find(|machine| machine.get("id").and_then(Value::as_str) == Some(machine_id))
}

fn saved_computer_icon(
    settings: &serde_json::Map<String, Value>,
    machine_id: &str,
) -> SavedComputerIcon {
    let Some(machine) = saved_machine(settings, machine_id) else {
        return SavedComputerIcon::default();
    };
    let text = |key: &str| machine.get(key).and_then(Value::as_str).map(str::to_string);
    SavedComputerIcon {
        icon: text("icon"),
        kept_here: machine.get("iconKeptHere") == Some(&Value::Bool(true)),
        logo: text("logo"),
    }
}

/// How long to wait before reading a computer's icon again after a read failed, by attempt; after
/// the last it is read again when its client next starts.
const READ_RETRY_DELAYS: [std::time::Duration; 5] = [
    std::time::Duration::from_secs(3),
    std::time::Duration::from_secs(10),
    std::time::Duration::from_secs(30),
    std::time::Duration::from_secs(60),
    std::time::Duration::from_secs(120),
];

fn read_failures() -> &'static Mutex<HashMap<String, usize>> {
    static FAILURES: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    FAILURES.get_or_init(Default::default)
}

/// Counts a failed read of that computer's icon and says how long to wait before the next, `None`
/// once the retries are spent.
fn note_read_failure(machine_id: &str) -> Option<std::time::Duration> {
    let mut failures = read_failures().lock().ok()?;
    let count = failures.entry(machine_id.to_string()).or_default();
    *count += 1;
    READ_RETRY_DELAYS.get(*count - 1).copied()
}

/// Its client started again (remote_clients.rs): its reads get their retries back.
pub(super) fn forget_computer_icon_read_failures(machine_id: &str) {
    if let Ok(mut failures) = read_failures().lock() {
        failures.remove(machine_id);
    }
}

/// This computer's own Linux distribution's logo, for when its gxserver hasn't answered yet.
fn this_linux_logo() -> Option<&'static str> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    static LOGO: OnceLock<&'static str> = OnceLock::new();
    Some(LOGO.get_or_init(|| {
        let release = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();
        ghostex_gx_core::computer_icons::linux_distribution_logo_from_os_release(&release)
    }))
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
    /// That computer's picked icon (`None` for its default) and the drawing it wears unpicked: what
    /// its gxserver answered, else (an older gxserver, or one not read yet) the pick kept on its
    /// saved machine.
    fn computer_icon_choice(
        machine_id: &str,
        settings: &serde_json::Map<String, Value>,
        systems: &HashMap<String, &'static str>,
    ) -> (Option<String>, &'static str) {
        let local = machine_id == LOCAL_MACHINE_ID;
        let facts = computer_icon_facts(machine_id);
        let saved = if local {
            SavedComputerIcon::default()
        } else {
            saved_computer_icon(settings, machine_id)
        };
        let picked = match &facts {
            // A pick kept here wins until that computer has taken it (`note_computer_icon`).
            Some(facts) if facts.knows_logos() && facts.icon.is_none() && saved.kept_here => {
                saved.icon.clone()
            }
            Some(facts) if facts.knows_logos() => facts.icon.clone(),
            Some(facts) => saved.icon.clone().or_else(|| facts.icon.clone()),
            None => saved.icon.clone(),
        };
        let logo = facts
            .as_ref()
            .and_then(|facts| facts.logo.clone())
            .or(saved.logo)
            .or_else(|| local.then(this_linux_logo).flatten().map(str::to_string));
        let kind = facts.as_ref().and_then(|facts| facts.kind.clone());
        let system = facts
            .as_ref()
            .and_then(|facts| facts.platform.clone())
            .or_else(|| {
                if local {
                    Some(this_system().to_string())
                } else {
                    systems.get(machine_id).map(|system| system.to_string())
                }
            });
        let default = ghostex_gx_core::computer_icons::computer_icon_asset(
            None,
            logo.as_deref(),
            kind.as_deref(),
            system.as_deref(),
        );
        (picked, default)
    }

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
                let name = if tab.machine_id == LOCAL_MACHINE_ID {
                    crate::host_name::local_computer_name()
                } else {
                    tab.label.clone()
                };
                let asset = Self::computer_asset(&tab.machine_id, settings.object(), &systems);
                (tab.machine_id.clone(), ComputerMark { asset, name })
            })
            .collect()
    }

    /// The drawing a computer wears: the icon picked for it, else its system's logo.
    fn computer_asset(
        machine_id: &str,
        settings: &serde_json::Map<String, Value>,
        systems: &HashMap<String, &'static str>,
    ) -> &'static str {
        let (picked, default) = Self::computer_icon_choice(machine_id, settings, systems);
        picked
            .as_deref()
            .and_then(ghostex_gx_core::computer_icons::computer_icon)
            .map_or(default, |icon| icon.asset)
    }

    /// The mark of the computer a Browser page loads from, whatever the sidebar lists: `None` is
    /// this computer (its real name), a Cloud Box wears the cloud, any other computer its mark.
    ///
    /// CDXC:Browser 2026-11-15 DECISION:
    /// User: "please just show the icon of the machine here, don't show this text. tooltip should
    /// say the machine's name". The address bar's route control draws this and nothing else.
    pub(crate) fn browser_source_mark(&self, machine_id: Option<&str>) -> ComputerMark {
        let machine_id = machine_id.unwrap_or(LOCAL_MACHINE_ID);
        if machine_id == LOCAL_MACHINE_ID {
            let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
            let systems = self.remote_machine_systems();
            return ComputerMark {
                asset: Self::computer_asset(machine_id, settings.object(), &systems),
                name: crate::host_name::local_computer_name(),
            };
        }
        if let Some(box_id) =
            ghostex_gx_core::protocol::cloud_box::cloud_box_id_from_machine_id(machine_id)
        {
            let name = self
                .gx_store
                .core
                .cloud_boxes()
                .iter()
                .find(|cloud_box| cloud_box.box_id == box_id)
                .map(|cloud_box| cloud_box.name.trim().to_string())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Cloud Box".to_string());
            return ComputerMark {
                asset: "titlebar/cloud.svg",
                name,
            };
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let systems = self.remote_machine_systems();
        ComputerMark {
            asset: Self::computer_asset(machine_id, settings.object(), &systems),
            name: crate::app::helpers::gpui_remote_machine_name_from_settings(machine_id)
                .unwrap_or_else(|| "Remote computer".to_string()),
        }
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
                if answer.is_ok() {
                    forget_computer_icon_read_failures(&machine_id);
                    let _ = this.update(cx, |this, cx| {
                        let picked_here = next.knows_logos();
                        this.note_computer_icon(&machine_id, next, picked_here, cx);
                    });
                    return;
                }
                // A read that failed (a tunnel still coming up, a server restarting) is tried
                // again a few times; until then the computer wears what its saved machine keeps.
                let Some(delay) = note_read_failure(&machine_id) else {
                    return;
                };
                cx.background_executor().timer(delay).await;
                let _ = this.update(cx, |this, cx| {
                    this.gx_store.remote.icons_asked.remove(&machine_id);
                    this.gx_store_ask_computer_icons(cx);
                });
            })
            .detach();
        }
    }

    /// Takes a computer's answer: keeps it, saves a remote one's pick and logo on its saved machine
    /// so they are drawn while that computer is away, and redraws. `authoritative` is false for an
    /// older gxserver's answer to a read, whose pick only fills a saved machine that keeps none.
    fn note_computer_icon(
        &mut self,
        machine_id: &str,
        next: ComputerIconFacts,
        authoritative: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let knows_logos = next.knows_logos();
        let answered_icon = next.icon.clone();
        let answered_logo = next.logo.clone();
        store_facts(machine_id, next);
        if machine_id != LOCAL_MACHINE_ID {
            let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
            let saved = saved_computer_icon(settings.object(), machine_id);
            let logo = answered_logo.or(saved.logo.clone());
            if knows_logos && answered_icon.is_none() && saved.kept_here && saved.icon.is_some() {
                // That computer can keep the pick made here while it couldn't: hand it over.
                self.save_remote_computer_icon(
                    machine_id,
                    saved.icon.as_deref(),
                    true,
                    logo.as_deref(),
                    cx,
                );
                self.hand_kept_computer_icon(machine_id.to_string(), saved.icon.unwrap(), cx);
            } else if authoritative || saved.icon.is_none() {
                self.save_remote_computer_icon(
                    machine_id,
                    answered_icon.as_deref(),
                    false,
                    logo.as_deref(),
                    cx,
                );
            } else {
                self.save_remote_computer_icon(
                    machine_id,
                    saved.icon.as_deref(),
                    saved.kept_here,
                    logo.as_deref(),
                    cx,
                );
            }
        }
        self.gx_store_sync_remote_clients(true, cx);
        cx.notify();
    }

    /// Saves the pick kept here on that computer, quietly: if it still can't keep it, the pick
    /// stays kept here.
    fn hand_kept_computer_icon(
        &mut self,
        machine_id: String,
        icon: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(remote) = self.gpui_remote_gxserver_request_target(&machine_id) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let answer = super::rpc::gx_rpc(
                Some(remote),
                "/api/setComputerIcon",
                json!({ "icon": &icon }),
            )
            .await;
            if let Ok(answer) = answer {
                let facts = ComputerIconFacts::from_answer(&answer);
                let _ = this.update(cx, |this, cx| {
                    this.note_computer_icon(&machine_id, facts, true, cx);
                    this.publish_computer_icons(cx);
                });
            }
        })
        .detach();
    }

    /// Writes `icon` (picked here when `kept_here`) and `logo` on that saved machine when they
    /// differ from what is there.
    fn save_remote_computer_icon(
        &mut self,
        machine_id: &str,
        icon: Option<&str>,
        kept_here: bool,
        logo: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let kept_here = kept_here && icon.is_some();
        let next = SavedComputerIcon {
            icon: icon.map(str::to_string),
            kept_here,
            logo: logo.map(str::to_string),
        };
        if saved_machine(settings.object(), machine_id).is_none()
            || saved_computer_icon(settings.object(), machine_id) == next
        {
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
                    let mut set = |key: &str, value: Option<Value>| match value {
                        Some(value) => {
                            fields.insert(key.to_string(), value);
                        }
                        None => {
                            fields.remove(key);
                        }
                    };
                    set("icon", icon.map(|icon| json!(icon)));
                    set("iconKeptHere", kept_here.then_some(Value::Bool(true)));
                    set("logo", logo.map(|logo| json!(logo)));
                }
                machine
            })
            .collect();
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({
                "type": "updateSettingsPatch",
                "patch": { "remoteMachines": machines },
                "source": COMPUTER_ICON_SETTINGS_SOURCE,
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
                super::rpc::gx_rpc(remote, "/api/setComputerIcon", json!({ "icon": &icon })).await;
            let _ = this.update(cx, |this, cx| match answer {
                Ok(answer) => {
                    let facts = ComputerIconFacts::from_answer(&answer);
                    if machine_id != LOCAL_MACHINE_ID {
                        // That computer kept this pick (or the default), so no pick kept here
                        // is handed back over it.
                        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
                        let logo = facts
                            .logo
                            .clone()
                            .or(saved_computer_icon(settings.object(), &machine_id).logo);
                        this.save_remote_computer_icon(
                            &machine_id,
                            facts.icon.as_deref(),
                            false,
                            logo.as_deref(),
                            cx,
                        );
                    }
                    this.note_computer_icon(&machine_id, facts, true, cx);
                    this.publish_computer_icons(cx);
                }
                // An older Ghostex there refuses the logos (or keeps no icon at all): the pick is
                // kept on this app's saved computer, which wins for such a computer.
                Err(_) if machine_id != LOCAL_MACHINE_ID => {
                    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
                    let logo = computer_icon_facts(&machine_id)
                        .and_then(|facts| facts.logo)
                        .or(saved_computer_icon(settings.object(), &machine_id).logo);
                    this.save_remote_computer_icon(
                        &machine_id,
                        Some(icon.as_str()).filter(|icon| !icon.is_empty()),
                        true,
                        logo.as_deref(),
                        cx,
                    );
                    this.gx_store_sync_remote_clients(true, cx);
                    this.publish_computer_icons(cx);
                    this.dispatch_gpui_workspace_action_toast(
                        "info",
                        "Icon kept on this device",
                        "That computer runs an older Ghostex that can't keep this icon. Update Ghostex there so every device shows it.",
                        cx,
                    );
                }
                Err(_) => this.dispatch_gpui_workspace_action_toast(
                    "error",
                    "Icon not changed",
                    "This computer's Ghostex server is older than the app. Restart Ghostex, then try again.",
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

    /// Hands an open Settings window the icon picked for every computer and the one each wears
    /// unpicked (its system's logo), for Settings > Remote's pickers and computer tiles.
    pub(crate) fn publish_computer_icons(&mut self, cx: &mut gpui::Context<Self>) {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let systems = self.remote_machine_systems();
        let mut machine_ids: Vec<String> = vec![LOCAL_MACHINE_ID.to_string()];
        machine_ids.extend(
            settings
                .object()
                .get("remoteMachines")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|machine| machine.get("id").and_then(Value::as_str))
                .map(str::to_string),
        );
        if let Ok(facts) = facts().lock() {
            machine_ids.extend(facts.keys().cloned());
        }
        machine_ids.sort();
        machine_ids.dedup();
        let mut picked = serde_json::Map::new();
        let mut defaults = serde_json::Map::new();
        for machine_id in machine_ids {
            let (icon, default) =
                Self::computer_icon_choice(&machine_id, settings.object(), &systems);
            picked.insert(machine_id.clone(), json!(icon));
            defaults.insert(machine_id, json!(default));
        }
        let default_local = defaults
            .get(LOCAL_MACHINE_ID)
            .cloned()
            .unwrap_or_else(|| json!("titlebar/device-desktop.svg"));
        self.dispatch_open_gpui_app_modal_message(
            json!({
                "type": "computerIcons",
                "picked": picked,
                "defaultAssets": defaults,
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
