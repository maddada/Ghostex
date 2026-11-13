//! Creating a coordinator from the New Coordinator dialog: the agent session gxserver gives the
//! coordinator role (the orchestrator skill and its first message), and opening it in chat.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! A coordinator is created like any sidebar agent launch (the same launch settings, the same focus of the created session), with three differences: the `coordinator` object that makes gxserver add the role and the record; no draft, because a draft can switch its agent before the first prompt and the role would not follow; and a title the user chose, saved as the user's so gxserver keeps it when the agent names the conversation. It opens in chat, where the coordinator's reports read best.
//! SEE-ALSO: apps/desktop/src/app/window/new_coordinator_modal.rs, apps/desktop/src/app/new_coordinator_modal_lifecycle.rs, server/src/server/route_http/sessions.rs (`/api/createAgentSession` with `coordinator`).

use ghostex_gx_chat_core::menus::catalog::AgentModelCatalog;
use ghostex_gx_core::{
    ProjectKey, SessionKey, created_session, default_agent_id_for_icon, local_agent_launch_params,
    new_coordinator_modal_payload, open_remote_session_terminal, remote_agent_launch_params,
    resolve_sidebar_agent, start_provider_params,
};
use serde_json::{Map, Value, json};

use super::super::gx_rpc;
use super::terminal::REMOTE_TIMEOUT;
use crate::GhostexGpuiApp;
use crate::app::remote_conn::sidebar_rpc::{
    GpuiRemoteSidebarRpcMode, gpui_remote_sidebar_rpc_failure_reason,
};
use crate::app::window::{
    DEFAULT_CLAUDE_COORDINATOR_MODEL, NewCoordinatorAgent, NewCoordinatorRefusal,
};
use ghostex_gx_chat_core::protocol::{
    coordinator_launch_takes_effort, coordinator_quality_concern,
};

/// The agent family a launcher row runs as: the coordinator family it is one of
/// (`COORDINATOR_AGENT_FAMILIES` in gx-protocol, by icon or id), else its built-in agent id, else
/// its own id. Any agent may run a coordinator (Q2980).
fn coordinator_family(agent: &Value) -> Option<String> {
    let icon = agent.get("icon").and_then(Value::as_str);
    let id = agent.get("agentId").and_then(Value::as_str);
    let family = default_agent_id_for_icon(icon)
        .or_else(|| default_agent_id_for_icon(id))
        .or(id)?;
    Some(family.to_string())
}

/// The model and effort a new coordinator on `family` starts on, as the create sends them:
/// Opus 5.5 at High on Claude, High where the launch takes an effort; `None` keeps the agent's own.
fn coordinator_start_options(family: &str) -> (Option<&'static str>, Option<&'static str>) {
    let model = (family == "claude").then_some(DEFAULT_CLAUDE_COORDINATOR_MODEL);
    let effort = (coordinator_launch_takes_effort(family)
        && matches!(family, "claude" | "codex" | "antigravity"))
    .then_some("high");
    (model, effort)
}

/// The agent picker's warning for a coordinator on `family`: by the model it starts on (Claude's
/// coordinator model, else the agent's default in `catalog`) and its effort. A model Ghostex does
/// not know warns too.
fn coordinator_start_warning(family: &str, catalog: Option<&AgentModelCatalog>) -> Option<String> {
    let (start_model, effort) = coordinator_start_options(family);
    let lineup = catalog.and_then(|catalog| catalog.agents.get(family));
    let model = lineup.and_then(|agent| match start_model {
        Some(value) => agent.models.iter().find(|model| model.value == value),
        None => agent.models.iter().find(|model| model.default),
    });
    let value = start_model.or(model.map(|model| model.value.as_str()));
    let label = model.map(|model| model.label.as_str());
    let effort = effort.or_else(|| {
        model
            .and_then(|model| model.default_effort.as_deref())
            .or_else(|| lineup.and_then(|agent| agent.default_effort.as_deref()))
    });
    coordinator_quality_concern(value, label, effort).map(|concern| concern.pill_text())
}

