//! Agent Sync's plan sheet (packages/core-ui/agents-hub-sync/sync-plan-sheet.tsx (deleted 2026-10-01)): the plan as
//! switchable groups of operations over a scrim, then the apply result.
use super::super::native_modal_kit::{
    MODAL_MONO_FONT, MODAL_UI_FONT, ModalRailItem, SwitchRowExt as _, a11y_toggled, bind_press,
    hsla, modal_bordered_segmented_control, modal_rgba, modal_switch, rgba_of, switch_row_wash,
};
use super::palette::HubPalette;
use super::sync_model::*;
use super::widgets::*;
use super::window::GpuiAgentsHubModalWindow;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Rgba, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledText, TextRun, div, font, point, px,
};
use gpui_component::{h_flex, v_flex};

/// A plan group shows eight changes until it is expanded.
const PLAN_GROUP_LIMIT: usize = 8;

fn verb_color(hp: &HubPalette, verb: &str) -> Rgba {
    match verb {
        "link" => hp.ok,
        "write" => hp.info,
        "backup" => hp.warn,
        "unlink" | "drop" => hp.err,
        _ => hp.muted,
    }
}

#[derive(Clone, Copy)]
enum VerdictTone {
    Neutral,
    Ok,
    Warn,
    Err,
}

/// `.agents-hub-sync-verdict`: a 3px coloured edge and a faint fill behind 13px copy.
fn verdict(hp: &HubPalette, tone: VerdictTone, body: AnyElement) -> AnyElement {
    let (fill, edge) = match tone {
        VerdictTone::Neutral => (modal_rgba(0x86d3f8, 0.06), hp.verdict_edge),
        VerdictTone::Ok => (hp.ok_fill(), hp.ok),
        VerdictTone::Warn => (hp.warn_fill(), hp.warn),
        VerdictTone::Err => (hp.err_fill(), hp.err),
    };
    div()
        .w_full()
        .px(px(14.0))
        .py(px(10.0))
        .bg(hsla(fill))
        .border_l(px(3.0))
        .border_color(hsla(edge))
        .rounded_r(px(8.0))
        .text_size(px(13.0))
        .line_height(px(18.85))
        .text_color(hsla(hp.foreground))
        .child(body)
        .into_any_element()
}

