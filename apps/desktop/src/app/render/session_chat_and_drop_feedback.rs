// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: session-chat/session-find surface content, the session-chat body frame, workspace pane drop-zone/feedback, and drop-edge band rendering.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
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
use crate::app::native_sidebar::drag::SidebarDrag;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:SessionChat 2026-09-21 WHY:
    /// The window root redraws for every sidebar scroll tick, store update and header change, and an uncached child view is rendered and laid out again on each of those draws: the visible transcript rows' markdown was measured again on frames where the chat had not changed, which is what made mouse wheel scrolling stall.
    /// A cached view is reused until it is notified or its bounds change, so anything the chat reads from outside itself has to notify it; shared settings and the system appearance do so through `notify_native_chat_views`.
    fn cached_native_chat(
        view: gpui::Entity<crate::app::native_chat::state::NativeChatView>,
    ) -> impl IntoElement {
        gpui::AnyView::from(view).cached(gpui::StyleRefinement::default().size_full())
    }

    pub(crate) fn notify_native_chat_views(&mut self, cx: &mut gpui::Context<Self>) {
        for view in self
            .native_chat_views
            .values()
            .chain(self.side_panel_chat_views())
        {
            view.update(cx, |_, cx| cx.notify());
        }
        // The Kanban board, the Automate page and the Bot automations feed are cached views drawn from the same settings.
        self.native_kanban_notify_appearance(cx);
        self.native_automate_notify_appearance(cx);
        self.native_bot_feed_notify_appearance(cx);
    }

    pub(crate) fn render_agents_session_chat_body(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:SessionChat 2026-07-31:
        Chat owns the same normal-layout workspace body rectangle as a
        terminal: a per-session GPUI child plus ordinary placeholder layout children.
        The native chat remains inside its workspace pane's layout frame.
        */
        // A Claude cloud session's pane is its claude.ai page once it has one (claude_cloud_chat_page.rs).
        if let Some(page) = self.render_claude_cloud_chat_page(session_id, cx) {
            return self.render_agents_session_chat_body_frame(pane_id, session_id, page, cx);
        }
        let content = match self
            .native_chat_views
            .get(&session_id)
            .filter(|_| {
                self.session_account_switch_placeholder_progress(session_id)
                    .is_none()
            })
            .cloned()
        {
            Some(view) => {
                self.record_session_chat_render(session_id);
                // A view passed during a held "next tab" is drawn as it is; its runtime resumes with the chat reconcile that runs when the selection settles.
                if !self.native_chat_visible_sessions.contains(&session_id)
                    && !self.gx_store_selection_is_settling()
                {
                    let app = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.note_native_chat_pane_painted(session_id, cx)
                        });
                    });
                }
                div()
                    .id(format!("native-chat-{}", session_id.0))
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(Self::cached_native_chat(view))
                    .into_any_element()
            }
            None => self.render_session_chat_surface_content(session_id),
        };
        self.render_agents_session_chat_body_frame(pane_id, session_id, content, cx)
    }

    /// The chat surface (or its loading/unavailable placeholder) for one
    /// session — shared by the Agents workspace body and the project-editor
    /// companion slot body.
    pub(crate) fn render_session_chat_surface_content(
        &self,
        session_id: TerminalSessionId,
    ) -> AnyElement {
        let switching = self.session_account_switch_placeholder_progress(session_id);
        self.record_session_chat_render(session_id);
        if let Some(view) = self
            .native_chat_views
            .get(&session_id)
            .filter(|_| switching.is_none())
        {
            return div()
                .id(format!("native-chat-{}", session_id.0))
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(Self::cached_native_chat(view.clone()))
                .into_any_element();
        }
        {
            let bootstrap_missing = self.sidebar_gxserver_bootstrap.is_none();
            let (title, message) = if let Some(progress) = switching {
                (progress.title.as_str(), progress.email.as_str())
            } else if bootstrap_missing {
                (
                    "Chat unavailable",
                    "Session Chat needs the local Ghostex server. Start it from the sidebar, then toggle Chat View again.",
                )
            } else {
                // A chat-mode tab whose native view does not exist yet: the session is still being
                // created or mapped, so the pane shows the transcript skeleton, not a sentence.
                return self.render_session_chat_skeleton();
            };
            let hide_emails = shared_settings::shared_sidebar_settings_snapshot()
                .object()
                .get("hideAccountEmails")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            let message = if switching.is_some() && hide_emails {
                match message.split_once('@') {
                    Some((address, _)) => {
                        let chars: Vec<_> = address.chars().collect();
                        format!(
                            "{}•••{}@•••••.•••",
                            chars.first().copied().unwrap_or('•'),
                            if chars.len() > 1 {
                                chars.last().unwrap().to_string()
                            } else {
                                String::new()
                            }
                        )
                    }
                    None => message.to_string(),
                }
            } else {
                message.to_string()
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
                .bg(glass_clear(gpui_session_chat_background_color()))
                .child(
                    v_flex()
                        .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                        .items_center()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(workspace_terminal_placeholder_title_color())
                                .child(title.to_string()),
                        )
                        .when(!message.is_empty(), |this| {
                            this.child(
                                div()
                                    .mt(px(5.0))
                                    .max_w(px(390.0))
                                    .text_size(px(12.5))
                                    .line_height(px(18.0))
                                    .text_color(workspace_terminal_placeholder_message_color())
                                    .flex().items_center().gap(px(8.0))
                                    .when_some(switching, |this, progress| {
                                        let labelled = !progress.indicator.is_empty() && progress.indicator != "-";
                                        let size = if labelled { 19.2 } else { 18.0 };
                                        this.child(
                                            div().relative().size(px(size)).flex().items_center().justify_center().flex_shrink_0()
                                                .child(gpui::svg().path(workspace_tab_agent_icon_path(progress.provider).unwrap())
                                                    .absolute().top_0().left_0().size(px(size))
                                                    .text_color(gpui::rgb(if progress.provider == "claude" { 0xd97757 } else { 0xffffff }))
                                                    .when(labelled, |this| this.opacity(0.3)))
                                                .when(labelled, |this| {
                                                    this.child(div().relative()
                                                        .text_color(if progress.provider == "codex" { gpui::rgb(0x7db8fb).into() } else { workspace_terminal_placeholder_message_color() })
                                                        .text_size(px(9.9)).line_height(px(9.9)).font_family(ACCOUNT_INDICATOR_FONT_FAMILY)
                                                        .font_weight(gpui::FontWeight::SEMIBOLD).child(progress.indicator.clone()))
                                                }),
                                        )
                                    })
                                    .child(message.clone()),
                            )
                        })
                        .when(!bootstrap_missing, |this| {
                            // The spinner reads the clock itself; the throttled animation
                            // redraws it at the shared indicator rate instead of asking for every
                            // display frame from its own paint.
                            this.child(
                                canvas(
                                    move |_bounds, _window, _cx| {},
                                    move |bounds, _state: (), window, _cx| {
                                        paint_agent_gui_loading_spinner(bounds, window);
                                    },
                                )
                                .size(px(18.0))
                                .mt(px(10.0))
                                .with_throttled_animation(
                                    format!(
                                        "ghostex-gpui-session-chat-placeholder-spinner-{}",
                                        session_id.0
                                    ),
                                    std::time::Duration::from_secs(1),
                                    |spinner, _progress| spinner,
                                ),
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
            .bg(glass_clear(gpui_session_chat_background_color()))
            /*
            CDXC:FocusRouting 2026-09-17 WHY:
            The native chat composer stops mouse-down propagation, so a bubble-phase listener here never sees a click on the composer itself.
            Capture the click like the composited terminal body does: claim the pane and hand the keyboard off right away, before the composer or an answer field takes its own GPUI focus from the same click.
            */
            .capture_any_mouse_down(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    this.focus_agents_pane(pane_id, cx);
                    this.drain_pending_keyboard_handoff(window, cx);
                }),
            )
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
            .on_drag_move::<SidebarDrag>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<SidebarDrag>, window, cx| {
                    this.update_sidebar_session_pane_drag_feedback(event, pane_id, window, cx);
                },
            ))
            .can_drop(|value, _window, _cx| {
                value.is::<DraggedWorkspaceTab>()
                    || value.is::<DraggedCommandTab>()
                    || value.is::<SidebarDrag>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &SidebarDrag, window, cx| {
                this.handle_sidebar_session_pane_body_drop(pane_id, dragged, window, cx);
            }))
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_tab_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .child(content)
            .when_some(self.workspace_pane_drop_zone(pane_id), |this, zone| {
                this.child(self.render_workspace_pane_drop_feedback(pane_id, zone))
            })
            .when_some(self.workspace_pane_drop_refusal(pane_id), |this, reason| {
                this.child(self.render_workspace_pane_drop_refusal(pane_id, reason))
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

    /// Why the pane under a refused drag cannot take it (`session_pane_placement.rs`).
    pub(crate) fn workspace_pane_drop_refusal(
        &self,
        pane_id: WorkspacePaneId,
    ) -> Option<&'static str> {
        match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id: feedback_pane_id,
                target: WorkspaceDropTarget::Refused(reason),
            }) if feedback_pane_id == pane_id => Some(reason),
            _ => None,
        }
    }

    /// A refused pane dims and says why in the middle, drawn as a non-interactive child like the
    /// split zones it stands in for.
    pub(crate) fn render_workspace_pane_drop_refusal(
        &self,
        pane_id: WorkspacePaneId,
        reason: &'static str,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-workspace-pane-drop-refusal-{}",
                pane_id.0
            ))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.45))
            .child(
                div()
                    .flex()
                    .h(px(28.0))
                    .items_center()
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(gpui::white().opacity(0.14))
                    .bg(gpui::rgb(0x1d1d1d).opacity(0.94))
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(gpui::rgb(0xe5e5e5))
                    .child(reason),
            )
            .into_any_element()
    }

    pub(crate) fn render_workspace_pane_drop_feedback(
        &self,
        pane_id: WorkspacePaneId,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        /*
        CDXC:Workarea 2026-06-22-05:31:
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
