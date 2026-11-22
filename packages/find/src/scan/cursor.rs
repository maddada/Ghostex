use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // cursor
    // -----------------------------------------------------------------------

    pub(super) fn scan_cursor(&mut self) {
        let base = self.path(".cursor/projects");
        let chat_metas = self.cursor_chat_metas();
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
                    if let Some(chat) = chat_metas
                        .get(&session_id)
                        .and_then(|path| cursor_info_from_meta(path))
                    {
                        info.project = chat.project;
                        if info.title.is_empty() {
                            info.title = chat.title;
                        }
                    }
                }
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
        cursor_info_from_meta(&meta_path).unwrap_or_default()
    }

    /// CDXC:PromptSearch 2026-10-17 WHY: Cursor CLI keeps each chat's `meta.json` (with its `cwd` and title) under `~/.cursor/chats/<md5 of the workspace>/<chat id>/`; only ACP sessions write `acp-sessions/<id>/meta.json`. Without it a CLI prompt's project came only from decoding the transcript folder's name, which drops `_` and, on Windows, the drive letter, so every Windows Cursor row had no project and the project filter found nothing (agent flow tests, Windows, 2026-10-17).
    fn cursor_chat_metas(&self) -> HashMap<String, PathBuf> {
        let mut metas = HashMap::new();
        for workspace in read_dir_sorted(&self.path(".cursor/chats")) {
            for chat in read_dir_sorted(&workspace) {
                if let Some(id) = chat.file_name().and_then(|n| n.to_str()) {
                    metas.insert(id.to_string(), chat.join("meta.json"));
                }
            }
        }
        metas
    }

    // CDXC:PromptSearch 2026-06-11-10:25:
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
                kind: RecordKind::User,
                interrupted: false,
                title: title.to_string(),
                text,
                project: project.to_string(),
                session: session_id.to_string(),
                ts,
                meta: Meta::default(),
            });
        }
    }
}

fn cursor_info_from_meta(meta_path: &Path) -> Option<CursorInfo> {
    let data = read_all(meta_path)?;
    let v = serde_json::from_slice::<Value>(&data).ok()?;
    Some(CursorInfo {
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
    })
}

#[derive(Default, Clone)]
struct CursorInfo {
    project: String,
    title: String,
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

pub(super) fn cursor_encoded_path_segment(segment: &str) -> String {
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
