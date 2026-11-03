//! The pending secret requests and their answers. A request lives in memory only: it belongs to
//! the `ghostex secret request` call that is waiting for it, so a gxserver restart ends both.
//! A saved value goes straight to an owner-only file and is never kept here, logged, or sent to
//! any client.

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Condvar, LazyLock, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rand::Rng;
use serde::Serialize;

use crate::paths::GxserverPaths;

/// How long a request waits for the user unless the agent asks for another limit.
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub(crate) const MAX_TIMEOUT: Duration = Duration::from_secs(60 * 60);
pub(crate) const MIN_TIMEOUT: Duration = Duration::from_secs(10);
/// A saved file is deleted this long after it was written.
const FILE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// The waiting CLI asks again at least this often; a request nobody has asked about for longer
/// belongs to an agent that was interrupted or killed, and its card closes.
pub(crate) const HEARTBEAT_GRACE: Duration = Duration::from_secs(75);
/// Answered requests stay listed this long so a second client sees the card close, not vanish
/// mid-typing without a word.
const ANSWERED_LINGER: Duration = Duration::from_secs(30);
const MAX_LABEL_CHARS: usize = 120;
const MAX_REASON_CHARS: usize = 1000;
const MAX_PLACEHOLDER_CHARS: usize = 120;
pub(crate) const MAX_SECRET_BYTES: usize = 64 * 1024;
const MAX_PENDING_PER_SESSION: usize = 4;
const FILES_DIR: &str = "secret-requests";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SecretRequestStatus {
    Pending,
    Saved,
    Declined,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Debug)]
struct SecretRequest {
    id: String,
    project_id: String,
    session_id: String,
    label: String,
    reason: String,
    placeholder: Option<String>,
    accepts_reference: bool,
    created_at_ms: i64,
    expires_at_ms: i64,
    last_heard_ms: i64,
    status: SecretRequestStatus,
    answered_at_ms: Option<i64>,
    file: Option<PathBuf>,
}

/// What a chat client draws: never the value, never the file path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecretRequestView {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) placeholder: Option<String>,
    pub(crate) status: SecretRequestStatus,
    pub(crate) created_at_ms: i64,
    pub(crate) expires_at_ms: i64,
    /// A 1Password provider was set up when the agent asked, so the card may say an `op://`
    /// reference works too.
    pub(crate) accepts_reference: bool,
}

/// What the waiting CLI gets back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WaitOutcome {
    pub(crate) status: SecretRequestStatus,
    pub(crate) file: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SecretRequestError {
    Invalid(&'static str),
    NotFound,
    AlreadyAnswered,
    Ended,
    TooMany,
    StoreFailed,
}

impl SecretRequestError {
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::Invalid(message) => message,
            Self::NotFound => "This secret request no longer exists.",
            Self::AlreadyAnswered => "This secret request was already answered.",
            Self::Ended => "The agent that asked stopped waiting, so this secret was not saved.",
            Self::TooMany => "This session already has several secret requests waiting. Answer or decline those first.",
            Self::StoreFailed => "Could not save the secret privately. Try again.",
        }
    }
}

pub(crate) struct NewSecretRequest {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) label: String,
    pub(crate) reason: String,
    pub(crate) placeholder: Option<String>,
    pub(crate) accepts_reference: bool,
    pub(crate) timeout: Duration,
}

pub(crate) enum SecretAnswer {
    Save(String),
    Decline,
}

#[derive(Default)]
struct Registry {
    requests: HashMap<String, SecretRequest>,
}

static REGISTRY: LazyLock<(Mutex<Registry>, Condvar)> =
    LazyLock::new(|| (Mutex::new(Registry::default()), Condvar::new()));

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn random_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn clean_text(value: &str, max_chars: usize) -> String {
    let collapsed: String = value
        .chars()
        .map(|ch| {
            if ch.is_control() && ch != '\n' {
                ' '
            } else {
                ch
            }
        })
        .collect();
    collapsed.trim().chars().take(max_chars).collect()
}

