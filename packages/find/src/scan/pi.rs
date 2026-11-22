use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // pi
    // -----------------------------------------------------------------------

    pub(super) fn scan_pi(&mut self) {
        let base = self.path(".pi/agent/sessions");
        self.scan_pi_session_dir(&base, Agent::Pi);
    }

    /// Every `<base>/<project>/*.jsonl` session log, read as `agent`'s. OMP (oh-my-pi) is a Pi
    /// fork that keeps the same session log under `~/.omp/agent/sessions`.
    pub(super) fn scan_pi_session_dir(&mut self, base: &Path, agent: Agent) {
        let mut files: Vec<PathBuf> = Vec::new();
        for sub in read_dir_sorted(base) {
            let Ok(entries) = fs::read_dir(&sub) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    files.push(path);
                }
            }
        }
        files.sort();
        for path in files {
            let fallback_ts = fs::metadata(&path).map(|m| mtime_seconds(&m)).unwrap_or(0);
            if let Some(data) = read_all(&path) {
                self.parse_pi_session_with_last_active(&data, fallback_ts, agent);
            }
        }
    }

    pub fn parse_pi_session(&mut self, data: &[u8]) {
        self.parse_pi_session_with_last_active(data, 0, Agent::Pi);
    }

    fn parse_pi_session_with_last_active(&mut self, data: &[u8], fallback_ts: i64, agent: Agent) {
        let mut cwd = String::new();
        let mut session_id = String::new();
        let mut session_title = String::new();
        let mut meta = Meta::default();
        let mut session_ts = fallback_ts;
        let record_start = self.records.len();

        for line in data.split(|&b| b == b'\n') {
            // Only session headers and user messages yield records. Skip the far
            // larger/more numerous assistant & tool lines before paying for a
            // full JSON parse: pi writes compact JSON with `type` first and the
            // role marker near the start, so bound the scan to the line head.
            // omp 18.11 writes `"runtime":{"version":"18.11.3"}` before `message`, which put the
            // role at byte 138 and dropped every one of its prompts from a 128-byte head.
            let head = &line[..line.len().min(256)];
            let keep = line.starts_with(b"{\"type\":\"session\"")
                || line.starts_with(b"{\"type\":\"title\"")
                || line.starts_with(b"{\"type\":\"model_change\"")
                || line.starts_with(b"{\"type\":\"thinking_level_change\"")
                || contains(head, b"\"role\":\"user\"")
                || contains(head, b"\"role\":\"assistant\"");
            if !keep {
                continue;
            }
            let Some(v) = parse_line(line) else { continue };
            let line_ts = timestamp_from_object(&v);
            let Some(typ) = string_field(&v, "type").map(str::to_string) else {
                continue;
            };

            // OMP opens its log with a padded `title` row it rewrites in place when the session is
            // renamed, so that row outranks the title copied into the session header.
            if typ == "title" {
                if let Some(title) = title_from_object(&v) {
                    session_title = title.clone();
                    self.apply_title_to_session_records(record_start, &title);
                }
                continue;
            }
            if typ == "session" {
                if session_ts == 0 && line_ts > 0 {
                    session_ts = line_ts;
                }
                if let Some(x) = string_field(&v, "cwd") {
                    cwd = x.to_string();
                }
                if let Some(x) = string_field(&v, "id") {
                    session_id = x.to_string();
                }
                if let Some(title) = title_from_object(&v).filter(|_| session_title.is_empty()) {
                    session_title = title.clone();
                    self.apply_title_to_session_records(record_start, &title);
                }
                continue;
            }
            if typ == "model_change" {
                if let Some(x) = string_field(&v, "provider") {
                    meta.provider = x.to_string();
                }
                if let Some(x) = string_field(&v, "modelId") {
                    meta.model = x.to_string();
                }
                // omp 18.11 names the model once, as `provider/id`.
                if let Some((provider, model)) = string_field(&v, "model")
                    .filter(|_| string_field(&v, "modelId").is_none())
                    .and_then(|x| x.split_once('/'))
                {
                    meta.provider = provider.to_string();
                    meta.model = model.to_string();
                }
                self.apply_meta_to_session_records(record_start, &meta);
                continue;
            }
            if typ == "thinking_level_change" {
                if let Some(x) = string_field(&v, "thinkingLevel") {
                    meta.thinking = x.to_string();
                }
                self.apply_meta_to_session_records(record_start, &meta);
                continue;
            }
            if typ != "message" {
                continue;
            }
            let Some(msg) = field(&v, "message") else {
                continue;
            };
            if !msg.is_object() {
                continue;
            }
            let Some(role) = string_field(msg, "role") else {
                continue;
            };
            if role == "assistant" {
                if let Some(x) = string_field(msg, "provider") {
                    meta.provider = x.to_string();
                }
                if let Some(x) = string_field(msg, "model") {
                    meta.model = x.to_string();
                }
                if let Some(u) = field(msg, "usage") {
                    if u.is_object() {
                        meta.usage.input += int_val(field(u, "input"));
                        meta.usage.output += int_val(field(u, "output"));
                        meta.usage.cache_read += int_val(field(u, "cacheRead"));
                        meta.usage.cache_write += int_val(field(u, "cacheWrite"));
                        meta.usage.total += int_val(field(u, "totalTokens"));
                        if let Some(cost) = field(u, "cost") {
                            if cost.is_object() {
                                meta.usage.cost += float_val(field(cost, "total"));
                            }
                        }
                    }
                }
                self.apply_meta_to_session_records(record_start, &meta);
                continue;
            }
            if role != "user" {
                continue;
            }
            let Some(content) = field(msg, "content") else {
                continue;
            };
            let Some(text) = content_text(content) else {
                continue;
            };
            let Some(text) = visible_user_prompt(&text) else {
                continue;
            };
            self.records.push(Record {
                agent,
                kind: RecordKind::User,
                interrupted: false,
                title: session_title.clone(),
                text,
                project: cwd.clone(),
                session: session_id.clone(),
                ts: if session_ts > 0 { session_ts } else { line_ts },
                meta: meta.clone(),
            });
        }
    }
}
