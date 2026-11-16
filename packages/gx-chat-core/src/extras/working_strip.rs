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

/// The word the strip shows while the agent works: Thinking, or the stint's drawn word when the
/// Varied thinking words setting (`chatVariedWorkingWords`) is on.
///
/// CDXC:SessionChat 2026-11-15 DECISION:
/// User: "Instead of ruminating and different words, let's just always say "Thinking..." when it's thinking. (keep other words as setting toggle advanced in chat)". Off by default, an Advanced row in Settings > Chat; the phone follows the computer through the chat read (`variedWorkingWords`). The stint word is still drawn every stint, so turning the setting on mid-turn shows a word at once and the host's random draws are consumed as before. Experience presets do not touch it.
fn stint_word(state: &ChatState) -> &str {
    if state.core.varied_working_words {
        &state.extras.working_word.word
    } else {
        "Thinking"
    }
}

/// `computeSessionChatWorkingStrip` plus the `presentation` field `publish` adds to it.
///
/// The word is only shown when nothing more specific is: a live terminal activity (compaction)
/// replaces it, because it says what is actually happening.
///
/// CDXC:SessionChat 2026-10-18 DECISION:
/// User (2026-10-11): while a turn runs and background tasks are running too, the chat must still show the working indicator; background tasks are an extra, not a replacement. Background tasks say nothing about the turn, so they never take the word's place. They are the Background pill (extras/chat_pills.rs), which since 2026-10-17 rides on the working card (extras/working_card.rs, since 2026-10-18 in its own row right above the composer) and opens its own menu; `extras::document` moves them out of `presentation` into `backgroundTasksPanel`.
pub fn working_strip(state: &ChatState, context: &ChatContext, working: bool) -> WorkingStrip {
    let activity = state.session.terminal_activity.clone();
    let background = activity
        .as_ref()
        .and_then(|activity| activity.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("shells-running");
    let mut presentation = compute_activity_at(
        activity.as_ref(),
        state.extras.activity_now_ms.unwrap_or(context.now_ms),
        context,
    );
    if let Some(tasks) = presentation
        .as_mut()
        .and_then(|presentation| presentation.get_mut("backgroundTasks"))
        .and_then(serde_json::Value::as_object_mut)
    {
        tasks.insert(
            "open".to_string(),
            state.extras.panels.background_tasks_open.into(),
        );
    }
    WorkingStrip {
        label: (working && (activity.is_none() || background)).then(|| {
            format!("{}\u{2026}", stint_word(state))
        }),
        presentation,
        activity,
        extra: serde_json::Map::new(),
    }
}
