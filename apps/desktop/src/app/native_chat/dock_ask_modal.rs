//! A dock row's ask as a modal over the chat (work_cards.rs `dock_row`): its mark, its title
//! whole, the work it belongs to, its whole Markdown description and where it stands, then its
//! item pills and threads at the left of its answers (the recommended one starred) and Reply after
//! a divider, or its Open / Reply when it has no answers. The ⋯ in its head holds the row's other
//! actions (Show in chat, Dismiss, Snooze, Copy as Markdown, Mention in chat box), as the row's ⋯
//! does.
//!
//! An answer, Open, Reply, a link to the work or a pick in the ⋯ closes it as it goes, back to the
//! dock; so does Escape, the X or a click beside the card. Enter picks nothing: the modal takes
//! every key but the app's own chords while it is up. It closes by itself when its row leaves
//! the dock (answered elsewhere, dismissed, snoozed).
//!
//! It is drawn inside the chat pane, as the subagent viewer is (subagent_view.rs): off glass on
//! the chat's light dim (`pane_dialog_scrim`) with the chat's own background, under glass on the
//! pane's glass with the composer's frosted wash, the chat behind it not painted (render.rs
//! `covered`), so no transcript shows through the card and nothing dark is laid on the glass.
//!
//! CDXC:Coordinators 2026-11-14 DECISION:
//! User, of an ask row in the dock opening in place: "i want when i click on a thing like this one
//! to open modal instead of expanding like accordion because accordion is annoying pls fix". The
//! phone shows the same ask in a sheet (apps/mobile/app/src/chat/native/cards/WorkCards.tsx
//! `DockRow`).

use super::chat_text_look::{PROSE_LINE_HEIGHT, PROSE_SIZE};
use super::work_cards::{OPENED_ROW_GAP, answers_divider, pill_fit, pill_widths, row_mark};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

/// The command a pick in the modal's ⋯ menu is wrapped in, so the modal closes as it goes
/// (`unwrap_dock_ask_command`, from `invoke`).
const THEN_CLOSE: &str = "dockAskModalThen";

/// The card's widest, and its tallest before its text scrolls.
const MODAL_MAX_WIDTH: f32 = 640.0;
const MODAL_MAX_HEIGHT: f32 = 720.0;
const MODAL_PAD_X: f32 = 18.0;

/// A menu's rows with each command wrapped in `THEN_CLOSE`, a submenu's (Snooze's lengths) too.
fn wrap_menu_rows(rows: Vec<Value>) -> Vec<Value> {
    rows.into_iter()
        .map(|mut row| {
            if row["command"].is_object() {
                row["command"] = json!({ "type": THEN_CLOSE, "command": row["command"].take() });
            }
            if let Some(children) = row["children"].as_array().cloned() {
                row["children"] = Value::Array(wrap_menu_rows(children));
            }
            row
        })
        .collect()
}

impl NativeChatView {
    /// The row the modal shows, as the dock has it now, or `None` when it has left the dock.
    fn dock_ask_row(&self) -> Option<Value> {
        let key = self.board.dock_modal.as_deref()?;
        self.snapshot["workDock"]["rows"]
            .as_array()?
            .iter()
            .find(|row| row["key"].as_str() == Some(key))
            .cloned()
    }

    /// Whether the modal is up (render.rs `covered`).
    pub(super) fn dock_ask_modal_shown(&self) -> bool {
        self.board.dock_modal.is_some() && self.dock_ask_row().is_some()
    }

