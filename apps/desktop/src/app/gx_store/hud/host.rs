//! The HUD's sources and its composition on the desktop.
//!
//! What moves the HUD, and what this file does about it:
//! - this computer's stream goes live (`Effect::MachineLive`): read the HUD and the recent
//!   projects, as the runtime's `startFromBootstrap` and its stream recovery did;
//! - `globalSidebarCommandsChanged` (`Effect::RefetchSidebarHud`): read the HUD;
//! - a project's domain row arrives or a project is removed (`Effect::DomainProjectChanged`): read
//!   the HUD, and the recent projects when the project is or was parked
//!   (`applyDomainProjectDelta`);
//! - a remote machine's stream goes live or one of its projects changes: read that machine's HUD;
//! - Settings saved (`gx_store_hud_settings_changed`), the runtime's facts rows (the remote
//!   machines' client-parked projects, still the runtime's to write), and any store change the HUD
//!   reads (the Spaces and collections documents, the projects, the active project).
//!
//! Every composition that produces a different document replaces the HUD every reader takes
//! (`runtime_facts.hud`, bumping its generation) and refreshes the tab strip's Global Actions.
//!
//! CDXC:Sidebar 2026-09-25 WHY:
//! The runtime composed this and posted it on the facts channel; the app runtime port (family F2)
//! composes it here. The runtime's other HUD readers (a sidebar Action run, Generate Name, Quick
//! Access) moved to Rust too, and the runtime itself was deleted with QuickJS on 2026-09-25.
//!
//! SEE-ALSO: packages/gx-core/src/hud/, apps/desktop/src/app/gx_store/runtime_facts.rs.

use std::sync::Arc;

use ghostex_gx_core::hud::{HudSources, compose_sidebar_hud};
use ghostex_gx_core::{ChangeSummary, Effect, MachineId};
use serde_json::{Value, json};

use super::reads::{read_local_sidebar_hud, read_recent_projects, read_remote_sidebar_hud};
use crate::GhostexGpuiApp;
use crate::app::consts::{
    GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE, GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION,
    GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS,
};
use crate::app::helpers::gpui_remote_project_reference_from_project_id;

/// The HUD's sources, as last read.
#[derive(Default)]
pub(crate) struct HudHost {
    sources: HudSources,
    /// The Global Actions payload last handed to the tab strip.
    posted_global_actions: Option<String>,
    pub(crate) counters: HudCounters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HudCounters {
    pub(crate) composes: u64,
    pub(crate) changes: u64,
    pub(crate) local_reads: u64,
    pub(crate) remote_reads: u64,
    pub(crate) recent_reads: u64,
    pub(crate) failed_reads: u64,
}

impl GhostexGpuiApp {
    /// A HUD effect of the store (effects.rs hands them over).
    pub(in crate::app::gx_store) fn gx_store_perform_hud_effect(
        &mut self,
        effect: Effect,
        cx: &mut gpui::Context<Self>,
    ) {
        match effect {
            Effect::MachineLive { machine } => match machine.remote_id() {
                None => {
                    self.gx_store_read_local_sidebar_hud(cx);
                    self.gx_store_read_recent_projects(cx);
                }
                Some(machine_id) => self.gx_store_read_remote_sidebar_hud(machine_id, cx),
            },
            Effect::RefetchSidebarHud {
                machine: MachineId::Local,
            } => self.gx_store_read_local_sidebar_hud(cx),
            Effect::DomainProjectChanged {
                machine,
                project_id,
                is_recent_project,
                removed,
            } => match machine.remote_id() {
                None => {
                    // `applyDomainProjectDelta`: the recent projects only when this project is or
                    // was one of them, the HUD always.
                    let listed = self.gx_store.hud.sources.recent_projects.iter().any(|row| {
                        row.get("projectId").and_then(Value::as_str) == Some(project_id.as_str())
                    });
                    if is_recent_project || removed || listed {
                        self.gx_store_read_recent_projects(cx);
                    }
                    self.gx_store_read_local_sidebar_hud(cx);
                }
                Some(machine_id) => self.gx_store_read_remote_sidebar_hud(machine_id, cx),
            },
            _ => {}
        }
    }

