//! Agent Sync's overview (packages/core-ui/agents-hub-sync/sync-overview-pane.tsx (deleted 2026-10-01) and
//! sync-fix-list.tsx (deleted 2026-10-01)): one status with one action, what is shared, the fix list, and the
//! optional cleanup.
use super::super::native_modal_kit::{
    SwitchRowExt as _, a11y_toggled, bind_press, hsla, modal_switch, rgba_of, switch_row_wash,
};
use super::palette::HubPalette;
use super::sync_model::*;
use super::widgets::*;
use super::window::GpuiAgentsHubModalWindow;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    canvas, div, point, px,
};
use gpui_component::{h_flex, v_flex};

const WHERE_LIMIT: usize = 4;
const PATH_LIMIT: usize = 3;

/// `.agents-hub-sync-detail-body`: 24px between sections, centred at most 980px wide.
pub(crate) fn detail_body() -> gpui::Div {
    v_flex()
        .w_full()
        .max_w(px(980.0))
        .mx_auto()
        .gap(px(24.0))
        .pt(px(24.0))
        .px(px(28.0))
        .pb(px(32.0))
}

/// `.agents-hub-sync-card`: a raised card whose direct children are split by hairlines.
pub(crate) fn sync_card(hp: &HubPalette, children: Vec<AnyElement>) -> AnyElement {
    let line = hsla(hp.line);
    v_flex()
        .w_full()
        .bg(hsla(hp.raised))
        .border_1()
        .border_color(line)
        .rounded(px(12.0))
        .overflow_hidden()
        .children(children.into_iter().enumerate().map(move |(index, child)| {
            div()
                .w_full()
                .when(index > 0, |this| this.border_t_1().border_color(line))
                .child(child)
        }))
        .into_any_element()
}

/// `.agents-hub-sync-section-header`: a 14px/500 title and a hint or link at the far right.
pub(crate) fn section_header(
    hp: &HubPalette,
    title: impl Into<SharedString>,
    right: Option<AnyElement>,
) -> AnyElement {
    h_flex()
        .w_full()
        .items_end()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(hp.foreground))
                .child(title.into()),
        )
        .children(right)
        .into_any_element()
}

pub(crate) fn section_hint(hp: &HubPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(12.48))
        .line_height(px(18.0))
        .text_color(hsla(hp.muted))
        .child(text.into())
        .into_any_element()
}

/// The fix row's two text lines: a 13.44px title over a 12.48px muted line.
pub(crate) fn fix_text(
    hp: &HubPalette,
    title: impl Into<SharedString>,
    sub: impl Into<SharedString>,
) -> AnyElement {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(13.44))
                .line_height(px(19.0))
                .text_color(hsla(hp.foreground))
                .child(title.into()),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.48))
                .line_height(px(18.0))
                .text_color(hsla(hp.muted))
                .child(sub.into()),
        )
        .into_any_element()
}

