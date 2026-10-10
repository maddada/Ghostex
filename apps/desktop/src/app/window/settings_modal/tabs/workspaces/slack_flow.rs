//! A Work workspace's Team flow section (mockup 10): the Slack flow settings kept in the team's
//! Convex project. The working channel, the watch-only channels, which repo and Linear team new
//! work from each channel goes to, where new work runs, the default Linear team, and the team
//! instructions every session started from Slack gets. "Never work without a ticket" is built in
//! and shows as a locked switch. Reads `/api/readSlackFlowSettings`, writes `/api/setSlackFlowSettings`.
//!
//! CDXC:TeamSync 2026-10-09 DECISION:
//! User: team flow settings are "Owners only". Who may edit is decided by the team's Convex
//! functions alone (`canEditTeamFlow` in packages/team-sync/convex/teamFlow.ts, reported here as
//! `canEdit`); members see the rows read-only with one line saying only owners can change them.
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    ButtonVariant, RowSpec, setting_row, settings_button, settings_icon_button, settings_segmented,
    settings_text_input_with_disabled, settings_textarea, switch_control,
};
use super::super::super::store::store_gxserver_rpc;
use super::team::TEAM_TIMEOUT;
use super::*;
use gpui_component::v_flex;

/// The team instructions limit in `teamFlow:set`.
const MAX_INSTRUCTIONS_CHARS: usize = 50_000;

/// The Slack flow settings of one workspace's team.
#[derive(Default)]
pub(crate) struct SlackFlowState {
    loading: bool,
    /// `teamFlow:get` as last read or saved.
    settings: Option<Value>,
    error: Option<String>,
    saving: bool,
}

impl SlackFlowState {
    fn string(&self, key: &str) -> String {
        self.settings
            .as_ref()
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }
}

