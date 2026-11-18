//! One toggle rule shared by every chat-box trigger that opens a menu, popup or picker.

use super::state::NativeChatView;
use gpui::Context;
use std::time::Duration;
use web_time::Instant;

/// How long after a menu dismissed itself a press on its own trigger still counts as the
/// second half of the very press that dismissed it.
const SAME_PRESS: Duration = Duration::from_millis(250);

/// The trigger ids of the composer's option pills, one per `optionMenus` kind.
pub(in crate::app::native_chat) fn option_pill_trigger_id(kind: &str) -> &'static str {
    match kind {
        "model" => "chat-model-pill",
        "mode" => "chat-mode-pill",
        _ => "chat-options-pill",
    }
}

/// Which trigger owns the menu on screen, and which one owned the menu the last press dismissed.
#[derive(Default)]
pub(in crate::app::native_chat) struct ChatMenuToggle {
    open: Option<&'static str>,
    dismissed: Option<(&'static str, Instant)>,
    pending: Option<&'static str>,
}

impl ChatMenuToggle {
    /// The pointer press that just took the main window dismissed the menu: remember whose it was.
    pub(in crate::app::native_chat) fn note_dismissed(&mut self) {
        self.dismissed = self.open.take().map(|trigger| (trigger, Instant::now()));
    }

    /// The menu closed by a row, by Escape, or because another menu replaced it.
    pub(in crate::app::native_chat) fn note_closed(&mut self) {
        self.open = None;
    }

    /// The menu about to go up belongs to `trigger`, opened by something other than a press on
    /// it (a count opening the Work panel's card under the header's button).
    pub(in crate::app::native_chat) fn arm(&mut self, trigger: &'static str) {
        self.pending = Some(trigger);
    }

    /// A menu is going up now: it belongs to the trigger the guard armed, if any.
    pub(in crate::app::native_chat) fn note_opened(&mut self) {
        self.open = self.pending.take();
    }
}

impl NativeChatView {
    /// True when this press on `trigger` only has to shut the menu the same trigger owns.
    ///
    /// CDXC:SessionChat 2026-09-19 DECISION:
    /// User: "please make clicking again on any of the buttons that trigger showing a menu or a
    /// submenu hide that menu or submenu if it's already shown. ex. switch account button, model
    /// button, effort button, mode button, context button". A chat menu is a native child window
    /// that dismisses itself as soon as the main window takes the press, so the press that closes
    /// it is an outside click and the click that lands on mouse up would find no menu and open a
    /// fresh one. Reading the open state alone therefore cannot toggle. The dismissal records the
    /// trigger it belonged to, and a press on that same trigger within `SAME_PRESS` is treated as
    /// the closing half of that press; pressing a different trigger consumes the record without
    /// matching it, so one click still swaps one menu for another.
    pub(in crate::app::native_chat) fn chat_menu_toggled_shut(
        &mut self,
        trigger: &'static str,
        cx: &mut Context<Self>,
    ) -> bool {
        let dismissed = self
            .menu_toggle
            .dismissed
            .take()
            .is_some_and(|(owner, at)| owner == trigger && at.elapsed() < SAME_PRESS);
        if self.menu_toggle.open == Some(trigger)
            && let Some(menu) = self.option_menu.take()
        {
            self.menu_toggle.open = None;
            menu.update(cx, |menu, cx| menu.close(None, cx));
            cx.notify();
            return true;
        }
        if dismissed {
            return true;
        }
        self.menu_toggle.pending = Some(trigger);
        false
    }

    /// Whether `trigger`'s menu is the one on screen, for its pressed look.
    pub(in crate::app::native_chat) fn chat_menu_is_open(&self, trigger: &'static str) -> bool {
        self.option_menu.is_some() && self.menu_toggle.open == Some(trigger)
    }
}
