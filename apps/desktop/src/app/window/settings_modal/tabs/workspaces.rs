//! The Workspaces page: every workspace with its name, color, kind (Work or Personal), Linear API
//! key, Claude account and browser sign-ins, plus New workspace and Delete. It reads
//! `/api/readWorkspaces`, `/api/readWorkModeStatus` (which keys exist, never their values) and
//! `/api/agentAccounts` (the Claude accounts), and writes through `/api/createWorkspace`,
//! `/api/updateWorkspace`, `/api/deleteWorkspace` and `/api/setLinearApiKey`.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User (mockup 09): one Settings page for the workspaces, each with a field per key and a status
//! line under it; Work or Personal sets whether its projects start with work mode on. The Linear
//! key is shared by the workspace's projects (a project can still override it), and the Personal
//! workspace's key is the shared one. A Work workspace also gets its team's rows (Convex, Slack,
//! the team's Linear key: `workspaces/team.rs`), its Team flow (`workspaces/slack_flow.rs`) and its
//! team-flow steps (`workspaces/flow_steps.rs`).
//! SEE-ALSO: server/src/server/route_http/workspaces.rs, server/src/work_mode/credentials.rs,
//! apps/desktop/src/app/workspace_browser.rs (`clear_workspace_browser_signins`).
mod claude_accounts;
mod drafts;
mod flow_steps;
mod instructions;
mod slack_flow;
mod team;
mod tracker;

use super::super::catalog::SettingOption;
use super::super::fields::notice::{greyed_out, is_service_unreachable, service_unreachable_state};
use super::super::fields::{
    ButtonVariant, FieldStates, RowSpec, SettingsPage, setting_row, settings_button, settings_icon,
    settings_section, settings_segmented, settings_select, settings_text_input, static_note,
    switch_control,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, post_store_message, store_gxserver_rpc};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Task, Window, div, px, rgb,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
/// Checking a Linear key is a call to Linear.
const LINEAR_KEY_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a typed name waits before it is saved.
const NAME_SAVE_DELAY: Duration = Duration::from_millis(600);
const DEFAULT_WORKSPACE_ID: &str = "personal";

/// The workspace colors offered (the Spaces palette).
const WORKSPACE_COLORS: [&str; 12] = [
    "#596fd1", "#3f8fc7", "#2f9b95", "#3aa675", "#8c9b45", "#c4a23d", "#d6873f", "#c95353",
    "#d75b72", "#b36ad4", "#7c6df2", "#4f5663",
];

pub(crate) fn workspaces_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| WorkspacesTab::new(store.clone(), cx)).into()
}

pub(crate) struct WorkspacesTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// `sidebarWorkspaces` from the last read.
    workspaces: Option<Value>,
    error: Option<String>,
    /// `linearKeys` from `/api/readWorkModeStatus`: which keys exist.
    linear_keys: Option<Value>,
    /// The saved Claude accounts with their detected `claude.ai` connectors.
    claude_accounts: Vec<claude_accounts::ClaudeAccountInfo>,
    /// Whether `/api/agentAccounts` is being refreshed.
    reloading_accounts: bool,
    /// Per-workspace file-backed agent instructions from `/api/readWorkspaceInstructions`.
    instructions: HashMap<String, instructions::WorkspaceInstructionsState>,
    /// A Linear key typed but not saved yet, per workspace.
    key_drafts: HashMap<String, String>,
    /// Workspaces whose Linear key is being checked and saved.
    saving_keys: HashSet<String>,
    /// The Linear account a key was saved for, per workspace, from this session's saves.
    key_accounts: HashMap<String, String>,
    /// A name typed, waiting to be saved.
    name_saves: HashMap<String, Task<()>>,
    masked: HashMap<SharedString, bool>,
    confirm_delete: Option<String>,
    /// Typed-but-unsaved text on the team rows, by input id.
    drafts: HashMap<SharedString, String>,
    /// Each Work workspace's team connection, Slack flow settings and team-flow steps.
    team: HashMap<String, team::TeamConnectionState>,
    slack_flows: HashMap<String, slack_flow::SlackFlowState>,
    flow_steps: HashMap<String, flow_steps::FlowStepsState>,
    /// Each workspace's primary tracker (Linear or GitHub).
    trackers: HashMap<String, tracker::TrackerState>,
}

