//! What Settings, the CLI and agents know about the Executor CLI: whether it is installed (and
//! Executor 2), its sign-in from `executor whoami` and the apps it reaches, read in the background
//! and kept for a minute; the Install job; and the change listener that tells clients.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::managed_tools::jobs;
use crate::paths::GxserverPaths;
use crate::workspaces::DEFAULT_WORKSPACE_ID;

use super::cli::{self, App, Found, Whoami};

/// How long a read of the CLI is reused before the next read refreshes it in the background.
const FRESH_FOR: Duration = Duration::from_secs(60);
/// How long the first read of this gxserver waits for the CLI before answering anyway.
const FIRST_READ_WAIT: Duration = Duration::from_secs(8);
/// The install job's key among Ghostex's tool installs.
pub(super) const INSTALL_JOB: &str = "executor-cli";

/// One read of the CLI.
#[derive(Clone, Default)]
pub(super) struct Snapshot {
    pub(super) found: Option<Found>,
    /// `Ok(None)`: not signed in.
    pub(super) whoami: Option<Result<Option<Whoami>, String>>,
    pub(super) apps: Option<Result<Vec<App>, String>>,
}

impl Snapshot {
    pub(super) fn signed_in(&self) -> Option<&Whoami> {
        self.whoami.as_ref()?.as_ref().ok()?.as_ref()
    }

    /// Agents get Executor's tools: an Executor 2 CLI, signed in, with approvals in model mode
    /// (Ghostex answers each pause with its card, which only model mode hands to the client).
    pub(super) fn usable(&self) -> Option<(&Found, &Whoami)> {
        let found = self.found.as_ref().filter(|found| found.usable())?;
        let whoami = self.signed_in().filter(|whoami| whoami.mode == "model")?;
        Some((found, whoami))
    }

    /// An app's slug and name, by its id.
    pub(super) fn app(&self, id: &str) -> Option<&App> {
        self.apps
            .as_ref()?
            .as_ref()
            .ok()?
            .iter()
            .find(|app| app.id == id)
    }

    pub(super) fn app_by_slug(&self, slug: &str) -> Option<&App> {
        self.apps
            .as_ref()?
            .as_ref()
            .ok()?
            .iter()
            .find(|app| app.slug == slug)
    }
}

struct Cache {
    snapshot: Option<Snapshot>,
    read_at: Option<Instant>,
    reading: bool,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    snapshot: None,
    read_at: None,
    reading: false,
});

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

type ChangeListener = Arc<dyn Fn(&str) + Send + Sync>;
static CHANGE_LISTENER: Mutex<Option<ChangeListener>> = Mutex::new(None);

/// Called when the CLI's state changed (installed, signed in or out), so clients read it again.
pub(crate) fn set_changed_listener(listener: ChangeListener) {
    *CHANGE_LISTENER
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(listener);
}

/// The Executor CLI belongs to the computer, not a workspace; clients read every workspace again
/// on any change, so the default workspace names it.
pub(super) fn notify_changed() {
    let listener = CHANGE_LISTENER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(listener) = listener {
        listener(DEFAULT_WORKSPACE_ID);
    }
}

fn read() -> Snapshot {
    let found = cli::find();
    let usable = found.as_ref().filter(|found| found.usable());
    let whoami = usable.map(|found| cli::whoami(&found.executable));
    let apps = match (usable, whoami.as_ref()) {
        (Some(found), Some(Ok(Some(whoami)))) => Some(cli::apps(&found.executable, &whoami.host)),
        _ => None,
    };
    Snapshot {
        found,
        whoami,
        apps,
    }
}

/// Reads the CLI on a background thread; `notify` tells clients when it finishes.
fn refresh_in_background(notify: bool) -> std::sync::mpsc::Receiver<()> {
    let (done, finished) = std::sync::mpsc::channel();
    {
        let mut cache = cache();
        if cache.reading {
            return finished;
        }
        cache.reading = true;
    }
    std::thread::spawn(move || {
        let snapshot = read();
        let changed = {
            let mut cache = cache();
            let changed = cache.snapshot.as_ref().map(describe) != Some(describe(&snapshot));
            cache.snapshot = Some(snapshot);
            cache.read_at = Some(Instant::now());
            cache.reading = false;
            changed
        };
        let _ = done.send(());
        if notify && changed {
            notify_changed();
        }
    });
    finished
}

/// Reads the CLI again now (after a sign-in, sign-out or install) and tells clients.
pub(super) fn refresh_now() {
    let finished = refresh_in_background(false);
    let _ = finished.recv_timeout(Duration::from_secs(60));
    notify_changed();
}

