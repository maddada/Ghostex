//! The Agent Sync tab's frame and its agent list (packages/core-ui/agents-hub-sync/
//! agent-sync-surface.tsx (deleted 2026-10-01) and sync-agent-list.tsx (deleted 2026-10-01)).
//!
//! CDXC:AgentSync 2026-09-16 WHY:
//! The tab keeps no filesystem state of its own: the report, the plan and the apply result all arrive from the app, and every action is one command, so the Hub, the CLI and a remote host behave identically. The plan sheet always precedes an apply.
use super::super::native_modal_kit::{
    ModalRailItem, hsla, modal_bordered_segmented_control, rgba_of,
};
use super::files_tab::hub_layout;
use super::palette::HubPalette;
use super::sync_model::*;
use super::widgets::*;
use super::window::{GpuiAgentsHubModalWindow, SyncFilter};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::scroll::Scrollbar;
use gpui_component::{h_flex, v_flex};

/// `.agents-hub-sync-layout`: `minmax(16rem, 18rem)` for the list.
const SYNC_LIST_WIDTH: f32 = 288.0;

/// One agent row, with its profiles (`claude-code:work`) nested under it.
struct AgentGroup<'a> {
    agent: &'a SyncAgentReport,
    profiles: Vec<&'a SyncAgentReport>,
}

impl AgentGroup<'_> {
    fn members(&self) -> impl Iterator<Item = &SyncAgentReport> {
        std::iter::once(self.agent).chain(self.profiles.iter().copied())
    }

    fn issues(&self) -> usize {
        self.members().map(issue_count).sum()
    }

    fn tone(&self) -> SyncTone {
        let tones: Vec<SyncTone> = self.members().map(worst_tone).collect();
        if tones.contains(&SyncTone::Err) {
            SyncTone::Err
        } else if tones.contains(&SyncTone::Warn) {
            SyncTone::Warn
        } else {
            SyncTone::Ok
        }
    }
}

fn tone_rank(tone: SyncTone) -> u8 {
    match tone {
        SyncTone::Err => 0,
        SyncTone::Warn => 1,
        _ => 2,
    }
}

/// Profiles nest under their agent, so one agent is one row until it is opened.
fn group_agents<'a>(
    agents: &[&'a SyncAgentReport],
    all: &'a [SyncAgentReport],
) -> Vec<AgentGroup<'a>> {
    let mut groups: Vec<AgentGroup<'a>> = Vec::new();
    for agent in agents {
        let parent = (agent.kind == "profile")
            .then(|| agent.id.split(':').next().unwrap_or_default())
            .and_then(|parent_id| all.iter().find(|candidate| candidate.id == parent_id));
        match parent {
            Some(parent) => {
                if let Some(group) = groups.iter_mut().find(|group| group.agent.id == parent.id) {
                    group.profiles.push(agent);
                } else {
                    groups.push(AgentGroup {
                        agent: parent,
                        profiles: vec![agent],
                    });
                }
            }
            None => {
                if !groups.iter().any(|group| group.agent.id == agent.id) {
                    groups.push(AgentGroup {
                        agent,
                        profiles: Vec::new(),
                    });
                }
            }
        }
    }
    groups
}

fn profile_label(profile: &SyncAgentReport) -> String {
    profile.id.split(':').skip(1).collect::<Vec<_>>().join(":")
}

/// `RowStatus`: a check when nothing is left, else "N to fix" in the row's tone.
fn row_status(hp: &HubPalette, issues: usize, tone: SyncTone) -> AnyElement {
    if issues == 0 {
        return div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(icon(ICON_CHECK_STRONG, 13.0, hp.ok))
            .into_any_element();
    }
    let tone = if tone == SyncTone::Ok {
        SyncTone::Warn
    } else {
        tone
    };
    div()
        .flex_shrink_0()
        .text_size(px(11.52))
        .text_color(hsla(tone_color(hp, tone)))
        .child(format!("{issues} to fix"))
        .into_any_element()
}

