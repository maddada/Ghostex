//! The one-time offer: once Connectors can reach the user's Gmail or Google Calendar, Chief of
//! Staff is told once to offer keeping an eye on their day.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! The plan installs the day watch only when the user asks for it or accepts it from Chief of Staff, never on its own: it reads private mail and spends a model turn every morning. So gxserver tells Chief of Staff once, when GBots are on, Connectors is on and the workspace's Composio account has Gmail or Google Calendar connected (the list Connectors already keeps, so the look costs no network call), and nobody set up or offered the day watch before. The offer is marked when it is queued, so a restart never offers twice; the user can still ask for it any time.

use std::sync::Arc;
use std::time::Duration;

use crate::coordinators::now_iso;
use crate::domain::DomainRepository;
use crate::paths::GxserverPaths;
use crate::server::{queue_session_chat_prompt, AppState};
use crate::storage::open_gxserver_database;

/// How often the offer is looked at; the first look waits for gxserver to settle.
const LOOK_EVERY: Duration = Duration::from_secs(10 * 60);
const FIRST_LOOK: Duration = Duration::from_secs(2 * 60);

/// The apps (Composio slugs) connected in `workspace`, as Connectors last saw them.
pub(super) fn connected_apps(paths: &GxserverPaths, workspace: &str) -> Vec<String> {
    crate::connectors::composio_connected_apps(paths, workspace)
}

pub(crate) fn start_day_watch_offer(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(FIRST_LOOK) => {}
        }
        let mut poll = crate::ticks::steady_interval(LOOK_EVERY);
        loop {
            let looker = state.clone();
            let offered = tokio::task::spawn_blocking(move || offer_if_due(&looker))
                .await
                .unwrap_or(false);
            if offered {
                break;
            }
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
        }
    });
}

/// Offers the day watch when it is due (module comment); true once nothing is left to offer.
fn offer_if_due(state: &AppState) -> bool {
    if !super::super::gbots_on(&state.paths) || !crate::connectors::connectors_enabled(&state.paths)
    {
        return false;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let Ok(mut record) = super::store::read(&db) else {
        return false;
    };
    if record.offered_at.is_some() || record.set_up_at.is_some() {
        return true;
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let workspace = super::ops::watch_workspace(state, &db, &repository, None);
    let apps = connected_apps(&state.paths, &workspace);
    let names: Vec<&str> = [("gmail", "Gmail"), ("googlecalendar", "Google Calendar")]
        .into_iter()
        .filter(|(slug, _)| apps.iter().any(|app| app == slug))
        .map(|(_, name)| name)
        .collect();
    if names.is_empty() {
        return false;
    }
    let Some((project, session)) = crate::chief::chief_key(&state.paths) else {
        return false;
    };
    let Some(chief) = repository.get_session(&project, &session).ok().flatten() else {
        return false;
    };
    let open = matches!(
        crate::presentation::session_attributes::effective_lifecycle_state(&chief).as_str(),
        "running" | "sleep" | "sleeping"
    );
    if !open {
        return false;
    }
    let apps = match names.as_slice() {
        [one] => format!("{one} is"),
        _ => "Gmail and Google Calendar are".to_string(),
    };
    record.offered_at = Some(now_iso());
    if super::store::save(&db, &record).is_err() {
        return false;
    }
    let message = super::text::chief_offer_message(&apps);
    if queue_session_chat_prompt(state, &project, &session, &message, false).is_ok() {
        super::super::assign::wake_if_asleep(state, &chief);
    }
    true
}
