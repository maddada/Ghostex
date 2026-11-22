use serde_json::{json, Map, Value};

use crate::session_chat_options::{SessionChatContextUsage, SessionChatDetectedOptions};

/// CDXC:AgentProviders 2026-09-08 WHY:
/// Codex persists usage alongside chat messages, so the existing tail and append readers collect stats without another poll or terminal capture.
/// Current context is the latest token snapshot, while response records retain real request usage across estimated post-compaction snapshots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CodexSessionStats {
    pub(crate) status: Map<String, Value>,
    pub(crate) context: Option<SessionChatContextUsage>,
    turn_context_seen: bool,
    last_turn_seen: bool,
}

impl CodexSessionStats {
    pub(crate) fn observe(&mut self, line: &str, reverse: bool) {
        // Large tool outputs cannot contain stats at the envelope level.
        let prefix_end = (0..=line.len().min(180))
            .rev()
            .find(|index| line.is_char_boundary(*index))
            .unwrap_or(0);
        let prefix = &line[..prefix_end];
        if ![
            "session_meta",
            "turn_context",
            "token_count",
            "token_usage_record",
            "task_complete",
            "turn_complete",
        ]
        .iter()
        .any(|kind| prefix.contains(kind))
        {
            return;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(payload) = record.get("payload") else {
            return;
        };
        match record.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                for (source, target) in [
                    ("cli_version", "version"),
                    ("cwd", "currentDir"),
                    ("model_provider", "provider"),
                    ("id", "sessionId"),
                    ("parent_thread_id", "parentThreadId"),
                    ("forked_from_id", "forkedFromId"),
                    ("timestamp", "startedAt"),
                ] {
                    self.text(payload, source, target, reverse);
                }
            }
            Some("turn_context") => {
                if reverse && self.turn_context_seen {
                    return;
                }
                self.turn_context_seen = true;
                for key in ["model", "effort", "currentDir", "approvalPolicy", "sandbox"] {
                    self.status.remove(key);
                }
                for (source, target) in [
                    ("model", "model"),
                    ("effort", "effort"),
                    ("cwd", "currentDir"),
                    ("approval_policy", "approvalPolicy"),
                ] {
                    self.text(payload, source, target, reverse);
                }
                if let Some(mode) = payload
                    .pointer("/sandbox_policy/type")
                    .and_then(Value::as_str)
                {
                    self.put("sandbox", json!(mode), reverse);
                }
            }
            Some("token_usage_record") => {
                if let Some(usage) = token_usage(payload.get("usage")) {
                    self.put("lastRequest", usage, reverse);
                }
                if let Some(usage) = token_usage(payload.get("turn_token_usage")) {
                    self.put("turnTokens", usage, reverse);
                }
                if let Some(usage) = token_usage(payload.get("thread_token_usage")) {
                    self.put("totalTokens", usage, reverse);
                }
            }
            Some("event_msg") => match payload.get("type").and_then(Value::as_str) {
                Some("token_count") => {
                    if let Some(info) = payload.get("info").filter(|v| v.is_object()) {
                        if let Some(used) = info
                            .pointer("/last_token_usage/total_tokens")
                            .and_then(Value::as_u64)
                        {
                            if !reverse || self.context.is_none() {
                                let window = info
                                    .get("model_context_window")
                                    .and_then(Value::as_u64)
                                    .filter(|n| *n > 0);
                                let percentage = window.map(|window| {
                                    // Match Codex's user-controllable context percentage, including its 12k baseline.
                                    if window <= 12_000 {
                                        return 100;
                                    }
                                    let effective = window - 12_000;
                                    let remaining =
                                        effective.saturating_sub(used.saturating_sub(12_000));
                                    100 - ((remaining as f64 / effective as f64 * 100.0).round()
                                        as u32)
                                        .min(100)
                                });
                                self.context = Some(SessionChatContextUsage {
                                    used_tokens: Some(used),
                                    window_size: window,
                                    used_percentage: percentage,
                                });
                            }
                        }
                        if let Some(usage) = token_usage(info.get("total_token_usage")) {
                            self.put("totalTokens", usage, reverse);
                        }
                        // Recomputed context has only total_tokens set; it is not a completed request.
                        if let Some(last) = info.get("last_token_usage") {
                            if last
                                .get("input_tokens")
                                .and_then(Value::as_u64)
                                .unwrap_or(0)
                                > 0
                                || last
                                    .get("output_tokens")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0)
                                    > 0
                            {
                                if let Some(usage) = token_usage(Some(last)) {
                                    self.put("lastRequest", usage, reverse);
                                }
                            }
                        }
                    }
                    if let Some(limits) = payload.get("rate_limits").filter(|v| v.is_object()) {
                        for key in ["primary", "secondary"] {
                            if let Some(window) = limits.get(key).filter(|v| v.is_object()) {
                                let mut mapped = Map::new();
                                for (source, target) in [
                                    ("used_percent", "usedPercentage"),
                                    ("window_minutes", "windowMinutes"),
                                    ("resets_at", "resetsAt"),
                                ] {
                                    if let Some(n) = window.get(source).filter(|v| {
                                        v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0)
                                    }) {
                                        mapped.insert(target.into(), n.clone());
                                    }
                                }
                                self.put(key, Value::Object(mapped), reverse);
                            }
                        }
                        self.text(limits, "plan_type", "plan", reverse);
                        self.text(limits, "limit_name", "limitName", reverse);
                        if let Some(credits) = limits.get("credits").filter(|v| v.is_object()) {
                            let mut mapped = Map::new();
                            for (source, target) in [
                                ("has_credits", "hasCredits"),
                                ("unlimited", "unlimited"),
                                ("balance", "balance"),
                            ] {
                                if let Some(v) = credits
                                    .get(source)
                                    .filter(|v| v.is_boolean() || v.is_string())
                                {
                                    mapped.insert(target.into(), v.clone());
                                }
                            }
                            self.put("credits", Value::Object(mapped), reverse);
                        }
                    }
                }
                Some("task_complete" | "turn_complete") => {
                    if reverse && self.last_turn_seen {
                        return;
                    }
                    self.last_turn_seen = true;
                    self.status.remove("lastTurnDurationMs");
                    self.status.remove("timeToFirstTokenMs");
                    for (source, target) in [
                        ("duration_ms", "lastTurnDurationMs"),
                        ("time_to_first_token_ms", "timeToFirstTokenMs"),
                    ] {
                        if let Some(n) = payload.get(source).and_then(Value::as_u64) {
                            self.put(target, json!(n), reverse);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn text(&mut self, source: &Value, key: &str, target: &str, reverse: bool) {
        if let Some(text) = source
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            self.put(target, json!(text), reverse);
        }
    }

    fn put(&mut self, key: &str, value: Value, reverse: bool) {
        if !reverse || !self.status.contains_key(key) {
            self.status.insert(key.into(), value);
        }
    }

    pub(crate) fn apply(&self, options: &mut Option<SessionChatDetectedOptions>) {
        if self.status.is_empty() && self.context.is_none() {
            return;
        }
        let options =
            options.get_or_insert_with(|| SessionChatDetectedOptions::new(Default::default()));
        options.selection.context_usage = self.context.clone();
        options.selection.codex_status = Some(Value::Object(self.status.clone()));
    }
}

fn token_usage(value: Option<&Value>) -> Option<Value> {
    let value = value?.as_object()?;
    let mut mapped = Map::new();
    for (source, target) in [
        ("input_tokens", "inputTokens"),
        ("cached_input_tokens", "cachedInputTokens"),
        ("cache_write_input_tokens", "cacheWriteInputTokens"),
        ("output_tokens", "outputTokens"),
        ("reasoning_output_tokens", "reasoningOutputTokens"),
        ("total_tokens", "totalTokens"),
    ] {
        if let Some(n) = value.get(source).and_then(Value::as_u64) {
            mapped.insert(target.into(), json!(n));
        }
    }
    (!mapped.is_empty()).then_some(Value::Object(mapped))
}
