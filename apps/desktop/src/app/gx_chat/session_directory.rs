//! The session list the chat core names the other session from on a message between agents
//! (`Event::SessionDirectoryChanged`): every session the sidebar store knows, on every machine.
//!
//! The store lives on the UI thread and the chat host on its own (the web build runs both on the
//! page's thread), so the store publishes the list here after each pump and the host hands it to
//! every chat whose copy is older (`world.rs`). The phone builds the same list from its inventory
//! (`apps/mobile/app/src/chat/rust/sessionDirectory.ts`).

use std::sync::{Arc, Mutex};

use ghostex_gx_chat_core::KnownSession;
use ghostex_gx_core::PresentationStore;

/// The published list, its revision, and what it was built from.
#[derive(Default)]
struct Published {
    revision: u64,
    sessions: Arc<Vec<KnownSession>>,
    /// Each machine's daemon revision and row count when the list was built, so a pump that moved
    /// no row builds nothing.
    built_from: Vec<(String, i64, usize)>,
}

static PUBLISHED: Mutex<Option<Published>> = Mutex::new(None);

/// The title a session's sidebar row shows, as `tab_title` in gx-core's selectors reads it; empty
/// when it has none.
fn row_title(session: &ghostex_gx_core::protocol::PresentationSession) -> String {
    let primary = session
        .primary_title
        .as_deref()
        .unwrap_or(session.title.as_str());
    [
        session.display_title.as_deref(),
        Some(primary),
        session.terminal_title.as_deref(),
        Some(session.title.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|title| !title.is_empty())
    .unwrap_or_default()
    .to_string()
}

/// Rebuilds the list from the store when a machine's rows moved, for the chat host to pick up.
pub(crate) fn publish(store: &PresentationStore) {
    let built_from: Vec<(String, i64, usize)> = store
        .machines()
        .filter_map(|(machine, _)| {
            let loaded = store.loaded(machine)?;
            Some((
                format!("{machine:?}"),
                loaded.revision,
                loaded.session_count(),
            ))
        })
        .collect();
    let mut published = PUBLISHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let current = published.get_or_insert_with(Published::default);
    if current.built_from == built_from && current.revision > 0 {
        return;
    }
    let mut sessions = Vec::new();
    for (machine, _) in store.machines() {
        let Some(loaded) = store.loaded(machine) else {
            continue;
        };
        for session in loaded.server_sessions() {
            sessions.push(KnownSession {
                global_ref: if loaded.server_id.is_empty() {
                    String::new()
                } else {
                    format!(
                        "{}:{}:{}",
                        loaded.server_id, session.project_id, session.session_id
                    )
                },
                project_id: session.project_id.clone(),
                session_id: session.session_id.clone(),
                title: row_title(session),
                agent_id: session.agent_id.clone().unwrap_or_default(),
                agent_name: session.agent_name.clone().unwrap_or_default(),
                // The same test as the sidebar's crown (`is_coordinator` in gx-core's rows).
                coordinator: session.coordinator_role.as_deref() == Some("coordinator"),
                // The composer's `+` picker shows each session's state, newest first.
                activity: session.activity.as_str().to_string(),
                lifecycle_state: session.lifecycle_state.as_str().to_string(),
                active_at: session
                    .meaningful_activity_at
                    .clone()
                    .or_else(|| session.last_active_at.clone())
                    .unwrap_or_default(),
            });
        }
    }
    current.built_from = built_from;
    if *current.sessions != sessions || current.revision == 0 {
        current.sessions = Arc::new(sessions);
        current.revision += 1;
    }
}

/// The list and its revision; revision 0 while the store has published nothing.
pub(super) fn current() -> (u64, Arc<Vec<KnownSession>>) {
    let published = PUBLISHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    published
        .as_ref()
        .map(|published| (published.revision, published.sessions.clone()))
        .unwrap_or_default()
}

/// The session a composer mention names (`[<title>](<Global Ref>)`), for its chip's agent logo.
pub(crate) fn find(global_ref: &str) -> Option<KnownSession> {
    let target = ghostex_gx_chat_core::protocol::parse_global_session_ref(global_ref.trim())?;
    current()
        .1
        .iter()
        .find(|session| {
            session.project_id == target.project_id && session.session_id == target.session_id
        })
        .cloned()
}
