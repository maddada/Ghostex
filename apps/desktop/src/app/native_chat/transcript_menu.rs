//! The transcript's right-click menu: reference rows on a pill or link, then Copy and Add to Chat.

use super::state::NativeChatView;
use gpui::{Context, MouseDownEvent, Pixels, Point, Window};
use gpui_component::WindowExt as _;
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
        let selection = window.selected_text(cx).trim().to_owned();
        // The composer hides behind a question card (composer.rs), which is when Add to Chat is disabled.
        let question_active = self.snapshot["questionCard"]["visible"] == true
            && self.snapshot["prompt"]["kind"] == "question";
        let rows: Vec<Value> = self
            .runtime
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
            .unwrap_or_default();
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
