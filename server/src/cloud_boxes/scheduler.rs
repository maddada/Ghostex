//! The Cloud Boxes background work, once a minute while anything can have boxes: reconcile the
//! registry with the providers (idle pauses and self-stops show up here), the archive rule (Q1012:
//! a box whose sessions are all archived is stopped now and deleted after 7 days, unless it holds
//! unpushed work, D13), expiring "Previous" snapshots (D9), and reading the spend every few
//! minutes. Idle detection itself is the providers' (Instabox) and box mode's (Boat).

use std::sync::Arc;
use std::time::Duration;

use ghostex_gx_protocol::cloud_box::{
    setting_keys, CloudBoxState, DEFAULT_DELETE_STOPPED_AFTER_DAYS,
};
use serde_json::Value;

use super::{events, lifecycle, reconcile, registry, snapshots, spend, store};
use crate::server::AppState;

const TICK: Duration = Duration::from_secs(60);
const FIRST_TICK: Duration = Duration::from_secs(20);
/// Spend is read every this many ticks.
const SPEND_EVERY_TICKS: u64 = 5;
/// Deleted rows stay listed this long.
const DELETED_ROW_KEEP_SECONDS: i64 = 30 * 24 * 3600;

pub(crate) fn start_cloud_boxes_scheduler(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut tick: u64 = 0;
        let mut wait = FIRST_TICK;
        loop {
            tokio::select! {
                _ = shutdown.recv() => return,
                _ = tokio::time::sleep(wait) => {}
            }
            wait = TICK;
            let tick_state = state.clone();
            let read_spend = tick % SPEND_EVERY_TICKS == 0;
            tick += 1;
            let _ = tokio::task::spawn_blocking(move || run_tick(&tick_state, read_spend)).await;
        }
    });
}

fn delete_after_days(state: &AppState) -> i64 {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| {
            settings
                .get(setting_keys::DELETE_STOPPED_AFTER_DAYS)
                .and_then(Value::as_u64)
        })
        .filter(|days| *days > 0)
        .unwrap_or(DEFAULT_DELETE_STOPPED_AFTER_DAYS) as i64
}

fn run_tick(state: &AppState, read_spend: bool) {
    if reconcile::providers_to_check(state).is_empty() {
        return;
    }
    reconcile::reconcile_all(state);
    archive_rule(state);
    delete_due(state);
    snapshots::expire_previous(state);
    if read_spend {
        spend::refresh(state);
    }
    let _ = store::prune_deleted_boxes(
        &state.paths,
        &super::time::iso_in(-DELETED_ROW_KEEP_SECONDS),
    );
}

/// Q1012: a box whose sessions are all archived is stopped now (its unpushed work recorded first)
/// and gets its delete date.
///
/// CDXC:CloudBoxes 2026-11-02 DECISION: the user chose "stop then delete 7d" for a box whose session is archived (Q1012): it stops at once and is deleted after the days in "Delete stopped boxes after" (7 by default). A box another open session still uses is never stopped.
fn archive_rule(state: &AppState) {
    let days = delete_after_days(state);
    for stored in store::list_boxes(&state.paths).unwrap_or_default() {
        let row = &stored.row;
        let all_archived =
            !row.sessions.is_empty() && row.sessions.iter().all(|session| session.archived);
        if !all_archived || row.delete_after.is_some() || registry::is_starting(&row.box_id) {
            continue;
        }
        if matches!(
            row.state,
            CloudBoxState::Deleted | CloudBoxState::Deleting | CloudBoxState::Failed
        ) {
            continue;
        }
        if row.state != CloudBoxState::Stopped && lifecycle::stop(state, &row.box_id).is_err() {
            continue;
        }
        if let Ok(row) = registry::update(&state.paths, &row.box_id, |stored| {
            stored.row.delete_after = Some(super::time::iso_in(days * 24 * 3600));
        }) {
            events::broadcast_box(state, &row);
        }
    }
}

/// Deletes boxes whose delete date passed, unless they hold (or may hold) unpushed work, which
/// keeps them with a note (D13).
fn delete_due(state: &AppState) {
    for stored in store::list_boxes(&state.paths).unwrap_or_default() {
        let row = &stored.row;
        let due = row
            .delete_after
            .as_deref()
            .is_some_and(super::time::is_past);
        if !due || row.state == CloudBoxState::Deleted {
            continue;
        }
        if row.has_unpushed_work == Some(false) {
            let _ = lifecycle::delete(state, &row.box_id, true);
            continue;
        }
        let note = "Has unpushed work: push or delete anyway.".to_string();
        if row.state_detail.as_deref() != Some(note.as_str()) {
            if let Ok(row) = registry::update(&state.paths, &row.box_id, |stored| {
                stored.row.state_detail = Some(note.clone());
            }) {
                events::broadcast_box(state, &row);
            }
        }
    }
}
