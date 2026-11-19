//! Sends queued notes to the Grok Bot routine's webhook: one POST carries every note waiting (up to
//! [`BATCH`]), at most one POST every [`MIN_GAP`], and a failed POST is tried again later.
//!
//! CDXC:GrokBot 2026-11-19 WHY:
//! A Grok Bot routine runs at most 200 times an hour (300 for all of a user's routines), every run
//! spends the user's Grok Bot usage, and Grok Bot sometimes refuses a run under a limit all its
//! users share and never retries it. So notes are queued on disk, sent together, spaced 20 seconds
//! apart (180 runs an hour at most) and retried with a growing wait until a 2xx says the run
//! started; a note older than a day is dropped, since nobody needs a day-old "needs you" ping.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use super::store::{self, QueuedNote};
use crate::paths::GxserverPaths;

/// The fewest seconds between two POSTs.
const MIN_GAP: Duration = Duration::from_secs(20);
/// The most notes one POST carries.
const BATCH: usize = 20;
/// The most notes kept waiting; the oldest go first.
pub(crate) const MAX_QUEUE: usize = 200;
/// A note older than this is dropped instead of sent.
const MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;
const FIRST_RETRY: Duration = Duration::from_secs(30);
const LONGEST_RETRY: Duration = Duration::from_secs(10 * 60);
const POST_TIMEOUT: Duration = Duration::from_secs(20);

static WORKER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Serializes every read-change-write of the queue and connection files in this process.
pub(crate) fn files_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// The webhook's JSON body for `notes`.
pub(crate) fn body(notes: &[QueuedNote]) -> Value {
    json!({
        "source": "ghostex",
        "computer": crate::connected_computers::this_computer_name(),
        "notes": notes,
        "reply": "Answer a note's agent with `ghostex agents send <ref> \"<text>\"` on this computer, only with the user's own words.",
    })
}

/// One POST to `url` with the key. `Ok` once Grok Bot answered 2xx (the routine started).
pub(crate) fn post(url: &str, key: &str, body: &Value) -> Result<(), String> {
    let agent: ureq::Agent = crate::http_agent::config()
        .timeout_global(Some(POST_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .post(url)
        .header("Authorization", &format!("Bearer {key}"))
        .send_json(body)
        .map_err(|error| {
            if crate::http_agent::never_connected(&error) {
                "Grok Bot could not be reached.".to_string()
            } else {
                format!("The call to Grok Bot failed ({error}).")
            }
        })?;
    let status = response.status().as_u16();
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(format!(
            "Grok Bot refused the key (HTTP {status}). Copy the routine's current key and run `ghostex grokbot connect` again."
        )),
        404 => Err("Grok Bot doesn't know this webhook (HTTP 404). Check the routine still has its webhook, then connect again.".to_string()),
        _ => Err(format!(
            "Grok Bot didn't start the routine (HTTP {status}). Check the routine isn't paused."
        )),
    }
}

/// Starts the delivery worker unless it already runs.
pub(crate) fn kick(paths: &GxserverPaths) {
    if WORKER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let paths = paths.clone();
    let spawned = std::thread::Builder::new()
        .name("grokbot-delivery".to_string())
        .spawn(move || run(&paths));
    if spawned.is_err() {
        WORKER_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn run(paths: &GxserverPaths) {
    let mut retry = FIRST_RETRY;
    loop {
        let (url, key, batch) = {
            let _guard = files_lock();
            let mut queue = store::read_queue(paths);
            let oldest = now_ms() - MAX_AGE_MS;
            let before = queue.len();
            queue.retain(|note| note.at >= oldest);
            if queue.len() != before {
                let _ = store::write_queue(paths, &queue);
            }
            let connection = store::read_connection(paths);
            let key = store::read_key(paths);
            match (connection, key) {
                (Some(connection), Some(key)) if !queue.is_empty() => {
                    queue.truncate(BATCH);
                    (connection.url, key, queue)
                }
                _ => {
                    // Checked and cleared under the lock that `enqueue` takes, so a note queued
                    // now finds the worker stopped and starts a new one.
                    WORKER_RUNNING.store(false, Ordering::SeqCst);
                    return;
                }
            }
        };
        let outcome = post(&url, &key, &body(&batch));
        {
            let _guard = files_lock();
            let Some(mut connection) = store::read_connection(paths) else {
                continue;
            };
            match &outcome {
                Ok(()) => {
                    let mut queue = store::read_queue(paths);
                    queue.retain(|note| !batch.contains(note));
                    let _ = store::write_queue(paths, &queue);
                    connection.last_sent_at = Some(now_ms());
                    connection.sent += batch.len() as u64;
                    connection.last_error = None;
                    connection.last_error_at = None;
                }
                Err(message) => {
                    connection.last_error = Some(message.clone());
                    connection.last_error_at = Some(now_ms());
                }
            }
            let _ = store::write_connection(paths, &connection);
        }
        match outcome {
            Ok(()) => {
                retry = FIRST_RETRY;
                std::thread::sleep(MIN_GAP);
            }
            Err(_) => {
                std::thread::sleep(retry);
                retry = (retry * 2).min(LONGEST_RETRY);
            }
        }
    }
}
