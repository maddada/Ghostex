//! Drives the online check (server/src/network_status/): every 2 seconds it reads the system's
//! route and, when due, probes the agents' API hosts off the async threads, publishes the answer,
//! logs each change (`networkOffline`, `networkBack`) and redraws the open chats so the Autopilot
//! pill follows.

use super::*;

use crate::network_status::{self, NetworkWatch, SystemChecker, Transition};

const TICK: Duration = Duration::from_secs(2);

pub(crate) fn start_network_status_runtime(state: Arc<AppState>) {
    let watch = Arc::new(Mutex::new(NetworkWatch::new()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let ticking = watch.clone();
            let transition = tokio::task::spawn_blocking(move || {
                let mut watch = ticking.lock().unwrap_or_else(|error| error.into_inner());
                let now = chrono::Utc::now().timestamp_millis();
                let wanted = network_status::wanted_targets(now);
                let transition = watch.tick(now, &wanted, &SystemChecker);
                network_status::publish(&watch);
                transition.map(|transition| (transition, wanted))
            })
            .await
            .ok()
            .flatten();
            if let Some((transition, wanted)) = transition {
                let changed = state.clone();
                let _ =
                    tokio::task::spawn_blocking(move || announce(&changed, transition, &wanted))
                        .await;
            }
        }
    });
}

/// BLOCKING (SQLite). Logs the change and redraws the open chats.
fn announce(state: &AppState, transition: Transition, wanted: &std::collections::BTreeSet<String>) {
    let (event, details) = match transition {
        Transition::Offline { at } => (
            "networkOffline",
            json!({
                "offlineAt": crate::coordinators::iso_from_ms(at),
                "probed": wanted,
            }),
        ),
        Transition::Back { offline_since, at } => (
            "networkBack",
            json!({
                "offlineAt": crate::coordinators::iso_from_ms(offline_since),
                "backAt": crate::coordinators::iso_from_ms(at),
                "offlineMs": at - offline_since,
            }),
        ),
    };
    let _ = state.logger.log_routine(
        crate::logging::DiagnosticLogScenario::ServerLifecycle,
        GxserverLogInput {
            level: LogLevel::Info,
            event: event.to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(details),
        },
    );
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    super::autopilot_http::republish_open_chats(state, &repository);
}
