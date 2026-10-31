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
use crate::app::helpers::indicator_animation::ThrottledAnimationExt as _;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::status::{FAILED_COLOR, QUESTION_COLOR, WORKING_COLOR};

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
fn status_look(status: UnifiedStatus, appearance: &SidebarAppearance) -> (&'static str, Hsla) {
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

/// The status word with its icon and, while the agent works, its timer.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User (Q786): a Unified card shows its status as an icon, a word and a live timer, in Ghostex's status colours, where a classic row draws a dot.
pub(super) fn status_slot(
    card: &UnifiedCard,
    appearance: &SidebarAppearance,
    now: i64,
) -> Option<AnyElement> {
    let status = card.status?;
    let scale = appearance.scale;
    let (icon, color) = status_look(status, appearance);
    let timer = (status == UnifiedStatus::Working)
        .then(|| card.row.timing.working_started_ms)
        .flatten()
        .map(|started| elapsed_label(now - started));
    let glyph = gpui::svg()
        .path(icon)
        .size(px(13.0 * scale))
        .flex_shrink_0()
        .text_color(color);
    let glyph = if status == UnifiedStatus::Working {
        glyph
            .with_throttled_animation(
                format!("unified-working-{}", card.session.session_id),
                std::time::Duration::from_millis(3000),
                |icon, progress| {
                    icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                        progress,
                    )))
                },
            )
            .into_any_element()
    } else {
        glyph.into_any_element()
    };
    Some(
        h_flex()
            .flex_shrink_0()
            .gap(px(4.0 * scale))
            .text_size(px(12.5 * scale))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(color)
            .child(glyph)
            .child(status.label())
            .children(timer.map(|timer| {
                div()
                    .font_weight(gpui::FontWeight::NORMAL)
                    .opacity(0.85)
                    .child(timer)
            }))
            .into_any_element(),
    )
}

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
        row.on_prepaint(crate::app::native_sidebar::menus::record_menu_owner(&owner))
            .on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
                let current =
                    app.native_sidebar.hovered_session.as_deref() == Some(hover_id.as_str());
                if *hovered && !current {
                    app.native_sidebar.hovered_session = Some(hover_id.clone());
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

    /// A card: three lines, or two when there is no branch or pull request to show.
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
        let project = match &card.machine_label {
            Some(machine) => format!("{} · {machine}", card.repo_label),
            None => card.repo_label.clone(),
        };
        let parking = self
            .native_sidebar
            .unified
            .as_ref()
            .is_some_and(|unified| unified.parking);
        let right: AnyElement = if hovered {
            self.render_unified_card_actions(card, parking, appearance, cx)
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
        let has_meta = card.row.menu_facts.branch.is_some() || pull_request.is_some();
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
        let card_row = div()
            .id(format!("native-unified-card-{}", session.session_id))
            .role(gpui::Role::TreeItem)
            .aria_label(session.title().to_owned())
            .aria_selected(focused)
            .w_full()
            .flex()
            .flex_col()
            .gap(px(3.0 * scale))
            .px(px(8.0 * scale))
            .py(px(7.0 * scale))
            .rounded(px(5.0 * scale))
            .relative()
            .cursor_default()
            .when(!focused, |row| {
                row.hover(|row| row.bg(appearance.session_hover))
            })
            .child(
                h_flex()
                    .h(px(18.0 * scale))
                    .gap(px(6.0 * scale))
                    .child(repo_tile(card, appearance, show_project_icons(hud), false))
                    // CDXC:Sidebar 2026-10-30 DECISION:
                    // User: "make the pin icon not look like this, it should be the other pin icon we're using in the app already". A pinned card shows the filled pin the sidebar's Links list and account usage button use for something pinned (`pin-filled.svg`), at their 13px, not the `pin.svg` outline the user turned down before.
                    .when(session.is_pinned, |line| {
                        line.child(
                            gpui::svg()
                                .path("titlebar/pin-filled.svg")
                                .size(px(13.0 * scale))
                                .flex_shrink_0()
                                .text_color(appearance.muted),
                        )
                    })
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
                    .child(right),
            )
            .child(
                h_flex()
                    .min_h(px(20.0 * scale))
                    .gap(px(6.0 * scale))
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
                    .when(!has_meta, |line| line.child(agent())),
            )
            .when(has_meta, |column| {
                column.child(
                    h_flex()
                        .h(px(17.0 * scale))
                        .gap(px(5.0 * scale))
                        .text_size(px(12.5 * scale))
                        .font_weight(gpui::FontWeight::NORMAL)
                        .text_color(appearance.muted)
                        .children(card.row.menu_facts.branch.clone().map(|branch| {
                            h_flex()
                                .min_w_0()
                                .flex_1()
                                .gap(px(4.0 * scale))
                                .child(
                                    gpui::svg()
                                        .path("titlebar/git-branch.svg")
                                        .size(px(12.0 * scale))
                                        .flex_shrink_0()
                                        .text_color(appearance.muted),
                                )
                                .child(div().min_w_0().truncate().child(branch))
                        }))
                        .when(card.row.menu_facts.branch.is_none(), |line| {
                            line.child(div().flex_1())
                        })
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
            .pb(px(2.0 * scale))
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