/// The title a coordinator is created with, and its source: an unnamed coordinator is a placeholder
/// "Coordinator" the agent's first name may replace (see keeps_its_given_title in
/// server/src/coordinators/title.rs).
fn coordinator_title(name: &str) -> (&str, &'static str) {
    if name.trim().is_empty() {
        ("Coordinator", "placeholder")
    } else {
        (name, "user")
    }
}

/// The create parameters every coordinator shares, on top of an agent launch's.
fn coordinator_params(
    mut params: Value,
    title_source: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Value {
    if let Some(object) = params.as_object_mut() {
        if let Some(model) = model.filter(|model| !model.is_empty()) {
            object.insert("agentModel".to_string(), json!(model));
        }
        if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
            object.insert("agentEffort".to_string(), json!(effort));
        }

        object.remove("draft");
        object.insert("coordinator".to_string(), json!({}));
        let runtime = object
            .entry("runtimeSettings")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(runtime) = runtime.as_object_mut() {
            runtime.insert("titleSource".to_string(), json!(title_source));
        }
    }
    params
}

/// The family of the launcher `agent_id` in a sidebar HUD (`coordinator_family`).
fn coordinator_family_in(hud: Option<&Value>, agent_id: &str) -> Option<String> {
    hud?.get("agents")?
        .as_array()?
        .iter()
        .find(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
        .and_then(coordinator_family)
}

/// Every launcher of a sidebar HUD, in launcher order, with the warning the agent picker shows
/// for a coordinator on it, by `catalog`'s models. The Claude (cloud) agent is left out: a
/// coordinator runs the `ghostex` CLI on a computer Ghostex runs on.
pub(crate) fn coordinator_agents(
    hud: Option<&Value>,
    catalog: Option<&AgentModelCatalog>,
) -> Vec<NewCoordinatorAgent> {
    hud.and_then(|hud| hud.get("agents"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            let agent_id = row.get("agentId")?.as_str()?.to_string();
            if ghostex_gx_core::is_claude_cloud_agent(&agent_id) {
                return None;
            }
            let family = coordinator_family(row)?;
            Some(NewCoordinatorAgent {
                agent_id,
                warning: coordinator_start_warning(&family, catalog),
            })
        })
        .collect()
}

impl GhostexGpuiApp {
    /// This computer's launchers an orchestrator can run on, with this computer's lineups.
    pub(crate) fn gx_store_coordinator_agents(&self) -> Vec<NewCoordinatorAgent> {
        let catalog = crate::app::gx_chat::current_model_catalog();
        coordinator_agents(self.gx_store_launch_hud().as_deref(), catalog.as_ref())
    }

    /// Another computer's launchers an orchestrator can run on, from the sidebar HUD read from it
    /// (`HudHost::remote_sidebar_hud`), with `catalog`'s lineups (that computer's, once read).
    /// `None` while that HUD has not been read.
    pub(crate) fn gx_store_remote_coordinator_agents(
        &self,
        machine_id: &str,
        catalog: Option<&AgentModelCatalog>,
    ) -> Option<Vec<NewCoordinatorAgent>> {
        let hud = self.gx_store.hud.remote_sidebar_hud(machine_id)?;
        Some(coordinator_agents(Some(hud), catalog))
    }

    /// The launcher rows of the sidebar HUD of the copy's computer, which the agent picker lists,
    /// and whether that computer is another one (its accounts are not this computer's).
    pub(crate) fn new_coordinator_launchers(
        &self,
        group_id: &str,
    ) -> (Vec<serde_json::Value>, bool) {
        let remote = ProjectKey::parse_sidebar_group_id(group_id)
            .and_then(|project| project.machine.remote_id().map(str::to_string));
        let rows = |hud: Option<&serde_json::Value>| {
            hud.and_then(|hud| hud.get("agents"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        match remote {
            None => (rows(self.gx_store_launch_hud().as_deref()), false),
            Some(machine_id) => (
                rows(self.gx_store.hud.remote_sidebar_hud(&machine_id)),
                true,
            ),
        }
    }

    /// Opens the New Coordinator dialog for the project a group id names, read from the machine
    /// that has it rather than from the drawn list (gx-core `new_coordinator_modal_payload`). The
    /// New thread picker's Coordinator, on the desktop and the web build. A project that is gone
    /// since the picker opened (closed or removed, here or by another client) opens no dialog,
    /// whose Create would target it: a notice says so instead.
    pub(crate) fn gx_store_open_new_coordinator(
        &mut self,
        group_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let project_name = ProjectKey::parse_sidebar_group_id(group_id).and_then(|project| {
            let loaded = self.gx_store.core.presentation().loaded(&project.machine)?;
            Some(loaded.project(&project.project_id)?.title.clone())
        });
        let Some(project_name) = project_name else {
            self.gx_store_create_toast(
                "warning",
                "Project no longer open",
                Some("It was closed or removed. Choose another project and try again."),
                cx,
            );
            return;
        };
        self.open_app_modal_from_bridge(new_coordinator_modal_payload(group_id, &project_name), cx);
    }

    /// Creates the coordinator in the project a group id names (this computer or a remote one) on
    /// the agent and account the agent picker gave, and opens it in chat. gxserver queues its
    /// first message (the skill line) before it answers. Its model and effort are the agent's
    /// coordinator start (`coordinator_start_options`). `done` hears whether it was created
    /// (before it opens) or why not; a remote computer's `agent_id` is that computer's own
    /// launcher (the picker lists its agents), sent as it is.
    pub(crate) fn gx_store_create_coordinator(
        &mut self,
        group_id: &str,
        agent_id: &str,
        account_id: Option<&str>,
        name: &str,
        done: impl FnOnce(&mut Self, Result<(), NewCoordinatorRefusal>, &mut gpui::Context<Self>)
        + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
            done(
                self,
                Err(NewCoordinatorRefusal::plain(
                    "That project is no longer open.",
                )),
                cx,
            );
            return;
        };
        let hud = self.gx_store_launch_hud();
        let (name, title_source) = coordinator_title(name);
        if let Some(machine_id) = project.machine.remote_id().map(str::to_string) {
            let family =
                coordinator_family_in(self.gx_store.hud.remote_sidebar_hud(&machine_id), agent_id)
                    .unwrap_or_default();
            let (model, effort) = coordinator_start_options(&family);
            let params = coordinator_params(
                remote_agent_launch_params(
                    agent_id,
                    &project.project_id,
                    Map::new(),
                    account_id,
                    name,
                ),
                title_source,
                model,
                effort,
            );
            let task = self.start_gpui_remote_sidebar_rpc(
                &machine_id,
                "/api/createAgentSession",
                Some(params),
                REMOTE_TIMEOUT,
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            );
            let project_id = project.project_id.clone();
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    const NOT_CREATED: &str =
                        "The remote computer could not create it. Its Ghostex may need an update.";
                    let created = match result {
                        Ok(response) => created_session(&response, Some(&project_id))
                            .ok_or_else(|| NOT_CREATED.to_string()),
                        Err(error) => {
                            Err(gpui_remote_sidebar_rpc_failure_reason(&error, NOT_CREATED))
                        }
                    };
                    match created {
                        Ok((created_project, session_id)) => {
                            done(this, Ok(()), cx);
                            let created_project = created_project.unwrap_or(project_id.clone());
                            let session = SessionKey::remote(
                                machine_id.as_str(),
                                created_project,
                                session_id,
                            );
                            let payload = open_remote_session_terminal(
                                &session.to_sidebar_session_id(),
                                true,
                                Some("chat"),
                            );
                            this.receive_sidebar_native_project_path_action_payload(
                                &payload.to_string(),
                                cx,
                            );
                        }
                        Err(reason) => {
                            done(this, Err(NewCoordinatorRefusal::from_computer(reason)), cx)
                        }
                    }
                });
            })
            .detach();
            return;
        }
        let project_id = project.project_id.clone();
        if !self.gx_store_local_project_path_is_available(&project_id) {
            // The dialog closes, and the missing folder's own dialog offers Locate.
            done(self, Ok(()), cx);
            self.gx_store_ensure_local_project_path_available(&project_id, cx);
            return;
        }
        let Some(agent) = resolve_sidebar_agent(hud.as_deref(), agent_id) else {
            done(
                self,
                Err(NewCoordinatorRefusal::plain(
                    "That agent is no longer set up on this computer.",
                )),
                cx,
            );
            return;
        };
        let family = coordinator_family_in(hud.as_deref(), agent_id).unwrap_or_default();
        let (model, effort) = coordinator_start_options(&family);
        let params = coordinator_params(
            local_agent_launch_params(&agent, &project_id, Map::new(), account_id, name),
            title_source,
            model,
            effort,
        );
        cx.spawn(async move |this, cx| {
            let created = gx_rpc(None, "/api/createAgentSession", params).await;
            let (created, role_error) = match created {
                Ok(response) => (
                    created_session(&response, Some(&project_id)),
                    response
                        .get("coordinatorRoleError")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                ),
                Err(error) => {
                    let message = error.message.clone();
                    let _ = this.update(cx, |this, cx| {
                        done(this, Err(NewCoordinatorRefusal::from_computer(message)), cx)
                    });
                    return;
                }
            };
            let Some((created_project, session_id)) = created else {
                let _ = this.update(cx, |this, cx| {
                    done(
                        this,
                        Err(NewCoordinatorRefusal::plain(
                            "Coordinator not created. Try again.",
                        )),
                        cx,
                    )
                });
                return;
            };
            let _ = this.update(cx, |this, cx| done(this, Ok(()), cx));
            let created_project = created_project.unwrap_or(project_id.clone());
            // Started here rather than by the attach, so its first message has an agent to wait
            // for even when the chat is not opened yet.
            let _ = gx_rpc(
                None,
                "/api/startSessionProvider",
                start_provider_params(&created_project, &session_id),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(message) = role_error.as_deref() {
                    this.gx_store_create_toast(
                        "warning",
                        "Coordinator's skill not set up",
                        Some(message),
                        cx,
                    );
                }
                this.gx_store_focus_created_session(
                    &created_project,
                    &session_id,
                    true,
                    Some("chat"),
                    cx,
                );
            });
        })
        .detach();
    }

    /// Makes an existing session a coordinator (`/api/promoteCoordinator` on the gxserver of the
    /// computer that runs it, this one or a connected one). The session keeps running untouched;
    /// gxserver queues its playbook for after the current turn.
    pub(crate) fn gx_store_promote_coordinator(
        &mut self,
        sidebar_session_id: &str,
        goal: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id) else {
            return;
        };
        // CDXC:Coordinators 2026-11-07 FIX: a connected computer's session is promoted by that
        // computer's gxserver, which installs the orchestrator skill there and queues the
        // playbook in that session's chat (Discord: "why orchestrator cannot be promoted to a
        // session in a remote machine from the desktop app?"). See the decision in
        // packages/gx-core/src/sidebar_menu/session.rs.
        let remote = match session.machine.remote_id() {
            None => Some(None),
            Some(machine_id) => self
                .gpui_remote_gxserver_request_target(machine_id)
                .map(Some),
        };
        let Some(remote) = remote else {
            self.gx_store_create_toast(
                "warning",
                "Not made a coordinator",
                Some("That computer isn't connected. Try again once it is."),
                cx,
            );
            return;
        };
        let params = json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "goal": goal.trim(),
        });
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(remote, "/api/promoteCoordinator", params).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => this.gx_store_create_toast(
                    "info",
                    "Now a coordinator",
                    Some("Its playbook reaches it once its current turn is over."),
                    cx,
                ),
                Err(error) => this.gx_store_create_toast(
                    "warning",
                    "Not made a coordinator",
                    Some(&error.message),
                    cx,
                ),
            });
        })
        .detach();
    }
}
