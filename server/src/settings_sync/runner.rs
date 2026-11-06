//! Trades entries with every connected computer: every few seconds, and right after a local edit.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::connected_computers::ConnectedComputer;
use crate::server::AppState;

use super::engine::{now_ms, with_engine};
use super::exchange::{ask, broadcast_notes, broadcast_spaces_named_apart, Asked};

/// How often every connected computer is asked when nothing changed here.
const EXCHANGE_EVERY: Duration = Duration::from_secs(15);
/// How long an edit waits for the ones right after it before they go out together.
const EDIT_SETTLE: Duration = Duration::from_millis(800);
/// A computer whose gxserver has no settings sync is asked again after this long.
const UNSUPPORTED_RETRY_MS: u64 = 10 * 60 * 1000;

fn wake() -> &'static Notify {
    static WAKE: OnceLock<Notify> = OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

/// A local edit to something that syncs: trade with the connected computers soon.
pub(crate) fn nudge_settings_sync() {
    wake().notify_one();
}

pub(crate) fn start_settings_sync(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut tick = crate::ticks::steady_interval(EXCHANGE_EVERY);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tick.tick() => {}
                () = wake().notified() => tokio::time::sleep(EDIT_SETTLE).await,
            }
            let round_state = state.clone();
            let _ = tokio::task::spawn_blocking(move || run_round(&round_state)).await;
        }
    });
}

/// One exchange with each connected computer in the group.
fn run_round(state: &AppState) {
    let computers: Vec<ConnectedComputer> = crate::connected_computers::list()
        .into_iter()
        .filter(|computer| computer.agent_access && !computer.base_url.is_empty())
        .collect();
    // With nobody to trade with, this computer's own edits are still taken in, so the entries
    // (and the baseline they are compared with) follow what the user does here.
    let request = with_engine(state, |engine| {
        engine.export()?;
        let now = now_ms();
        let due: Vec<ConnectedComputer> = computers
            .iter()
            .filter(|computer| !engine.removed(&computer.server_id))
            .filter(|computer| {
                engine
                    .store
                    .peers
                    .get(&computer.server_id)
                    .is_none_or(|peer| peer.unsupported_until_ms <= now)
            })
            .cloned()
            .collect();
        let categories = engine.enabled_categories();
        // The fingerprint each computer is asked with first, over what both sync as of last time.
        let probes: Vec<(ConnectedComputer, Option<Value>)> = due
            .into_iter()
            .map(|computer| {
                let probe = engine
                    .store
                    .peers
                    .get(&computer.server_id)
                    .filter(|peer| peer.met && !peer.categories.is_empty())
                    .map(|peer| {
                        json!({
                            "serverId": engine.server_id,
                            "name": engine.name,
                            "probe": true,
                            "categories": categories,
                            "digest": engine.digest_with(&peer.categories),
                        })
                    });
                (computer, probe)
            })
            .collect();
        Ok((
            probes,
            json!({
                "serverId": engine.server_id,
                "name": engine.name,
                "digest": engine.digest(),
                "categories": categories,
                "entries": engine.outgoing(),
            }),
        ))
    });
    let Ok((due, body)) = request else {
        return;
    };
    for (computer, probe) in due {
        // CDXC:SettingsSync 2026-11-07 WHY:
        // Skills carry their files, so the whole set going both ways every 15 seconds became megabytes. A round first sends only a fingerprint of what both computers sync; the entries go only when it differs. A gxserver from before this answers the fingerprint as an empty exchange, and the full one follows.
        if let Some(probe) = probe {
            if let Asked::Answered(answer) = ask(&computer, &probe) {
                if answer.get("same").and_then(Value::as_bool) == Some(true) {
                    let _ = with_engine(state, |engine| {
                        let record = engine
                            .store
                            .peers
                            .entry(computer.server_id.clone())
                            .or_default();
                        record.last_sync_ms = now_ms();
                        record.refused = None;
                        Ok(())
                    });
                    continue;
                }
            }
        }
        let asked = ask(&computer, &body);
        let notes = with_engine(state, |engine| {
            let now = now_ms();
            let record = engine
                .store
                .peers
                .entry(computer.server_id.clone())
                .or_default();
            record.name = computer.name.clone();
            let first = !record.met;
            match asked {
                Asked::Answered(answer) => {
                    record.last_sync_ms = now;
                    record.met = true;
                    record.refused = None;
                    let entries = answer
                        .get("entries")
                        .cloned()
                        .and_then(|entries| serde_json::from_value(entries).ok())
                        .unwrap_or_default();
                    let name = answer
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&computer.name)
                        .to_string();
                    record.categories = answer
                        .get("categories")
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter_map(|prefix| prefix.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    let notes = engine.merge(entries, first);
                    engine.welcome(&computer.server_id, &name);
                    engine.import()?;
                    Ok((notes, engine.spaces_named_apart))
                }
                Asked::Removed(message) => {
                    record.refused = Some(message);
                    Ok((Vec::new(), false))
                }
                Asked::Unsupported => {
                    record.unsupported_until_ms = now + UNSUPPORTED_RETRY_MS;
                    Ok((Vec::new(), false))
                }
                // Not reachable right now: the next round tries again.
                Asked::Failed => Ok((Vec::new(), false)),
            }
        });
        if let Ok((notes, named_apart)) = notes {
            broadcast_notes(state, &notes);
            if named_apart {
                broadcast_spaces_named_apart(state);
            }
        }
    }
}
