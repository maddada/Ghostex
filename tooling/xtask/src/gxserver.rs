//! What `start` and `start-server` need to know about a running gxserver: where its state and auth token live, its health, and how to stop its control plane.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::http::{request, request_json, NoAnswer};
use crate::util::{env_trimmed, home_dir, sleep_ms, xdg_state_root};

/// CDXC:ServerDaemon 2026-09-18 SEE-ALSO:
/// Mirrors the state_dir resolution in packages/paths (GhostexStoragePaths): GHOSTEX_HOME wins, native Windows keeps state under %LOCALAPPDATA%\Ghostex\State, and everything else follows XDG.
/// gxserver's own files sit under <state>/gxserver (server/src/paths.rs), so the token and runtime/server.json are both read from here.
pub fn ghostex_state_dir() -> PathBuf {
    if let Some(home) = explicit_ghostex_home() {
        return home.join("state");
    }
    if cfg!(windows) {
        return local_app_data().join("Ghostex").join("State");
    }
    xdg_state_root().join("ghostex")
}

pub fn explicit_ghostex_home() -> Option<PathBuf> {
    env_trimmed("GHOSTEX_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub fn local_app_data() -> PathBuf {
    env_trimmed("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData").join("Local"))
}

pub fn read_token() -> Option<String> {
    read_token_file().map(|(token, _)| token)
}

/// The auth token and the file it came from (named in messages about a refused token).
pub fn read_token_file() -> Option<(String, PathBuf)> {
    let mut token_paths = vec![ghostex_state_dir()
        .join("gxserver")
        .join("auth")
        .join("token")];
    if explicit_ghostex_home().is_none() {
        // Read-only upgrade compatibility before the app has had a chance to run the storage migration. Current XDG state always wins.
        let home = home_dir();
        token_paths
            .push(home.join("Library/Application Support/Ghostex/State/gxserver/auth/token"));
        token_paths.push(home.join(".ghostex/gxserver/auth/token"));
    }
    token_paths.into_iter().find_map(|path| {
        let token = fs::read_to_string(&path).ok()?.trim().to_string();
        (!token.is_empty()).then_some((token, path))
    })
}

/// The health record when a gxserver answers at `base_url`.
pub fn health(base_url: &str, token: &str, timeout: Duration) -> Option<serde_json::Value> {
    match probe(base_url, token, timeout) {
        Probe::Healthy(value) => Some(value),
        _ => None,
    }
}

/*
CDXC:ServerDaemon 2026-11-09 WHY:
`health` returns None for every failure, and `cargo xtask start` read that None as "does not answer; ending it". Since 2026-11-03 each gxserver also holds a second loopback port for connected computers (server/src/server/peer_http.rs), which refuses the main token with 401 by design, and the Windows start probes every loopback port a gxserver process holds, so it killed a healthy gxserver on every start (Windows Orchestrator, 2026-11-09: :7547 and :58746). A port that answers at all, even 401, has a live server behind it; only a port that never answers is a hang.
*/
/// What a gxserver port answered to `/api/health/server`.
pub enum Probe {
    /// A gxserver health record.
    Healthy(serde_json::Value),
    /// The server answered but refused this token (401 or 403): it is alive, and this port does not take the token, or the token is not its own.
    Refused { status: u16, message: String },
    /// The server answered something that is not a gxserver health record.
    Other { status: u16 },
    /// Nothing answered: something holds the port but stayed silent, or nothing accepted the connection.
    NoAnswer(NoAnswer),
}

impl Probe {
    /// Whether the server answered at all, which a hung or stopped one never does.
    pub fn answered(&self) -> bool {
        !matches!(self, Probe::NoAnswer(_))
    }

    /// What it answered, for a log line ("answered 401: …", "did not answer in time").
    pub fn describe(&self) -> String {
        match self {
            Probe::Healthy(health) => format!(
                "answered as gxserver pid {}",
                health
                    .get("pid")
                    .and_then(|p| p.as_u64())
                    .map_or("?".to_string(), |p| p.to_string())
            ),
            Probe::Refused { status, message } if message.is_empty() => {
                format!("answered {status} (refused the token)")
            }
            Probe::Refused { status, message } => {
                format!("answered {status} (refused the token: {message})")
            }
            Probe::Other { status } => {
                format!("answered {status}, which is not a gxserver health record")
            }
            Probe::NoAnswer(NoAnswer::TimedOut) => {
                "accepted the connection but did not answer in time".to_string()
            }
            Probe::NoAnswer(NoAnswer::ConnectTimedOut) => {
                "did not accept the connection in time".to_string()
            }
            Probe::NoAnswer(NoAnswer::Refused) => "refused the connection".to_string(),
            Probe::NoAnswer(NoAnswer::Failed) => "could not be asked".to_string(),
        }
    }
}

/// One `/api/health/server` request, sorted into what came back.
pub fn probe(base_url: &str, token: &str, timeout: Duration) -> Probe {
    match request(base_url, "GET", "/api/health/server", token, timeout) {
        Err(reason) => Probe::NoAnswer(reason),
        Ok(answer) if (200..300).contains(&answer.status) => match answer.json {
            Some(value) if value.get("product").and_then(|p| p.as_str()) == Some("gxserver") => {
                Probe::Healthy(value)
            }
            _ => Probe::Other {
                status: answer.status,
            },
        },
        Ok(answer) if matches!(answer.status, 401 | 403) => Probe::Refused {
            status: answer.status,
            message: answer
                .json
                .as_ref()
                .and_then(|body| body.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        Ok(answer) => Probe::Other {
            status: answer.status,
        },
    }
}

/// How long [`probe_patiently`] keeps asking a silent port: a gxserver busy with the app's disconnect can miss one short probe.
pub const PATIENT_PROBE: Duration = Duration::from_secs(5);

/// Probes until the port answers, asking again for up to [`PATIENT_PROBE`] while it stays silent, so one slow moment is not taken for a hang.
pub fn probe_patiently(base_url: &str, token: &str) -> Probe {
    let deadline = Instant::now() + PATIENT_PROBE;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let result = probe(
            base_url,
            token,
            left.clamp(Duration::from_millis(200), Duration::from_millis(1500)),
        );
        if result.answered() || Instant::now() + Duration::from_millis(300) >= deadline {
            return result;
        }
        sleep_ms(300);
    }
}

pub fn request_stop(base_url: &str, token: &str) {
    let _ = request_json(
        base_url,
        "POST",
        "/api/control/stop",
        token,
        Duration::from_secs(1),
    );
}

pub fn build_identity_of(health: &serde_json::Value) -> String {
    health
        .get("buildIdentity")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

/// The `buildIdentity` recorded in a gxserver package folder's build-identity.json.
pub fn read_build_identity(identity_file: &Path) -> Option<String> {
    let text = fs::read_to_string(identity_file).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let identity = parsed.get("buildIdentity")?.as_str()?.trim().to_string();
    (!identity.is_empty()).then_some(identity)
}

/// Waits until nothing listens at `base_url` any more. A port that answers anything (401 included) or accepts the connection without answering still has its server; a connection that is refused, or not accepted in time (how Windows reports a closed loopback port within 500 ms), counts as stopped.
pub fn wait_for_stop(base_url: &str, token: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if matches!(
            probe(base_url, token, Duration::from_millis(500)),
            Probe::NoAnswer(NoAnswer::Refused | NoAnswer::ConnectTimedOut)
        ) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep_ms(100);
    }
}