/// Ends requests nobody waits for any more; returns whether anything changed.
fn settle(registry: &mut Registry, now: i64) -> bool {
    let mut changed = false;
    registry.requests.retain(|_, request| {
        if request.status == SecretRequestStatus::Pending {
            if now >= request.expires_at_ms {
                request.status = SecretRequestStatus::TimedOut;
                request.answered_at_ms = Some(now);
                changed = true;
            } else if now - request.last_heard_ms > HEARTBEAT_GRACE.as_millis() as i64 {
                request.status = SecretRequestStatus::Cancelled;
                request.answered_at_ms = Some(now);
                changed = true;
            }
        }
        let keep = request.status == SecretRequestStatus::Pending
            || request
                .answered_at_ms
                .is_some_and(|at| now - at <= ANSWERED_LINGER.as_millis() as i64)
            // An answered request the CLI has not collected yet stays until its own deadline.
            || (request.status == SecretRequestStatus::Saved && now < request.expires_at_ms);
        if !keep {
            changed = true;
        }
        keep
    });
    changed
}

pub(crate) fn create(input: NewSecretRequest) -> Result<String, SecretRequestError> {
    let label = clean_text(&input.label, MAX_LABEL_CHARS);
    if label.is_empty() {
        return Err(SecretRequestError::Invalid(
            "Say what you need with --label, e.g. --label \"Cloudflare API token\".",
        ));
    }
    let reason = clean_text(&input.reason, MAX_REASON_CHARS);
    if reason.is_empty() {
        return Err(SecretRequestError::Invalid(
            "Say what it is for and where the user gets it with --reason.",
        ));
    }
    let placeholder = input
        .placeholder
        .map(|value| clean_text(&value, MAX_PLACEHOLDER_CHARS))
        .filter(|value| !value.is_empty());
    let timeout = input.timeout.clamp(MIN_TIMEOUT, MAX_TIMEOUT);
    let now = now_ms();
    let mut registry = registry();
    settle(&mut registry, now);
    let waiting = registry
        .requests
        .values()
        .filter(|request| {
            request.status == SecretRequestStatus::Pending
                && request.project_id == input.project_id
                && request.session_id == input.session_id
        })
        .count();
    if waiting >= MAX_PENDING_PER_SESSION {
        return Err(SecretRequestError::TooMany);
    }
    let id = random_id();
    registry.requests.insert(
        id.clone(),
        SecretRequest {
            id: id.clone(),
            project_id: input.project_id,
            session_id: input.session_id,
            label,
            reason,
            placeholder,
            accepts_reference: input.accepts_reference,
            created_at_ms: now,
            expires_at_ms: now + timeout.as_millis() as i64,
            last_heard_ms: now,
            status: SecretRequestStatus::Pending,
            answered_at_ms: None,
            file: None,
        },
    );
    Ok(id)
}

/// The session `id` belongs to, for announcing its change.
pub(crate) fn session_of(id: &str) -> Option<(String, String)> {
    registry()
        .requests
        .get(id)
        .map(|request| (request.project_id.clone(), request.session_id.clone()))
}

