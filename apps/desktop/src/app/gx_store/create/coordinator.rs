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
use crate::app::window::{NewCoordinatorAgent, NewCoordinatorModel, NewCoordinatorRefusal};
use ghostex_gx_chat_core::composer::skill_invocation::orchestrator_skill_line;
use ghostex_gx_chat_core::protocol::{
    COORDINATOR_AGENT_FAMILIES, coordinator_default_model_label, coordinator_launch_takes_effort,
    coordinator_launch_takes_model, coordinator_model_label,
};

/// The agent families a coordinator can run on (`COORDINATOR_AGENT_FAMILIES` in gx-protocol,
/// which gxserver checks too).
fn coordinator_family(agent: &Value) -> Option<&'static str> {
    let icon = agent.get("icon").and_then(Value::as_str);
    let id = agent.get("agentId").and_then(Value::as_str);
    let family = default_agent_id_for_icon(icon)
        .or_else(|| default_agent_id_for_icon(id))
        .or(id)?;
    COORDINATOR_AGENT_FAMILIES
        .into_iter()
        .find(|supported| *supported == family)
}

/// The model lineup a coordinator on this family can pick from in `catalog` (this computer's
/// newest, which the chat's model pill reads too, or the one another computer serves), each
/// model with the efforts it accepts. Empty without a catalog, and for an agent whose launch
/// takes no model (`coordinator_launch_takes_model`): it starts on its configured one; no efforts
/// for one whose launch takes no effort.
fn coordinator_models(
    family: &str,
    catalog: Option<&AgentModelCatalog>,
) -> Vec<NewCoordinatorModel> {
    if !coordinator_launch_takes_model(family) {
        return Vec::new();
    }
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    let Some(agent) = catalog.agents.get(family) else {
        return Vec::new();
    };
    let takes_effort = coordinator_launch_takes_effort(family);
    // OpenCode's lineup marks no default, so its first row keeps OpenCode's own model.
    let keep_default = coordinator_default_model_label(family).map(|label| NewCoordinatorModel {
        value: String::new(),
        label: label.to_string(),
        efforts: Vec::new(),
        default: true,
    });
    let models = agent.models.iter().map(|model| NewCoordinatorModel {
        value: model.value.clone(),
        label: coordinator_model_label(family, &model.value, &model.label),
        efforts: model
            .efforts
            .iter()
            .filter(|_| takes_effort)
            .map(|effort| (effort.clone(), catalog.effort_label(effort)))
            .collect(),
        default: model.default,
    });
    keep_default.into_iter().chain(models).collect()
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

/// The launchers of a sidebar HUD an orchestrator can run on, Claude first, in launcher order,
/// with their lineups from `catalog`.
pub(crate) fn coordinator_agents(
    hud: Option<&Value>,
    catalog: Option<&AgentModelCatalog>,
) -> Vec<NewCoordinatorAgent> {
    let rows = hud
        .and_then(|hud| hud.get("agents"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut agents = rows
        .iter()
        .filter_map(|row| {
            let family = coordinator_family(row)?;
            Some((
                family,
                NewCoordinatorAgent {
                    agent_id: row.get("agentId")?.as_str()?.to_string(),
                    name: row
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(family)
                        .to_string(),
                    family: family.to_string(),
                    models: coordinator_models(family, catalog),
                    skill_line: orchestrator_skill_line(Some(family)),
                },
            ))
        })
        .collect::<Vec<_>>();
    agents.sort_by_key(|(family, _)| *family != "claude");
    agents.into_iter().map(|(_, agent)| agent).collect()
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

    /// Creates the coordinator in the project a group id names (this computer or a remote one) and
    /// opens it in chat. gxserver queues its first message (the skill line) before it answers.
    /// `done` hears whether it was created (before it opens) or why not; a remote computer's
    /// `agent_id` is that computer's own launcher (the dialog lists its agents), sent as it is.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn gx_store_create_coordinator(
        &mut self,
        group_id: &str,
        agent_id: &str,
        name: &str,
        model: Option<&str>,
        effort: Option<&str>,
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
            let params = coordinator_params(
                remote_agent_launch_params(agent_id, &project.project_id, Map::new(), None, name),
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
                        Err(reason) => done(this, Err(NewCoordinatorRefusal::plain(reason)), cx),
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
        let params = coordinator_params(
            local_agent_launch_params(&agent, &project_id, Map::new(), None, name),
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
