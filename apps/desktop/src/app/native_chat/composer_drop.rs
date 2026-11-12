//! Text dropped on the chat box: a session row dragged from the sidebar types its link at the
//! caret (the host finds the text, `gx_store/session_mentions.rs`; the chat only takes it).

use super::state::NativeChatView;
use gpui::{Context, Pixels, Point, Window};

impl NativeChatView {
    /// Whether a drop at `position` (window coordinates) lands on the chat box. A maximized chat
    /// box is drawn in its own window, so the pane under the pointer holds none.
    pub(crate) fn composer_drop_target(&self, position: Point<Pixels>) -> bool {
        if self.maximized_window.is_some() {
            return false;
        }
        let bounds = self.composer_bounds.get();
        bounds.size.width > Pixels::ZERO && bounds.contains(&position)
    }

    /// Rings the chat box while something that would be typed into it is held over it. Returns
    /// whether that changed.
    pub(crate) fn set_composer_drop_hover(&mut self, hover: bool, cx: &mut Context<Self>) -> bool {
        if self.composer_drop_hover == hover {
            return false;
        }
        self.composer_drop_hover = hover;
        self.notify_composer(cx);
        true
    }

    /// Types `text` at the chat box's caret and gives it the keyboard, as typing there would; the
    /// caret keeps its place across blur, and a box that was never focused has it at the draft's
    /// end. A chat whose box is not made yet adds the text to the end of its draft.
    pub(crate) fn insert_composer_drop_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_composer_drop_hover(false, cx);
        if text.is_empty() {
            return;
        }
        self.short_pane_composer_open = true;
        self.expand_composer(true, cx);
        match self.input.clone() {
            Some(input) => input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.insert(text.to_string(), window, cx);
            }),
            None => {
                let separator =
                    if self.draft.is_empty() || self.draft.ends_with(char::is_whitespace) {
                        ""
                    } else {
                        " "
                    };
                let next = format!("{}{separator}{text}", self.draft);
                self.replace_draft(&next, true, false, cx);
            }
        }
    }
}
