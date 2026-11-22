use std::fs;

use serde_json::Value;

use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // grok
    // -----------------------------------------------------------------------

    pub(super) fn scan_grok(&mut self) {
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
                kind: RecordKind::User,
                interrupted: false,
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
