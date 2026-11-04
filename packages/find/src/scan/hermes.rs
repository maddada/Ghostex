use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::Agent;

use super::*;

/// Opening lines of the user rows Hermes writes itself (notices, compaction summaries and
/// handoffs, delegation and cron reports).
/// SEE-ALSO: `HERMES_INJECTED_PREFIXES` in server/src/external_sessions/hermes.rs.
const HERMES_INJECTED_PREFIXES: &[&str] = &[
    "[System:",
    "[System note:",
    "[CONTEXT",
    "[PRIOR CONTEXT",
    "[IMPORTANT: Background",
    "[Your active task list",
    "[Planning state preserved",
    "[ASYNC DELEGATION",
    "[OUT-OF-BAND",
    "[STILL IN PROGRESS",
    "Cronjob Response:",
];

/// User rows of every session a person talks in: subagent runs hold the task the parent agent
/// wrote, and cron runs a scheduled job's prompt. Rows Hermes typed itself carry a `display_kind`.
const HERMES_QUERY: &str = concat!(
    "SELECT s.id, COALESCE(s.title, ''), COALESCE(s.cwd, ''), COALESCE(s.model, ''), ",
    "COALESCE(s.billing_provider, ''), CAST(COALESCE(s.last_activity_at, s.started_at) AS INTEGER), ",
    "m.content ",
    "FROM messages m JOIN sessions s ON m.session_id = s.id ",
    "WHERE m.role = 'user' AND m.display_kind IS NULL AND m.content IS NOT NULL ",
    "AND s.source NOT IN ('subagent', 'cron') ",
    "ORDER BY m.id;"
);

struct HermesRow {
    session: String,
    title: String,
    cwd: String,
    model: String,
    provider: String,
    ts: i64,
    content: String,
}

impl Scanner {
    // -----------------------------------------------------------------------
    // hermes
    // -----------------------------------------------------------------------

    /// CDXC:PromptSearch 2026-10-17 WHY:
    /// Hermes Agent keeps its conversations in SQLite: `~/.hermes/state.db` for the default
    /// profile and `profiles/<name>/state.db` for each named one, the stores gxserver's Hermes chat
    /// reads. Each prompt remembers its store's profile so resuming it opens the right bot.
    pub(super) fn scan_hermes(&mut self) {
        let home = self.path(".hermes");
        for (db, profile) in hermes_stores(&home) {
            let Ok(rows) = read_hermes_rows(&db) else {
                continue;
            };
            for row in rows {
                self.push_hermes_row(row, &profile);
            }
        }
    }

    fn push_hermes_row(&mut self, row: HermesRow, profile: &str) {
        let content = row.content.trim_start();
        if HERMES_INJECTED_PREFIXES
            .iter()
            .any(|prefix| content.starts_with(prefix))
        {
            return;
        }
        let Some(text) = visible_user_prompt(content) else {
            return;
        };
        self.records.push(Record {
            agent: Agent::Hermes,
            title: row.title,
            text,
            project: row.cwd,
            session: row.session,
            ts: row.ts,
            meta: Meta {
                provider: row.provider,
                model: row.model,
                profile: profile.to_string(),
                ..Default::default()
            },
        });
    }
}

/// Every Hermes store with the profile `-p` names it by: the default profile's, then each named
/// profile's.
fn hermes_stores(home: &Path) -> Vec<(PathBuf, String)> {
    let mut stores = vec![(home.join("state.db"), "default".to_string())];
    for dir in read_dir_sorted(&home.join("profiles")) {
        let Some(name) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        stores.push((dir.join("state.db"), name));
    }
    stores
        .into_iter()
        .filter(|(db, _)| fs::metadata(db).is_ok_and(|meta| meta.is_file()))
        .collect()
}

fn read_hermes_rows(db_path: &Path) -> Result<Vec<HermesRow>, String> {
    let conn =
        crate::agent_store::open_agent_store(db_path).map_err(|e| format!("open state.db: {e}"))?;
    let _ = conn.busy_timeout(std::time::Duration::from_millis(250));
    let mut stmt = conn
        .prepare(HERMES_QUERY)
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(HermesRow {
                session: row.get(0)?,
                title: row.get(1)?,
                cwd: row.get(2)?,
                model: row.get(3)?,
                provider: row.get(4)?,
                ts: row.get(5)?,
                content: row.get(6)?,
            })
        })
        .map_err(|e| format!("query: {e}"))?;
    Ok(rows.flatten().collect())
}
