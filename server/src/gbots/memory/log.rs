//! The daily log, `memory/log/YYYY-MM-DD.md`: one line per finished turn of each of the bot's
//! chats (the time, the chat's title and the first sentence of what the bot said), written by
//! gxserver when the chat stops working. No model call: it is a record of what the bot did, which
//! the bot reads on demand ("what did I do yesterday?") and the Memory tab shows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use chrono::{Local, NaiveDate};
use serde_json::{json, Map, Value};

use crate::gbots::folder::{bot_folder, MEMORY_DIR};
use crate::paths::GxserverPaths;
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;

const LOG_DIR: &str = "log";
/// A log line's summary keeps at most this many characters.
const SUMMARY_MAX_CHARS: usize = 200;
/// The days a client or `memory log` may ask for at once.
pub(crate) const DAYS_MAX: usize = 14;

/// The reply each chat's newest log line was written for, so a turn that ends twice (a hook and a
/// title both saying so) logs once.
static LOGGED: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn log_dir(folder: &Path) -> PathBuf {
    folder.join(MEMORY_DIR).join(LOG_DIR)
}

/// The first sentence of `text`, on one line, short.
fn first_sentence(text: &str) -> String {
    let line = text
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '-', '*', '>', ' ']))
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    // A numbered list's "1. " is not a sentence.
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let line = match line[digits..].strip_prefix(['.', ')']) {
        Some(rest) if digits > 0 && rest.starts_with(' ') => rest.trim_start(),
        _ => line,
    };
    let mut end = line.len();
    for (index, character) in line.char_indices() {
        if matches!(character, '.' | '!' | '?')
            && line[index + 1..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            end = index + character.len_utf8();
            break;
        }
    }
    let sentence = &line[..end];
    if sentence.chars().count() > SUMMARY_MAX_CHARS {
        let cut: String = sentence.chars().take(SUMMARY_MAX_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        sentence.to_string()
    }
}

/// Adds a line to today's log of the bot in `folder`.
pub(crate) fn append_line(folder: &Path, chat_title: &str, summary: &str) -> std::io::Result<()> {
    let now = Local::now();
    let path = log_dir(folder).join(format!("{}.md", now.format("%Y-%m-%d")));
    let mut text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| format!("# {}\n\n", now.format("%A %-d %B %Y")));
    let (summary, _) = super::redact::redact(summary);
    let title = chat_title.trim();
    let title = if title.is_empty() { "A chat" } else { title };
    let line = if summary.is_empty() {
        format!("- {} · {title}\n", now.format("%H:%M"))
    } else {
        format!("- {} · {title} · {summary}\n", now.format("%H:%M"))
    };
    text.push_str(&line);
    crate::gbots::folder::write_atomic(&path, &text)
}

/// The log of the newest `days` days that have one, newest first: `{date, lines}`.
pub(crate) fn recent(folder: &Path, days: usize) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(log_dir(folder)) else {
        return Vec::new();
    };
    let mut dates: Vec<NaiveDate> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            NaiveDate::parse_from_str(name.strip_suffix(".md")?, "%Y-%m-%d").ok()
        })
        .collect();
    dates.sort_unstable_by(|a, b| b.cmp(a));
    dates
        .into_iter()
        .take(days.clamp(1, DAYS_MAX))
        .map(|date| day(folder, date))
        .collect()
}

/// One day's log: `{date, lines}` (empty when the bot did nothing that day).
pub(crate) fn day(folder: &Path, date: NaiveDate) -> Value {
    let date_text = date.format("%Y-%m-%d").to_string();
    let lines: Vec<String> =
        std::fs::read_to_string(log_dir(folder).join(format!("{date_text}.md")))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.strip_prefix("- ").map(str::to_string))
            .collect();
    json!({ "date": date_text, "lines": lines })
}

fn hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Called on every agent activity change (server/src/notification_feed/producer.rs): when a GBot's
/// chat goes from working to idle, or to the attention a finished turn raises, its log gets a line.
/// The transcript is read on its own thread so the request that reported the change never waits.
pub(crate) fn observe_activity(
    paths: &GxserverPaths,
    previous: &str,
    next: &str,
    activity: &Map<String, Value>,
    session: &Value,
) {
    if previous != "working" || next == "working" {
        return;
    }
    let source = activity
        .get("attentionSource")
        .and_then(Value::as_str)
        .or_else(|| {
            session
                .pointer("/runtimeSettings/agentActivity/attentionSource")
                .and_then(Value::as_str)
        });
    if next == "attention" && source != Some(TURN_COMPLETE_ATTENTION_SOURCE) {
        return;
    }
    let Some((bot_id, _)) = crate::gbots::session::chat_of(session) else {
        return;
    };
    let folder = bot_folder(paths, bot_id);
    if !folder.exists() {
        return;
    }
    let session = session.clone();
    let _ = std::thread::Builder::new()
        .name("gbots-daily-log".to_string())
        .spawn(move || {
            // A chat that stops working before it has said anything (starting up) did nothing to log.
            let Some(reply) = crate::notification_feed::body::last_assistant_message_text(&session)
                .filter(|reply| !reply.trim().is_empty())
            else {
                return;
            };
            let chat = format!(
                "{}:{}",
                session["projectId"].as_str().unwrap_or_default(),
                session["sessionId"].as_str().unwrap_or_default()
            );
            {
                let mut logged = LOGGED
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let reply_hash = hash(&reply);
                if logged.get(&chat) == Some(&reply_hash) {
                    return;
                }
                logged.insert(chat, reply_hash);
            }
            let title = crate::coordinators::session_title_of(&session);
            let _ = append_line(&folder, &title, &first_sentence(&reply));
        });
}
