//! Whether the composer may send, what it says when it may not, and the toast a blocked Send
//! raises.
//!
//! Port of `packages/shared/session-chat-controller/composer-policy.ts` and
//! `packages/shared/session-chat-presentation/send-blocked.ts`.

use serde::{Deserialize, Serialize};

use crate::composer::json::OrderedMap;

/// The 2 s hold on the Stop button after an interrupt, shipped so neither renderer hard-codes it.
pub const STOP_BUTTON_COOLDOWN_MS: u64 = 2_000;

/// What the desktop composer says when nothing is blocking it.
pub const DESKTOP_COMPOSER_PLACEHOLDER: &str =
    "Press Enter to send a message and Tab to Queue.\nUse @ to mention a file and $ for using skills.";

/// What a touch composer (the phone, `StartConfig::touch_composer`) says when nothing is blocking
/// it, the text of the React chat's `MOBILE_SESSION_CHAT_PLACEHOLDER`
/// (`session-chat-composer.tsx`).
pub const TOUCH_COMPOSER_PLACEHOLDER: &str =
    "Tap \u{2191} to send or hold it to queue; use @ for files and $ for skills.";

/// The title of the toast a blocked Send raises.
pub const SEND_BLOCKED_TITLE: &str = "Message not sent";

/// Everything that can stop a send, read off the session state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendGate {
    pub can_send: bool,
    pub account_switch_busy: bool,
    pub conversation_locked: bool,
    pub terminal_choice_pending: bool,
    pub notice_card_visible: bool,
    pub session_option_switching: bool,
}

/// Why sending is blocked, or `None` when it is not.
pub fn send_blocked_reason(state: &SendGate) -> Option<&'static str> {
    if !state.can_send {
        return Some("Input is held by another device.");
    }
    if state.account_switch_busy {
        return Some("Wait for the account switch to complete.");
    }
    if state.conversation_locked {
        return Some(
            "This conversation is open elsewhere. Use Continue here or close it in the other app and retry.",
        );
    }
    if state.terminal_choice_pending {
        return Some(if state.notice_card_visible {
            "Answer the question above first."
        } else {
            "Your answer is still being applied. Try again in a moment."
        });
    }
    if state.session_option_switching {
        return Some("Claude is still switching mode. Try again in a moment.");
    }
    None
}

/// Why a send is refused outright, or `None` when it may go (at once or once the gate clears).
///
/// CDXC:SessionChat 2026-09-24 SEE-ALSO:
/// Port of `sessionChatSendRefusedReason` in `composer-policy.ts`, which holds the user's decision
/// that a block which clears on its own (an answer still being applied, a mode or model switch, an
/// account switch) holds the send instead of refusing it. Only these blocks need the user.
pub fn send_refused_reason(state: &SendGate) -> Option<&'static str> {
    if !state.can_send {
        return Some("Input is held by another device.");
    }
    if state.conversation_locked {
        return Some(
            "This conversation is open elsewhere. Use Continue here or close it in the other app and retry.",
        );
    }
    if state.terminal_choice_pending && state.notice_card_visible {
        return Some("Answer the question above first.");
    }
    None
}

/// What the composer shows instead of its own placeholder, or `None` to keep it.
pub fn composer_placeholder(
    can_send: bool,
    terminal_choice_pending: bool,
    controls_only: bool,
    notice_card_visible: bool,
    session_option_switching: bool,
) -> Option<&'static str> {
    if !can_send {
        return Some("Input is held by another device.");
    }
    if terminal_choice_pending {
        if controls_only {
            return Some("Use the controls above to continue.");
        }
        return Some(if notice_card_visible {
            "Answer the question above to continue."
        } else {
            "Applying your answer\u{2026}"
        });
    }
    if session_option_switching {
        return Some("Switching Claude mode\u{2026}");
    }
    None
}

/// CDXC:SessionChat 2026-09-18 SEE-ALSO:
/// The 2026-09-03 decision, first recorded in
/// `packages/core-ui/chat/session-chat-send-blocked-toast.tsx`, is that a blocked Send raises a red
/// toast naming the reason instead of disabling the composer. GPUI chat emits this request to its
/// native toast host.
pub fn send_blocked_toast_request(reason: &str) -> OrderedMap {
    let description = reason.trim();
    app_toast_request("error", SEND_BLOCKED_TITLE, description)
}

/// `createAppToastRequest` from `packages/shared/app-toast-contract.ts`, with no options.
///
/// The key order is the object literal's: the optional description, then `level`, `title`, `type`.
fn app_toast_request(level: &str, title: &str, description: &str) -> OrderedMap {
    let mut request = OrderedMap::new();
    if let Some(description) = normalize_toast_description(title, description) {
        request.insert("description", description);
    }
    request.insert("level", level);
    request.insert("title", title.trim());
    request.insert("type", "toast");
    request
}

/// A description that only repeats the title is dropped.
fn normalize_toast_description(title: &str, description: &str) -> Option<String> {
    let normalized = description.trim();
    if normalized.is_empty() {
        return None;
    }
    if comparable_toast_text(title) == comparable_toast_text(normalized) {
        return None;
    }
    Some(normalized.to_string())
}

/// `trim`, collapse whitespace runs, drop trailing `.`, `!` and `?`, trim, lowercase.
fn comparable_toast_text(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_end_matches(['.', '!', '?'])
        .trim()
        .to_lowercase()
}