    pub(super) fn open_dock_ask_modal(
        &mut self,
        key: &str,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.board.dock_modal = Some(key.to_owned());
        self.board.dock_modal_focused = false;
        self.dock_modal_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_dock_ask_modal(&mut self, cx: &mut Context<Self>) {
        if self.board.dock_modal.take().is_some() {
            self.board.dock_modal_focused = false;
            // The chat box takes the keyboard back, as it had it before the modal.
            self.focus_requested = true;
            cx.notify();
        }
    }

    /// A command from the modal's ⋯ menu closes the modal and goes on as the command it wraps;
    /// any other passes as it is.
    pub(super) fn unwrap_dock_ask_command(
        &mut self,
        action: Value,
        cx: &mut Context<Self>,
    ) -> Value {
        if action["type"] != THEN_CLOSE {
            return action;
        }
        self.close_dock_ask_modal(cx);
        action["command"].clone()
    }

    /// `element`, closing the modal once a click on it has done what it does (an answer, Open,
    /// Reply, a link to the work): the press is seen on its way in and the close runs after it.
    fn closes_modal(&self, element: AnyElement, cx: &Context<Self>) -> AnyElement {
        div()
            .flex_shrink_0()
            .capture_any_mouse_up(cx.listener(|_, _: &gpui::MouseUpEvent, window, cx| {
                cx.defer_in(window, |this, _, cx| this.close_dock_ask_modal(cx));
            }))
            .child(element)
            .into_any_element()
    }

    /// A round icon button in the modal's head.
    fn dock_ask_icon_button(
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        p: &ChatAppearance,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(26.0 * s))
            .rounded(px(6.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.08)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(svg().path(icon).size(px(14.0 * s)).text_color(p.card_muted))
    }

    pub(super) fn render_dock_ask_modal(
        &mut self,
        p: &ChatAppearance,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.board.dock_modal.as_ref()?;
        let Some(row) = self.dock_ask_row() else {
            // Its row left the dock: nothing is left to answer.
            self.board.dock_modal = None;
            self.board.dock_modal_focused = false;
            return None;
        };
        if !self.board.dock_modal_focused {
            self.board.dock_modal_focused = true;
            self.dock_modal_focus.focus(window, cx);
        }
        let s = p.scale;
        let glass = crate::app::helpers::window_glass_active_in(window);
        let card_fill = if glass {
            p.composer_background
        } else {
            p.background
        };
        let card_border = if glass { p.composer_border } else { p.border };
        let key = text(&row, "key");
        let (icon, color) = row_mark(&row, p);
        let body = p.strengthened(p.prose);
        let busy = row["busy"] == true;
        let title = text(&row, "title");
        let age = text(&row, "age");

        // The head: mark, title whole, age, the ⋯ and the X (leading on macOS, as the app's
        // dialogs have it: window/round_close_button.rs).
        let menu_row = row.clone();
        let more =
            Self::dock_ask_icon_button("dock-ask-more", "titlebar/dots.svg", "More actions", p)
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        let rows = wrap_menu_rows(this.row_menu(&menu_row, cx));
                        this.show_dock_row_menu(rows, event.position(), window, cx);
                    }),
                );
        let close = Self::dock_ask_icon_button("dock-ask-close", "titlebar/x.svg", "Close", p)
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_dock_ask_modal(cx);
            }));
        let close_on_left = crate::app::window::round_close_button::close_buttons_on_left();
        let mut close = Some(close);
        let mut head = div()
            .flex()
            .flex_shrink_0()
            .items_start()
            .gap(px(10.0 * s))
            .px(px(MODAL_PAD_X * s))
            .pt(px(14.0 * s))
            .pb(px(10.0 * s));
        if close_on_left {
            head = head.children(close.take());
        }
        let title_size = PROSE_SIZE + 2.0;
        let title_line = PROSE_LINE_HEIGHT + 3.0;
        head = head
            .child(
                div()
                    .h(px(title_line * s))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .child(svg().path(icon).size(px(14.0 * s)).text_color(color)),
            )
            .child(
                div()
                    .id("dock-ask-title")
                    .role(gpui::Role::Heading)
                    .flex_1()
                    .min_w_0()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(self.ask_title(
                        &key,
                        &title,
                        &row["titleRuns"],
                        false,
                        body,
                        title_size,
                        title_line,
                        p,
                        cx,
                    )),
            )
            .child(
                div()
                    .h(px(title_line * s))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .text_size(px(PROSE_SIZE * s))
                    .text_color(p.muted)
                    .child(age.to_string()),
            )
            .child(more)
            .children(close);

        // The text: the work it belongs to, the whole description, where it stands. It scrolls
        // when it is taller than the card's room.
        let (thread_chips, other_chips): (Vec<Value>, Vec<Value>) = if !row["waitedBy"].is_array() {
            row["chips"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .partition(|chip| chip["kind"] == "thread")
        } else {
            (Vec::new(), Vec::new())
        };
        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.0 * s))
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s));
        let mut has_text = false;
        if row["waitedBy"].is_array() {
            // An ask's own item, then the other items that wait on its answer: one line. With item
            // pills on the answers row, only its other parts (the steps) are left here.
            let parts: Vec<AnyElement> = self
                .item_line(&key, &row, p, cx)
                .into_iter()
                .map(|part| self.closes_modal(part, cx))
                .collect();
            if !parts.is_empty() {
                has_text = true;
                column = column.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x(px(6.0 * s))
                        .line_height(px(20.0 * s))
                        .children(parts),
                );
            }
        } else {
            let chips: Vec<AnyElement> = other_chips
                .iter()
                .enumerate()
                .map(|(index, chip)| {
                    let part = self.work_card_part(chip, &format!("dock-ask:{key}:{index}"), p, cx);
                    self.closes_modal(part, cx)
                })
                .collect();
            if !chips.is_empty() {
                has_text = true;
                column = column.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x(px(14.0 * s))
                        .gap_y(px(2.0 * s))
                        .children(chips),
                );
            }
        }
        let body_lead = if has_text { OPENED_ROW_GAP - 2.0 } else { 0.0 };
        let detail = text(&row, "body");
        let mut first_body = true;
        if !detail.is_empty() {
            first_body = false;
            column = column.child(div().mt(px(body_lead * s)).child(self.ask_markdown(
                &format!("dock-ask:{key}"),
                &detail,
                p.card_muted,
                PROSE_SIZE,
                PROSE_LINE_HEIGHT,
                p,
                cx,
            )));
        }
        for line in row["detail"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|line| !line.is_empty())
        {
            column = column.child(
                div()
                    .min_w_0()
                    .when(first_body, |line| line.mt(px(body_lead * s)))
                    .whitespace_normal()
                    .text_color(p.card_muted)
                    .child(line.to_string()),
            );
            first_body = false;
        }
        let has_body = has_text || !first_body;

        // The foot: item pills and threads at the left, the answers (never folded: the card has
        // room to wrap them) and Reply after a divider at the right; Open / Reply for a row that
        // asks with no answers.
        let answers: Vec<Value> = row["buttons"].as_array().cloned().unwrap_or_default();
        let action = answers.is_empty() && row["needs"] == true;
        let reply_answer = ((!answers.is_empty() || (action && row["open"].is_object()))
            && row["reply"].is_object())
        .then(|| Self::reply_answer(&row["reply"], &text(&row, "replied"), busy));
        let mut buttons: Vec<AnyElement> = if action {
            self.work_row_action(&key, &row, p, cx)
                .into_iter()
                .collect()
        } else {
            self.work_answer_buttons(&format!("dock-ask:{key}"), &answers, busy, p, cx)
        };
        if let Some(answer) = &reply_answer {
            if !buttons.is_empty() {
                buttons.push(answers_divider(p));
            }
            buttons.extend(self.work_answer_buttons(
                &format!("dock-ask:{key}:reply"),
                std::slice::from_ref(answer),
                busy,
                p,
                cx,
            ));
        }
        let buttons: Vec<AnyElement> = buttons
            .into_iter()
            .map(|button| self.closes_modal(button, cx))
            .collect();
        let pills: Vec<Value> = row["itemPills"].as_array().cloned().unwrap_or_default();
        let mut lead: Vec<AnyElement> = self
            .item_pill_elements(
                &format!("dock-ask:{key}"),
                &pills,
                pill_fit(pill_widths(&pills, p, cx), MODAL_MAX_WIDTH / 2.0),
                p,
                cx,
            )
            .into_iter()
            .map(|pill| self.closes_modal(pill, cx))
            .collect();
        lead.extend(thread_chips.iter().enumerate().map(|(index, chip)| {
            let logo = self.work_thread_logo(chip, &format!("dock-ask:{key}:t{index}"), p, cx);
            self.closes_modal(logo, cx)
        }));
        let has_foot = !buttons.is_empty() || !lead.is_empty();
        let foot = has_foot.then(|| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(10.0 * s))
                .px(px(MODAL_PAD_X * s))
                .py(px(12.0 * s))
                .border_t_1()
                .border_color(card_border)
                .text_size(px(PROSE_SIZE * s))
                .line_height(px(PROSE_LINE_HEIGHT * s))
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .children(lead),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .items_center()
                        .gap(px(6.0 * s))
                        .children(buttons),
                )
        });

        let card = div()
            .id("dock-ask-card")
            .occlude()
            .flex()
            .flex_col()
            .min_h_0()
            .w_full()
            .max_w(px(MODAL_MAX_WIDTH * s))
            .max_h(px(MODAL_MAX_HEIGHT * s))
            .rounded(px(if glass { 16.0 } else { 10.0 } * s))
            .border_1()
            .border_color(card_border)
            .bg(card_fill)
            .text_color(p.primary)
            .overflow_hidden()
            .role(gpui::Role::Dialog)
            .aria_label(title.to_string())
            // A press inside the card is not a press beside it, which closes it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(head)
            .when(has_body, |card| {
                card.child(
                    div()
                        .id("dock-ask-text")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(px((MODAL_PAD_X + 24.0) * s))
                        .pb(px(14.0 * s))
                        .child(column),
                )
            })
            .children(foot);
        Some(
            div()
                .id("dock-ask-overlay")
                .track_focus(&self.dock_modal_focus)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .px(px(16.0 * s))
                .py(px(24.0 * s))
                // Off glass the chat's light dim for dialogs; under glass the pane's glass, the
                // chat behind it not painted (render.rs `covered`).
                .when(!glass, |overlay| {
                    overlay.bg(super::child_window::pane_dialog_scrim(false, p.light))
                })
                .font_family(p.font.clone())
                .on_click(cx.listener(|this, _, _, cx| this.close_dock_ask_modal(cx)))
                .child(card)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_pick_closes_the_modal_snooze_lengths_included() {
        let rows = wrap_menu_rows(vec![
            json!({ "label": "Dismiss", "command": { "type": "dismiss" } }),
            json!({ "label": "Snooze", "children": [
                { "label": "1 hour", "command": { "type": "snooze", "for": 3600 } },
            ] }),
        ]);
        assert_eq!(rows[0]["command"]["type"], THEN_CLOSE);
        assert_eq!(rows[0]["command"]["command"]["type"], "dismiss");
        assert_eq!(rows[1]["children"][0]["command"]["type"], THEN_CLOSE);
        assert_eq!(rows[1]["children"][0]["command"]["command"]["for"], 3600);
        assert!(rows[1]["command"].is_null());
    }
}
