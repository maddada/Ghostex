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
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

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

fn last_post() -> MutexGuard<'static, Option<Instant>> {
    static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Set by [`kick_and_wake`] to cut short the worker's wait between tries; [`pause`] waits on it.
fn wake_signal() -> &'static (Mutex<bool>, Condvar) {
    static WAKE: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();
    WAKE.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

/// Waits `duration`, or less when [`kick_and_wake`] wakes the worker (a new key was connected).
fn pause(duration: Duration) -> bool {
    let (lock, signal) = wake_signal();
    let woken = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (mut woken, _) = signal
        .wait_timeout_while(woken, duration, |woken| !*woken)
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut *woken)
}

/// Records that a POST to Grok Bot is being made now (the worker's, `connect`'s or `test`'s).
pub(crate) fn note_post() {
    *last_post() = Some(Instant::now());
}

/// How long until the next POST keeps [`MIN_GAP`] after the last one, if it would come too soon.
pub(crate) fn wait_before_next_post() -> Option<Duration> {
    let last = (*last_post())?;
    MIN_GAP
        .checked_sub(last.elapsed())
        .filter(|wait| !wait.is_zero())
}

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
    })
}

/// Why a POST to Grok Bot didn't start the routine: `kind` is one of `keyRefused` (401, 403),
/// `notFound` (404), `unreachable` (no connection), `notStarted` (another status) or `failed`, so
/// Settings > Grok Bot can show its own fix; `message` says it in words.
#[derive(Clone, Debug)]
pub(crate) struct PostFailure {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
}

fn failure(kind: &'static str, message: impl Into<String>) -> PostFailure {
    PostFailure {
        kind,
        message: message.into(),
    }
}

/// One POST to `url` with the key. `Ok` once Grok Bot answered 2xx (the routine started).
pub(crate) fn post(url: &str, key: &str, body: &Value) -> Result<(), PostFailure> {
    let agent: ureq::Agent = crate::http_agent::config()
        .timeout_global(Some(POST_TIMEOUT))
        // A webhook that redirects is not the address the user connected; never follow it with
        // the key.
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .post(url)
        .header("Authorization", &format!("Bearer {key}"))
        .send_json(body)
        .map_err(|error| {
            if crate::http_agent::never_connected(&error) {
                failure("unreachable", "Grok Bot could not be reached.")
            } else {
                failure("failed", format!("The call to Grok Bot failed ({error})."))
            }
        })?;
    let status = response.status().as_u16();
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(failure(
            "keyRefused",
            format!(
                "Grok Bot refused the key (HTTP {status}). Copy the routine's current key and connect again."
            ),
        )),
        404 => Err(failure(
            "notFound",
            "Grok Bot doesn't know this webhook (HTTP 404). Check the routine still has its webhook, then connect again.",
        )),
        _ => Err(failure(
            "notStarted",
            format!(
                "Grok Bot didn't start the routine (HTTP {status}). Check the routine isn't paused."
            ),
        )),
    }
}

/// Starts the delivery worker, and makes a running one stop waiting between tries and try now
/// (a new key was connected).
pub(crate) fn kick_and_wake(paths: &GxserverPaths) {
    if WORKER_RUNNING.load(Ordering::SeqCst) {
        let (lock, signal) = wake_signal();
        *lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        signal.notify_all();
    }
    kick(paths);
}

/// Starts the delivery worker unless it already runs. A running one keeps its wait, so a failing
/// webhook's retry wait still grows while agents keep sending notes.
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
            // Grok Bot Integration turned off: stop; the notes wait in the queue until it is back on.
            let on = super::feature::on(paths);
            match (connection, key) {
                (Some(connection), Some(key)) if on && !queue.is_empty() => {
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
        if let Some(wait) = wait_before_next_post() {
            std::thread::sleep(wait);
        }
        note_post();
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
                Err(failure) => {
                    connection.last_error = Some(failure.message.clone());
                    connection.last_error_at = Some(now_ms());
                }
            }
            let _ = store::write_connection(paths, &connection);
        }
        match outcome {
            Ok(()) => {
                retry = FIRST_RETRY;
                pause(MIN_GAP);
            }
            Err(_) => {
                if pause(retry) {
                    retry = FIRST_RETRY;
                } else {
                    retry = (retry * 2).min(LONGEST_RETRY);
                }
            }
        }
    }
}
