use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent::Agent;

use super::*;

/// The text parts of the user messages of every top-level interactive session, in order. Subagent
/// sessions (`parent_id`, `sess_subagent*`) hold the task the parent agent wrote.
const ZCODE_QUERY: &str = concat!(
    "SELECT s.id, s.directory, COALESCE(s.title, ''), s.time_updated, m.id, m.data, p.data ",
    "FROM session s JOIN message m ON m.session_id = s.id JOIN part p ON p.message_id = m.id ",
    "WHERE s.task_type = 'interactive' AND s.parent_id IS NULL AND s.id NOT GLOB 'sess_subagent*' ",
    "AND json_extract(m.data, '$.role') = 'user' ",
    "ORDER BY s.time_created, s.id, m.time_created, m.id, p.sequence, p.time_created, p.id;"
);

struct ZcodeRow {
    session: String,
    cwd: String,
    title: String,
    updated_ms: i64,
    message: String,
    message_data: String,
    part_data: String,
}

impl Scanner {
    // -----------------------------------------------------------------------
    // zcode
    // -----------------------------------------------------------------------

    /// CDXC:PromptSearch 2026-10-17 WHY:
    /// ZCode CLI keeps its conversations in SQLite (`~/.zcode/cli/db/db.sqlite`, or the
    /// `storage.sessionDbPath` its settings name), the store gxserver's ZCode chat reads: `session`
    /// (id, directory, title, time_updated in ms), `message` (role and visibility in `data`) and
    /// `part` (the text). A message ZCode injected itself is marked hidden (`semantics.uiVisibility`
    /// or `metadata.visibility: model-only`), and its synthetic parts are skipped. The `rollout/`
    /// files are raw model I/O logs, not prompts. Resume is `zcode --resume <sess_id>`.
    pub(super) fn scan_zcode(&mut self) {
        let db = zcode_database_path(&self.home);
        let Ok(rows) = read_zcode_rows(&db) else {
            return;
        };
        let mut pending: Option<(ZcodeRow, Meta, String)> = None;
        for row in rows {
            if pending
                .as_ref()
                .is_some_and(|(open, _, _)| open.message != row.message)
            {
                if let Some((open, meta, text)) = pending.take() {
                    self.push_zcode_prompt(open, meta, &text);
                }
            }
            let Some(part) = serde_json::from_str::<Value>(&row.part_data).ok() else {
                continue;
            };
            if string_field(&part, "type") != Some("text")
                || field(&part, "synthetic").and_then(Value::as_bool) == Some(true)
            {
                continue;
            }
            let Some(part_text) = string_field(&part, "text").map(str::to_string) else {
                continue;
            };
            match pending.as_mut() {
                Some((_, _, text)) => {
                    text.push('\n');
                    text.push_str(&part_text);
                }
                None => {
                    let Some(meta) = zcode_visible_user_message(&row.message_data) else {
                        continue;
                    };
                    pending = Some((row, meta, part_text));
                }
            }
        }
        if let Some((open, meta, text)) = pending {
            self.push_zcode_prompt(open, meta, &text);
        }
    }

    fn push_zcode_prompt(&mut self, row: ZcodeRow, meta: Meta, text: &str) {
        let Some(text) = visible_user_prompt(text) else {
            return;
        };
        self.records.push(Record {
            agent: Agent::Zcode,
            title: clean_title(&row.title).unwrap_or_default(),
            text,
            project: row.cwd,
            session: row.session,
            ts: row.updated_ms / 1000,
            meta,
        });
    }
}

/// The model of a user message a person typed, or `None` for one ZCode injected.
fn zcode_visible_user_message(data: &str) -> Option<Meta> {
    let info = serde_json::from_str::<Value>(data).ok()?;
    if info
        .pointer("/semantics/uiVisibility")
        .and_then(Value::as_str)
        == Some("hidden")
        || info.pointer("/metadata/visibility").and_then(Value::as_str) == Some("model-only")
    {
        return None;
    }
    let selection = field(&info, "modelSelection");
    Some(Meta {
        provider: selection
            .and_then(|s| string_field(s, "providerId"))
            .unwrap_or("")
            .to_string(),
        model: selection
            .and_then(|s| string_field(s, "modelId"))
            .unwrap_or("")
            .to_string(),
        thinking: info
            .pointer("/modelSelection/options/reasoningLevel")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        ..Default::default()
    })
}

/// ZCode's session store: `storage.sessionDbPath` from its settings (`setting.json`, then the
/// older `config.json`), else the default. SEE-ALSO: `zcode_database_path` in
/// server/src/session_chat_zcode.rs.
fn zcode_database_path(home: &Path) -> PathBuf {
    let cli = home.join(".zcode/cli");
    ["setting.json", "config.json"]
        .iter()
        .find_map(|name| {
            let data = read_prefix(&cli.join(name), 1024 * 1024)?;
            let config = serde_json::from_slice::<Value>(&data).ok()?;
            let path = config.pointer("/storage/sessionDbPath")?.as_str()?.trim();
            if path.is_empty() {
                return None;
            }
            Some(match path.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None => PathBuf::from(path),
            })
        })
        .unwrap_or_else(|| cli.join("db/db.sqlite"))
}

fn read_zcode_rows(db_path: &Path) -> Result<Vec<ZcodeRow>, String> {
    if !db_path.is_file() {
        return Ok(Vec::new());
    }
    let conn = crate::agent_store::open_agent_store(db_path)
        .map_err(|e| format!("open db.sqlite: {e}"))?;
    let _ = conn.busy_timeout(std::time::Duration::from_millis(250));
    let mut stmt = conn
        .prepare(ZCODE_QUERY)
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ZcodeRow {
                session: row.get(0)?,
                cwd: row.get(1)?,
                title: row.get(2)?,
                updated_ms: row.get(3)?,
                message: row.get(4)?,
                message_data: row.get(5)?,
                part_data: row.get(6)?,
            })
        })
        .map_err(|e| format!("query: {e}"))?;
    Ok(rows.flatten().collect())
}