/// The requests a session's chat shows, oldest first.
pub(crate) fn list_for_session(project_id: &str, session_id: &str) -> Vec<SecretRequestView> {
    let mut registry = registry();
    if settle(&mut registry, now_ms()) {
        REGISTRY.1.notify_all();
    }
    let mut views: Vec<SecretRequestView> = registry
        .requests
        .values()
        .filter(|request| request.project_id == project_id && request.session_id == session_id)
        .map(|request| SecretRequestView {
            id: request.id.clone(),
            label: request.label.clone(),
            reason: request.reason.clone(),
            placeholder: request.placeholder.clone(),
            status: request.status,
            created_at_ms: request.created_at_ms,
            expires_at_ms: request.expires_at_ms,
            accepts_reference: request.accepts_reference,
        })
        .collect();
    views.sort_by(|left, right| {
        left.created_at_ms
            .cmp(&right.created_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    views
}

/// Records the user's answer. A saved value is written to its private file before the request is
/// marked saved, so the waiting CLI never sees "saved" without a file to read.
pub(crate) fn answer(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    id: &str,
    answer: SecretAnswer,
) -> Result<(), SecretRequestError> {
    let now = now_ms();
    let mut registry = registry();
    settle(&mut registry, now);
    let request = registry
        .requests
        .get_mut(id)
        .filter(|request| request.project_id == project_id && request.session_id == session_id)
        .ok_or(SecretRequestError::NotFound)?;
    match request.status {
        SecretRequestStatus::Pending => {}
        SecretRequestStatus::Saved | SecretRequestStatus::Declined => {
            return Err(SecretRequestError::AlreadyAnswered)
        }
        SecretRequestStatus::TimedOut | SecretRequestStatus::Cancelled => {
            return Err(SecretRequestError::Ended)
        }
    }
    match answer {
        SecretAnswer::Decline => {
            request.status = SecretRequestStatus::Declined;
        }
        SecretAnswer::Save(value) => {
            if value.trim().is_empty() {
                return Err(SecretRequestError::Invalid("Enter the secret, or decline."));
            }
            if value.len() > MAX_SECRET_BYTES {
                return Err(SecretRequestError::Invalid(
                    "That is too long for a key or a token.",
                ));
            }
            let file = write_private_file(&files_dir(paths), &request.id, &value)
                .map_err(|_| SecretRequestError::StoreFailed)?;
            request.file = Some(file);
            request.status = SecretRequestStatus::Saved;
        }
    }
    request.answered_at_ms = Some(now);
    drop(registry);
    REGISTRY.1.notify_all();
    Ok(())
}

/// Waits up to `max_wait` for an answer and counts as the CLI still being there. A saved
/// request is handed over once: the next wait finds it gone.
pub(crate) fn wait(id: &str, max_wait: Duration) -> Result<WaitOutcome, SecretRequestError> {
    let deadline = std::time::Instant::now() + max_wait;
    let mut guard = registry();
    loop {
        let now = now_ms();
        settle(&mut guard, now);
        let request = guard
            .requests
            .get_mut(id)
            .ok_or(SecretRequestError::NotFound)?;
        request.last_heard_ms = now;
        if request.status != SecretRequestStatus::Pending {
            let outcome = WaitOutcome {
                status: request.status,
                file: request.file.take(),
            };
            if outcome.status == SecretRequestStatus::Saved {
                // Collected: it now only lingers so other clients see the card close.
                request.expires_at_ms = now;
            }
            return Ok(outcome);
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Ok(WaitOutcome {
                status: SecretRequestStatus::Pending,
                file: None,
            });
        }
        let until_expiry = Duration::from_millis((request.expires_at_ms - now).max(0) as u64);
        let (next, _) = REGISTRY
            .1
            .wait_timeout(guard, left.min(until_expiry).max(Duration::from_millis(10)))
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard = next;
    }
}

/// The CLI gave up (Ctrl+C); the card closes.
pub(crate) fn cancel(id: &str) -> bool {
    let mut registry = registry();
    let Some(request) = registry.requests.get_mut(id) else {
        return false;
    };
    if request.status != SecretRequestStatus::Pending {
        return false;
    }
    request.status = SecretRequestStatus::Cancelled;
    request.answered_at_ms = Some(now_ms());
    drop(registry);
    REGISTRY.1.notify_all();
    true
}

pub(crate) fn files_dir(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join(FILES_DIR)
}

/// Writes `value` to `<dir>/<id>`: the folder is 0700, the file 0600 and created fresh, never
/// through an existing path. Old files are swept first.
fn write_private_file(dir: &Path, id: &str, value: &str) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    sweep_expired_files(dir);
    let file = dir.join(id);
    let partial = dir.join(format!("{id}.partial"));
    let _ = fs::remove_file(&partial);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = (|| {
        let mut handle = options.open(&partial)?;
        handle.write_all(value.as_bytes())?;
        handle.sync_all()?;
        fs::rename(&partial, &file)
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    Ok(file)
}

fn is_request_file_name(name: &str) -> bool {
    let id = name.strip_suffix(".partial").unwrap_or(name);
    id.len() == 32 && id.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Deletes saved files older than a day. Runs before every write and when gxserver starts.
pub(crate) fn sweep_expired_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !is_request_file_name(&name.to_string_lossy()) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_none_or(|age| age > FILE_TTL);
        if old && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
pub(crate) fn backdate_for_tests(id: &str, by: Duration) {
    if let Some(request) = registry().requests.get_mut(id) {
        let by = by.as_millis() as i64;
        request.last_heard_ms -= by;
        request.created_at_ms -= by;
    }
}
