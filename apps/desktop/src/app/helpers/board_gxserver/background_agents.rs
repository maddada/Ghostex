//! Ghostex's background helpers as SMAppService agents (macOS only): the gxserver job, the
//! login-recovery agent and the start-at-login agent, registered from the plists the app ships in
//! Contents/Library/LaunchAgents. Any failure falls back to the legacy plists in
//! ~/Library/LaunchAgents at once (`gpui_spawn_local_gxserver_launchd_job`), so gxserver always
//! starts. Why: CDXC:BackgroundItems 2026-11-08 in packages/paths/src/macos_launch_agent.rs.

use std::{
    ffi::{CStr, CString, c_char},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use ghostex_paths::macos_launch_agent as agents;

use crate::app::helpers::*;
use crate::*;

unsafe extern "C" {
    fn ghostex_gpui_background_agent_status(plist_name: *const c_char) -> i32;
    fn ghostex_gpui_background_agent_register(
        plist_name: *const c_char,
        error_buffer: *mut c_char,
        error_length: usize,
    ) -> i32;
    fn ghostex_gpui_background_agent_unregister(
        plist_name: *const c_char,
        error_buffer: *mut c_char,
        error_length: usize,
    ) -> i32;
    fn ghostex_gpui_background_agent_open_login_items();
}

/// SMAppServiceStatus values (and -1: no SMAppService on this macOS).
const STATUS_ENABLED: i32 = 1;
const STATUS_REQUIRES_APPROVAL: i32 = 2;

/// Set to 1 to keep the legacy LaunchAgents (a way back if the bundled helpers misbehave).
const LEGACY_OVERRIDE_ENV: &str = "GHOSTEX_GXSERVER_LEGACY_LAUNCHD";

/// How gxserver's background helpers run, for Settings > About.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiBackgroundHelpers {
    /// Not decided yet in this run (gxserver was already running when the app started).
    Unknown,
    /// Registered with macOS from the app bundle; Login Items lists them under Ghostex.
    Bundled,
    /// macOS wants the user to allow Ghostex in System Settings > General > Login Items.
    NeedsApproval,
    /// The ~/Library/LaunchAgents plists, and why.
    Legacy(String),
}

static HELPERS: Mutex<GpuiBackgroundHelpers> = Mutex::new(GpuiBackgroundHelpers::Unknown);

pub(crate) fn gpui_background_helpers() -> GpuiBackgroundHelpers {
    HELPERS
        .lock()
        .map(|helpers| helpers.clone())
        .unwrap_or(GpuiBackgroundHelpers::Unknown)
}

fn set_helpers(state: GpuiBackgroundHelpers) {
    if let Ok(mut helpers) = HELPERS.lock() {
        *helpers = state;
    }
}

/// The About line: what runs gxserver's helpers, and whether macOS waits for the user's OK.
/// `None` until this run has started gxserver or checked the helpers.
pub(crate) fn gpui_background_helpers_status_line() -> Option<(String, bool)> {
    match gpui_background_helpers() {
        GpuiBackgroundHelpers::Unknown => None,
        GpuiBackgroundHelpers::Bundled => {
            Some(("Background helpers: registered with macOS".to_string(), false))
        }
        GpuiBackgroundHelpers::NeedsApproval => Some((
            "Background helpers: waiting for your OK in System Settings > General > Login Items; using classic login items until then"
                .to_string(),
            true,
        )),
        GpuiBackgroundHelpers::Legacy(reason) => Some((
            format!("Background helpers: classic login items ({reason})"),
            false,
        )),
    }
}

pub(crate) fn gpui_open_login_items_settings() {
    unsafe { ghostex_gpui_background_agent_open_login_items() }
}

/// Why a bundled registration failed: approval is reported on its own so the status can say what
/// to click.
enum RegisterError {
    NeedsApproval,
    Failed(String),
}

