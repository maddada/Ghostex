//! Per-agent history scanners (port of zehn's `src/scan.zig`).
//!
//! | Agent    | History location                                                      |
//! |----------|-----------------------------------------------------------------------|
//! | claude   | `~/.claude/history.jsonl`                                             |
//! | codex    | `~/.codex/history.jsonl` and `~/.codex/sessions/**/*.jsonl`           |
//! | pi       | `~/.pi/agent/sessions/*/*.jsonl`                                      |
//! | opencode | `~/.local/share/opencode/opencode.db` (SQLite)                        |
//! | cursor   | `~/.cursor/projects/*/agent-transcripts/*/*.jsonl`                    |
//! | grok     | `~/.grok/sessions/*/*/chat_history.jsonl` plus sibling `summary.json` |
//!
//! CDXC:AgentHistorySearch 2026-08-20:
//! opencode history used to be read by shelling out to the `sqlite3` CLI, so a
//! machine without it silently lost opencode results. The Rust port links
//! SQLite directly, which removes that external dependency instead of papering
//! over it with a skip-and-warn path.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::Agent;
use crate::wyhash::Wyhash;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub context_window: u64,
    #[serde(default)]
    pub rate_percent: f64,
    #[serde(default)]
    pub cost: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub thinking: String,
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub usage: Usage,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub agent: Agent,
    #[serde(default)]
    pub title: String,
    pub text: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub session: String,
    /// Session last-active unix seconds, 0 if unknown.
    pub ts: i64,
    #[serde(default)]
    pub meta: Meta,
}

impl Record {
    /// Title shown under a result: explicit title, else session id, else a placeholder.
    pub fn display_title(&self) -> &str {
        if !self.title.is_empty() {
            return &self.title;
        }
        if !self.session.is_empty() {
            return &self.session;
        }
        "Untitled session"
    }

    /// Last path segment of the project directory, or "No project".
    pub fn project_display_name(&self) -> &str {
        project_display_name(&self.project)
    }
}

pub fn project_display_name(project: &str) -> &str {
    if project.is_empty() {
        return "No project";
    }
    let trimmed = project.trim_end_matches(['/', '\\']);
    let trimmed = if trimmed.is_empty() { project } else { trimmed };
    match trimmed.rsplit(['/', '\\']).next() {
        Some(base) if !base.is_empty() => base,
        _ => trimmed,
    }
}

// CDXC:AgentHistorySearch 2026-06-28-07:49:
// Codex session transcripts emit current user prompt blocks as `input_text` as
// well as the older `text` shape. Accept both and keep the derived cache at v5
// so sessions previously cached as zero-record parses are rebuilt.
const CODEX_CACHE_VERSION: i64 = 5;
const MAX_HISTORY_FILE_BYTES: u64 = 64 * 1024 * 1024;

struct SourceStamp {
    size_text: String,
    mtime_text: String,
}

#[derive(Default)]
pub struct Scanner {
    home: PathBuf,
    cache_root: PathBuf,
    pub records: Vec<Record>,
    claude_titles: HashMap<String, String>,
    codex_titles: HashMap<String, String>,
    codex_projects: HashMap<String, String>,
    /// Set when an opencode DB exists but could not be read.
    pub opencode_error: Option<String>,
}

