use std::fs;
use std::path::Path;

use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // copilot
    // -----------------------------------------------------------------------

    /// CDXC:PromptSearch 2026-10-17 WHY:
    /// Copilot CLI keeps one folder per session in `~/.copilot/session-state/<id>/`: `events.jsonl`
    /// holds the session's start (folder, model) and every `user.message`, whose `content` is what
    /// the person typed (`transformedContent` adds Copilot's own date header), and `workspace.yaml`
    /// the session's name and last update. `copilot --resume <id>` takes the folder name.
    pub(super) fn scan_copilot(&mut self) {
        let base = self.path(".copilot/session-state");
        for dir in read_dir_sorted(&base) {
            let Some(session) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
                continue;
            };
            let events = dir.join("events.jsonl");
            let Ok(stat) = fs::metadata(&events) else {
                continue;
            };
            let Some(data) = read_all(&events) else {
                continue;
            };
            let mut info = read_copilot_workspace(&dir.join("workspace.yaml"));
            info.session = session;
            if info.ts == 0 {
                info.ts = mtime_seconds(&stat);
            }
            self.parse_copilot_events(&data, &info);
        }
    }

    /// Parse a Copilot CLI `events.jsonl`.
    pub fn parse_copilot_events(&mut self, data: &[u8], info: &CopilotInfo) {
        let record_start = self.records.len();
        let mut project = info.project.clone();
        let mut model = String::new();
        for line in data.split(|&b| b == b'\n') {
            // Each event opens with its type; skip the far more numerous tool and model events unparsed.
            let head = &line[..line.len().min(48)];
            let is_user = line.starts_with(b"{\"type\":\"user.message\"");
            if !is_user
                && !contains(head, b"\"session.start\"")
                && !contains(head, b"\"session.model_change\"")
            {
                continue;
            }
            let Some(v) = parse_line(line) else { continue };
            let Some(data) = field(&v, "data") else {
                continue;
            };
            match string_field(&v, "type") {
                Some("session.start") => {
                    if let Some(cwd) = field(data, "context").and_then(|c| string_field(c, "cwd")) {
                        if project.is_empty() {
                            project = cwd.to_string();
                        }
                    }
                    if let Some(selected) = string_field(data, "selectedModel") {
                        model = selected.to_string();
                    }
                }
                Some("session.model_change") => {
                    if let Some(next) = string_field(data, "newModel") {
                        model = next.to_string();
                    }
                }
                Some("user.message") => {
                    let Some(text) = string_field(data, "content").and_then(visible_user_prompt)
                    else {
                        continue;
                    };
                    self.records.push(Record {
                        agent: Agent::Copilot,
                        kind: RecordKind::User,
                        interrupted: false,
                        title: info.title.clone(),
                        text,
                        project: project.clone(),
                        session: info.session.clone(),
                        ts: info.ts,
                        meta: Meta::default(),
                    });
                }
                _ => {}
            }
        }
        for rec in &mut self.records[record_start..] {
            rec.meta.provider = "github".to_string();
            rec.meta.model = model.clone();
            if rec.project.is_empty() {
                rec.project = project.clone();
            }
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct CopilotInfo {
    pub session: String,
    pub project: String,
    pub title: String,
    pub ts: i64,
}

/// The few flat `key: value` lines of a session's `workspace.yaml` that Find shows.
fn read_copilot_workspace(path: &Path) -> CopilotInfo {
    let mut info = CopilotInfo::default();
    let Some(data) = read_prefix(path, 64 * 1024) else {
        return info;
    };
    let text = String::from_utf8_lossy(&data);
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches(|ch| ch == '"' || ch == '\'');
        match key {
            "cwd" => info.project = value.to_string(),
            "name" => info.title = value.to_string(),
            "updated_at" => info.ts = parse_iso8601_seconds(value),
            _ => {}
        }
    }
    info
}