/// The last read, refreshed in the background once it is a minute old. This gxserver's first
/// read waits for the CLI up to a few seconds.
pub(super) fn snapshot() -> Snapshot {
    let (snapshot, stale) = {
        let cache = cache();
        (
            cache.snapshot.clone(),
            cache.read_at.is_none_or(|at| at.elapsed() >= FRESH_FOR),
        )
    };
    match snapshot {
        Some(snapshot) => {
            if stale {
                refresh_in_background(true);
            }
            snapshot
        }
        None => {
            let finished = refresh_in_background(true);
            let _ = finished.recv_timeout(FIRST_READ_WAIT);
            cache().snapshot.clone().unwrap_or_default()
        }
    }
}

/// The Executor status as Settings shows it.
fn describe(snapshot: &Snapshot) -> Value {
    let found = snapshot.found.as_ref();
    let usable_cli = found.filter(|found| found.usable());
    let problem = match found {
        Some(found) if !found.usable() => Some(match &found.version {
            Some(version) => format!(
                "This is Executor {version}. Ghostex needs Executor 2: install it again with Install."
            ),
            None => "The Executor CLI on this computer didn't report its version.".to_string(),
        }),
        _ => None,
    };
    let whoami = snapshot.signed_in();
    let error = match (&snapshot.whoami, whoami) {
        (Some(Err(error)), _) => Some(error.clone()),
        (_, Some(whoami)) if whoami.mode != "model" => Some(format!(
            "Its approvals are in {} mode, and Ghostex's cards need model mode. Sign out, then sign in again and pick Model on the approval page.",
            whoami.mode
        )),
        _ => None,
    };
    let apps: Vec<Value> = match &snapshot.apps {
        Some(Ok(apps)) => apps
            .iter()
            .map(|app| json!({ "slug": app.slug, "name": app.name }))
            .collect(),
        _ => Vec::new(),
    };
    json!({
        "installed": usable_cli.is_some(),
        "version": found.and_then(|found| found.version.clone()),
        "problem": problem,
        "signedIn": whoami.is_some(),
        "host": whoami.map(|whoami| whoami.host.clone()),
        "hostLabel": whoami.map(Whoami::host_label),
        "organization": whoami.and_then(|whoami| whoami.organization.clone()),
        "access": whoami.map(|whoami| whoami.access.clone()),
        "mode": whoami.map(|whoami| whoami.mode.clone()),
        "usable": snapshot.usable().is_some(),
        "error": error,
        "apps": apps,
        "appsError": match &snapshot.apps {
            Some(Err(error)) => Some(error.clone()),
            _ => None,
        },
        "manageUrl": whoami.map(Whoami::manage_url),
    })
}

/// `executor` in `/api/readConnectors`: the computer's Executor CLI, its sign-in, apps, an
/// Install or sign-in under way, and the old built-in Executor's apps until they are cleaned up.
pub(crate) fn status(paths: &GxserverPaths) -> Value {
    let mut value = describe(&snapshot());
    value["install"] = jobs::get(INSTALL_JOB)
        .map(|job| job.view())
        .unwrap_or(Value::Null);
    value["signingIn"] = json!(super::signin::signing_in());
    value["oldApps"] = super::legacy::status(paths);
    value
}

/// Runs Executor's official installer as one of Ghostex's tool installs: `install.sh` on macOS
/// and Linux, `install.ps1` on Windows, which put Executor 2 in the user's own folders.
///
/// CDXC:Connectors 2026-11-27 DECISION:
/// User (Executor CLI page, PLACEHOLDER-3): use the user's own `executor`, never a copy Ghostex pins or downloads itself; Install runs Executor's own installer, and Executor updates itself with `executor upgrade`. The command it runs is shown first in the job's log.
pub(crate) fn install() -> Result<Value, String> {
    if jobs::begin(INSTALL_JOB, "install").is_err() {
        return Ok(json!({ "started": false }));
    }
    std::thread::spawn(|| {
        let _turn = jobs::INSTALL_LOCK.blocking_lock();
        jobs::set_status(INSTALL_JOB, "running");
        let log = jobs::log_for(INSTALL_JOB);
        let (program, args): (&str, Vec<&str>) = if cfg!(windows) {
            (
                "powershell",
                vec![
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    "irm https://executor.sh/docs/install.ps1 | iex",
                ],
            )
        } else {
            (
                "sh",
                vec!["-c", "curl -fsSL https://executor.sh/docs/install.sh | sh"],
            )
        };
        log.line(&format!(
            "Running: {}",
            args.last().copied().unwrap_or_default()
        ));
        let result = crate::managed_tools::run::run(
            program,
            &args,
            &[],
            &[],
            &log,
            Duration::from_secs(10 * 60),
        );
        jobs::finish(INSTALL_JOB, &result);
        crate::platform::live_path::clear_cache();
        refresh_now();
    });
    notify_changed();
    Ok(json!({ "started": true }))
}
