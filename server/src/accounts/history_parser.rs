use chrono::{DateTime, Local, NaiveDate};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::model::Provider;

#[derive(Clone)]
pub(super) struct Entry {
    pub key: String,
    pub day: NaiveDate,
    pub tokens: u64,
    pub sidechain: bool,
}

#[derive(Default)]
pub(super) struct Parser {
    session: Option<String>,
    saw_meta: bool,
    replay: bool,
    created: Option<i64>,
    previous: Option<u64>,
    detailed: bool,
}

fn stamp(value: &Value) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc3339(value.as_str()?).ok()
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(0)
}

fn codex_tokens(value: &Value) -> u64 {
    // Cached input and reasoning output are subsets, not additional tokens.
    number(value, "input_tokens").saturating_add(number(value, "output_tokens"))
}

fn present(value: &Value) -> bool {
    !value.is_null() && value.as_str().is_none_or(|s| !s.trim().is_empty())
}

impl Parser {
    pub fn relevant(provider: Provider, line: &[u8]) -> bool {
        let prefix = &line[..line.len().min(220)];
        match provider {
            Provider::Claude => line.windows(7).any(|w| w == b"\"usage\""),
            Provider::Codex => [
                b"session_meta".as_slice(),
                b"task_started",
                b"token_count",
                b"token_usage_record",
            ]
            .iter()
            .any(|s| prefix.windows(s.len()).any(|w| w == *s)),
        }
    }

    /// CDXC:AgentProviders 2026-09-11 WHY:
    /// Shared homes and copied conversations repeat usage records. Claude message IDs and Codex response IDs deduplicate them across files; cumulative counters deduplicate older Codex logs.
    /// Codex forks replay parent counters with new timestamps, so those counters seed the baseline until the child's first live task. Detailed response records replace the subsequent summary counters.
    pub fn observe(&mut self, provider: Provider, line: &[u8], file: &str) -> Option<Entry> {
        // Most transcript bytes are tool output. Avoid parsing their JSON trees.
        if !Self::relevant(provider, line) {
            return None;
        }
        let record: Value = serde_json::from_slice(line).ok()?;
        let timestamp = stamp(&record["timestamp"]);
        if provider == Provider::Claude {
            if record["type"] != "assistant" || record["message"]["model"] == "<synthetic>" {
                return None;
            }
            let usage = &record["message"]["usage"];
            let tokens = [
                "input_tokens",
                "output_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            ]
            .iter()
            .fold(0u64, |sum, key| sum.saturating_add(number(usage, key)));
            if tokens == 0 {
                return None;
            }
            let key = record["message"]["id"]
                .as_str()
                .or_else(|| record["uuid"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{:x}", Sha256::digest(line)));
            return Some(Entry {
                key,
                day: timestamp?.with_timezone(&Local).date_naive(),
                tokens,
                sidechain: record["isSidechain"] == true,
            });
        }
        let payload = &record["payload"];
        let kind = record["type"].as_str()?;
        if kind == "session_meta" && !self.saw_meta {
            self.saw_meta = true;
            self.session = payload["id"].as_str().map(str::to_owned);
            self.created = timestamp.map(|t| t.timestamp());
            self.replay = present(&payload["forked_from_id"])
                || present(&payload["parent_thread_id"])
                || payload["thread_source"] == "subagent"
                || present(&payload["source"]["subagent"]);
            return None;
        }
        if kind == "event_msg" && payload["type"] == "task_started" {
            if let (Some(started), Some(created)) = (
                payload["started_at"].as_f64(),
                self.created.or_else(|| timestamp.map(|t| t.timestamp())),
            ) {
                if started >= created as f64 {
                    self.replay = false;
                }
            }
            return None;
        }
        let (key, tokens) = if kind == "token_usage_record" {
            self.detailed = true;
            if self.replay {
                return None;
            }
            let response = payload["response_id"].as_str()?;
            (
                format!("response:{response}"),
                codex_tokens(&payload["usage"]),
            )
        } else if kind == "event_msg" && payload["type"] == "token_count" && !self.detailed {
            let info = &payload["info"];
            if !info.is_object() {
                return None;
            }
            let totals = info["total_token_usage"]
                .as_object()
                .map(|_| codex_tokens(&info["total_token_usage"]));
            let previous = self.previous;
            if let Some(total) = totals {
                self.previous = Some(total);
            }
            if self.replay || totals.is_some_and(|t| Some(t) == previous) {
                return None;
            }
            let last = &info["last_token_usage"];
            let tokens = if last.is_object() {
                codex_tokens(last)
            } else {
                totals?.saturating_sub(previous.unwrap_or(0))
            };
            let session = self.session.as_deref().unwrap_or(file);
            let snapshot = totals
                .map(|t| t.to_string())
                .unwrap_or_else(|| format!("{:x}", Sha256::digest(line)));
            (format!("{session}:{snapshot}"), tokens)
        } else {
            return None;
        };
        if tokens == 0 {
            return None;
        }
        Some(Entry {
            key,
            day: timestamp?.with_timezone(&Local).date_naive(),
            tokens,
            sidechain: false,
        })
    }
}
