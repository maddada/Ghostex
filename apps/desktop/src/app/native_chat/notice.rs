use super::tone_card::{ToneBodyMotion, ToneButton, ToneCardParts, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::json;

impl NativeChatView {
    /// CDXC:SessionChat 2026-10-11 DECISION: User picked the proposed notices design: a notice is a tone card (tone_card.rs) washed in the tone the chat core gives it, with its actions in the footer (the account switch first and filled, Terminal view beside it, Dismiss quiet at the right end); this supersedes the 2026-09-16 rule that notices are the shared status card. The `/resume` picker, pass-through and side-question cards keep their own shells. User (2026-09-16): no "Selected in terminal" badge on any picker row, in any state.
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
            self.terminal_picker_input = None;
            return None;
        }
        // A Ghostex Collab guest sees what the agent asks, never its answers: only the owner
        // answers a prompt (gxserver refuses a guest's answer too).
        if self.collab_guest() {
            let waiting = text(&snapshot["collab"], "waitingForOwner");
            let title = [
                text(notice, "title"),
                text(notice, "label"),
                text(notice, "kind"),
            ]
            .into_iter()
            .find(|value| !value.is_empty())
            .unwrap_or_else(|| "The agent is asking something".to_string());
            return Some(self.tone_card(
                ToneCardParts {
                    look: tone_look("wait", p),
                    title,
                    detail: waiting,
                    trailing: Vec::new(),
                    body: Vec::new(),
                    footer_start: Vec::new(),
                    footer_note: None,
                    footer_end: Vec::new(),
                },
                None,
                None,
                p,
            ));
        }
        if notice["dialog"]["presentation"]["picker"].is_object() {
            self.terminal_dialog_input = None;
            return Some(self.terminal_picker_card(notice, p, window, cx));
        }
        self.terminal_picker_input = None;
        if notice["passthrough"].is_object() {
            return Some(self.passthrough_card(notice, p, window, cx));
        }
        let key = format!(
            "notice:{}:{}",
            text(notice, "kind"),
            text(notice, "detectedAt")
        );
        let choices = notice["choices"].as_array().cloned().unwrap_or_default();
        let answerable = !choices.is_empty();
        // A tabbed Claude panel with option rows (`/plugin`'s install scope) shows its details
        // above the rows and never folds them away (terminal_prompts.rs `highlighted_row`).
        let panel = notice["dialog"]["presentation"]["blocks"]
            .as_array()
            .is_some_and(|blocks| !blocks.is_empty());
        // A live dialog's rows are numbered choices that open with the card (dialog_rows.rs).
        let numbered = notice["numbered"] == true;
        let foldable = answerable && !panel && !numbered;
        let collapsed = foldable && !self.expanded.contains(&key);
        // The card keeps its collapsed choices while shut, so an opening body grows from their
        // height rather than from nothing.
        let motion = foldable
            .then(|| self.disclosure_frame(&key, !collapsed, cx))
            .flatten();
        let body_key = key.clone();
        // CDXC:SessionChat 2026-09-07 DECISION: User: the rate-limit picker already has clickable options, so omit its redundant Previous/Next/Confirm/Cancel controls and terminal footer.
        let rate_limit = choices.iter().any(|choice| {
            text(choice, "label").starts_with("Wait here, then continue automatically")
        });
        let mut body = Vec::new();
        // The notice's own actions lead the footer; a live dialog's buttons and Dismiss sit at its
        // right end.
        let mut actions = Vec::new();
        let mut footer_end = Vec::new();
        if notice["dialog"].is_object()
            && !numbered
            && notice["dialog"]["rows"]
                .as_array()
                .is_some_and(Vec::is_empty)
        {
            if notice["dialog"]["presentation"]["sideQuestion"].is_object() {
                self.terminal_dialog_input = None;
                return Some(self.side_question_card(notice, p, window, cx));
            }
            let (mut body, mut actions) =
                self.render_terminal_dialog(&notice["dialog"], p, window, cx);
            // A refused answer says why, and the card can then be closed (`dismiss_notice`).
            if let Some(error) = snapshot["noticeError"].as_str() {
                body.push(
                    div()
                        .id("terminal-dialog-error")
                        .role(gpui::Role::Alert)
                        .text_size(px(14.0 * p.scale))
                        .line_height(px(19.25 * p.scale))
                        .text_color(p.card_muted)
                        .child(error.to_owned())
                        .into_any_element(),
                );
                actions.push(
                    self.tone_button(
                        "dismiss-notice",
                        "Dismiss",
                        None,
                        ToneButton::Secondary,
                        None,
                        false,
                        p,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.invoke(json!({"type":"dismissNotice"}), cx)
                    }))
                    .into_any_element(),
                );
            }
            // "Ready to code?" with "Tell Claude what to change" highlighted: the plan the
            // feedback is about stays readable above the field.
            if notice["plan"].is_object() {
                let plan = &notice["plan"];
                body.insert(
                    0,
                    self.side_answer_body(
                        format!("ready-to-code-plan:{key}"),
                        text(plan, "markdown"),
                        &plan["references"],
                        Some(window.viewport_size().height.as_f32() * 0.55),
                        p,
                        cx,
                    ),
                );
            }
            let copy_title = text(&notice["dialog"]["presentation"]["copy"], "title");
            let panel_title = text(&notice["dialog"]["presentation"], "title");
            return Some(self.tone_card(
                ToneCardParts {
                    look: tone_look(&text(notice, "tone"), p),
                    title: if !copy_title.is_empty() {
                        copy_title
                    } else if !panel_title.is_empty() {
                        panel_title
                    } else {
                        text(&notice["dialog"], "title")
                    },
                    detail: String::new(),
                    trailing: Vec::new(),
                    body,
                    footer_start: Vec::new(),
                    footer_note: None,
                    footer_end: actions,
                },
                None,
                None,
                p,
            ));
        }
        if panel && !rate_limit {
            let (dialog_body, dialog_actions) =
                self.render_terminal_dialog(&notice["dialog"], p, window, cx);
            body.extend(dialog_body);
            footer_end.extend(dialog_actions);
        }
        // A permission prompt read whole (`approval` in the chat core) says what it asks itself.
        let approval = notice["approval"].is_object();
        if approval {
            body.extend(self.permission_details(&notice["approval"], collapsed, &key, p, cx));
        }
        // The plan "Ready to code?" asks to approve, whole: Claude's pane shows only a screenful of
        // it (`plan` in the chat core's questions/document.rs).
        if !collapsed && notice["plan"].is_object() {
            let plan = &notice["plan"];
            body.push(self.side_answer_body(
                format!("ready-to-code-plan:{key}"),
                text(plan, "markdown"),
                &plan["references"],
                Some(window.viewport_size().height.as_f32() * 0.55),
                p,
                cx,
            ));
        }
        if !collapsed && !panel && !approval && notice["detail"].is_string() {
            body.push(
                div()
                    .line_height(px(19.6 * p.scale))
                    .child(text(notice, "detail"))
                    .into_any_element(),
            );
        }
        // The command a collapsed approval card is asking to run (`collapsedDetail` in the chat core).
        if collapsed && notice["collapsedDetail"].is_string() {
            body.push(
                div()
                    .id("notice-collapsed-detail")
                    .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                    .text_size(px(12.0 * p.scale))
                    .line_height(px(17.0 * p.scale))
                    .text_color(p.card_muted)
                    .child(text(notice, "collapsedDetail"))
                    .into_any_element(),
            );
        }
        // The terminal's own words (`details`) and what a failed start printed (`errorLines`), behind Details.
        if !collapsed {
            body.extend(self.details_toggle(
                &key,
                vec![text(notice, "details"), text(notice, "errorLines")],
                p,
                cx,
            ));
        }
        if answerable {
            let mut rows = div()
                .id("notice-choices")
                .flex()
                .gap(px(6.0 * p.scale))
                .max_h(px(
                    f32::from(window.viewport_size().height) * if panel { 0.3 } else { 0.45 }
                ))
                .overflow_y_scroll();
            if !collapsed {
                rows = rows.flex_col();
            }
            // CDXC:SessionChat 2026-10-28 DECISION: a folded card shows the rows the chat core names in `collapsedChoices` (the Cmd+Enter row and the No, Q706), so a permission prompt folds to Yes and No; each row keeps its own answer.
            // SEE-ALSO: packages/gx-chat-core/src/questions/notice_choices.rs (collapsed_choice_positions).
            let shown: Vec<&serde_json::Value> = if collapsed {
                notice["collapsedChoices"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|position| choices.get(position.as_u64()? as usize))
                    .collect()
            } else {
                choices.iter().collect()
            };
            // CDXC:SessionChat 2026-10-27 DECISION: the number badge is a row's only hint (Q680, "numbers only"): no ⌘Enter or Esc keycap on any row. Both keys keep answering their rows (keyboard.rs).
            // SEE-ALSO: packages/gx-chat-core/src/questions/terminal_prompts.rs (CDXC:SessionChat 2026-10-27).
            let shortcuts: Vec<Option<String>> = vec![None; shown.len()];
            /*
            CDXC:SessionChat 2026-09-27 DECISION:
            User: when a notice card's choices do not fit side by side, "make buttons appear full width and on top of each other", for every card like the model switch confirmation.
            Each collapsed button starts at the width the widest label needs on one line and the row wraps, so the buttons share the row in equal halves only when every label fits whole, and otherwise stack full width; only a label wider than the whole card is still cut with an ellipsis.
            SEE-ALSO: apps/mobile/app/src/chat/native/cards/NoticeCard.tsx
            */
            let basis = collapsed.then(|| {
                shown
                    .iter()
                    .zip(&shortcuts)
                    .map(|(choice, shortcut)| {
                        Self::choice_row_width(
                            &text(choice, "collapsedLabel"),
                            shortcut.as_deref(),
                            p,
                            window,
                        )
                    })
                    .fold(0.0_f32, f32::max)
            });
            if collapsed {
                rows = rows.flex_wrap();
            }
            let more_key = format!("{key}:more");
            let more_open = self.expanded.contains(&more_key);
            for (choice, shortcut) in shown.iter().copied().zip(shortcuts.clone()) {
                if numbered {
                    if choice["folded"] == true && !more_open {
                        continue;
                    }
                    rows = rows.child(self.dialog_choice(
                        format!("notice-choice:{}", choice["index"]),
                        choice,
                        shortcut,
                        snapshot["questionCard"]["busy"] == true,
                        rate_limit,
                        Some(json!({"type":"answer","answer":choice["answer"]})),
                        None,
                        p,
                        cx,
                    ));
                    continue;
                }
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
                        .when_some(basis, |row, basis| row.flex_grow(1.0).flex_basis(px(basis)))
                        .child(row),
                );
            }
            if numbered && notice["more"].is_object() && !more_open {
                let toggle_key = more_key.clone();
                rows = rows.child(self.dialog_choice(
                    "notice-choice:more".to_string(),
                    &json!({"title": notice["more"]["label"], "detail": notice["more"]["detail"]}),
                    None,
                    false,
                    rate_limit,
                    None,
                    Some(Box::new(move |this: &mut Self, cx: &mut Context<Self>| {
                        this.expanded.insert(toggle_key.clone());
                        cx.notify();
                    })),
                    p,
                    cx,
                ));
            }
            body.push(rows.into_any_element());
        }
        if !collapsed && !rate_limit && !panel && notice["dialog"].is_object() {
            let (dialog_body, dialog_actions) =
                self.render_terminal_dialog(&notice["dialog"], p, window, cx);
            body.extend(dialog_body);
            footer_end.extend(dialog_actions);
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
                        // A quiet text toggle, and the output on the card's own row surface (Q682): the black slab on the chat background read as a hole in the card.
                        .flex()
                        .items_center()
                        .self_start()
                        .gap(px(4.0 * p.scale))
                        .chat_cursor_pointer()
                        .h(px(22.0 * p.scale))
                        .text_size(px(13.0 * p.scale))
                        .text_color(p.muted)
                        .hover(|style| style.text_color(p.foreground))
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
                                .border_color(p.control_border)
                                .bg(p.card_row)
                                .p(px(12.0 * p.scale))
                                .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                                .text_size(px(12.0 * p.scale))
                                .line_height(px(17.0 * p.scale))
                                .text_color(p.card_muted)
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
                self.tone_button(
                    "notice-switch-account",
                    "Switch account",
                    Some("titlebar/switch-horizontal.svg"),
                    ToneButton::Primary,
                    None,
                    false,
                    p,
                )
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
                    self.tone_button(
                        "notice-terminal-view",
                        "Terminal view",
                        Some("titlebar/terminal-2.svg"),
                        ToneButton::Secondary,
                        None,
                        false,
                        p,
                    )
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                        this.perform_composer_action("terminalView", event.position(), window, cx)
                    }))
                    .into_any_element(),
                );
            } else {
                // The first action the chat can answer itself is the card's one filled button
                // when no account switch already took that place.
                let kind = if actions.is_empty() {
                    ToneButton::Primary
                } else {
                    ToneButton::Secondary
                };
                let answer = json!({"type":"answer","answer":action["answer"].clone()});
                actions.push(
                    self.tone_button(
                        gpui::SharedString::from(format!("notice-action:{}", text(action, "id"))),
                        &text(action, "label"),
                        None,
                        kind,
                        None,
                        false,
                        p,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.invoke(answer.clone(), cx)))
                    .into_any_element(),
                );
            }
        }
        // A card with rows or a dialog is answered, not closed, until an answer on it is refused
        // (`dismiss_notice` in the chat core's questions/actions.rs).
        if (notice["choices"].is_null() && notice["dialog"].is_null())
            || snapshot["noticeError"].is_string()
        {
            footer_end.push(
                self.tone_button(
                    "dismiss-notice",
                    "Dismiss",
                    None,
                    ToneButton::Secondary,
                    None,
                    false,
                    p,
                )
                .on_click(
                    cx.listener(|this, _, _, cx| this.invoke(json!({"type":"dismissNotice"}), cx)),
                )
                .into_any_element(),
            );
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
        let press: Option<Box<dyn FnOnce(gpui::Div) -> gpui::Stateful<gpui::Div>>> =
            foldable.then(|| {
                let toggle = cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                    if !this.expanded.remove(&key) {
                        this.expanded.insert(key.clone());
                    }
                    cx.notify();
                });
                Box::new(move |row: gpui::Div| {
                    row.id("notice-header")
                        .tab_index(0)
                        .role(gpui::Role::Button)
                        .aria_label(if collapsed {
                            "Show all options"
                        } else {
                            "Show less"
                        })
                        .chat_cursor_pointer()
                        .on_click(toggle)
                }) as Box<dyn FnOnce(gpui::Div) -> gpui::Stateful<gpui::Div>>
            });
        let trailing = if foldable {
            vec![
                gpui::svg()
                    .path(if collapsed {
                        "titlebar/chevron-right.svg"
                    } else {
                        "titlebar/chevron-down.svg"
                    })
                    .size(px(14.0 * p.scale))
                    .text_color(p.muted)
                    .flex_shrink_0()
                    .into_any_element(),
            ]
        } else {
            Vec::new()
        };
        Some(self.tone_card(
            ToneCardParts {
                look: tone_look(&text(notice, "tone"), p),
                title: if approval {
                    text(&notice["approval"], "title")
                } else {
                    Some(text(&notice["dialog"]["presentation"], "title"))
                        .filter(|title| panel && !title.is_empty())
                        .unwrap_or_else(|| text(notice, "title"))
                },
                detail: text(&notice["approval"], "source"),
                trailing,
                body,
                footer_start: Vec::new(),
                footer_note: None,
                // The notice's own actions sit at the footer's right end, ahead of its dialog
                // buttons and Dismiss.
                footer_end: actions.into_iter().chain(footer_end).collect(),
            },
            Some(ToneBodyMotion {
                key: &body_key,
                frame: motion,
                shut: collapsed,
            }),
            press,
            p,
        ))
    }
}
