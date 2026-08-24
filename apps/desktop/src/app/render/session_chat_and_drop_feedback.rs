// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: session-chat/session-find surface content, the session-chat body frame, workspace pane drop-zone/feedback, and drop-edge band rendering.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::canvas;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_agents_session_chat_body(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUISessionChatSurface 2026-07-31:
        Chat owns the same normal-layout workspace body rectangle as a
        terminal: a per-session CefSurface child
        plus ordinary placeholder layout children. No terminal mount canvas,
        native geometry probe, overlay, or hidden hit region participates.
        */
        let content = self.render_session_chat_surface_content(session_id);
        self.render_agents_session_chat_body_frame(pane_id, session_id, content, cx)
    }

    /// The chat surface (or its loading/unavailable placeholder) for one
    /// session — shared by the Agents workspace body and the project-editor
    /// companion slot body.
    pub(crate) fn render_session_chat_surface_content(
        &self,
        session_id: TerminalSessionId,
    ) -> AnyElement {
        let surface = self.agents_chat_surfaces.get(&session_id).cloned();
        if let Some(surface) = surface {
            div()
                .id(format!("ghostex-gpui-session-chat-cef-{}", session_id.0))
                .relative()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(surface)
                .into_any_element()
        } else {
            let bootstrap_missing = self.sidebar_gxserver_bootstrap.is_none();
            let (title, message) = if bootstrap_missing {
                (
                    "Chat unavailable",
                    "Session Chat needs the local Ghostex server. Start it from the sidebar, then toggle Chat View again.",
                )
            } else {
                ("Loading Chat...", "")
            };
            v_flex()
                .id(format!(
                    "ghostex-gpui-session-chat-placeholder-{}",
                    session_id.0
                ))
                .size_full()
                .min_w_0()
                .min_h_0()
                .items_center()
                .justify_center()
                .bg(gpui_session_chat_background_color())
                .child(
                    v_flex()
                        .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                        .items_center()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(workspace_terminal_placeholder_title_color())
                                .child(title),
                        )
                        .when(!bootstrap_missing, |this| {
                            this.child(
                                canvas(
                                    move |_bounds, _window, _cx| {},
                                    move |bounds, _state: (), window, _cx| {
                                        window.request_animation_frame();
                                        paint_agent_gui_loading_spinner(bounds, window);
                                    },
                                )
                                .size(px(18.0))
                                .mt(px(10.0)),
                            )
                        })
                        .when(bootstrap_missing, |this| {
                            this.child(
                                div()
                                    .mt(px(5.0))
                                    .max_w(px(390.0))
                                    .text_size(px(12.5))
                                    .line_height(px(18.0))
                                    .text_color(workspace_terminal_placeholder_message_color())
                                    .child(message),
                            )
                        }),
                )
                .into_any_element()
        }
    }

    pub(crate) fn render_agents_session_chat_body_frame(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        content: AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-session-chat-body-{}-{}",
                pane_id.0, session_id.0
            ))
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(gpui_session_chat_background_color())
            .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                    this.update_workspace_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .on_drag_move::<DraggedCommandTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                    this.update_command_tab_over_workspace_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .can_drop(|value, _window, _cx| {
                value.is::<DraggedWorkspaceTab>() || value.is::<DraggedCommandTab>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_tab_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .child(content)
            .when_some(self.workspace_pane_drop_zone(pane_id), |this, zone| {
                this.child(self.render_workspace_pane_drop_feedback(pane_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn workspace_pane_drop_zone(
        &self,
        pane_id: WorkspacePaneId,
    ) -> Option<WorkspaceDropZone> {
        match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id: feedback_pane_id,
                target: WorkspaceDropTarget::PaneBody(zone),
            }) if feedback_pane_id == pane_id => Some(zone),
            _ => None,
        }
    }

    pub(crate) fn render_workspace_pane_drop_feedback(
        &self,
        pane_id: WorkspacePaneId,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        /*
        CDXC:GPUIWorkspaceDragDrop 2026-06-22-05:31:
        Drag feedback for Agents pane-body drops must be visible but non-interactive. Render the center group or edge split indication as a normal child inside the pane body instead of adding transparent overlap, root hit-test shields, or window-level mouse routing.
        */
        let feedback = div()
            .id(format!(
                "ghostex-gpui-workspace-pane-drop-feedback-{}",
                pane_id.0
            ))
            .absolute()
            .top_0()
            .left_0()
            .size_full();

        match zone {
            WorkspaceDropZone::Center => feedback
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_color(agents_drop_feedback_border_color())
                .bg(agents_drop_group_feedback_color())
                .into_any_element(),
            WorkspaceDropZone::Left => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .left_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Right => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .right_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Top => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .top_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Bottom => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .bottom_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
        }
    }

    pub(crate) fn render_agents_workspace_drop_edge_band(
        &self,
        zone: WorkspaceDropZone,
    ) -> gpui::Div {
        let band = div()
            .absolute()
            .border_2()
            .border_color(agents_drop_feedback_border_color())
            .bg(agents_drop_split_feedback_color());

        match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => band
                .w(relative(AGENTS_SPLIT_DROP_PREVIEW_FRACTION))
                .h_full(),
            WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => band
                .h(relative(AGENTS_SPLIT_DROP_PREVIEW_FRACTION))
                .w_full(),
            WorkspaceDropZone::Center => band.size_full(),
        }
    }

    pub(crate) fn render_workspace_drop_edge_band(
        &self,
        label: &'static str,
        zone: WorkspaceDropZone,
    ) -> gpui::Div {
        let band = div()
            .absolute()
            .flex()
            .items_center()
            .justify_center()
            .border_2()
            .border_color(workspace_drop_feedback_border_color())
            .bg(workspace_drop_split_feedback_color())
            .child(self.render_workspace_drop_feedback_label(label, zone));

        match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => {
                band.w(relative(WORKSPACE_DROP_EDGE_BAND_FRACTION)).h_full()
            }
            WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => {
                band.h(relative(WORKSPACE_DROP_EDGE_BAND_FRACTION)).w_full()
            }
            WorkspaceDropZone::Center => band.size_full(),
        }
    }

    pub(crate) fn render_workspace_drop_feedback_label(
        &self,
        label: &'static str,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        div()
            .flex()
            .h(px(24.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(workspace_drop_feedback_border_color())
            .bg(workspace_drop_feedback_label_color(zone))
            .px(px(9.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(workspace_drop_feedback_text_color())
            .child(label)
            .into_any_element()
    }
}