impl Scanner {
    pub fn new(home: impl Into<PathBuf>, cache_root: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            cache_root: cache_root.into(),
            ..Default::default()
        }
    }

    fn path(&self, suffix: &str) -> PathBuf {
        self.home.join(suffix)
    }

    pub fn scan_all(&mut self) {
        self.scan_claude();
        self.scan_codex();
        self.scan_pi();
        self.scan_opencode();
        self.scan_cursor();
        self.scan_grok();
        self.dedup();
    }

    /// Collapse identical (agent, text) prompts, keeping the most recent
    /// occurrence (highest ts). Preserves first-seen ordering otherwise.
    pub fn dedup(&mut self) {
        let mut seen: HashMap<(Agent, String), usize> = HashMap::new();
        let mut out: Vec<Record> = Vec::with_capacity(self.records.len());
        for rec in std::mem::take(&mut self.records) {
            let key = (rec.agent, rec.text.clone());
            match seen.get(&key) {
                Some(&pos) => {
                    if rec.ts > out[pos].ts {
                        out[pos] = rec;
                    }
                }
                None => {
                    seen.insert(key, out.len());
                    out.push(rec);
                }
            }
        }
        self.records = out;
    }

    // -----------------------------------------------------------------------
    // claude
    // -----------------------------------------------------------------------

    fn scan_claude(&mut self) {
        self.load_claude_session_titles();
        let p = self.path(".claude/history.jsonl");
        if let Some(data) = read_all(&p) {
            self.parse_claude_history(&data);
        }
    }

    fn load_claude_session_titles(&mut self) {
        self.load_claude_project_session_indexes(&self.path(".claude/projects"));
        self.load_claude_project_session_indexes(&self.path(".claude/projects2"));
        self.load_claude_profile_session_indexes();
    }

    fn load_claude_project_session_indexes(&mut self, base: &Path) {
        let Ok(entries) = fs::read_dir(base) else {
            return;
        };
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            if let Some(data) = read_all(&entry.path().join("sessions-index.json")) {
                self.parse_claude_sessions_index(&data);
            }
        }
    }

    fn load_claude_profile_session_indexes(&mut self) {
        let base = self.path(".claude-profiles");
        let Ok(profiles) = fs::read_dir(&base) else {
            return;
        };
        for profile in profiles.flatten() {
            let projects = profile.path().join("projects");
            let Ok(entries) = fs::read_dir(&projects) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                if let Some(data) = read_all(&entry.path().join("sessions-index.json")) {
                    self.parse_claude_sessions_index(&data);
                }
            }
        }
    }

    fn parse_claude_sessions_index(&mut self, data: &[u8]) {
        let Ok(v) = serde_json::from_slice::<Value>(data) else {
            return;
        };
        let Some(Value::Array(entries)) = field(&v, "entries") else {
            return;
        };
        for entry in entries {
            let Some(session_id) =
                string_field(entry, "sessionId").or_else(|| string_field(entry, "id"))
            else {
                continue;
            };
            let Some(title) = title_from_fields(
                entry,
                &["customTitle", "agentName", "summary", "slug", "title"],
            ) else {
                continue;
            };
            self.put_claude_title_if_absent(session_id, &title);
        }
    }

    fn put_claude_title_if_absent(&mut self, session_id: &str, title: &str) {
        if clean_title(session_id).is_none() {
            return;
        }
        let Some(safe) = clean_title(title) else {
            return;
        };
        self.claude_titles
            .entry(session_id.to_string())
            .or_insert(safe);
    }

    fn claude_title_for_session(&self, session_id: &str) -> Option<String> {
        if session_id.is_empty() {
            return None;
        }
        self.claude_titles.get(session_id).cloned()
    }

    /// Parse `~/.claude/history.jsonl` content.
    pub fn parse_claude_history(&mut self, data: &[u8]) {
        let paste_cache = self.path(".claude/paste-cache");
        for line in data.split(|&b| b == b'\n') {
            let Some(v) = parse_line(line) else { continue };
            let Some(disp) = string_field(&v, "display") else {
                continue;
            };
            if disp.is_empty() {
                continue;
            }
            /*
            Claude collapses large prompt pastes in history.jsonl to markers
            such as `[Pasted text #1 +69 lines]`. The original text remains in
            `pastedContents`, either inline or by content hash in paste-cache.
            Index the original prompt rather than the display-only marker so
            terms that occur inside a paste remain searchable.
            */
            let disp = expand_claude_pasted_contents(&v, disp, &paste_cache);
            if has_unresolved_claude_paste(&disp) {
                continue;
            }
            let Some(disp) = visible_user_prompt(&disp) else {
                continue;
            };
            let project = string_field(&v, "project").unwrap_or("").to_string();
            let session = string_field(&v, "sessionId").unwrap_or("").to_string();
            let ts = match field(&v, "timestamp").and_then(Value::as_i64) {
                Some(ms) => ms / 1000,
                None => 0,
            };
            let title = title_from_object(&v)
                .or_else(|| self.claude_title_for_session(&session))
                .unwrap_or_default();
            self.records.push(Record {
                agent: Agent::Claude,
                title,
                text: disp,
                project,
                session,
                ts,
                meta: Meta::default(),
            });
        }
    }

    // -----------------------------------------------------------------------
    // codex
    // -----------------------------------------------------------------------

    fn scan_codex(&mut self) {
        self.load_codex_session_index_titles();
        if let Some(data) = read_all(&self.path(".codex/history.jsonl")) {
            self.parse_codex_history(&data);
        }
        self.scan_codex_sessions();
        self.apply_codex_session_projects();
    }

    fn load_codex_session_index_titles(&mut self) {
        let Some(data) = read_all(&self.path(".codex/session_index.jsonl")) else {
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

    // CDXC:AgentHistorySearch 2026-06-11-10:02:
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
        let base = self.path(".codex/sessions");
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

    // CDXC:AgentHistorySearch 2026-06-28-07:49:
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

    fn apply_meta_to_session_records(&mut self, start: usize, meta: &Meta) {
        for rec in &mut self.records[start..] {
            rec.meta = meta.clone();
        }
    }

    fn apply_title_to_session_records(&mut self, start: usize, title: &str) {
        for rec in &mut self.records[start..] {
            rec.title = title.to_string();
        }
    }

    // -----------------------------------------------------------------------
    // pi
    // -----------------------------------------------------------------------

    fn scan_pi(&mut self) {
        let base = self.path(".pi/agent/sessions");
        let mut files: Vec<PathBuf> = Vec::new();
        for sub in read_dir_sorted(&base) {
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
                self.parse_pi_session_with_last_active(&data, fallback_ts);
            }
        }
    }

    pub fn parse_pi_session(&mut self, data: &[u8]) {
        self.parse_pi_session_with_last_active(data, 0);
    }

    fn parse_pi_session_with_last_active(&mut self, data: &[u8], fallback_ts: i64) {
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
            let head = &line[..line.len().min(128)];
            let keep = line.starts_with(b"{\"type\":\"session\"")
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
                if let Some(title) = title_from_object(&v) {
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
                agent: Agent::Pi,
                title: session_title.clone(),
                text,
                project: cwd.clone(),
                session: session_id.clone(),
                ts: if session_ts > 0 { session_ts } else { line_ts },
                meta: meta.clone(),
            });
        }
    }

    // -----------------------------------------------------------------------
    // cursor
    // -----------------------------------------------------------------------

    fn scan_cursor(&mut self) {
        let base = self.path(".cursor/projects");
        for project_dir in read_dir_sorted(&base) {
            let encoded = project_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let transcripts = project_dir.join("agent-transcripts");
            let session_dirs = read_dir_sorted(&transcripts);
            if session_dirs.is_empty() {
                continue;
            }
            let fallback_project = self.cursor_project_for_project_directory(&encoded);
            for session_dir in session_dirs {
                let Some(session_id) = session_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                else {
                    continue;
                };
                let file = session_dir.join(format!("{session_id}.jsonl"));
                let Ok(stat) = fs::metadata(&file) else {
                    continue;
                };
                let Some(data) = read_all(&file) else {
                    continue;
                };
                let mut info = self.cursor_info_for_session(&session_id);
                if info.project.is_empty() {
                    info.project = fallback_project.clone();
                }
                self.parse_cursor_transcript(
                    &data,
                    &info.project,
                    &info.title,
                    &session_id,
                    mtime_seconds(&stat),
                );
            }
        }
    }

    fn cursor_info_for_session(&self, session_id: &str) -> CursorInfo {
        let meta_path = self
            .home
            .join(format!(".cursor/acp-sessions/{session_id}/meta.json"));
        let Some(data) = read_all(&meta_path) else {
            return CursorInfo::default();
        };
        let Ok(v) = serde_json::from_slice::<Value>(&data) else {
            return CursorInfo::default();
        };
        CursorInfo {
            project: string_field(&v, "cwd").unwrap_or("").to_string(),
            title: title_from_fields(
                &v,
                &[
                    "thread_name",
                    "threadName",
                    "title",
                    "session_title",
                    "sessionTitle",
                ],
            )
            .unwrap_or_default(),
        }
    }

    // CDXC:AgentHistorySearch 2026-06-11-10:25:
    // Cursor Agent transcripts can exist without ACP meta.json cwd. Resolve
    // Cursor's encoded project directory name back to a verified filesystem path
    // so result rows show the real project name without inventing a bad cwd.
    fn cursor_project_for_project_directory(&self, encoded_project: &str) -> String {
        resolve_cursor_encoded_project_path(Path::new("/"), encoded_project, 0)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Parse Cursor Agent transcript JSONL from `~/.cursor/projects/*/agent-transcripts`.
    pub fn parse_cursor_transcript(
        &mut self,
        data: &[u8],
        project: &str,
        title: &str,
        session_id: &str,
        ts: i64,
    ) {
        for line in data.split(|&b| b == b'\n') {
            let Some(v) = parse_line(line) else { continue };
            if string_field(&v, "role") != Some("user") {
                continue;
            }
            let Some(msg) = field(&v, "message") else {
                continue;
            };
            if !msg.is_object() {
                continue;
            }
            let Some(content) = field(msg, "content") else {
                continue;
            };
            let Some(text) = content_text(content) else {
                continue;
            };
            let text = tagged_text(&text, "user_query").unwrap_or(&text);
            let Some(text) = visible_user_prompt(text) else {
                continue;
            };
            self.records.push(Record {
                agent: Agent::Cursor,
                title: title.to_string(),
                text,
                project: project.to_string(),
                session: session_id.to_string(),
                ts,
                meta: Meta::default(),
            });
        }
    }

    // -----------------------------------------------------------------------
    // grok
    // -----------------------------------------------------------------------

    fn scan_grok(&mut self) {
        let base = self.path(".grok/sessions");
        for project_dir in read_dir_sorted(&base) {
            for session_dir in read_dir_sorted(&project_dir) {
                let Some(dir_name) = session_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                else {
                    continue;
                };
                let Some(summary) = read_all(&session_dir.join("summary.json")) else {
                    continue;
                };
                let mut info = parse_grok_summary(&summary, &dir_name);
                let chat = session_dir.join("chat_history.jsonl");
                let Ok(stat) = fs::metadata(&chat) else {
                    continue;
                };
                let Some(data) = read_all(&chat) else {
                    continue;
                };
                if info.ts == 0 {
                    info.ts = mtime_seconds(&stat);
                }
                self.parse_grok_chat_history(&data, &info);
            }
        }
    }

    /// Parse Grok chat history JSONL from `~/.grok/sessions/*/*/chat_history.jsonl`.
    pub fn parse_grok_chat_history(&mut self, data: &[u8], info: &GrokInfo) {
        for line in data.split(|&b| b == b'\n') {
            let Some(v) = parse_line(line) else { continue };
            if string_field(&v, "type") != Some("user") {
                continue;
            }
            let Some(content) = field(&v, "content") else {
                continue;
            };
            let Some(text) = content_text(content) else {
                continue;
            };
            let text = tagged_text(&text, "user_query").unwrap_or(&text);
            let Some(text) = visible_user_prompt(text) else {
                continue;
            };
            self.records.push(Record {
                agent: Agent::Grok,
                title: info.title.clone(),
                text,
                project: info.project.clone(),
                session: info.session.clone(),
                ts: info.ts,
                meta: Meta {
                    provider: "xai".to_string(),
                    model: info.model.clone(),
                    ..Default::default()
                },
            });
        }
    }

    // -----------------------------------------------------------------------
    // opencode
    // -----------------------------------------------------------------------

    fn scan_opencode(&mut self) {
        let db = self.path(".local/share/opencode/opencode.db");
        self.scan_opencode_db(&db);
    }

    pub fn scan_opencode_db(&mut self, db_path: &Path) {
        if !db_path.exists() {
            return;
        }
        let fallback_ts = fs::metadata(db_path)
            .map(|m| mtime_seconds(&m))
            .unwrap_or(0);
        match read_opencode_rows(db_path) {
            Ok(rows) => {
                for row in rows {
                    self.parse_opencode_row(&row, fallback_ts);
                }
            }
            Err(err) => self.opencode_error = Some(err),
        }
    }

    /// Parse JSONL produced by `OPENCODE_QUERY` (one object per text part).
    pub fn parse_opencode(&mut self, data: &[u8]) {
        for line in data.split(|&b| b == b'\n') {
            if let Ok(text) = std::str::from_utf8(line) {
                self.parse_opencode_row(text, 0);
            }
        }
    }

    fn parse_opencode_row(&mut self, row: &str, fallback_ts: i64) {
        let Some(v) = parse_line(row.as_bytes()) else {
            return;
        };
        if string_field(&v, "role") != Some("user") {
            return;
        }
        if string_field(&v, "type") != Some("text") {
            return;
        }
        let Some(text) = string_field(&v, "text") else {
            return;
        };
        let Some(text) = visible_user_prompt(text) else {
            return;
        };
        let ts = timestamp_value(field(&v, "ts"));
        self.records.push(Record {
            agent: Agent::Opencode,
            title: title_from_object(&v).unwrap_or_default(),
            text,
            project: string_field(&v, "project").unwrap_or("").to_string(),
            session: string_field(&v, "session").unwrap_or("").to_string(),
            ts: if ts > 0 { ts } else { fallback_ts },
            meta: Meta {
                provider: string_field(&v, "provider").unwrap_or("").to_string(),
                model: string_field(&v, "model").unwrap_or("").to_string(),
                ..Default::default()
            },
        });
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

#[derive(Default, Clone)]
struct CursorInfo {
    project: String,
    title: String,
}

#[derive(Default, Clone, Debug)]
pub struct GrokInfo {
    pub session: String,
    pub project: String,
    pub title: String,
    pub model: String,
    pub ts: i64,
}

pub fn parse_grok_summary(data: &[u8], fallback_session: &str) -> GrokInfo {
    let Ok(v) = serde_json::from_slice::<Value>(data) else {
        return GrokInfo {
            session: fallback_session.to_string(),
            ..Default::default()
        };
    };
    let info_obj = field(&v, "info");
    let mut info = GrokInfo {
        session: info_obj
            .and_then(|i| string_field(i, "id"))
            .or_else(|| string_field(&v, "id"))
            .unwrap_or(fallback_session)
            .to_string(),
        project: info_obj
            .and_then(|i| string_field(i, "cwd"))
            .or_else(|| string_field(&v, "git_root_dir"))
            .unwrap_or("")
            .to_string(),
        title: title_from_object(&v)
            .or_else(|| info_obj.and_then(title_from_object))
            .unwrap_or_default(),
        model: string_field(&v, "current_model_id")
            .unwrap_or("")
            .to_string(),
        ts: 0,
    };
    if let Some(updated) = string_field(&v, "updated_at") {
        info.ts = parse_iso8601_seconds(updated);
    }
    info
}

const OPENCODE_QUERY: &str = concat!(
    "SELECT json_object(",
    "'role',json_extract(m.data,'$.role'),",
    "'type',json_extract(p.data,'$.type'),",
    "'text',json_extract(p.data,'$.text'),",
    "'title',null,",
    "'provider',json_extract(m.data,'$.model.providerID'),",
    "'model',json_extract(m.data,'$.model.modelID'),",
    "'project',s.directory,",
    "'session',s.id,",
    "'ts',coalesce(json_extract(m.data,'$.time.updated'),json_extract(m.data,'$.time.created'),json_extract(m.data,'$.updated_at'),json_extract(m.data,'$.created_at'),json_extract(m.data,'$.updatedAt'),json_extract(m.data,'$.createdAt'),json_extract(m.data,'$.timestamp'))) ",
    "FROM part p ",
    "JOIN message m ON p.message_id=m.id ",
    "JOIN session s ON p.session_id=s.id ",
    "WHERE json_extract(p.data,'$.type')='text';"
);

fn read_opencode_rows(db_path: &Path) -> Result<Vec<String>, String> {
    use rusqlite::{Connection, OpenFlags};
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("open opencode.db: {e}"))?;
    let mut stmt = conn
        .prepare(OPENCODE_QUERY)
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        match row {
            Ok(text) => out.push(text),
            Err(e) => return Err(format!("row: {e}")),
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn read_dir_sorted(base: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(base) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

fn expand_claude_pasted_contents(v: &Value, display: &str, paste_cache: &Path) -> String {
    let Some(Value::Object(pasted_contents)) = field(v, "pastedContents") else {
        return display.to_string();
    };
    let mut expanded = display.to_string();
    for (slot, item) in pasted_contents {
        let content = string_field(item, "content")
            .map(str::to_string)
            .or_else(|| {
                let hash = string_field(item, "contentHash")?;
                if hash.is_empty() || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
                let bytes = read_all(&paste_cache.join(format!("{hash}.txt")))?;
                String::from_utf8(bytes).ok()
            });
        let Some(content) = content else { continue };
        let marker_start = format!("[Pasted text #{slot}");
        let Some(start) = expanded.find(&marker_start) else {
            continue;
        };
        let Some(relative_end) = expanded[start..].find(']') else {
            continue;
        };
        expanded.replace_range(start..=start + relative_end, &content);
    }
    expanded
}

fn has_unresolved_claude_paste(text: &str) -> bool {
    text.contains("[Pasted text #")
}

fn tagged_text<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim())
}

/*
Agent transcripts frequently encode injected instructions as user-role records.
This picker is specifically a history of prompts a person sent, so reject those
provider envelopes at ingestion rather than making every UI hide them later.
*/
const INJECTED_PROVIDER_TAG_NAMES: &[&str] = &[
    "INSTRUCTIONS",
    "agent-message",
    "app-context",
    "bash-input",
    "bash-stderr",
    "bash-stdout",
    "collaboration_mode",
    "command-args",
    "command-message",
    "command-name",
    "cross-session-message",
    "environment_context",
    "fork-boilerplate",
    "ide_opened_file",
    "local-command-caveat",
    "local-command-stderr",
    "local-command-stdout",
    "mcp-polling-update",
    "mcp-resource-update",
    "rules",
    "system-reminder",
    "task-notification",
    "teammate-message",
    "turn_aborted",
    "user-memory-input",
    "user-prompt-submit-hook",
    "user_info",
    "user_instructions",
];

fn leading_tag_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('<')?;
    let end = rest
        .find(|ch: char| ch.is_whitespace() || ch == '>')
        .unwrap_or(rest.len());
    let name = &rest[..end];
    (!name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
    .then_some(name)
}

fn starts_with_injected_provider_envelope(text: &str) -> bool {
    leading_tag_name(text).is_some_and(|tag| INJECTED_PROVIDER_TAG_NAMES.contains(&tag))
        || text.starts_with("<channel source=")
        || text.starts_with("<permissions instructions>")
}

fn visible_user_prompt(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(|ch: char| ch.is_whitespace() || ch.is_control());
    if trimmed.is_empty()
        || starts_with_injected_provider_envelope(trimmed)
        || trimmed.starts_with("# AGENTS.md instructions")
        || trimmed.starts_with("[Image extracted from tool result above]")
        || trimmed.starts_with("[Request interrupted")
        || trimmed.starts_with("Caveat: The messages below")
        || trimmed.starts_with("The user interrupted the previous turn:")
    {
        return None;
    }
    if let Some(first_token) = trimmed.split_whitespace().next() {
        let is_slash_command = first_token.starts_with('/')
            && first_token
                .chars()
                .skip(1)
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            && !trimmed.contains('\n');
        if is_slash_command {
            return None;
        }
    }
    Some(trimmed.to_string())
}

fn read_all(path: &Path) -> Option<Vec<u8>> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_HISTORY_FILE_BYTES {
        return None;
    }
    fs::read(path).ok()
}

fn read_prefix(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(limit.min(64 * 1024));
    file.take(limit as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn mtime_seconds(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn mtime_nanos(meta: &fs::Metadata) -> Option<u128> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
}

fn stamp_from_stat(meta: &fs::Metadata) -> Option<SourceStamp> {
    Some(SourceStamp {
        size_text: meta.len().to_string(),
        mtime_text: mtime_nanos(meta)?.to_string(),
    })
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

fn parse_line(line: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(line).ok()?;
    let trimmed = text.trim_matches([' ', '\t', '\r', '\n']);
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

pub fn field<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.as_object()?.get(name)
}

pub fn string_field<'a>(v: &'a Value, name: &str) -> Option<&'a str> {
    field(v, name)?.as_str()
}

/// Extract plain text from a message `content` field that may be either a JSON
/// string or an array of text-bearing content blocks.
fn content_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let mut buf = String::new();
            for item in items {
                let Some(t) = string_field(item, "type") else {
                    continue;
                };
                if t != "text" && t != "input_text" {
                    continue;
                }
                let Some(txt) = string_field(item, "text") else {
                    continue;
                };
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(txt);
            }
            if buf.is_empty() {
                None
            } else {
                Some(buf)
            }
        }
        _ => None,
    }
}

