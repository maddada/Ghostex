//! One-time startup migration for Chief's older default titles.

use std::sync::Arc;

use serde_json::{json, Map};

use crate::server::AppState;

pub(crate) fn start_chief_sweep(state: Arc<AppState>) {
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            rename_old_title(&state);
            unpin_earlier_chats(&state);
        })
        .await;
    });
}

/// The metadata row that says the rename below has run on this computer. A new key each time the
/// name changes, so the rename runs once more ("chiefTitlesRenamed" was the "Ask Ghostex AI" pass;
/// CDXC:Chief 2026-11-28 in mod.rs renamed the chat to "Ghostex AI").
const TITLES_RENAMED_KEY: &str = "chiefTitlesRenamedGhostexAi";
/// The metadata row that says earlier Chief chats were unpinned once on this computer.
const EARLIER_UNPINNED_KEY: &str = "chiefEarlierUnpinned";

/// Once per computer, at gxserver start: a Chief chat still titled with a name Ghostex gave it under
/// an earlier name ("Chief", "Main Hub", "Chief of Staff", "Ask Ghostex AI", or an earlier chat's
/// "Ask Ghostex AI · 8 Nov, 13:30") takes the current name. A chat the user renamed is left alone, whatever its
/// title, and so is any other title.
///
/// WHY: it runs once (the `chiefTitlesRenamed` row), so a user who later names a chat "Chief of
/// Staff" keeps it (review of 55635e490: it ran at every start and undid such a rename).
fn rename_old_title(state: &AppState) {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let ran = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [TITLES_RENAMED_KEY],
            |row| row.get::<_, String>(0),
        )
        .is_ok();
    if ran {
        return;
    }
    if let Some(record) = super::read_record(&db) {
        let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
        let chats = std::iter::once((record.project_id.clone(), record.session_id.clone())).chain(
            record
                .earlier
                .iter()
                .map(|chat| (chat.project_id.clone(), chat.session_id.clone())),
        );
        for (project_id, session_id) in chats {
            let Some(session) = repository
                .get_session(&project_id, &session_id)
                .ok()
                .flatten()
            else {
                continue;
            };
            let renamed_by_user = session
                .pointer("/runtimeSettings/pendingAgentTitleRequestTitleSource")
                .and_then(serde_json::Value::as_str)
                == Some("user");
            let Some(new_title) = session
                .get("title")
                .and_then(serde_json::Value::as_str)
                .filter(|_| !renamed_by_user)
                .and_then(renamed_title)
            else {
                continue;
            };
            let mut params = Map::new();
            params.insert("projectId".into(), json!(project_id));
            params.insert("sessionId".into(), json!(session_id));
            params.insert("title".into(), json!(new_title));
            if repository.update_session(&params).is_ok() {
                let _ = crate::server::schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
                );
            }
        }
    }
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, 'true', ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![TITLES_RENAMED_KEY, crate::coordinators::now_iso()],
    );
}

/// Once per computer, at gxserver start: earlier Chief chats (like "Ghostex AI · 21 Nov,
/// 04:33") that carried a leftover `isPinned` from when they were active Chief are unpinned so
/// they move out of Pinned into Main Hub's Chats section. Because this runs once, any earlier chat
/// the user pins themselves afterwards stays pinned.
fn unpin_earlier_chats(state: &AppState) {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let ran = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [EARLIER_UNPINNED_KEY],
            |row| row.get::<_, String>(0),
        )
        .is_ok();
    if ran {
        return;
    }
    if let Some(record) = super::read_record(&db) {
        let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
        for chat in &record.earlier {
            let Some(session) = repository
                .get_session(&chat.project_id, &chat.session_id)
                .ok()
                .flatten()
            else {
                continue;
            };
            if session.get("isPinned").and_then(serde_json::Value::as_bool) != Some(true) {
                continue;
            }
            let mut params = Map::new();
            params.insert("projectId".into(), json!(chat.project_id));
            params.insert("sessionId".into(), json!(chat.session_id));
            params.insert("isPinned".into(), json!(false));
            if repository.update_session(&params).is_ok() {
                let _ = crate::server::schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &chat.project_id,
                    &chat.session_id,
                );
            }
        }
    }
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, 'true', ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![EARLIER_UNPINNED_KEY, crate::coordinators::now_iso()],
    );
}

/// The current name for a title Ghostex gave a Chief chat under an earlier name: one of the old
/// names exactly, or an earlier chat's "<old name> · 8 Nov, 13:30" (the day `retitle_earlier` in
/// session.rs writes). `None` for any other title.
fn renamed_title(title: &str) -> Option<String> {
    if super::OLD_CHIEF_TITLES.contains(&title) {
        return Some(super::CHIEF_TITLE.to_string());
    }
    super::OLD_CHIEF_TITLES.iter().find_map(|old| {
        let started = title.strip_prefix(old)?.strip_prefix(" \u{b7} ")?;
        chrono::NaiveTime::parse_from_str(started.split(", ").nth(1)?, "%H:%M").ok()?;
        let day = started.split(", ").next()?;
        let mut parts = day.split(' ');
        let (date, month) = (parts.next()?, parts.next()?);
        (parts.next().is_none()
            && date
                .parse::<u32>()
                .is_ok_and(|date| (1..=31).contains(&date))
            && [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ]
            .contains(&month)
            && started.split(", ").count() == 2)
            .then(|| format!("{} \u{b7} {started}", super::CHIEF_TITLE))
    })
}
