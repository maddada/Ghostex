// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: project-editor companion pane rendering: companion terminal body/slot, split divider/button, collapse button, restore rail, and divider.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::Hsla;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::ParentElement as _;
use gpui::ScrollWheelEvent;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::canvas;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui::rgb;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::tooltip::Tooltip;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::resize_rail::*;
use crate::*;

use super::terminal_content_layout::terminal_content_frame;

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-19 WHY:
    /// A side-by-side pair lives inside the companion's own layout region rather than as extra children of
    /// the editor-shell row: that row's resize metrics read its first and third child as companion and main,
    /// so widening it would have made the outer divider resize one sidepane against the other.
    pub(crate) fn render_project_editor_companion_region(
        &self,
        mode: TitlebarMode,
        flex_grow: f32,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if !self.project_editor_companion_columns_split_active() {
            return self.render_project_editor_companion_pane(mode, flex_grow, None, window, cx);
        }

        let columns_ratio = self
            .project_editor_shell
            .left_companion_columns_ratio
            .clamp(0.1, 0.9);
        let view = cx.entity().clone();
        h_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_project_editor_companion_split_layout_metrics(
                        WorkspaceSplitAxis::Horizontal,
                        &child_bounds,
                    );
                    // The pair navigates and focuses as one companion, so the
                    // region's own span is the companion's focus bounds.
                    this.record_project_editor_companion_layout_bounds(mode, &child_bounds);
                });
            })
            .id(format!(
                "ghostex-gpui-project-editor-companion-columns-{}",
                mode.element_slug()
            ))
            .flex_grow(flex_grow)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .min_w(px(PROJECT_EDITOR_COMPANION_MIN_WIDTH))
            .h_full()
            .min_h_0()
            .items_start()
            .overflow_hidden()
            .child(self.render_project_editor_companion_pane(
                mode,
                columns_ratio,
                Some(ProjectEditorCompanionTerminalSlot::Top),
                window,
                cx,
            ))
            .child(self.render_project_editor_companion_split_divider(
                mode,
                WorkspaceSplitAxis::Horizontal,
                false,
                cx,
            ))
            .child(self.render_project_editor_companion_pane(
                mode,
                1.0 - columns_ratio,
                Some(ProjectEditorCompanionTerminalSlot::Bottom),
                window,
                cx,
            ))
            .into_any_element()
    }

    /// CDXC:Workarea 2026-09-09 DECISION:
    /// User: remove the hide button from the companion pane header; the app titlebar owns its visibility toggle.
    /// CDXC:Workarea 2026-09-09 WHY:
    /// The floating window already applies the saved width ratio, so its lone pane must grow to fill the host (1.0).
    /// Reapplying the docked ratio inside that window narrowed the content and left a large black strip beside it.
    /// CDXC:Workarea 2026-09-14 DECISION:
    /// User: reduce the companion sidepane title area height by 1px.
    ///
    /// `column` is `Some` for one of a side-by-side pair, and `None` for the
    /// lone sidepane that hosts the stacked arrangement itself.
    pub(crate) fn render_project_editor_companion_pane(
        &self,
        mode: TitlebarMode,
        flex_grow: f32,
        column: Option<ProjectEditorCompanionTerminalSlot>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let stacked_split = column.is_none()
            && self
                .project_editor_companion_secondary_terminal_session_id
                .is_some();
        let companion_border_state = self.project_editor_companion_border_state(mode, window);
        // A lone sidepane and the first of a pair start at the sidebar divider; every sidepane ends at
        // a rail, either the pair's own divider or the one before the main surface.
        let outer_rail_edges = self.main_workspace_outer_rail_edges(window);
        let rail_edges = RailFacingEdges {
            left: match column {
                None | Some(ProjectEditorCompanionTerminalSlot::Top) => outer_rail_edges.left,
                Some(_) => true,
            },
            right: true,
            top: false,
            bottom: outer_rail_edges.bottom,
        };
        let border_state = match column {
            // Side by side, each sidepane owns a real frame, so the focused one
            // carries the outline itself.
            Some(slot) => {
                if companion_border_state == WorkspacePaneBorderState::Focused
                    && self.project_editor_companion_focused_terminal_slot == slot
                {
                    WorkspacePaneBorderState::Focused
                } else {
                    WorkspacePaneBorderState::Neutral
                }
            }
            // Stacked, the slot bodies inside draw the focus outline.
            None if stacked_split => WorkspacePaneBorderState::Neutral,
            None => companion_border_state,
        };
        let companion_title = match column {
            Some(slot) => self.project_editor_companion_slot_title(slot),
            None => self.project_editor_companion_active_title(mode),
        };
        let pane_slug = match column {
            Some(ProjectEditorCompanionTerminalSlot::Top) => {
                format!("{}-left", mode.element_slug())
            }
            Some(ProjectEditorCompanionTerminalSlot::Bottom) => {
                format!("{}-right", mode.element_slug())
            }
            None => mode.element_slug(),
        };
        let record_layout_bounds = column.is_none();
        let view = cx.entity().clone();
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                if !record_layout_bounds {
                    return;
                }
                let _ = view.update(cx, |this, _cx| {
                    this.record_project_editor_companion_layout_bounds(mode, &child_bounds);
                });
            })
            .id(format!(
                "ghostex-gpui-project-editor-companion-pane-{}",
                pane_slug
            ))
            .flex_grow(flex_grow)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            // Both dividers' resize clamps keep the sidepane minimums; the
            // sidepanes themselves stay shrinkable so a window too narrow for two
            // of them keeps showing all of each instead of clipping the second.
            .when(column.is_none(), |pane| {
                pane.min_w(px(PROJECT_EDITOR_COMPANION_MIN_WIDTH))
            })
            .when(column.is_some(), |pane| pane.min_w_0())
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .map(|pane| {
                rail_aware_pane_border(
                    pane,
                    rail_edges,
                    project_editor_companion_border_color_for_state(border_state),
                    project_editor_companion_border_color_for_state(
                        WorkspacePaneBorderState::Neutral,
                    ),
                )
            })
            .bg(workspace_terminal_placeholder_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_companion(mode, window, cx);
                    cx.notify();
                }),
            )
            .child(
                h_flex()
                    .id(format!(
                        "ghostex-gpui-project-editor-companion-tabbar-{}",
                        pane_slug
                    ))
                    .flex_shrink_0()
                    .h(px(WORKSPACE_TAB_BAR_HEIGHT - 1.0))
                    .w_full()
                    .items_center()
                    .border_b_1()
                    .border_color(workspace_tab_border_color())
                    .bg(workspace_tab_bar_color())
                    .child(
                        h_flex()
                            .h_full()
                            .w_full()
                            .items_center()
                            .overflow_hidden()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_tab_active_text_color())
                            .child(
                                div()
                                    .mx(px(8.0))
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(companion_title),
                            )
                            .children(self.render_project_editor_companion_pane_split_controls(
                                mode, column, cx,
                            )),
                    ),
            )
            .child(match column {
                Some(slot) => self.render_project_editor_companion_terminal_slot_body(
                    mode,
                    slot,
                    self.project_editor_companion_terminal_session_for_slot(slot),
                    1.0,
                    false,
                    false,
                    cx,
                ),
                None => self.render_project_editor_companion_terminal_body(mode, window, cx),
            })
            .window_corner_pane()
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_terminal_body(
        &self,
        mode: TitlebarMode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let top_session_id = self.project_editor_companion_terminal_session_id;
        // Only the stacked arrangement draws a second slot here, so a state whose
        // stored axis and slot occupancy disagree cannot render a stacked divider
        // that then drives the side-by-side ratio.
        let stacked_session_id = self
            .project_editor_companion_secondary_terminal_session_id
            .filter(|_| {
                self.project_editor_shell.left_companion_split_enabled
                    && self.project_editor_shell.left_companion_split_axis
                        == WorkspaceSplitAxis::Vertical
            });
        let focused_slot = (stacked_session_id.is_some()
            && self.project_editor_companion_border_state(mode, window)
                == WorkspacePaneBorderState::Focused)
            .then_some(self.project_editor_companion_focused_terminal_slot);
        let split_ratio = self
            .project_editor_shell
            .left_companion_split_ratio
            .clamp(0.1, 0.9);
        let has_stacked_slot = stacked_session_id.is_some();
        let view = cx.entity().clone();
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                if has_stacked_slot {
                    let _ = view.update(cx, |this, _cx| {
                        this.record_project_editor_companion_split_layout_metrics(
                            WorkspaceSplitAxis::Vertical,
                            &child_bounds,
                        );
                    });
                }
            })
            .id(format!(
                "ghostex-gpui-project-editor-companion-terminal-stack-{}",
                mode.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(self.render_project_editor_companion_terminal_slot_body(
                mode,
                ProjectEditorCompanionTerminalSlot::Top,
                top_session_id,
                if has_stacked_slot { split_ratio } else { 1.0 },
                has_stacked_slot,
                focused_slot == Some(ProjectEditorCompanionTerminalSlot::Top),
                cx,
            ))
            .when_some(stacked_session_id, |this, session_id| {
                this.child(self.render_project_editor_companion_split_divider(
                    mode,
                    WorkspaceSplitAxis::Vertical,
                    focused_slot.is_none(),
                    cx,
                ))
                .child(self.render_project_editor_companion_terminal_slot_body(
                    mode,
                    ProjectEditorCompanionTerminalSlot::Bottom,
                    Some(session_id),
                    1.0 - split_ratio,
                    true,
                    focused_slot == Some(ProjectEditorCompanionTerminalSlot::Bottom),
                    cx,
                ))
            })
            .into_any_element()
    }

    /// CDXC:Workarea 2026-09-15 DECISION:
    /// User: remove the "No running terminal" message that flashes in panes during app startup.
    /// A missing mount slot is also a normal restoration state, so leave its background blank until terminal content attaches.
    ///
    /// `stacked_split` is true only while this slot shares one sidepane with its
    /// sibling: a side-by-side pair draws its focus outline on the sidepane frame
    /// instead, so the inner slot border would double it.
    pub(crate) fn render_project_editor_companion_terminal_slot_body(
        &self,
        mode: TitlebarMode,
        slot: ProjectEditorCompanionTerminalSlot,
        session_id: Option<TerminalSessionId>,
        flex_grow: f32,
        stacked_split: bool,
        show_focus_outline: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:SessionChat 2026-08-02:
        Chat mode swaps this companion slot's terminal body for the same
        per-session chat surface the Agents workspace shows in the same slot;
        the terminal mount parks exactly like an Agents tab in
        chat mode. The way back is the chat page's in-DOM cluster.
        */
        if let Some(session_id) = session_id {
            let pane_surface_content = if self.agents_chat_mode_sessions.contains(&session_id) {
                Some(self.render_session_chat_surface_content(session_id))
            } else {
                None
            };
            if let Some(content) = pane_surface_content {
                return self.render_project_editor_companion_pane_surface_body(
                    mode,
                    slot,
                    session_id,
                    flex_grow,
                    stacked_split,
                    show_focus_outline,
                    content,
                    cx,
                );
            }
        }
        let slot_id = session_id
            .map(|session_id| ProjectEditorCompanionTerminalBodyMountSlotId { mode, session_id })
            .filter(|slot_id| {
                self.is_current_project_editor_companion_terminal_body_mount_slot(*slot_id)
            });
        let gpui_engine_view = slot_id
            .and_then(|slot_id| self.agents_gpui_engine_terminals.get(&slot_id.session_id))
            .map(|record| record.view.clone());
        let gpui_engine_owns_pointer_input = gpui_engine_view.is_some();
        let gpui_engine_slot_id = slot_id.filter(|_| gpui_engine_owns_pointer_input);
        let remote_attach_unavailable_message = slot_id.and_then(|slot_id| {
            self.project_editor_companion_remote_attach_unavailable_message(slot_id)
        });
        let native_slot_id = slot_id
            .filter(|_| gpui_engine_view.is_none() && remote_attach_unavailable_message.is_none());
        let slot_slug = match slot {
            ProjectEditorCompanionTerminalSlot::Top => "top",
            ProjectEditorCompanionTerminalSlot::Bottom => "bottom",
        };
        let body_id = match slot_id {
            Some(slot_id) => format!(
                "ghostex-gpui-project-editor-companion-terminal-body-{}-{}-{}",
                mode.element_slug(),
                slot_slug,
                slot_id.session_id.0
            ),
            None => format!(
                "ghostex-gpui-project-editor-companion-empty-body-{}-{}",
                mode.element_slug(),
                slot_slug
            ),
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let (terminal_horizontal_padding, terminal_vertical_padding, terminal_width_percent) =
            settings_snapshot.terminal_pane_layout(false);
        let persistence_label = settings_snapshot
            .show_session_id_in_terminal_panes()
            .then(|| {
                session_id.and_then(|session_id| {
                    self.agents_workspace
                        .session(session_id)
                        .and_then(|session| session.zmx_session_name.as_deref())
                        .map(|name| format!("zmx - {name}"))
                })
            })
            .flatten();
        let search_bar = slot_id.and_then(|slot_id| {
            self.render_project_editor_companion_terminal_search_bar(slot_id, cx)
        });
        let terminal_body = div()
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .bg(workspace_terminal_placeholder_color())
            .when_some(gpui_engine_slot_id, |this, slot_id| {
                this.capture_any_mouse_down(cx.listener(
                    move |this, _event: &MouseDownEvent, window, cx| {
                        support_logs::append(
                            support_logs::GpuiSupportLog::TerminalFocus,
                            "gpui.terminalEngine.pointerFocusCapture",
                            serde_json::json!({
                                "surface": "projectEditorCompanion",
                                "mode": format!("{:?}", mode),
                                "session": slot_id.session_id.0,
                                "activeMode": format!("{:?}", this.active_mode),
                                "shellFocusBefore": format!("{:?}", this.shell_focus),
                                "firstResponderBefore": format!("{:?}", this.first_responder_target),
                            }),
                        );
                        this.focus_project_editor_companion_terminal_session(
                            mode,
                            slot_id.session_id,
                            window,
                            cx,
                        );
                        this.refresh_zmx_persistence_companion_terminal_if_stale(mode, cx);
                        cx.notify();
                    },
                ))
            })
            .when(!gpui_engine_owns_pointer_input, |this| {
                this.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if let Some(session_id) = session_id {
                            this.focus_project_editor_companion_terminal_session(
                                mode, session_id, window, cx,
                            );
                        } else {
                            this.focus_project_editor_companion(mode, window, cx);
                        }
                        this.refresh_zmx_persistence_companion_terminal_if_stale(mode, cx);
                        cx.notify();
                    }),
                )
            })
            .when_some(remote_attach_unavailable_message, |this, message| {
                this.child(
                    v_flex()
                        .absolute()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .px(px(24.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(workspace_terminal_placeholder_title_color())
                                .child("Remote terminal unavailable"),
                        )
                        .child(
                            div()
                                .mt(px(5.0))
                                .max_w(px(390.0))
                                .text_size(px(12.5))
                                .line_height(px(18.0))
                                .text_color(workspace_terminal_placeholder_message_color())
                                .child(message),
                        ),
                )
            })
            .when_some(gpui_engine_view, |this, view| {
                this.child(
                    terminal_content_frame(
                        div().size_full().child(view),
                        terminal_horizontal_padding,
                        terminal_vertical_padding,
                        terminal_width_percent,
                    ),
                )
            })
            .when_some(native_slot_id, |this, slot_id| {
                let view = cx.entity().clone();
                this.on_scroll_wheel(cx.listener(
                    move |this, event: &ScrollWheelEvent, window, cx| {
                        if this.forward_project_editor_companion_terminal_mount_slot_mouse_scroll(
                            slot_id,
                            event.position,
                            event.delta,
                            event.modifiers,
                        ) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    },
                ))
                .child({
                    let bounds_view = view.clone();
                    let input_handler_view = view.clone();
                    terminal_content_frame(
                        canvas(
                            move |bounds, window, cx| {
                                let scale_factor = window.scale_factor();
                                let _ = bounds_view.update(cx, |this, cx| {
                                    this.record_project_editor_companion_terminal_mount_slot_bounds(
                                        slot_id,
                                        bounds,
                                        scale_factor,
                                        cx,
                                    );
                                });
                            },
                            move |bounds, _, window, cx| {
                                let input_view = input_handler_view.clone();
                                let _ = input_handler_view.update(cx, |this, cx| {
                                    this.register_project_editor_companion_terminal_text_input_handler(
                                        slot_id, bounds, input_view, window, cx,
                                    );
                                });
                            },
                        )
                        .size_full(),
                        terminal_horizontal_padding,
                        terminal_vertical_padding,
                        terminal_width_percent,
                    )
                })
            })
            .when_some(persistence_label, |this, label| {
                this.child(
                    div()
                        .absolute()
                        .top(px(6.0))
                        .right(px(3.0))
                        .text_size(px(10.0))
                        .text_color(chrome_ink().opacity(0.24))
                        .child(label),
                )
            });
        v_flex()
            .id(body_id)
            .flex_grow(flex_grow)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .when(stacked_split, |this| {
                this.border_1()
                    .border_color(if show_focus_outline && show_active_pane_outline() {
                        workspace_pane_focused_border_color()
                    } else {
                        rgb(0x000000).opacity(0.0).into()
                    })
            })
            .bg(workspace_terminal_placeholder_color())
            .when_some(search_bar, |this, search_bar| this.child(search_bar))
            .child(terminal_body)
            // Companion terminals show Agents sessions and carried the same
            // overlay cluster, so they get the same bare bottom bar, as a
            // normal sibling below the body.
            .when_some(
                self.render_project_editor_companion_terminal_agent_action_bar(
                    mode, session_id, cx,
                ),
                |this, bar| this.child(bar),
            )
            .window_corner_pane()
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_pane_surface_body(
        &self,
        mode: TitlebarMode,
        slot: ProjectEditorCompanionTerminalSlot,
        session_id: TerminalSessionId,
        flex_grow: f32,
        stacked_split: bool,
        show_focus_outline: bool,
        content: AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let slot_slug = match slot {
            ProjectEditorCompanionTerminalSlot::Top => "top",
            ProjectEditorCompanionTerminalSlot::Bottom => "bottom",
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-chat-body-{}-{}-{}",
                mode.element_slug(),
                slot_slug,
                session_id.0
            ))
            .relative()
            .flex_grow(flex_grow)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .when(stacked_split, |this| {
                this.border_1()
                    .border_color(if show_focus_outline && show_active_pane_outline() {
                        workspace_pane_focused_border_color()
                    } else {
                        rgb(0x000000).opacity(0.0).into()
                    })
            })
            .bg(gpui_session_chat_background_color())
            // Capture phase: the native chat composer stops mouse-down propagation, so a bubble listener never sees a click on the composer itself.
            .capture_any_mouse_down(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    this.focus_project_editor_companion_terminal_session(
                        mode, session_id, window, cx,
                    );
                    cx.notify();
                }),
            )
            .child(content)
            .window_corner_pane()
            .into_any_element()
    }

    /// The boundary between the two companion sessions, whichever way they are
    /// arranged: a full-width handle between stacked slots, or a full-height one
    /// between side-by-side sidepanes. Both are real reserved layout regions and
    /// the visible handle is its own resize target.
    pub(crate) fn render_project_editor_companion_split_divider(
        &self,
        mode: TitlebarMode,
        axis: WorkspaceSplitAxis,
        show_separator_line: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let hover_visible = self.project_editor_companion_split_divider_hover_visible == Some(mode);
        let rail_color: Hsla = if show_separator_line {
            rgb(0x6d6d6d).into()
        } else {
            project_editor_companion_divider_background_color()
        };
        let side_by_side = axis == WorkspaceSplitAxis::Horizontal;
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-split-divider-{}-{}",
                axis.element_slug(),
                mode.element_slug()
            ))
            .relative()
            .flex_shrink_0()
            .when(side_by_side, |this| {
                this.w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS)).h_full()
            })
            .when(!side_by_side, |this| {
                this.h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS)).w_full()
            })
            // Side by side, both sidepanes own a top edge one pixel tall, so carry
            // that hairline across the divider the way the outer one does instead
            // of leaving a notch in it.
            .when(side_by_side, |this| {
                this.border_t_1()
                    .border_color(titlebar_button_border_color())
            })
            .bg(rail_color)
            .child(resize_rail_deferred_strip(
                resize_rail_grab_strip(
                    format!(
                        "ghostex-gpui-project-editor-companion-split-grab-strip-{}-{}",
                        axis.element_slug(),
                        mode.element_slug()
                    ),
                    axis,
                    ResizeRailGrabSide::Straddle,
                )
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    this.set_project_editor_companion_split_divider_hovering(mode, *hovered, cx);
                }))
                .on_mouse_move(
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        this.set_project_editor_companion_split_divider_hovering(mode, true, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_project_editor_companion_split_divider_mouse_down(
                            mode, axis, event, window, cx,
                        );
                    }),
                )
                .when(hover_visible, |this| {
                    this.child(resize_rail_hover_line(
                        format!(
                            "ghostex-gpui-project-editor-companion-split-divider-hover-line-{}-{}",
                            axis.element_slug(),
                            mode.element_slug()
                        ),
                        axis,
                        ResizeRailGrabSide::Straddle,
                    ))
                }),
            ))
            .into_any_element()
    }

    /// One control per arrangement, always both: while the companion shows a
    /// single session each one splits it that way, and while it is split the live
    /// arrangement's control collapses the pair while the other rearranges it.
    /// Dropping the second control once split would leave rearranging reachable
    /// only by collapsing and splitting again, which re-picks the second session.
    fn render_project_editor_companion_pane_split_controls(
        &self,
        mode: TitlebarMode,
        column: Option<ProjectEditorCompanionTerminalSlot>,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        vec![
            self.render_project_editor_companion_split_button(
                mode,
                WorkspaceSplitAxis::Horizontal,
                column,
                cx,
            ),
            self.render_project_editor_companion_split_button(
                mode,
                WorkspaceSplitAxis::Vertical,
                column,
                cx,
            ),
        ]
    }

    /// CDXC:Workarea 2026-09-14 WHY:
    /// Browser takes focus on entry, which dimmed this shared split control while other views kept it bright.
    /// Keep its icon color independent of pane focus so switching views does not change the affordance.
    /// CDXC:Workarea 2026-09-19 DECISION:
    /// User: remove the border line from the companion sidepane's split button.
    pub(crate) fn render_project_editor_companion_split_button(
        &self,
        mode: TitlebarMode,
        axis: WorkspaceSplitAxis,
        column: Option<ProjectEditorCompanionTerminalSlot>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let is_split = self.project_editor_shell.left_companion_split_enabled
            && self.project_editor_shell.left_companion_split_axis == axis
            && self
                .project_editor_companion_secondary_terminal_session_id
                .is_some();
        let tooltip = match (is_split, axis) {
            (true, _) => "Show one companion session",
            (false, WorkspaceSplitAxis::Horizontal) => "Split companion to the right",
            (false, WorkspaceSplitAxis::Vertical) => "Split companion vertically",
        };
        let icon = match (is_split, axis) {
            (true, _) => TITLEBAR_ICON_LAYOUT_SINGLE_PANE,
            (false, WorkspaceSplitAxis::Horizontal) => TITLEBAR_ICON_LAYOUT_COLUMNS,
            (false, WorkspaceSplitAxis::Vertical) => TITLEBAR_ICON_LAYOUT_SPLIT_VERTICAL,
        };
        let pane_slug = match column {
            Some(ProjectEditorCompanionTerminalSlot::Top) => {
                format!("{}-left", mode.element_slug())
            }
            Some(ProjectEditorCompanionTerminalSlot::Bottom) => {
                format!("{}-right", mode.element_slug())
            }
            None => mode.element_slug(),
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-split-{}-{}",
                axis.element_slug(),
                pane_slug
            ))
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(41.0))
            .items_center()
            .justify_center()
            .text_color(workspace_tab_close_active_color())
            .cursor_default()
            .hover(|this| this.bg(workspace_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_project_editor_companion_split(mode, axis, column, window, cx);
                }),
            )
            .managed_tooltip_with_placement(
                ManagedTooltipPlacement::BelowLeft,
                move |window, cx| Tooltip::new(tooltip).build(window, cx),
            )
            .child(titlebar_svg_icon(
                icon,
                13.0,
                workspace_tab_close_active_color(),
            ))
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_divider(
        &self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:CodeEditor 2026-06-22-05:49:
        The project-editor companion boundary is a real reserved layout region between sibling panes. The visible divider is the resize/reset hit target; it persists shell-only companion sizing and does not use invisible overlays or root-level hit-test routing.
        */
        let hover_visible = self.project_editor_companion_divider_hover_visible == Some(mode);
        // The companion is the leading pane and a composited terminal; the main surface behind the
        // rail is a CEF page, so the whole grab strip lies over the companion.
        let grab_side = ResizeRailGrabSide::Leading;
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-divider-{}",
                mode.element_slug()
            ))
            .relative()
            .flex_shrink_0()
            .h_full()
            .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            // The body row sits 1px under the titlebar so panes can own
            // their top edge; carry the titlebar hairline across the divider.
            .border_t_1()
            .border_color(titlebar_button_border_color())
            .bg(project_editor_companion_divider_background_color())
            .child(resize_rail_deferred_strip(
                resize_rail_grab_strip(
                    format!(
                        "ghostex-gpui-project-editor-companion-grab-strip-{}",
                        mode.element_slug()
                    ),
                    WorkspaceSplitAxis::Horizontal,
                    grab_side,
                )
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    this.set_project_editor_companion_divider_hovering(mode, *hovered, cx);
                }))
                .on_mouse_move(
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        this.set_project_editor_companion_divider_hovering(mode, true, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_project_editor_companion_divider_mouse_down(
                            mode, event, window, cx,
                        );
                    }),
                )
                .when(hover_visible, |this| {
                    this.child(resize_rail_hover_line(
                        format!(
                            "ghostex-gpui-project-editor-companion-divider-hover-line-{}",
                            mode.element_slug()
                        ),
                        WorkspaceSplitAxis::Horizontal,
                        grab_side,
                    ))
                }),
            ))
            .into_any_element()
    }
}
