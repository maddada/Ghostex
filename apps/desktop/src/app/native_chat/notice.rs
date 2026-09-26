use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::json;

impl NativeChatView {
    /// CDXC:SessionChat 2026-09-16 DECISION: User: notices are the shared status card. Actions sit right-aligned in the footer band with a keyed action on the left; the 2026-09-07 20px padding rule is superseded by the card's shared padding. User: no "Selected in terminal" badge on any picker row, in any state.
    pub(crate) fn render_notice(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let snapshot = self.snapshot.clone();
        let notice = &snapshot["terminalNotice"];
        if !notice.is_object() || snapshot["noticeVisible"] != true {
            self.terminal_dialog_input = None;
            return None;
        }
        let key = format!(
            "notice:{}:{}",
            text(notice, "kind"),
            text(notice, "detectedAt")
        );
        let choices = notice["choices"].as_array().cloned().unwrap_or_default();
        let answerable = !choices.is_empty();
        let collapsed = answerable && !self.expanded.contains(&key);
        // The card keeps its collapsed choices while shut, so an opening body grows from their
        // height rather than from nothing.
        let motion = answerable
            .then(|| self.disclosure_frame(&key, !collapsed, cx))
            .flatten();
        let body_key = key.clone();
        // CDXC:SessionChat 2026-09-07 DECISION: User: the rate-limit picker already has clickable options, so omit its redundant Previous/Next/Confirm/Cancel controls and terminal footer.
        let rate_limit = choices.iter().any(|choice| {
            text(choice, "label").starts_with("Wait here, then continue automatically")
        });
        let mut body = Vec::new();
        let mut actions = Vec::new();
        if notice["dialog"].is_object()
            && notice["dialog"]["rows"]
                .as_array()
                .is_some_and(Vec::is_empty)
        {
            let (body, actions) = self.render_terminal_dialog(&notice["dialog"], p, window, cx);
            let copy_title = text(&notice["dialog"]["presentation"]["copy"], "title");
            return Some(self.status_card(
                if copy_title.is_empty() {
                    text(&notice["dialog"], "title")
                } else {
                    copy_title
                },
                "titlebar/terminal-2.svg",
                body,
                actions,
                p,
            ));
        }
        if !collapsed && notice["detail"].is_string() {
            body.push(
                div()
                    .line_height(px(19.6 * p.scale))
                    .child(text(notice, "detail"))
                    .into_any_element(),
            );
        }
        if answerable {
            let mut rows = div()
                .id("notice-choices")
                .flex()
                .gap(px(6.0 * p.scale))
                .max_h(px(f32::from(window.viewport_size().height) * 0.45))
                .overflow_y_scroll();
            if !collapsed {
                rows = rows.flex_col();
            }
            let count = if collapsed {
                notice["collapsedChoiceCount"].as_u64().unwrap_or(2) as usize
            } else {
                choices.len()
            };
            let secondary = notice["secondaryChoice"].as_u64().map(|index| index as usize);
            for (index, choice) in choices.iter().take(count).enumerate() {
                let shortcut = if self.snapshot["showShortcutLabels"] != false {
                    if index == 0 {
                        Some(
                            if cfg!(target_os = "macos") {
                                "⌘Enter"
                            } else {
                                "Ctrl+Enter"
                            }
                            .to_string(),
                        )
                    } else if Some(index) == secondary {
                        Some("Esc".to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                // CDXC:SessionChat 2026-09-26 DECISION:
                // User: a notice card's choice must never wrap onto 2 lines; truncate it with "..." and show the whole label on hover.
                // SEE-ALSO: apps/mobile/app/src/chat/native/cards/NoticeCard.tsx, where pressing and holding the choice stands in for hover.
                let row = self.choice_row(
                    format!("notice-choice:{}", choice["index"]),
                    text(choice, if collapsed { "collapsedLabel" } else { "label" }),
                    String::new(),
                    false,
                    shortcut,
                    collapsed,
                    true,
                    snapshot["questionCard"]["busy"] == true,
                    json!({"type":"answer","answer":choice["answer"]}),
                    p,
                    cx,
                );
                rows = rows.child(
                    div()
                        .min_w_0()
                        .when(collapsed, |row| row.flex_1())
                        .child(row),
                );
            }
            body.push(rows.into_any_element());
        }
        if !collapsed && !rate_limit && notice["dialog"].is_object() {
            let (dialog_body, dialog_actions) =
                self.render_terminal_dialog(&notice["dialog"], p, window, cx);
            body.extend(dialog_body);
            actions.extend(dialog_actions);
        }
        if !collapsed && !text(notice, "screenTail").is_empty() {
            // CDXC:SessionChat 2026-09-13 DECISION: User: always hide the terminal-output disclosure in narrow chats. Use the shared narrow-chat breakpoint and hide already-expanded output as well.
            if f32::from(self.bounds.get().size.width) / p.scale <= 1070.0 {
                body.push(div().into_any_element());
            } else {
                let tail_key = format!("{key}:tail");
                let tail_open = self.expanded.contains(&tail_key);
                let tail_motion = self.disclosure_frame(&tail_key, tail_open, cx);
                let tail_body_key = tail_key.clone();
                let mut tail = div().flex().flex_col().gap(px(8.0 * p.scale)).child(
                    div()
                        .id("notice-terminal-output")
                        .tab_index(0)
                        .role(gpui::Role::Button)
                        .aria_label(if tail_open {
                            "Hide terminal output"
                        } else {
                            "Show terminal output"
                        })
                        .flex()
                        .items_center()
                        .gap(px(4.0 * p.scale))
                        .chat_cursor_pointer()
                        .h(px(32.0 * p.scale))
                        .px(px(12.0 * p.scale))
                        .border_1()
                        .border_color(p.control_border)
                        .rounded(px(6.0 * p.scale))
                        .text_size(px(14.0 * p.scale))
                        .text_color(p.foreground)
                        .child(if tail_open {
                            "Hide terminal output"
                        } else {
                            "Show terminal output"
                        })
                        .child(
                            gpui::svg()
                                .path(if tail_open {
                                    "titlebar/chevron-down.svg"
                                } else {
                                    "titlebar/chevron-right.svg"
                                })
                                .size(px(12.0 * p.scale))
                                .text_color(p.muted),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.expanded.remove(&tail_key) {
                                this.expanded.insert(tail_key.clone());
                            }
                            cx.notify();
                        })),
                );
                if tail_open || tail_motion.is_some() {
                    tail = tail.child(
                        self.disclosure_body_motion(
                            &tail_body_key,
                            tail_motion,
                            8.0 * p.scale,
                            div()
                                .id("notice-terminal-output-text")
                                .max_h(px(240.0 * p.scale))
                                .overflow_y_scroll()
                                .rounded(px(8.0 * p.scale))
                                .border_1()
                                .border_color(p.control_border.opacity(0.65))
                                .bg(p.background.opacity(0.7))
                                .p(px(12.0 * p.scale))
                                .font_family("Menlo")
                                .text_size(px(12.0 * p.scale))
                                .child(text(notice, "screenTail"))
                                .into_any_element(),
                        ),
                    );
                }
                body.push(tail.into_any_element());
            }
        }
        // CDXC:AgentProviders 2026-09-13 DECISION: User approved restricting Switch account to relevant notices after it appeared on a queued reply. Only sign-in and usage-limit notices offer the existing account picker beside Open terminal, superseding the September 12 rule for all terminal notices.
        if !collapsed
            && snapshot["accountPanel"].is_object()
            && matches!(notice["kind"].as_str(), Some("loginExpired" | "usageLimit"))
        {
            actions.push(
                div()
                    .id("notice-switch-account")
                    .role(gpui::Role::Button)
                    .aria_label("Switch account")
                    .chat_cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * p.scale))
                    .px(px(8.0 * p.scale))
                    .py(px(4.0 * p.scale))
                    .rounded(px(6.0 * p.scale))
                    .border_1()
                    .border_color(p.border)
                    .text_color(p.primary)
                    .hover(|style| style.bg(p.border))
                    .child(
                        gpui::svg()
                            .path("titlebar/switch-horizontal.svg")
                            .size(px(14.0 * p.scale))
                            .text_color(p.primary),
                    )
                    .child("Switch account")
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                        this.show_account_panel(event.position(), window, cx)
                    }))
                    .into_any_element(),
            );
        }
        // Trust and Remember stays reachable on a collapsed picker, as on
        // the React card: the collapsed rows are the prompt's own Yes/No.
        for action in notice["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|action| !collapsed || action["kind"] == "trustAndRemember")
        {
            if action["kind"] == "switchToTerminal" {
                actions.push(
                    div()
                        .id("notice-terminal-view")
                        .role(gpui::Role::Button)
                        .aria_label("Terminal View")
                        .chat_cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * p.scale))
                        .px(px(8.0 * p.scale))
                        .py(px(4.0 * p.scale))
                        .rounded(px(6.0 * p.scale))
                        .border_1()
                        .border_color(p.border)
                        .text_color(p.primary)
                        .hover(|style| style.bg(p.border))
                        .child(
                            gpui::svg()
                                .path("titlebar/terminal-2.svg")
                                .size(px(14.0 * p.scale))
                                .text_color(p.primary),
                        )
                        .child("Terminal View")
                        .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                            this.perform_composer_action(
                                "terminalView",
                                event.position(),
                                window,
                                cx,
                            )
                        }))
                        .into_any_element(),
                );
            } else {
                let answer = action["answer"].clone();
                actions.push(self.chat_button(
                    format!("notice-action:{}", text(action, "id")),
                    text(action, "label"),
                    json!({"type":"answer","answer":answer}),
                    p,
                    cx,
                ));
            }
        }
        if notice["choices"].is_null() && notice["dialog"].is_null() {
            actions.push(self.chat_button(
                "dismiss-notice".into(),
                "Dismiss".into(),
                json!({"type":"dismissNotice"}),
                p,
                cx,
            ));
        }
        if let Some(error) = snapshot["noticeError"].as_str() {
            body.push(
                div()
                    .id("notice-choice-error")
                    .role(gpui::Role::Alert)
                    .text_size(px(14.0 * p.scale))
                    .line_height(px(19.25 * p.scale))
                    .text_color(p.card_muted)
                    .child(error.to_owned())
                    .into_any_element(),
            );
        }
        let icon = match notice["severity"].as_str() {
            Some("error") => "titlebar/alert-circle.svg",
            Some("warning") => "titlebar/alert-triangle.svg",
            _ => "titlebar/info-circle.svg",
        };
        let header = div()
            .id("notice-header")
            .flex()
            .items_start()
            .gap(px(8.0 * p.scale))
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(14.0 * p.scale))
                    .mt(px(4.0 * p.scale))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_1()
                    .text_color(p.foreground)
                    .child(text(notice, "title")),
            )
            .when(answerable, |header| {
                header
                    .mx(px(-16.0 * p.scale))
                    .mt(px(-12.0 * p.scale))
                    .mb(px(-2.0 * p.scale))
                    .px(px(16.0 * p.scale))
                    .pt(px(12.0 * p.scale))
                    .pb(px(6.0 * p.scale))
                    .rounded_t(px(12.0 * p.scale - 1.0))
                    .hover(|style| style.bg(super::cards::card_hover_fill(p)))
                    .tab_index(0)
                    .role(gpui::Role::Button)
                    .aria_label(if collapsed {
                        "Show all options"
                    } else {
                        "Show less"
                    })
                    .chat_cursor_pointer()
                    .child(
                        gpui::svg()
                            .path(if collapsed {
                                "titlebar/chevron-right.svg"
                            } else {
                                "titlebar/chevron-down.svg"
                            })
                            .size(px(14.0 * p.scale))
                            .mt(px(4.0 * p.scale))
                            .text_color(p.muted),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded.remove(&key) {
                            this.expanded.insert(key.clone());
                        }
                        cx.notify();
                    }))
            });
        Some(self.status_card_with_header_motion(
            super::cards::CardBodyMotion {
                key: &body_key,
                frame: motion,
                shut_body: true,
                shut: collapsed,
            },
            header.into_any_element(),
            body,
            actions,
            p,
        ))
    }
}
