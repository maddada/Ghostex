//! How many sessions one turn of Chief of Staff, a GBot chat or a Ghostex Help chat may start.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::coordinators::SessionKey;
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::security::Principal;

/// The most sessions one turn of these helpers may start.
pub(crate) const TURN_SESSION_CAP: usize = 6;
/// The turn a caller is in when its activity doesn't say when it began.
const FALLBACK_TURN_MS: i64 = 10 * 60 * 1000;

/// CDXC:Chief 2026-11-10 WHY:
/// On 9 Nov a Windows user asked Chief of Staff how to change a GBot's project icon and about ten Codex sessions asking the same thing filled the "Ghostex" project: every Ghostex Help chat that couldn't find the answer ran `ghostex help ask`, which started the next one (reproduced on a test gxserver: one question, 16 sessions in about ten minutes). The CLI now refuses that ask (ghostex_cli/help_ask.rs); this is the guard behind it, so no loop of `ghostex agents create`, `coordinator start-thread` or `help ask` from one of these helpers spawns sessions without end. The count is per turn (since the caller's activity turned working), kept in memory: a restart forgets it, which only ever allows more.
/// SEE-ALSO: server/src/ghostex_cli/help_ask.rs, server/src/server/route_http/sessions.rs (`/api/createAgentSession`).
pub(crate) struct TurnCreate {
    key: SessionKey,
}

fn ledger() -> &'static Mutex<HashMap<SessionKey, Vec<i64>>> {
    static LEDGER: OnceLock<Mutex<HashMap<SessionKey, Vec<i64>>>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}

/// Refuses a create when the caller is Chief of Staff, a GBot chat or a Ghostex Help chat that
/// has already started [`TURN_SESSION_CAP`] sessions this turn; otherwise returns what
/// [`record_turn_create`] counts once the session exists (`None` for every other caller).
pub(crate) fn check_turn_create_cap(
    paths: &GxserverPaths,
    repository: &DomainRepository<'_>,
    caller: &Principal,
) -> Result<Option<TurnCreate>, DomainStateError> {
    let Principal::Session {
        project_id,
        session_id,
        ..
    } = caller
    else {
        return Ok(None);
    };
    let key: SessionKey = (project_id.clone(), session_id.clone());
    let Some(helper) = helper_name(paths, repository, &key)? else {
        return Ok(None);
    };
    let now = chrono::Utc::now().timestamp_millis();
    let turn_started = repository
        .get_session(project_id, session_id)?
        .as_ref()
        .and_then(crate::zcode_turn_activity::working_since_ms)
        .unwrap_or(now - FALLBACK_TURN_MS);
    let mut ledger = ledger()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let created = ledger.entry(key.clone()).or_default();
    created.retain(|at| *at >= turn_started.min(now - FALLBACK_TURN_MS));
    let this_turn = created.iter().filter(|at| **at >= turn_started).count();
    if this_turn >= TURN_SESSION_CAP {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "{helper} has already started {TURN_SESSION_CAP} sessions for this request, the most one request may start. Wait for them to answer (`ghostex read-session-chat <ref>`), or tell the user what is left instead of starting more."
            ),
        });
    }
    Ok(Some(TurnCreate { key }))
}

/// Counts a session [`check_turn_create_cap`] allowed, now that it exists.
pub(crate) fn record_turn_create(create: Option<TurnCreate>) {
    let Some(create) = create else {
        return;
    };
    ledger()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(create.key)
        .or_default()
        .push(chrono::Utc::now().timestamp_millis());
}

/// "Chief of Staff", "This GBot" or "Ghostex Help" when the session is one of them.
fn helper_name(
    paths: &GxserverPaths,
    repository: &DomainRepository<'_>,
    key: &SessionKey,
) -> Result<Option<&'static str>, DomainStateError> {
    if crate::chief::chief_key(paths).as_ref() == Some(key) {
        return Ok(Some("Chief of Staff"));
    }
    let Some(project) = repository.get_project(&key.0)? else {
        return Ok(None);
    };
    if crate::gbots::gbots_project_bot_id(&project).is_some() {
        return Ok(Some("This GBot"));
    }
    let in_help_folder = project
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| is_ghostex_help_folder(paths, Path::new(path)));
    Ok(in_help_folder.then_some("Ghostex Help"))
}

/// Whether `folder` is the Ghostex config folder, the project Ghostex Help chats run in (Ask
/// Ghostex in the app, `ghostex help ask`).
pub(crate) fn is_ghostex_help_folder(paths: &GxserverPaths, folder: &Path) -> bool {
    same_folder(folder, &paths.app_config_dir)
}

fn same_folder(left: &Path, right: &Path) -> bool {
    let normal = |path: &Path| {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let text = path
            .to_string_lossy()
            .trim_end_matches(['/', '\\'])
            .to_string();
        // Windows folders compare without case and either slash.
        if cfg!(windows) {
            text.replace('/', "\\").to_lowercase()
        } else {
            text
        }
    };
    normal(left) == normal(right)
}