/// `.agents-hub-sync-agent-row`: 8px sides, 7px top and bottom, 6px corners, the 5% wash on
/// hover and 9% when selected.
fn agent_row(
    hp: &HubPalette,
    id: impl Into<gpui::ElementId>,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = hp.ink(0.05);
    h_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .items_center()
        .gap(px(8.0))
        .px(px(8.0))
        .py(px(7.0))
        .rounded(px(6.0))
        .when(active, |this| this.bg(hsla(hp.ink(0.09))))
        .when(!active, |this| this.hover(move |this| this.bg(hsla(hover))))
}

fn agent_name(
    hp: &HubPalette,
    name: impl Into<SharedString>,
    color: gpui::Rgba,
    size: f32,
) -> AnyElement {
    let _ = hp;
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(size))
        .line_height(px(18.57))
        .text_color(hsla(color))
        .child(name.into())
        .into_any_element()
}

impl GpuiAgentsHubModalWindow {
    pub(crate) fn render_sync_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hp = self.hp;
        let list = match self.sync.report.as_ref() {
            Some(report) => self.render_sync_agent_list(&hp, report, cx),
            None => empty_note(&hp, "Scanning agents…"),
        };
        let list_pane = v_flex()
            .flex_shrink_0()
            .w(px(SYNC_LIST_WIDTH))
            .h_full()
            .min_h_0()
            .border_r_1()
            .border_color(hsla(hp.line))
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .p(px(10.0))
                    .child(hub_search_input(&hp, &self.sync_search, window, cx)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        div()
                            .id("agents-hub-sync-list")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.sync.list_scroll)
                            .child(list),
                    )
                    .child(Scrollbar::vertical(&self.sync.list_scroll)),
            );
        let detail = match self.sync.report.as_ref() {
            None => empty_note(&hp, "Scanning agents…"),
            Some(report) => match report.error_message.as_ref() {
                Some(message) => empty_note(&hp, message.clone()),
                None => match report.agent(&self.sync.selected_id) {
                    Some(agent) => self.render_sync_agent_pane(&hp, report, agent, cx),
                    None => self.render_sync_overview(&hp, report, cx),
                },
            },
        };
        let detail_pane = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .id("agents-hub-sync-detail")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.sync.detail_scroll)
                    .child(detail),
            )
            .child(Scrollbar::vertical(&self.sync.detail_scroll));
        hub_layout(&hp)
            .child(list_pane)
            .child(detail_pane)
            .when(self.sync.sheet.is_some(), |this| {
                this.child(self.render_sync_plan_sheet(&hp, cx))
            })
            .into_any_element()
    }

    fn render_sync_agent_list(
        &self,
        hp: &HubPalette,
        report: &SyncReport,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let needs_fixing = report
            .agents
            .iter()
            .filter(|agent| agent.status == "attention")
            .count();
        let in_sync = report
            .agents
            .iter()
            .filter(|agent| agent.status == "linked")
            .count();
        let filter = self.sync.filter.unwrap_or(if needs_fixing > 0 {
            SyncFilter::Fix
        } else {
            SyncFilter::All
        });
        let needle = self.sync.query.trim().to_lowercase();
        let matches = |agent: &SyncAgentReport| {
            needle.is_empty()
                || agent.display_name.to_lowercase().contains(&needle)
                || agent.id.to_lowercase().contains(&needle)
        };
        let installed: Vec<&SyncAgentReport> = report
            .agents
            .iter()
            .filter(|agent| agent.status != "notInstalled")
            .collect();
        let hidden: Vec<&SyncAgentReport> = report
            .agents
            .iter()
            .filter(|agent| agent.status == "notInstalled" && matches(agent))
            .collect();
        let mut groups: Vec<AgentGroup> = group_agents(&installed, &report.agents)
            .into_iter()
            .filter(|group| group.members().any(&matches))
            .filter(|group| {
                if !needle.is_empty() || filter == SyncFilter::All {
                    return true;
                }
                if filter == SyncFilter::Fix {
                    group.issues() > 0
                } else {
                    group.issues() == 0
                }
            })
            .collect();
        groups.sort_by_key(|group| tone_rank(group.tone()));
        let selected = self.sync.selected_id.clone();
        // The React list sat directly in the ScrollArea viewport, whose `> div { display: block }`
        // rule turned its flex gap off, so the rows stack with no gap.
        let mut list = v_flex().w_full().min_w_0().px(px(8.0)).pb(px(10.0));
        if needle.is_empty() {
            let overview = agent_row(hp, "agents-hub-sync-overview", selected == "all")
                .gap(px(10.0))
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.select_sync_agent("all".to_string(), cx);
                }))
                .child(icon(ICON_LAYOUT_GRID, 16.0, hp.muted))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(hp.foreground))
                        .child("Overview"),
                );
            let items = [
                ModalRailItem {
                    label: "To fix".into(),
                    trailing: Some(needs_fixing.to_string().into()),
                    dot: None,
                },
                ModalRailItem {
                    label: "In sync".into(),
                    trailing: Some(in_sync.to_string().into()),
                    dot: None,
                },
                ModalRailItem {
                    label: "All".into(),
                    trailing: Some(installed.len().to_string().into()),
                    dot: None,
                },
            ];
            let selected_filter = match filter {
                SyncFilter::Fix => 0,
                SyncFilter::Synced => 1,
                SyncFilter::All => 2,
            };
            let segmented = modal_bordered_segmented_control(
                hp.foreground,
                hp.hairline,
                rgba_of(hp.muted, hp.muted.a * 0.8),
                "agents-hub-sync-filter",
                &items,
                selected_filter,
                |this: &mut Self, index, _window, cx| {
                    this.sync.filter = Some(match index {
                        0 => SyncFilter::Fix,
                        1 => SyncFilter::Synced,
                        _ => SyncFilter::All,
                    });
                    cx.notify();
                },
                cx,
            );
            list = list.child(overview).child(
                div()
                    .mt(px(10.0))
                    .mb(px(6.0))
                    .mx(px(4.0))
                    .child(segmented.w_full()),
            );
        }
        if groups.is_empty() {
            let message = if !needle.is_empty() {
                "No agent matches your search."
            } else if filter == SyncFilter::Fix {
                "Nothing to fix."
            } else {
                "No agents are in sync yet."
            };
            list = list.child(
                div()
                    .px(px(8.0))
                    .py(px(12.0))
                    .text_size(px(12.48))
                    .text_color(hsla(hp.muted))
                    .child(message),
            );
        }
        for (index, group) in groups.iter().enumerate() {
            let has_profiles = !group.profiles.is_empty();
            let open = has_profiles
                && (self.sync.open_profiles.contains(&group.agent.id)
                    || !needle.is_empty()
                    || group.profiles.iter().any(|profile| profile.id == selected));
            let agent_id = group.agent.id.clone();
            let chip_id = group.agent.id.clone();
            let chip_hover = hp.ink(0.08);
            let foreground = hp.foreground;
            let row = agent_row(
                hp,
                ("agents-hub-sync-agent", index),
                selected == group.agent.id,
            )
            .child(
                h_flex()
                    .id(("agents-hub-sync-agent-select", index))
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(10.0))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.select_sync_agent(agent_id.clone(), cx);
                    }))
                    .child(sync_agent_logo(
                        group.agent.icon.as_deref(),
                        &group.agent.display_name,
                        20.0,
                        hp,
                    ))
                    .child(agent_name(
                        hp,
                        group.agent.display_name.clone(),
                        hp.foreground,
                        13.0,
                    )),
            )
            .when(has_profiles, |this| {
                this.child(
                    h_flex()
                        .id(("agents-hub-sync-profiles", index))
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(2.0))
                        .px(px(4.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .text_size(px(11.0))
                        .text_color(hsla(hp.muted))
                        .hover(move |this| this.bg(hsla(chip_hover)).text_color(hsla(foreground)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            if !this.sync.open_profiles.remove(&chip_id) {
                                this.sync.open_profiles.insert(chip_id.clone());
                            }
                            cx.notify();
                        }))
                        .child(format!("{} profiles", group.profiles.len() + 1))
                        .child(icon(
                            if open {
                                ICON_CHEVRON_DOWN
                            } else {
                                ICON_CHEVRON_RIGHT
                            },
                            12.0,
                            hp.muted,
                        )),
                )
            })
            .child(if open {
                row_status(hp, issue_count(group.agent), worst_tone(group.agent))
            } else {
                row_status(hp, group.issues(), group.tone())
            });
            let mut block = v_flex().w_full().gap(px(1.0)).child(row);
            if open {
                for (profile_index, profile) in group.profiles.iter().enumerate() {
                    let active = selected == profile.id;
                    let profile_id = profile.id.clone();
                    let hover = hp.ink(0.05);
                    block = block.child(
                        h_flex()
                            .id(("agents-hub-sync-profile", index * 100 + profile_index))
                            .relative()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap(px(8.0))
                            .pl(px(38.0))
                            .pr(px(8.0))
                            .py(px(7.0))
                            .rounded(px(6.0))
                            .when(active, |this| this.bg(hsla(hp.ink(0.09))))
                            .when(!active, |this| this.hover(move |this| this.bg(hsla(hover))))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.select_sync_agent(profile_id.clone(), cx);
                            }))
                            .child(
                                div()
                                    .absolute()
                                    .left(px(18.0))
                                    .top_0()
                                    .bottom_0()
                                    .w(px(1.0))
                                    .bg(hsla(hp.line)),
                            )
                            .child(agent_name(
                                hp,
                                profile_label(profile),
                                if active { hp.foreground } else { hp.muted },
                                12.48,
                            ))
                            .child(row_status(hp, issue_count(profile), worst_tone(profile))),
                    );
                }
            }
            list = list.child(block);
        }
        if !hidden.is_empty() && (filter == SyncFilter::All || !needle.is_empty()) {
            list = list.child(
                h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .pt(px(10.0))
                    .pb(px(4.0))
                    .text_size(px(11.0))
                    .text_color(hsla(hp.muted))
                    .child("NOT INSTALLED")
                    .child(crate::app::element::count_badge_sized(
                        hidden.len().to_string(),
                        hsla(hp.foreground),
                        16.0,
                        10.0,
                    )),
            );
            if self.sync.show_hidden {
                for (index, agent) in hidden.iter().enumerate() {
                    let agent_id = agent.id.clone();
                    list = list.child(
                        agent_row(hp, ("agents-hub-sync-hidden", index), selected == agent.id)
                            .gap(px(10.0))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.select_sync_agent(agent_id.clone(), cx);
                            }))
                            .child(sync_agent_logo(
                                agent.icon.as_deref(),
                                &agent.display_name,
                                20.0,
                                hp,
                            ))
                            .child(agent_name(
                                hp,
                                agent.display_name.clone(),
                                hp.foreground,
                                13.0,
                            )),
                    );
                }
            } else {
                list = list.child(
                    agent_row(hp, "agents-hub-sync-show-hidden", false)
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.sync.show_hidden = !this.sync.show_hidden;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex_shrink_0()
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.0))
                                .border_1()
                                .border_color(hsla(hp.line))
                                .bg(hsla(hp.ink(0.12)))
                                .child(icon(ICON_PLUS, 12.0, hp.foreground)),
                        )
                        .child(agent_name(
                            hp,
                            "Show agents without a config folder",
                            hp.muted,
                            13.0,
                        )),
                );
            }
        }
        list.into_any_element()
    }
}
