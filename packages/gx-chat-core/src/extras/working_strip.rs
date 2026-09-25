//! The pinned working strip, ported from
//! `packages/shared/session-chat-controller/working-strip.ts`.
//!
//! CDXC:SessionChat 2026-09-17 SEE-ALSO:
//! Stint words and activity precedence live here; visual dimensions and spark artwork live in
//! packages/gx-chat-core/visual/working-strip.json.

use crate::document::WorkingStrip;
use crate::extras::activity::compute_activity_at;
use crate::extras::working_words::pick_working_word;
use crate::state::{ChatContext, ChatState, WorkingWordState};

/// Re-draws the stint word the way `useState(pick)` plus `useEffect(..., [working])` does.
///
/// The initializer's draw is made once and then immediately replaced whenever the first
/// computation already sees a working session, which is why the two draws come off the context in
/// order, the same two `Math.random()` reads the TypeScript made.
pub fn settle_working_word(word: &mut WorkingWordState, working: bool, context: &ChatContext) {
    let mut slot = 0usize;
    if word.last_working.is_none() {
        // `useState(pickSessionChatWorkingWord)`: the lazy initializer, once per chat.
        word.word = pick_working_word(context.random_units[slot]).to_string();
        slot += 1;
    }
    if word.last_working != Some(working) {
        if working {
            word.word = pick_working_word(context.random_units[slot]).to_string();
        }
        word.last_working = Some(working);
    }
}

/// `computeSessionChatWorkingStrip` plus the `presentation` field `publish` adds to it.
///
/// The word is only shown when nothing more specific is: a live terminal activity (compaction, a
/// running shell) replaces it, because it says what is actually happening.
pub fn working_strip(state: &ChatState, context: &ChatContext, working: bool) -> WorkingStrip {
    let activity = state.session.terminal_activity.clone();
    WorkingStrip {
        label: match (&activity, working) {
            (None, true) => Some(format!("{}\u{2026}", state.extras.working_word.word)),
            _ => None,
        },
        presentation: compute_activity_at(
            activity.as_ref(),
            state.extras.activity_now_ms.unwrap_or(context.now_ms),
            context,
        ),
        activity,
        extra: serde_json::Map::new(),
    }
}
