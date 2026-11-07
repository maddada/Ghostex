use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // claude
    // -----------------------------------------------------------------------

    pub(super) fn scan_claude(&mut self) {
        self.load_claude_session_titles();
        let p = self.claude_path("history.jsonl");
        if let Some(data) = read_all(&p) {
            self.parse_claude_history(&data);
        }
    }

    fn load_claude_session_titles(&mut self) {
        self.load_claude_project_session_indexes(&self.claude_path("projects"));
        self.load_claude_project_session_indexes(&self.claude_path("projects2"));
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
        let paste_cache = self.claude_path("paste-cache");
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
