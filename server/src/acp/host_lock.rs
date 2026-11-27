//! Per-session OS lock file for `ghostex acp-host` so at most one Native host runs per session.
//!
//! CDXC:AgentProviders 2026-11-27 WHY:
//! On a busy server a live session can drop off `zmx list --short` or miss a channel poll, causing
//! self-heal (or another wake path) to launch a second host for the same session, each spawning and
//! restarting its own agent on the same conversation (GitHub #786). Holding a non-blocking kernel
//! advisory lock (`std::fs::File::try_lock`: `flock` on Unix, `LockFileEx` on Windows) lets a
//! second host detect the live owner and exit, while the kernel automatically releases the lock if
//! the owner crashes so a stale lock file never blocks a real restart.
//!
//! The lock is keyed by the session's zmx name (`GHOSTEX_SESSION_ID`), not its global ref: moving a
//! session to another project changes the project part of its ref but keeps its zmx name, so a
//! host started before the move and one started after it still meet on the same lock.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

const ACP_HOSTS_DIR: &str = "acp-hosts";

/// How long a starting host keeps retrying a held lock before it gives up as the duplicate.
/// gxserver's `is_host_lock_held` probe takes the lock for an instant to test it, and a host
/// replacing one that is just exiting can overlap it briefly; neither may make a real host quit.
/// A live owner holds the lock for its whole life, so a true duplicate still exits.
const ACQUIRE_RETRY_WINDOW: Duration = Duration::from_millis(1_500);
const ACQUIRE_RETRY_STEP: Duration = Duration::from_millis(50);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostLockRecord {
    pid: u32,
    started_at_ms: i64,
    session_ref: String,
}

pub(crate) struct HostSessionLock {
    _file: File,
}

pub(crate) enum AcquireResult {
    NotApplicable,
    Acquired(HostSessionLock),
    AlreadyHeld {
        session_ref: String,
        owner_pid: Option<u32>,
    },
    Unavailable {
        session_ref: String,
        error: String,
    },
}

fn sanitize_filename_segment(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Whether `zmx_name` is a gxserver session name (`<serverId>-<projectId>-<sessionId>`).
fn is_gxserver_zmx_name(zmx_name: &str) -> bool {
    let mut parts = zmx_name.split('-');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(server_id), Some(project_id), Some(session_id), None)
            if crate::ids::is_gxserver_server_id(server_id)
                && crate::ids::is_gxserver_project_id(project_id)
                && crate::ids::is_gxserver_session_id(session_id)
    )
}

pub(crate) fn lock_path(paths: &GxserverPaths, zmx_name: &str) -> PathBuf {
    paths
        .runtime_dir
        .join(ACP_HOSTS_DIR)
        .join(format!("{}.lock", sanitize_filename_segment(zmx_name)))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn read_owner_pid(file: &mut File) -> Option<u32> {
    if file.seek(SeekFrom::Start(0)).is_err() {
        return None;
    }
    let mut buf = Vec::new();
    if std::io::Read::by_ref(file)
        .take(4096)
        .read_to_end(&mut buf)
        .is_err()
    {
        return None;
    }
    let record: HostLockRecord = serde_json::from_slice(&buf).ok()?;
    (record.pid > 0).then_some(record.pid)
}

fn open_lock_file(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
        }
    }
    let mut opts = OpenOptions::new();
    // Never truncate before acquiring the lock, or a losing process would erase the owner's PID.
    opts.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let file = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    Ok(file)
}

/// Acquire the per-session `acp-host` lock for the current process's `GHOSTEX_SESSION_ID` (its
/// zmx name). The file is opened close-on-exec (Rust's default), so the agent child, and the CLI a
/// Terminal switch `exec`s into, never hold it.
pub(crate) fn acquire_for_current_session(paths: &GxserverPaths) -> AcquireResult {
    let Ok(zmx_name) = std::env::var("GHOSTEX_SESSION_ID") else {
        return AcquireResult::NotApplicable;
    };
    let zmx_name = zmx_name.trim().to_string();
    if !is_gxserver_zmx_name(&zmx_name) {
        return AcquireResult::NotApplicable;
    }
    let session_ref = std::env::var("GHOSTEX_GLOBAL_SESSION_REF")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| zmx_name.clone());
    let path = lock_path(paths, &zmx_name);
    let mut file = match open_lock_file(&path) {
        Ok(file) => file,
        Err(err) => {
            return AcquireResult::Unavailable {
                session_ref,
                error: err.to_string(),
            };
        }
    };
    let deadline = Instant::now() + ACQUIRE_RETRY_WINDOW;
    loop {
        match file.try_lock() {
            Ok(()) => {
                let record = HostLockRecord {
                    pid: std::process::id(),
                    started_at_ms: now_ms(),
                    session_ref,
                };
                if let Ok(payload) = serde_json::to_vec(&record) {
                    let _ = file.set_len(0);
                    let _ = file.seek(SeekFrom::Start(0));
                    let _ = file.write_all(&payload);
                    let _ = file.flush();
                }
                return AcquireResult::Acquired(HostSessionLock { _file: file });
            }
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(ACQUIRE_RETRY_STEP);
            }
            Err(TryLockError::WouldBlock) => {
                let owner_pid = read_owner_pid(&mut file);
                return AcquireResult::AlreadyHeld {
                    session_ref,
                    owner_pid,
                };
            }
            Err(TryLockError::Error(err)) => {
                return AcquireResult::Unavailable {
                    session_ref,
                    error: err.to_string(),
                };
            }
        }
    }
}

/// Returns `Some(owner_pid)` (`0` when the PID could not be read) if a live `acp-host` currently
/// holds the kernel lock for the session named `zmx_name`. Returns `None` if no lock file exists or
/// the lock is unheld (including stale lock files left behind by a crashed host). Testing takes
/// the lock for an instant when it is free; `acquire_for_current_session` retries past that.
pub(crate) fn is_host_lock_held(paths: &GxserverPaths, zmx_name: &str) -> Option<u32> {
    let path = lock_path(paths, zmx_name);
    let mut opts = OpenOptions::new();
    opts.read(true).write(true);
    let mut file = opts.open(&path).ok()?;
    match file.try_lock() {
        Ok(()) => None,
        Err(TryLockError::WouldBlock) => Some(read_owner_pid(&mut file).unwrap_or(0)),
        Err(TryLockError::Error(_)) => None,
    }
}
