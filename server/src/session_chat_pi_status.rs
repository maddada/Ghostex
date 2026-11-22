/*
CDXC:AgentProviders 2026-10-06 DECISION:
User: Pi gets "Ghostex's own status line for Pi (model, usage, context, like Claude's)". Everything comes from the session's own Pi transcript: the model and thinking level its newest reply ran on, the session's token and cost totals the way Pi's `/session` counts them (every assistant reply, tool results that did model work, `usage` rows, and the summaries compaction and branch switches paid for), and the context the newest reply filled, over the window the model's lineup row reports (`get_available_models`).
SEE-ALSO: packages/gx-chat-core/src/menus/context/ builds the status line rows from `piStatus`, server/src/session_chat_pi_models.rs holds the lineup.
*/

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde_json::{json, Map, Value};

use crate::session_chat::parse_timestamp;

/// One file's status, kept until the file changes: detection reads it about once a second while a
/// session works, and Pi only writes when a message ends.
struct CachedStatus {
    len: u64,
    modified: Option<SystemTime>,
    status: Option<PiTranscriptStatus>,
}

static STATUS_CACHE: Mutex<Option<HashMap<PathBuf, CachedStatus>>> = Mutex::new(None);

/// What one Pi transcript says about its session.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PiTranscriptStatus {
    /// `provider/id` of the model the newest reply ran on, else of the newest `model_change`.
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub started_at: Option<i64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: f64,
    /// Tokens the newest finished reply held in context; `None` before the first reply and after
    /// a compaction until the next one, as Pi's own footer shows `?`.
    pub context_tokens: Option<u64>,
}

/// The transcript's status, read again only when the file changed.
pub(crate) fn read_pi_transcript_status(path: &Path) -> Option<PiTranscriptStatus> {
    let metadata = std::fs::metadata(path).ok()?;
    let (len, modified) = (metadata.len(), metadata.modified().ok());
    if let Ok(cache) = STATUS_CACHE.lock() {
        if let Some(cached) = cache.as_ref().and_then(|cache| cache.get(path)) {
            if cached.len == len && cached.modified == modified {
                return cached.status.clone();
            }
        }
    }
    let status = scan_pi_transcript(path);
    if let Ok(mut cache) = STATUS_CACHE.lock() {
        cache.get_or_insert_with(HashMap::new).insert(
            path.to_path_buf(),
            CachedStatus {
                len,
                modified,
                status: status.clone(),
            },
        );
    }
    status
}

fn scan_pi_transcript(path: &Path) -> Option<PiTranscriptStatus> {
    let file = std::fs::File::open(path).ok()?;
    let mut status = PiTranscriptStatus::default();
    let mut changed_model: Option<String> = None;
    let mut reply_model: Option<String> = None;
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        // Only these rows carry anything the status line shows; the rest are skipped unparsed.
        if !(line.contains("\"usage\"")
            || line.contains("\"model_change\"")
            || line.contains("\"thinking_level_change\"")
            || line.contains("\"compaction\"")
            || line.contains("\"type\":\"session\""))
        {
            continue;
        }
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let text = |record: &Map<String, Value>, key: &str| {
            record
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        match record.get("type").and_then(Value::as_str) {
            Some("session") => status.started_at = parse_timestamp(record.get("timestamp")),
            Some("model_change") => {
                changed_model = match (text(&record, "provider"), text(&record, "modelId")) {
                    (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
                    _ => text(&record, "model"),
                };
            }
            Some("thinking_level_change") => {
                status.thinking_level = text(&record, "thinkingLevel");
            }
            Some("usage") => add_usage(&mut status, record.get("usage")),
            Some("compaction" | "branch_summary") => {
                add_usage(&mut status, record.get("usage"));
                if record.get("type").and_then(Value::as_str) == Some("compaction") {
                    status.context_tokens = None;
                }
            }
            Some("message") => {
                let Some(message) = record.get("message").and_then(Value::as_object) else {
                    continue;
                };
                add_usage(&mut status, message.get("usage"));
                if message.get("role").and_then(Value::as_str) != Some("assistant") {
                    continue;
                }
                if let (Some(provider), Some(model)) =
                    (text(message, "provider"), text(message, "model"))
                {
                    reply_model = Some(format!("{provider}/{model}"));
                }
                if let Some(level) = text(message, "thinkingLevel") {
                    status.thinking_level = Some(level);
                }
                // An aborted or failed reply never reached the model's full context.
                if !matches!(
                    message.get("stopReason").and_then(Value::as_str),
                    Some("aborted" | "error")
                ) {
                    if let Some(tokens) = context_tokens(message.get("usage")) {
                        status.context_tokens = Some(tokens);
                    }
                }
            }
            _ => {}
        }
    }
    // A switch after the newest reply is the model the next reply will run on.
    status.model = match (changed_model, reply_model) {
        (Some(changed), Some(reply)) => Some(if status_model_changed_last(path) {
            changed
        } else {
            reply
        }),
        (changed, reply) => reply.or(changed),
    };
    Some(status)
}

/// Whether the newest model row in the file is a `model_change` rather than an assistant reply.
fn status_model_changed_last(path: &Path) -> bool {
    let Ok(text) = crate::session_chat_options::transcript_tail_text(path) else {
        return false;
    };
    for line in text.lines().rev() {
        if line.contains("\"model_change\"") {
            return true;
        }
        if line.contains("\"role\":\"assistant\"") {
            return false;
        }
    }
    false
}

fn usage_number(usage: &Map<String, Value>, key: &str) -> u64 {
    usage
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|v| v.max(0.0) as u64))
        })
        .unwrap_or(0)
}

