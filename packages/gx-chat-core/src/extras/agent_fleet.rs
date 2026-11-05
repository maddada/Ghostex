//! The Subagents strip, ported from `packages/shared/session-chat-presentation/agent-fleet.ts`.
//!
//! CDXC:AgentScreenDetection 2026-09-18 SEE-ALSO:
//! apps/desktop/src/app/native_chat/agent_fleet.rs renders this projection; the roster's counts,
//! status text, clocks and transcript selectors must not be recomputed in a renderer.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::extras::activity::{activity_elapsed_seconds, format_activity_elapsed};
use crate::extras::time::parse_iso_millis;
use crate::state::ChatContext;

/// One row of the Subagents strip, ready to lay out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFleetRow {
    /// Stable list key.
    pub key: String,
    /// What the subagent transcript viewer is opened with.
    pub selector: String,
    /// Agent type as the CLI names it, also the viewer's `agentType`.
    pub agent_type: String,
    /// The viewer's display name.
    pub name: String,
    pub task: String,
    pub model: String,
    pub effort: String,
    /// Compact model plus effort label shown in the name cell.
    pub model_label: String,
    /// Codex rows show the child name or path here; Claude keeps its task text.
    pub status_text: String,
    pub idle: bool,
    /// An idle child gxserver knows finished its task (its `done` flag), not one that may be woken.
    pub done: bool,
    /// "Done" or "Idle" at the start of the status cell, empty while working or stale.
    pub state_label: String,
    pub working: bool,
    /// Claude rows expose the agent type in the link tooltip; Codex rows do not.
    pub show_agent_type: bool,
    /// The marker that opens the status cell, shown only when the cell carries something.
    pub marker: bool,
    /// Further agents folded into this row, or 0.
    pub nested: i64,
    pub nested_title: String,
    pub tokens: String,
    pub elapsed_label: String,
    /// The separator between the token counter and the clock, shown only with both sides present.
    pub separator: String,
}

/// The strip around those rows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFleetStrip {
    pub stale: bool,
    /// "1 running, 1 idle, 2 done", or just the roster size when the sample is stale.
    pub count_label: String,
    pub rows: Vec<AgentFleetRow>,
    /// A clock is still moving, so the host must re-project once a second.
    pub ticking: bool,
}