impl RegisterError {
    fn describe(&self) -> String {
        match self {
            Self::NeedsApproval => "macOS is waiting for approval in Login Items".to_string(),
            Self::Failed(reason) => reason.clone(),
        }
    }
}

fn plist_name(label: &str) -> CString {
    CString::new(format!("{label}.plist")).unwrap_or_default()
}

fn agent_status(label: &str) -> i32 {
    let name = plist_name(label);
    unsafe { ghostex_gpui_background_agent_status(name.as_ptr()) }
}

fn call_with_error(
    label: &str,
    call: unsafe extern "C" fn(*const c_char, *mut c_char, usize) -> i32,
) -> (i32, String) {
    let name = plist_name(label);
    let mut buffer = vec![0 as c_char; 512];
    let result = unsafe { call(name.as_ptr(), buffer.as_mut_ptr(), buffer.len()) };
    let error = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    (result, error)
}

/// FNV-1a of the plist's bytes: stable across builds, unlike std's hasher.
fn plist_digest(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    Some(format!("{hash:016x}"))
}

/// The running app's bundle, when it ships the bundled helpers.
fn app_with_bundled_agents() -> Option<PathBuf> {
    let app = agents::outermost_app_bundle(&std::env::current_exe().ok()?)?;
    app.join(agents::BUNDLED_AGENTS_DIR)
        .join(format!("{}.plist", agents::BUNDLED_GXSERVER))
        .is_file()
        .then_some(app)
}

/// The app bundle when this gxserver start may use the bundled agent, or why not.
fn eligible(binary: &Path) -> Result<PathBuf, String> {
    if std::env::var(LEGACY_OVERRIDE_ENV).is_ok_and(|value| value == "1") {
        return Err(format!("{LEGACY_OVERRIDE_ENV}=1 is set"));
    }
    if ghostex_paths::uses_separate_profile()
        || gpui_local_gxserver_api_port() != GPUI_GXSERVER_LOCAL_API_PORT
    {
        return Err("this is a separate Ghostex profile".to_string());
    }
    // The bundled plist cannot pass the app's folders on, so an app started with its own folders
    // keeps the plist that carries them.
    if [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ]
    .iter()
    .any(|key| std::env::var(key).is_ok_and(|value| Path::new(value.trim()).is_absolute()))
    {
        return Err("Ghostex was started with its own XDG folders".to_string());
    }
    let app = app_with_bundled_agents()
        .ok_or_else(|| "this build of Ghostex has no bundled helpers".to_string())?;
    let program = app.join(agents::BUNDLED_GXSERVER_PROGRAM);
    match (program.canonicalize(), binary.canonicalize()) {
        (Ok(program), Ok(binary)) if program == binary => Ok(app),
        _ => Err("gxserver is not the one inside this app".to_string()),
    }
}

