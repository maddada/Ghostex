//! The right-hand detail pane: `AutomationDefinitionDetail` and `AutomationRunDetail`.

use super::drafts::{describe_mode, describe_schedule, format_short_date, run_status_label};
use super::lists::agent_icon;
use super::model::{AutomationExecutionMode, AutomationRun, AutomationState};
use super::style::{
    AutomatePalette, CARD_RADIUS, ICON_ARCHIVE, ICON_BELL, ICON_COPY, ICON_EXTERNAL,
    ICON_FOLDER_OPEN, ICON_PENCIL, ICON_PLAY, ICON_TRASH, detail_row, empty_state, group_card,
    heading, icon_button, section_label, truncated,
};
use super::view::NativeAutomateView;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};

fn text(value: impl Into<String>) -> AnyElement {
    truncated(value.into()).into_any_element()
}

/// The prompt or run summary in its own card, wrapped and selectable-looking like the React `<p>`.
fn body_card(p: &AutomatePalette, body: String) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .p(px(16.0))
        .rounded(px(CARD_RADIUS))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .text_size(px(14.0))
        .line_height(px(22.0))
        .text_color(p.foreground.opacity(0.9))
        .child(body)
        .into_any_element()
}

fn section(p: &AutomatePalette, label: &'static str, card: AnyElement) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(10.0))
        .child(section_label(p, label))
        .child(card)
        .into_any_element()
}

