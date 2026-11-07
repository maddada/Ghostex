use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent::Agent;
use crate::wyhash::Wyhash;

use super::*;

// CDXC:PromptSearch 2026-06-28-07:49:
// Codex session transcripts emit current user prompt blocks as `input_text` as
// well as the older `text` shape. Accept both and keep the derived cache at v5
// so sessions previously cached as zero-record parses are rebuilt.
const CODEX_CACHE_VERSION: i64 = 5;

impl Scanner {
    // -----------------------------------------------------------------------
    // codex
    // -----------------------------------------------------------------------

    pub(super) fn scan_codex(&mut self) {
        self.load_codex_session_index_titles();
        if let Some(data) = read_all(&self.codex_path("history.jsonl")) {
            self.parse_codex_history(&data);
        }
        self.scan_codex_sessions();
        self.apply_codex_session_projects();
    }

    fn load_codex_session_index_titles(&mut self) {
        let Some(data) = read_all(&self.codex_path("session_index.jsonl")) else {
            return;
        };
        for line in data.split(|&b| b == b'\n') {
            let Some(v) = parse_line(line) else { continue };
            let Some(session_id) = string_field(&v, "id") else {
                continue;
            };
            if session_id.is_empty() {
                continue;
            }
            let session_id = session_id.to_string();
            let Some(title) = title_from_object(&v) else {
                continue;
            };
            self.codex_titles.insert(session_id, title);
        }
    }

    fn codex_title_for_session(&self, session_id: &str) -> Option<String> {
        if session_id.is_empty() {
            return None;
        }
        self.codex_titles.get(session_id).cloned()
    }

    fn put_codex_project_if_absent(&mut self, session_id: &str, project: &str) {
        if session_id.is_empty() || project.is_empty() {
            return;
        }
        self.codex_projects
            .entry(session_id.to_string())
            .or_insert_with(|| project.to_string());
    }

    // CDXC:PromptSearch 2026-06-11-10:02:
    // Project-filtered browsing must include Codex history rows for the selected
    // project even when legacy ~/.codex/history.jsonl records omit cwd. Backfill
    // missing Codex projects by session id instead of guessing.
    fn apply_codex_session_projects(&mut self) {
        let pairs: Vec<(String, String)> = self
            .records
            .iter()
            .filter(|rec| {
                rec.agent == Agent::Codex && !rec.session.is_empty() && !rec.project.is_empty()
            })
            .map(|rec| (rec.session.clone(), rec.project.clone()))
            .collect();
        for (session, project) in pairs {
            self.put_codex_project_if_absent(&session, &project);
        }

        for rec in &mut self.records {
            if rec.agent != Agent::Codex || rec.session.is_empty() || !rec.project.is_empty() {
                continue;
            }
            if let Some(project) = self.codex_projects.get(&rec.session) {
                rec.project = project.clone();
            }
        }
    }