/*
CDXC:BackgroundItems 2026-11-08 WHY: launchd keeps the plist it was given at registration (a
changed bundled plist had no effect until it was unregistered and registered again, in the
2026-11-08 test), so the digest of each registered plist is recorded and a changed one registered
again; that posted no new notification. Unregistering the gxserver agent ends its process, so this
runs for it only from the start path, where gxserver is being (re)started anyway.
*/
fn register(app: &Path, label: &str) -> Result<(), RegisterError> {
    let plist = app
        .join(agents::BUNDLED_AGENTS_DIR)
        .join(format!("{label}.plist"));
    let digest = plist_digest(&plist)
        .ok_or_else(|| RegisterError::Failed(format!("{} is missing", plist.display())))?;
    let mut state = agents::read_bundled_agents_state();
    let key = format!("digest:{label}");
    let status = agent_status(label);
    if status == STATUS_ENABLED
        && state.get(&key).and_then(serde_json::Value::as_str) != Some(digest.as_str())
    {
        let (_, error) = call_with_error(label, ghostex_gpui_background_agent_unregister);
        if !error.is_empty() {
            return Err(RegisterError::Failed(format!(
                "unregister the old {label}: {error}"
            )));
        }
    }
    let (status, error) = call_with_error(label, ghostex_gpui_background_agent_register);
    match status {
        STATUS_ENABLED => {}
        STATUS_REQUIRES_APPROVAL => return Err(RegisterError::NeedsApproval),
        -1 => {
            return Err(RegisterError::Failed(
                "this macOS has no SMAppService".to_string(),
            ));
        }
        status => {
            return Err(RegisterError::Failed(format!(
                "SMAppService status {status} for {label}{}",
                if error.is_empty() {
                    String::new()
                } else {
                    format!(": {error}")
                }
            )));
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while !agents::gui_job_loaded(label) {
        if Instant::now() >= deadline {
            return Err(RegisterError::Failed(format!(
                "{label} is registered but launchd has not loaded it"
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    state.insert(key, serde_json::Value::String(digest));
    agents::write_bundled_agents_state(&state)
        .map_err(|error| RegisterError::Failed(format!("record {label}: {error}")))
}

/*
CDXC:BackgroundItems 2026-11-08 DECISION: User: gxserver "must ALWAYS still start at login and on
app start: if SMAppService register fails, needs approval, or the macOS version can't do it, fall
back to today's legacy plist path at once (and say so in the Remote/About status), never leaving
the user without gxserver; migrate existing installs once (unregister/remove the legacy plists only
after the SMAppService agent is confirmed enabled)". This returns Err with the reason whenever the
bundled agent is not running gxserver, and the caller goes straight on with the legacy job. The
legacy job is booted out only after the bundled one is registered and loaded (the port must be
free), and its plist is deleted only once the bundled job has a gxserver process.
*/
/// Starts gxserver through the bundled agent. `Err` means: use the legacy job now.
pub(crate) fn gpui_start_gxserver_through_bundled_agent(
    binary: &Path,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    let started = Instant::now();
    let mut own = Vec::new();
    let result = start_through_bundled_agent(binary, &mut |text: String| {
        own.push(format!(
            "bundled agent +{}ms: {text}",
            started.elapsed().as_millis()
        ))
    });
    steps.extend(own.iter().cloned());
    match result {
        Ok(()) => {
            set_helpers(GpuiBackgroundHelpers::Bundled);
            Ok(())
        }
        Err(failure) => {
            set_helpers(if failure.needs_approval {
                GpuiBackgroundHelpers::NeedsApproval
            } else {
                GpuiBackgroundHelpers::Legacy(failure.reason.clone())
            });
            // Written whatever the debug-log setting, so a fallback on a user's Mac says why.
            if !failure.expected {
                support_logs::append(
                    support_logs::GpuiSupportLog::HostLifecycle,
                    "gpui.gxserverBootstrap.bundledAgentFailed",
                    serde_json::json!({ "reason": failure.reason, "steps": own }),
                );
            }
            Err(failure.reason)
        }
    }
}

/// Why the bundled agent did not start gxserver.
struct BundledFailure {
    reason: String,
    needs_approval: bool,
    /// A separate profile or a build without the plists: nothing went wrong.
    expected: bool,
}

impl From<String> for BundledFailure {
    fn from(reason: String) -> Self {
        Self {
            reason,
            needs_approval: false,
            expected: false,
        }
    }
}

/// Where the legacy gxserver plist waits while the bundled job starts: out of
/// ~/Library/LaunchAgents, so nothing can load the legacy job meanwhile.
fn parked_legacy_plist() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("legacy-gxserver-launch-agent.plist")
}

/// Puts a parked legacy plist back, for the fallback.
fn restore_legacy_plist(note: &mut impl FnMut(String)) {
    let parked = parked_legacy_plist();
    let legacy = agents::legacy_plist_path(agents::LEGACY_GXSERVER);
    if parked.exists() && !legacy.exists() {
        match std::fs::rename(&parked, &legacy) {
            Ok(()) => note("legacy plist put back".to_string()),
            Err(error) => note(format!("could not put the legacy plist back: {error}")),
        }
    }
}

/// The legacy job and plist go, the parked copy too, and the other helpers follow: the bundled
/// job runs gxserver.
fn finish_migration(note: &mut impl FnMut(String)) {
    match agents::remove_legacy_agent(agents::LEGACY_GXSERVER) {
        Ok(()) => note("legacy gxserver job and plist removed".to_string()),
        Err(error) => note(format!("could not remove the legacy plist: {error}")),
    }
    let _ = std::fs::remove_file(parked_legacy_plist());
    std::thread::spawn(|| gpui_reconcile_bundled_helpers(None));
}

/*
CDXC:BackgroundItems 2026-11-08 WHY: on the user's Mac the first migration lost a race. While the
app booted out the legacy job, agent hooks in the running sessions called `gxserver start`, which
found the legacy plist still in ~/Library/LaunchAgents and loaded and started the legacy job again
(`launchctl print` showed it loaded with runs = 1 seconds after a bootout), so the app fell back.
The legacy plist is therefore moved out of the folder as soon as the bundled job is registered and
loaded (that is the "confirmed enabled" point), before the legacy job is booted out; from then on
`gxserver start` finds the bundled job. It is moved back if the bundled job does not start
gxserver, and the legacy path below starts it as before.
*/
fn start_through_bundled_agent(
    binary: &Path,
    note: &mut impl FnMut(String),
) -> Result<(), BundledFailure> {
    let app = eligible(binary).map_err(|reason| BundledFailure {
        reason,
        needs_approval: false,
        expected: true,
    })?;
    let target = format!("gui/{}/{}", gpui_current_uid(), agents::BUNDLED_GXSERVER);
    let before = gpui_inspect_launchd_job(&target)?;
    note(format!("job before start: {}", before.describe()));
    if before.pid.is_some() {
        let probe_started = Instant::now();
        while probe_started.elapsed() < Duration::from_secs(20) {
            if matches!(
                gpui_probe_local_gxserver_health(),
                GpuiLocalGxserverHealthState::Healthy { .. }
            ) {
                note("its gxserver is running and healthy; leaving it alone".to_string());
                finish_migration(note);
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    register(&app, agents::BUNDLED_GXSERVER).map_err(|error| BundledFailure {
        reason: error.describe(),
        needs_approval: matches!(error, RegisterError::NeedsApproval),
        expected: false,
    })?;
    note("registered and loaded".to_string());
    let program = app.join(agents::BUNDLED_GXSERVER_PROGRAM);
    let mut state = agents::read_bundled_agents_state();
    state.insert(
        "program".to_string(),
        serde_json::Value::String(program.to_string_lossy().into_owned()),
    );
    agents::write_bundled_agents_state(&state)
        .map_err(|error| format!("record the bundled program: {error}"))?;

    let legacy = agents::legacy_plist_path(agents::LEGACY_GXSERVER);
    if legacy.exists() {
        let parked = parked_legacy_plist();
        if let Some(folder) = parked.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        std::fs::rename(&legacy, &parked)
            .map_err(|error| format!("move the legacy plist aside: {error}"))?;
        note("legacy plist moved aside".to_string());
    }
    let result = kickstart_bundled_job(&target, note);
    match result {
        Ok(pid) => {
            note(format!("gxserver pid {pid}"));
            finish_migration(note);
            Ok(())
        }
        Err(reason) => {
            restore_legacy_plist(note);
            Err(reason.into())
        }
    }
}

/// Boots out the legacy job (its plist is already aside), then starts the bundled one.
fn kickstart_bundled_job(target: &str, note: &mut impl FnMut(String)) -> Result<u32, String> {
    let legacy_target = format!("gui/{}/{}", gpui_current_uid(), agents::LEGACY_GXSERVER);
    if gpui_inspect_launchd_job(&legacy_target)?.loaded {
        let bootout = gpui_run_launchctl(&["bootout", &legacy_target]);
        note(format!(
            "bootout of the legacy job: {}",
            gpui_describe_launchctl_result(&bootout)
        ));
        gpui_wait_for_launchd_job_removal(&legacy_target, Duration::from_secs(12))?;
    }
    for round in 1..=2u32 {
        let current = gpui_inspect_launchd_job(target)?;
        // A process that did not answer health is replaced.
        let arguments: &[&str] = if current.pid.is_some() {
            &["kickstart", "-k", target]
        } else {
            &["kickstart", target]
        };
        let kickstart = gpui_run_launchctl(arguments);
        note(format!(
            "kickstart {round}: {}",
            gpui_describe_launchctl_result(&kickstart)
        ));
        if let Some(pid) = gpui_wait_for_launchd_job_pid(target, Duration::from_secs(3))? {
            return Ok(pid);
        }
    }
    Err("the bundled gxserver job did not start a process".to_string())
}

fn start_server_at_login_setting() -> bool {
    ghostex_paths::settings_file::read(&shared_settings::shared_sidebar_settings_path())
        .ok()
        .and_then(|read| read.object)
        .and_then(|object| {
            object
                .get("startServerAtLogin")
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(false)
}

/// Brings the recovery and start-at-login agents in line with the bundled gxserver job: called
/// after the bundled job starts gxserver, at app start and when the start-at-login switch
/// changes (`at_login`, the switch's new value). Does nothing while the gxserver job is the
/// legacy one: gxserver keeps the legacy agents then.
pub(crate) fn gpui_reconcile_bundled_helpers(at_login: Option<bool>) {
    static RECONCILE: Mutex<()> = Mutex::new(());
    let Ok(_guard) = RECONCILE.lock() else {
        return;
    };
    if !agents::gui_job_loaded(agents::BUNDLED_GXSERVER)
        || agents::legacy_plist_path(agents::LEGACY_GXSERVER).exists()
    {
        if gpui_background_helpers() == GpuiBackgroundHelpers::Unknown
            && agents::legacy_plist_path(agents::LEGACY_GXSERVER).exists()
        {
            set_helpers(GpuiBackgroundHelpers::Legacy(
                "they move to macOS's registered helpers the next time gxserver starts".to_string(),
            ));
        }
        return;
    }
    let Some(app) = app_with_bundled_agents() else {
        return;
    };
    if gpui_background_helpers() == GpuiBackgroundHelpers::Unknown {
        set_helpers(GpuiBackgroundHelpers::Bundled);
    }
    match register(&app, agents::BUNDLED_LOGIN_RECOVERY) {
        Ok(()) => {
            if let Err(error) = agents::remove_legacy_agent(agents::LEGACY_LOGIN_RECOVERY) {
                log::warn!("could not remove the legacy login-recovery agent: {error}");
            }
        }
        Err(error) => log::warn!(
            "login-recovery agent stays a legacy plist: {}",
            error.describe()
        ),
    }
    let wanted = at_login.unwrap_or_else(start_server_at_login_setting);
    if wanted {
        // gxserver writes the legacy entry itself while this one is not loaded.
        match register(&app, agents::BUNDLED_AT_LOGIN) {
            Ok(()) => {
                if let Err(error) = agents::remove_legacy_agent(agents::LEGACY_AT_LOGIN) {
                    log::warn!("could not remove the legacy login entry: {error}");
                }
            }
            Err(error) => log::warn!("the login entry stays a legacy plist: {}", error.describe()),
        }
    } else if agent_status(agents::BUNDLED_AT_LOGIN) == STATUS_ENABLED {
        let (_, error) = call_with_error(
            agents::BUNDLED_AT_LOGIN,
            ghostex_gpui_background_agent_unregister,
        );
        if !error.is_empty() {
            log::warn!("could not remove the bundled login entry: {error}");
        }
    }
}
