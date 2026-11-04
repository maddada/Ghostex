use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::agent::Agent;

use super::*;

impl Scanner {
    // -----------------------------------------------------------------------
    // empryo
    // -----------------------------------------------------------------------

    /// CDXC:PromptSearch 2026-10-06 WHY:
    /// Empryo keeps each session inside its repository (`<repo>/.empryo/sessions/<id>/`), so there
    /// is no single history folder to walk. Its own thread index, `threads.db` in Empryo's home
    /// (`~/.empryo`, or `AppData\Local\Empryo` on Windows, as gxserver's `empryo_home` resolves it),
    /// names every repository it has run in; the scan opens it read-only and reads the session
    /// folders of each one, which also finds a session the index has not caught up with yet.
    pub(super) fn scan_empryo(&mut self) {
        let db = if cfg!(windows) {
            self.path("AppData/Local/Empryo/threads.db")
        } else {
            self.path(".empryo/threads.db")
        };
        if !db.exists() {
            return;
        }
        match read_empryo_checkouts(&db) {
            Ok(checkouts) => {
                for checkout in checkouts {
                    let sessions = Path::new(&checkout).join(".empryo").join("sessions");
                    for dir in read_dir_sorted(&sessions) {
                        self.scan_empryo_session(&dir, &checkout);
                    }
                }
            }
            Err(err) => self.empryo_error = Some(err),
        }
    }

    fn scan_empryo_session(&mut self, dir: &Path, checkout: &str) {
        let Some(session) = dir.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        // `empryo --session <id>` resolves the folder by this name, so only a name it accepts resumes.
        if session.is_empty()
            || !session
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return;
        }
        let log = dir.join("session.jsonl");
        let Some(data) = read_all(&log) else {
            return;
        };
        let mut info = read_all(&dir.join("meta.json"))
            .map(|meta| parse_empryo_meta(&meta))
            .unwrap_or_default();
        info.session = session.to_string();
        if info.project.is_empty() {
            info.project = checkout.to_string();
        }
        if info.ts == 0 {
            info.ts = fs::metadata(&log).map(|m| mtime_seconds(&m)).unwrap_or(0);
        }
        self.parse_empryo_session(&data, &info);
    }

    /// Parse an Empryo `session.jsonl`. A turn's prompt is the `ui` copy of its `user` record;
    /// the `core` copy carries the repository map Empryo appends to every prompt, and hidden
    /// prompts are the background-agent reports Empryo injects itself.
    pub fn parse_empryo_session(&mut self, data: &[u8], info: &EmpryoInfo) {
        for line in data.split(|&b| b == b'\n') {
            // Every record opens with its kind; skip edit baselines and replies unparsed.
            if !line.starts_with(b"{\"k\":\"user\"") {
                continue;
            }
            let Some(v) = parse_line(line) else { continue };
            let Some(ui) = field(&v, "ui") else { continue };
            if ui.get("hidden").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let Some(text) = string_field(ui, "content").and_then(visible_user_prompt) else {
                continue;
            };
            self.records.push(Record {
                agent: Agent::Empryo,
                title: info.title.clone(),
                text,
                project: info.project.clone(),
                session: info.session.clone(),
                ts: info.ts,
                meta: Meta {
                    provider: info.provider.clone(),
                    model: info.model.clone(),
                    ..Default::default()
                },
            });
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct EmpryoInfo {
    pub session: String,
    pub project: String,
    pub title: String,
    pub provider: String,
    pub model: String,
    pub ts: i64,
}

/// Title, folder, last activity and the active tab's model from a session's `meta.json`.
pub fn parse_empryo_meta(data: &[u8]) -> EmpryoInfo {
    let Ok(v) = serde_json::from_slice::<Value>(data) else {
        return EmpryoInfo::default();
    };
    let tabs = field(&v, "tabs").and_then(Value::as_array);
    let active_tab = string_field(&v, "activeTabId");
    let tab = tabs.and_then(|tabs| {
        tabs.iter()
            .find(|tab| active_tab.is_some() && string_field(tab, "id") == active_tab)
            .or_else(|| tabs.first())
    });
    // Models read `<provider>/<model>`, e.g. `subscriptions/gpt-6-luna`.
    let (provider, model) = match tab.and_then(|tab| string_field(tab, "activeModel")) {
        Some(full) => match full.split_once('/') {
            Some((provider, model)) => (provider.to_string(), model.to_string()),
            None => (String::new(), full.to_string()),
        },
        None => (String::new(), String::new()),
    };
    EmpryoInfo {
        session: String::new(),
        project: string_field(&v, "cwd").unwrap_or("").to_string(),
        title: title_from_fields(&v, &["customTitle", "title"]).unwrap_or_default(),
        provider,
        model,
        ts: timestamp_value(field(&v, "updatedAt")),
    }
}

/// Every repository Empryo has run in, from its thread index, opened for reading only. Each table is read
/// on its own so an Empryo version without one of them still yields the other's repositories, and
/// Empryo writing the index at the same moment waits briefly instead of failing the scan.
fn read_empryo_checkouts(db_path: &Path) -> Result<Vec<String>, String> {
    let conn = crate::agent_store::open_agent_store(db_path)
        .map_err(|e| format!("open {}: {e}", db_path.display()))?;
    conn.busy_timeout(std::time::Duration::from_millis(1000))
        .map_err(|e| format!("open {}: {e}", db_path.display()))?;
    let column = |sql: &str| -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
        rows.filter_map(|row| row.transpose()).collect()
    };
    let mut checkouts = Vec::new();
    let mut errors = Vec::new();
    for sql in [
        "SELECT DISTINCT checkout FROM threads",
        "SELECT DISTINCT path FROM checkouts",
    ] {
        match column(sql) {
            Ok(rows) => checkouts.extend(rows),
            Err(e) => errors.push(e.to_string()),
        }
    }
    if errors.len() == 2 {
        return Err(format!("read {}: {}", db_path.display(), errors.join("; ")));
    }
    checkouts.sort();
    checkouts.dedup();
    Ok(checkouts)
}