impl SettingsPage for WorkspacesTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl WorkspacesTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.spawn(async move |page, cx| {
            let _ = page.update(cx, |page, cx| page.reload(cx));
        })
        .detach();
        Self {
            store,
            fields: FieldStates::default(),
            workspaces: None,
            error: None,
            linear_keys: None,
            claude_accounts: Vec::new(),
            reloading_accounts: false,
            instructions: HashMap::new(),
            key_drafts: HashMap::new(),
            saving_keys: HashSet::new(),
            key_accounts: HashMap::new(),
            name_saves: HashMap::new(),
            masked: HashMap::new(),
            confirm_delete: None,
            drafts: HashMap::new(),
            team: HashMap::new(),
            slack_flows: HashMap::new(),
            flow_steps: HashMap::new(),
            trackers: HashMap::new(),
        }
    }

    fn rpc_available(&self, cx: &App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    fn toast(&self, level: &str, title: &str, description: &str, cx: &mut App) {
        self.store
            .update(cx, |store, cx| store.toast(level, title, description, cx));
    }

    /// Reads the workspaces, which Linear keys exist, and the Claude accounts.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readWorkspaces",
            json!({}),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    match result {
                        Ok(result) => {
                            page.workspaces = result.get("sidebarWorkspaces").cloned();
                            page.error = None;
                        }
                        Err(error) => page.error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readWorkModeStatus",
            json!({}),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Ok(result) = result {
                        page.linear_keys = result.get("linearKeys").cloned();
                        cx.notify();
                    }
                });
            },
            cx,
        );
        self.reload_claude_accounts(cx);
        self.reload_all_workspace_instructions(cx);
    }

    /// One workspace write, then a reload so the page shows what the daemon kept.
    fn write(
        &mut self,
        path: &'static str,
        params: Value,
        failure: &'static str,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            path,
            params,
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Err(error) = result {
                        page.toast("error", failure, &error, cx);
                    }
                    page.reload(cx);
                });
            },
            cx,
        );
    }

    fn update_workspace(&mut self, workspace_id: &str, patch: Value, cx: &mut Context<Self>) {
        let mut params = json!({ "workspaceId": workspace_id });
        if let (Some(params), Some(patch)) = (params.as_object_mut(), patch.as_object()) {
            params.extend(patch.clone());
        }
        // Shown at once; the reload after the write settles it.
        if let Some(workspace) = self
            .workspaces
            .as_mut()
            .and_then(|state| state.get_mut("workspaces"))
            .and_then(|workspaces| workspaces.get_mut(workspace_id))
            .and_then(Value::as_object_mut)
            && let Some(patch) = patch.as_object()
        {
            for (key, value) in patch {
                workspace.insert(key.clone(), value.clone());
            }
        }
        cx.notify();
        self.write(
            "/api/updateWorkspace",
            params,
            "Couldn't save the workspace",
            cx,
        );
    }

    fn schedule_name_save(&mut self, workspace_id: String, name: String, cx: &mut Context<Self>) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let key = workspace_id.clone();
        let task = cx.spawn(async move |page, cx| {
            cx.background_executor().timer(NAME_SAVE_DELAY).await;
            let _ = page.update(cx, |page, cx| {
                page.name_saves.remove(&workspace_id);
                let saved = page
                    .workspace(&workspace_id)
                    .and_then(|workspace| workspace.get("name"))
                    .and_then(Value::as_str)
                    == Some(name.as_str());
                if !saved {
                    // The letter follows the name unless the daemon was given one; send it.
                    let letter = name
                        .chars()
                        .find(|c| c.is_alphanumeric())
                        .map(|c| c.to_uppercase().collect::<String>());
                    page.update_workspace(
                        &workspace_id,
                        json!({ "name": name, "letter": letter }),
                        cx,
                    );
                }
            });
        });
        self.name_saves.insert(key, task);
    }

    fn save_linear_key(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        let key = self
            .key_drafts
            .get(&workspace_id)
            .map(|key| key.trim().to_string())
            .unwrap_or_default();
        if key.is_empty() || self.saving_keys.contains(&workspace_id) {
            return;
        }
        self.saving_keys.insert(workspace_id.clone());
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/setLinearApiKey",
            json!({ "workspaceId": workspace_id, "apiKey": key }),
            LINEAR_KEY_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.saving_keys.remove(&workspace_id);
                    match result {
                        Ok(result) => {
                            page.key_drafts.remove(&workspace_id);
                            if let Some(account) = linear_account_label(&result) {
                                page.key_accounts.insert(workspace_id.clone(), account);
                            }
                            page.fields.texts.remove(&SharedString::from(format!(
                                "workspace-linear-{workspace_id}"
                            )));
                            // A workspace that never picked a tracker follows its Linear key.
                            page.trackers.clear();
                        }
                        Err(error) => {
                            page.toast("error", "Couldn't save the Linear key", &error, cx)
                        }
                    }
                    page.reload(cx);
                });
            },
            cx,
        );
    }

    fn remove_linear_key(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        self.key_accounts.remove(&workspace_id);
        self.trackers.clear();
        self.write(
            "/api/setLinearApiKey",
            json!({ "workspaceId": workspace_id, "apiKey": "" }),
            "Couldn't remove the Linear key",
            cx,
        );
    }

    fn workspace(&self, workspace_id: &str) -> Option<&Value> {
        self.workspaces
            .as_ref()?
            .get("workspaces")?
            .get(workspace_id)
    }

    fn ordered_workspaces(&self) -> Vec<Value> {
        let Some(state) = self.workspaces.as_ref() else {
            return Vec::new();
        };
        state
            .get("order")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|id| state.get("workspaces")?.get(id).cloned())
            .collect()
    }

    fn has_linear_key(&self, workspace_id: &str) -> bool {
        let Some(keys) = self.linear_keys.as_ref() else {
            return false;
        };
        if workspace_id == DEFAULT_WORKSPACE_ID {
            return keys.get("shared").and_then(Value::as_bool) == Some(true);
        }
        keys.get("workspaces")
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(workspace_id)))
    }

    fn workspace_section(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let workspace_id = text(workspace, "workspaceId");
        let name = text(workspace, "name");
        let color = text(workspace, "color");
        let kind = text(workspace, "kind");
        let is_default = workspace_id == DEFAULT_WORKSPACE_ID;
        let mut rows: Vec<AnyElement> = Vec::new();

        // Name.
        let name_id = SharedString::from(format!("workspace-name-{workspace_id}"));
        let name_workspace = workspace_id.clone();
        let name_input = FieldStates::text_state(
            self,
            &name_id,
            &name,
            Some("Workspace name"),
            move |page: &mut Self, text, _window, cx| {
                page.schedule_name_save(name_workspace.clone(), text, cx);
            },
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            name_id.clone(),
            RowSpec::new("Name")
                .description("Shown in the workspace menu and on the workspace tile's tooltip."),
            None,
            settings_text_input(p, &name_input, Some(220.0), false, window, cx),
            cx,
        ));

        // Color.
        let swatches = h_flex()
            .gap(px(6.0))
            .children(WORKSPACE_COLORS.iter().map(|swatch| {
                let selected = swatch.eq_ignore_ascii_case(&color);
                let value = swatch.to_string();
                let target = workspace_id.clone();
                div()
                    .id(SharedString::from(format!(
                        "workspace-color-{workspace_id}-{swatch}"
                    )))
                    .size(px(20.0))
                    .rounded(px(5.0))
                    .bg(hex(swatch))
                    .cursor_pointer()
                    .when(selected, |swatch| {
                        swatch
                            .border_2()
                            .border_color(gpui::Hsla::from(p.foreground))
                    })
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.update_workspace(&target, json!({ "color": value }), cx);
                    }))
            }));
        rows.push(setting_row(
            p,
            format!("workspace-color-{workspace_id}"),
            RowSpec::new("Color"),
            None,
            swatches.into_any_element(),
            cx,
        ));

        // Icon: the workspace icons (gx-protocol `WORKSPACE_ICONS`) as they draw on the tile, on
        // the workspace's colour; the one in use (picked, or the kind's default) is ringed.
        let disc = hex(&color);
        let current_icon =
            serde_json::from_value::<ghostex_gx_protocol::SidebarWorkspace>(workspace.clone())
                .map(|workspace| workspace.icon_id().to_string())
                .unwrap_or_default();
        let icons = h_flex().flex_wrap().gap(px(6.0)).children(
            ghostex_gx_protocol::WORKSPACE_ICONS.iter().map(|icon| {
                let selected = *icon == current_icon;
                let target = workspace_id.clone();
                div()
                    .id(SharedString::from(format!(
                        "workspace-icon-{workspace_id}-{icon}"
                    )))
                    .p(px(2.0))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        gpui::Hsla::from(p.foreground)
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .child(icon_disc(icon, disc, 22.0))
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.update_workspace(&target, json!({ "icon": icon }), cx);
                    }))
            }),
        );
        rows.push(setting_row(
            p,
            format!("workspace-icon-{workspace_id}"),
            RowSpec::new("Icon").description(
                "On the workspace tile, the first tile of the Spaces row, and in the workspace menu.",
            ),
            None,
            icons.into_any_element(),
            cx,
        ));

        // Kind.
        let kind_workspace = workspace_id.clone();
        rows.push(setting_row(
            p,
            format!("workspace-kind-{workspace_id}"),
            RowSpec::new("Work or Personal").description(
                "A Work workspace can share a team backend with your team (requests from Slack, the team's Linear key); a Personal one is just yours.",
            ),
            None,
            settings_segmented(
                p,
                &format!("workspace-kind-{workspace_id}"),
                &[
                    SettingOption {
                        label: "Work".into(),
                        value: "work".into(),
                    },
                    SettingOption {
                        label: "Personal".into(),
                        value: "personal".into(),
                    },
                ],
                Some(kind.as_str()),
                move |page: &mut Self, value, _window, cx| {
                    page.update_workspace(&kind_workspace, json!({ "kind": value }), cx);
                },
                cx,
            ),
            cx,
        ));

        // Show another workspace's Spaces in this one's Spaces row.
        rows.extend(self.show_spaces_rows(p, workspace, cx));

        // Primary tracker (and the GitHub Projects scope while `gh` lacks it).
        rows.extend(self.tracker_rows(p, &workspace_id, cx));

        // Linear API key.
        rows.push(self.linear_key_row(p, &workspace_id, is_default, window, cx));

        // Workspace computer (shown when remote computers are saved or this workspace lives on
        // another computer). Instructions, Claude accounts and connectors live only on that one
        // computer's gxserver with no multi-machine sync.
        if let Some(row) = self.workspace_computer_row(p, workspace, window, cx) {
            rows.push(row);
        }

        let home_machine_id = text(workspace, "homeMachineId");
        if home_machine_id.trim().is_empty() {
            // Agent instructions (file-backed Markdown, local to this computer).
            rows.push(self.agent_instructions_row(p, &workspace_id, &name, window, cx));

            // Claude accounts (multi-account) and per-account claude.ai connectors + guide.
            rows.extend(self.claude_accounts_rows(p, workspace, cx));
        } else {
            rows.push(self.remote_workspace_local_only_row(p, workspace, cx));
        }

        rows.extend(self.connection_rows(p, &workspace_id, window, cx));

        // Browser sign-ins.
        if !is_default {
            let target = workspace_id.clone();
            rows.push(setting_row(
                p,
                format!("workspace-browser-{workspace_id}"),
                RowSpec::new("Browser sign-ins").description(
                    "This workspace's Browser keeps its own cookies and history, so you can be signed in to a different GitHub or Linear account here.",
                ),
                None,
                settings_button(
                    p,
                    SharedString::from(format!("workspace-signout-{workspace_id}")),
                    "Sign out of all sites",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        post_store_message(
                            &page.store,
                            json!({ "type": "clearWorkspaceBrowserSignins", "workspaceId": target }),
                            cx,
                        );
                    },
                    cx,
                ),
                cx,
            ));
        }

        // Site permissions (CDXC:Browser 2026-10-10 in app/browser_site_requests.rs).
        let target = (!is_default).then(|| workspace_id.clone());
        rows.push(setting_row(
            p,
            format!("workspace-site-answers-{workspace_id}"),
            RowSpec::new("Site permissions").description(
                "Your Allow and Don't Allow answers to sites in this workspace's Browser, such as letting linear.app connect to apps on this computer. Forget them so each site asks again.",
            ),
            None,
            settings_button(
                p,
                SharedString::from(format!("workspace-site-answers-button-{workspace_id}")),
                "Forget all answers",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    post_store_message(
                        &page.store,
                        json!({ "type": "forgetWorkspaceBrowserSiteAnswers", "workspaceId": target }),
                        cx,
                    );
                },
                cx,
            ),
            cx,
        ));

        // Delete.
        if !is_default {
            let confirming = self.confirm_delete.as_deref() == Some(workspace_id.as_str());
            let target = workspace_id.clone();
            rows.push(setting_row(
                p,
                format!("workspace-delete-{workspace_id}"),
                RowSpec::new("Delete workspace").description(if confirming {
                    "Its projects and Spaces move to Personal. Click again to delete."
                } else {
                    "Its projects and Spaces move to Personal."
                }),
                None,
                settings_button(
                    p,
                    SharedString::from(format!("workspace-delete-button-{workspace_id}")),
                    if confirming { "Delete" } else { "Delete…" },
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        if page.confirm_delete.as_deref() == Some(target.as_str()) {
                            page.confirm_delete = None;
                            page.write(
                                "/api/deleteWorkspace",
                                json!({ "workspaceId": target }),
                                "Couldn't delete the workspace",
                                cx,
                            );
                        } else {
                            page.confirm_delete = Some(target.clone());
                            cx.notify();
                        }
                    },
                    cx,
                ),
                cx,
            ));
        }

        let title = if name.is_empty() {
            workspace_id.clone()
        } else {
            name
        };
        settings_section(
            p,
            title,
            Some(SharedString::from(if kind == "work" {
                "Work workspace"
            } else {
                "Personal workspace"
            })),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    fn workspace_computer_row(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let workspace_id = text(workspace, "workspaceId");
        let current_id = text(workspace, "homeMachineId");
        let saved_name = text(workspace, "homeMachineName");
        let machines = saved_remote_machines(&self.store.read(cx).value("remoteMachines"));
        if machines.is_empty() && current_id.is_empty() {
            return None;
        }
        let local_name = crate::host_name::local_computer_name();
        let mut options = vec![SettingOption {
            label: format!("This computer ({local_name})").into(),
            value: String::new(),
        }];
        for (id, label) in &machines {
            options.push(SettingOption {
                label: label.clone().into(),
                value: id.clone(),
            });
        }
        if !current_id.is_empty() && !machines.iter().any(|(id, _)| id == &current_id) {
            let fallback = if saved_name.is_empty() {
                current_id.clone()
            } else {
                saved_name
            };
            options.push(SettingOption {
                label: fallback.into(),
                value: current_id.clone(),
            });
        }
        let names_by_id: HashMap<String, String> = machines.into_iter().collect();
        let target_workspace = workspace_id.clone();
        let select = settings_select(
            self,
            p,
            format!("workspace-home-machine-{workspace_id}"),
            &options,
            &current_id,
            Some(260.0),
            false,
            None,
            move |page: &mut Self, value, _window, cx| {
                if value.is_empty() {
                    page.update_workspace(
                        &target_workspace,
                        json!({
                            "homeMachineId": Value::Null,
                            "homeMachineName": Value::Null,
                        }),
                        cx,
                    );
                } else {
                    let machine_name = names_by_id
                        .get(&value)
                        .cloned()
                        .unwrap_or_else(|| value.clone());
                    page.update_workspace(
                        &target_workspace,
                        json!({
                            "homeMachineId": value,
                            "homeMachineName": machine_name,
                        }),
                        cx,
                    );
                }
            },
            window,
            cx,
        );
        Some(setting_row(
            p,
            format!("workspace-home-machine-{workspace_id}"),
            RowSpec::new("Workspace computer").description(
                "Which computer owns this workspace's instructions file, Claude accounts and claude.ai connectors. They stay on that one computer and are not synced to other PCs.",
            ),
            None,
            select,
            cx,
        ))
    }

    /// Read-only note shown when a workspace is tied to another computer (its workspace PC).
    fn remote_workspace_local_only_row(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace_id = text(workspace, "workspaceId");
        let home_id = text(workspace, "homeMachineId");
        let machines = saved_remote_machines(&self.store.read(cx).value("remoteMachines"));
        let machine_label = machines
            .into_iter()
            .find(|(id, _)| id == &home_id)
            .map(|(_, label)| label)
            .or_else(|| {
                let saved = text(workspace, "homeMachineName");
                (!saved.is_empty()).then_some(saved)
            })
            .unwrap_or(home_id);
        let target_workspace = workspace_id.clone();
        let card = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(super::super::super::native_modal_kit::hsla(p.hairline))
            .bg(super::super::super::native_modal_kit::hsla(p.surface))
            .child(
                gpui_component::v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .items_center()
                            .child(settings_icon(
                                "modals/settings/device-desktop.svg",
                                14.0,
                                p.muted,
                            ))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(super::super::super::native_modal_kit::hsla(
                                        p.foreground,
                                    ))
                                    .child(format!("Lives on {machine_label}")),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .line_height(px(16.0))
                            .text_color(super::super::super::native_modal_kit::hsla(p.muted))
                            .child(format!(
                                "This workspace's instructions file, Claude accounts and claude.ai connectors live on {machine_label}'s Ghostex service and are not synced across computers. Open Settings > Workspaces on {machine_label} to edit them."
                            )),
                    ),
            )
            .child(settings_button(
                p,
                SharedString::from(format!("workspace-use-this-pc-{workspace_id}")),
                "Use this computer",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.update_workspace(
                        &target_workspace,
                        json!({
                            "homeMachineId": Value::Null,
                            "homeMachineName": Value::Null,
                        }),
                        cx,
                    );
                },
                cx,
            ));
        setting_row(
            p,
            format!("workspace-remote-note-{workspace_id}"),
            RowSpec::new("Instructions & Claude connectors")
                .readout(format!("On {machine_label}"))
                .wide(),
            None,
            card.into_any_element(),
            cx,
        )
    }

    /// One switch per other workspace: "Show Work's Spaces" mixes that workspace's Spaces in with
    /// this one's in its windows' Spaces row (gx-protocol `SidebarWorkspace::show_spaces_from`).
    ///
    /// CDXC:Workspaces 2026-11-26 DECISION:
    /// User (Q5013 A): the switch lives on the viewing workspace's own section here, per
    /// workspace, worded "Show Work's Spaces"; no shortcut in the workspace menu. Off by default.
    fn show_spaces_rows(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let workspace_id = text(workspace, "workspaceId");
        let shown: Vec<String> = workspace
            .get("showSpacesFrom")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let mut rows = Vec::new();
        for other in self.ordered_workspaces() {
            let other_id = text(&other, "workspaceId");
            if other_id == workspace_id || other_id.is_empty() {
                continue;
            }
            let other_name = match text(&other, "name") {
                name if name.is_empty() => other_id.clone(),
                name => name,
            };
            let on = shown.contains(&other_id);
            let title = format!("Show {other_name}'s Spaces");
            let target = workspace_id.clone();
            let current = shown.clone();
            let toggled = other_id.clone();
            let control = switch_control(
                p,
                SharedString::from(format!("workspace-show-spaces-{workspace_id}-{other_id}")),
                title.clone(),
                on,
                false,
                None,
                move |page: &mut Self, next, _window, cx| {
                    let mut ids: Vec<String> = current
                        .iter()
                        .filter(|id| **id != toggled)
                        .cloned()
                        .collect();
                    if next {
                        ids.push(toggled.clone());
                    }
                    page.update_workspace(&target, json!({ "showSpacesFrom": ids }), cx);
                },
                cx,
            )
            .into_any_element();
            rows.push(setting_row(
                p,
                format!("workspace-show-spaces-{workspace_id}-{other_id}"),
                RowSpec::new(title).description(format!(
                    "Mixes {other_name}'s Spaces in with yours in the Spaces row: drag them anywhere, swipe to them, and the workspace switch hides. Their sessions keep {other_name}'s accounts and sign-ins."
                )),
                None,
                control,
                cx,
            ));
        }
        rows
    }

    fn linear_key_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        is_default: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input_id = SharedString::from(format!("workspace-linear-{workspace_id}"));
        let draft_workspace = workspace_id.to_string();
        let draft = self
            .key_drafts
            .get(workspace_id)
            .cloned()
            .unwrap_or_default();
        let input = FieldStates::text_state(
            self,
            &input_id,
            &draft,
            Some("lin_api_…"),
            move |page: &mut Self, text, _window, cx| {
                page.key_drafts.insert(draft_workspace.clone(), text);
                cx.notify();
            },
            window,
            cx,
        );
        super::accounts::widgets::sync_masked(
            &mut self.masked,
            &input_id,
            &input,
            true,
            window,
            cx,
        );
        let configured = self.has_linear_key(workspace_id);
        let saving = self.saving_keys.contains(workspace_id);
        let status = if saving {
            "Checking the key with Linear…".to_string()
        } else if configured {
            match self.key_accounts.get(workspace_id) {
                Some(account) => format!("Saved · {account}"),
                None => "Saved. Paste a new key to replace it.".to_string(),
            }
        } else if is_default {
            "Not set. Linear → Settings → Security & access → Personal API keys.".to_string()
        } else {
            "Not set: this workspace's projects use the Personal workspace's key. Linear → Settings → Security & access → Personal API keys.".to_string()
        };
        let save_workspace = workspace_id.to_string();
        let remove_workspace = workspace_id.to_string();
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
                SharedString::from(format!("workspace-linear-save-{workspace_id}")),
                "Save",
                None,
                ButtonVariant::Outline,
                saving || draft.trim().is_empty(),
                None,
                move |page: &mut Self, _window, cx| {
                    page.save_linear_key(save_workspace.clone(), cx)
                },
                cx,
            ))
            .when(configured && !saving, |row| {
                row.child(settings_button(
                    p,
                    SharedString::from(format!("workspace-linear-remove-{workspace_id}")),
                    "Remove",
                    None,
                    ButtonVariant::Ghost,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.remove_linear_key(remove_workspace.clone(), cx)
                    },
                    cx,
                ))
            });
        setting_row(
            p,
            input_id,
            RowSpec::new("Linear API key").description(status),
            None,
            control.into_any_element(),
            cx,
        )
    }

    /// A Work workspace's team rows: Convex, Slack and the team's Linear key.
    fn connection_rows(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(workspace) = self.workspace(workspace_id).cloned() else {
            return Vec::new();
        };
        if text(&workspace, "kind") != "work" {
            return Vec::new();
        }
        self.team_rows(p, workspace_id, &text(&workspace, "name"), window, cx)
    }

    fn create_workspace(&mut self, cx: &mut Context<Self>) {
        self.write(
            "/api/createWorkspace",
            json!({ "name": "New workspace", "kind": "work" }),
            "Couldn't create the workspace",
            cx,
        );
    }
}

