//! The worktree tray on top of a new chat's message box: This folder, New worktree (from a base
//! branch) or Existing, drawn from the document's `worktreeTray` key. gx-chat-core decides when it
//! shows, what it offers and what is picked (`packages/gx-chat-core/src/menus/draft_worktree.rs`);
//! this file only lays it out and reports picks. The phone draws the same tray in
//! `apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx`.
//!
//! The project chip and its search are `draft_project_picker.rs`.
//!
//! The Cloud Box segment's chip and its boot progress card are `box_picker.rs`.
//!
//! CDXC:Worktrees 2026-10-31 DECISION:
//! User (Q828): the tray sits on top of the message box, attached to it like a tab, while the chat is
//! new. The pick uses the shared bordered segmented control (AGENTS.md "Shared UI controls"); the
//! base branch and the existing worktree open the chat's own menu. No branch name is shown (Q830).
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use crate::app::window::native_modal_kit::{ModalRailItem, modal_bordered_segmented_control};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Rgba,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb, size, svg,
};
use serde_json::{Value, json};

const BASE_BRANCH_TRIGGER: &str = "chat-worktree-base-branch";
const EXISTING_TRIGGER: &str = "chat-worktree-existing";
const FAILED_COLOR: u32 = 0xef4444;
/// One segment of the mode control.
const SEGMENT_WIDTH: f32 = 112.0;
/// The message box's corner radius (`composer.rs`), which the tray is inset by.
const COMPOSER_RADIUS: f32 = 22.0;
/// The gap the composer's footer leaves between the tray and the box (`composer.rs`).
const FOOTER_GAP: f32 = 8.0;

