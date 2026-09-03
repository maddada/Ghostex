// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: project-editor companion pane rendering: companion terminal body/slot, split divider/button, collapse button, restore rail, and divider.

use gpui::Animation;
use gpui::AnimationExt as _;
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
use crate::*;

use super::terminal_content_layout::terminal_content_frame;

impl GhostexGpuiApp {
    pub(crate) fn render_project_editor_companion_pane(
        &self,
        mode: TitlebarMode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let is_focused = self.shell_focus == ShellFocusTarget::ProjectEditorCompanion(mode);
        let has_terminal_split = self
            .project_editor_companion_secondary_terminal_session_id
            .is_some();
        let border_state = if has_terminal_split {
            WorkspacePaneBorderState::Neutral
        } else {
            self.project_editor_companion_border_state(mode, window)
        };
        let companion_title = self.project_editor_companion_active_title(mode);
        let view = cx.entity().clone();
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_project_editor_companion_layout_bounds(mode, &child_bounds);
                });
            })
            .id(format!(
                "ghostex-gpui-project-editor-companion-pane-{}",
                mode.element_slug()
            ))
            .flex_grow(project_editor_companion_width_ratio(
                self.project_editor_shell.left_companion_width_ratio,
            ))
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .min_w(px(PROJECT_EDITOR_COMPANION_MIN_WIDTH))
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .border_1()
            .border_color(project_editor_companion_border_color_for_state(
                border_state,
            ))
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
                        mode.element_slug()
                    ))
                    .flex_shrink_0()
                    .h(px(WORKSPACE_TAB_BAR_HEIGHT))
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
                            .child(self.render_project_editor_companion_collapse_button(
                                mode, is_focused, cx,
                            ))
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
                            .child(self.render_project_editor_companion_split_button(
                                mode, is_focused, cx,
                            )),
                    ),
            )
            .child(self.render_project_editor_companion_terminal_body(mode, window, cx))
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_terminal_body(
        &self,
        mode: TitlebarMode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let top_session_id = self.project_editor_companion_terminal_session_id;
        let bottom_session_id = self.project_editor_companion_secondary_terminal_session_id;
        let focused_slot = (bottom_session_id.is_some()
            && self.project_editor_companion_border_state(mode, window)
                == WorkspacePaneBorderState::Focused)
            .then_some(self.project_editor_companion_focused_terminal_slot);
        let split_ratio = self
            .project_editor_shell
            .left_companion_split_ratio
            .clamp(0.1, 0.9);
        let view = cx.entity().clone();
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                if bottom_session_id.is_some() {
                    let _ = view.update(cx, |this, _cx| {
                        this.record_project_editor_companion_split_layout_metrics(&child_bounds);
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
                if bottom_session_id.is_some() {
                    split_ratio
                } else {
                    1.0
                },
                focused_slot == Some(ProjectEditorCompanionTerminalSlot::Top),
                cx,
            ))
            .when_some(bottom_session_id, |this, session_id| {
                this.child(self.render_project_editor_companion_split_divider(
                    mode,
                    focused_slot.is_none(),
                    cx,
                ))
                .child(self.render_project_editor_companion_terminal_slot_body(
                    mode,
                    ProjectEditorCompanionTerminalSlot::Bottom,
                    Some(session_id),
                    1.0 - split_ratio,
                    focused_slot == Some(ProjectEditorCompanionTerminalSlot::Bottom),
                    cx,
                ))
            })
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_terminal_slot_body(
        &self,
        mode: TitlebarMode,
        slot: ProjectEditorCompanionTerminalSlot,
        session_id: Option<TerminalSessionId>,
        flex_grow: f32,
        show_focus_outline: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUISessionChatSurface 2026-08-02:
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
        let has_terminal_split = self
            .project_editor_companion_secondary_terminal_session_id
            .is_some();
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
            .when(slot_id.is_none(), |this| {
                this.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.0))
                        .text_color(workspace_tab_inactive_text_color())
                        .child("No running terminal"),
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
                        .text_color(rgb(0xffffff).opacity(0.24))
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
            .when(has_terminal_split, |this| {
                this.border_1().border_color(if show_focus_outline {
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
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_pane_surface_body(
        &self,
        mode: TitlebarMode,
        slot: ProjectEditorCompanionTerminalSlot,
        session_id: TerminalSessionId,
        flex_grow: f32,
        show_focus_outline: bool,
        content: AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let has_terminal_split = self
            .project_editor_companion_secondary_terminal_session_id
            .is_some();
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
            .when(has_terminal_split, |this| {
                this.border_1().border_color(if show_focus_outline {
                    workspace_pane_focused_border_color()
                } else {
                    rgb(0x000000).opacity(0.0).into()
                })
            })
            .bg(gpui_session_chat_background_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.focus_project_editor_companion_terminal_session(
                        mode, session_id, window, cx,
                    );
                    cx.notify();
                }),
            )
            .child(content)
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_split_divider(
        &self,
        mode: TitlebarMode,
        show_separator_line: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let hover_line_offset =
            (WORKSPACE_SPLIT_HANDLE_THICKNESS - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH) / 2.0;
        let hover_visible = self.project_editor_companion_split_divider_hover_visible == Some(mode);
        let separator_line_color: Hsla = if show_separator_line {
            rgb(0x6d6d6d).into()
        } else {
            rgb(0x000000).opacity(0.0).into()
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-split-divider-{}",
                mode.element_slug()
            ))
            .relative()
            .flex()
            .flex_shrink_0()
            .h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            .w_full()
            .items_center()
            .justify_center()
            .cursor_ns_resize()
            .bg(project_editor_companion_divider_background_color())
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
                        mode, event, window, cx,
                    );
                }),
            )
            .child(
                div()
                    .h(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                    .w_full()
                    .cursor_ns_resize()
                    .bg(separator_line_color),
            )
            .when(hover_visible, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(hover_line_offset))
                        .h(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                        .w_full()
                        .cursor_ns_resize()
                        .bg(sidebar_divider_hover_line_color())
                        .with_animation(
                            format!(
                                "ghostex-gpui-project-editor-companion-split-divider-hover-line-{}",
                                mode.element_slug()
                            ),
                            Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                .with_easing(gpui::ease_out_quint()),
                            |line, delta| line.opacity(delta),
                        ),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_split_button(
        &self,
        mode: TitlebarMode,
        is_focused: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let is_split = self.project_editor_shell.left_companion_split_enabled
            && self
                .project_editor_companion_secondary_terminal_session_id
                .is_some();
        let tooltip = if is_split {
            "Show one companion session"
        } else {
            "Split companion vertically"
        };
        let icon = if is_split {
            TITLEBAR_ICON_LAYOUT_SINGLE_PANE
        } else {
            TITLEBAR_ICON_LAYOUT_SPLIT_VERTICAL
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-split-{}",
                mode.element_slug()
            ))
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(41.0))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(rgb(0x252525))
            .text_color(if is_focused {
                workspace_tab_close_active_color()
            } else {
                workspace_tab_close_inactive_color()
            })
            .cursor_default()
            .hover(|this| this.bg(workspace_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_project_editor_companion_split(mode, window, cx);
                }),
            )
            .managed_tooltip_with_placement(
                ManagedTooltipPlacement::BelowLeft,
                move |window, cx| Tooltip::new(tooltip).build(window, cx),
            )
            .child(titlebar_svg_icon(
                icon,
                13.0,
                if is_focused {
                    workspace_tab_close_active_color()
                } else {
                    workspace_tab_close_inactive_color()
                },
            ))
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_collapse_button(
        &self,
        mode: TitlebarMode,
        is_focused: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let icon_color = if is_focused {
            workspace_tab_close_active_color()
        } else {
            workspace_tab_close_inactive_color()
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-collapse-{}",
                mode.element_slug()
            ))
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(31.0))
            .items_center()
            .justify_center()
            .border_r_1()
            .border_color(rgb(0x252525))
            .text_color(icon_color)
            .cursor_default()
            .hover(|this| this.bg(workspace_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.hide_project_editor_companion(mode, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                Tooltip::new("Hide companion").build(window, cx)
            })
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_LAYOUT_SIDEBAR_LEFT_COLLAPSE,
                13.0,
                icon_color,
            ))
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_restore_rail(
        &self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        v_flex()
            .id(format!(
                "ghostex-gpui-project-editor-companion-restore-rail-{}",
                mode.element_slug()
            ))
            .flex_shrink_0()
            .h_full()
            .w(px(PROJECT_EDITOR_COMPANION_RESTORE_RAIL_WIDTH))
            .items_center()
            .border_r_1()
            .border_t_1()
            .border_color(rgb(0x252525))
            .bg(workspace_tab_bar_color())
            .cursor_default()
            .hover(|this| this.bg(workspace_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.restore_project_editor_companion(mode, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                Tooltip::new("Show companion").build(window, cx)
            })
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .h(px(WORKSPACE_TAB_BAR_HEIGHT))
                    .w_full()
                    .items_center()
                    .justify_center()
                    .child(titlebar_svg_icon(
                        PROJECT_EDITOR_COMPANION_RESTORE_ICON,
                        12.0,
                        rgb(0x737373).into(),
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_divider(
        &self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        The project-editor companion boundary is a real reserved layout region between sibling panes. The visible divider is the resize/reset hit target; it persists shell-only companion sizing and does not use invisible overlays or root-level hit-test routing.
        */
        let hover_line_offset =
            (WORKSPACE_SPLIT_HANDLE_THICKNESS - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH) / 2.0;
        let hover_visible = self.project_editor_companion_divider_hover_visible == Some(mode);
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-divider-{}",
                mode.element_slug()
            ))
            .relative()
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            .items_center()
            .justify_center()
            // The body row sits 1px under the titlebar so panes can own
            // their top edge; carry the titlebar hairline across the divider
            // so its full-height line child starts below it.
            .border_t_1()
            .border_color(titlebar_button_border_color())
            .cursor_ew_resize()
            .bg(project_editor_companion_divider_background_color())
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
            .child(
                div()
                    .h_full()
                    .w(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                    .cursor_ew_resize()
                    .bg(project_editor_companion_divider_line_color()),
            )
            .when(hover_visible, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(hover_line_offset))
                        .h_full()
                        .w(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                        .cursor_ew_resize()
                        .bg(sidebar_divider_hover_line_color())
                        .with_animation(
                            format!(
                                "ghostex-gpui-project-editor-companion-divider-hover-line-{}",
                                mode.element_slug()
                            ),
                            Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                .with_easing(gpui::ease_out_quint()),
                            |line, delta| line.opacity(delta),
                        ),
                )
            })
            .into_any_element()
    }
}
