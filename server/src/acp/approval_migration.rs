//! One-time cleanup of the `ask` approval level 12.0 saved on every Native session by itself.
//!
//! CDXC:NativeApprovals 2026-11-25 DECISION:
//! Brief (12.1.1 patch, Discord report "with 12.0 it keeps asking permission for every single tool call since i turned it to native mode"): 12.0 saved `acpApprovalLevel: ask` on every Native session it started or switched (the "Turn on Native mode" bulk switch included), and a saved level wins over settings on resume, so those chats and coordinator threads kept asking with Accept all or a per-agent Full permissions on. #746 (e1ca9bf43) only filled sessions with no saved level. On the first start of this build, [`clear_automatic_ask_once`] removes every saved `ask` that the Mode menu did not set, so the session follows the user's settings again; when those settings name a level (Accept all or per-agent Full permissions, per-agent Auto, a bypass flag in the agent's command) that level is saved in its place, so a Native host still running from before the update is handed it when it reconnects (`pi_chat_channel.rs` sends the saved level to a newly registered host) instead of waiting for the chat to restart. Kept: a level picked in the Mode menu from this build on (`acpApprovalLevelPicked`, set by `channel.rs` `set_approval_level`), every level other than `ask`, and chats that always start in Ask (read-only threads, side chats, GBot chats, whose level comes from the bot). 12.0 and 12.1.0 kept no record of a Mode-menu pick, so an Ask picked there before this build cannot be told apart and follows settings like the rest; the user picks it again once.
//! CDXC:NativeApprovals 2026-11-25 WHY: GitHub #750, "Sessions the user never touched still report `mode: ask` after the update to 12.1.1". The cleanup above read the agent's settings from the session's own project, and a project added after the last agent edit keeps an empty agent list (launch_plan.rs `project_with_global_agent_list`), so a bypass flag or per-agent Full permissions was not seen there: the `ask` was removed and nothing saved, and a host still running from before kept asking until it restarted. Both passes now read the global agent list as a new session does, and [`fill_settings_level_once`] gives, once, every Native session with no saved level the level its settings name, which is the level its next start uses anyway.
//! SEE-ALSO: server/src/server/foreground.rs (runs it at startup), server/src/acp/approval.rs `resolve_explicit_starting_level`.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::paths::GxserverPaths;

/// Written in gxserver's state folder once the cleanup has run, so it runs once per machine.
const DONE_FILE: &str = "native-automatic-ask-cleared";
/// Written once [`fill_settings_level_once`] has run.
const FILL_DONE_FILE: &str = "native-settings-level-filled";

type SessionRow = (String, String, Option<String>, String, Option<String>);

/// Clear 12.0's automatic `ask` levels once; returns `(cleared, given a level from settings)`,
/// or `None` when it already ran or could not read the database (it then tries again next start).
pub(crate) fn clear_automatic_ask_once(
    paths: &GxserverPaths,
    server_id: &str,
) -> Option<(usize, usize)> {
    let done = paths.app_state_dir.join(DONE_FILE);
    if done.exists() {
        return None;
    }
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    let counts = clear_automatic_ask(&db, server_id).ok()?;
    let _ = std::fs::write(&done, b"12.1.1\n");
    Some(counts)
}

/// Give every Native session with no saved level the level its settings name, once; returns how
/// many were given one, or `None` when it already ran or could not read the database.
pub(crate) fn fill_settings_level_once(paths: &GxserverPaths, server_id: &str) -> Option<usize> {
    let done = paths.app_state_dir.join(FILL_DONE_FILE);
    if done.exists() {
        return None;
    }
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    let filled = fill_settings_level(&db, server_id).ok()?;
    let _ = std::fs::write(&done, b"12.2.0\n");
    Some(filled)
}

fn session_rows(db: &Connection, level_filter: &str) -> rusqlite::Result<Vec<SessionRow>> {
    let mut statement = db.prepare(&format!(
        "SELECT projectId, sessionId, agentId, runtimeSettingsJson, launchSettingsJson FROM sessions \
         WHERE json_valid(runtimeSettingsJson) \
           AND json_extract(runtimeSettingsJson, '$.acpApprovalLevel') {level_filter}"
    ))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Resolves the starting level a Native session's settings name, reading the agent from the same
/// global agent list a new session launches from.
struct SettingsLevels<'a> {
    db: &'a Connection,
    repository: DomainRepository<'a>,
    settings: Map<String, Value>,
    projects: std::collections::HashMap<String, Value>,
}

impl<'a> SettingsLevels<'a> {
    fn new(db: &'a Connection, server_id: &'a str) -> Self {
        Self {
            db,
            repository: DomainRepository::new(db, server_id),
            settings: crate::agents::read_agent_settings(db).unwrap_or_default(),
            projects: Default::default(),
        }
    }

