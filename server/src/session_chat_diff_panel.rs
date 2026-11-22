/*
CDXC:SessionChatTerminalActivity 2026-09-04 DECISION:
User: when Claude Code's diff panel is on the session's screen, detect it and
send `/diff` to get rid of it, but only while the chat view is shown; never
hide it forcefully for someone using the terminal view.

The panel is the diff sidebar of Claude Code's fullscreen TUI (`"tui":
"fullscreen"` in its settings). It is a per-session tab that `/diff` toggles
and that Claude can open on its own, drawn only when the terminal is at least
110 columns wide, which the 200-column resting grid always is while the
terminal client is hidden. Its header row is one of two forms, always ending
with the close glyph, and the conversation may share the row's left part:

    22 files changed +1056 -133                                        ✕
    No changes this session                                            ✕

Ghostex types the command through the same queued session-message path the
auto-title job uses. It does so only when the daemon reports no visible client,
the composer chrome is on screen, and the prompt line is empty, so a draft the
user typed in the terminal is never appended to. A cooldown keeps a probe that
still sees the panel (the command is queued, the next capture is a second
later) from typing it twice.
*/

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const DIFF_PANEL_CLOSE_GLYPH: char = '✕';
const DIFF_PANEL_EMPTY_HEADER: &str = "No changes this session";
const DIFF_PANEL_HIDE_COMMAND: &str = "/diff";
const DIFF_PANEL_HIDE_COOLDOWN: Duration = Duration::from_secs(30);
/// The gap that separates the conversation column from the panel on a shared row.
const DIFF_PANEL_COLUMN_GAP: &str = "   ";
const CLAUDE_PROMPT_MARKER: char = '❯';

/// `22 files changed +1056 -133` or `No changes this session`.
fn is_diff_panel_header(segment: &str) -> bool {
    if segment == DIFF_PANEL_EMPTY_HEADER {
        return true;
    }
    let digits = segment.trim_start_matches(|ch: char| ch.is_ascii_digit());
    digits.len() < segment.len()
        && (digits.starts_with(" file changed") || digits.starts_with(" files changed"))
}

/// The panel's header row is on the screen.
pub(crate) fn claude_diff_panel_on_screen(screen_text: &str) -> bool {
    screen_text.lines().any(|raw| {
        let line = normalize_spaces(&strip_ansi_sgr(raw));
        let text = line.trim();
        let Some(head) = text.strip_suffix(DIFF_PANEL_CLOSE_GLYPH) else {
            return false;
        };
        head.split(DIFF_PANEL_COLUMN_GAP)
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .last()
            .is_some_and(is_diff_panel_header)
    })
}

/// `Some(true)` when the last prompt row is a bare `❯`, `Some(false)` when it
/// carries text, `None` when no prompt row is on screen.
fn prompt_line_is_empty(screen_text: &str) -> Option<bool> {
    screen_text
        .lines()
        .rev()
        .map(|raw| normalize_spaces(&strip_ansi_sgr(raw)))
        .find_map(|line| {
            let rest = line.trim().strip_prefix(CLAUDE_PROMPT_MARKER)?;
            Some(rest.trim().is_empty())
        })
}

fn hide_store() -> &'static Mutex<HashMap<String, Instant>> {
    static STORE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Types `/diff` into the session when its panel is up and nobody is watching
/// the terminal. Safe to call on every probe: it is a no-op inside the cooldown.
pub(crate) fn hide_claude_diff_panel_if_unwatched(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    screen_text: &str,
    composer_ready: bool,
) {
    if !composer_ready
        || !claude_diff_panel_on_screen(screen_text)
        || prompt_line_is_empty(screen_text) != Some(true)
    {
        return;
    }
    let key = format!("{project_id}\u{0}{session_id}");
    {
        let store = hide_store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if store
            .get(&key)
            .is_some_and(|sent_at| sent_at.elapsed() < DIFF_PANEL_HIDE_COOLDOWN)
        {
            return;
        }
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(&session) else {
        return;
    };
    if crate::zmx::zmx_session_has_visible_client(&zmx_name) != Some(false) {
        return;
    }
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert(
        "diagnosticInputSource".to_string(),
        json!("claude-diff-panel-hide"),
    );
    params.insert("submit".to_string(), Value::Bool(true));
    params.insert("text".to_string(), json!(DIFF_PANEL_HIDE_COMMAND));
    let mut store = hide_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    store.insert(key, Instant::now());
    let _ = crate::zmx::dispatch_zmx_session_interaction_endpoint(
        repository,
        "/api/sendSessionMessage",
        &params,
    );
}
