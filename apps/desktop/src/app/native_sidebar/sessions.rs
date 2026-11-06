use super::drag::SidebarDrag;
use super::drag::SidebarDropTarget;
use super::drag_source::SidebarDragSource;
use super::session_list::{SESSION_HEIGHT, SESSION_INSET_X, SESSION_SPACING, session_card_height};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::{Value, json};

use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarGroup, NativeSidebarSession},
};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

impl GhostexGpuiApp {
    pub(crate) fn render_native_sidebar_session(
        &self,
        group: &NativeSidebarGroup,
        session: &std::sync::Arc<NativeSidebarSession>,
        // The unscaled height of the card above this one in the same list, which a thread's tree
        // line rises to.
        above_height: f32,
        hud: &Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let view = cx.entity().clone();
        // CDXC:SessionSleep 2026-10-25 WHY: a flash stays in the map until another session completes, and its last opacity (1.0) is set after the Dim sleeping sessions fade, so a session that finished a turn and then slept stayed bright; only a running flash counts. The clock is read once, so a frame that draws the flash also asks for the next one, and the flash always ends on a frame drawn without it.
        let completion = self
            .native_sidebar
            .completion_flashes
            .get(&session.session_id)
            .map(|start| start.elapsed())
            .filter(|elapsed| elapsed.as_secs_f32() < 3.0)
            .map(super::status::completion_opacity);
        let reveal_id = session.session_id.clone();
        let session_id = session.session_id.clone();
        let group_id = group.group_id.clone();
        let drag_id = session_id.clone();
        let drag_group_id = group_id.clone();
        let dragged = SidebarDrag {
            kind: "session",
            preview: super::drag::SidebarDragPreview::Row(super::row_drag::RowDragPreview {
                identity: super::row_drag::RowDragIdentity::Session {
                    session: session.clone(),
                },
                appearance: appearance.clone(),
                width: px(0.0),
                pointer_x: px(0.0),
            }),
            id: session_id.clone(),
            title: session.title().to_owned(),
            scale: appearance.scale,
            refused: Default::default(),
        };
        // Every local row drags: a drop on an Agents pane splits it (session_pane_placement.rs),
        // and the reorder drop line still shows only when the sort mode allows a reorder.
        // Another machine's session in a merged project (`machineName`) has no place in this
        // computer's groups to move to, except a pinned one among the Pinned rows it is drawn with
        // (CDXC:Sidebar 2026-11-07 WHY in gx-core sidebar_drag/pinned_order.rs).
        let can_drag = !session.is_browser()
            && !group.is_stale
            && group.remote_machine_context.is_none()
            && (session.details.get("machineName").is_none() || session.is_pinned);
        let selected = session
            .details
            .get("isMultiSelected")
            .and_then(Value::as_bool)
            == Some(true);
        let drop_position = self.native_sidebar_session_drop_line(&session_id);
        let scale = appearance.scale;
        let card_height =
            session_card_height(session, self.native_session_has_second_line(session));
        // The row's left padding before its icon: the card inset plus its depth in a coordinator's tree.
        let indent = 5.0 + super::threads::thread_depth(session) * super::threads::THREAD_INDENT;
        let hovered = self.native_sidebar.hovered_session.as_deref() == Some(&session_id);
        // CDXC:Sessions 2026-10-28 DECISION: User: "if i disable all buttons for the session card in the sidebar please let the sidebar sessions not change when i hover over one of them". `hovered` alone still draws the row's hover fill; everything hover swaps (time, badges, status dots, icon, dimming, the button strip) follows `buttons_shown`, which is false when the strip has no button.
        let buttons_shown = hovered
            && !super::hover_actions::session_hover_items(session, group.hover_actions_expanded)
                .is_empty();
        // CDXC:Sidebar 2026-09-19 WHY: sidebar clicks and tab selections must show in the same frame (user decision in gx_store/local_focus.rs). The focused and visible fills of a local session row read the Rust store, which a selection changes in the same frame; the snapshot's flags arrive a sidebar projection later. This supersedes the click-only `optimistic_focus` mark and its 1.5 second timeout of earlier the same day.
        let (focused, visible) = self.gx_store_sidebar_row_focus(
            &session_id,
            session.is_browser(),
            session.is_focused,
            session.is_visible,
        );
        // A folded coordinator carries the active fill for the focused thread it hides.
        let highlighted = focused || super::threads::coordinator_holds_active(session);
        let stale = group.is_stale && !session.is_browser();
        // A coordinator's thread drawn here from another computer (`computerName`, threads.rs).
        let borrowed_thread = session.details.get("computerName").is_some();
        let sleeping = session.lifecycle_state.as_deref() == Some("sleeping");
        // CDXC:SessionSleep 2026-09-29 DECISION: User (issue 177): by default a sleeping row dims only its last-active time; Advanced > Dim sleeping sessions fades the whole row, and hovering it brings the row back to full strength so its hover buttons stay readable.
        let dim_sleeping = sleeping
            && !buttons_shown
            && hud["settings"]["dimSleepingSessions"].as_bool() == Some(true);
        let icon = super::icons::session_icon(session, hud, appearance, buttons_shown, hovered);
        let double_click_rename = hud["settings"]["renameSessionOnDoubleClick"].as_bool()
            == Some(true)
            && !session.is_browser();
        let context_id = session_id.clone();
        let close_id = session_id.clone();
        let context_session = session.clone();
        let tooltip_span = self
            .native_sidebar
            .session_card_bounds
            .get(&session_id)
            .map(|card| super::tooltips::SidebarTooltipSpan {
                left: card.left().as_f32(),
                right: card.right().as_f32(),
            })
            .unwrap_or_else(|| {
                super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale)
            });
        // CDXC:Tooltips 2026-09-12 DECISION: User: session-card title tooltips must always show on hover, even when the session name is short and the visible title is not truncated.
        let tooltip = session
            .details
            .get("titleTooltip")
            .and_then(Value::as_str)
            .unwrap_or(session.title())
            .to_owned();
        // A Claude cloud row says where it runs and its status under its own lines (CDXC:ClaudeCloud in claude_cloud.rs).
        let cloud_lines = super::claude_cloud::claude_cloud_tooltip_lines(session);
        let tooltip = match &cloud_lines {
            Some(lines) => format!("{tooltip}\n\n{lines}"),
            None => tooltip,
        };
        // The computer badge draws a short name; the tooltip says it in full.
        let tooltip = match session
            .details
            .get("computerName")
            .or_else(|| session.details.get("machineName"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
        {
            Some(name) => format!("{tooltip}\nOn {}", name.trim()),
            None => tooltip,
        };
        // A worktree session its parent project lists says which worktree (`worktree_badge`).
        let tooltip = match session.details.get("worktree") {
            Some(worktree) => {
                let text = |key: &str| {
                    worktree
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_owned()
                };
                let (name, branch) = (text("name"), text("branch"));
                match (name.is_empty(), branch.is_empty()) {
                    (false, false) if name != branch => {
                        format!("{tooltip}\nIn worktree {name} ({branch})")
                    }
                    (false, _) => format!("{tooltip}\nIn worktree {name}"),
                    (true, false) => format!("{tooltip}\nIn worktree {branch}"),
                    (true, true) => tooltip,
                }
            }
            None => tooltip,
        };
        // CDXC:Tooltips 2026-10-29 DECISION: User: "please let me disable the tooltip for sessions in the sidebar with a toggle (advanced setting)". `showSidebarSessionTooltips` (on unless saved off) gates only this title tooltip; this one row renderer draws every sidebar session row (project lists, a coordinator's thread rows, the peek), and the Running Agents panel has no session tooltip. Chip, badge, hover-button and project-row tooltips stay.
        let session_tooltips =
            hud["settings"]["showSidebarSessionTooltips"].as_bool() != Some(false);
        let question = session
            .details
            .get("pendingQuestionCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0;
        let question_fill = super::status::pending_question_fill(&hud["settings"], question);
        let failed =
            session.model_selection_failed || session.program_status_failed || session.stuck;
        // The red approval dot wins over every other status (CDXC:SessionStatus 2026-10-27 in status.rs).
        let approval = session.awaiting_approval;
        let folded_dots = (!failed && !approval)
            .then(|| {
                super::threads::folded_status_indicator(
                    session,
                    scale,
                    super::status::session_row_ring(
                        appearance,
                        highlighted,
                        visible,
                        question_fill,
                        hovered,
                    ),
                )
            })
            .flatten();
        let question = question && folded_dots.is_none() && !approval;
        let timer = session.details.get("timerLabel").and_then(Value::as_str);
        let show_time = hud
            .get("settings")
            .and_then(|settings| settings.get("hideLastActiveTimeOnSessionCards"))
            .and_then(Value::as_bool)
            != Some(true);
        let time = timer
            .or_else(|| {
                session
                    .details
                    .get("lastInteractionLabel")
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .to_owned();
        // What an assistive client or an e2e run reads after the title: the row's state words, which the pixels convey by colour and glyph.
        let a11y_description = [
            session.lifecycle_state.as_deref().unwrap_or(""),
            session.activity.as_str(),
            if session.is_pinned { "pinned" } else { "" },
            if session.is_draft { "draft" } else { "" },
            if cloud_lines.is_some() {
                "runs in Claude's cloud"
            } else {
                ""
            },
            if approval { "waiting for approval" } else { "" },
            if question { "needs an answer" } else { "" },
            time.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        div().on_children_prepainted(move |bounds, window, cx| { if completion.is_some() { window.request_animation_frame(); cx.notify(view.entity_id()); }
            if let Some(bounds) = bounds.first() { view.update(cx, |app, cx| { if app.native_sidebar.session_card_bounds.get(&reveal_id) != Some(bounds) { app.native_sidebar.session_card_bounds.insert(reveal_id.clone(), *bounds); } app.reveal_native_session_bounds(&reveal_id, *bounds, scale, window, cx) }); } }).w_full().pb(px(SESSION_SPACING * scale)).px(px(SESSION_INSET_X * scale))
            .child(div()
                .id(format!("native-sidebar-session-{session_id}"))
                .role(gpui::Role::TreeItem)
                .aria_label(session.title().to_owned())
                .aria_description(a11y_description)
                .aria_selected(focused)
                .relative().h(px(card_height * scale)).w_full().min_w_0().flex().flex_col().rounded(px(5.0 * scale))
                .cursor_default()
                .when(stale, |row| row.opacity(0.55))
                .when(dim_sleeping && !stale, |row| row.opacity(0.5))
                .when(self.native_sidebar.is_dragging("session", &session_id), |row| row.opacity(0.2))
                .when_some(completion, |row, opacity| row.opacity(opacity))
                .when(visible && !highlighted, |row| row.bg(appearance.visible))
                .when(highlighted, |row| row.bg(appearance.session_selected))
                .when(visible || highlighted, |row| row.text_color(chrome_color(0xd8d8d8, 0x292929)))
                .when(selected, |row| row.child(super::decorations::multi_selected_outline(scale)))
                // CDXC:Sidebar 2026-09-24 DECISION: User: the drop line between session cards is a straight line, not a top or bottom border that bends into the card's rounded corners; it is the same line project and collection rows use, drawn in the gap beside the card.
                .when_some(drop_position, |row, position| row.child(super::drag::drop_line(position, scale)))
                .when(!highlighted, |row| row.hover(|row| row.bg(appearance.session_hover)))
                .when(highlighted, |row| row.child(super::decorations::session_outline(appearance)))
                .when_some(question_fill, |row, fill| row.bg(fill).hover(move |row| row.bg(fill)))
                .children(super::threads::thread_tree_lines(session, card_height, above_height, appearance))
                .children(self.render_native_session_decorations(session, appearance, cx))
                .when_some(self.native_sidebar.reveal_flash.as_ref().filter(|(id, _)| id == &session.session_id).map(|(_, start)| *start), |row, start| row.child(super::scroll::reveal_flash(start, scale)))
                // Line 1 is the whole card of every row without work links, and the absolute
                // decorations above keep their tops because it starts at the card's top.
                .child(h_flex().h(px(SESSION_HEIGHT * scale)).flex_shrink_0().w_full().min_w_0().pl(px(indent * scale)).pr(px(6.0 * scale)).gap(px(6.0 * scale))
                .child(self.render_native_session_identity(session, icon, hovered, appearance, cx))
                .child(div().id(format!("native-session-title-{session_id}")).flex_1().min_w_0().h_full().flex().items_center().child(title_with_rename_shake(div().min_w_0().truncate().child(session.title().to_owned()), &session_id, session.rename_failed_at.as_deref(), scale)).when(session_tooltips && self.native_sidebar.pointer_inside && self.native_sidebar.menu.is_none() && !cx.has_active_drag(), |row| row.managed_discrete_tooltip_with_placement(tooltip_span.placement(), appearance.tooltip_delay, move |window, cx| super::tooltips::sidebar_tooltip(tooltip.clone(), tooltip_span, scale, window, cx))))
                // CDXC:Sidebar 2026-10-29 DECISION: User: "please swap the order of these 2 things (the threads one for orchestrator with the link tag)". The links badge comes before a coordinator's workers badge; the phone's SessionRow.tsx follows.
                .when(!buttons_shown, |row| row.children(self.render_native_session_links_badge(session, appearance)))
                .when(!buttons_shown, |row| row.children(super::threads::coordinator_badge(session, appearance)))
                .when(!buttons_shown, |row| row.children(super::threads::worktree_badge(session, appearance)))
                .when(!buttons_shown, |row| row.children(super::threads::computer_badge(session, appearance)))
                .when(!buttons_shown, |row| row.children(self.render_voice_call_badge(&session.session_id, appearance)))
                .when(!buttons_shown, |row| row.children(super::agentbox::agentbox_badge(session, appearance, tooltip_span, self.native_sidebar.pointer_inside && self.native_sidebar.menu.is_none() && !cx.has_active_drag())))
                .when(!buttons_shown, |row| row.children(super::cloud_box::cloud_box_badge(session, appearance, tooltip_span, self.native_sidebar.pointer_inside && self.native_sidebar.menu.is_none() && !cx.has_active_drag())))
                .when(!buttons_shown && folded_dots.is_none() && !question && !approval, |row| row.children(super::status::activity_indicator(&session.activity, session.has_background_work, failed, scale)))
                // CDXC:SessionStatus 2026-09-13 DECISION: User: hide Last Active while the question dot is shown so they do not overlap, including when the session is not working.
                .when(!buttons_shown && folded_dots.is_none() && !question && !approval && !failed && (timer.is_some() || (show_time && session.activity != "working" && session.activity != "attention" && !session.has_background_work)), |row| row.child(div().text_size(px(13.55 * scale)).text_color(if sleeping { chrome_color(0x686868, 0x959595) } else { chrome_color(0xa6a6a6, 0x424242) }).child(time)))
                .when(buttons_shown, |row| row.child(self.render_native_session_hover_actions(group, session, appearance, cx)))
                .when(approval, |row| row.child(super::status::approval_indicator(scale)))
                .when(question, |row| row.child(super::status::question_indicator(&session.activity, scale, super::status::session_row_ring(appearance, highlighted, visible, question_fill, hovered))))
                .children(folded_dots))
                .children(self.render_native_session_work_chips(session, indent, tooltip_span, appearance, cx))
                .children(self.render_native_session_link_line(session, indent, tooltip_span, appearance, cx))
                .when(can_drag && self.native_sidebar.menu.is_none(), |row| row.sidebar_drag_source(dragged, cx))
.sidebar_drop_target("session", drag_id, Some(drag_group_id), cx)
                .on_mouse_down(MouseButton::Middle, |_, window, _| {
                    window.prevent_default();
                })
                .on_aux_click(cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                    if !event.is_middle_click() { return; }
                    window.prevent_default();
                    cx.stop_propagation();
                    app.close_native_sidebar_menu(window, cx);
                    app.dispatch_native_sidebar_command(
                        json!({"type": "closeSession", "sessionId": close_id, "closeSource": "sidebarMiddleClick"}),
                        cx,
                    );
                }))
                .on_click(cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    if event.click_count() == 2 && double_click_rename {
                        app.dispatch_native_sidebar_ui(json!({"type": "sessionAction", "sessionId": session_id, "action": "rename"}), cx);
                        return;
                    }
                    if stale { return; }
                    let modifiers = event.modifiers();
                    let mode = if modifiers.shift { "range" } else if modifiers.platform || modifiers.control { "additive" } else { "focus" };
                    // A coordinator's thread nested from another computer this app has not connected to yet: connect, then open it.
                    if mode == "focus" && borrowed_thread && app.open_borrowed_thread_row_when_connected(&session_id, cx) { return; }
                    // The page Claude's sidebar button opened over the sessions closes first, so the
                    // session shows (CDXC:Extensions 2026-10-25 in app/claude_web/sidebar_button.rs).
                    if mode == "focus" { app.close_sidebar_extension_view(window, cx); }
                    app.dispatch_native_sidebar_ui(json!({"type": "selectSession", "sessionId": session_id, "mode": mode}), cx);
                    if mode == "focus" {
                        let _ = app.react_to_native_sidebar_session_click(&session_id, cx);
                        app.reveal_floating_sessions(cx);
                    }
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if !selected { app.dispatch_native_sidebar_ui(json!({"type": "selectSession", "mode": "clear", "sessionId": context_id}), cx); }
                    Self::show_native_sidebar_context_menu(context_session.details.get("menu").unwrap_or(&Value::Null), app.native_sidebar.session_card_bounds.get(&context_id).copied(), event.position, scale, window, cx);
                })))
            .into_any_element()
    }
}

/// How long the title shakes after a rename the agent never confirmed.
const RENAME_SHAKE_DURATION: std::time::Duration = std::time::Duration::from_millis(480);

/// The row's title, shaken once sideways (like a rejected password field) when gxserver has just
/// given up on the user's rename. The animation is keyed by the failure, so a later failure on the
/// same session shakes again; `ghostex_gx_core::rename_shake_key` holds the rule.
fn title_with_rename_shake(
    title: gpui::Div,
    session_id: &str,
    rename_failed_at: Option<&str>,
    scale: f32,
) -> AnyElement {
    use gpui::AnimationExt as _;
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default();
    let Some(failed_at) = ghostex_gx_core::rename_shake_key(rename_failed_at, now) else {
        return title.into_any_element();
    };
    title
        .relative()
        .with_animation(
            gpui::ElementId::Name(format!("rename-shake-{session_id}-{failed_at}").into()),
            gpui::Animation::new(RENAME_SHAKE_DURATION),
            move |title, delta| {
                let offset = (delta * std::f32::consts::PI * 6.0).sin() * 5.0 * (1.0 - delta);
                title.left(px(offset * scale))
            },
        )
        .into_any_element()
}