fn add_usage(status: &mut PiTranscriptStatus, usage: Option<&Value>) {
    let Some(usage) = usage.and_then(Value::as_object) else {
        return;
    };
    status.input_tokens += usage_number(usage, "input");
    status.output_tokens += usage_number(usage, "output");
    status.cache_read_tokens += usage_number(usage, "cacheRead");
    status.cache_write_tokens += usage_number(usage, "cacheWrite");
    if let Some(total) = usage
        .get("cost")
        .and_then(|cost| cost.get("total"))
        .and_then(Value::as_f64)
        .filter(|total| total.is_finite() && *total > 0.0)
    {
        status.cost_usd += total;
    }
}

/// Pi's own measure of a reply's context: its total, else the sum of its parts.
fn context_tokens(usage: Option<&Value>) -> Option<u64> {
    let usage = usage?.as_object()?;
    let total = usage_number(usage, "totalTokens");
    let total = if total > 0 {
        total
    } else {
        ["input", "output", "cacheRead", "cacheWrite"]
            .into_iter()
            .map(|key| usage_number(usage, key))
            .sum()
    };
    (total > 0).then_some(total)
}

/// The `piStatus` document the chat's status line reads, camelCase and absent-when-absent.
pub(crate) fn pi_status_value(
    status: &PiTranscriptStatus,
    model_label: Option<String>,
    context_window: Option<u64>,
) -> Value {
    let mut value = Map::new();
    let mut put = |key: &str, item: Option<Value>| {
        if let Some(item) = item {
            value.insert(key.to_string(), item);
        }
    };
    put("model", status.model.clone().map(Value::String));
    put("modelName", model_label.map(Value::String));
    put(
        "thinkingLevel",
        status.thinking_level.clone().map(Value::String),
    );
    put("startedAt", status.started_at.map(|ms| json!(ms)));
    let tokens = status.input_tokens
        + status.output_tokens
        + status.cache_read_tokens
        + status.cache_write_tokens;
    if tokens > 0 {
        put("inputTokens", Some(json!(status.input_tokens)));
        put("outputTokens", Some(json!(status.output_tokens)));
        put("cacheReadTokens", Some(json!(status.cache_read_tokens)));
        put("cacheWriteTokens", Some(json!(status.cache_write_tokens)));
    }
    put(
        "costUsd",
        (status.cost_usd > 0.0).then(|| json!(status.cost_usd)),
    );
    put(
        "contextTokens",
        status.context_tokens.map(|tokens| json!(tokens)),
    );
    put("contextWindow", context_window.map(|tokens| json!(tokens)));
    Value::Object(value)
}
