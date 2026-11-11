//! The selected project's Issues Workflow section on the Projects page: this project's own Linear
//! API key (`/api/setLinearApiKey` with `projectId`), with a status line saying which key the project
//! uses now (`/api/readWorkModeStatus` says which keys exist, never their values;
//! `/api/readWorkspaces` names the workspace whose key it falls back to). Every project shows it:
//! Issues Workflow has no per-project switch and does not need Workspaces (CDXC:WorkMode
//! 2026-10-24 in server/src/work_mode/project.rs).
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: the Linear key is a "shared key, overridable per project".
//! SEE-ALSO: server/src/server/route_http/work_mode.rs, server/src/work_mode/credentials.rs
//! (project → workspace → shared lookup), settings_modal/tabs/workspaces.rs (the workspace key row).
use super::super::super::fields::{
    ButtonVariant, FieldStates, settings_button, settings_text_input,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_gxserver_rpc;
use super::{ProjectItem, ProjectsTab, plain_row};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
/// Checking a Linear key is a call to Linear.
const LINEAR_KEY_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_WORKSPACE_ID: &str = "personal";
const LINEAR_KEY_HELP: &str = "Linear → Settings → Security & access → Personal API keys.";

/// What the Linear key row reads from gxserver and what it is saving.
#[derive(Default)]
pub(super) struct ProjectWorkModeState {
    requested: bool,
    /// `linearKeys` from `/api/readWorkModeStatus`.
    linear_keys: Option<Value>,
    /// Workspace names by id, from `/api/readWorkspaces`.
    workspace_names: HashMap<String, String>,
    /// A key typed but not saved yet, per project.
    key_drafts: HashMap<String, String>,
    saving_keys: HashSet<String>,
    /// The Linear account a key was saved for, per project, from this session's saves.
    key_accounts: HashMap<String, String>,
    masked: HashMap<SharedString, bool>,
}

/// `workspaceId` of each project the page lists (projectSettingsProjects).
pub(super) fn project_workspace_ids(hud: Option<&Value>) -> HashMap<String, Option<String>> {
    hud.and_then(|hud| hud.get("projectSettingsProjects"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let project_id = project.get("projectId")?.as_str()?.to_string();
            let workspace_id = project
                .get("workspaceId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string);
            Some((project_id, workspace_id))
        })
        .collect()
}

impl ProjectsTab {
    fn work_rpc_available(&self, cx: &App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    fn work_toast(&self, title: &str, description: &str, cx: &mut App) {
        self.store
            .update(cx, |store, cx| store.toast("error", title, description, cx));
    }

    /// Reads which Linear keys exist and the workspace names, once per page and after each save.
    fn reload_work_mode_status(&mut self, cx: &mut Context<Self>) {
        self.work.requested = true;
        if !self.work_rpc_available(cx) {
            return;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readWorkModeStatus",
            json!({}),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Ok(result) = result {
                        page.work.linear_keys = result.get("linearKeys").cloned();
                        cx.notify();
                    }
                });
            },
            cx,
        );
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readWorkspaces",
            json!({}),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Ok(result) = result {
                        page.work.workspace_names = result
                            .pointer("/sidebarWorkspaces/workspaces")
                            .and_then(Value::as_object)
                            .map(|workspaces| {
                                workspaces
                                    .iter()
                                    .filter_map(|(id, workspace)| {
                                        let name = workspace.get("name")?.as_str()?.trim();
                                        (!name.is_empty()).then(|| (id.clone(), name.to_string()))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    fn save_project_linear_key(&mut self, project_id: String, cx: &mut Context<Self>) {
        let key = self
            .work
            .key_drafts
            .get(&project_id)
            .map(|key| key.trim().to_string())
            .unwrap_or_default();
        if key.is_empty() || self.work.saving_keys.contains(&project_id) {
            return;
        }
        self.work.saving_keys.insert(project_id.clone());
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/setLinearApiKey",
            json!({ "projectId": project_id, "apiKey": key }),
            LINEAR_KEY_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.work.saving_keys.remove(&project_id);
                    match result {
                        Ok(result) => {
                            page.work.key_drafts.remove(&project_id);
                            if let Some(account) = linear_account_label(&result) {
                                page.work.key_accounts.insert(project_id.clone(), account);
                            }
                            page.fields
                                .texts
                                .remove(&SharedString::from(key_input_id(&project_id)));
                        }
                        Err(error) => page.work_toast("Couldn't save the Linear key", &error, cx),
                    }
                    page.reload_work_mode_status(cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn remove_project_linear_key(&mut self, project_id: String, cx: &mut Context<Self>) {
        self.work.key_accounts.remove(&project_id);
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/setLinearApiKey",
            json!({ "projectId": project_id, "apiKey": "" }),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Err(error) = result {
                        page.work_toast("Couldn't remove the Linear key", &error, cx);
                    }
                    page.reload_work_mode_status(cx);
                });
            },
            cx,
        );
    }

    fn listed(&self, field: &str, id: &str) -> bool {
        self.work
            .linear_keys
            .as_ref()
            .and_then(|keys| keys.get(field))
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|entry| entry.as_str() == Some(id)))
    }

    /// The status line under the key: which key this project's Linear calls use now, in the
    /// order gxserver looks them up (this project's, its workspace's, the shared one).
    fn linear_key_status(&self, project_id: &str, workspace_id: &str) -> String {
        if self.work.saving_keys.contains(project_id) {
            return "Checking the key with Linear…".to_string();
        }
        if self.work.linear_keys.is_none() {
            return "Paste a key to give this project its own Linear key.".to_string();
        }
        let workspace_name = self
            .work
            .workspace_names
            .get(workspace_id)
            .cloned()
            .unwrap_or_else(|| "its".to_string());
        let fallback =
            if workspace_id != DEFAULT_WORKSPACE_ID && self.listed("workspaces", workspace_id) {
                Some(format!("the {workspace_name} workspace's key"))
            } else if self
                .work
                .linear_keys
                .as_ref()
                .and_then(|keys| keys.get("shared"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                Some("the shared key (the Personal workspace's)".to_string())
            } else {
                None
            };
        if self.listed("projectOverrides", project_id) {
            let saved = match self.work.key_accounts.get(project_id) {
                Some(account) => format!("Uses this project's own key · {account}."),
                None => "Uses this project's own key.".to_string(),
            };
            return match fallback {
                Some(fallback) => format!("{saved} Remove it to use {fallback}."),
                None => saved,
            };
        }
        match fallback {
            Some(fallback) => {
                format!("Uses {fallback}. Paste a key to give this project its own.")
            }
            None => format!(
                "No key yet: paste one for this project, or set one for every project with ghostex issues linear-key (or per workspace on the Workspaces page). Linear tickets show once a key applies. {LINEAR_KEY_HELP}"
            ),
        }
    }

    /// This project's Linear key, with the status line under it: the rows of its Issues Workflow
    /// section.
    pub(super) fn work_mode_rows(
        &mut self,
        p: &SettingsPalette,
        projects: &[ProjectItem],
        project: &ProjectItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.work.requested {
            self.reload_work_mode_status(cx);
        }
        let workspace_ids = project_workspace_ids(self.store.read(cx).hud());
        let project_id = project.project_id.clone();
        // A worktree project uses its parent checkout's workspace, as gxserver does.
        let workspace_id = project
            .worktree_parent_project_id
            .as_ref()
            .filter(|parent| projects.iter().any(|item| &item.project_id == *parent))
            .and_then(|parent| workspace_ids.get(parent).cloned().flatten())
            .or_else(|| workspace_ids.get(&project_id).cloned().flatten())
            .filter(|id| {
                self.work.workspace_names.is_empty() || self.work.workspace_names.contains_key(id)
            })
            .unwrap_or_else(|| DEFAULT_WORKSPACE_ID.to_string());
        let rpc = self.work_rpc_available(cx);
        let mut rows = Vec::new();

        let input_id = SharedString::from(key_input_id(&project_id));
        let draft = self
            .work
            .key_drafts
            .get(&project_id)
            .cloned()
            .unwrap_or_default();
        let draft_project = project_id.clone();
        let input = FieldStates::text_state(
            self,
            &input_id,
            &draft,
            Some("lin_api_…"),
            move |page: &mut Self, text, _window, cx| {
                page.work.key_drafts.insert(draft_project.clone(), text);
                cx.notify();
            },
            window,
            cx,
        );
        super::super::accounts::widgets::sync_masked(
            &mut self.work.masked,
            &input_id,
            &input,
            true,
            window,
            cx,
        );
        let own_key = self.listed("projectOverrides", &project_id);
        let saving = self.work.saving_keys.contains(&project_id);
        let status = self.linear_key_status(&project_id, &workspace_id);
        let save_project = project_id.clone();
        let remove_project = project_id.clone();
        let control = h_flex()
            .gap(px(8.0))
            .child(settings_text_input(
                p,
                &input,
                Some(220.0),
                true,
                window,
                cx,
            ))
            .child(settings_button(
                p,
                SharedString::from("project-linear-key-save"),
                "Save",
                None,
                ButtonVariant::Outline,
                !rpc || saving || draft.trim().is_empty(),
                None,
                move |page: &mut Self, _window, cx| {
                    page.save_project_linear_key(save_project.clone(), cx)
                },
                cx,
            ))
            .when(own_key && !saving, |row| {
                row.child(settings_button(
                    p,
                    SharedString::from("project-linear-key-remove"),
                    "Remove",
                    None,
                    ButtonVariant::Ghost,
                    !rpc,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.remove_project_linear_key(remove_project.clone(), cx)
                    },
                    cx,
                ))
            });
        rows.push(plain_row(
            p,
            &SharedString::from("project-linear-key-row"),
            "Linear API key",
            None,
            None,
            true,
            gpui_component::v_flex()
                .w_full()
                .gap(px(8.0))
                .child(control)
                .child(
                    gpui::div()
                        .text_size(px(12.0))
                        .line_height(px(17.0))
                        .text_color(super::super::super::super::native_modal_kit::hsla(p.muted))
                        .child(status),
                )
                .into_any_element(),
            cx,
        ));
        rows
    }
}

fn key_input_id(project_id: &str) -> String {
    format!("project-linear-key-{project_id}")
}

/// "Name @ Organization" from a `/api/setLinearApiKey` answer.
fn linear_account_label(result: &Value) -> Option<String> {
    let account = result.get("account")?;
    let name = account.get("name").and_then(Value::as_str)?;
    let organization = account.get("organization").and_then(Value::as_str);
    Some(match organization {
        Some(organization) => format!("{name} @ {organization}"),
        None => name.to_string(),
    })
}
