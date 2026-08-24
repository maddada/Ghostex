// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: terminal missing-session/state placeholders and the project-editor shell entry.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui::rgb;
use gpui_component::h_flex;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_terminal_missing_session_placeholder(
        &self,
        session_id: TerminalSessionId,
    ) -> AnyElement {
        /*
        CDXC:GPUITerminalMissingSessionCopy 2026-06-24-07:38:
        Missing-session visible copy must describe the terminal surface state without exposing source records or private details. Keep this source-only for the parity pass because runtime checks are user-side and validation commands are deferred.
        */
        v_flex()
            .id(format!(
                "ghostex-gpui-terminal-missing-session-placeholder-{}",
                session_id.0
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(0x7f8a99).opacity(0.22))
                    .bg(rgb(0x11151b))
                    .px(px(28.0))
                    .py(px(24.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(rgb(0x7f8a99).opacity(0.16))
                            .px(px(8.0))
                            .py(px(3.0))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xd8dee8).opacity(0.92))
                            .child("Missing"),
                    )
                    .child(
                        div()
                            .mt(px(14.0))
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_title_color())
                            .child("Terminal session unavailable"),
                    )
                    .child(
                        div()
                            .mt(px(7.0))
                            .max_w(px(390.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(
                                "This selected tab has no available terminal session, so no terminal surface can be shown.",
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_terminal_state_placeholder(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        title: String,
        presentation_state: TerminalSessionPresentationState,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        v_flex()
            .id(format!(
                "ghostex-gpui-terminal-state-placeholder-{}-{}",
                session_id.0,
                presentation_state.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(workspace_terminal_placeholder_border_color(
                        presentation_state,
                    ))
                    .bg(workspace_terminal_placeholder_card_color(
                        presentation_state,
                    ))
                    .px(px(28.0))
                    .py(px(24.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(workspace_terminal_placeholder_badge_background(
                                presentation_state,
                            ))
                            .px(px(8.0))
                            .py(px(3.0))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_badge_text_color(
                                presentation_state,
                            ))
                            .child(presentation_state.placeholder_label()),
                    )
                    .child(
                        div()
                            .mt(px(14.0))
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_title_color())
                            .child(presentation_state.placeholder_title()),
                    )
                    .child(
                        div()
                            .mt(px(7.0))
                            .max_w(px(390.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(presentation_state.placeholder_message()),
                    )
                    .child(
                        div()
                            .mt(px(9.0))
                            .max_w(px(390.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.0))
                            .text_color(workspace_terminal_placeholder_session_color())
                            .child(title),
                    )
                    .child(
                        div()
                            .id(format!(
                                "ghostex-gpui-terminal-state-action-{}-{}",
                                session_id.0,
                                presentation_state.element_slug()
                            ))
                            .flex()
                            .h(px(29.0))
                            .mt(px(18.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .border_1()
                            .border_color(workspace_terminal_placeholder_action_border_color(
                                presentation_state,
                            ))
                            .bg(workspace_terminal_placeholder_action_color(
                                presentation_state,
                            ))
                            .px(px(12.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_action_text_color(
                                presentation_state,
                            ))
                            .hover(|this| {
                                this.bg(workspace_terminal_placeholder_action_hover_color(
                                    presentation_state,
                                ))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    this.activate_agents_terminal_placeholder(
                                        pane_id, session_id, cx,
                                    );
                                }),
                            )
                            .child(presentation_state.placeholder_action_label()),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_project_editor_shell(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        Project-editor modes replace the main workspace area while active, but they still flow through the same command-pane wrapper as Agents mode. Browser keeps the existing CEF toolbar/body inside this shell, while Source, Kanban, Automate, and Docs render distinct GPUI-colored placeholders until their direct runtime CEF gates can replace them.

        CDXC:GPUIProjectEditor 2026-06-22-08:15:
        When the companion is hidden, the shell still owns a visible restore rail as a normal left layout sibling before the editor surface. The rail never overlaps the editor surface or Browser CEF child view, and restoring the companion reuses the stored width ratio instead of resetting layout.

        CDXC:GPUIProjectEditorLayout 2026-06-22-17:18:
        Source, Browser, Kanban, and Manage share this horizontal shell, and gpui-component h_flex centers children by default. Override that alignment and make the editor surface slot full-height so placeholders and Browser CEF bodies fill the available workspace height instead of rendering as a centered band with black space above and below.
        */
        let mode_slug = mode.element_slug();
        let surface_border_state = self.project_editor_surface_border_state(mode, window);
        if self.project_editor_shell.left_companion_visible {
            let companion_ratio = project_editor_companion_width_ratio(
                self.project_editor_shell.left_companion_width_ratio,
            );
            let view = cx.entity().clone();
            let surface_view = cx.entity().clone();
            h_flex()
                .on_children_prepainted(move |child_bounds, _window, cx| {
                    let _ = view.update(cx, |this, _cx| {
                        this.record_project_editor_companion_layout_metrics(&child_bounds);
                    });
                })
                .id(format!("ghostex-gpui-project-editor-shell-{}", mode_slug))
                .flex_1()
                .min_w_0()
                .min_h_0()
                .items_start()
                .overflow_hidden()
                .bg(project_editor_shell_background_color())
                .child(self.render_project_editor_companion_pane(mode, window, cx))
                .child(self.render_project_editor_companion_divider(mode, cx))
                .child(
                    div()
                        .on_children_prepainted(move |child_bounds, _window, cx| {
                            let _ = surface_view.update(cx, |this, _cx| {
                                this.record_project_editor_surface_layout_bounds(
                                    mode,
                                    &child_bounds,
                                );
                            });
                        })
                        .id(format!(
                            "ghostex-gpui-project-editor-surface-slot-{}",
                            mode_slug
                        ))
                        .flex()
                        .flex_col()
                        .flex_grow(1.0 - companion_ratio)
                        .flex_shrink_1()
                        .flex_basis(relative(0.0))
                        .h_full()
                        .min_w(px(WORKSPACE_MIN_WIDTH))
                        .min_h_0()
                        .overflow_hidden()
                        .when(mode != TitlebarMode::Browser, |this| {
                            this.border_1()
                                .border_color(workspace_pane_border_color_for_state(
                                    surface_border_state,
                                ))
                        })
                        .child(self.render_project_editor_surface(mode, window, cx)),
                )
                .into_any_element()
        } else {
            let surface_view = cx.entity().clone();
            h_flex()
                .id(format!("ghostex-gpui-project-editor-shell-{}", mode_slug))
                .flex()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .items_start()
                .overflow_hidden()
                .bg(project_editor_shell_background_color())
                .child(self.render_project_editor_companion_restore_rail(mode, cx))
                .child(
                    div()
                        .on_children_prepainted(move |child_bounds, _window, cx| {
                            let _ = surface_view.update(cx, |this, _cx| {
                                this.record_project_editor_surface_layout_bounds(
                                    mode,
                                    &child_bounds,
                                );
                            });
                        })
                        .id(format!(
                            "ghostex-gpui-project-editor-surface-slot-{}",
                            mode_slug
                        ))
                        .flex()
                        .flex_col()
                        .flex_1()
                        .h_full()
                        .min_w(px(WORKSPACE_MIN_WIDTH))
                        .min_h_0()
                        .overflow_hidden()
                        .when(mode != TitlebarMode::Browser, |this| {
                            this.border_1()
                                .border_color(workspace_pane_border_color_for_state(
                                    surface_border_state,
                                ))
                        })
                        .child(self.render_project_editor_surface(mode, window, cx)),
                )
                .into_any_element()
        }
    }
}