    fn level(
        &mut self,
        project_id: &str,
        agent_id: Option<String>,
        runtime: &Map<String, Value>,
        launch_json: Option<&str>,
    ) -> Option<super::AcpApprovalLevel> {
        let session_view = json!({ "runtimeSettings": Value::Object(runtime.clone()) });
        if !super::session_is_acp(&session_view) {
            return None;
        }
        let launch = launch_json
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        let agent_id = agent_id.unwrap_or_default();
        if !self.projects.contains_key(project_id) {
            let project = self
                .repository
                .get_project(project_id)
                .ok()
                .flatten()
                .unwrap_or(Value::Null);
            let project =
                crate::agents::project_with_global_agent_list(self.db, &project).unwrap_or(project);
            self.projects.insert(project_id.to_string(), project);
        }
        let agent_config = crate::agents::resolve_project_agent_config(
            &self.projects[project_id],
            &agent_id,
            Some(&launch),
        );
        let family = runtime
            .get(super::SESSION_ACP_FAMILY_KEY)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(agent_id);
        super::approval::resolve_explicit_starting_level(
            &family,
            runtime,
            &agent_config,
            &launch,
            &self.settings,
        )
    }
}

fn clear_automatic_ask(db: &Connection, server_id: &str) -> rusqlite::Result<(usize, usize)> {
    let rows = session_rows(db, "= 'ask'")?;
    if rows.is_empty() {
        return Ok((0, 0));
    }
    let mut levels = SettingsLevels::new(db, server_id);
    let (mut cleared, mut resolved) = (0usize, 0usize);
    for (project_id, session_id, agent_id, runtime_json, launch_json) in rows {
        let Ok(Value::Object(mut runtime)) = serde_json::from_str::<Value>(&runtime_json) else {
            continue;
        };
        if keeps_saved_ask(&runtime) {
            continue;
        }
        runtime.remove(super::SESSION_ACP_APPROVAL_LEVEL_KEY);
        if let Some(level) = levels.level(&project_id, agent_id, &runtime, launch_json.as_deref()) {
            runtime.insert(
                super::SESSION_ACP_APPROVAL_LEVEL_KEY.to_string(),
                json!(level.as_str()),
            );
            resolved += 1;
        }
        // `updatedAt` stays as it is: this is not the user's activity and must not reorder lists.
        let changed = db.execute(
            "UPDATE sessions SET runtimeSettingsJson = ?3 WHERE projectId = ?1 AND sessionId = ?2 \
             AND json_extract(runtimeSettingsJson, '$.acpApprovalLevel') = 'ask'",
            rusqlite::params![project_id, session_id, Value::Object(runtime).to_string()],
        )?;
        cleared += changed;
    }
    Ok((cleared, resolved))
}

fn fill_settings_level(db: &Connection, server_id: &str) -> rusqlite::Result<usize> {
    let rows = session_rows(db, "IS NULL")?;
    let mut levels = SettingsLevels::new(db, server_id);
    let mut filled = 0usize;
    for (project_id, session_id, agent_id, runtime_json, launch_json) in rows {
        let Ok(Value::Object(mut runtime)) = serde_json::from_str::<Value>(&runtime_json) else {
            continue;
        };
        if keeps_saved_ask(&runtime) {
            continue;
        }
        let Some(level) = levels.level(&project_id, agent_id, &runtime, launch_json.as_deref())
        else {
            continue;
        };
        runtime.insert(
            super::SESSION_ACP_APPROVAL_LEVEL_KEY.to_string(),
            json!(level.as_str()),
        );
        // `updatedAt` stays as it is, as in `clear_automatic_ask`.
        filled += db.execute(
            "UPDATE sessions SET runtimeSettingsJson = ?3 WHERE projectId = ?1 AND sessionId = ?2 \
             AND json_extract(runtimeSettingsJson, '$.acpApprovalLevel') IS NULL",
            rusqlite::params![project_id, session_id, Value::Object(runtime).to_string()],
        )?;
    }
    Ok(filled)
}

/// A saved `ask` the cleanup keeps: one the Mode menu set, or a chat that always starts in Ask.
fn keeps_saved_ask(runtime: &Map<String, Value>) -> bool {
    runtime
        .get(super::approval::SESSION_ACP_APPROVAL_LEVEL_PICKED_KEY)
        .and_then(Value::as_bool)
        == Some(true)
        || crate::coordinators::is_read_only_thread(Some(runtime))
        || crate::agents::is_gbot_chat(Some(runtime))
        || crate::side_chats::parent_of(&json!({ "runtimeSettings": runtime })).is_some()
}