/// A fix row that is not a button (`.agents-hub-sync-fix-row.is-static`).
fn static_fix_row(
    hp: &HubPalette,
    tile: AnyElement,
    text: AnyElement,
    trailing: Option<AnyElement>,
) -> AnyElement {
    let _ = hp;
    h_flex()
        .w_full()
        .items_center()
        .gap(px(14.0))
        .min_h(px(60.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(tile)
        .child(text)
        .children(trailing)
        .into_any_element()
}

/// The coverage ring: a 56px circle, the linked share of detected agents stroked from the top.
fn sync_ring(hp: &HubPalette, linked: usize, detected: usize, synced: bool) -> AnyElement {
    let track = if synced {
        rgba_of(gpui::rgb(0x10b981), 0.4)
    } else {
        rgba_of(gpui::rgb(0xf59e0b), 0.28)
    };
    let fill = hp.ok;
    let share = if detected > 0 {
        (linked as f32 / detected as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label: AnyElement = if synced {
        icon(ICON_CHECK_HEAVY, 22.0, hp.ok)
    } else {
        div()
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(hp.foreground))
            .child(format!("{linked}/{detected}"))
            .into_any_element()
    };
    div()
        .relative()
        .flex_shrink_0()
        .size(px(56.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    paint_ring(bounds, share, hsla(track), hsla(fill), window);
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(label)
        .into_any_element()
}

fn paint_ring(
    bounds: Bounds<Pixels>,
    share: f32,
    track: gpui::Hsla,
    fill: gpui::Hsla,
    window: &mut Window,
) {
    let scale = f32::from(bounds.size.width) / 56.0;
    let radius = 24.0 * scale;
    let width = 4.0 * scale;
    let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
    let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
    let at = |angle: f32| point(px(cx + radius * angle.cos()), px(cy + radius * angle.sin()));
    let top = -std::f32::consts::FRAC_PI_2;
    // The track is two half circles; one arc cannot close on itself.
    let mut circle = gpui::PathBuilder::stroke(px(width));
    circle.move_to(at(top));
    circle.arc_to(
        point(px(radius), px(radius)),
        px(0.0),
        false,
        true,
        at(top + std::f32::consts::PI),
    );
    circle.arc_to(point(px(radius), px(radius)), px(0.0), false, true, at(top));
    if let Ok(path) = circle.build() {
        window.paint_path(path, track);
    }
    if share <= 0.0 {
        return;
    }
    let sweep = (share * std::f32::consts::TAU).min(std::f32::consts::TAU - 0.001);
    let end = top + sweep;
    let mut arc = gpui::PathBuilder::stroke(px(width));
    arc.move_to(at(top));
    arc.arc_to(
        point(px(radius), px(radius)),
        px(0.0),
        sweep > std::f32::consts::PI,
        true,
        at(end),
    );
    if let Ok(path) = arc.build() {
        window.paint_path(path, fill);
    }
    // `stroke-linecap: round`: a dot of the stroke's width on each end.
    for angle in [top, end] {
        let center = at(angle);
        let cap = width / 2.0;
        let mut dot = gpui::PathBuilder::fill();
        dot.move_to(point(center.x - px(cap), center.y));
        dot.arc_to(
            point(px(cap), px(cap)),
            px(0.0),
            false,
            true,
            point(center.x + px(cap), center.y),
        );
        dot.arc_to(
            point(px(cap), px(cap)),
            px(0.0),
            false,
            true,
            point(center.x - px(cap), center.y),
        );
        if let Ok(path) = dot.build() {
            window.paint_path(path, fill);
        }
    }
}

/// The problem glyph (`ProblemIcon`).
fn problem_icon(kind: &str) -> &'static str {
    match kind {
        "danglingLinks" | "sourceBrokenLinks" | "redundantLinks" => ICON_LINK_OFF,
        "copiedFolders" | "backupFolders" => ICON_COPY,
        "wholeFolderLinks" => ICON_FOLDER,
        "missingPointers" => ICON_FILE_TEXT,
        _ => ICON_ALERT,
    }
}

/// `Checked at 04:00 PM`, in the computer's time zone.
fn checked_label(generated_at: &str) -> Option<String> {
    let time = chrono::DateTime::parse_from_rfc3339(generated_at).ok()?;
    Some(format!(
        "Checked at {}",
        time.with_timezone(&chrono::Local).format("%I:%M %p")
    ))
}

impl GpuiAgentsHubModalWindow {
    pub(crate) fn render_sync_overview(
        &self,
        hp: &HubPalette,
        report: &SyncReport,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let summary = &report.summary;
        let detected: Vec<&SyncAgentReport> = report
            .agents
            .iter()
            .filter(|agent| agent.detected)
            .collect();
        let skills_linked = detected
            .iter()
            .filter(|agent| {
                agent.universal
                    || agent.skills.as_ref().is_some_and(|skills| {
                        skills.dir_state == "realDir"
                            && skills.counts.dangling == 0
                            && skills.counts.missing == 0
                            && skills.counts.copies_identical == 0
                    })
            })
            .count();
        let pointers = detected
            .iter()
            .filter(|agent| {
                agent
                    .instructions
                    .as_ref()
                    .is_some_and(|value| value.state == "pointer")
            })
            .count();
        let pointer_targets = detected
            .iter()
            .filter(|agent| agent.instructions.is_some())
            .count();
        let hooks_linked = detected
            .iter()
            .filter(|agent| {
                agent
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.state == "linked")
            })
            .count();
        let hooks_targets = detected
            .iter()
            .filter(|agent| {
                agent
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.state != "nothingShared")
            })
            .count();
        let to_fix: Vec<&SyncProblem> = report
            .problems
            .iter()
            .filter(|problem| problem.fixable && problem.kind != "staleLockEntries")
            .collect();
        let stale_lock = report
            .problems
            .iter()
            .find(|problem| problem.kind == "staleLockEntries");
        let untracked = report
            .problems
            .iter()
            .find(|problem| problem.kind == "untrackedSkills");
        let part = self.sync.part;
        let visible: Vec<&SyncProblem> = match part {
            Some(part) => to_fix
                .iter()
                .copied()
                .filter(|problem| problem_part(&problem.kind) == Some(part))
                .collect(),
            None => to_fix.clone(),
        };
        let synced = summary.agents_attention == 0 && to_fix.is_empty();
        let checked = checked_label(&report.generated_at);
        let source_skills = report
            .source
            .skills
            .iter()
            .filter(|skill| !skill.broken)
            .count();

        let title = if synced {
            format!("All {} agents are in sync", summary.agents_detected)
        } else if summary.agents_attention > 0 {
            format!(
                "{} of {} agents are out of sync",
                summary.agents_attention, summary.agents_detected
            )
        } else {
            format!(
                "{} {} to fix in your shared folder",
                to_fix.len(),
                if to_fix.len() == 1 { "thing" } else { "things" }
            )
        };
        let sub = if synced {
            "Every agent on this computer uses the skills, instructions and hook scripts in your shared folder. Edit them there and every agent picks up the change."
        } else {
            "Agent Sync makes every agent on this computer use the same skills, instructions and hook scripts from one shared folder."
        };
        let actions = if synced {
            v_flex().items_end().gap(px(6.0)).child(hub_button(
                hp,
                "agents-hub-sync-check-again",
                HubButtonVariant::Outline,
                HubButtonSize::Small,
                Some(ICON_REFRESH),
                Some("Check again".into()),
                false,
                |this: &mut Self, _window, cx| this.refresh_sync_report(cx),
                cx,
            ))
        } else {
            v_flex()
                .flex_shrink_0()
                .items_end()
                .gap(px(6.0))
                .child(hub_button(
                    hp,
                    "agents-hub-sync-fix-all",
                    HubButtonVariant::Quiet,
                    HubButtonSize::Default,
                    None,
                    Some("Review and fix all…".into()),
                    false,
                    |this: &mut Self, _window, cx| {
                        /*
                        CDXC:AgentSync 2026-09-22 WHY:
                        Tidying the lock file stays opt-in (DECISION 4A, 2026-09-16), so it is a switch beside the fix list rather than a problem row, and "Review and fix all" only adds the pruneLock group while that switch is on.
                        */
                        let preset = this.sync.tidy_lock.then(|| {
                            let mut groups = default_plan_groups(None);
                            groups.push("pruneLock".to_string());
                            groups
                        });
                        this.open_sync_plan("all".to_string(), preset, cx);
                    },
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(11.52))
                        .text_color(hsla(rgba_of(hp.muted, hp.muted.a * 0.8)))
                        .child("Nothing changes until you approve"),
                )
        };
        let hero_main = h_flex()
            .w_full()
            .items_center()
            .gap(px(18.0))
            .px(px(22.0))
            .py(px(20.0))
            .child(sync_ring(
                hp,
                summary.agents_linked,
                summary.agents_detected,
                synced,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .line_height(px(26.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(hp.foreground))
                            .child(title),
                    )
                    .child(
                        div()
                            .max_w(px(520.0))
                            .text_size(px(13.0))
                            .line_height(px(19.5))
                            .text_color(hsla(hp.muted))
                            .child(sub),
                    ),
            )
            .child(actions);
        let source_path = report.source.path.clone();
        let strip = h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(8.0))
            .pl(px(22.0))
            .pr(px(12.0))
            .py(px(6.0))
            .bg(hsla(hp.ink(0.015)))
            .border_t_1()
            .border_color(hsla(hp.line))
            .text_size(px(12.48))
            .line_height(px(18.0))
            .text_color(hsla(hp.muted))
            .whitespace_nowrap()
            .child(icon(ICON_FOLDER, 14.0, hp.muted))
            .child("Shared folder")
            .child(
                div()
                    .font_family(super::super::native_modal_kit::MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .text_color(hsla(hp.foreground))
                    .child(SharedString::from(report.source.path.clone())),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(format!(
                        "· {} skills, {} instruction files, {} hook scripts",
                        source_skills,
                        report.source.md_files.len(),
                        report.source.hook_script_count
                    )),
            )
            .child(div().flex_1())
            .children(checked.map(|checked| {
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(rgba_of(hp.muted, hp.muted.a * 0.8)))
                    .child(checked)
            }))
            .when(!synced, |this| {
                this.child(hub_button(
                    hp,
                    "agents-hub-sync-strip-refresh",
                    HubButtonVariant::Ghost,
                    HubButtonSize::Small,
                    Some(ICON_REFRESH),
                    None,
                    false,
                    |this: &mut Self, _window, cx| this.refresh_sync_report(cx),
                    cx,
                ))
            })
            .child(hub_button(
                hp,
                "agents-hub-sync-open-shared",
                HubButtonVariant::Ghost,
                HubButtonSize::Small,
                Some(ICON_EXTERNAL_LINK),
                Some("Open folder".into()),
                false,
                move |this: &mut Self, _window, cx| {
                    let home = this
                        .sync
                        .report
                        .as_ref()
                        .map(|report| report.home.clone())
                        .unwrap_or_default();
                    this.send(
                        super::window::AgentsHubModalCommand::OpenPath {
                            path: expand_home_path(&source_path, &home),
                        },
                        cx,
                    );
                },
                cx,
            ));
        let hero = v_flex()
            .w_full()
            .bg(hsla(hp.raised))
            .border_1()
            .border_color(hsla(hp.line))
            .rounded(px(12.0))
            .overflow_hidden()
            .child(hero_main)
            .child(strip);

        let coverage = h_flex()
            .w_full()
            .items_stretch()
            .gap(px(10.0))
            .child(self.coverage_card(
                hp,
                SyncPart::Skills,
                ICON_BOOK,
                "Skills",
                skills_linked,
                detected.len(),
                "Each agent gets a link to every shared skill.",
                cx,
            ))
            .child(self.coverage_card(
                hp,
                SyncPart::Instructions,
                ICON_FILE_TEXT,
                "Instructions",
                pointers,
                pointer_targets,
                "Each agent is told to read your shared main.md.",
                cx,
            ))
            .child(self.coverage_card(
                hp,
                SyncPart::Hooks,
                ICON_TERMINAL,
                "Hook scripts",
                hooks_linked,
                hooks_targets,
                "Only Claude Code and Codex use them.",
                cx,
            ));
        let shared_section = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(section_header(
                hp,
                "What is shared",
                Some(section_hint(hp, "How many agents use the shared copy")),
            ))
            .child(coverage);

        let fix_title = if visible.is_empty() {
            "Nothing to fix".to_string()
        } else {
            format!(
                "{} {} to fix",
                visible.len(),
                if visible.len() == 1 {
                    "thing"
                } else {
                    "things"
                }
            )
        };
        let fix_right = if part.is_some() {
            Some(
                text_link(
                    hp,
                    "agents-hub-sync-show-everything",
                    "Show everything",
                    |this: &mut Self, _window, cx| {
                        this.sync.part = None;
                        cx.notify();
                    },
                    cx,
                )
                .into_any_element(),
            )
        } else if !visible.is_empty() {
            Some(section_hint(
                hp,
                "Fix one at a time, or all at once with the button above",
            ))
        } else {
            None
        };
        let fix_body = if visible.is_empty() {
            sync_card(
                hp,
                vec![static_fix_row(
                    hp,
                    icon_tile(hp, Some(SyncTone::Ok), ICON_CIRCLE_CHECK),
                    fix_text(
                        hp,
                        if part.is_some() && !synced {
                            "Nothing to fix for this part."
                        } else {
                            "Every agent uses your shared folder."
                        },
                        "When you install a new agent, it shows up here until you sync it.",
                    ),
                    None,
                )],
            )
        } else {
            self.render_fix_list(hp, report, &visible, cx)
        };
        let fix_section = v_flex()
            .w_full()
            .gap(px(10.0))
            .child(section_header(hp, fix_title, fix_right))
            .child(fix_body);

        let mut body = detail_body()
            .child(hero)
            .child(shared_section)
            .child(fix_section);
        if stale_lock.is_some() || untracked.is_some() {
            let mut cleanup = v_flex().w_full().gap(px(10.0));
            if let Some(stale) = stale_lock {
                let tidy = self.sync.tidy_lock;
                let switch = div()
                    .flex_shrink_0()
                    .child(modal_switch(&hp.modal, tidy, false));
                cleanup =
                    cleanup
                        .child(section_header(
                            hp,
                            "Optional cleanup",
                            Some(section_hint(
                                hp,
                                "Not included in \"fix all\" unless you turn it on",
                            )),
                        ))
                        .child(sync_card(
                            hp,
                            vec![div()
                            .id("agents-hub-sync-tidy-lock")
                            .role(gpui::Role::Switch)
                            .aria_toggled(a11y_toggled(tidy))
                            .aria_label("Tidy the skills lock file")
                            .w_full()
                            .switch_row_press(
                                switch_row_wash(hp.foreground),
                                false,
                                bind_press(cx, |this: &mut Self, _window, cx| {
                                    this.sync.tidy_lock = !this.sync.tidy_lock;
                                    cx.notify();
                                }),
                            )
                            .child(static_fix_row(
                            hp,
                            icon_tile(hp, None, ICON_LOCK),
                            fix_text(
                                hp,
                                "Tidy the skills lock file",
                                format!(
                                    "{} {} left over from skills you removed or renamed.",
                                    stale.count,
                                    if stale.count == 1 {
                                        "entry is"
                                    } else {
                                        "entries are"
                                    }
                                ),
                            ),
                            Some(switch.into_any_element()),
                        ))
                        .into_any_element()],
                        ));
            }
            if let Some(untracked) = untracked {
                cleanup = cleanup.child(
                    h_flex()
                        .items_start()
                        .gap(px(8.0))
                        .px(px(4.0))
                        .text_size(px(12.48))
                        .line_height(px(18.72))
                        .text_color(hsla(rgba_of(hp.muted, hp.muted.a * 0.85)))
                        .child(div().mt(px(2.0)).child(icon(ICON_INFO, 14.0, rgba_of(hp.muted, hp.muted.a * 0.85))))
                        .child(div().flex_1().child(format!(
                            "{} of your skills are not tracked by the skills CLI (your own and the ones Ghostex bundles). That is expected, nothing to do.",
                            untracked.count
                        ))),
                );
            }
            body = body.child(cleanup);
        }
        body.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn coverage_card(
        &self,
        hp: &HubPalette,
        part: SyncPart,
        glyph: &'static str,
        label: &'static str,
        done: usize,
        total: usize,
        caption: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.sync.part == Some(part);
        let complete = total > 0 && done >= total;
        let share = if total > 0 {
            done as f32 / total as f32
        } else {
            0.0
        };
        let hover = hp.raised_hover;
        v_flex()
            .id(label)
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .gap(px(10.0))
            .px(px(16.0))
            .py(px(14.0))
            .bg(hsla(hp.raised))
            .border_1()
            .border_color(hsla(if active { hp.ink(0.32) } else { hp.line }))
            .rounded(px(12.0))
            .hover(move |this| this.bg(hsla(hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.sync.part = if this.sync.part == Some(part) {
                    None
                } else {
                    Some(part)
                };
                cx.notify();
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .whitespace_nowrap()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(hp.foreground))
                    .child(icon(glyph, 16.0, hp.muted))
                    .child(label)
                    .child(
                        h_flex()
                            .ml_auto()
                            .gap(px(4.0))
                            .text_size(px(12.48))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(hsla(hp.muted))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(hp.foreground))
                                    .child(done.to_string()),
                            )
                            .child(format!("of {total} agents")),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(hsla(hp.ink(0.08)))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(share))
                            .rounded(px(2.0))
                            .bg(hsla(if complete { hp.ok } else { hp.warn })),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(17.4))
                    .text_color(hsla(hp.muted))
                    .child(caption),
            )
            .into_any_element()
    }

    /// One row per problem, one open at a time, so the list never turns back into a wall of text.
    fn render_fix_list(
        &self,
        hp: &HubPalette,
        report: &SyncReport,
        problems: &[&SyncProblem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut children = Vec::new();
        for (index, problem) in problems.iter().enumerate() {
            let open = self.sync.open_fix.as_deref() == Some(problem.kind.as_str());
            let copy = problem_copy(problem, report);
            let agents = problem_agents(problem, report);
            let groups: Vec<String> = problem_fix_groups(&problem.kind)
                .into_iter()
                .map(str::to_string)
                .collect();
            let kind = problem.kind.clone();
            let toggle_kind = kind.clone();
            let chevron_kind = kind.clone();
            let hover = hp.raised_hover;
            let agents_note: AnyElement = if agents.is_empty() {
                div().child("Shared folder").into_any_element()
            } else {
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        h_flex()
                            .gap(px(3.0))
                            .children(agents.iter().take(3).map(|entry| {
                                sync_agent_logo(
                                    entry.agent.icon.as_deref(),
                                    &entry.agent.display_name,
                                    18.0,
                                    hp,
                                )
                            })),
                    )
                    .child(format!(
                        "{} {}",
                        agents.len(),
                        if agents.len() == 1 { "agent" } else { "agents" }
                    ))
                    .into_any_element()
            };
            let row = h_flex()
                .id(("agents-hub-sync-fix-row", index))
                .w_full()
                .items_center()
                .gap(px(14.0))
                .min_h(px(60.0))
                .px(px(16.0))
                .py(px(12.0))
                .when(open, |this| this.bg(hsla(hover)))
                .when(!open, |this| this.hover(move |this| this.bg(hsla(hover))))
                .child(
                    h_flex()
                        .id(("agents-hub-sync-fix-toggle", index))
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(14.0))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.toggle_fix(&toggle_kind, cx);
                        }))
                        .child(icon_tile(hp, Some(copy.tone), problem_icon(&problem.kind)))
                        .child(fix_text(hp, copy.title.clone(), copy.sub.clone()))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.0))
                                .text_color(hsla(hp.muted))
                                .child(agents_note),
                        ),
                )
                .child(hub_button(
                    hp,
                    ("agents-hub-sync-fix", index),
                    HubButtonVariant::Outline,
                    HubButtonSize::Small,
                    None,
                    Some(if open { "Review fix…" } else { "Fix…" }.into()),
                    false,
                    move |this: &mut Self, _window, cx| {
                        this.open_sync_plan("all".to_string(), Some(groups.clone()), cx);
                    },
                    cx,
                ))
                .child(
                    div()
                        .id(("agents-hub-sync-fix-chevron", index))
                        .flex_shrink_0()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.toggle_fix(&chevron_kind, cx);
                        }))
                        .child(icon(
                            if open {
                                ICON_CHEVRON_DOWN
                            } else {
                                ICON_CHEVRON_RIGHT
                            },
                            16.0,
                            hp.muted,
                        )),
                );
            children.push(row.into_any_element());
            if open {
                children.push(self.render_fix_detail(hp, report, problem, &copy, &agents, cx));
            }
        }
        sync_card(hp, children)
    }

    fn toggle_fix(&mut self, kind: &str, cx: &mut Context<Self>) {
        if self.sync.open_fix.as_deref() == Some(kind) {
            self.sync.open_fix = None;
        } else {
            self.sync.open_fix = Some(kind.to_string());
        }
        cx.notify();
    }

    fn render_fix_detail(
        &self,
        hp: &HubPalette,
        report: &SyncReport,
        problem: &SyncProblem,
        copy: &ProblemCopy,
        agents: &[ProblemAgentCount],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let show_all_agents = self.sync.fix_all_agents.contains(&problem.kind);
        let show_all_paths = self.sync.fix_all_paths.contains(&problem.kind);
        let shown_agents = if show_all_agents {
            agents.len()
        } else {
            agents.len().min(WHERE_LIMIT)
        };
        let shown_paths = if show_all_paths {
            problem.items.len()
        } else {
            problem.items.len().min(PATH_LIMIT)
        };
        let label = |text: &'static str| {
            div()
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(hsla(hp.muted))
                .child(text)
        };
        let mut where_col = v_flex()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .items_start()
            .gap(px(8.0))
            .px(px(18.0))
            .pt(px(14.0))
            .pb(px(16.0))
            .child(label("WHERE"));
        if agents.is_empty() {
            where_col = where_col.child(
                div()
                    .text_size(px(12.48))
                    .text_color(hsla(hp.muted))
                    .child(format!("In your shared folder, {}", report.source.path)),
            );
        } else {
            for (index, entry) in agents.iter().take(shown_agents).enumerate() {
                let agent_id = entry.agent.id.clone();
                let hover = hp.ink(0.06);
                where_col = where_col.child(
                    h_flex()
                        .id(("agents-hub-sync-where", index))
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap(px(8.0))
                        .mx(px(-6.0))
                        .px(px(6.0))
                        .py(px(3.0))
                        .rounded(px(6.0))
                        .text_size(px(12.48))
                        .text_color(hsla(hp.foreground))
                        .hover(move |this| this.bg(hsla(hover)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.select_sync_agent(agent_id.clone(), cx);
                        }))
                        .child(sync_agent_logo(
                            entry.agent.icon.as_deref(),
                            &entry.agent.display_name,
                            18.0,
                            hp,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(SharedString::from(entry.agent.display_name.clone())),
                        )
                        .when(!entry.unit.is_empty(), |this| {
                            this.child(div().flex_shrink_0().text_color(hsla(hp.muted)).child(
                                format!(
                                    "{} {}{}",
                                    entry.count,
                                    entry.unit,
                                    if entry.count == 1 { "" } else { "s" }
                                ),
                            ))
                        }),
                );
            }
        }
        if agents.len() > shown_agents {
            let more = agents.len() - shown_agents;
            let kind = problem.kind.clone();
            where_col = where_col.child(text_link(
                hp,
                "agents-hub-sync-more-agents",
                format!(
                    "and {more} more {}",
                    if more == 1 { "agent" } else { "agents" }
                ),
                move |this: &mut Self, _window, cx| {
                    this.sync.fix_all_agents.insert(kind.clone());
                    cx.notify();
                },
                cx,
            ));
        }
        let steps = v_flex()
            .w_full()
            .gap(px(7.0))
            .children(copy.steps.iter().map(|step| {
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(8.0))
                    .text_size(px(12.48))
                    .line_height(px(18.1))
                    .text_color(hsla(rgba_of(hp.foreground, hp.foreground.a * 0.9)))
                    .child(
                        div()
                            .mt(px(2.0))
                            .child(icon(ICON_CHECK_STRONG, 14.0, hp.ok)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(SharedString::from(step.clone())),
                    )
            }));
        let mut fix_col = v_flex()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .items_start()
            .gap(px(8.0))
            .px(px(18.0))
            .pt(px(14.0))
            .pb(px(16.0))
            .border_l_1()
            .border_color(hsla(hp.line))
            .child(label("WHAT THE FIX DOES"))
            .child(steps);
        if !problem.items.is_empty() {
            fix_col = fix_col.child(
                v_flex()
                    .id("agents-hub-sync-fix-paths")
                    .w_full()
                    .max_h(px(192.0))
                    .overflow_y_scroll()
                    .px(px(10.0))
                    .py(px(8.0))
                    .bg(hsla(hp.panel))
                    .border_1()
                    .border_color(hsla(hp.line))
                    .rounded(px(8.0))
                    .font_family(super::super::native_modal_kit::MODAL_MONO_FONT)
                    .text_size(px(11.52))
                    .line_height(px(19.6))
                    .text_color(hsla(hp.muted))
                    .children(problem.items.iter().take(shown_paths).map(|item| {
                        div()
                            .w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SharedString::from(item.clone()))
                    })),
            );
        }
        if problem.items.len() > shown_paths {
            let kind = problem.kind.clone();
            fix_col = fix_col.child(text_link(
                hp,
                "agents-hub-sync-more-paths",
                format!("Show all {}", problem.items.len()),
                move |this: &mut Self, _window, cx| {
                    this.sync.fix_all_paths.insert(kind.clone());
                    cx.notify();
                },
                cx,
            ));
        }
        h_flex()
            .w_full()
            .items_stretch()
            .bg(hsla(hp.detail_fill()))
            .child(where_col)
            .child(fix_col)
            .into_any_element()
    }
}
