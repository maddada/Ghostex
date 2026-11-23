//! The left-hand lists: automations (`AutomationDefinitionList`) and runs (`AutomationRunList`).

use super::drafts::{describe_schedule, format_short_date, run_status_label};
use super::model::{AutomationAgentOption, AutomationRun, AutomationState};
use super::style::{
    AutomatePalette, ICON_ARCHIVE, ICON_BELL, ICON_EXTERNAL, ICON_FOLDER_OPEN, ROW_RADIUS, icon,
    icon_button, status_dot, switch, truncated,
};
use super::view::NativeAutomateView;
use crate::app::helpers::{chrome_agent_icon_color, workspace_tab_agent_icon_path};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};

/// The agent's 14px glyph, as `AutomationAgentIcon` draws it.
pub(crate) fn agent_icon(agent: Option<&AutomationAgentOption>) -> Option<AnyElement> {
    let icon_id = agent?.icon_id()?;
    let path = workspace_tab_agent_icon_path(icon_id)?;
    let color: Hsla = chrome_agent_icon_color(icon_id).into();
    Some(icon(path, 14.0, color).into_any_element())
}

fn list_row(
    p: &AutomatePalette,
    id: (&'static str, usize),
    selected: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.hover;
    h_flex()
        .id(id)
        .w_full()
        .flex_shrink_0()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(10.0))
        .items_center()
        .rounded(px(ROW_RADIUS))
        .cursor_default()
        .when(selected, |this| this.bg(p.selected))
        .when(!selected, |this| this.hover(move |this| this.bg(hover)))
}

fn list_frame(id: &'static str, rows: Vec<AnyElement>, scroll: &gpui::ScrollHandle) -> AnyElement {
    v_flex()
        .id(id)
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .p(px(8.0))
        .gap(px(1.0))
        .children(rows)
        .into_any_element()
}

impl NativeAutomateView {
    pub(crate) fn selected_automation_index(&self, state: &AutomationState) -> usize {
        self.selected_automation_id
            .as_deref()
            .and_then(|id| state.automations.iter().position(|a| a.id == id))
            .unwrap_or(0)
    }

