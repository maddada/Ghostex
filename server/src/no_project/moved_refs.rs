//! A moved session's old ref keeps working.
//!
//! CDXC:Projects 2026-11-27 DECISION:
//! User, of the one-time move of home sessions into No project and of Move to project: a moved
//! session's ref changes project (`S90:P9ny1:G0ns9` becomes `S90:P5nmc:G0ns9`; its session id
//! stays unless taken), so "their threads, linked work items, Grok Bot relays and anyone messaging
//! them by the old ref" must keep working. Rows that pair a project id with a session id follow
//! the move in the database (upgrade.rs `rekey_sessions`). What quotes the old ref as text (a
//! thread's "Reply to:" line in its transcript, another computer's record of a thread here, a Grok
//! Bot relay, a work item's link) reaches gxserver as the old `projectId` + `sessionId` pair or
//! as the old ref through the `ghostex` CLI. So every move is recorded here (old pair to new pair,
//! chains collapsed), any request naming an old pair is pointed at the new one before its route
//! runs (route_http/dispatch.rs), and each moved session lists its old refs (`movedFromRefs` in
//! its runtime settings), which the CLI's selector matches (ghostex_cli/selector.rs). The old
//! session id stays allocated in its old project, so no new session ever takes the old pair.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

/// The metadata row holding every move, `{"<old project>/<old session>": "<project>/<session>"}`.
const MOVES_KEY: &str = "movedSessionRefs";
/// The runtime setting listing a session's earlier refs.
pub(crate) const MOVED_FROM_REFS_KEY: &str = "movedFromRefs";

type Pair = (String, String);

fn moves() -> &'static RwLock<HashMap<Pair, Pair>> {
    static MOVES: OnceLock<RwLock<HashMap<Pair, Pair>>> = OnceLock::new();
    MOVES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn split(text: &str) -> Option<Pair> {
    let (project, session) = text.split_once('/')?;
    Some((project.to_string(), session.to_string()))
}

fn read_stored(db: &Connection) -> HashMap<Pair, Pair> {
    let text: Option<String> = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [MOVES_KEY],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let Some(Value::Object(stored)) = text.and_then(|text| serde_json::from_str(&text).ok()) else {
        return HashMap::new();
    };
    stored
        .iter()
        .filter_map(|(from, to)| Some((split(from)?, split(to.as_str()?)?)))
        .collect()
}

/// Reads the recorded moves into memory; at gxserver start.
pub(crate) fn load(db: &Connection) {
    let stored = read_stored(db);
    if let Ok(mut moves) = moves().write() {
        *moves = stored;
    }
}

/// Records sessions moved from project `from` to `to` (`(old session id, new session id)`), and
/// adds each one's old ref to its `movedFromRefs`. Runs after the move's transaction committed.
pub(crate) fn record(
    db: &Connection,
    server_id: &str,
    from: &str,
    to: &str,
    moved: &[(String, String)],
) {
    let mut all = read_stored(db);
    for (old_id, new_id) in moved {
        let old = (from.to_string(), old_id.clone());
        let new = (to.to_string(), new_id.clone());
        // A pair that moved here before now leads to the new place too.
        for target in all.values_mut() {
            if *target == old {
                *target = new.clone();
            }
        }
        all.insert(old, new);
        let old_ref = format!("{server_id}:{from}:{old_id}");
        let _ = db.execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(
                 CASE WHEN json_valid(runtimeSettingsJson) THEN runtimeSettingsJson ELSE '{}' END,
                 '$.movedFromRefs',
                 json_insert(
                     COALESCE(json_extract(runtimeSettingsJson, '$.movedFromRefs'), json('[]')),
                     '$[#]', ?1))
             WHERE projectId = ?2 AND sessionId = ?3",
            params![old_ref, to, new_id],
        );
    }
    let stored: Map<String, Value> = all
        .iter()
        .map(|((project, session), (to_project, to_session))| {
            (
                format!("{project}/{session}"),
                Value::String(format!("{to_project}/{to_session}")),
            )
        })
        .collect();
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        params![
            MOVES_KEY,
            Value::Object(stored).to_string(),
            crate::coordinators::now_iso()
        ],
    );
    if let Ok(mut moves) = moves().write() {
        *moves = all;
    }
}

