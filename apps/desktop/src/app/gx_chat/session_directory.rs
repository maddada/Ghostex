//! The session list the chat core names the other session from on a message between agents
//! (`Event::SessionDirectoryChanged`): every session the sidebar store knows, on every machine.
//!
//! The store lives on the UI thread and the chat host on its own (the web build runs both on the
//! page's thread), so the store publishes the list here after each pump and the host hands it to
//! every chat whose copy is older (`world.rs`). The phone builds the same list from its inventory
//! (`apps/mobile/app/src/chat/rust/sessionDirectory.ts`).

use std::sync::{Arc, Mutex};

use ghostex_gx_chat_core::KnownSession;
use ghostex_gx_core::{MachineId, MachineTabInput, PresentationStore};

/// The published list, its revision, and what it was built from.
#[derive(Default)]
struct Published {
    revision: u64,
    /// Moves only when a session's name did (`KnownSession::same_names`): what a transcript
    /// reads. The other fields are the composer's `+` picker's, live and always moving.
    names_revision: u64,
    sessions: Arc<Vec<KnownSession>>,
    /// This computer's gxserver id, which a session pill counts as home: one whose ref names
    /// another draws that computer's icon (`mention_card` in gx-chat-core).
    home_server: String,
    /// Each machine's gxserver id, by that id: which machine a session pill's computer icon is
    /// read for (`computer_icon`).
    machines_by_server: std::collections::HashMap<String, String>,
    /// Each machine's daemon revision, row count, name, whether it is connected and its gxserver id
    /// when the list was built, so a pump that moved none of them builds nothing. The id is part of
    /// it: a stream names the gxserver after a read of the cache did, at the same revision, and the
    /// home computer and the rows' refs must follow it then.
    built_from: Vec<(String, i64, usize, String, bool, String)>,
}

/// Each computer's icon drawing, by machine id, as the sidebar draws it (`gx_store_computer_marks`,
/// gx-core computer_icons.rs); empty with only this computer, and in the web build.
static COMPUTER_ICONS: Mutex<Option<std::collections::HashMap<String, &'static str>>> =
    Mutex::new(None);

/// Keeps every computer's icon drawing for the session pills, from the sidebar's marks. The web
/// build has no computer marks and never calls it.
#[cfg_attr(ghostex_web, allow(dead_code))]
pub(crate) fn set_computer_icons(icons: std::collections::HashMap<String, &'static str>) {
    *COMPUTER_ICONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(icons);
}

/// The icon of the computer whose gxserver is `server_id`: the one picked for it (the sidebar's
/// mark), else the plain computer.
///
/// CDXC:SessionChat 2026-11-14 DECISION:
/// User, of a "Message to" card's pill for a session on their Linux computer: "i believe we agreed to show the logo of the machine here? it's not showing pls fix". A session pill for a session on another computer ends with that computer's own icon, the one the sidebar's rows and computer tile draw for it (picked in Settings > Remote, else its kind's or its system's), not one plain computer icon for every computer. A computer the sidebar has no mark for keeps the plain computer icon.
pub(crate) fn computer_icon(server_id: &str) -> &'static str {
    const PLAIN: &str = "titlebar/device-desktop.svg";
    let machine = PUBLISHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .and_then(|published| published.machines_by_server.get(server_id).cloned());
    let Some(machine) = machine else {
        return PLAIN;
    };
    COMPUTER_ICONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .and_then(|icons| icons.get(&machine).copied())
        .unwrap_or(PLAIN)
}

/// The id the computer tabs and marks key a machine by.
fn machine_key(machine: &MachineId) -> &str {
    match machine {
        MachineId::Local => ghostex_gx_core::LOCAL_MACHINE_ID,
        MachineId::Remote(id) => id.as_str(),
    }
}

static PUBLISHED: Mutex<Option<Published>> = Mutex::new(None);

