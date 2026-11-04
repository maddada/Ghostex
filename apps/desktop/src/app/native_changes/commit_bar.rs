//! The commit bar at the bottom of the Changes view: the message, the agent that writes it when
//! it is left empty, the main button and its menu.
//!
//! CDXC:Changes 2026-10-25 DECISION:
//! User: "why does merge to main need worktree even??? i dont understand. and why would we need 'commit on new branch'??? please think about MOST NEEDED options here in this dropdown". The menu holds Commit, Commit & Push, Commit, Push & Create PR (which on the default branch first moves the work onto a new branch named from the commit message, so there is no separate Commit on new branch), Multiple Commits, and Merge into main, offered whenever the branch is not the default branch (worktree or not) and asking first. Delete worktree after is not here; the sidebar's worktree menu deletes worktrees. This replaces the old commit review window's footer.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

use super::rows::{ICON_CHEVRON_DOWN, small_button};
use super::style::{CODE_SIZE, COMMIT_BUTTON_HEIGHT, ChangesStyle, ICON_SIZE, SMALL_SIZE};
use super::view::{ChangesView, CommitAction, Confirm};
use crate::app::window::native_modal_kit::{hsla, modal_icon, modal_text_area, rgba_of};

impl ChangesView {
    pub(crate) fn render_commit_bar(
        &mut self,
        s: &ChangesStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let data = self.data.as_ref()?;
        let uncommitted = data.uncommitted_paths.len();
        if uncommitted == 0 {
            return None;
        }
        let input = self.message.clone()?;
        let on_default = data.on_default_branch;
        let branch = data.branch.clone().unwrap_or_default();
        let base = data
            .default_branch
            .clone()
            .unwrap_or_else(|| "main".to_string());
        let count = self.commit_count();
        let agent_name = self
            .agent_id
            .as_ref()
            .and_then(|id| {
                self.info
                    .as_ref()?
                    .agents
                    .iter()
                    .find(|agent| &agent.0 == id)
                    .map(|agent| agent.1.clone())
            })
            .unwrap_or_else(|| "Agent".to_string());
        let confirming_merge = self.confirm == Some(Confirm::Merge);
        let disabled = count == 0;
        let main_label = self.commit_action.short_label();
        let files_label = if self.excluded.is_empty() {
            format!(
                "{uncommitted} {}",
                if uncommitted == 1 { "file" } else { "files" }
            )
        } else {
            format!("{count} of {uncommitted} files")
        };
        let bar = v_flex()
            .id("changes-commit-bar")
            .flex_none()
            .w_full()
            .relative()
            .gap(px(6.0))
            .p(px(8.0))
            .border_t_1()
            .border_color(hsla(s.hairline))
            .bg(hsla(s.panel))
            .child(modal_text_area(&s.p, &input, Some(40.0), false, window, cx))
            .when(confirming_merge, |this| {
                this.child(
                    h_flex()
                        .gap(px(6.0))
                        .text_size(px(SMALL_SIZE))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(hsla(s.text))
                                .child(format!(
                                    "Merge {branch} into {base}? Uncommitted files are committed first, and you end up on {base}."
                                )),
                        )
                        .child(small_button(s, "changes-merge-cancel", "Cancel", false, cx.listener(|this, _, _, cx| {
                            this.confirm = None;
                            cx.notify();
                        })))
                        .child(small_button(s, "changes-merge-yes", "Merge", true, cx.listener(|this, _, window, cx| {
                            this.run_merge(window, cx);
                        }))),
                )
            })
            // CDXC:Changes 2026-10-25 DECISION: User picked "Both dark, side by side" (Q558): the file count stays on the left; the agent picker and the Commit split button sit together on the right with the same dark toolbar fill, 1px subtle border, 32px height, font and radius, and Commit's menu chevron is part of its split button behind a thin divider.
            .child(
                h_flex()
                    .gap(px(8.0))
                    .child(div().text_size(px(SMALL_SIZE)).text_color(hsla(s.muted)).child(files_label))
                    .when(!self.excluded.is_empty(), |this| {
                        this.child(
                            div()
                                .id("changes-include-all")
                                .text_size(px(SMALL_SIZE))
                                .text_color(hsla(s.accent))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| this.include_all(cx)))
                                .child("Include all"),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        dark_button(s)
                            .id("changes-agent")
                            .px(px(12.0))
                            .gap(px(6.0))
                            .rounded(px(8.0))
                            .hover(|this| this.bg(hsla(s.p.raised_hover)))
                            .tooltip(|window, cx| {
                                gpui_component::tooltip::Tooltip::new(
                                    "Writes the commit message when you leave it empty",
                                )
                                .build(window, cx)
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.commit_menu_open = false;
                                this.agent_select.open = !this.agent_select.open;
                                cx.notify();
                            }))
                            .child(agent_name)
                            .child(modal_icon(ICON_CHEVRON_DOWN, ICON_SIZE - 3.0, s.muted)),
                    )
                    .child(
                        dark_button(s)
                            .rounded(px(8.0))
                            .overflow_hidden()
                            .when(disabled, |this| this.opacity(0.5))
                            .child(
                                div()
                                    .id("changes-commit")
                                    .h_full()
                                    .px(px(12.0))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .hover(|this| this.bg(hsla(s.p.raised_hover)))
                                    .on_click(cx.listener(|this, _, window, cx| this.run_commit(window, cx)))
                                    .child(main_label),
                            )
                            .child(div().w(px(1.0)).h_full().bg(hsla(s.hairline)))
                            .child(
                                div()
                                    .id("changes-commit-menu")
                                    .h_full()
                                    .w(px(30.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|this| this.bg(hsla(s.p.raised_hover)))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.agent_select.close();
                                        this.commit_menu_open = !this.commit_menu_open;
                                        cx.notify();
                                    }))
                                    .child(modal_icon(ICON_CHEVRON_DOWN, ICON_SIZE - 3.0, s.muted)),
                            ),
                    ),
            )
            .when(self.commit_menu_open, |this| this.child(self.render_commit_menu(s, on_default, &base, cx)))
            .when(self.agent_select.open, |this| this.child(self.render_agent_menu(s, cx)))
            .into_any_element();
        Some(bar)
    }

    fn render_commit_menu(
        &mut self,
        s: &ChangesStyle,
        on_default: bool,
        base: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.commit_action;
        let row = |id: &'static str, label: String, detail: &'static str, checked: bool| {
            h_flex()
                .id(id)
                .min_h(px(34.0))
                .px(px(8.0))
                .gap(px(8.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .hover(|this| this.bg(hsla(s.hover)))
                .child(div().w(px(12.0)).flex_none().when(checked, |this| {
                    this.child(modal_icon(super::rows::ICON_CHECK, 12.0, s.accent))
                }))
                // The text column takes the rest of the row, so a long description wraps instead of
                // running past the menu's edge.
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .py(px(4.0))
                        .child(
                            div()
                                .text_size(px(CODE_SIZE))
                                .text_color(hsla(s.text))
                                .child(label),
                        )
                        .child(
                            div()
                                .text_size(px(SMALL_SIZE - 1.0))
                                .text_color(hsla(s.muted))
                                .child(detail),
                        ),
                )
        };
        let action_row = |id: &'static str,
                          action: CommitAction,
                          detail: &'static str,
                          cx: &mut Context<Self>| {
            row(
                id,
                action.label(on_default).to_string(),
                detail,
                current == action,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.commit_action = action;
                this.commit_menu_open = false;
                cx.notify();
            }))
        };
        v_flex()
            .id("changes-commit-dropdown")
            .absolute()
            .right(px(8.0))
            .bottom(px(COMMIT_BUTTON_HEIGHT + 16.0))
            .w(px(320.0))
            .map(|this| self.fit_menu_in_view(this))
            .p(px(4.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(s.menu_bubble().1)
            .bg(s.menu_bubble().0)
            .shadow_lg()
            // A menu takes the pointer, so scrolling it never scrolls the diff behind it.
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(action_row("changes-action-commit", CommitAction::Commit, "Commit the picked files here.", cx))
            .child(action_row("changes-action-push", CommitAction::CommitPush, "Commit, then push the branch.", cx))
            .child(action_row(
                "changes-action-pr",
                CommitAction::CommitPushPr,
                if on_default {
                    "You're on the default branch: moves the work to a new branch named from the message first."
                } else {
                    "Commit, push, then open a pull request."
                },
                cx,
            ))
            .child(div().h(px(1.0)).my(px(4.0)).bg(hsla(s.hairline)))
            .child(
                row(
                    "changes-action-multiple",
                    "Multiple Commits".to_string(),
                    "The agent splits the changes into logical commits.",
                    false,
                )
                .on_click(cx.listener(|this, _, window, cx| this.run_multiple_commits(window, cx))),
            )
            .when(self.can_merge(), |this| {
                this.child(
                    row(
                        "changes-action-merge",
                        format!("Merge into {base}\u{2026}"),
                        "Merge this branch into the default branch. Asks first.",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.commit_menu_open = false;
                        this.confirm = Some(Confirm::Merge);
                        cx.notify();
                    })),
                )
            })
            .into_any_element()
    }

    /// A menu anchored at the commit bar's right edge never runs past the view's left edge: in a
    /// narrow view it narrows and its lines wrap.
    fn fit_menu_in_view<E: Styled>(&self, menu: E) -> E {
        if self.view_width > px(0.0) {
            menu.max_w(self.view_width - px(16.0))
        } else {
            menu
        }
    }

    fn render_agent_menu(&mut self, s: &ChangesStyle, cx: &mut Context<Self>) -> AnyElement {
        let agents = self
            .info
            .as_ref()
            .map(|info| info.agents.clone())
            .unwrap_or_default();
        let current = self.agent_id.clone();
        v_flex()
            .id("changes-agent-dropdown")
            .absolute()
            .right(px(8.0))
            .bottom(px(COMMIT_BUTTON_HEIGHT + 16.0))
            .w(px(220.0))
            .map(|this| self.fit_menu_in_view(this))
            .max_h(px(260.0))
            .overflow_y_scroll()
            .p(px(4.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(s.menu_bubble().1)
            .bg(s.menu_bubble().0)
            .shadow_lg()
            // A menu takes the pointer, so scrolling it never scrolls the diff behind it.
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(agents.into_iter().map(|(id, name)| {
                let checked = current.as_deref() == Some(id.as_str());
                let pick = id.clone();
                h_flex()
                    .id(SharedString::from(format!("changes-agent-{id}")))
                    .min_h(px(30.0))
                    .py(px(4.0))
                    .px(px(8.0))
                    .gap(px(8.0))
                    .rounded(px(5.0))
                    .text_size(px(CODE_SIZE))
                    .cursor_pointer()
                    .hover(|this| this.bg(hsla(s.hover)))
                    .text_color(hsla(if checked {
                        s.text
                    } else {
                        rgba_of(s.text, 0.85)
                    }))
                    .child(div().w(px(12.0)).when(checked, |this| {
                        this.child(modal_icon(super::rows::ICON_CHECK, 12.0, s.accent))
                    }))
                    .child(div().flex_1().min_w_0().child(name))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.agent_id = Some(pick.clone());
                        this.agent_select.close();
                        (this.host)(
                            super::view::ChangesRequest::AgentPicked(pick.clone()),
                            window,
                            cx,
                        );
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }
}

/// The commit bar's shared button shape: the app's dark toolbar fill with a subtle border.
fn dark_button(s: &ChangesStyle) -> gpui::Div {
    h_flex()
        .flex_none()
        .h(px(COMMIT_BUTTON_HEIGHT))
        .border_1()
        .border_color(hsla(s.hairline))
        .bg(hsla(s.p.raised))
        .text_color(hsla(s.text))
        .text_size(px(SMALL_SIZE))
        .font_weight(FontWeight::MEDIUM)
}