/// A run of `text` in one font and colour, for the mixed-font lines.
fn run(text: &str, family: &'static str, weight: FontWeight, color: Rgba) -> TextRun {
    TextRun {
        len: text.len(),
        font: gpui::Font {
            weight,
            ..font(family)
        },
        color: hsla(color),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// `OpLine`: the verb in its colour, then the path, the target and a muted note, ellipsized.
fn op_line(
    hp: &HubPalette,
    verb: &str,
    verb_tone: Rgba,
    op: Option<&SyncPlanOp>,
    fallback: &str,
) -> AnyElement {
    let (text, runs) = match op {
        Some(op) => {
            let target = match op.verb.as_str() {
                "link" | "backup" => format!(" → {}", op.target.as_deref().unwrap_or("")),
                "drop" => format!(" {}", op.target.as_deref().unwrap_or("")),
                _ => String::new(),
            };
            let main = format!("{}{}", op.path, target);
            let note = op
                .note
                .as_ref()
                .map(|note| format!(" {note}"))
                .unwrap_or_default();
            let mut runs = vec![run(
                &main,
                MODAL_MONO_FONT,
                FontWeight::NORMAL,
                hp.foreground,
            )];
            if !note.is_empty() {
                runs.push(run(&note, MODAL_MONO_FONT, FontWeight::NORMAL, hp.muted));
            }
            (format!("{main}{note}"), runs)
        }
        None => (
            fallback.to_string(),
            vec![run(
                fallback,
                MODAL_MONO_FONT,
                FontWeight::NORMAL,
                hp.foreground,
            )],
        ),
    };
    h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap(px(10.0))
        .font_family(MODAL_MONO_FONT)
        .text_size(px(12.0))
        .line_height(px(19.2))
        .child(
            div()
                .w(px(72.0))
                .flex_shrink_0()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(verb_tone))
                .child(SharedString::from(verb.to_string())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(StyledText::new(text).with_runs(runs)),
        )
        .into_any_element()
}

impl GpuiAgentsHubModalWindow {
    pub(crate) fn render_sync_plan_sheet(
        &self,
        hp: &HubPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(sheet) = self.sync.sheet.as_ref() else {
            return div().into_any_element();
        };
        let plan = self.sync.sheet_plan();
        let result = self.sync.sheet_result();
        let enabled = self.sync.enabled_groups();
        let applying = self.sync.applying;
        let scope_label = if sheet.scope == "all" {
            "all agents".to_string()
        } else {
            self.sync
                .report
                .as_ref()
                .and_then(|report| report.agent(&sheet.scope))
                .map(|agent| agent.display_name.clone())
                .unwrap_or_else(|| sheet.scope.clone())
        };
        let change_count = plan.map_or(0, |plan| plan_change_count(plan, &enabled));

        let head = h_flex()
            .flex_shrink_0()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(12.0))
            .border_b_1()
            .border_color(hsla(hp.line))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(hp.foreground))
            .child(if result.is_some() {
                format!("Sync {scope_label} · result")
            } else {
                format!("Sync {scope_label}")
            })
            .when_some(plan.filter(|_| result.is_none()), |this, plan| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .justify_end()
                        .gap(px(6.0))
                        .child(pill(
                            hp,
                            PillTone::Ok,
                            format!(
                                "{} {}",
                                grouped(plan.summary.links),
                                ghostex_gx_protocol::plural_word(
                                    plan.summary.links,
                                    "link",
                                    "links"
                                )
                            ),
                        ))
                        .child(pill(
                            hp,
                            PillTone::Info,
                            if plan.groups.iter().any(|group| group.kind == "seedMainMd") {
                                ghostex_gx_protocol::plural(
                                    plan.summary.writes,
                                    "file to write",
                                    "files to write",
                                )
                            } else {
                                ghostex_gx_protocol::plural(
                                    plan.summary.writes,
                                    "pointer file",
                                    "pointer files",
                                )
                            },
                        ))
                        .child(pill(
                            hp,
                            PillTone::Warn,
                            ghostex_gx_protocol::plural(plan.summary.backups, "backup", "backups"),
                        ))
                        .child(pill(
                            hp,
                            PillTone::Err,
                            ghostex_gx_protocol::plural(
                                plan.summary.unlinks,
                                "removal",
                                "removals",
                            ),
                        )),
                )
            });

        let mut body = v_flex()
            .id("agents-hub-sync-sheet-body")
            .flex_1()
            .min_h_0()
            .w_full()
            .gap(px(10.0))
            .px(px(16.0))
            .py(px(12.0))
            .overflow_y_scroll()
            .track_scroll(&self.sync.sheet_scroll);
        if let Some(result) = result {
            match result.error_message.as_ref() {
                Some(message) => {
                    body = body.child(verdict(
                        hp,
                        VerdictTone::Err,
                        div()
                            .child(SharedString::from(message.clone()))
                            .into_any_element(),
                    ));
                }
                None => {
                    let done = result.done.len();
                    let failed = result.failed.len();
                    body = body.child(verdict(
                        hp,
                        if failed == 0 {
                            VerdictTone::Ok
                        } else {
                            VerdictTone::Warn
                        },
                        div()
                            .child(format!(
                                "{} operation{} done, {failed} failed, {} already correct.{}",
                                grouped(done),
                                if done == 1 { "" } else { "s" },
                                result.skipped_keeps,
                                if failed > 0 {
                                    " Fix the failures below and run Sync again; the plan is idempotent."
                                } else {
                                    ""
                                }
                            ))
                            .into_any_element(),
                    ));
                }
            }
            for failure in &result.failed {
                let op = SyncPlanOp {
                    path: format!("{} {}", failure.op.verb, failure.op.path),
                    note: Some(failure.error.clone()),
                    ..SyncPlanOp::default()
                };
                body = body.child(op_line(hp, "failed", hp.err, Some(&op), ""));
            }
        } else if let Some(plan) = plan {
            if let Some(message) = plan.error_message.as_ref() {
                body = body.child(verdict(
                    hp,
                    VerdictTone::Err,
                    div()
                        .child(SharedString::from(message.clone()))
                        .into_any_element(),
                ));
            } else if plan.groups.is_empty() && !plan.main_md_blocked && plan.warnings.is_empty() {
                body = body.child(
                    div()
                        .px(px(14.0))
                        .py(px(12.0))
                        .text_size(px(13.0))
                        .text_color(hsla(hp.muted))
                        .child("Everything is in sync. There is nothing to change."),
                );
            } else {
                for warning in &plan.warnings {
                    body = body.child(verdict(
                        hp,
                        VerdictTone::Warn,
                        div()
                            .child(SharedString::from(warning.clone()))
                            .into_any_element(),
                    ));
                }
                if plan.main_md_blocked {
                    if let Some(best) = plan.seed_recommendation.as_ref() {
                        let best_id = best.agent_id.clone();
                        let best_label =
                            format!("Create main.md from {} ({})", best.display_name, best.path);
                        let mut seed_row = h_flex()
                            .w_full()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(hub_button(
                                hp,
                                "agents-hub-sync-seed-richest",
                                HubButtonVariant::Quiet,
                                HubButtonSize::Small,
                                Some(ICON_FILE_TEXT),
                                Some(best_label.into()),
                                applying,
                                move |this: &mut Self, _window, cx| {
                                    this.set_sync_seed_source(Some(best_id.clone()), cx);
                                },
                                cx,
                            ));
                        for (idx, candidate) in plan.seed_candidates.iter().skip(1).enumerate() {
                            let cid = candidate.agent_id.clone();
                            let label = format!("Use {}", candidate.display_name);
                            seed_row = seed_row.child(hub_button(
                                hp,
                                ("agents-hub-sync-seed-alt", idx),
                                HubButtonVariant::Outline,
                                HubButtonSize::Small,
                                None,
                                Some(label.into()),
                                applying,
                                move |this: &mut Self, _window, cx| {
                                    this.set_sync_seed_source(Some(cid.clone()), cx);
                                },
                                cx,
                            ));
                        }
                        body = body.child(seed_row);
                    }
                } else if sheet.seed_main_from.is_some() && plan.seed_candidates.len() > 1 {
                    let selected_seed = sheet.seed_main_from.clone();
                    let mut picker = h_flex()
                        .w_full()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(hsla(hp.muted))
                        .child("Seed ~/.agents/main.md from:");
                    for (idx, candidate) in plan.seed_candidates.iter().enumerate() {
                        let cid = candidate.agent_id.clone();
                        let active = selected_seed.as_deref() == Some(candidate.agent_id.as_str());
                        picker = picker.child(hub_button(
                            hp,
                            ("agents-hub-sync-seed-pick", idx),
                            if active {
                                HubButtonVariant::Quiet
                            } else {
                                HubButtonVariant::Outline
                            },
                            HubButtonSize::Small,
                            None,
                            Some(
                                format!("{} ({} B)", candidate.display_name, candidate.bytes)
                                    .into(),
                            ),
                            applying,
                            move |this: &mut Self, _window, cx| {
                                this.set_sync_seed_source(Some(cid.clone()), cx);
                            },
                            cx,
                        ));
                    }
                    body = body.child(picker);
                }

                let show_pointer_policy = !plan.main_md_blocked
                    && (plan.groups.iter().any(|group| group.kind == "pointerFiles")
                        || sheet.pointer_policy != "replace");
                if show_pointer_policy {
                    let selected_policy = match sheet.pointer_policy.as_str() {
                        "prepend" => 1,
                        "leave-alone" | "leaveAlone" => 2,
                        _ => 0,
                    };
                    let items = [
                        ModalRailItem {
                            label: "Replace (backup first)".into(),
                            trailing: None,
                            dot: None,
                        },
                        ModalRailItem {
                            label: "Prepend pointer".into(),
                            trailing: None,
                            dot: None,
                        },
                        ModalRailItem {
                            label: "Leave alone".into(),
                            trailing: None,
                            dot: None,
                        },
                    ];
                    let segmented = modal_bordered_segmented_control(
                        hp.foreground,
                        hp.hairline,
                        rgba_of(hp.muted, hp.muted.a * 0.8),
                        "agents-hub-sync-pointer-policy",
                        &items,
                        selected_policy,
                        |this: &mut Self, index, _window, cx| {
                            let next = match index {
                                1 => "prepend",
                                2 => "leave-alone",
                                _ => "replace",
                            };
                            this.set_sync_pointer_policy(next.to_string(), cx);
                        },
                        cx,
                    );
                    body = body.child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .px(px(2.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(hsla(hp.muted))
                                    .child("Existing instruction files"),
                            )
                            .child(segmented),
                    );
                }

                if !plan.groups.is_empty() {
                    let before =
                        "Nothing is deleted. Anything in the way of a link or pointer is moved to ";
                    let code = plan.backup_dir.clone();
                    let after = " first, dangling links are removed, and every other file is left alone. Switch a group off to skip it.";
                    let text = format!("{before}{code}{after}");
                    let runs = vec![
                        run(before, MODAL_UI_FONT, FontWeight::NORMAL, hp.foreground),
                        run(&code, MODAL_MONO_FONT, FontWeight::NORMAL, hp.foreground),
                        run(after, MODAL_UI_FONT, FontWeight::NORMAL, hp.foreground),
                    ];
                    body = body.child(verdict(
                        hp,
                        VerdictTone::Neutral,
                        div()
                            .child(StyledText::new(text).with_runs(runs))
                            .into_any_element(),
                    ));
                    for (index, group) in plan.groups.iter().enumerate() {
                        body = body.child(self.render_plan_group(hp, index, group, &enabled, cx));
                    }
                }
            }
        } else {
            body = body.child(
                div()
                    .px(px(14.0))
                    .py(px(12.0))
                    .text_size(px(13.0))
                    .text_color(hsla(hp.muted))
                    .child("Computing the plan…"),
            );
        }

        let mut foot = h_flex()
            .flex_shrink_0()
            .w_full()
            .justify_end()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(hsla(hp.line));
        if result.is_some() {
            foot = foot.child(hub_button(
                hp,
                "agents-hub-sync-done",
                HubButtonVariant::Quiet,
                HubButtonSize::Small,
                None,
                Some("Done".into()),
                false,
                |this: &mut Self, _window, cx| this.finish_sync_sheet(cx),
                cx,
            ));
        } else {
            foot = foot
                .child(hub_button(
                    hp,
                    "agents-hub-sync-copy-script",
                    HubButtonVariant::Ghost,
                    HubButtonSize::Small,
                    None,
                    Some(
                        if self.sync_script_copied() {
                            "Copied"
                        } else {
                            "Copy as shell script"
                        }
                        .into(),
                    ),
                    plan.is_none_or(|plan| plan.groups.is_empty()) || applying,
                    |this: &mut Self, _window, cx| this.copy_sync_script(cx),
                    cx,
                ))
                .child(hub_button(
                    hp,
                    "agents-hub-sync-cancel",
                    HubButtonVariant::Ghost,
                    HubButtonSize::Small,
                    None,
                    Some("Cancel".into()),
                    applying,
                    |this: &mut Self, _window, cx| this.close_sync_sheet(cx),
                    cx,
                ))
                .child(hub_button(
                    hp,
                    "agents-hub-sync-apply",
                    HubButtonVariant::Quiet,
                    HubButtonSize::Small,
                    None,
                    Some(
                        if applying {
                            "Applying…".to_string()
                        } else if change_count == 0 {
                            "Nothing to apply".to_string()
                        } else {
                            format!(
                                "Apply {} change{}",
                                grouped(change_count),
                                if change_count == 1 { "" } else { "s" }
                            )
                        }
                        .into(),
                    ),
                    plan.is_none() || applying || change_count == 0,
                    |this: &mut Self, _window, cx| this.apply_sync_plan(cx),
                    cx,
                ));
        }

        let shadow = if hp.light {
            modal_rgba(0x000000, 0.25)
        } else {
            modal_rgba(0x000000, 0.6)
        };
        // CDXC:Theming 2026-11-20: the sheet draws inside the hub's window, which cannot blur its
        // own content, so under glass it is the lifted dialog fill (the theme colour lifted toward
        // white at 96%, as the dialogs inside Settings) and the scrim behind it a veil of the theme
        // colour instead of black. Glass off, the opaque theme chrome and the black dim stay.
        let (sheet_fill, scrim) = if hp.modal.glass {
            (
                hp.modal.lifted_dialog_fill(true),
                rgba_of(hp.modal.solid_surface, 0.6),
            )
        } else {
            (
                hp.modal.solid_surface,
                modal_rgba(0x000000, if hp.light { 0.16 } else { 0.3 }),
            )
        };
        let sheet_box = v_flex()
            .id("agents-hub-sync-sheet")
            .w_full()
            .max_w(px(896.0))
            .max_h_full()
            .bg(hsla(sheet_fill))
            .border_1()
            .border_color(hsla(hp.ink(0.14)))
            .rounded(px(12.0))
            .shadow(vec![gpui::BoxShadow {
                color: hsla(shadow),
                offset: point(px(0.0), px(16.0)),
                blur_radius: px(48.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .overflow_hidden()
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(head)
            .child(body)
            .child(foot);
        div()
            .id("agents-hub-sync-scrim")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .justify_center()
            .items_start()
            .p(px(20.0))
            // A light dim, so the hub stays in view behind the sheet (CDXC:AppModal 2026-11-09 on
            // `pane_dialog_scrim` in native_chat/child_window.rs: never black out what's behind).
            .bg(hsla(scrim))
            .child(sheet_box)
            .into_any_element()
    }

    fn render_plan_group(
        &self,
        hp: &HubPalette,
        index: usize,
        group: &SyncPlanGroup,
        enabled: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = enabled.contains(&group.kind);
        let changes: Vec<&SyncPlanOp> = group.ops.iter().filter(|op| op.verb != "keep").collect();
        let keeps = group.ops.len() - changes.len();
        let expanded = self.sync.expanded_plan_groups.contains(&group.kind);
        let visible = if expanded {
            changes.len()
        } else {
            changes.len().min(PLAN_GROUP_LIMIT)
        };
        let pill_tone = if group.change_count == 0 {
            PillTone::Neutral
        } else if group.kind == "removeDangling" {
            PillTone::Err
        } else if group.kind == "pruneLock" {
            PillTone::Warn
        } else {
            PillTone::Ok
        };
        let kind = group.kind.clone();
        let muted_note = |text: String| {
            div()
                .text_size(px(12.0))
                .text_color(hsla(hp.muted))
                .child(text)
        };
        let head = h_flex()
            .id(("agents-hub-sync-plan-switch", index))
            .role(gpui::Role::Switch)
            .aria_toggled(a11y_toggled(on))
            .aria_label(group.title.clone())
            .w_full()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .mt(px(6.0))
            .mb(px(4.0))
            .text_size(px(12.0))
            .text_color(hsla(hp.muted))
            .switch_row_press(
                switch_row_wash(hp.foreground),
                false,
                bind_press(cx, move |this: &mut Self, _window, cx| {
                    this.toggle_sync_plan_group(kind.clone(), cx);
                }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_switch(&hp.modal, on, false)),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(hp.foreground))
                    .child(group.title.to_uppercase()),
            )
            .child(pill(hp, pill_tone, group.change_count.to_string()))
            .when(group.kind == "pruneLock", |this| {
                this.child(muted_note(
                    "off by default; the skills CLI re-adds anything it reinstalls".to_string(),
                ))
            })
            .when(keeps > 0, |this| {
                this.child(muted_note(format!("{keeps} already correct")))
            });
        let dim = |element: AnyElement| {
            div()
                .w_full()
                .when(!on, |this| this.opacity(0.45))
                .child(element)
                .into_any_element()
        };
        let mut lines: Vec<AnyElement> = Vec::new();
        for op in changes.iter().take(visible) {
            lines.push(dim(op_line(
                hp,
                &op.verb,
                verb_color(hp, &op.verb),
                Some(op),
                "",
            )));
        }
        let hidden = changes.len() - visible;
        if hidden > 0 || (expanded && changes.len() > PLAN_GROUP_LIMIT) {
            let kind = group.kind.clone();
            let foreground = hp.foreground;
            lines.push(
                div()
                    .id(("agents-hub-sync-plan-more", index))
                    .w_full()
                    .px(px(14.0))
                    .pt(px(6.0))
                    .pb(px(8.0))
                    .text_size(px(12.0))
                    .text_color(hsla(hp.muted))
                    .hover(move |this| this.text_color(hsla(foreground)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        if !this.sync.expanded_plan_groups.remove(&kind) {
                            this.sync.expanded_plan_groups.insert(kind.clone());
                        }
                        cx.notify();
                    }))
                    .child(if hidden > 0 {
                        format!("+ {hidden} more")
                    } else {
                        "Show fewer".to_string()
                    })
                    .into_any_element(),
            );
        }
        v_flex()
            .w_full()
            .gap(px(2.0))
            .child(head)
            .children(lines)
            .into_any_element()
    }
}