impl NativeAutomateView {
    fn detail_frame(&self, id: &'static str, children: Vec<AnyElement>) -> AnyElement {
        div()
            .id(id)
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.detail_scroll)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(672.0))
                    .mx_auto()
                    .p(px(24.0))
                    .gap(px(24.0))
                    .children(children),
            )
            .into_any_element()
    }

    fn copy_button(
        &self,
        p: &AutomatePalette,
        id: &'static str,
        label: &'static str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        icon_button(
            p,
            id,
            ICON_COPY,
            label,
            false,
            move |this: &mut Self, _window, cx| this.copy_text(&value, cx),
            cx,
        )
        .into_any_element()
    }

    pub(crate) fn render_automation_detail(
        &self,
        p: &AutomatePalette,
        state: &AutomationState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(automation) = state.automations.get(self.selected_automation_index(state)) else {
            return empty_state(
                p,
                super::style::ICON_CLOCK,
                "No automation selected",
                "Select an automation from the list to see its schedule, prompt, and recent runs.",
                None,
            );
        };
        let busy = self.busy_id.as_deref() == Some(automation.id.as_str());
        let agent = state.agent(&automation.agent_id);
        let (run_target, edit_target, delete_target) =
            (automation.clone(), automation.clone(), automation.clone());
        let actions = h_flex()
            .flex_shrink_0()
            .gap(px(6.0))
            .child(icon_button(
                p,
                "automate-detail-run",
                ICON_PLAY,
                "Run automation now",
                busy,
                move |this: &mut Self, _window, cx| this.run_now(&run_target, cx),
                cx,
            ))
            .child(icon_button(
                p,
                "automate-detail-edit",
                ICON_PENCIL,
                "Edit automation",
                false,
                move |this: &mut Self, _window, cx| this.open_edit_dialog(&edit_target, cx),
                cx,
            ))
            .child(icon_button(
                p,
                "automate-detail-delete",
                ICON_TRASH,
                "Delete automation",
                busy,
                move |this: &mut Self, _window, cx| this.delete(&delete_target, cx),
                cx,
            ));
        let header = h_flex()
            .w_full()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .child(heading(
                p,
                if automation.enabled {
                    "Active"
                } else {
                    "Paused"
                },
                if automation.enabled {
                    p.accent
                } else {
                    p.muted
                },
                automation.name.clone(),
            ))
            .child(actions)
            .into_any_element();
        let mut rows = Vec::new();
        if self.is_all_projects()
            && let Some(project) = automation.project_id().and_then(|id| state.project(id))
        {
            rows.push(detail_row(p, "Project", vec![text(project.label.clone())]));
        }
        rows.push(detail_row(
            p,
            "Schedule",
            vec![text(describe_schedule(&automation.schedule))],
        ));
        rows.push(detail_row(
            p,
            "Next run",
            vec![text(match automation.next_run_at.as_deref() {
                Some(next) => format_short_date(Some(next)),
                None => "Not scheduled".to_string(),
            })],
        ));
        let mut agent_value: Vec<AnyElement> = agent_icon(agent).into_iter().collect();
        agent_value.push(text(
            agent
                .map(|agent| agent.display_label().to_string())
                .unwrap_or_else(|| automation.agent_id.clone()),
        ));
        rows.push(detail_row(p, "Agent", agent_value));
        let launch_choice = [automation.model.as_deref(), automation.effort.as_deref()]
            .into_iter()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>();
        if !launch_choice.is_empty() {
            rows.push(detail_row(
                p,
                "Model",
                vec![text(launch_choice.join(" \u{b7} "))],
            ));
        }
        rows.push(detail_row(
            p,
            "Mode",
            vec![text(describe_mode(&automation.execution_mode))],
        ));
        match &automation.execution_mode {
            AutomationExecutionMode::Worktree {
                setup_command: Some(command),
            } if !command.trim().is_empty() => {
                rows.push(detail_row(p, "Setup", vec![text(command.clone())]));
            }
            AutomationExecutionMode::Thread {
                agent_session_id,
                session_id,
                expires_at,
            } => {
                rows.push(detail_row(
                    p,
                    "Thread",
                    vec![text(
                        agent_session_id
                            .clone()
                            .or_else(|| session_id.clone())
                            .unwrap_or_default(),
                    )],
                ));
                if let Some(expires_at) = expires_at {
                    rows.push(detail_row(
                        p,
                        "Expires",
                        vec![text(format_short_date(Some(expires_at)))],
                    ));
                }
            }
            _ => {}
        }
        let recent: Vec<AnyElement> = state
            .runs
            .iter()
            .filter(|run| run.automation_id == automation.id)
            .take(5)
            .map(|run| {
                h_flex()
                    .w_full()
                    .min_h(px(44.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .text_size(px(14.0))
                    .child(
                        div()
                            .text_color(p.run_status_color(&run.status))
                            .child(run_status_label(&run.status)),
                    )
                    .child(div().text_color(p.muted).child(format_short_date(Some(
                        run.completed_at.as_deref().unwrap_or(&run.created_at),
                    ))))
                    .into_any_element()
            })
            .collect();
        let recent = if recent.is_empty() {
            vec![
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .text_size(px(14.0))
                    .text_color(p.muted)
                    .child("No runs yet.")
                    .into_any_element(),
            ]
        } else {
            recent
        };
        self.detail_frame(
            "automate-automation-detail",
            vec![
                header,
                body_card(p, automation.prompt.clone()),
                section(p, "Details", group_card(p, rows).into_any_element()),
                section(p, "Recent runs", group_card(p, recent).into_any_element()),
            ],
        )
    }

    pub(crate) fn render_run_detail(
        &self,
        p: &AutomatePalette,
        state: &AutomationState,
        run: Option<&AutomationRun>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(run) = run else {
            return empty_state(
                p,
                ICON_BELL,
                "No run selected",
                "Select a run from the list to review its status, summary, and linked session.",
                None,
            );
        };
        let automation = state
            .automations
            .iter()
            .find(|automation| automation.id == run.automation_id);
        let agent = automation.and_then(|automation| state.agent(&automation.agent_id));
        let busy = self.busy_id.as_deref() == Some(run.id.as_str());
        let mut actions = h_flex().flex_shrink_0().gap(px(6.0));
        if run.session_id().is_some() {
            let target = run.clone();
            actions = actions.child(icon_button(
                p,
                "automate-run-detail-session",
                ICON_EXTERNAL,
                "Open automation session",
                busy,
                move |this: &mut Self, _window, cx| this.open_run_session(&target, cx),
                cx,
            ));
        }
        if run.worktree().is_some() {
            let target = run.clone();
            actions = actions.child(icon_button(
                p,
                "automate-run-detail-worktree",
                ICON_FOLDER_OPEN,
                "Open automation worktree",
                busy,
                move |this: &mut Self, _window, cx| this.open_run_worktree(&target, cx),
                cx,
            ));
        }
        if run.is_unread {
            let target = run.clone();
            actions = actions.child(icon_button(
                p,
                "automate-run-detail-read",
                ICON_BELL,
                "Mark run read",
                busy,
                move |this: &mut Self, _window, cx| this.mark_run_read(&target, cx),
                cx,
            ));
        }
        let target = run.clone();
        actions = actions.child(icon_button(
            p,
            "automate-run-detail-archive",
            ICON_ARCHIVE,
            "Archive run",
            busy || run.is_active(),
            move |this: &mut Self, _window, cx| this.archive_run(&target, cx),
            cx,
        ));
        let header = h_flex()
            .w_full()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .child(heading(
                p,
                run_status_label(&run.status),
                p.run_status_color(&run.status),
                automation
                    .map(|automation| automation.name.clone())
                    .unwrap_or_else(|| run.automation_id.clone()),
            ))
            .child(actions)
            .into_any_element();
        let project_name = state
            .project(&run.project_id)
            .map(|project| project.label.clone())
            .unwrap_or_else(|| state.project_name.clone());
        let mut agent_value: Vec<AnyElement> = agent_icon(agent).into_iter().collect();
        agent_value.push(text(
            agent
                .map(|agent| agent.display_label().to_string())
                .or_else(|| automation.map(|automation| automation.agent_id.clone()))
                .unwrap_or_else(|| "Unknown agent".to_string()),
        ));
        let mut rows = vec![
            detail_row(p, "Project", vec![text(project_name)]),
            detail_row(p, "Agent", agent_value),
            detail_row(
                p,
                "Created",
                vec![text(format_short_date(Some(&run.created_at)))],
            ),
            detail_row(
                p,
                "Completed",
                vec![text(match run.completed_at.as_deref() {
                    Some(completed) => format_short_date(Some(completed)),
                    None => "Still running".to_string(),
                })],
            ),
        ];
        if let Some(session_id) = run.session_id() {
            rows.push(detail_row(
                p,
                "Session",
                vec![
                    text(session_id.to_string()),
                    self.copy_button(
                        p,
                        "automate-copy-session",
                        "Copy automation session id",
                        session_id.to_string(),
                        cx,
                    ),
                ],
            ));
        }
        if let Some(worktree) = run.worktree() {
            rows.push(detail_row(
                p,
                "Branch",
                vec![
                    text(worktree.branch.clone()),
                    self.copy_button(
                        p,
                        "automate-copy-branch",
                        "Copy automation worktree branch",
                        worktree.branch.clone(),
                        cx,
                    ),
                ],
            ));
            rows.push(detail_row(
                p,
                "Worktree",
                vec![
                    text(worktree.path.clone()),
                    self.copy_button(
                        p,
                        "automate-copy-worktree",
                        "Copy automation worktree path",
                        worktree.path.clone(),
                        cx,
                    ),
                ],
            ));
        }
        self.detail_frame(
            "automate-run-detail",
            vec![
                header,
                body_card(p, run.summary().to_string()),
                section(p, "Details", group_card(p, rows).into_any_element()),
            ],
        )
    }
}
