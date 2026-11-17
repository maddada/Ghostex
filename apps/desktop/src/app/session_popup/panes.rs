//! Splits inside a popup: a tab dragged onto a pane's edge splits the popup there, side by side or
//! above and below, and a pane's tab dragged back onto another pane's tab bar unsplits it. The
//! layout, the drop zones, their feedback and the dividers are the work area's
//! (`WorkspaceModel::split_tab_to_pane`, `workspace_pane_body_drop_zone`,
//! `render_workspace_pane_drop_feedback`, the resize rail and its ratio rules).
//!
//! CDXC:SessionPopup 2026-11-17 DECISION:
//! User: inside a popup, dragging a tab onto the content splits it, side by side or above and below, like the main window's split-pane drop zones; dragging a pane back onto the tab bar unsplits it. Sessions of different projects may split together here, since a popup isn't tied to one project's sidebar, and each pane shows which project it belongs to, the way split panes in the main window show their session. The main window's split and drop-zone code is reused, not built again.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement as _, Styled as _, Window, div, px, relative,
};
use gpui_component::{h_flex, v_flex};

use super::{SessionPopupId, SessionPopupTabDrag};
use crate::GhostexGpuiApp;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::resize_rail::{
    ResizeRailGrabSide, resize_rail_deferred_strip, resize_rail_grab_strip, resize_rail_hover_line,
};