fn int_val(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                if i > 0 {
                    i as u64
                } else {
                    0
                }
            } else if let Some(f) = n.as_f64() {
                if f > 0.0 {
                    f as u64
                } else {
                    0
                }
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn float_val(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn timestamp_from_object(v: &Value) -> i64 {
    for name in [
        "updated_at",
        "updatedAt",
        "timestamp",
        "created_at",
        "createdAt",
        "ts",
    ] {
        let ts = timestamp_value(field(v, name));
        if ts > 0 {
            return ts;
        }
    }
    let ts = timestamp_value(field(v, "time"));
    if ts > 0 {
        return ts;
    }
    0
}

fn timestamp_value(v: Option<&Value>) -> i64 {
    let Some(v) = v else { return 0 };
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                normalize_timestamp(i)
            } else if let Some(f) = n.as_f64() {
                normalize_timestamp(f as i64)
            } else {
                0
            }
        }
        Value::String(s) => parse_timestamp_string(s),
        Value::Object(_) => {
            for name in [
                "updated",
                "updated_at",
                "updatedAt",
                "created",
                "created_at",
                "createdAt",
                "timestamp",
                "ts",
            ] {
                let ts = timestamp_value(field(v, name));
                if ts > 0 {
                    return ts;
                }
            }
            0
        }
        _ => 0,
    }
}