/// `subagentModelLabel`: the compact model plus effort label for a subagent row, card or popup.
pub fn subagent_model_label(model: Option<&str>, effort: Option<&str>) -> String {
    let model_part = match model.filter(|value| !value.is_empty()) {
        Some(model) => short_model(model),
        None => "Model not recorded".to_string(),
    };
    let effort_part = match effort.filter(|value| !value.is_empty()) {
        Some(effort) => short_effort(effort),
        None => String::new(),
    };
    [model_part, effort_part]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `sessionChatAgentFleetRows`: the whole strip, or `None` when the roster is absent or empty.
pub fn agent_fleet_rows(
    fleet: Option<&Value>,
    provider: Option<&str>,
    context: &ChatContext,
) -> Option<AgentFleetStrip> {
    let fleet = fleet?.as_object()?;
    let agents: Vec<&Value> = fleet
        .get("agents")
        .and_then(Value::as_array)
        .map(|list| list.iter().collect())
        .unwrap_or_default();
    if agents.is_empty() {
        return None;
    }
    let detected_at = fleet
        .get("detectedAt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let now = context.now_ms;
    let stale = fleet_stale(fleet, now, context.utc_offset_minutes);
    let idle_count = agents.iter().filter(|agent| status_is_idle(agent)).count();
    let done_count = agents.iter().filter(|agent| is_done(agent)).count();
    let running_count = if stale { 0 } else { agents.len() - idle_count };
    // The header says how many are running; the working pulse lives on each row. A stale roster
    // cannot vouch for anything running, so it only carries its size.
    let count_label = if stale {
        agents.len().to_string()
    } else {
        [
            (running_count > 0).then(|| format!("{running_count} running")),
            (idle_count > done_count).then(|| format!("{} idle", idle_count - done_count)),
            (done_count > 0).then(|| format!("{done_count} done")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    };
    // Carry the captured roster, not the ticking display clock, so identical agent types can be
    // resolved against the provider's ordered launch records.
    let detected_millis = parse_iso_millis(detected_at, context.utc_offset_minutes);
    let roster: Vec<Value> = agents
        .iter()
        .map(|agent| {
            let started_at = agent
                .get("startedAt")
                .and_then(Value::as_f64)
                .or_else(|| {
                    let elapsed = agent.get("elapsedSeconds").and_then(Value::as_f64)?;
                    Some(detected_millis.unwrap_or(f64::NAN) - elapsed * 1_000.0)
                })
                .map(number)
                .unwrap_or(Value::Null);
            json!({ "name": agent.get("name").cloned().unwrap_or(Value::Null), "startedAt": started_at })
        })
        .collect();
    let codex = provider == Some("codex");
    // OMP's roster has no transcript viewer yet (server/src/session_chat_omp_fleet.rs), so its rows are
    // plain text: an empty selector leaves the row unlinked.
    let openable = provider != Some("omp");
    let mut ticking = false;
    let rows = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| {
            let agent_name = agent
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let task = agent.get("task").and_then(Value::as_str);
            let status_text = if codex {
                agent_name.to_string()
            } else {
                task.unwrap_or_default().to_string()
            };
            let idle = status_is_idle(agent);
            let done = is_done(agent);
            let working = !stale && !idle;
            let elapsed_seconds = agent.get("elapsedSeconds").and_then(Value::as_f64);
            if working && elapsed_seconds.is_some() {
                ticking = true;
            }
            let elapsed = activity_elapsed_seconds(
                elapsed_seconds,
                detected_at,
                if working {
                    now
                } else {
                    detected_millis.unwrap_or(f64::NAN)
                },
                context.utc_offset_minutes,
            );
            let nested = agent
                .get("nested")
                .and_then(Value::as_f64)
                .map(|value| value as i64)
                .unwrap_or(0);
            let tokens = agent
                .get("tokens")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let id = agent.get("id").and_then(Value::as_str);
            AgentFleetRow {
                key: id
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{index}:{agent_name}")),
                selector: match id {
                    _ if !openable => String::new(),
                    Some(id) => id.to_string(),
                    None => format!("fleet:{}", json!({ "agents": roster, "index": index })),
                },
                agent_type: agent_name.to_string(),
                name: task.unwrap_or(agent_name).to_string(),
                task: task.unwrap_or_default().to_string(),
                model: agent
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                effort: agent
                    .get("effort")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                model_label: subagent_model_label(
                    agent.get("model").and_then(Value::as_str),
                    agent.get("effort").and_then(Value::as_str),
                ),
                marker: !status_text.is_empty() || (idle && !stale) || nested > 0,
                status_text,
                idle,
                done,
                state_label: match (stale, done, idle) {
                    (false, true, _) => "Done",
                    (false, false, true) => "Idle",
                    _ => "",
                }
                .to_string(),
                working,
                show_agent_type: !codex,
                nested,
                nested_title: if nested > 0 {
                    format!(
                        "{nested} more agent{} under this one",
                        if nested == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                },
                separator: if !tokens.is_empty() && elapsed.is_some() {
                    "\u{2022}".to_string()
                } else {
                    String::new()
                },
                tokens,
                elapsed_label: elapsed.map(format_activity_elapsed).unwrap_or_default(),
            }
        })
        .collect();
    Some(AgentFleetStrip {
        stale,
        count_label,
        rows,
        ticking,
    })
}

/// `fleetStale`: the provider said so, or the clock lease has run out or cannot be read.
fn fleet_stale(fleet: &serde_json::Map<String, Value>, now: f64, utc_offset_minutes: i32) -> bool {
    if fleet.get("stale") == Some(&Value::Bool(true)) {
        return true;
    }
    match fleet.get("validUntil").and_then(Value::as_str) {
        // `Date.parse` of an unreadable stamp is `NaN`, which the TypeScript treats as stale.
        Some(stamp) if !stamp.is_empty() => match parse_iso_millis(stamp, utc_offset_minutes) {
            Some(valid_until) => now >= valid_until,
            None => true,
        },
        _ => false,
    }
}

/// `agent.status === 'idle'`.
fn status_is_idle(agent: &Value) -> bool {
    agent.get("status").and_then(Value::as_str) == Some("idle")
}

/// CDXC:SessionStatus 2026-11-04 WHY:
/// gxserver marks a finished child with `done` beside `status: "idle"` rather than a third status, because clients from before the flag count any status but `idle` as running. A `done` on a working child is ignored.
fn is_done(agent: &Value) -> bool {
    status_is_idle(agent) && agent.get("done") == Some(&Value::Bool(true))
}

/// `shortModel`: "Opus 5", "Astra", "GPT 5 Codex".
fn short_model(model: &str) -> String {
    let value = strip_bracketed(model.trim());
    let lowered = value.to_lowercase();
    // `split(/[ -]+/)` keeps the empty part a leading or trailing run of separators produces, so
    // the family index must be taken against exactly that list.
    let parts = split_spaces_and_dashes(&lowered);
    if let Some(family) = parts
        .iter()
        .position(|part| matches!(*part, "opus" | "sonnet" | "haiku" | "fable"))
    {
        let after = version(&parts[family + 1..]);
        let number = if after.is_empty() {
            let before: Vec<&str> = parts[..family]
                .iter()
                .copied()
                .filter(|part| *part != "claude")
                .collect();
            version(&before)
        } else {
            after
        };
        let name = capitalize(parts[family]);
        return if number.is_empty() {
            name
        } else {
            format!("{name} {number}")
        };
    }
    if let Some(named) = named_gpt(&value) {
        return named;
    }
    replace_dashes(&strip_codex(&strip_gpt_prefix(&value)))
}

/// `shortEffort`: `xhigh` keeps its camel hump, everything else is capitalized.
fn short_effort(effort: &str) -> String {
    let value = effort.trim().to_lowercase();
    if value == "xhigh" {
        return "xHigh".to_string();
    }
    if value.is_empty() {
        return String::new();
    }
    capitalize(&value)
}

/// The leading run of `\d{1,2}(\.\d{1,2})?` tokens, at most two, joined with a period.
fn version(tokens: &[&str]) -> String {
    let mut found: Vec<&str> = Vec::new();
    for token in tokens {
        if !is_version_token(token) || found.len() == 2 {
            break;
        }
        found.push(token);
    }
    found.join(".")
}

/// `/^\d{1,2}(?:\.\d{1,2})?$/`.
fn is_version_token(token: &str) -> bool {
    let (head, tail) = match token.split_once('.') {
        Some((head, tail)) => (head, Some(tail)),
        None => (token, None),
    };
    let ok = |part: &str| {
        (1..=2).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    ok(head) && tail.is_none_or(ok)
}

/// `/^gpt-\d+(?:\.\d+)?-(astra|sol|terra|luna)$/i`, capitalized.
fn named_gpt(value: &str) -> Option<String> {
    let lowered = value.to_lowercase();
    let rest = lowered.strip_prefix("gpt-")?;
    let (number, name) = rest.rsplit_once('-')?;
    if !matches!(name, "astra" | "sol" | "terra" | "luna") {
        return None;
    }
    let numeric = match number.split_once('.') {
        Some((head, tail)) => {
            !head.is_empty()
                && head.bytes().all(|byte| byte.is_ascii_digit())
                && !tail.is_empty()
                && tail.bytes().all(|byte| byte.is_ascii_digit())
        }
        None => !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()),
    };
    numeric.then(|| capitalize(name))
}

/// `value.replace(/\[[^\]]*\]/g, '')`.
fn strip_bracketed(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find('[') {
        match rest[open..].find(']') {
            Some(close) => {
                out.push_str(&rest[..open]);
                rest = &rest[open + close + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// Whether `value`'s bytes at `at` spell `needle`, case-insensitively and in ASCII.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// A byte range, not a `&str` slice. A model name is whatever the agent fleet frame carries, and
/// `&value[at..at + 6]` panics when that range ends inside a multi-byte character: `a😀😀` took
/// the whole chat window down on the second emoji. The comparison itself is ASCII either way,
/// because the needle is.
fn matches_ascii_at(value: &str, at: usize, needle: &str) -> bool {
    value
        .as_bytes()
        .get(at..at + needle.len())
        .is_some_and(|slice| slice.eq_ignore_ascii_case(needle.as_bytes()))
}

/// `/^gpt-/i` replaced with `GPT `.
fn strip_gpt_prefix(value: &str) -> String {
    match matches_ascii_at(value, 0, "gpt-") {
        // The four bytes matched ASCII, so byte 4 is a character boundary.
        true => format!("GPT {}", &value[4..]),
        false => value.to_string(),
    }
}

/// `/-codex\b/gi` replaced with ` Codex`.
fn strip_codex(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut characters = value.char_indices();
    while let Some((at, character)) = characters.next() {
        let matched = matches_ascii_at(value, at, "-codex")
            && bytes
                .get(at + 6)
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
        if matched {
            out.push_str(" Codex");
            // `-codex` is six ASCII bytes, so five more characters are consumed with it.
            for _ in 0..5 {
                characters.next();
            }
        } else {
            out.push(character);
        }
    }
    out
}

/// `value.replace(/-/g, ' ')`.
fn replace_dashes(value: &str) -> String {
    value.replace('-', " ")
}

/// `String.prototype.split(/[ -]+/)`, which keeps the empty leading and trailing parts a run of
/// separators at either end produces.
fn split_spaces_and_dashes(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut at = 0;
    let bytes = value.as_bytes();
    while at < bytes.len() {
        if bytes[at] == b' ' || bytes[at] == b'-' {
            parts.push(&value[start..at]);
            while at < bytes.len() && (bytes[at] == b' ' || bytes[at] == b'-') {
                at += 1;
            }
            start = at;
        } else {
            at += 1;
        }
    }
    parts.push(&value[start..]);
    parts
}

/// The first character upper-cased, the rest kept.
fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

/// A JavaScript number: an integral value serializes without a fractional part.
fn number(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        Value::from(value as i64)
    } else if value.is_finite() {
        Value::from(value)
    } else {
        Value::Null
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(agents: Value) -> AgentFleetStrip {
        let context = ChatContext {
            now_ms: 1_000.0,
            ..ChatContext::default()
        };
        agent_fleet_rows(
            Some(&json!({ "agents": agents })),
            Some("opencode"),
            &context,
        )
        .unwrap()
    }

    /// GitHub #403: finished OpenCode children read "Idle" with stopped clocks.
    #[test]
    fn finished_children_count_and_read_as_done() {
        let all_done = strip(json!([
            {"name": "explore", "status": "idle", "done": true},
            {"name": "general", "status": "idle", "done": true},
            {"name": "general", "status": "idle", "done": true},
        ]));
        assert_eq!(all_done.count_label, "3 done");
        assert!(all_done
            .rows
            .iter()
            .all(|row| row.done && row.state_label == "Done"));

        let mixed = strip(json!([
            {"name": "explore", "status": "working"},
            {"name": "general", "status": "idle"},
            {"name": "general", "status": "idle", "done": true},
            {"name": "plan", "status": "idle", "done": true},
        ]));
        assert_eq!(mixed.count_label, "1 running, 1 idle, 2 done");
        let labels: Vec<_> = mixed
            .rows
            .iter()
            .map(|row| row.state_label.as_str())
            .collect();
        assert_eq!(labels, ["", "Idle", "Done", "Done"]);
    }

    #[test]
    fn a_done_flag_on_a_working_child_or_a_stale_roster_shows_nothing() {
        let working = strip(json!([{"name": "explore", "status": "working", "done": true}]));
        assert_eq!(working.count_label, "1 running");
        assert!(!working.rows[0].done);
        assert_eq!(working.rows[0].state_label, "");

        let context = ChatContext::default();
        let stale = agent_fleet_rows(
            Some(&json!({"stale": true, "agents": [{"name": "explore", "status": "idle", "done": true}]})),
            Some("opencode"),
            &context,
        )
        .unwrap();
        assert_eq!(stale.count_label, "1");
        assert_eq!(stale.rows[0].state_label, "");
    }
}
