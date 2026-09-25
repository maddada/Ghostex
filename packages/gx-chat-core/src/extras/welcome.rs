//! The new-session welcome and the empty region's copy, ported from
//! `packages/shared/session-chat-presentation/new-session-welcome.ts` and
//! `packages/core-ui/chat/session-chat-empty-state.ts`.
//!
//! CDXC:SessionChat 2026-09-18 WHY:
//! React owned this decision inline in session-chat-view.tsx, so the GPUI chat had no way to reach
//! it and fell through to the `emptyState` copy: a brand new session greeted the user with
//! "Loading conversation… / Reading the agent transcript." forever instead of the welcome.
//! SEE-ALSO: apps/desktop/src/app/native_chat/new_session_welcome.rs.

use crate::document::EmptyState;
use crate::extras::agent_tasks::js_trim;
use crate::extras::agents::{agent_icon_id, default_agent_by_id, is_sidebar_agent_icon};

/// How long a transcript read stays `blank` before it reports the `indicator` stage.
///
/// CDXC:SessionChat 2026-09-19 DECISION:
/// User: the skeleton shows the moment a transcript starts loading, in React chat; the pane must
/// react at once instead of holding blank. This supersedes the 600ms blank hold from the same day.
/// The GPUI chat draws no skeleton for either stage since 2026-09-24 and fades the transcript in
/// once it is ready (apps/desktop/src/app/native_chat/transcript_reveal.rs).
pub const LOADING_INDICATOR_DELAY_MS: i64 = 0;
/// How long a transcript read runs before the empty region offers Retry.
pub const LOADING_RETRY_DELAY_MS: i64 = 12_000;

/// `sessionChatShowsNewSessionWelcome`: a new agent reports `starting` until its first transcript
/// file exists, and `empty` once the file is there but still has no turns. The welcome owns both.
pub fn shows_new_session_welcome(view_kind: &str) -> bool {
    view_kind == "starting" || view_kind == "empty"
}

/// `sessionChatWelcomeAgentName`: the agent's display name, `claude-code` to `Claude Code`.
pub fn welcome_agent_name(agent_label: Option<&str>) -> Option<String> {
    let normalized = js_trim(agent_label?);
    if normalized.is_empty() {
        return None;
    }
    if let Some((_, name)) = default_agent_by_id(Some(normalized)) {
        return Some(name.to_string());
    }
    Some(title_case_words(&replace_separators(normalized)))
}

/// `sessionChatWelcomeAgentIcon`: a draft's own agent row wins, then the default table, then the
/// transcript family's artwork.
///
/// A project custom agent has no entry in the default agent table, so only the daemon's list knows
/// its artwork. The read state's label is the transcript family id, which is not always the sidebar
/// agent id the artwork is registered under, hence the family fallback.
pub fn welcome_agent_icon(agent_label: Option<&str>, agent_icon: Option<&str>) -> Option<String> {
    let default_icon = agent_label.and_then(|label| default_agent_by_id(Some(label)));
    let family_icon = agent_icon_id(agent_label).filter(|icon| is_sidebar_agent_icon(Some(icon)));
    if is_sidebar_agent_icon(agent_icon) {
        return agent_icon.map(str::to_string);
    }
    default_icon
        .map(|(icon, _)| icon.to_string())
        .or_else(|| family_icon.map(str::to_string))
}

/// `sessionChatNewSessionWelcomeTitle`: the headline, already wrapped.
pub fn new_session_welcome_title(agent_name: Option<&str>) -> String {
    let title = match agent_name {
        Some(name) if !name.is_empty() => format!("What should we build with {name}?"),
        _ => "What should we work on?".to_string(),
    };
    wrap_new_session_welcome_title(&title)
}

/// CDXC:SessionChat 2026-09-20 DECISION:
/// User: when chat is very narrow, the welcome title wraps, is center aligned, and the second line
/// has 2 or 3 words, never 1. A 6+ word headline keeps 3 words on the last line ("What should we" /
/// "build with Codex?"); shorter ones keep 2 ("What should we" / "work on?").
/// SEE-ALSO: apps/desktop/src/app/native_chat/new_session_welcome.rs.
pub fn wrap_new_session_welcome_title(title: &str) -> String {
    if title.contains('\n') {
        return title.to_string();
    }
    let words: Vec<&str> = title
        .split(|character: char| character.is_whitespace() || character == '\u{feff}')
        .filter(|word| !word.is_empty())
        .collect();
    if words.len() < 4 {
        return title.to_string();
    }
    let last_count = if words.len() >= 6 { 3 } else { 2 };
    let split = words.len() - last_count;
    format!("{}\n{}", words[..split].join(" "), words[split..].join(" "))
}

/// `sessionChatEmptyStateCopy`: the headline and detail an empty transcript shows.
pub fn empty_state_copy(kind: &str, agent_label: Option<&str>) -> EmptyState {
    match kind {
        "empty" => {
            let agent = agent_label
                .map(js_trim)
                .filter(|label| !label.is_empty())
                .unwrap_or("the agent");
            EmptyState {
                title: format!("Start a chat with {agent}"),
                detail: format!("Ask {agent} to inspect code, explain output, or make a change."),
            }
        }
        "error" => EmptyState {
            title: "Could not load conversation".to_string(),
            detail:
                "The transcript could not be read. Toggle back to the terminal to keep working."
                    .to_string(),
        },
        "unsupported" => EmptyState {
            title: "No conversation here".to_string(),
            detail: "This terminal is not running a recognized coding agent.".to_string(),
        },
        // `starting` falls through to `loading`, and so does anything else the view reports.
        _ => EmptyState {
            title: "Loading conversation\u{2026}".to_string(),
            detail: "Reading the agent transcript.".to_string(),
        },
    }
}

/// `formatSessionChatDuration`: `1h 02m`, `3m` or `45s`.
///
/// CDXC:SessionChat 2026-09-08 WHY:
/// The context row catalog and agent rows both use this formatter; importing it from the catalog
/// created a runtime cycle that prevented chat from loading.
/// It existed twice until 2026-09-22, over two different `Math.round` ports, and the other copy
/// (`menus/context/usage.rs`) is the faithful one. This is now that copy under family f's name,
/// which is where `session-chat-duration.ts` belongs.
pub use crate::menus::context::usage::format_duration;

/// `.replace(/[-_]+/g, ' ')`.
fn replace_separators(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_run = false;
    for character in value.chars() {
        if character == '-' || character == '_' {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(character);
            in_run = false;
        }
    }
    out
}

/// `.replace(/\b\p{L}/gu, letter => letter.toLocaleUpperCase())`: every letter that starts a word.
fn title_case_words(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_is_word = false;
    for character in value.chars() {
        let is_word = character.is_alphanumeric() || character == '_';
        if character.is_alphabetic() && !previous_is_word {
            out.extend(character.to_uppercase());
        } else {
            out.push(character);
        }
        previous_is_word = is_word;
    }
    out
}
