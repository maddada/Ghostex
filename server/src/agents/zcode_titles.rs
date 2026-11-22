//! ZCode session renames: gxserver writes the CLI's own SQLite store directly.
//! SEE-ALSO: server/src/external_sessions.rs (the read side of the same store),
//! server/src/agents/fork.rs (`request_session_rename`, the dispatch site),
//! server/src/server/agent_http.rs (the staged-command consumer this branch
//! bypasses for ZCode).

use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{sql_error, DomainRepository, DomainStateError};
use crate::presentation::project_session_title_projection;

/// Mirrors the 180-char cap the discovery reader applies to zcode titles, so a
/// written title never disappears on the next discovery pass.
const ZCODE_TITLE_MAX_CHARS: usize = 180;

/// `Ok(true)` when the session row was updated, `Ok(false)` when there was
/// nothing to update yet (no known ZCode session id, no store, or no row for
/// the id — ZCode creates the row on a session's first prompt).
pub(crate) fn write_zcode_custom_session_title(
    zcode_home: &Path,
    zcode_session_id: Option<&str>,
    title: &str,
) -> Result<bool, rusqlite::Error> {
    let Some(zcode_session_id) = zcode_session_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(false);
    };
    let trimmed_title: String = title.trim().chars().take(ZCODE_TITLE_MAX_CHARS).collect();
    if trimmed_title.is_empty() {
        return Ok(false);
    }
    let db_path = zcode_home.join("cli").join("db").join("db.sqlite");
    if !db_path.is_file() {
        return Ok(false);
    }
    let connection = Connection::open(&db_path)?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    let updated = connection.execute(
        "UPDATE session \
         SET title = ?, title_source = 'custom', time_title_updated = ? \
         WHERE id = ?",
        rusqlite::params![
            trimmed_title,
            chrono::Utc::now().timestamp_millis(),
            zcode_session_id
        ],
    )?;
    Ok(updated == 1)
}

pub(crate) fn zcode_session_rename(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    session: &Value,
    zcode_session_id: Option<&str>,
    params: &Map<String, Value>,
    title: &str,
    home_dir: &Path,
) -> Result<Value, DomainStateError> {
    /*
    CDXC:SessionTitles 2026-09-17 WHY:
    ZCode's command registry has no rename slash command, so the staged
    `/rename <title>` the normal agent flow submits prints `Unknown command`
    and the pending metadata never resolves. ZCode keeps per-session titles in
    its own SQLite store and its title generator permanently skips rows whose
    `title_source` is `custom`, so the rename writes that store directly — the
    same write ZCode's own rename performs — and reports
    shouldSendAgentRenameCommand:false so no command reaches the pty. When the
    ZCode session row does not exist yet, the title still lands in Ghostex's
    own session record, like a non-agent rename.
    */
    let title: String = title.trim().chars().take(ZCODE_TITLE_MAX_CHARS).collect();
    let mut runtime_settings = object_field(session, "runtimeSettings");
    runtime_settings.insert(
        "titleSource".to_string(),
        json!(read_text(params, "titleSource").unwrap_or_else(|| "user".to_string())),
    );
    let mut update = lifecycle_update(lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    update.insert("title".to_string(), Value::String(title.clone()));
    let updated = repository.update_session(&update)?;
    let title_applied_in_zcode =
        write_zcode_custom_session_title(&home_dir.join(".zcode"), zcode_session_id, &title)
            .map_err(sql_error)?;
    Ok(json!({
        "changed": true,
        "pendingAgentMetadata": false,
        "projection": project_session_title_projection(&updated),
        "reason": if title_applied_in_zcode {
            "zcode-custom-title-applied"
        } else {
            "zcode-local-title-applied"
        },
        "session": updated,
        "shouldSendAgentRenameCommand": false,
    }))
}
