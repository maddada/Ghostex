/*
CDXC:SessionChatAgentFleet 2026-08-23:
The sub-agents Claude Code is running, which exist ONLY on its terminal
screen. It paints them as a block pinned below the statusline:

      ⏺ main
      ◯ general-purpose       Trimming unused imports in workspace_ter… 11m 58s · ↓ 155.4k tokens
      ◯ general-purpose (+1)  Launching board_gxserver.rs split           11m 2s · ↓ 76.0k tokens

Nothing about this reaches transcript JSONL, so a chat surface projecting the
transcript cannot show it at all — the user is told "the agent is working"
while three agents are working, and has no way to see what they are doing or
how long they have been at it. That is what this module recovers.

`⏺` is the SELECTION marker, not a status glyph: it rides whichever row the TUI
has selected, so the header wears `◯` the moment a subagent is picked. Every
test here is therefore positional, never glyph-based (see `split_fleet_row`).

The header row is the block's anchor and NOT one of the agents: `main` is the
agent the user is already talking to, and the chat view IS its output. Only the
rows below it are reported.

Two readers share this parse. This one turns the block into the fleet strip;
session_chat_terminal_activity.rs asks only where the block STARTS so it can cut
it off the screen before reading the status line, because a fleet row and a
status line are indistinguishable once the selection marker moves.

What changes and what does not decides the publish rate. `same_fleet` compares
the roster AND the token counters, but never the clocks: a clock moves every
second and would cost a frame per second, while the counters move only when an
agent actually did something. The clocks tick client-side from `detectedAt`
instead, which is why that timestamp is minted with the seconds it belongs to
and never carried forward — pairing a fresh reading with an older anchor would
make the client count the same interval twice.
*/

use crate::session_chat_options::{
    normalize_spaces, session_chat_option_agent, strip_ansi_sgr, SessionChatOptionAgent,
};
use crate::session_chat_terminal_activity::parse_elapsed_seconds;

use serde_json::{json, Map, Value};

/// The one agent Claude never spawns, so its row can only be the block header.
const AGENT_FLEET_MAIN_AGENT: &str = "main";

/// Column separator inside a row. Claude pads name/task/clock into columns, so
/// the gap is two spaces at minimum and grows with alignment.
const FLEET_COLUMN_GAP: &str = "  ";

/// Separator before the trailing token counter.
const FLEET_TOKENS_SEPARATOR: &str = " · ";

/// Opens the `(+1)` Claude paints beside a name; see `split_nested_count`.
const FLEET_NESTED_PREFIX: &str = " (+";

/// One row of the block: a sub-agent Claude is running alongside the main one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatSubAgent {
    /// Agent type as the CLI names it (`general-purpose`).
    pub name: String,
    /// What it is doing, already ellipsized by the terminal that painted it.
    pub task: Option<String>,
    /// Seconds the CLI reports it has been running, only when it painted them.
    pub elapsed_seconds: Option<u64>,
    /// The token counter exactly as painted (`↓ 155.4k tokens`), arrow and all.
    /// Kept whole rather than split into a number: the arrow is the direction
    /// and the CLI already rounded the figure for a narrow column.
    pub tokens: Option<String>,
    /*
    The `(+1)` Claude paints beside a name: further agents running under this
    one, which the block folds into its row instead of listing separately.
    Parsed off the name rather than left in it, because the name column is
    space-padded to align every task and gluing `(+1)` on would both misalign
    that column and make one row's agent type read as a different type.
    Absent when the screen painted no marker; never zero.
    */
    pub nested: Option<u32>,
}

impl SessionChatSubAgent {
    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("name".to_string(), json!(self.name));
        if let Some(task) = self.task.as_ref() {
            map.insert("task".to_string(), json!(task));
        }
        if let Some(elapsed_seconds) = self.elapsed_seconds {
            map.insert("elapsedSeconds".to_string(), json!(elapsed_seconds));
        }
        if let Some(tokens) = self.tokens.as_ref() {
            map.insert("tokens".to_string(), json!(tokens));
        }
        if let Some(nested) = self.nested {
            map.insert("nested".to_string(), json!(nested));
        }
        Value::Object(map)
    }
}

/// A whole block, never empty: no sub-agents means no fleet at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatAgentFleet {
    pub agents: Vec<SessionChatSubAgent>,
    /*
    RFC3339 millis, minted when `elapsed_seconds` above was read off the screen
    and NEVER carried forward from an earlier sample. The client shows
    `elapsedSeconds + (now - detectedAt)`, so the two only agree while they
    describe the same instant: hand it a fresh reading under an older anchor
    and it counts that interval twice. Holding a value still is `same_fleet`'s
    job — it decides whether a sample is published at all — not this field's.
    */
    pub detected_at: String,
}

impl SessionChatAgentFleet {
    fn new(agents: Vec<SessionChatSubAgent>) -> Self {
        Self {
            agents,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    }

    /*
    Two samples of the same roster doing the same things, ignoring ONLY the
    clocks. Clocks tick every second, so counting them would emit a frame per
    second for the life of the fleet; the client interpolates them from
    `detectedAt` instead. The token counters do count: they move when an agent
    actually did something, and a counter frozen at its first reading is worse
    than one that is a couple of seconds behind.
    */
    pub fn same_fleet(&self, other: Option<&SessionChatAgentFleet>) -> bool {
        other.is_some_and(|other| {
            self.agents.len() == other.agents.len()
                && self
                    .agents
                    .iter()
                    .zip(other.agents.iter())
                    .all(|(left, right)| {
                        left.name == right.name
                            && left.task == right.task
                            && left.tokens == right.tokens
                            && left.nested == right.nested
                    })
        })
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "agents".to_string(),
            Value::Array(self.agents.iter().map(SessionChatSubAgent::to_value).collect()),
        );
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        Value::Object(map)
    }
}