    fn scan_codex_sessions(&mut self) {
        let base = self.codex_path("sessions");
        let mut files: Vec<PathBuf> = Vec::new();
        for year in read_dir_sorted(&base) {
            for month in read_dir_sorted(&year) {
                for day in read_dir_sorted(&month) {
                    let Ok(entries) = fs::read_dir(&day) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                        {
                            files.push(path);
                        }
                    }
                }
            }
        }
        files.sort();
        for path in files {
            self.scan_codex_session_cached(&path);
        }
    }

    fn scan_codex_session_cached(&mut self, source_path: &Path) {
        let Ok(stat) = fs::metadata(source_path) else {
            return;
        };
        self.load_codex_session_metadata(source_path);
        let fallback_ts = mtime_seconds(&stat);
        let Some(stamp) = stamp_from_stat(&stat) else {
            self.scan_codex_session_uncached(source_path, fallback_ts, stat.len());
            return;
        };
        let Some(cache_path) = self.codex_cache_path(source_path) else {
            self.scan_codex_session_uncached(source_path, fallback_ts, stat.len());
            return;
        };
        if let Some(data) = read_all(&cache_path) {
            if self.parse_codex_cache(&data, source_path, &stamp) {
                return;
            }
        }
        let record_start = self.records.len();
        if self.scan_codex_session_uncached(source_path, fallback_ts, stat.len()) {
            self.save_codex_cache(&cache_path, source_path, &stamp, record_start);
        }
    }

    /// Read only the head of a transcript to pick up `session_meta`, which is
    /// the first line Codex writes.
    fn load_codex_session_metadata(&mut self, source_path: &Path) {
        let Some(data) = read_prefix(source_path, 64 * 1024) else {
            return;
        };
        for line in data.split(|&b| b == b'\n') {
            let Some(v) = parse_line(line) else { continue };
            if self.record_codex_session_metadata(&v) {
                return;
            }
        }
    }

    fn record_codex_session_metadata(&mut self, v: &Value) -> bool {
        let Some(typ) = string_field(v, "type") else {
            return false;
        };
        if typ != "session_meta" {
            return false;
        }
        let Some(payload) = field(v, "payload") else {
            return true;
        };
        if !payload.is_object() {
            return true;
        }
        let Some(session_id) = string_field(payload, "id").map(str::to_string) else {
            return true;
        };
        let Some(cwd) = string_field(payload, "cwd").map(str::to_string) else {
            return true;
        };
        self.put_codex_project_if_absent(&session_id, &cwd);
        true
    }

    fn scan_codex_session_uncached(
        &mut self,
        source_path: &Path,
        fallback_ts: i64,
        source_size: u64,
    ) -> bool {
        if source_size > MAX_HISTORY_FILE_BYTES {
            return self.scan_codex_session_streaming(source_path, fallback_ts);
        }
        match read_all(source_path) {
            Some(data) => {
                self.parse_codex_session_with_last_active(&data, fallback_ts);
                true
            }
            None => self.scan_codex_session_streaming(source_path, fallback_ts),
        }
    }

    // CDXC:PromptSearch 2026-06-28-07:49:
    // Large Codex transcripts must still be indexed instead of disappearing at
    // the bounded whole-file read limit. Stream them so prompt extraction stays
    // proportional to the retained prompt text, not the full transcript size.
    fn scan_codex_session_streaming(&mut self, source_path: &Path, fallback_ts: i64) -> bool {
        let Ok(file) = fs::File::open(source_path) else {
            return false;
        };
        let mut reader = BufReader::with_capacity(64 * 1024, file);
        let mut state = CodexSessionParseState {
            record_start: self.records.len(),
            fallback_ts,
            ..Default::default()
        };
        let mut line: Vec<u8> = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => return false,
            }
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            let owned = std::mem::take(&mut line);
            self.parse_codex_session_line(&owned, &mut state);
            line = owned;
        }
        true
    }

    fn codex_cache_path(&self, source_path: &Path) -> Option<PathBuf> {
        let mut h = Wyhash::new(0);
        h.update(source_path.to_str()?.as_bytes());
        let id = h.finish();
        Some(
            self.cache_root
                .join("codex-sessions-v5")
                .join(format!("{id:016x}.jsonl")),
        )
    }

    fn save_codex_cache(
        &self,
        cache_path: &Path,
        source_path: &Path,
        stamp: &SourceStamp,
        record_start: usize,
    ) {
        let mut out = String::new();
        let header = serde_json::json!({
            "kind": "header",
            "version": CODEX_CACHE_VERSION,
            "source": source_path.to_string_lossy(),
            "source_size": stamp.size_text,
            "source_mtime_ns": stamp.mtime_text,
        });
        out.push_str(&header.to_string());
        out.push('\n');
        for rec in &self.records[record_start..] {
            let line = serde_json::json!({
                "kind": "record",
                "title": rec.title,
                "text": rec.text,
                "project": rec.project,
                "session": rec.session,
                "ts": rec.ts,
                "provider": rec.meta.provider,
                "model": rec.meta.model,
                "thinking": rec.meta.thinking,
                "plan": rec.meta.plan,
                "input": rec.meta.usage.input,
                "output": rec.meta.usage.output,
                "cache_read": rec.meta.usage.cache_read,
                "cache_write": rec.meta.usage.cache_write,
                "total": rec.meta.usage.total,
                "context_window": rec.meta.usage.context_window,
                "rate_percent": rec.meta.usage.rate_percent,
                "cost": rec.meta.usage.cost,
            });
            out.push_str(&line.to_string());
            out.push('\n');
        }
        out.push_str("{\"kind\":\"footer\"}\n");
        if let Some(dir) = cache_path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(cache_path, out);
    }

    fn parse_codex_cache(&mut self, data: &[u8], source_path: &Path, stamp: &SourceStamp) -> bool {
        let record_start = self.records.len();
        let mut lines = data.split(|&b| b == b'\n');
        let Some(header_line) = lines.next() else {
            return false;
        };
        let Some(header) = parse_line(header_line) else {
            return false;
        };
        if !codex_cache_header_matches(&header, source_path, stamp) {
            return false;
        }

        let mut saw_footer = false;
        for line in lines {
            let Some(v) = parse_line(line) else { continue };
            let Some(kind) = string_field(&v, "kind") else {
                continue;
            };
            if kind == "footer" {
                saw_footer = true;
                break;
            }
            if kind != "record" {
                continue;
            }
            let Some(text) = string_field(&v, "text") else {
                continue;
            };
            let Some(text) = visible_user_prompt(text) else {
                continue;
            };
            let session = string_field(&v, "session").unwrap_or("").to_string();
            let title = self
                .codex_title_for_session(&session)
                .or_else(|| string_field(&v, "title").map(str::to_string))
                .unwrap_or_default();
            self.records.push(Record {
                agent: Agent::Codex,
                title,
                text,
                project: string_field(&v, "project").unwrap_or("").to_string(),
                session,
                ts: timestamp_value(field(&v, "ts")),
                meta: Meta {
                    provider: string_field(&v, "provider").unwrap_or("").to_string(),
                    model: string_field(&v, "model").unwrap_or("").to_string(),
                    thinking: string_field(&v, "thinking").unwrap_or("").to_string(),
                    plan: string_field(&v, "plan").unwrap_or("").to_string(),
                    profile: String::new(),
                    usage: Usage {
                        input: int_val(field(&v, "input")),
                        output: int_val(field(&v, "output")),
                        cache_read: int_val(field(&v, "cache_read")),
                        cache_write: int_val(field(&v, "cache_write")),
                        total: int_val(field(&v, "total")),
                        context_window: int_val(field(&v, "context_window")),
                        rate_percent: float_val(field(&v, "rate_percent")),
                        cost: float_val(field(&v, "cost")),
                    },
                },
            });
        }
        if saw_footer {
            return true;
        }
        self.records.truncate(record_start);
        false
    }

    /// Parse `~/.codex/history.jsonl` content.
    pub fn parse_codex_history(&mut self, data: &[u8]) {
        for line in data.split(|&b| b == b'\n') {
            let Some(v) = parse_line(line) else { continue };
            let Some(text) = string_field(&v, "text") else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let text = text.to_string();
            let session = string_field(&v, "session_id").unwrap_or("").to_string();
            let ts = field(&v, "ts").and_then(Value::as_i64).unwrap_or(0);
            let title = self
                .codex_title_for_session(&session)
                .or_else(|| title_from_object(&v))
                .unwrap_or_default();
            self.records.push(Record {
                agent: Agent::Codex,
                title,
                text,
                project: String::new(),
                session,
                ts,
                meta: Meta::default(),
            });
        }
    }

    pub fn parse_codex_session(&mut self, data: &[u8]) {
        self.parse_codex_session_with_last_active(data, 0);
    }

    fn parse_codex_session_with_last_active(&mut self, data: &[u8], fallback_ts: i64) {
        let mut state = CodexSessionParseState {
            record_start: self.records.len(),
            fallback_ts,
            ..Default::default()
        };
        for line in data.split(|&b| b == b'\n') {
            self.parse_codex_session_line(line, &mut state);
        }
    }

    fn parse_codex_session_line(&mut self, line: &[u8], state: &mut CodexSessionParseState) {
        let Some(v) = parse_line(line) else { return };
        let line_ts = timestamp_from_object(&v);
        let Some(typ) = string_field(&v, "type").map(str::to_string) else {
            return;
        };

        if typ == "session_meta" {
            self.record_codex_session_metadata(&v);
            let Some(payload) = field(&v, "payload") else {
                return;
            };
            if !payload.is_object() {
                return;
            }
            if let Some(cwd) = string_field(payload, "cwd") {
                state.cwd = cwd.to_string();
            }
            if let Some(id) = string_field(payload, "id") {
                state.session_id = id.to_string();
                if let Some(title) = self.codex_title_for_session(&state.session_id) {
                    state.session_title = title.clone();
                    self.apply_title_to_session_records(state.record_start, &title);
                }
            }
            if state.session_title.is_empty() {
                if let Some(title) = title_from_object(payload) {
                    state.session_title = title.clone();
                    self.apply_title_to_session_records(state.record_start, &title);
                }
            }
            if let Some(provider) = string_field(payload, "model_provider") {
                state.meta.provider = provider.to_string();
            }
            let meta = state.meta.clone();
            self.apply_meta_to_session_records(state.record_start, &meta);
            return;
        }

        if typ == "event_msg" {
            let Some(payload) = field(&v, "payload") else {
                return;
            };
            let Some(ptype) = string_field(payload, "type") else {
                return;
            };
            if ptype != "token_count" {
                return;
            }
            if let Some(info) = field(payload, "info") {
                if info.is_object() {
                    if let Some(cw) = field(info, "model_context_window") {
                        state.meta.usage.context_window = int_val(Some(cw));
                    }
                    if let Some(u) = field(info, "total_token_usage") {
                        if u.is_object() {
                            if let Some(x) = field(u, "input_tokens") {
                                state.meta.usage.input = int_val(Some(x));
                            }
                            if let Some(x) = field(u, "output_tokens") {
                                state.meta.usage.output = int_val(Some(x));
                            }
                            if let Some(x) = field(u, "cached_input_tokens") {
                                state.meta.usage.cache_read = int_val(Some(x));
                            }
                            if let Some(x) = field(u, "total_tokens") {
                                state.meta.usage.total = int_val(Some(x));
                            }
                        }
                    }
                }
            }
            if let Some(rl) = field(payload, "rate_limits") {
                if rl.is_object() {
                    if let Some(primary) = field(rl, "primary") {
                        if primary.is_object() {
                            if let Some(x) = field(primary, "used_percent") {
                                state.meta.usage.rate_percent = float_val(Some(x));
                            }
                        }
                    }
                    if let Some(plan) = string_field(rl, "plan_type") {
                        state.meta.plan = plan.to_string();
                    }
                }
            }
            let meta = state.meta.clone();
            self.apply_meta_to_session_records(state.record_start, &meta);
            return;
        }

        if typ != "response_item" {
            return;
        }
        let Some(payload) = field(&v, "payload") else {
            return;
        };
        if string_field(payload, "type") != Some("message") {
            return;
        }
        if string_field(payload, "role") != Some("user") {
            return;
        }
        let Some(content) = field(payload, "content") else {
            return;
        };
        let Some(text) = content_text(content) else {
            return;
        };
        let Some(text) = visible_user_prompt(&text) else {
            return;
        };
        self.records.push(Record {
            agent: Agent::Codex,
            title: state.session_title.clone(),
            text,
            project: state.cwd.clone(),
            session: state.session_id.clone(),
            ts: if state.fallback_ts > 0 {
                state.fallback_ts
            } else {
                line_ts
            },
            meta: state.meta.clone(),
        });
    }

    pub(super) fn apply_meta_to_session_records(&mut self, start: usize, meta: &Meta) {
        for rec in &mut self.records[start..] {
            rec.meta = meta.clone();
        }
    }

    pub(super) fn apply_title_to_session_records(&mut self, start: usize, title: &str) {
        for rec in &mut self.records[start..] {
            rec.title = title.to_string();
        }
    }
}

#[derive(Default)]
struct CodexSessionParseState {
    cwd: String,
    session_id: String,
    session_title: String,
    meta: Meta,
    record_start: usize,
    fallback_ts: i64,
}

fn codex_cache_header_matches(v: &Value, source_path: &Path, stamp: &SourceStamp) -> bool {
    if string_field(v, "kind") != Some("header") {
        return false;
    }
    if field(v, "version").and_then(Value::as_i64) != Some(CODEX_CACHE_VERSION) {
        return false;
    }
    if string_field(v, "source") != Some(source_path.to_string_lossy().as_ref()) {
        return false;
    }
    if string_field(v, "source_size") != Some(stamp.size_text.as_str()) {
        return false;
    }
    if string_field(v, "source_mtime_ns") != Some(stamp.mtime_text.as_str()) {
        return false;
    }
    true
}