impl WorkspacesTab {
    pub(super) fn load_slack_flow(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        let state = self.slack_flows.entry(workspace_id.clone()).or_default();
        if state.loading {
            return;
        }
        state.loading = true;
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readSlackFlowSettings",
            json!({ "workspaceId": workspace_id }),
            TEAM_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let state = page.slack_flows.entry(workspace_id).or_default();
                    state.loading = false;
                    match result {
                        Ok(settings) => {
                            state.settings = Some(settings);
                            state.error = None;
                        }
                        Err(error) => state.error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Saves some `teamFlow:set` fields; `saved_drafts` are the inputs whose drafts the save
    /// settles.
    fn save_slack_flow(
        &mut self,
        workspace_id: String,
        fields: Value,
        saved_drafts: Vec<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let mut params = json!({ "workspaceId": workspace_id });
        if let (Some(params), Some(fields)) = (params.as_object_mut(), fields.as_object()) {
            params.extend(fields.clone());
        }
        let state = self.slack_flows.entry(workspace_id.clone()).or_default();
        if state.saving {
            return;
        }
        state.saving = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/setSlackFlowSettings",
            params,
            TEAM_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let state = page.slack_flows.entry(workspace_id).or_default();
                    state.saving = false;
                    match result {
                        Ok(settings) => {
                            state.settings = Some(settings);
                            for id in &saved_drafts {
                                page.drafts.remove(id);
                            }
                        }
                        Err(error) => {
                            page.toast("error", "Couldn't save the team flow", &error, cx)
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// The Team flow section of a connected Work workspace, or a note while it loads.
    pub(super) fn slack_flow_section(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        workspace_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self
            .team
            .get(workspace_id)
            .is_some_and(|team| team.connected())
        {
            return None;
        }
        let (loaded, error, saving, can_edit) = {
            let state = self
                .slack_flows
                .entry(workspace_id.to_string())
                .or_default();
            (
                state.settings.is_some(),
                state.error.clone(),
                state.saving,
                state
                    .settings
                    .as_ref()
                    .and_then(|settings| settings.get("canEdit"))
                    .and_then(Value::as_bool)
                    // Functions deployed before `canEdit` let every member edit.
                    .unwrap_or(true),
            )
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        if !loaded {
            rows.push(setting_row(
                p,
                format!("team-flow-loading-{workspace_id}"),
                RowSpec::new("Team flow")
                    .readout(if error.is_some() {
                        "Couldn't read it"
                    } else {
                        "Reading…"
                    })
                    .description(match error {
                        Some(error) => format!("Couldn't read the team flow: {error}"),
                        None => "Reading your team's flow…".to_string(),
                    }),
                None,
                div().into_any_element(),
                cx,
            ));
        } else {
            let locked = saving || !can_edit;
            let reason = (!can_edit)
                .then(|| SharedString::from("Only the team's owners can change the team flow."));
            if !can_edit {
                rows.push(owners_only_row(
                    p,
                    format!("team-flow-owners-{workspace_id}"),
                    cx,
                ));
            }
            rows.push(self.channel_row(
                p,
                workspace_id,
                "workingChannelId",
                "Working channel",
                "Each ticket gets exactly one working thread here, and all the work on it happens in that thread. A Slack channel ID (channel details → About → Channel ID).",
                locked,
                window,
                cx,
            ));
            rows.push(self.channel_row(
                p,
                workspace_id,
                "watchOnlyChannelIds",
                "Watch-only channels",
                "A request here is forwarded to the ticket's working thread; Ghostex only reacts 👀 and posts nothing else. Channel IDs, separated by commas.",
                locked,
                window,
                cx,
            ));
            rows.extend(self.channel_repo_rows(p, workspace_id, locked, window, cx));
            rows.push(self.run_place_row(p, workspace_id, locked, reason.clone(), cx));
            rows.push(setting_row(
                p,
                format!("team-flow-ticket-{workspace_id}"),
                RowSpec::new("Never work without a ticket").description(
                    "Ghostex finds the ticket in the thread or creates one in Linear before any session starts. Always on.",
                ),
                None,
                switch_control(
                    p,
                    SharedString::from(format!("team-flow-ticket-switch-{workspace_id}")),
                    "Never work without a ticket",
                    true,
                    true,
                    Some("Always on".into()),
                    |_: &mut Self, _, _, _| {},
                    cx,
                ),
                cx,
            ));
            rows.push(self.channel_row(
                p,
                workspace_id,
                "linearTeamKey",
                "New tickets go to",
                "The Linear team key (for example SPX) for tickets Ghostex creates when a channel has no team of its own.",
                locked,
                window,
                cx,
            ));
            rows.push(self.instructions_row(p, workspace_id, locked, window, cx));
        }
        let title = if workspace_name.is_empty() {
            "Team flow".to_string()
        } else {
            format!("{workspace_name} · Team flow")
        };
        settings_section(
            p,
            title,
            Some(SharedString::from(
                "Your Slack bot's rules, kept in your team's Convex project so every teammate's Ghostex follows them.",
            )),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// A one-line setting with a Save button: a channel, the channel list (comma separated) or the
    /// Linear team key.
    #[allow(clippy::too_many_arguments)]
    fn channel_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        key: &'static str,
        label: &'static str,
        description: &'static str,
        locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let saved = {
            let state = self
                .slack_flows
                .entry(workspace_id.to_string())
                .or_default();
            match state
                .settings
                .as_ref()
                .and_then(|settings| settings.get(key))
            {
                Some(Value::Array(items)) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => state.string(key),
            }
        };
        let input_id = SharedString::from(format!("team-flow-{key}-{workspace_id}"));
        let placeholder = match key {
            "watchOnlyChannelIds" => "C0123456789, C0987654321",
            "linearTeamKey" => "SPX",
            _ => "C0123456789",
        };
        let input = self.draft_input(&input_id, &saved, placeholder, window, cx);
        let changed = self.draft_changed(&input_id, &saved);
        let save_workspace = workspace_id.to_string();
        let save_id = input_id.clone();
        let control = h_flex()
            .gap(px(8.0))
            .child(settings_text_input_with_disabled(
                p,
                &input,
                Some(220.0),
                true,
                locked,
                window,
                cx,
            ))
            .child(settings_button(
                p,
                SharedString::from(format!("team-flow-{key}-save-{workspace_id}")),
                "Save",
                None,
                ButtonVariant::Outline,
                locked || !changed,
                None,
                move |page: &mut Self, _window, cx| {
                    let typed = page.draft(&save_id);
                    let value = if key == "watchOnlyChannelIds" {
                        json!(
                            typed
                                .split([',', ' ', '\n'])
                                .map(str::trim)
                                .filter(|channel| !channel.is_empty())
                                .collect::<Vec<_>>()
                        )
                    } else if typed.trim().is_empty() {
                        Value::Null
                    } else {
                        json!(typed.trim())
                    };
                    page.save_slack_flow(
                        save_workspace.clone(),
                        json!({ key: value }),
                        vec![save_id.clone()],
                        cx,
                    );
                },
                cx,
            ));
        setting_row(
            p,
            input_id,
            RowSpec::new(label).description(description),
            None,
            control.into_any_element(),
            cx,
        )
    }

    /// One row per mapped channel, then the row that adds one.
    fn channel_repo_rows(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mappings: Vec<Value> = self
            .slack_flows
            .get(workspace_id)
            .and_then(|state| state.settings.as_ref())
            .and_then(|settings| settings.get("channelRepos"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut rows = Vec::new();
        for mapping in &mappings {
            let channel = text(mapping, "channelId");
            let repo = text(mapping, "repo");
            let team = text(mapping, "linearTeamKey");
            let target = workspace_id.to_string();
            let removed = channel.clone();
            rows.push(setting_row(
                p,
                format!("team-flow-repo-{workspace_id}-{channel}"),
                RowSpec::new(format!("#{channel}"))
                    .dependent()
                    .readout(format!(
                        "{} · Linear team {}",
                        if repo.is_empty() {
                            "ask each time"
                        } else {
                            &repo
                        },
                        if team.is_empty() { "default" } else { &team },
                    )),
                None,
                settings_icon_button(
                    p,
                    SharedString::from(format!("team-flow-repo-remove-{workspace_id}-{channel}")),
                    "modals/settings/trash.svg",
                    16.0,
                    28.0,
                    ButtonVariant::Ghost,
                    Some("Remove".into()),
                    locked,
                    move |page: &mut Self, _window, cx| {
                        page.save_slack_flow(
                            target.clone(),
                            json!({ "unmapChannel": removed }),
                            Vec::new(),
                            cx,
                        );
                    },
                    cx,
                ),
                cx,
            ));
        }

        let channel_id = SharedString::from(format!("team-flow-map-channel-{workspace_id}"));
        let repo_id = SharedString::from(format!("team-flow-map-repo-{workspace_id}"));
        let team_id = SharedString::from(format!("team-flow-map-team-{workspace_id}"));
        let channel_input = self.draft_input(&channel_id, "", "Channel ID", window, cx);
        let repo_input = self.draft_input(&repo_id, "", "owner/repo", window, cx);
        let team_input = self.draft_input(&team_id, "", "Linear team", window, cx);
        let can_add = !self.draft(&channel_id).trim().is_empty();
        let target = workspace_id.to_string();
        let ids = (channel_id.clone(), repo_id.clone(), team_id.clone());
        let control = h_flex()
            .w_full()
            .gap(px(8.0))
            .child(settings_text_input_with_disabled(
                p,
                &channel_input,
                Some(150.0),
                true,
                locked,
                window,
                cx,
            ))
            .child(settings_text_input_with_disabled(
                p,
                &repo_input,
                None,
                true,
                locked,
                window,
                cx,
            ))
            .child(settings_text_input_with_disabled(
                p,
                &team_input,
                Some(110.0),
                true,
                locked,
                window,
                cx,
            ))
            .child(settings_button(
                p,
                SharedString::from(format!("team-flow-map-add-{workspace_id}")),
                "Add",
                Some("modals/settings/plus.svg"),
                ButtonVariant::Outline,
                locked || !can_add,
                None,
                move |page: &mut Self, _window, cx| {
                    let (channel_id, repo_id, team_id) = ids.clone();
                    let optional = |text: String| {
                        let text = text.trim().to_string();
                        if text.is_empty() {
                            Value::Null
                        } else {
                            json!(text)
                        }
                    };
                    let mapping = json!({
                        "channelId": page.draft(&channel_id).trim(),
                        "repo": optional(page.draft(&repo_id)),
                        "linearTeamKey": optional(page.draft(&team_id)),
                    });
                    page.save_slack_flow(
                        target.clone(),
                        json!({ "mapChannel": mapping }),
                        vec![channel_id, repo_id, team_id],
                        cx,
                    );
                },
                cx,
            ));
        rows.insert(
            0,
            setting_row(
                p,
                channel_id,
                RowSpec::new("Repos for new work")
                    .description(if mappings.is_empty() {
                        "Which repo (owner/name) and Linear team new work from each Slack channel goes to. Leave the repo empty to ask each time."
                    } else {
                        "Which repo (owner/name) and Linear team new work from each Slack channel goes to. Adding a mapped channel again replaces it."
                    })
                    .wide(),
                None,
                control.into_any_element(),
                cx,
            ),
        );
        rows
    }

    fn run_place_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        locked: bool,
        reason: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let place = self
            .slack_flows
            .get(workspace_id)
            .map(|state| state.string("defaultRunPlace"))
            .filter(|place| !place.is_empty())
            .unwrap_or_else(|| "cloud".to_string());
        let target = workspace_id.to_string();
        let control = settings_segmented(
            p,
            &format!("team-flow-run-place-{workspace_id}"),
            &[
                SettingOption {
                    label: "Cloud".into(),
                    value: "cloud".into(),
                },
                SettingOption {
                    label: "This computer".into(),
                    value: "local".into(),
                },
            ],
            Some(place.as_str()),
            move |page: &mut Self, value, _window, cx| {
                if locked {
                    return;
                }
                page.save_slack_flow(
                    target.clone(),
                    json!({ "defaultRunPlace": value }),
                    Vec::new(),
                    cx,
                );
            },
            cx,
        );
        setting_row(
            p,
            format!("team-flow-run-place-{workspace_id}"),
            RowSpec::new("Where new work runs")
                .description(
                    "For @Ghostex in Slack without \"cloud\" or \"local\". Start chat in Ghostex always starts on this computer.",
                )
                .disabled_reason(reason),
            None,
            div()
                .when(locked, |this| this.opacity(0.5))
                .child(control)
                .into_any_element(),
            cx,
        )
    }

    fn instructions_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let saved = self
            .slack_flows
            .get(workspace_id)
            .map(|state| state.string("instructions"))
            .unwrap_or_default();
        let input_id = SharedString::from(format!("team-flow-instructions-{workspace_id}"));
        let shown = self
            .drafts
            .get(&input_id)
            .cloned()
            .unwrap_or_else(|| saved.clone());
        let draft_id = input_id.clone();
        let input = FieldStates::textarea_state(
            self,
            &input_id,
            &shown,
            Some("## Communication\n- Post a short update as each milestone lands…"),
            (6, 24),
            move |page: &mut Self, text, _window, cx| {
                page.drafts.insert(draft_id.clone(), text);
                cx.notify();
            },
            window,
            cx,
        );
        let changed = self.draft_changed(&input_id, &saved);
        let too_long = shown.chars().count() > MAX_INSTRUCTIONS_CHARS;
        let save_workspace = workspace_id.to_string();
        let save_id = input_id.clone();
        let discard_id = input_id.clone();
        let control = v_flex()
            .w_full()
            .gap(px(8.0))
            .child(settings_textarea(
                p, &input, 140.0, true, locked, window, cx,
            ))
            .child(
                h_flex()
                    .gap(px(8.0))
                    .child(settings_button(
                        p,
                        SharedString::from(format!("team-flow-instructions-save-{workspace_id}")),
                        "Save instructions",
                        None,
                        ButtonVariant::Outline,
                        locked || !changed || too_long,
                        too_long.then(|| {
                            SharedString::from(format!(
                                "At most {MAX_INSTRUCTIONS_CHARS} characters."
                            ))
                        }),
                        move |page: &mut Self, _window, cx| {
                            let typed = page.draft(&save_id);
                            let value = if typed.trim().is_empty() {
                                Value::Null
                            } else {
                                json!(typed)
                            };
                            page.save_slack_flow(
                                save_workspace.clone(),
                                json!({ "instructions": value }),
                                vec![save_id.clone()],
                                cx,
                            );
                        },
                        cx,
                    ))
                    .when(changed, |row| {
                        row.child(settings_button(
                            p,
                            SharedString::from(format!(
                                "team-flow-instructions-discard-{workspace_id}"
                            )),
                            "Discard",
                            None,
                            ButtonVariant::Ghost,
                            false,
                            None,
                            move |page: &mut Self, _window, cx| {
                                page.drafts.remove(&discard_id);
                                page.fields.textareas.remove(&discard_id);
                                cx.notify();
                            },
                            cx,
                        ))
                    }),
            );
        setting_row(
            p,
            input_id,
            RowSpec::new("Team instructions")
                .description(
                    "Added to every session Ghostex starts from Slack in this workspace: how to report, ask questions, record demo videos, package for QC. Stored in your team's Convex project, so everyone's sessions follow the same rules.",
                )
                .wide(),
            None,
            control.into_any_element(),
            cx,
        )
    }
}

/// The one line members see above the team flow's read-only rows.
pub(super) fn owners_only_row(
    p: &SettingsPalette,
    id: String,
    cx: &mut Context<WorkspacesTab>,
) -> AnyElement {
    setting_row(
        p,
        id,
        RowSpec::new("Only the team's owners can change the team flow")
            .description("You see your team's flow as its owners set it."),
        None,
        div().into_any_element(),
        cx,
    )
}