    fn gx_store_read_local_sidebar_hud(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.hud.counters.local_reads += 1;
        let active_project_id = self
            .gx_store
            .core
            .focus()
            .active_project
            .as_ref()
            .filter(|project| project.machine.is_local())
            .map(|project| project.project_id.clone());
        cx.spawn(async move |this, cx| {
            let hud = read_local_sidebar_hud(active_project_id).await;
            let _ = this.update(cx, |this, cx| match hud {
                // A failed read keeps the previous HUD (`CDXC:AgentLauncher 2026-06-24-20:34`).
                None => this.gx_store.hud.counters.failed_reads += 1,
                Some(hud) => {
                    this.gx_store.hud.sources.sidebar_hud = Some(hud);
                    this.gx_store_hud_sources_changed(cx);
                }
            });
        })
        .detach();
    }

    fn gx_store_read_recent_projects(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.hud.counters.recent_reads += 1;
        cx.spawn(async move |this, cx| {
            let rows = read_recent_projects().await;
            let _ = this.update(cx, |this, cx| match rows {
                None => this.gx_store.hud.counters.failed_reads += 1,
                Some(rows) => {
                    this.gx_store.hud.sources.recent_projects = rows;
                    this.gx_store_hud_sources_changed(cx);
                }
            });
        })
        .detach();
    }

    fn gx_store_read_remote_sidebar_hud(&mut self, machine_id: &str, cx: &mut gpui::Context<Self>) {
        // A machine with no tunnel has nobody to ask, as the runtime's request would have failed.
        let Some(target) = self
            .remote_gxserver_connections
            .get(machine_id)
            .map(|connection| connection.request_target())
        else {
            return;
        };
        self.gx_store.hud.counters.remote_reads += 1;
        let machine_id = machine_id.to_string();
        cx.spawn(async move |this, cx| {
            let hud = read_remote_sidebar_hud(target).await;
            let _ = this.update(cx, |this, cx| match hud {
                None => this.gx_store.hud.counters.failed_reads += 1,
                Some(hud) => {
                    this.gx_store
                        .hud
                        .sources
                        .remote_sidebar_huds
                        .insert(machine_id, hud);
                    this.gx_store_hud_sources_changed(cx);
                }
            });
        })
        .detach();
    }

    /// Settings were saved or re-read: the HUD's `settings` block and everything drawn from it,
    /// and the status item and pet, which read the same settings (gx_store/indicators/).
    pub(crate) fn gx_store_hud_settings_changed(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_hud_sources_changed(cx);
        self.gx_store_indicators_settings_changed(cx);
    }

    /// The saved settings object the HUD was last composed from.
    pub(in crate::app::gx_store) fn gx_store_hud_settings_value(&self) -> Value {
        self.gx_store.hud.sources.settings.clone()
    }

    /// A source moved outside a list update: recompose, and bring the list up to date with it.
    pub(crate) fn gx_store_hud_sources_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store_compose_hud(cx) {
            self.gx_store_sidebar_state_changed(cx);
        }
    }

    /// The store moved: recompose when the change touched what the HUD reads from it. Called by
    /// the sidebar list's update before it reads the HUD's generation.
    pub(in crate::app::gx_store) fn gx_store_hud_store_changed(
        &mut self,
        changes: &ChangeSummary,
        cx: &mut gpui::Context<Self>,
    ) {
        let relevant = !changes.machines_reloaded.is_empty()
            || !changes.projects_changed.is_empty()
            || !changes.projects_removed.is_empty()
            || !changes.project_order_changed.is_empty()
            || !changes.connection_changed.is_empty()
            || changes.side_state.any()
            || changes.focus_changed
            // A parked remote project counts that machine's sessions.
            || changes
                .sessions_changed
                .iter()
                .chain(&changes.sessions_removed)
                .any(|session| !session.machine.is_local());
        if relevant {
            self.gx_store_compose_hud(cx);
        }
    }

