//! A workspace's Primary tracker row: Linear tickets & projects, or GitHub issues & projects, and
//! (while `gh` lacks the `read:project` scope) the command that lets Ghostex read GitHub Projects.
//! It reads `/api/readWorkTracker` and writes through `/api/setWorkTracker`.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: "in settings we need to say what is the primary for that workspace (Linear Tickets &
//! Projects or Github Issues & Projects - Need to pick just 1)", and the GitHub Projects scope
//! command is shown "also in settings". A team workspace's choice is the team's (owners only).
//! SEE-ALSO: server/src/work_mode/tracker.rs, server/src/work_mode/github_projects.rs,
//! server/src/server/route_http/work_tracker.rs.
use super::super::super::store::{store_copy_to_clipboard, store_gxserver_rpc};
use super::*;

/// A team workspace's tracker is read from (and saved to) the team's Convex project.
const TRACKER_TIMEOUT: Duration = Duration::from_secs(30);

/// What Settings knows about one workspace's tracker.
#[derive(Default)]
pub(crate) struct TrackerState {
    loading: bool,
    /// The last `/api/readWorkTracker` answer.
    answer: Option<Value>,
    error: Option<String>,
    saving: bool,
}

impl WorkspacesTab {
    fn ensure_tracker_loaded(&mut self, workspace_id: &str, cx: &mut Context<Self>) {
        let state = self.trackers.entry(workspace_id.to_string()).or_default();
        if state.loading || state.answer.is_some() || state.error.is_some() {
            return;
        }
        self.load_tracker(workspace_id.to_string(), cx);
    }

    fn load_tracker(&mut self, workspace_id: String, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            return;
        }
        self.trackers
            .entry(workspace_id.clone())
            .or_default()
            .loading = true;
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readWorkTracker",
            json!({ "workspaceId": workspace_id }),
            TRACKER_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let state = page.trackers.entry(workspace_id).or_default();
                    state.loading = false;
                    match result {
                        Ok(answer) => {
                            state.answer = Some(answer);
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

    fn set_tracker(&mut self, workspace_id: String, tracker: String, cx: &mut Context<Self>) {
        let state = self.trackers.entry(workspace_id.clone()).or_default();
        if state.saving {
            return;
        }
        state.saving = true;
        // Shown at once; the read after the write settles it.
        if let Some(answer) = state.answer.as_mut() {
            answer["tracker"] = json!(tracker);
        }
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/setWorkTracker",
            json!({ "workspaceId": workspace_id, "tracker": tracker }),
            TRACKER_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.trackers
                        .entry(workspace_id.clone())
                        .or_default()
                        .saving = false;
                    if let Err(error) = result {
                        page.toast("error", "Couldn't change the tracker", &error, cx);
                    }
                    page.load_tracker(workspace_id, cx);
                });
            },
            cx,
        );
    }

    /// The Primary tracker row, and the GitHub Projects scope row while `gh` lacks it.
    pub(super) fn tracker_rows(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.ensure_tracker_loaded(workspace_id, cx);
        let (answer, error, saving) = {
            let state = self.trackers.entry(workspace_id.to_string()).or_default();
            (state.answer.clone(), state.error.clone(), state.saving)
        };
        let row_id = format!("workspace-tracker-{workspace_id}");
        let Some(answer) = answer else {
            let note = error.unwrap_or_else(|| "Reading…".to_string());
            return vec![setting_row(
                p,
                row_id,
                RowSpec::new("Primary tracker").description(note),
                None,
                div().into_any_element(),
                cx,
            )];
        };
        let tracker = answer["tracker"].as_str().unwrap_or("linear").to_string();
        let team = answer["team"].as_bool() == Some(true);
        let can_edit = answer["canEdit"].as_bool() != Some(false);
        let source = answer["source"].as_str().unwrap_or_default();
        let mut description = String::from(
            "Pick one. Create ticket, the Issues view, Link to and the cards use it, and so does the Slack flow for a team.",
        );
        if team && !can_edit {
            description.push_str(" Only the team's owners can change it.");
        } else if team {
            description.push_str(" It is set for the whole team.");
        } else if source == "default" {
            description.push_str(if tracker == "linear" {
                " Not picked yet, so Linear (a Linear key is set)."
            } else {
                " Not picked yet, so GitHub (no Linear key is set)."
            });
        }
        let options = [
            SettingOption {
                label: "Linear tickets & projects".into(),
                value: "linear".into(),
            },
            SettingOption {
                label: "GitHub issues & projects".into(),
                value: "github".into(),
            },
        ];
        let control = if can_edit && !saving {
            let target = workspace_id.to_string();
            settings_segmented(
                p,
                &row_id,
                &options,
                Some(tracker.as_str()),
                move |page: &mut Self, value, _window, cx| {
                    page.set_tracker(target.clone(), value, cx);
                },
                cx,
            )
        } else {
            static_note(
                p,
                options
                    .iter()
                    .find(|option| option.value == tracker)
                    .map(|option| option.label.clone())
                    .unwrap_or_default(),
                false,
            )
        };
        let mut rows = vec![setting_row(
            p,
            row_id,
            RowSpec::new("Primary tracker").description(description),
            None,
            control,
            cx,
        )];
        if tracker == "github"
            && answer
                .pointer("/githubProjects/access")
                .and_then(Value::as_str)
                == Some("missingScope")
        {
            let command = answer
                .pointer("/githubProjects/command")
                .and_then(Value::as_str)
                .unwrap_or("gh auth refresh -s read:project")
                .to_string();
            // Run in terminal's id, from a gxserver that offers it (server/src/shown_commands/).
            let run_in_terminal = answer
                .pointer("/githubProjects/runInTerminal")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string);
            let description = format!(
                "To show GitHub Projects, run `{command}` in a terminal, then reopen this page."
            );
            rows.push(setting_row(
                p,
                format!("workspace-github-projects-{workspace_id}"),
                RowSpec::new("GitHub Projects").description(description),
                None,
                h_flex()
                    .gap(px(12.0))
                    .items_center()
                    .child(static_note(p, format!("Run {command}"), false))
                    .child(settings_button(
                        p,
                        SharedString::from(format!(
                            "workspace-github-projects-copy-{workspace_id}"
                        )),
                        "Copy command",
                        Some("modals/settings/copy.svg"),
                        ButtonVariant::Outline,
                        false,
                        None,
                        {
                            let command = command.clone();
                            move |page: &mut Self, _window, cx| {
                                store_copy_to_clipboard(&page.store, command.clone(), cx);
                            }
                        },
                        cx,
                    ))
                    .when_some(run_in_terminal, |row, command_id| {
                        row.child(settings_button(
                            p,
                            SharedString::from(format!(
                                "workspace-github-projects-run-{workspace_id}"
                            )),
                            "Run in terminal",
                            None,
                            ButtonVariant::Default,
                            false,
                            None,
                            move |page: &mut Self, _window, cx| {
                                post_store_message(
                                    &page.store,
                                    json!({
                                        "type": "openShownCommandTerminal",
                                        "commandId": command_id,
                                    }),
                                    cx,
                                )
                            },
                            cx,
                        ))
                    })
                    .into_any_element(),
                cx,
            ));
        }
        rows
    }
}
