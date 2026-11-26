//! Sends queued notes to the Grok Bot routine's webhook: one POST carries every note waiting (up to
//! [`BATCH`]), at most one POST every [`MIN_GAP`], and a failed POST is tried again later. A new
//! note waits [`GATHER`] for others, so notes sent close together go out as one POST with one
//! summary (one routine run, one message in the bot's chat).
//!
//! CDXC:GrokBot 2026-11-26 DECISION:
//! User: "Ideally batch what's left into one summary instead of one ping each". A fixed window from
//! the first waiting note, not a wait for quiet: a question waits at most a minute (plus the 20
//! seconds between POSTs), however busy the agents are. Every note joining the window rides the
//! same POST, and the body's `summary` is that POST's whole message, a line a note, so the routine
//! can say it at once (instructions.md). A retried note is already older than the window and goes
//! straight out.
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

use serde_json::{json, Value};

use super::store::{self, QueuedNote};
use crate::paths::GxserverPaths;

/// The fewest seconds between two POSTs.
const MIN_GAP: Duration = Duration::from_secs(20);
/// How long the first waiting note waits for others to join its POST.
const GATHER: Duration = Duration::from_secs(60);
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

/// The webhook's JSON body for `notes`: each note, and `summary`, all of them as one message.
pub(crate) fn body(notes: &[QueuedNote]) -> Value {
    json!({
        "source": "ghostex",
        "computer": crate::connected_computers::this_computer_name(),
        "count": notes.len(),
        "summary": summary(notes),
        "notes": notes,
    })
}

/// One line a note, with who sent it and the `ref` the bot answers it with.
fn summary(notes: &[QueuedNote]) -> String {
    let lines = notes.iter().map(|note| {
        let kind = match note.kind.as_str() {
            "ask" => "Needs you",
            "blocked" => "Blocked",
            "done" => "Done",
            "report" => "Report",
            "test" => "Test",
            _ => "Note",
        };
        let from = match (note.agent.as_deref(), note.project.as_deref()) {
            (Some(agent), Some(project)) => format!(" from {agent} ({project})"),
            (Some(agent), None) => format!(" from {agent}"),
            (None, Some(project)) => format!(" from {project}"),
            (None, None) => String::new(),
        };
        let reply = note
            .reply_to
            .as_deref()
            .map(|reference| format!(" [ref {reference}]"))
            .unwrap_or_default();
        format!("- {kind}{from}: {}{reply}", note.text.trim())
    });
    let heading = match notes.len() {
        1 => "1 note from Ghostex:".to_string(),
        count => format!("{count} notes from Ghostex, in one summary:"),
    };
    std::iter::once(heading)
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// How much longer the oldest of `notes` waits for others to join it, if it still does.
fn gather_wait(notes: &[QueuedNote], now: i64) -> Option<Duration> {
    let oldest = notes.iter().map(|note| note.at).min()?;
    let waited = u64::try_from(now.saturating_sub(oldest)).unwrap_or_default();
    GATHER
        .checked_sub(Duration::from_millis(waited))
        .filter(|wait| !wait.is_zero())
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
            format!("Grok Bot refused the key (HTTP {status}). Copy the routine's current key and connect again."),
        )),
        404 => Err(failure(
            "notFound",
            "Grok Bot doesn't know this webhook (HTTP 404). Check the routine still has its webhook, then connect again.",
        )),
        _ => Err(failure(
            "notStarted",
            format!("Grok Bot didn't start the routine (HTTP {status}). Check the routine isn't paused."),
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
        // Notes sent close together go as one POST: wait out the window, then read the queue
        // again, so every note that joined it rides along.
        if let Some(wait) = gather_wait(&batch, now_ms()) {
            pause(wait);
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn note(at: i64, kind: &str, text: &str) -> QueuedNote {
        QueuedNote {
            at,
            kind: kind.to_string(),
            text: text.to_string(),
            project: Some("Ghostex".to_string()),
            agent: Some("Fix login".to_string()),
            reply_to: Some("S90:P3lv0:G9jt7".to_string()),
        }
    }

    #[test]
    fn a_new_note_waits_for_others_and_an_old_one_goes() {
        let now = 1_000_000;
        assert_eq!(gather_wait(&[], now), None);
        assert_eq!(
            gather_wait(&[note(now - 15_000, "ask", "a")], now),
            Some(Duration::from_secs(45))
        );
        // The window runs from the oldest note: a later one doesn't push it back.
        assert_eq!(
            gather_wait(
                &[note(now - 50_000, "ask", "a"), note(now, "ask", "b")],
                now
            ),
            Some(Duration::from_secs(10))
        );
        assert_eq!(gather_wait(&[note(now - 60_000, "ask", "a")], now), None);
    }

    #[test]
    fn the_summary_is_a_line_a_note() {
        assert_eq!(
            summary(&[
                note(1, "ask", "Merge PR 12?"),
                note(2, "blocked", " Needs the API key "),
            ]),
            "2 notes from Ghostex, in one summary:\n- Needs you from Fix login (Ghostex): Merge PR 12? [ref S90:P3lv0:G9jt7]\n- Blocked from Fix login (Ghostex): Needs the API key [ref S90:P3lv0:G9jt7]"
        );
    }
}