    /// Composes the HUD and, when it moved, hands it to every reader. Returns whether it moved;
    /// the caller brings the list up to date.
    pub(crate) fn gx_store_compose_hud(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let runtime_settings = &self.sidebar_runtime_settings_snapshot;
        let hud_host = &mut self.gx_store.hud;
        hud_host.sources.settings = serde_json::from_str(&runtime_settings.saved_settings_json)
            .unwrap_or_else(|_| json!({}));
        hud_host.sources.debugging_mode = runtime_settings.debugging_mode;
        hud_host.sources.show_beta_features = runtime_settings.show_beta_features;
        hud_host.sources.remote_recent_projects =
            self.gx_store.runtime_facts.remote_recent_projects.clone();
        hud_host.sources.active_project_id = self
            .gx_store
            .core
            .focus()
            .active_project
            .as_ref()
            .map(|project| project.to_workspace_project_id());
        hud_host.counters.composes += 1;
        let hud = compose_sidebar_hud(&self.gx_store.core, &hud_host.sources);
        if self.gx_store.runtime_facts.hud.as_deref() == Some(&hud) {
            return false;
        }
        self.gx_store.hud.counters.changes += 1;
        // The Settings window shows the machine-scoped Actions of a remote project; a HUD whose
        // row for that project moved refreshes it (`with_remote_project_action_rows`).
        let remote_project = self.gpui_app_modal_active_project_id().filter(|id| {
            self.app_modal_window.is_some()
                && gpui_remote_project_reference_from_project_id(id).is_some()
        });
        let remote_actions_changed = remote_project.as_deref().is_some_and(|project| {
            let rows = |hud: Option<&Value>| {
                hud.and_then(|hud| hud.get("commandsByProject"))
                    .and_then(|rows| rows.get(project))
                    .cloned()
            };
            rows(self.gx_store.runtime_facts.hud.as_deref()) != rows(Some(&hud))
        });
        let global_actions = global_actions_payload(&hud);
        let facts = &mut self.gx_store.runtime_facts;
        facts.hud = Some(Arc::new(hud));
        facts.hud_generation += 1;
        facts.counters.hud_posts += 1;
        if self.gx_store.hud.posted_global_actions.as_deref() != Some(global_actions.as_str()) {
            self.receive_sidebar_global_actions_payload(&global_actions, cx);
            self.gx_store.hud.posted_global_actions = Some(global_actions);
        }
        if remote_actions_changed {
            self.refresh_open_gpui_app_modal_sidebar_state_in_background(cx);
        }
        true
    }

    /// The counters, for the store's periodic summary.
    #[allow(dead_code)]
    pub(crate) fn gx_store_hud_counters(&self) -> HudCounters {
        self.gx_store.hud.counters
    }
}

/// The tab strip's Global Actions: the bounded id, name and icon of the first eight, which is all
/// the strip draws.
///
/// CDXC:AgentLauncher 2026-08-01:
/// Publish only what the native strip draws: bounded action id, display name, and icon slug.
/// Command text, URLs, links, and run state deliberately stay out: a strip click sends the id back
/// and the trusted definition is resolved there, so the strip never holds anything executable.
fn global_actions_payload(hud: &Value) -> String {
    let actions: Vec<Value> = hud
        .get("globalCommands")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .take(GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS)
        .map(|command| {
            let mut action = serde_json::Map::new();
            action.insert("commandId".into(), command["commandId"].clone());
            if let Some(icon) = command
                .get("icon")
                .filter(|icon| icon.as_str().is_some_and(|icon| !icon.is_empty()))
            {
                action.insert("icon".into(), icon.clone());
            }
            action.insert("name".into(), command["name"].clone());
            Value::Object(action)
        })
        .collect();
    json!({
        "actions": actions,
        "type": GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE,
        "version": GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION,
    })
    .to_string()
}