/// Points a request's old `projectId` + `sessionId` pairs (and `<x>ProjectId` + `<x>SessionId`,
/// such as a thread's coordinator) at where those sessions moved. Nothing to do on a gxserver
/// that never moved a session.
pub(crate) fn follow_moved_refs(body: &mut Value) {
    let Ok(moves) = moves().read() else {
        return;
    };
    if moves.is_empty() {
        return;
    }
    let Some(params) = body.get_mut("params").and_then(Value::as_object_mut) else {
        return;
    };
    let project_keys: Vec<String> = params
        .keys()
        .filter(|key| key.as_str() == "projectId" || key.ends_with("ProjectId"))
        .cloned()
        .collect();
    for project_key in project_keys {
        let session_key = match project_key.strip_suffix("ProjectId") {
            Some(prefix) => format!("{prefix}SessionId"),
            None => "sessionId".to_string(),
        };
        let (Some(project), Some(session)) = (
            params.get(&project_key).and_then(Value::as_str),
            params.get(&session_key).and_then(Value::as_str),
        ) else {
            continue;
        };
        if let Some((to_project, to_session)) =
            moves.get(&(project.to_string(), session.to_string()))
        {
            params.insert(project_key, Value::String(to_project.clone()));
            params.insert(session_key, Value::String(to_session.clone()));
        }
    }
}

/// Records moves made before they were recorded (gxserver builds of 2026-11-27 between the move
/// and this record): a session in No project whose zmx name still names another project
/// (`S90-P9ny1-G0ns9`, kept from where it started) came from there, and a chat Move to project
/// took in place carries where it came from (`movedFromNoProject`).
pub(crate) fn backfill(db: &Connection, server_id: &str, no_project_id: &str) {
    let known = read_stored(db);
    let Ok(mut statement) = db.prepare(
        "SELECT projectId, sessionId, zmxName, runtimeSettingsJson FROM sessions \
         WHERE projectId = ?1 OR json_extract(runtimeSettingsJson, '$.movedFromNoProject') IS NOT NULL",
    ) else {
        return;
    };
    let rows: Vec<(String, String, String, String)> = statement
        .query_map([no_project_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default();
    let session_exists = |project: &str, session: &str| {
        db.query_row(
            "SELECT 1 FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
            params![project, session],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
    };
    let mut found: Vec<(Pair, Pair)> = Vec::new();
    for (project, session, zmx_name, runtime) in rows {
        let now = (project.clone(), session.clone());
        if project == no_project_id {
            let parts: Vec<&str> = zmx_name.split('-').collect();
            if let [_, from_project, from_session] = parts.as_slice() {
                if *from_project != project {
                    found.push((
                        (from_project.to_string(), from_session.to_string()),
                        now.clone(),
                    ));
                }
            }
        }
        let moved_from = serde_json::from_str::<Value>(&runtime)
            .ok()
            .and_then(|runtime| runtime.get("movedFromNoProject").cloned());
        if let Some(moved_from) = moved_from {
            if let (Some(from_project), Some(from_session)) = (
                moved_from.get("projectId").and_then(Value::as_str),
                moved_from.get("sessionId").and_then(Value::as_str),
            ) {
                if from_project != project || from_session != session {
                    found.push(((from_project.to_string(), from_session.to_string()), now));
                }
            }
        }
    }
    for (old, new) in found {
        if known.contains_key(&old) || session_exists(&old.0, &old.1) {
            continue;
        }
        record(
            db,
            server_id,
            &old.0,
            &new.0,
            &[(old.1.clone(), new.1.clone())],
        );
    }
}
