//! When the user or Chief of Staff changes what loads into a bot (its memory, its instructions or
//! the shared preferences) while chats of that bot are open, each awake chat gets one short queued
//! line telling it to read its brief again. The line waits in the chat's queue until its turn ends,
//! so nothing is typed into a running turn. A bot's own memory writes send nothing: it already
//! knows.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::domain::DomainRepository;
use crate::gbots::record::Bot;
use crate::server::{queue_session_chat_prompt, AppState};

pub(crate) const CHANGED_LINE: &str =
    "Your memory or instructions changed; run `ghostex gbots brief` before you go on.";
/// A chat told once is not told again for this long: the queued line already makes it read the
/// latest brief.
const QUIET_FOR: Duration = Duration::from_secs(120);

static TOLD: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Queues [`CHANGED_LINE`] into every awake chat of `bot`.
pub(crate) fn tell_open_chats(state: &AppState, repository: &DomainRepository<'_>, bot: &Bot) {
    let Ok(chats) = crate::gbots::session::bot_chats(repository, bot) else {
        return;
    };
    for chat in chats {
        if crate::presentation::session_attributes::effective_lifecycle_state(&chat) != "running" {
            continue;
        }
        let text = |key: &str| chat.get(key).and_then(Value::as_str).unwrap_or_default();
        let key = format!("{}:{}", text("projectId"), text("sessionId"));
        {
            let mut told = TOLD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if told.get(&key).is_some_and(|at| at.elapsed() < QUIET_FOR) {
                continue;
            }
            told.insert(key, Instant::now());
        }
        let _ = queue_session_chat_prompt(
            state,
            text("projectId"),
            text("sessionId"),
            CHANGED_LINE,
            false,
        );
    }
}

/// The same for every bot (the shared preferences changed).
pub(crate) fn tell_every_bot(state: &AppState, repository: &DomainRepository<'_>, bots: &[Bot]) {
    for bot in bots {
        tell_open_chats(state, repository, bot);
    }
}
