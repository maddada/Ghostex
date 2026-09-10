//! CDXC:SessionStatus 2026-09-10 WHY:
//! Child lifecycle records decide whether an agent is working. Claude's current footer also keeps idle resident children visible without projecting them as active work.
//! A periodic fresh observation renews the client clock lease; missing evidence pauses the roster instead of animating historical work indefinitely.

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use serde_json::{json, Map, Value};

const AGENT_FLEET_MAIN_AGENT: &str = "main";

/// One child in the provider-owned roster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatSubAgent {
    /// Provider identity, so repeated names and resumed turns open the exact child.
    pub id: Option<String>,
    pub started_at: i64,
    pub working: bool,
    /// Agent type as the CLI names it (`general-purpose`).
    pub name: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The persisted task description.
    pub task: Option<String>,
    /// Provider elapsed time, measured at detected_at and frozen while idle.
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
        if let Some(id) = &self.id {
            map.insert("id".to_string(), json!(id));
        }
        map.insert("startedAt".to_string(), json!(self.started_at));
        map.insert(
            "status".to_string(),
            json!(if self.working { "working" } else { "idle" }),
        );
        map.insert("name".to_string(), json!(self.name));
        if let Some(model) = &self.model {
            map.insert("model".to_string(), json!(model));
        }
        if let Some(effort) = &self.effort {
            map.insert("effort".to_string(), json!(effort));
        }
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
    pub stale: bool,
    /*
    RFC3339 millis, minted when `elapsed_seconds` above was sampled
    and NEVER carried forward from an earlier sample. The client shows
    `elapsedSeconds + (now - detectedAt)`, so the two only agree while they
    describe the same instant: hand it a fresh reading under an older anchor
    and it counts that interval twice. Holding a value still is `same_fleet`'s
    job — it decides whether a sample is published at all — not this field's.
    */
    pub detected_at: String,
}

impl SessionChatAgentFleet {
    pub(crate) fn new(agents: Vec<SessionChatSubAgent>) -> Self {
        Self {
            agents,
            stale: false,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    }

    /// Publish roster changes immediately and renew unchanged observations every 15 seconds.
    pub fn same_fleet(&self, other: Option<&SessionChatAgentFleet>) -> bool {
        other.is_some_and(|other| {
            self.stale == other.stale
                && (self.stale
                    || crate::session_chat_fleet_transcript::parse_time(&self.detected_at)
                        .zip(crate::session_chat_fleet_transcript::parse_time(
                            &other.detected_at,
                        ))
                        .is_some_and(|(now, previous)| now.saturating_sub(previous) < 15_000))
                && self.agents.len() == other.agents.len()
                && self
                    .agents
                    .iter()
                    .zip(other.agents.iter())
                    .all(|(left, right)| {
                        left.id == right.id
                            && left.started_at == right.started_at
                            && left.working == right.working
                            && left.name == right.name
                            && left.model == right.model
                            && left.effort == right.effort
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
            Value::Array(
                self.agents
                    .iter()
                    .map(SessionChatSubAgent::to_value)
                    .collect(),
            ),
        );
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        map.insert("stale".to_string(), json!(self.stale));
        if let Some(at) = crate::session_chat_fleet_transcript::parse_time(&self.detected_at)
            .and_then(|at| chrono::DateTime::from_timestamp_millis(at + 90_000))
        {
            map.insert(
                "validUntil".to_string(),
                json!(at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            );
        }
        Value::Object(map)
    }

    pub(crate) fn is_working(&self) -> bool {
        !self.stale
            && self.agents.iter().any(|agent| agent.working)
            && crate::session_chat_fleet_transcript::parse_time(&self.detected_at)
                .is_some_and(|at| chrono::Utc::now().timestamp_millis().saturating_sub(at) < 90_000)
    }

    /// CDXC:SessionStatus 2026-09-10 WHY:
    /// An unreadable observation cannot keep asserting that the last roster is running. Preserve its identity for inspection, but pause its clocks and retire the working projection until evidence is readable again.
    pub(crate) fn unavailable(mut self) -> Self {
        self.stale = true;
        self
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
    split_fleet_row(line.trim_start_matches('❯').trim_start()) == Some(AGENT_FLEET_MAIN_AGENT)
}

/*
Index of the header in an already-normalized, blank-stripped screen, or `None`
when no fleet is running. Everything from here to the bottom belongs to the
block: it is pinned below the statusline, so nothing else can follow it.
*/
pub(crate) fn agent_fleet_block_start(lines: &[String]) -> Option<usize> {
    let start = lines.iter().rposition(|line| is_agent_fleet_header(line))?;
    lines[start + 1..]
        .iter()
        .all(|line| split_fleet_row(line.trim_start_matches('❯').trim_start()).is_some())
        .then_some(start)
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