/// Change test for a value that can also disappear; an omitted field on a frame
/// means CLEARED, so present→absent is a change clients must be told about.
pub fn same_session_chat_agent_fleet(
    current: Option<&SessionChatAgentFleet>,
    published: Option<&SessionChatAgentFleet>,
) -> bool {
    match (current, published) {
        (None, None) => true,
        (Some(current), published) => current.same_fleet(published),
        (None, Some(_)) => false,
    }
}

/*
A block row is one marker glyph, a space, then content. The glyph is only
required to be decoration — no letter, no digit — because which glyph a row
wears depends on the TUI's selection, not on what the row means.
*/
fn split_fleet_row(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let mut chars = trimmed.chars();
    let glyph = chars.next()?;
    if glyph.is_alphanumeric() || glyph.is_whitespace() {
        return None;
    }
    let rest = chars.as_str();
    rest.starts_with(' ').then(|| rest.trim_start())
}

/// The block's first row: the main agent's name and nothing else. A bare `main`
/// cannot be anything else on this screen — a status line always reports work.
fn is_agent_fleet_header(line: &str) -> bool {
    split_fleet_row(line) == Some(AGENT_FLEET_MAIN_AGENT)
}

/*
Index of the header in an already-normalized, blank-stripped screen, or `None`
when no fleet is running. Everything from here to the bottom belongs to the
block: it is pinned below the statusline, so nothing else can follow it.
*/
pub(crate) fn agent_fleet_block_start(lines: &[String]) -> Option<usize> {
    lines.iter().rposition(|line| is_agent_fleet_header(line))
}

/// The screen as this module and the activity detector both want it: SGR gone,
/// exotic whitespace flattened to spaces, blank lines dropped. Column gaps
/// survive on purpose — they are what separates a row's name from its task.
pub(crate) fn normalized_screen_lines(screen_text: &str) -> Vec<String> {
    screen_text
        .lines()
        .map(|raw| normalize_spaces(&strip_ansi_sgr(raw)).trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// `Fixing tool-ro… 12m 36s` → (`Fixing tool-ro…`, 756). Scans for the EARLIEST
/// word boundary whose whole remainder is a clock, so `12m 36s` is taken as one
/// clock rather than just its last word.
fn split_trailing_elapsed(text: &str) -> (&str, Option<u64>) {
    for (start, _) in text.char_indices() {
        if start > 0 && !text[..start].ends_with(' ') {
            continue;
        }
        if let Some(seconds) = parse_elapsed_seconds(&text[start..]) {
            return (text[..start].trim_end(), Some(seconds));
        }
    }
    (text, None)
}

/// `general-purpose (+1)` → (`general-purpose`, 1). Anything that is not a
/// positive integer in that exact shape stays part of the name.
fn split_nested_count(name: &str) -> (&str, Option<u32>) {
    let trimmed = name.trim();
    let Some(open) = trimmed.rfind(FLEET_NESTED_PREFIX) else {
        return (trimmed, None);
    };
    let Some(digits) = trimmed[open + FLEET_NESTED_PREFIX.len()..].strip_suffix(')') else {
        return (trimmed, None);
    };
    match digits.parse::<u32>() {
        Ok(nested) if nested > 0 => (trimmed[..open].trim_end(), Some(nested)),
        _ => (trimmed, None),
    }
}

fn sub_agent_from_row(content: &str) -> Option<SessionChatSubAgent> {
    // The token counter is the last column, kept exactly as it was painted.
    let (columns, tokens) = match content.rsplit_once(FLEET_TOKENS_SEPARATOR) {
        Some((left, counter)) => {
            let counter = counter.trim();
            (left, (!counter.is_empty()).then(|| counter.to_string()))
        }
        None => (content, None),
    };
    // Name first, because the clock must be peeled off the TASK: a row whose
    // task never arrived is all name, and peeling first would empty it.
    let (name, rest) = match columns.split_once(FLEET_COLUMN_GAP) {
        Some((name, rest)) => (name.trim(), rest.trim()),
        None => (columns.trim(), ""),
    };
    let (name, nested) = split_nested_count(name);
    if name.is_empty() {
        return None;
    }
    let (task, elapsed_seconds) = split_trailing_elapsed(rest);
    let task = task.trim();
    Some(SessionChatSubAgent {
        name: name.to_string(),
        task: (!task.is_empty()).then(|| task.to_string()),
        elapsed_seconds,
        tokens,
        nested,
    })
}

/// `Some` while Claude is painting sub-agents. `None` covers both "no
/// fleet" and "a header with nothing under it", which is the block on its way
/// out as the last agent finishes.
pub fn detect_session_chat_agent_fleet(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatAgentFleet> {
    // Claude Code is the only CLI that paints this block; every other agent
    // would only ever false-match.
    if session_chat_option_agent(agent) != Some(SessionChatOptionAgent::Claude) {
        return None;
    }
    let lines = normalized_screen_lines(screen_text);
    let start = agent_fleet_block_start(&lines)?;
    let agents: Vec<SessionChatSubAgent> = lines[start + 1..]
        .iter()
        .map_while(|line| split_fleet_row(line))
        .filter_map(sub_agent_from_row)
        .collect();
    (!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents))
}