/// The title a session's sidebar row shows, as `tab_title` in gx-core's selectors reads it but
/// without its terminal-title fallback; empty when it has none.
fn row_title(session: &ghostex_gx_core::protocol::PresentationSession) -> String {
    let primary = session
        .primary_title
        .as_deref()
        .unwrap_or(session.title.as_str());
    // CDXC:SessionChat 2026-11-06 WHY: never the live terminal title. Claude and other agents
    // animate it while they work, and any change to a session's name settles every open chat's
    // transcript again (`session_directory_revision`, packages/gx-chat-core/src/dispatch/events.rs).
    // Measured over 90 s with two agents working, the names below never moved; the current
    // gxserver no longer streams `terminalTitle` (server/src/presentation/session_attributes.rs),
    // so only an older remote gxserver could have reached the raw title here. The phone names
    // sessions the same way (`displayTitle || title`, apps/mobile/app/src/chat/rust/sessionDirectory.ts).
    [
        session.display_title.as_deref(),
        Some(primary),
        Some(session.title.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|title| !title.is_empty())
    .unwrap_or_default()
    .to_string()
}

/// The name a machine's tab carries (`tabs`, the host's machine tabs; the web build has none).
fn machine_label(machine: &MachineId, tabs: &[MachineTabInput]) -> String {
    let id = machine_key(machine);
    tabs.iter()
        .find(|tab| tab.machine_id == id)
        .map(|tab| tab.label.trim().to_string())
        .unwrap_or_default()
}

/// Rebuilds the list from the store when a machine's rows moved, for the chat host to pick up.
/// `tabs` names each machine for the composer's `+` picker, which offers sessions on the other
/// connected computers (`session_mentions.rs` in gx-chat-core).
pub(crate) fn publish(store: &PresentationStore, tabs: &[MachineTabInput]) {
    let built_from: Vec<(String, i64, usize, String, bool, String)> = store
        .machines()
        .filter_map(|(machine, _)| {
            let loaded = store.loaded(machine)?;
            Some((
                format!("{machine:?}"),
                loaded.revision,
                loaded.session_count(),
                machine_label(machine, tabs),
                ghostex_gx_core::unified_sidebar::unified_shows_machine(store, machine),
                loaded.server_id.clone(),
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
    let mut machines_by_server = std::collections::HashMap::new();
    for (machine, _) in store.machines() {
        let Some(loaded) = store.loaded(machine) else {
            continue;
        };
        if !loaded.server_id.is_empty() {
            machines_by_server
                .entry(loaded.server_id.clone())
                .or_insert_with(|| machine_key(machine).to_string());
        }
        let computer = machine_label(machine, tabs);
        // The Unified sidebar's test: a computer whose rows are a kept copy, or whose stream is
        // down, shows no cards, and the `+` picker offers none of its sessions.
        let offline = !ghostex_gx_core::unified_sidebar::unified_shows_machine(store, machine);
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
                main_coordinator: session.is_main_coordinator,
                // The composer's `+` picker shows each session's state, newest first.
                activity: session.activity.as_str().to_string(),
                lifecycle_state: session.lifecycle_state.as_str().to_string(),
                active_at: session
                    .meaningful_activity_at
                    .clone()
                    .or_else(|| session.last_active_at.clone())
                    .unwrap_or_default(),
                computer: computer.clone(),
                project_name: loaded
                    .project(&session.project_id)
                    .map(|project| project.title.trim().to_string())
                    .unwrap_or_default(),
                offline,
            });
        }
    }
    current.built_from = built_from;
    current.machines_by_server = machines_by_server;
    current.home_server = store
        .loaded(&MachineId::Local)
        .map(|loaded| loaded.server_id.clone())
        .unwrap_or_default();
    if *current.sessions != sessions || current.revision == 0 {
        if current.revision == 0 || !KnownSession::same_names(&current.sessions, &sessions) {
            current.names_revision += 1;
        }
        current.sessions = Arc::new(sessions);
        current.revision += 1;
    }
}

/// The list and its revision; revision 0 while the store has published nothing.
pub(crate) fn current() -> (u64, Arc<Vec<KnownSession>>) {
    let (revision, _, sessions) = current_named();
    (revision, sessions)
}

/// [`current`] with the revision of the names alone between the two.
pub(super) fn current_named() -> (u64, u64, Arc<Vec<KnownSession>>) {
    let published = PUBLISHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    published
        .as_ref()
        .map(|published| {
            (
                published.revision,
                published.names_revision,
                published.sessions.clone(),
            )
        })
        .unwrap_or_default()
}

/// This computer's gxserver id, empty before the store has loaded it.
pub(crate) fn home_server() -> String {
    PUBLISHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|published| published.home_server.clone())
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