impl GhostexGpuiApp {
    /// Popup `id`'s panes: its split tree, or its one pane.
    pub(crate) fn render_session_popup_node(
        &mut self,
        id: SessionPopupId,
        node: &WorkspaceNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            WorkspaceNode::Leaf(leaf) => {
                self.render_session_popup_pane(id, leaf.pane_id, window, cx)
            }
            WorkspaceNode::Split(split) => {
                let split_id = split.id;
                let axis = split.axis;
                let ratio = workspace_split_ratio(split.ratio);
                let first = self.render_session_popup_node(id, &split.first, window, cx);
                let second = self.render_session_popup_node(id, &split.second, window, cx);
                let side = |child: AnyElement, grow: f32, half: &str| {
                    div()
                        .id(gpui::SharedString::from(format!(
                            "session-popup-split-{}-{half}",
                            split_id.0
                        )))
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .min_h_0()
                        .when(axis == WorkspaceSplitAxis::Horizontal, |side| side.h_full())
                        .flex_grow(grow)
                        .flex_shrink_1()
                        .flex_basis(relative(0.0))
                        .child(child)
                };
                let view = cx.entity().downgrade();
                let row = match axis {
                    WorkspaceSplitAxis::Horizontal => h_flex().items_start(),
                    WorkspaceSplitAxis::Vertical => v_flex(),
                };
                row.on_children_prepainted(move |child_bounds, _, cx| {
                    let _ = view.update(cx, |app, _| {
                        if let Some(span) = split_resize_content_span(&child_bounds, axis)
                            && let Some(popup) = app.session_popups.get_mut(id)
                        {
                            popup
                                .split_metrics
                                .insert(split_id, SplitResizeMetrics { content_span: span });
                        }
                    });
                })
                .id(gpui::SharedString::from(format!(
                    "session-popup-split-{}",
                    split_id.0
                )))
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(side(first, ratio, "first"))
                .child(self.render_session_popup_split_handle(id, split_id, axis, cx))
                .child(side(second, 1.0 - ratio, "second"))
                .into_any_element()
            }
        }
    }

    /// The divider between two panes: the work area's resize rail. Dragging it resizes the panes;
    /// a double-click makes them equal.
    fn render_session_popup_split_handle(
        &self,
        id: SessionPopupId,
        split_id: WorkspaceSplitId,
        axis: WorkspaceSplitAxis,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dragging = self
            .session_popups
            .get(id)
            .and_then(|popup| popup.split_drag)
            .is_some_and(|drag| drag.split_id == split_id);
        let side = ResizeRailGrabSide::Straddle;
        let rail = div()
            .id(gpui::SharedString::from(format!(
                "session-popup-split-handle-{}",
                split_id.0
            )))
            .relative()
            .flex_shrink_0()
            .bg(workspace_split_handle_color());
        let rail = match axis {
            WorkspaceSplitAxis::Horizontal => rail.h_full().w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS)),
            WorkspaceSplitAxis::Vertical => rail.w_full().h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS)),
        };
        rail.child(resize_rail_deferred_strip(
            resize_rail_grab_strip(
                gpui::SharedString::from(format!("session-popup-split-grab-strip-{}", split_id.0)),
                axis,
                side,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    app.session_popup_split_drag_start(id, split_id, axis, event, cx);
                }),
            )
            .when(dragging, |strip| {
                strip.child(resize_rail_hover_line(
                    gpui::SharedString::from(format!(
                        "session-popup-split-hover-line-{}",
                        split_id.0
                    )),
                    axis,
                    side,
                ))
            }),
        ))
        .into_any_element()
    }

    fn session_popup_split_drag_start(
        &mut self,
        id: SessionPopupId,
        split_id: WorkspaceSplitId,
        axis: WorkspaceSplitAxis,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        popup.split_drag = None;
        if event.click_count >= 2 {
            popup
                .layout
                .equalize_split_panes(split_id, &popup.split_metrics);
        } else if let (Some(start_ratio), Some(metrics)) = (
            popup.layout.split_ratio(split_id),
            popup.split_metrics.get(&split_id).copied(),
        ) {
            popup.split_drag = Some(WorkspaceSplitResizeDragState {
                split_id,
                axis,
                start_position: split_resize_event_position(axis, event.position),
                start_ratio,
                content_span: metrics.content_span.max(1.0),
            });
        }
        popup.revision += 1;
        cx.notify();
    }

    /// The pointer moved in popup `id`'s window while a divider is held: the panes follow it,
    /// within the work area's minimum pane sizes.
    pub(crate) fn session_popup_split_drag_move(
        &mut self,
        id: SessionPopupId,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        let Some(drag) = popup.split_drag else {
            return;
        };
        if !event.dragging() {
            popup.split_drag = None;
            popup.revision += 1;
            cx.notify();
            return;
        }
        let raw = drag.start_ratio
            + (split_resize_event_position(drag.axis, event.position) - drag.start_position)
                / drag.content_span.max(1.0);
        let Some((min, max)) = popup
            .layout
            .split_drag_ratio_bounds(drag.split_id, drag.content_span)
        else {
            return;
        };
        if popup
            .layout
            .set_split_ratio(drag.split_id, raw.clamp(min, max))
        {
            popup.revision += 1;
            cx.notify();
        }
    }

    /// The mouse went up in popup `id`: a held divider is let go.
    pub(crate) fn session_popup_split_drag_end(
        &mut self,
        id: SessionPopupId,
        cx: &mut Context<Self>,
    ) {
        if let Some(popup) = self.session_popups.get_mut(id)
            && popup.split_drag.take().is_some()
        {
            popup.revision += 1;
            cx.notify();
        }
    }

    /// Wraps a pane's session so a dragged tab can split the pane at its edges or take its place
    /// in the middle, with the work area's drop feedback.
    pub(crate) fn session_popup_pane_drop_target(
        &self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let zone = self
            .session_popups
            .get(id)
            .and_then(|popup| popup.drop)
            .and_then(|feedback| match feedback {
                WorkspaceDropFeedback {
                    pane_id,
                    target: WorkspaceDropTarget::PaneBody(zone),
                } if pane_id == pane => Some(zone),
                _ => None,
            });
        div()
            .id(gpui::SharedString::from(format!(
                "session-popup-pane-body-{}",
                pane.0
            )))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .relative()
            .on_drag_move::<SessionPopupTabDrag>(cx.listener(
                move |app, event: &gpui::DragMoveEvent<SessionPopupTabDrag>, _, cx| {
                    let zone = event
                        .bounds
                        .contains(&event.event.position)
                        .then(|| workspace_pane_body_drop_zone(event.bounds, event.event.position));
                    app.session_popup_set_pane_drop(id, pane, zone, cx);
                },
            ))
            .on_drop(
                cx.listener(move |app, dragged: &SessionPopupTabDrag, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    app.session_popup_pane_drop(id, pane, dragged, cx);
                }),
            )
            .child(body)
            .when_some(zone, |target, zone| {
                target.child(self.render_workspace_pane_drop_feedback(pane, zone))
            })
            .into_any_element()
    }

    /// A dragged tab is over pane `pane` of popup `id` at `zone`, or left it.
    fn session_popup_set_pane_drop(
        &mut self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        zone: Option<WorkspaceDropZone>,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        let next = match zone {
            Some(zone) => Some(WorkspaceDropFeedback {
                pane_id: pane,
                target: WorkspaceDropTarget::PaneBody(zone),
            }),
            // Only the pane that set it clears it.
            None if popup.drop.is_some_and(|drop| drop.pane_id == pane) => None,
            None => return,
        };
        if popup.drop != next || (next.is_some() && popup.strip_drop.is_some()) {
            popup.drop = next;
            if next.is_some() {
                popup.strip_drop = None;
            }
            popup.revision += 1;
            cx.notify();
        }
    }

    /// A tab dropped on pane `pane` of popup `id`: at an edge it splits the pane there, in the
    /// middle it joins the pane as its selected tab (`WorkspaceModel::split_tab_to_pane`). A tab
    /// from another popup comes over first.
    fn session_popup_pane_drop(
        &mut self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        dragged: &SessionPopupTabDrag,
        cx: &mut Context<Self>,
    ) {
        self.session_popups.drag = None;
        let Some(zone) = self
            .session_popups
            .get(id)
            .and_then(|popup| popup.drop)
            .and_then(|feedback| match feedback.target {
                WorkspaceDropTarget::PaneBody(zone) if feedback.pane_id == pane => Some(zone),
                _ => None,
            })
        else {
            return;
        };
        if dragged.popup != id {
            self.session_popup_move_tab(
                dragged.key.clone(),
                super::tabs::SessionPopupTarget::Popup(id),
                cx,
            );
        }
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        popup.drop = None;
        popup.strip_drop = None;
        popup.revision += 1;
        if let Some(tab) = popup.tab_for(&dragged.key)
            && let Some(source) = popup.layout.pane_id_for_session(tab)
        {
            popup.layout.split_tab_to_pane(source, pane, tab, zone);
        }
        self.session_popup_save_open_list();
        self.session_popup_settle(cx);
    }

    /// The name of the project `key` belongs to, for a split pane's bar.
    pub(crate) fn session_popup_project_title(&self, key: &ghostex_gx_core::SessionKey) -> String {
        self.gx_store
            .core
            .presentation()
            .loaded(&key.machine)
            .and_then(|loaded| loaded.project(&key.project_id))
            .map(|project| project.title.trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| key.project_id.clone())
    }

    /// A pane was clicked: it becomes the popup's focused pane, the one its header acts on.
    pub(crate) fn session_popup_focus_pane(
        &mut self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        cx: &mut Context<Self>,
    ) {
        if let Some(popup) = self.session_popups.get_mut(id)
            && popup.layout.focused_pane != pane
            && popup.layout.find_leaf(pane).is_some()
        {
            popup.layout.set_focused_pane(pane);
            popup.revision += 1;
            cx.notify();
        }
    }
}