fn normalize_timestamp(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    if n > 10_000_000_000 {
        n / 1000
    } else {
        n
    }
}

fn parse_timestamp_string(s: &str) -> i64 {
    if s.is_empty() {
        return 0;
    }
    if let Ok(n) = s.parse::<i64>() {
        return normalize_timestamp(n);
    }
    parse_iso8601_seconds(s)
}

fn title_from_fields(v: &Value, fields: &[&str]) -> Option<String> {
    if !v.is_object() {
        return None;
    }
    for name in fields {
        let Some(title) = string_field(v, name) else {
            continue;
        };
        if let Some(safe) = clean_title(title) {
            return Some(safe);
        }
    }
    None
}

fn title_from_object(v: &Value) -> Option<String> {
    title_from_fields(
        v,
        &[
            "thread_name",
            "threadName",
            "title",
            "session_title",
            "sessionTitle",
            "name",
        ],
    )
}

// CDXC:AgentHistorySearch 2026-06-07-14:59:
// Sanitize display titles at ingestion so corrupt or control-bearing metadata
// cannot render mojibake in the result list.
fn clean_title(title: &str) -> Option<String> {
    let trimmed = title.trim_matches([' ', '\t', '\r', '\n']);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed
        .chars()
        .any(|c| (c as u32) < 0x20 || c == '\u{7f}' || c == '\u{fffd}')
    {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn parse_iso8601_seconds(s: &str) -> i64 {
    let b = s.as_bytes();
    if b.len() < 19 {
        return 0;
    }
    let num = |range: std::ops::Range<usize>| -> Option<i64> {
        std::str::from_utf8(&b[range]).ok()?.parse::<i64>().ok()
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        num(0..4),
        num(5..7),
        num(8..10),
        num(11..13),
        num(14..16),
        num(17..19),
    ) else {
        return 0;
    };
    let days = days_from_civil(year, month, day);
    days * 86_400 + hour * 3_600 + minute * 60 + second
}

pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let mut y = year;
    let m = month;
    if m <= 2 {
        y -= 1;
    }
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CivilDate {
    pub year: i64,
    pub month: u32,
    pub day: u32,
}

pub fn civil_from_day_key(day: i64) -> CivilDate {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    if m <= 2 {
        y += 1;
    }
    CivilDate {
        year: y,
        month: m as u32,
        day: d as u32,
    }
}

fn resolve_cursor_encoded_project_path(
    current_path: &Path,
    remaining: &str,
    depth: usize,
) -> Option<PathBuf> {
    if remaining.is_empty() {
        return Some(current_path.to_path_buf());
    }
    if depth > 32 {
        return None;
    }
    let entries = fs::read_dir(current_path).ok()?;
    for entry in entries.flatten() {
        let file_type = entry.file_type().ok()?;
        if !file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let encoded_name = cursor_encoded_path_segment(name);
        if encoded_name.is_empty() {
            continue;
        }
        let child_path = current_path.join(name);
        if remaining == encoded_name {
            return Some(child_path);
        }
        if remaining.len() > encoded_name.len()
            && remaining.as_bytes()[encoded_name.len()] == b'-'
            && remaining.starts_with(&encoded_name)
        {
            if let Some(resolved) = resolve_cursor_encoded_project_path(
                &child_path,
                &remaining[encoded_name.len() + 1..],
                depth + 1,
            ) {
                return Some(resolved);
            }
        }
    }
    None
}

fn cursor_encoded_path_segment(segment: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in segment.bytes() {
        if c.is_ascii_alphanumeric() {
            out.push(c as char);
            last_dash = false;
        } else if c == b'_' {
            continue;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanner() -> Scanner {
        Scanner::new("/nonexistent-home", "/nonexistent-cache")
    }

    #[test]
    fn parses_claude_history_rows() {
        let mut s = scanner();
        s.parse_claude_history(
            br#"{"display":"fix the parser","project":"/tmp/x","sessionId":"abc","timestamp":1750000000000}
not json
{"display":"","project":"/tmp/x"}
{"display":"second","sessionId":"def"}"#,
        );
        assert_eq!(s.records.len(), 2);
        assert_eq!(s.records[0].text, "fix the parser");
        assert_eq!(s.records[0].project, "/tmp/x");
        assert_eq!(s.records[0].session, "abc");
        assert_eq!(s.records[0].ts, 1_750_000_000);
        assert_eq!(s.records[1].text, "second");
        assert_eq!(s.records[1].ts, 0);
    }

    #[test]
    fn parses_codex_session_user_messages_and_metadata() {
        let mut s = scanner();
        s.parse_codex_session(
            br#"{"type":"session_meta","payload":{"id":"sess-1","cwd":"/work/app","model_provider":"openai","title":"Rework login"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"add a test"}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"sure"}]}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"text","text":"<environment_context>skip me"}]}}"#,
        );
        assert_eq!(s.records.len(), 1);
        let rec = &s.records[0];
        assert_eq!(rec.agent, Agent::Codex);
        assert_eq!(rec.text, "add a test");
        assert_eq!(rec.project, "/work/app");
        assert_eq!(rec.session, "sess-1");
        assert_eq!(rec.title, "Rework login");
        assert_eq!(rec.meta.provider, "openai");
    }

    #[test]
    fn codex_token_counts_land_on_session_records() {
        let mut s = scanner();
        s.parse_codex_session(
            br#"{"type":"session_meta","payload":{"id":"s","cwd":"/w"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":"hello"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":200000,"total_token_usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}},"rate_limits":{"primary":{"used_percent":12.5},"plan_type":"pro"}}}"#,
        );
        assert_eq!(s.records.len(), 1);
        assert_eq!(s.records[0].meta.usage.input, 10);
        assert_eq!(s.records[0].meta.usage.context_window, 200_000);
        assert_eq!(s.records[0].meta.usage.rate_percent, 12.5);
        assert_eq!(s.records[0].meta.plan, "pro");
    }

    #[test]
    fn parses_pi_sessions_with_titles_and_usage() {
        let mut s = scanner();
        s.parse_pi_session(
            br#"{"type":"session","id":"pi-1","cwd":"/p","title":"Naming things","timestamp":1750000000}
{"type":"message","message":{"role":"user","content":"rename the module"}}
{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"opus","usage":{"input":3,"output":4,"cost":{"total":0.5}}}}
{"type":"message","message":{"role":"user","content":[{"type":"text","text":"again"}]}}"#,
        );
        assert_eq!(s.records.len(), 2);
        assert_eq!(s.records[0].title, "Naming things");
        assert_eq!(s.records[0].session, "pi-1");
        assert_eq!(s.records[0].ts, 1_750_000_000);
        assert_eq!(s.records[1].text, "again");
        assert_eq!(s.records[1].meta.model, "opus");
        assert_eq!(s.records[0].meta.usage.cost, 0.5);
    }

    #[test]
    fn parses_cursor_and_grok_transcripts() {
        let mut s = scanner();
        s.parse_cursor_transcript(
            br#"{"role":"user","message":{"content":[{"type":"text","text":"cursor prompt"}]}}
{"role":"assistant","message":{"content":"nope"}}"#,
            "/proj",
            "Cursor thread",
            "cur-1",
            42,
        );
        assert_eq!(s.records.len(), 1);
        assert_eq!(s.records[0].agent, Agent::Cursor);
        assert_eq!(s.records[0].ts, 42);

        let info = parse_grok_summary(
            br#"{"info":{"id":"grok-1","cwd":"/g"},"title":"Grok thread","current_model_id":"grok-4","updated_at":"2026-08-20T10:11:12Z"}"#,
            "fallback",
        );
        assert_eq!(info.session, "grok-1");
        assert_eq!(info.project, "/g");
        assert_eq!(info.model, "grok-4");
        assert!(info.ts > 0);
        s.parse_grok_chat_history(br#"{"type":"user","content":"grok prompt"}"#, &info);
        assert_eq!(s.records.len(), 2);
        assert_eq!(s.records[1].agent, Agent::Grok);
        assert_eq!(s.records[1].meta.provider, "xai");
    }

    #[test]
    fn parses_opencode_rows() {
        let mut s = scanner();
        s.parse_opencode(
            br#"{"role":"user","type":"text","text":"opencode prompt","project":"/o","session":"oc-1","provider":"anthropic","model":"sonnet","ts":1750000000000}
{"role":"assistant","type":"text","text":"reply"}"#,
        );
        assert_eq!(s.records.len(), 1);
        assert_eq!(s.records[0].ts, 1_750_000_000);
        assert_eq!(s.records[0].meta.model, "sonnet");
    }

    #[test]
    fn dedup_keeps_the_most_recent_occurrence_per_agent_and_text() {
        let mut s = scanner();
        let mk = |agent, text: &str, ts| Record {
            agent,
            title: String::new(),
            text: text.to_string(),
            project: String::new(),
            session: String::new(),
            ts,
            meta: Meta::default(),
        };
        s.records = vec![
            mk(Agent::Claude, "same", 10),
            mk(Agent::Codex, "same", 20),
            mk(Agent::Claude, "same", 30),
            mk(Agent::Claude, "other", 5),
        ];
        s.dedup();
        assert_eq!(s.records.len(), 3);
        assert_eq!(s.records[0].ts, 30);
        assert_eq!(s.records[1].agent, Agent::Codex);
        assert_eq!(s.records[2].text, "other");
    }

    #[test]
    fn timestamps_normalize_millis_and_iso_strings() {
        assert_eq!(normalize_timestamp(1_750_000_000_000), 1_750_000_000);
        assert_eq!(normalize_timestamp(1_750_000_000), 1_750_000_000);
        assert_eq!(normalize_timestamp(-4), 0);
        assert_eq!(parse_iso8601_seconds("1970-01-01T00:00:00Z"), 0);
        assert_eq!(parse_iso8601_seconds("1970-01-02T00:00:01Z"), 86_401);
        assert_eq!(parse_iso8601_seconds("short"), 0);
    }

    #[test]
    fn civil_date_round_trips() {
        for day in [-1000i64, 0, 19_000, 20_684] {
            let d = civil_from_day_key(day);
            assert_eq!(days_from_civil(d.year, d.month as i64, d.day as i64), day);
        }
    }

    #[test]
    fn titles_reject_control_characters_and_replacement_chars() {
        assert_eq!(clean_title("  Fine title  ").as_deref(), Some("Fine title"));
        assert_eq!(clean_title(""), None);
        assert_eq!(clean_title("bad\u{7}title"), None);
        assert_eq!(clean_title("moji\u{fffd}bake"), None);
    }

    #[test]
    fn cursor_encoded_segments_collapse_non_alphanumerics() {
        assert_eq!(cursor_encoded_path_segment("_active"), "active");
        assert_eq!(
            cursor_encoded_path_segment("my project.v2"),
            "my-project-v2"
        );
        assert_eq!(cursor_encoded_path_segment("--"), "");
    }

    #[test]
    fn project_display_name_uses_the_last_segment() {
        assert_eq!(project_display_name("/a/b/Ghostex"), "Ghostex");
        assert_eq!(project_display_name("/a/b/Ghostex/"), "Ghostex");
        assert_eq!(project_display_name(""), "No project");
    }
}