    pub(crate) fn render_automation_list(
        &self,
        p: &AutomatePalette,
        state: &AutomationState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_index = self.selected_automation_index(state);
        let all = self.is_all_projects();
        let rows = state
            .automations
            .iter()
            .enumerate()
            .map(|(index, automation)| {
                let unread = state
                    .runs
                    .iter()
                    .filter(|run| {
                        run.automation_id == automation.id && run.is_unread && !run.is_archived
                    })
                    .count();
                let busy = self.busy_id.as_deref() == Some(automation.id.as_str());
                let mut subtitle = Vec::new();
                if all
                    && let Some(project) = automation.project_id().and_then(|id| state.project(id))
                {
                    subtitle.push(project.label.clone());
                }
                subtitle.push(describe_schedule(&automation.schedule));
                subtitle.push(match automation.next_run_at.as_deref() {
                    Some(next) => format!("Next run {}", format_short_date(Some(next))),
                    None => "Not scheduled".to_string(),
                });
                let id = automation.id.clone();
                let unread_id = automation.id.clone();
                let unread_label = format!("View {unread} unread runs for {}", automation.name);
                let unread_bg = p.accent.opacity(0.14);
                let unread_hover = p.accent.opacity(0.24);
                let toggle = automation.clone();
                let enabled = automation.enabled;
                list_row(
                    p,
                    ("automate-automation-row", index),
                    index == selected_index,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.selected_automation_id = Some(id.clone());
                    cx.notify();
                }))
                .child(status_dot(if enabled {
                    p.success.opacity(0.8)
                } else {
                    p.foreground.opacity(0.2)
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap(px(8.0))
                                .items_center()
                                .children(agent_icon(state.agent(&automation.agent_id)))
                                .child(
                                    truncated(automation.name.clone())
                                        .text_size(px(14.0))
                                        .text_color(p.foreground),
                                )
                                .when(!enabled, |this| {
                                    this.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(12.0))
                                            .text_color(p.muted)
                                            .child("Paused"),
                                    )
                                })
                                .when(unread > 0, |this| {
                                    this.child(
                                        div()
                                            .id(("automate-automation-unread", index))
                                            .role(gpui::Role::Button)
                                            .aria_label(unread_label)
                                            .flex_shrink_0()
                                            .px(px(7.0))
                                            .py(px(1.0))
                                            .rounded(px(ROW_RADIUS))
                                            .bg(unread_bg)
                                            .text_size(px(12.0))
                                            .text_color(p.accent)
                                            .cursor_pointer()
                                            .hover(move |this| this.bg(unread_hover))
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _window, cx| {
                                                    cx.stop_propagation();
                                                    this.open_unread_runs_for_automation(
                                                        &unread_id, cx,
                                                    );
                                                },
                                            ))
                                            .child(format!("{unread} unread")),
                                    )
                                }),
                        )
                        .child(
                            truncated(subtitle.join(" \u{b7} "))
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .text_color(p.muted),
                        ),
                )
                .child(
                    div()
                        .id(("automate-automation-switch", index))
                        .role(gpui::Role::Switch)
                        .aria_label(if enabled {
                            "Pause automation"
                        } else {
                            "Enable automation"
                        })
                        .when(!busy, |this| {
                            this.cursor_pointer().on_click(cx.listener(
                                move |this, _: &ClickEvent, _window, cx| {
                                    cx.stop_propagation();
                                    this.set_enabled(&toggle, !toggle.enabled, cx);
                                },
                            ))
                        })
                        .child(switch(p, enabled, busy)),
                )
                .into_any_element()
            })
            .collect();
        list_frame("automate-automation-list", rows, &self.list_scroll)
    }

    pub(crate) fn render_run_list(
        &self,
        p: &AutomatePalette,
        state: &AutomationState,
        runs: &[&AutomationRun],
        selected_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = runs
            .iter()
            .enumerate()
            .map(|(index, run)| {
                let name = state
                    .automations
                    .iter()
                    .find(|automation| automation.id == run.automation_id)
                    .map(|automation| automation.name.clone())
                    .unwrap_or_else(|| run.automation_id.clone());
                let busy = self.busy_id.as_deref() == Some(run.id.as_str());
                let id = run.id.clone();
                let group = format!("automate-run-row-{index}");
                let mut actions = Vec::new();
                if run.session_id().is_some() {
                    let run = (*run).clone();
                    actions.push(icon_button(
                        p,
                        ("automate-run-open-session", index),
                        ICON_EXTERNAL,
                        "Open automation session",
                        busy,
                        move |this: &mut Self, _window, cx| this.open_run_session(&run, cx),
                        cx,
                    ));
                }
                if run.worktree().is_some() {
                    let run = (*run).clone();
                    actions.push(icon_button(
                        p,
                        ("automate-run-open-worktree", index),
                        ICON_FOLDER_OPEN,
                        "Open automation worktree",
                        busy,
                        move |this: &mut Self, _window, cx| this.open_run_worktree(&run, cx),
                        cx,
                    ));
                }
                if run.is_unread {
                    let run = (*run).clone();
                    actions.push(icon_button(
                        p,
                        ("automate-run-mark-read", index),
                        ICON_BELL,
                        "Mark run read",
                        busy,
                        move |this: &mut Self, _window, cx| this.mark_run_read(&run, cx),
                        cx,
                    ));
                }
                let archive_run = (*run).clone();
                actions.push(icon_button(
                    p,
                    ("automate-run-archive", index),
                    ICON_ARCHIVE,
                    "Archive run",
                    busy || run.is_active(),
                    move |this: &mut Self, _window, cx| this.archive_run(&archive_run, cx),
                    cx,
                ));
                list_row(
                    p,
                    ("automate-run-row", index),
                    selected_id == Some(run.id.as_str()),
                )
                .group(group.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.selected_run_id = Some(id.clone());
                    cx.notify();
                }))
                .child(status_dot(if run.is_unread {
                    p.accent
                } else {
                    gpui::transparent_black()
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap(px(8.0))
                                .items_baseline()
                                .child(truncated(name).text_size(px(14.0)).text_color(p.foreground))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(12.0))
                                        .text_color(p.run_status_color(&run.status))
                                        .child(run_status_label(&run.status)),
                                ),
                        )
                        .child(
                            truncated(run.summary().to_string())
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .text_color(p.muted),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .text_color(p.muted)
                        .child(format_short_date(Some(
                            run.completed_at.as_deref().unwrap_or(&run.created_at),
                        ))),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap(px(2.0))
                        .opacity(0.0)
                        .group_hover(group, |this| this.opacity(1.0))
                        .children(actions),
                )
                .into_any_element()
            })
            .collect();
        list_frame("automate-run-list", rows, &self.list_scroll)
    }
}
