//! The transcript's right-click menu: reference rows on a pill or link, then Copy, Add to Chat and
//! Reply.

use super::message_actions::MESSAGE_MENU_TRIGGER;
use super::state::NativeChatView;
use gpui::{Context, MouseDownEvent, Pixels, Point, Window};
use serde_json::{Value, json};
use std::time::Duration;

/// Toggle key in `menu_toggle.rs`: a second right press while the menu is up only closes it.
const TRANSCRIPT_MENU_TRIGGER: &str = "chat-transcript-menu";

impl NativeChatView {
    /// Open the transcript menu at `at`, for the reference `href` when the press landed on one.
    ///
    /// CDXC:SessionChat 2026-09-19 SEE-ALSO:
    /// The rows come from the core (`packages/gx-chat-core/src/composer/transcript_menu.rs`).
    /// React's subagent dialog sat outside its transcript menu's trigger and suppressed the
    /// browser menu, so the subagent viewer does not open this one either.
    ///
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// The window selection is read here, when the menu opens, as the same trimmed text Cmd+C
    /// copies (`Root::on_action_copy`). gpui-component's selection controller only answers left
    /// presses, so a right press anywhere in the transcript keeps the selection it finds.
    pub(super) fn show_transcript_menu(
        &mut self,
        href: Option<String>,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let selection = gpui_component::TextSelection::selected_text(window, cx)
            .trim()
            .to_owned();
        self.show_transcript_menu_for(href, selection, at, window, cx)
    }

    /// The transcript menu for `text` rather than the window's selection: Copy, Add to Chat and
    /// Reply take it whole (a side question's answer, side_question.rs, the phone's long press).
    pub(super) fn show_text_menu(
        &mut self,
        text: String,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.show_transcript_menu_for(None, text.trim().to_owned(), at, window, cx)
    }

    /// The core's transcript menu rows for `selection` (and `href`, the reference it was opened
    /// on): its reference rows, then Copy, Add to Chat and Reply.
    pub(super) fn transcript_menu_rows(&self, href: Option<String>, selection: &str) -> Vec<Value> {
        // The composer hides behind a question card (composer.rs), which is when Add to Chat is disabled.
        let question_active = self.snapshot["questionCard"]["visible"] == true
            && self.snapshot["prompt"]["kind"] == "question";
        self.runtime
            .as_ref()
            .and_then(|runtime| {
                runtime.query_for_gesture(
                    "transcriptMenu",
                    vec![
                        json!({"href":href,"selection":selection,"questionActive":question_active}),
                    ],
                    Duration::from_millis(250),
                )
            })
            .and_then(|rows| rows.as_array().cloned())
            .unwrap_or_default()
    }

    fn show_transcript_menu_for(
        &mut self,
        href: Option<String>,
        selection: String,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut rows = self.transcript_menu_rows(href, &selection);
        self.keep_available_file_rows(&mut rows);
        // The Reply popover opens where the menu did (reply_popover.rs).
        for row in &mut rows {
            if row["command"]["type"] == "replyToSelection" {
                row["command"]["at"] = json!([f32::from(at.x), f32::from(at.y)]);
            }
        }
        if rows.is_empty() {
            return false;
        }
        if !self.chat_menu_toggled_shut(TRANSCRIPT_MENU_TRIGGER, cx) {
            self.show_chat_menu_at(rows, at, 240.0, window, cx);
        }
        true
    }

    /// A right press on the main transcript that no pill, link or control claimed first.
    pub(super) fn transcript_secondary_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_transcript_menu(None, event.position, window, cx) {
            cx.stop_propagation();
        }
    }

    /// A right press inside a prompt or a final reply that no pill, link or control claimed first:
    /// the message's own ⋯ menu (`rows`) at the pointer. With text selected the transcript menu's
    /// Copy, Add to Chat and Reply come first, then a separator, then the message's rows.
    ///
    /// CDXC:SessionChat 2026-11-19 DECISION:
    /// User: "pls make this menu appear when i right click in the middle of a message of mine; also same idea for ai's messages". The ⋯ button's menu opens from a right press anywhere in the message, on the same toggle key as the button, so the ⋯ stays shown while it is up and a second right press closes it.
    pub(super) fn message_secondary_press(
        &mut self,
        id: &str,
        rows: Vec<Value>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if rows.is_empty() {
            return;
        }
        let at = event.position;
        let selection = gpui_component::TextSelection::selected_text(window, cx)
            .trim()
            .to_owned();
        let mut menu = if selection.is_empty() || !self.in_main_transcript(at) {
            Vec::new()
        } else {
            self.transcript_menu_rows(None, &selection)
        };
        // The Reply popover opens where the menu did (reply_popover.rs).
        for row in &mut menu {
            if row["command"]["type"] == "replyToSelection" {
                row["command"]["at"] = json!([f32::from(at.x), f32::from(at.y)]);
            }
        }
        if !menu.is_empty() {
            menu.push(json!({"separator":true}));
        }
        menu.extend(rows);
        cx.stop_propagation();
        if self.chat_menu_toggled_shut(MESSAGE_MENU_TRIGGER, cx) {
            return;
        }
        // A chat drawn in one of its own windows (the floating chat) keeps the menu there.
        let in_main = self
            .open_main_window(cx)
            .is_none_or(|main| main == window.window_handle());
        self.message_menu = Some(id.to_owned());
        if in_main {
            self.show_chat_menu_at(menu, at, 240.0, window, cx);
        } else {
            self.show_chat_menu_in_window_at(menu, at, 240.0, window, cx);
        }
    }

    /// Whether a press at `at` landed on the main transcript rather than the subagent viewer or a
    /// card below the list, the only place the transcript menu answers.
    pub(super) fn in_main_transcript(&self, at: Point<Pixels>) -> bool {
        !self.snapshot["subagent"].is_object() && self.list.viewport_bounds().contains(&at)
    }

    /// Add to Chat: the controller appends the quoted selection to the current draft.
    pub(super) fn append_to_draft(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        let draft = self.draft.clone();
        self.invoke(
            json!({"type":"appendToDraft","text":text,"draft":draft}),
            cx,
        );
    }
}
