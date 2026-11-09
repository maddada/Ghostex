//! Turning GBots off (their switch or Main Hub's) stops their agents: every five seconds this reads
//! both switches, and when GBots go off (or gxserver starts with them off) it puts every running
//! bot chat to sleep, as Chief of Staff's sweep does for Chief (server/src/chief/sweep.rs). A flip
//! either way republishes the sidebar, which carries the GBots fields only while they are on. The
//! chats, their memory and the bots stay.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

const POLL: Duration = Duration::from_secs(5);

pub(crate) fn start_gbots_sweep(state: Arc<AppState>) {
    // Chief of Staff offers keeping an eye on the user's day once (day_watch/offer.rs).
    super::day_watch::start_day_watch_offer(state.clone());
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let reader = state.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(db) = open_gxserver_database(&reader.paths) {
                super::presentation::refresh_presented_bots(&db);
            }
        })
        .await;
        let mut was_on: Option<bool> = None;
        let mut poll = crate::ticks::steady_interval(POLL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
            let on = super::gbots_on(&state.paths);
            let turned_off = !on && was_on != Some(false);
            let flipped = was_on.is_some_and(|was| was != on);
            was_on = Some(on);
            super::feature::remember_gbots_shown(on);
            if flipped {
                let publisher = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    crate::server::publish_presentation_snapshot(&publisher)
                })
                .await;
            }
            if turned_off {
                let sleeper = state.clone();
                let _ = tokio::task::spawn_blocking(move || sleep_running_chats(&sleeper)).await;
            }
        }
    });
}

/// Puts every running chat of every bot to sleep.
fn sleep_running_chats(state: &AppState) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(bots) = super::record::list_bots(&db) else {
        return;
    };
    for bot in bots {
        let Ok(chats) = super::session::bot_chats(&repository, &bot) else {
            continue;
        };
        for chat in chats {
            let running = crate::presentation::session_attributes::effective_lifecycle_state(&chat)
                == "running";
            let text = |key: &str| {
                chat.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            if running {
                super::session::sleep_chat(state, &text("projectId"), &text("sessionId"));
            }
        }
    }
}
