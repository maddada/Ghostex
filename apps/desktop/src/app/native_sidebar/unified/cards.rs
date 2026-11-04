//! A Unified sidebar card: the project and the status on top, the title, then the branch, the pull
//! request and the agent.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::{ElementExt as _, h_flex};
use serde_json::{Value, json};

use ghostex_gx_core::unified_sidebar::UnifiedStatus;

use super::model::UnifiedCard;
use crate::GhostexGpuiApp;
use crate::app::helpers::chrome_color;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::status::{self, FAILED_COLOR, QUESTION_COLOR, WORKING_COLOR};

pub(super) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// `38s`, `4m 12s`, `1h 5m`: how long the agent has worked.
pub(super) fn elapsed_label(ms: i64) -> String {
    let seconds = (ms / 1000).max(0);
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// The icon and colour of a status word, in the classic row's status colours.
pub(super) fn status_look(
    status: UnifiedStatus,
    appearance: &SidebarAppearance,
) -> (&'static str, Hsla) {
    match status {
        UnifiedStatus::Approval => ("titlebar/alert-triangle.svg", rgb(FAILED_COLOR).into()),
        UnifiedStatus::Question => ("titlebar/help-circle.svg", rgb(QUESTION_COLOR).into()),
        UnifiedStatus::Failed => ("titlebar/alert-circle.svg", rgb(FAILED_COLOR).into()),
        UnifiedStatus::Working => ("titlebar/circle-dashed.svg", rgb(WORKING_COLOR).into()),
        UnifiedStatus::Waiting => ("titlebar/circle-dot.svg", appearance.muted),
        UnifiedStatus::Done => (
            "titlebar/circle-check.svg",
            chrome_color(0x95d7f6, 0x2f8fd0).into(),
        ),
    }
}

/// The status dot and, while the agent works, its timer.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User (Q786): a Unified card shows its status in Ghostex's status colours, with a live timer while the agent works.
///
/// CDXC:Sidebar 2026-11-04 DECISION:
/// User: "Please don't say Working or Question or Done for the threads in the sidebar, should just show the icon", and "we should make the indicator dots stay in 1 place please instead of moving around when the time keeps changing". The status word is gone; the dot sits on the card's right edge, where a Project sidebar row draws its dot, and the timer grows leftward from it, so the dot never moves as the seconds tick. The word stays the slot's accessible label.
pub(super) fn status_slot(
    card: &UnifiedCard,
    appearance: &SidebarAppearance,
    now: i64,
) -> Option<AnyElement> {
    let status = card.status?;
    let scale = appearance.scale;
    let (_, color) = status_look(status, appearance);
    let timer = (status == UnifiedStatus::Working)
        .then(|| card.row.timing.working_started_ms)
        .flatten()
        .map(|started| elapsed_label(now - started));
    /*
    CDXC:Sidebar 2026-11-04 DECISION:
    User (Q1230): the Unified card shows the classic row's static status dots ("use the same icons we have for the non-unified sidebar pls"). The working icon used to spin, and each of its frames rebuilt and laid out the whole sidebar thirty times a second while any agent worked: most of the app's CPU in a profile (issues-1027/macos-perf). The live timer has its own once-a-second tick (list.rs).
    */
    let row = &card.row;
    let glyph = match status {
        UnifiedStatus::Approval => status::approval_indicator(scale),
        UnifiedStatus::Question => status::question_indicator(
            &row.activity,
            scale,
            status::session_row_ring(appearance, false, false, None, false),
        ),
        _ => status::activity_indicator(
            &row.activity,
            row.has_background_work,
            row.model_selection_failed || row.program_status_failed,
            scale,
        )?,
    };
    Some(
        h_flex()
            .id("unified-card-status")
            .aria_label(status.label())
            .flex_shrink_0()
            .gap(px(4.0 * scale))
            .text_size(px(12.5 * scale))
            .text_color(color)
            .children(timer.map(|timer| div().opacity(0.85).child(timer)))
            .child(glyph)
            .into_any_element(),
    )
}

/// A card's padding, the gap between its lines and the lines' heights, before the zoom.
const CARD_PADDING_Y: f32 = 7.0;
const CARD_LINE_GAP: f32 = 3.0;
const CARD_PROJECT_LINE: f32 = 18.0;
/// The sidebar's text at GPUI's default line height, which is what the title line comes to.
const CARD_TITLE_LINE: f32 =
    crate::app::native_sidebar::appearance::SIDEBAR_TEXT_SIZE * std::f32::consts::GOLDEN_RATIO;
const CARD_META_LINE: f32 = 17.0;
/// The space under a card, before the next one.
const CARD_GAP: f32 = 2.0;

/// Whether project icons show, as the Project sidebar's headers follow it.
pub(super) fn show_project_icons(hud: &Value) -> bool {
    hud["settings"]["showProjectIcons"].as_bool() != Some(false)
}

/// The project's icon, or its repository's first letter on a tile when it has none.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: "please make sure we make the icons of projects show up in the unified sidebar". A card's tile is the icon the Project sidebar's project header shows (the one set for the project, else the one found in its folder), on every machine; a repository whose projects have one on any machine shows it on all its cards. The letter tile stays for a repository with no icon, and for every card while Show project icons is off.
pub(super) fn repo_tile(
    card: &UnifiedCard,
    appearance: &SidebarAppearance,
    show_icon: bool,
    dim: bool,
) -> AnyElement {
    let tile = match card.repo_icon.clone().filter(|_| show_icon) {
        Some(icon) => gpui::img(icon)
            .size(px(16.0 * appearance.scale))
            .flex_shrink_0()
            .into_any_element(),
        None => crate::app::native_sidebar::project_header::letter_tile(
            &card.repo_label,
            appearance.muted.opacity(0.12),
            appearance.muted,
            appearance.scale,
        ),
    };
    if dim {
        div().opacity(0.5).child(tile).into_any_element()
    } else {
        tile
    }
}

/// Marks a card or a slim row as the open session: its fill, and an outline drawn over it.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: "when unified sidebar is enabled, when i activate a session in the sidebar then we're kind of shifting the elements in that session card in the sidebar which is bad please don't do this. the contents of that session in the sidebar have to stay static". The active card used to take a real 1px border, which moved everything inside it by that pixel and made it 2px taller, so the cards below moved too. The outline is now the Project sidebar's own overlay (`session_outline`, CDXC:Sidebar 2026-10-25 in decorations.rs), which takes no layout space, so activating a session changes the fill, the outline and the title's brightness and moves nothing.
pub(super) fn mark_active<E: Styled + ParentElement>(row: E, fill: Hsla, outline: AnyElement) -> E {
    row.bg(fill).child(outline)
}

impl GhostexGpuiApp {
    /// The click, right-click and hover a card and a slim row share with the classic row.
    pub(super) fn unified_row_interactions(
        &self,
        row: gpui::Stateful<gpui::Div>,
        card: &UnifiedCard,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let session_id = card.session.session_id.clone();
        let hover_id = session_id.clone();
        let menu = card
            .session
            .details
            .get("menu")
            .cloned()
            .unwrap_or(Value::Null);
        let scale = self
            .native_sidebar
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot.hud["agentManagerZoomPercent"]
                    .as_f64()
                    .unwrap_or(100.0) as f32
                    / 100.0
            })
            .unwrap_or(1.0);
        // The menu hangs below the card, as a Project sidebar row's does.
        let owner = crate::app::native_sidebar::menus::MenuOwner::default();
        let record_owner = crate::app::native_sidebar::menus::record_menu_owner(&owner);
        // A card waiting to be scrolled to (a new session in Drafts, gx_store/draft_arrivals.rs)
        // reports where it was drawn, as a Project sidebar row does.
        let reveal = self
            .native_sidebar
            .pending_reveal
            .as_ref()
            .is_some_and(|request| request.session_id == session_id);
        // Where the card was drawn, as a Project sidebar row records it: the links card opens
        // beside it and the hover strip's menus hang under it.
        let view = cx.entity().downgrade();
        let bounds_id = session_id.clone();
        // A card with links opens the Project sidebar's floating links card on hover.
        let opens_links_card = self.native_session_links_row(&card.session).hover_card;
        row.on_prepaint(move |bounds, window, app| {
            record_owner(bounds, window, app);
            let Some(view) = view.upgrade() else {
                return;
            };
            view.update(app, |this, cx| {
                if this.native_sidebar.session_card_bounds.get(&bounds_id) != Some(&bounds) {
                    this.native_sidebar
                        .session_card_bounds
                        .insert(bounds_id.clone(), bounds);
                }
                if reveal {
                    this.reveal_native_session_bounds(&bounds_id, bounds, scale, window, cx)
                }
            });
        })
        .on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
            let current = app.native_sidebar.hovered_session.as_deref() == Some(hover_id.as_str());
            if *hovered && !current {
                app.native_sidebar.hovered_session = Some(hover_id.clone());
                if opens_links_card {
                    app.open_session_links_card_after_delay(hover_id.clone(), cx);
                }
                cx.notify();
            } else if !*hovered && current {
                app.native_sidebar.hovered_session = None;
                cx.notify();
            }
        }))
        .on_click(
            cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                let modifiers = event.modifiers();
                if modifiers.shift || modifiers.platform || modifiers.control {
                    return;
                }
                app.close_sidebar_extension_view(window, cx);
                app.dispatch_native_sidebar_ui(
                    json!({"type": "selectSession", "sessionId": session_id, "mode": "focus"}),
                    cx,
                );
                let _ = app.react_to_native_sidebar_session_click(&session_id, cx);
                app.reveal_floating_sessions(cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |_, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                Self::show_native_sidebar_context_menu(
                    &menu,
                    owner.get(),
                    event.position,
                    scale,
                    window,
                    cx,
                );
            }),
        )
    }

    /// A card: three lines, or two when there is no branch or pull request to show and its folder
    /// is not known to be outside git.
    ///
    /// CDXC:Sidebar 2026-10-30 DECISION:
    /// User (Q785, Q789): a Unified row is a three-line card. Line one names the project by its repository, followed by the machine's name for a session on another machine, with the status on the right; line two is the title; line three is the branch, the pull request and the agent. Hovering it shows Snooze and Park in the status's place (hover.rs).
    pub(in crate::app::native_sidebar) fn render_native_unified_card(
        &self,
        card: &UnifiedCard,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let session = &card.session;
        let now = now_ms();
        let hovered = self.native_sidebar.hovered_session.as_deref() == Some(&session.session_id);
        let (focused, _) = self.gx_store_sidebar_row_focus(
            &session.session_id,
            false,
            session.is_focused,
            session.is_visible,
        );
        let needs_you = matches!(
            card.status,
            Some(
                UnifiedStatus::Approval
                    | UnifiedStatus::Question
                    | UnifiedStatus::Done
                    | UnifiedStatus::Failed
            )
        );
        // A row that waits on nobody recedes, so the ones that need you stand out.
        let recede = !focused
            && !needs_you
            && matches!(
                card.status,
                None | Some(UnifiedStatus::Working | UnifiedStatus::Waiting)
            );
        // CDXC:Sidebar 2026-10-31 DECISION: User (Q849): under a project header a card drops its project line, keeping the computer's name for a session on another machine. With nothing left on that line, its status or time and its hover strip move up beside the title.
        let project = match (&card.machine_label, card.in_group) {
            (Some(machine), true) => machine.clone(),
            (None, true) => String::new(),
            (Some(machine), false) => format!("{} · {machine}", card.repo_label),
            (None, false) => card.repo_label.clone(),
        };
        let title_line_only = card.in_group && card.machine_label.is_none();
        // The strip of the Unified sidebar, or the status when the user turned every button off.
        let strip = hovered
            .then(|| self.render_unified_card_strip(card, appearance, cx))
            .flatten();
        let right: AnyElement = if let Some(strip) = strip {
            strip
        } else {
            status_slot(card, appearance, now).unwrap_or_else(|| {
                div()
                    .flex_shrink_0()
                    .text_size(px(13.55 * scale))
                    .text_color(chrome_color(0xa6a6a6, 0x424242))
                    .child(
                        card.row
                            .last_interaction_label(now.max(0) as u64)
                            .unwrap_or_default(),
                    )
                    .into_any_element()
            })
        };
        let pull_request = card
            .row
            .work
            .as_ref()
            .and_then(|work| work.pull_request.as_ref());
        /*
        CDXC:Sidebar 2026-11-04 DECISION:
        User: "for threads on madda folder which doesn't have github it just shows 2 rows (it shouldn't do this, should say something that indicates that it's without source control for that thread, with an icon there too to indicate this (no need for different color tho)". A card whose folder is in no git repository (gx-core unified_sidebar `no_source_control`) keeps its third line, reading "No source control" beside a crossed-out branch, in the branch line's colour, so every card is the same height.
        */
        let no_source_control = card.no_source_control;
        let branch_line = card
            .row
            .menu_facts
            .branch
            .clone()
            .map(|branch| ("titlebar/git-branch.svg", branch))
            .or_else(|| {
                no_source_control.then(|| {
                    (
                        "titlebar/git-branch-deleted.svg",
                        "No source control".to_string(),
                    )
                })
            });
        let has_branch_line = branch_line.is_some();
        let has_meta = has_branch_line || pull_request.is_some();
        let agent = || {
            div().flex_shrink_0().opacity(0.75).child(
                crate::app::native_sidebar::icons::session_icon(
                    session, hud, appearance, false, false,
                ),
            )
        };
        let title_color = if needs_you || focused {
            Hsla::from(chrome_color(0xd8d8d8, 0x292929))
        } else {
            appearance.foreground
        };
        let tooltip = [
            Some(session.title().to_owned()),
            Some(card.repo_label.clone()),
            card.machine_label.clone(),
            card.row.menu_facts.branch.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        let span = crate::app::native_sidebar::tooltips::SidebarTooltipSpan::sidebar(
            self.sidebar_width,
            scale,
        );
        let tooltips = hud["settings"]["showSidebarSessionTooltips"].as_bool() != Some(false)
            && self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none();
        let mut right = Some(right);
        let pin = || {
            gpui::svg()
                .path("titlebar/pin-filled.svg")
                .size(px(13.0 * scale))
                .flex_shrink_0()
                .text_color(appearance.muted)
        };
        let card_row = div()
            .id(format!("native-unified-card-{}", session.session_id))
            .role(gpui::Role::TreeItem)
            .aria_label(session.title().to_owned())
            .aria_selected(focused)
            .w_full()
            .flex()
            .flex_col()
            .gap(px(CARD_LINE_GAP * scale))
            .px(px(8.0 * scale))
            .py(px(CARD_PADDING_Y * scale))
            .rounded(px(5.0 * scale))
            .relative()
            .cursor_default()
            .when(!focused, |row| {
                row.hover(|row| row.bg(appearance.session_hover))
            })
            .when(!title_line_only, |column| {
                column.child(
                    h_flex()
                        .h(px(CARD_PROJECT_LINE * scale))
                        .gap(px(6.0 * scale))
                        .when(!card.in_group, |line| {
                            line.child(repo_tile(card, appearance, show_project_icons(hud), false))
                        })
                        // CDXC:Sidebar 2026-10-30 DECISION:
                        // User: "make the pin icon not look like this, it should be the other pin icon we're using in the app already". A pinned card shows the filled pin the sidebar's Links list and account usage button use for something pinned (`pin-filled.svg`), at their 13px, not the `pin.svg` outline the user turned down before.
                        .when(session.is_pinned, |line| line.child(pin()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.5 * scale))
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_color(appearance.muted)
                                .child(project),
                        )
                        .children(right.take()),
                )
            })
            .child(
                h_flex()
                    .min_h(px(CARD_TITLE_LINE * scale))
                    .gap(px(6.0 * scale))
                    .when(title_line_only && session.is_pinned, |line| {
                        line.child(pin())
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(title_color)
                            // CDXC:Sidebar 2026-10-30 DECISION:
                            // User: "let's please not change the weight of any of the text when i change the currently active session card". Every line keeps the sidebar's own weight whether the card is active, needs you or neither (opening a Done card also clears its Done), so nothing reflows when the selection moves; the active card shows by its background, outline and brighter title, as a Project sidebar row does.
                            .when(recede, |title| title.opacity(0.75))
                            .child(session.title().to_owned()),
                    )
                    // A coordinator's workers are no cards (CDXC:Sidebar 2026-11-04 in gx-core
                    // unified_sidebar/build.rs), so its card carries the Project sidebar's crew
                    // badge: how many wait for approval, wait on you or work; nothing while idle.
                    .children(crate::app::native_sidebar::threads::coordinator_badge(
                        session, appearance,
                    ))
                    .when(!has_meta && !(title_line_only && hovered), |line| {
                        line.child(agent())
                    })
                    .children(right.take()),
            )
            .when(has_meta, |column| {
                column.child(
                    h_flex()
                        .h(px(CARD_META_LINE * scale))
                        .gap(px(5.0 * scale))
                        .text_size(px(12.5 * scale))
                        .font_weight(gpui::FontWeight::NORMAL)
                        .text_color(appearance.muted)
                        .children(branch_line.map(|(icon, branch)| {
                            h_flex()
                                .min_w_0()
                                .flex_1()
                                .gap(px(4.0 * scale))
                                .child(
                                    gpui::svg()
                                        .path(icon)
                                        .size(px(12.0 * scale))
                                        .flex_shrink_0()
                                        .text_color(appearance.muted),
                                )
                                .child(div().min_w_0().truncate().child(branch))
                        }))
                        .when(!has_branch_line, |line| line.child(div().flex_1()))
                        .children(pull_request.map(|pull_request| {
                            let color: Hsla = match pull_request.state.as_str() {
                                "merged" => chrome_color(0xa371f7, 0x8250df).into(),
                                "closed" => rgb(FAILED_COLOR).into(),
                                "draft" => appearance.muted,
                                _ => chrome_color(0x3fb950, 0x1a7f37).into(),
                            };
                            h_flex()
                                .flex_shrink_0()
                                .gap(px(2.0 * scale))
                                .text_color(color)
                                .child(
                                    gpui::svg()
                                        .path("titlebar/git-pull-request.svg")
                                        .size(px(12.0 * scale))
                                        .text_color(color),
                                )
                                .child(format!("#{}", pull_request.number))
                        }))
                        .child(agent()),
                )
            })
            .when(focused, |row| {
                mark_active(
                    row,
                    appearance.session_selected,
                    crate::app::native_sidebar::decorations::session_outline(appearance),
                )
            });
        let card_row = self.unified_row_interactions(card_row, card, cx);
        div()
            .w_full()
            .pb(px(CARD_GAP * scale))
            .px(px(3.0 * scale))
            .child(if tooltips {
                card_row
                    .managed_discrete_tooltip_with_placement(
                        span.placement(),
                        appearance.tooltip_delay,
                        move |window, cx| {
                            crate::app::native_sidebar::tooltips::sidebar_tooltip(
                                tooltip.clone(),
                                span,
                                scale,
                                window,
                                cx,
                            )
                        },
                    )
                    .into_any_element()
            } else {
                card_row.into_any_element()
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row's frame, as the card and the slim row start theirs.
    fn frame() -> gpui::Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .px(px(8.0))
            .py(px(7.0))
            .rounded(px(5.0))
            .relative()
    }

    #[test]
    fn opening_a_session_changes_its_rows_fill_and_no_layout() {
        let mut inactive = frame();
        let mut active = mark_active(
            frame(),
            rgb(0x333333).into(),
            div().absolute().inset_0().into_any_element(),
        );
        assert!(active.style().background.is_some());
        active.style().background = None;
        assert_eq!(inactive.style(), active.style());
    }
}