impl NativeChatView {
    /// A chip that opens one of the tray's menus.
    fn worktree_tray_chip(
        &self,
        id: &'static str,
        label: String,
        p: &ChatAppearance,
        busy: bool,
        rows: Vec<Value>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .flex_shrink_0()
            .h(px(26.0 * s))
            .px(px(8.0 * s))
            .flex()
            .items_center()
            .gap(px(5.0 * s))
            .rounded(px(7.0 * s))
            .border_1()
            .border_color(p.composer_border)
            .text_color(p.primary)
            .when(!busy, |chip| {
                chip.chat_cursor_pointer()
                    .hover(|style| style.bg(p.border))
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        if chat.chat_menu_toggled_shut(id, cx) {
                            return;
                        }
                        let at = window.mouse_position();
                        chat.show_chat_menu(
                            rows.clone(),
                            Bounds::new(at, size(px(0.0), px(0.0))),
                            260.0,
                            window,
                            cx,
                        );
                    }))
            })
            .child(
                svg()
                    .path("titlebar/git-branch.svg")
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                div()
                    .max_w(px(200.0 * s))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label),
            )
            .child(
                svg()
                    .path("titlebar/chevron-down.svg")
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .into_any_element()
    }

    pub(crate) fn render_worktree_tray(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tray = self
            .snapshot
            .get("worktreeTray")
            .filter(|tray| tray.is_object())?
            .clone();
        let s = p.scale;
        let busy = tray["busy"] == true;
        let mode = tray["mode"].as_str().unwrap_or("folder").to_owned();
        let segments: Vec<(String, String)> = tray["segments"]
            .as_array()
            .map(|segments| {
                segments
                    .iter()
                    .filter_map(|segment| {
                        Some((
                            segment["mode"].as_str()?.to_owned(),
                            segment["label"].as_str()?.to_owned(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let selected = segments
            .iter()
            .position(|(segment, _)| *segment == mode)
            .unwrap_or(0);
        let items: Vec<ModalRailItem> = segments
            .iter()
            .map(|(_, label)| ModalRailItem {
                label: label.clone().into(),
                trailing: None,
                dot: None,
            })
            .collect();
        let modes: Vec<String> = segments.iter().map(|(mode, _)| mode.clone()).collect();
        let foreground: Rgba = p.primary.into();
        let border: Rgba = p.composer_border.into();
        let trailing: Rgba = p.muted.into();
        let has_choices = !segments.is_empty();
        let control = modal_bordered_segmented_control(
            foreground,
            border,
            trailing,
            "chat-worktree-mode",
            &items,
            selected,
            move |chat: &mut NativeChatView, index, _, cx| {
                if let Some(mode) = modes.get(index) {
                    chat.invoke(json!({"type":"setDraftWorktree","mode":mode}), cx);
                }
            },
            cx,
        )
        .h(px(28.0 * s))
        // Equal segments wide enough for "New worktree" with its padding.
        .w(px(SEGMENT_WIDTH * segments.len() as f32 * s))
        .text_size(px(13.0 * s))
        .when(busy, |control| control.opacity(0.6));

        // A narrow chat wraps the chips after the segments onto a second line instead of clipping.
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .min_w_0()
            .text_size(px(12.5 * s))
            .when_some(tray["projectName"].as_str(), |row, name| {
                row.child(self.render_draft_project_chip(name, p, busy, cx))
                    .children(self.render_draft_machine_chip(p, busy, cx))
            })
            .when(has_choices, |row| row.child(control));
        let status = &tray["status"];
        match status["state"].as_str() {
            Some("creating") | Some("opened") => {
                row = row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .min_w_0()
                        .text_color(p.muted)
                        .child(
                            svg()
                                .path("titlebar/loader2.svg")
                                .size(px(13.0 * s))
                                .flex_shrink_0()
                                .text_color(p.muted),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(status["label"].as_str().unwrap_or_default().to_owned()),
                        ),
                );
            }
            Some("failed") => {
                let message = status["message"].as_str().unwrap_or_default().to_owned();
                // A line of its own, like the boot card: as a flexible item beside the segments
                // the message was cut to its first word ("ghostex b…") even in a wide window.
                row = row.child(
                    div()
                        .id("chat-worktree-failure")
                        .flex()
                        .w_full()
                        .items_center()
                        .gap(px(6.0 * s))
                        .min_w_0()
                        .text_color(rgb(FAILED_COLOR))
                        .child(
                            svg()
                                .path("titlebar/alert-triangle.svg")
                                .size(px(13.0 * s))
                                .flex_shrink_0()
                                .text_color(rgb(FAILED_COLOR)),
                        )
                        .child(
                            div()
                                .id("chat-worktree-failure-message")
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(message.clone())
                                .tooltip(move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(message.clone())
                                        .build(window, cx)
                                }),
                        )
                        .child(
                            div()
                                .id("chat-worktree-retry")
                                .role(gpui::Role::Button)
                                .aria_label("Try again")
                                .flex_shrink_0()
                                .h(px(26.0 * s))
                                .px(px(10.0 * s))
                                .flex()
                                .items_center()
                                .rounded(px(7.0 * s))
                                .border_1()
                                .border_color(p.composer_border)
                                .text_color(p.primary)
                                .chat_cursor_pointer()
                                .hover(|style| style.bg(p.border))
                                .child("Try again")
                                .on_click(cx.listener(|chat, _, window, cx| {
                                    chat.submit("send", window, cx);
                                })),
                        ),
                );
            }
            _ => {}
        }
        if status.is_null() && has_choices {
            match mode.as_str() {
                "newWorktree" => {
                    let base = tray["baseBranch"].as_str().unwrap_or("main").to_owned();
                    let rows: Vec<Value> = tray["branches"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|branch| {
                            let name = branch["name"].as_str()?;
                            Some(json!({
                                "label": name,
                                "detail": if branch["current"] == true { "Current" } else { "" },
                                "checked": name == base,
                                "command": {"type":"setDraftWorktree","baseBranch":name},
                            }))
                        })
                        .collect();
                    row = row
                        .child(div().flex_shrink_0().text_color(p.muted).child("from"))
                        .child(self.worktree_tray_chip(
                            BASE_BRANCH_TRIGGER,
                            base,
                            p,
                            busy,
                            rows,
                            cx,
                        ));
                }
                "existingWorktree" => {
                    let key = tray["worktreeKey"].as_str().unwrap_or_default().to_owned();
                    let label = tray["worktreeLabel"]
                        .as_str()
                        .unwrap_or("Pick a worktree")
                        .to_owned();
                    let rows: Vec<Value> = tray["worktrees"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|worktree| {
                            let worktree_key = worktree["worktreeKey"].as_str()?;
                            Some(json!({
                                "label": worktree["name"],
                                "detail": worktree["branch"],
                                "checked": worktree_key == key,
                                "command": {"type":"setDraftWorktree","worktreeKey":worktree_key},
                            }))
                        })
                        .collect();
                    row = row.child(self.worktree_tray_chip(
                        EXISTING_TRIGGER,
                        label,
                        p,
                        busy,
                        rows,
                        cx,
                    ));
                }
                "cloudBox" => {
                    if tray["cloudBox"].is_object() {
                        row = row.child(self.render_cloud_box_chip(&tray["cloudBox"], p, busy, cx));
                    }
                }
                _ => {
                    // CDXC:CloudBoxes 2026-11-02 DECISION: User (Q1006 note): "move the branch name to align to the end of the left aligned elements in that bar". The current branch sits right after the segments, not at the bar's right end.
                    if let Some(current) = tray["currentBranch"].as_str() {
                        row = row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.0 * s))
                                .flex_shrink_0()
                                .text_color(p.muted)
                                .child(
                                    svg()
                                        .path("titlebar/git-branch.svg")
                                        .size(px(13.0 * s))
                                        .text_color(p.muted),
                                )
                                .child(current.to_owned()),
                        );
                    }
                }
            }
        }
        /*
        CDXC:Worktrees 2026-10-31 DECISION:
        User: "the bar above the composer must be transparent like it". The tray is part of the
        message box: the box's own fill and border (`composer_background`, which window glass turns
        into the bubble's wash), never the pane's solid background. It ends exactly on the box's top
        edge instead of reaching over its border, because two translucent fills stacked there read
        darker; the box's top border is the seam. It is inset by the box's corner radius so its sides
        meet the straight part of that edge, and the chips on it draw only their border so they do
        not stack a second wash on the tray's.
        */
        Some(
            div()
                .w_full()
                .relative()
                .px(px(COMPOSER_RADIUS * s))
                .mb(px(-FOOTER_GAP * s))
                .child(
                    div()
                        .w_full()
                        .px(px(8.0 * s))
                        .pt(px(6.0 * s))
                        .pb(px(7.0 * s))
                        .rounded_t(px(12.0 * s))
                        .border_1()
                        .border_b_0()
                        .border_color(p.composer_border)
                        .bg(p.composer_background)
                        .child(row)
                        .when(status["card"].is_object(), |tray| {
                            tray.child(self.render_cloud_box_card(&status["card"], p))
                        }),
                )
                .into_any_element(),
        )
    }
}