impl Render for WorkspacesTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Workspaces),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Workspaces,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Workspaces, &p, blocks, cx);
        }
        let intro = static_note(
            &p,
            "Each workspace has its own projects, Spaces, Linear key, agent instructions, Claude accounts and browser sign-ins. Switch workspaces with the button left of your Spaces; each window shows one workspace.",
            false,
        );
        let new_button = settings_button(
            &p,
            "workspace-new",
            "New workspace",
            Some("modals/settings/plus.svg"),
            ButtonVariant::Outline,
            !self.rpc_available(cx),
            None,
            |page: &mut Self, _window, cx| page.create_workspace(cx),
            cx,
        );
        blocks.push(PageBlock::plain(
            h_flex()
                .w_full()
                .gap(px(12.0))
                .items_center()
                .child(div().flex_1().min_w_0().child(intro))
                .child(new_button),
        ));
        let unreachable = self.error.as_deref().is_some_and(is_service_unreachable);
        if unreachable {
            blocks.push(PageBlock::plain(service_unreachable_state(
                &p,
                "workspaces-unreachable",
                |page: &mut Self, _window, cx| page.reload(cx),
                cx,
            )));
        } else if let Some(error) = self.error.clone() {
            blocks.push(PageBlock::plain(static_note(&p, error, true)));
        }
        // What follows needs the service to save: dimmed while it does not answer.
        let first_control_block = blocks.len();
        for workspace in self.ordered_workspaces() {
            let anchor = format!("workspace-{}", text(&workspace, "workspaceId"));
            if let Some(section) = self.workspace_section(&p, &workspace, window, cx) {
                blocks.push(PageBlock::section(anchor.clone(), section));
            }
            if text(&workspace, "kind") == "work" {
                let workspace_id = text(&workspace, "workspaceId");
                let name = text(&workspace, "name");
                if let Some(section) = self.slack_flow_section(&p, &workspace_id, &name, window, cx)
                {
                    blocks.push(PageBlock::section(format!("{anchor}-team-flow"), section));
                }
                if let Some(section) = self.flow_steps_section(&p, &workspace_id, &name, window, cx)
                {
                    blocks.push(PageBlock::section(format!("{anchor}-flow-steps"), section));
                }
            }
        }
        if unreachable {
            for block in &mut blocks[first_control_block..] {
                let element = std::mem::replace(&mut block.element, gpui::Empty.into_any_element());
                block.element = greyed_out(element);
            }
        }
        settings_page(&self.store, SettingsTabId::Workspaces, &p, blocks, cx)
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn saved_remote_machines(machines: &Value) -> Vec<(String, String)> {
    machines
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|machine| {
            let id = machine
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())?
                .to_string();
            let name = machine
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .or_else(|| {
                    machine
                        .get("sshHost")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|host| !host.is_empty())
                })
                .unwrap_or(id.as_str())
                .to_string();
            Some((id, name))
        })
        .collect()
}

/// A workspace icon as the sidebar draws it (`workspace_badge` in native_sidebar/workspace_tile.rs,
/// which the native-modal demo this tab also builds into does not have): the icon in white on a
/// disc of the workspace's colour. The icon ids are their own `assets/titlebar/` file names.
fn icon_disc(icon: &str, color: gpui::Hsla, size: f32) -> gpui::AnyElement {
    div()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(color)
        .child(
            gpui::svg()
                .path(SharedString::from(format!("titlebar/{icon}.svg")))
                .size(px((size * 0.62).round()))
                .text_color(gpui::white()),
        )
        .into_any_element()
}

fn hex(color: &str) -> gpui::Hsla {
    u32::from_str_radix(color.trim_start_matches('#'), 16)
        .map(rgb)
        .map(gpui::Hsla::from)
        .unwrap_or(gpui::Hsla::from(rgb(0x808080)))
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
