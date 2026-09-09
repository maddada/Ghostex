use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::session_chat::{SessionChatTranscriptAgent, MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FleetRow {
    name: String,
    started_at: Option<i64>,
}

#[derive(Deserialize)]
struct FleetSelection {
    agents: Vec<FleetRow>,
    index: usize,
}

struct Launch {
    id: String,
    name: String,
    started_at: i64,
}

fn timestamp(record: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(record.get("timestamp")?.as_str()?)
        .ok()
        .map(|at| at.timestamp_millis())
}

fn claude_launches(root: &Path) -> anyhow::Result<Vec<Launch>> {
    let mut metadata = HashMap::new();
    for entry in fs::read_dir(root.with_extension("").join("subagents"))? {
        let entry = entry?;
        let filename = entry.file_name();
        let Some(id) = filename
            .to_str()
            .and_then(|name| name.strip_prefix("agent-"))
            .and_then(|name| name.strip_suffix(".meta.json"))
        else {
            continue;
        };
        if !super::safe_agent_id(id) {
            continue;
        }
        let mut bytes = Vec::new();
        File::open(entry.path())?
            .take(64 * 1024)
            .read_to_end(&mut bytes)?;
        let Ok(meta) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if let (Some(tool_id), Some(name)) = (
            meta.get("toolUseId").and_then(Value::as_str),
            meta.get("agentType").and_then(Value::as_str),
        ) {
            metadata.insert(tool_id.to_string(), (id.to_string(), name.to_string()));
        }
    }
    let file = File::open(root)?;
    let length = file.metadata()?.len();
    let mut reader = BufReader::new(file.take(length));
    let mut line = String::new();
    let mut launches: Vec<Launch> = Vec::new();
    while reader
        .by_ref()
        .take(MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES as u64 + 1)
        .read_line(&mut line)?
        != 0
    {
        anyhow::ensure!(
            line.len() <= MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES,
            "A transcript record is too large to resolve the subagent roster."
        );
        let record = serde_json::from_str::<Value>(&line).ok();
        line.clear();
        let Some(record) = record else {
            continue;
        };
        let Some(at) = timestamp(&record) else {
            continue;
        };
        if let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) {
            for block in blocks {
                if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    continue;
                }
                if let Some((id, name)) = block
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| metadata.get(id))
                {
                    if !launches.iter().any(|launch| launch.id == *id) {
                        launches.push(Launch {
                            id: id.clone(),
                            name: name.clone(),
                            started_at: at,
                        });
                    }
                }
            }
        }
        // Async launch acknowledgements locate the actual start after any
        // pre-tool hooks; completed results instead timestamp the END of a run.
        if record
            .pointer("/toolUseResult/status")
            .and_then(Value::as_str)
            == Some("async_launched")
        {
            if let Some(id) = record
                .pointer("/toolUseResult/agentId")
                .and_then(Value::as_str)
            {
                if let Some(launch) = launches.iter_mut().find(|launch| launch.id == id) {
                    launch.started_at = at;
                }
            }
        }
    }
    launches.sort_by_key(|launch| launch.started_at);
    Ok(launches)
}

/// CDXC:SessionChat 2026-09-09 WHY:
/// Claude's fleet repeats agent types such as Explore and paints a changing activity sentence, neither of which identifies a transcript.
/// Match the captured roster's order and start times to the parent's launch records and toolUseId sidecars. Two seconds cover the whole-second clock and screen-capture delay.
/// Prefix/suffix matching preserves launch order for simultaneous same-type spawns; ambiguous matches are refused rather than opening another child's conversation.
/// SEE-ALSO: packages/core-ui/chat/session-chat-agent-fleet-strip.tsx.
pub(super) fn resolve_fleet_selector(
    root: &Path,
    family: SessionChatTranscriptAgent,
    snapshot: &str,
) -> anyhow::Result<String> {
    let selection: FleetSelection = serde_json::from_str(snapshot)?;
    anyhow::ensure!(
        selection.agents.len() <= 128 && selection.index < selection.agents.len(),
        "Invalid subagent roster selection."
    );
    if family == SessionChatTranscriptAgent::Codex {
        return Ok(selection.agents[selection.index].name.clone());
    }
    let launches = claude_launches(root)?;
    let rows = selection.agents.len();
    let count = launches.len();
    let matches = |row: usize, candidate: usize| {
        let row = &selection.agents[row];
        let launch = &launches[candidate];
        row.name == launch.name
            && row
                .started_at
                .is_none_or(|at| at.abs_diff(launch.started_at) <= 2_000)
    };
    let mut prefix = vec![vec![false; count + 1]; rows + 1];
    prefix[0].fill(true);
    for row in 0..rows {
        for candidate in 0..count {
            prefix[row + 1][candidate + 1] =
                prefix[row + 1][candidate] || (prefix[row][candidate] && matches(row, candidate));
        }
    }
    let mut suffix = vec![vec![false; count + 1]; rows + 1];
    suffix[rows].fill(true);
    for row in (0..rows).rev() {
        for candidate in (0..count).rev() {
            suffix[row][candidate] = suffix[row][candidate + 1]
                || (suffix[row + 1][candidate + 1] && matches(row, candidate));
        }
    }
    let candidates: Vec<_> = (0..count)
        .filter(|&candidate| {
            prefix[selection.index][candidate]
                && matches(selection.index, candidate)
                && suffix[selection.index + 1][candidate + 1]
        })
        .collect();
    anyhow::ensure!(!candidates.is_empty(), "This subagent's launch record is not available yet. Close the popup and try the current row again.");
    anyhow::ensure!(candidates.len() == 1, "This roster does not uniquely identify the subagent. Open its transcript from the agent's spawn message.");
    Ok(launches[candidates[0]].id.clone())
}
