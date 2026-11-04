use std::fs;
use std::path::Path;

use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // opencode
    // -----------------------------------------------------------------------

    pub(super) fn scan_opencode(&mut self) {
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

    /// Parse JSONL produced by `OPENCODE_QUERY` (one object per user prompt).
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

/// OpenCode 2 keeps each conversation's messages in `session_message` (a user prompt is a
/// `user` row whose JSON holds `text`) and the conversation's folder, title and model in
/// `session_v2`. Subagent conversations (`parent_id`) hold prompts the agent wrote, not the user.
const OPENCODE_QUERY: &str = concat!(
    "SELECT json_object(",
    "'role','user',",
    "'type','text',",
    "'text',json_extract(m.data,'$.text'),",
    "'title',s.title,",
    "'provider',json_extract(s.model,'$.providerID'),",
    "'model',json_extract(s.model,'$.id'),",
    "'project',s.directory,",
    "'session',s.id,",
    "'ts',m.time_created) ",
    "FROM session_message m ",
    "JOIN session_v2 s ON m.session_id=s.id ",
    "WHERE m.type='user' AND s.parent_id IS NULL;"
);

fn read_opencode_rows(db_path: &Path) -> Result<Vec<String>, String> {
    let conn = crate::agent_store::open_agent_store(db_path)
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
