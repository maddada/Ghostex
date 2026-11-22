use std::fs;
use std::path::Path;

use crate::agent::Agent;

use super::*;

/// Opening lines of user rows no person typed: Droid's own skill launch notice, and what the pi
/// bridge (`pi-droid-sdk`) sends when Pi drives Droid (Pi's system prompt, project context and
/// compaction requests, whose typed prompt is already in Pi's own log).
const DROID_INJECTED_PREFIXES: &[&str] = &[
    "<system-notification>",
    "<project_context>",
    "Droid pi bridge contract:",
    "You are an expert coding assistant operating inside pi",
    "You are a context summarization assistant",
];

/// The title Droid gives a session before it names it.
const DROID_PLACEHOLDER_TITLE: &str = "New Session";

impl Scanner {
    // -----------------------------------------------------------------------
    // droid (Factory)
    // -----------------------------------------------------------------------

    /// CDXC:PromptSearch 2026-10-17 WHY:
    /// Factory Droid keeps one log per session in `~/.factory/sessions/<project>/<id>.jsonl`: a
    /// `session_start` row (id, cwd, title) and `message` rows. A typed prompt is a user message
    /// with no `visibility` (hook runs, errors and injected reminders carry one); subagent runs
    /// (`callingSessionId`) hold the parent's task, not a prompt. The sibling `<id>.settings.json`
    /// names the model. `~/.factory/history.json` has no session id or folder, so it is not read.
    pub(super) fn scan_droid(&mut self) {
        let base = self.path(".factory/sessions");
        for dir in read_dir_sorted(&base) {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<_> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                })
                .collect();
            files.sort();
            for path in files {
                let Ok(stat) = fs::metadata(&path) else {
                    continue;
                };
                let Some(data) = read_all(&path) else {
                    continue;
                };
                let model = read_droid_model(&path.with_extension("settings.json"));
                self.parse_droid_session(&data, mtime_seconds(&stat), &model);
            }
        }
    }

    /// Parse one Droid session log; `ts` is the session's last-active time.
    pub fn parse_droid_session(&mut self, data: &[u8], ts: i64, model: &str) {
        let mut lines = data.split(|&b| b == b'\n');
        let Some(header) = lines.next().and_then(parse_line) else {
            return;
        };
        if string_field(&header, "type") != Some("session_start")
            || field(&header, "callingSessionId").is_some()
        {
            return;
        }
        let session = string_field(&header, "id").unwrap_or("").to_string();
        let project = string_field(&header, "cwd").unwrap_or("").to_string();
        let title = title_from_object(&header)
            .filter(|title| title != DROID_PLACEHOLDER_TITLE)
            .unwrap_or_default();
        for line in lines {
            let head = &line[..line.len().min(160)];
            if !line.starts_with(b"{\"type\":\"message\"") || !contains(head, b"\"role\":\"user\"")
            {
                continue;
            }
            let Some(v) = parse_line(line) else { continue };
            let Some(msg) = field(&v, "message") else {
                continue;
            };
            if string_field(msg, "role") != Some("user") || field(msg, "visibility").is_some() {
                continue;
            }
            let Some(text) = field(msg, "content").and_then(content_text) else {
                continue;
            };
            let trimmed = text.trim_start();
            if DROID_INJECTED_PREFIXES
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
            {
                continue;
            }
            let Some(text) = visible_user_prompt(&text) else {
                continue;
            };
            self.records.push(Record {
                agent: Agent::Droid,
                kind: RecordKind::User,
                interrupted: false,
                title: title.clone(),
                text,
                project: project.clone(),
                session: session.clone(),
                ts,
                meta: Meta {
                    provider: "factory".to_string(),
                    model: model.to_string(),
                    ..Default::default()
                },
            });
        }
    }
}

/// The model a Droid session last ran on, from its `<id>.settings.json`.
fn read_droid_model(path: &Path) -> String {
    read_prefix(path, 64 * 1024)
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
        .and_then(|settings| string_field(&settings, "model").map(str::to_string))
        .unwrap_or_default()
}
