//! Notices when SpaceO (the `spaceo` virtual-display tool) is stuck and heals it, for the
//! desktop app (Settings > Integrations, the "SpaceO is stuck" notice) and `ghostex spaceo`.
//!
//! CDXC:ManagedTools 2026-11-16 DECISION:
//! User: "please we need to add something to ghostex to manage this crap and self heal space o it's always failing with same errors and i have to repeat same steps". Ghostex checks SpaceO when it starts and whenever SpaceO's log shows a failure. What needs no operator authority it does on its own and logs (restart a stopped LaunchAgent daemon, end an MCP server whose agent is gone). What SpaceO reserves for the operator (`--operator` commands such as `safety clear-host-health`) runs only from the user's own Repair click or `ghostex spaceo repair` typed at a terminal, never from an agent.
//!
//! CDXC:ManagedTools 2026-11-16 WHY:
//! SpaceO latches a "display safety" stop when it once sees the Mac swapping or under memory pressure, keeps it in Application Support/SpaceO/display-safety.json across every daemon restart and reboot, and refuses every new agent screen ("an earlier display mutation failed or never completed") until `spaceo safety clear-host-health --operator` runs. That command refuses while any owner runs ("another SpaceO process owns the display lifecycle"), and the daemon is a launchd KeepAlive job that comes straight back when stopped, so Repair takes the job out of launchd first, stops the daemon and Viewer, clears the latch, then puts the job back. On 2026-11-16 the latch had blocked every agent since it tripped on 2026-10-25.

mod processes;
mod run;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

pub use processes::{Kind as ProcessKind, Process};
pub use run::Output;

const LAUNCH_AGENT_LABEL: &str = "com.spaceo.daemon";
const VIEWER_BUNDLE_ID: &str = "dev.spaceo.viewer";
const LOG_FILE: &str = "spaceo-heal.log";
const REPAIR_MARKER_FILE: &str = "spaceo-repair.pid";
const LOG_LIMIT_BYTES: u64 = 512 * 1024;

/// Where SpaceO is and where Ghostex keeps this module's log.
#[derive(Clone, Debug)]
pub struct Spaceo {
    pub exe: PathBuf,
    pub home: PathBuf,
    /// Ghostex's state folder; the heal log and the running-repair marker live in it.
    pub state_dir: PathBuf,
}

/// `spaceo` on PATH, else where SpaceO's installer puts it (an app's PATH may not list it).
pub fn locate(home: &Path) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("spaceo"))
            .find(|candidate| is_executable(candidate))
    });
    on_path.or_else(|| {
        let installed = home.join(".local/bin/spaceo");
        is_executable(&installed).then_some(installed)
    })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// SpaceO can open agent screens.
    Healthy,
    /// SpaceO's display safety stop is on: no agent can get a screen until the operator clears it.
    Stuck,
    /// No SpaceO daemon answers.
    DaemonDown,
    /// `spaceo doctor` did not answer.
    Unknown,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Stuck => "stuck",
            Self::DaemonDown => "daemonDown",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Health {
    pub state: State,
    /// A card title in plain words, such as "SpaceO is stuck".
    pub title: String,
    /// One or two plain sentences: what happened and what to do.
    pub detail: String,
    /// SpaceO's own words for the stop (`host health: swap_activity`), for logs and Details.
    pub spaceo_reason: Option<String>,
    /// When the stop was latched (the latch file's last write), when known.
    pub since: Option<SystemTime>,
    /// Whether Repair has something to do.
    pub repairable: bool,
    pub daemon_pid: Option<i64>,
    pub launch_agent_installed: bool,
}

impl Health {
    pub fn json(&self) -> Value {
        serde_json::json!({
            "state": self.state.as_str(),
            "title": self.title,
            "detail": self.detail,
            "spaceoReason": self.spaceo_reason,
            "since": self.since.map(iso_time),
            "repairable": self.repairable,
            "daemonPid": self.daemon_pid,
            "launchAgentInstalled": self.launch_agent_installed,
        })
    }

    /// The notice line, such as "SpaceO is stuck: it stopped opening agent screens on Oct 25 …".
    pub fn summary(&self) -> String {
        format!("{}: {}", self.title, self.detail)
    }
}

impl Spaceo {
    fn launch_agent_plist(&self) -> PathBuf {
        self.home
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
    }

    fn latch_file(&self) -> PathBuf {
        self.home
            .join("Library/Application Support/SpaceO/display-safety.json")
    }

    /// SpaceO's daemon log, which every refused agent request is written to.
    pub fn daemon_log(&self) -> PathBuf {
        self.home.join("Library/Logs/SpaceO/daemon.log")
    }

    pub fn heal_log(&self) -> PathBuf {
        self.state_dir.join(LOG_FILE)
    }

    fn spaceo(&self, args: &[&str], timeout: Duration) -> Result<Output, String> {
        run::run(&self.exe, args, timeout)
    }

    /// `spaceo doctor --json`, read-only. doctor exits 1 whenever it found something; its JSON
    /// is still complete.
    pub fn doctor(&self) -> Option<Value> {
        let output = self
            .spaceo(&["doctor", "--json"], Duration::from_secs(20))
            .ok()?;
        run::json_object(&output.stdout)
    }

    /// What state SpaceO is in, from `spaceo doctor --json`.
    pub fn check(&self) -> Health {
        let since = fs::metadata(self.latch_file())
            .and_then(|meta| meta.modified())
            .ok();
        parse_doctor(
            self.doctor().as_ref(),
            since,
            self.launch_agent_plist().is_file(),
        )
    }

    /// Appends one line to the heal log, keeping it bounded.
    pub fn log(&self, source: &str, message: &str) {
        let path = self.heal_log();
        if fs::metadata(&path).is_ok_and(|meta| meta.len() > LOG_LIMIT_BYTES) {
            let _ = fs::rename(&path, path.with_extension("log.old"));
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(
                file,
                "{} [{source}] {}",
                iso_time(SystemTime::now()),
                message.replace('\n', " | ")
            );
        }
    }

    fn repair_marker(&self) -> PathBuf {
        self.state_dir.join(REPAIR_MARKER_FILE)
    }

    /// True while a Repair runs here or in another Ghostex process (the CLI or the app).
    pub fn repair_running(&self) -> bool {
        fs::read_to_string(self.repair_marker())
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
            .is_some_and(processes::alive)
    }
}

/// Reads `spaceo doctor --json` into a state and the words a notice shows.
pub fn parse_doctor(
    doctor: Option<&Value>,
    since: Option<SystemTime>,
    launch_agent: bool,
) -> Health {
    let base = |state, title: &str, detail: String| Health {
        state,
        title: title.to_string(),
        detail,
        spaceo_reason: None,
        since,
        repairable: false,
        daemon_pid: None,
        launch_agent_installed: launch_agent,
    };
    let Some(doctor) = doctor else {
        return base(
            State::Unknown,
            "SpaceO didn't answer",
            "Ghostex couldn't ask SpaceO how it is. Run spaceo doctor in a terminal to see why."
                .to_string(),
        );
    };
    let daemon = doctor.get("daemon");
    let running = daemon
        .and_then(|daemon| daemon.get("running"))
        .and_then(Value::as_bool)
        == Some(true);
    let daemon_pid = daemon
        .and_then(|daemon| daemon.get("pid"))
        .and_then(Value::as_i64);
    let safety = doctor
        .get("displaySafety")
        .or_else(|| daemon.and_then(|daemon| daemon.get("displaySafety")));
    let blocked = safety
        .and_then(|safety| safety.get("state"))
        .and_then(Value::as_str)
        == Some("blocked");
    let reason = safety
        .and_then(|safety| safety.get("reason"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if blocked {
        let when = since
            .map(|since| format!(" on {}", short_date(since)))
            .unwrap_or_default();
        let cause = plain_cause(reason.as_deref());
        let mut health = base(
            State::Stuck,
            "SpaceO is stuck",
            format!(
                "It stopped giving agents screens{when} because {cause}, and stays stopped until it is repaired. Repair restarts SpaceO and clears the stop."
            ),
        );
        health.spaceo_reason = reason;
        health.repairable = true;
        health.daemon_pid = daemon_pid;
        return health;
    }
    if !running {
        let mut health = base(
            State::DaemonDown,
            "SpaceO isn't running",
            if launch_agent {
                "Its background service stopped, so agents can't get screens. Ghostex starts it again; Repair does it now.".to_string()
            } else {
                "Its background service isn't set up, so agents can't get screens. Repair sets it up.".to_string()
            },
        );
        health.repairable = true;
        return health;
    }
    let mut health = base(
        State::Healthy,
        "SpaceO is working",
        "Agents can get screens.".to_string(),
    );
    health.daemon_pid = daemon_pid;
    health
}

/// SpaceO's latch reason in plain words.
fn plain_cause(reason: Option<&str>) -> &'static str {
    let reason = reason.unwrap_or_default().to_ascii_lowercase();
    if reason.contains("swap") {
        "your Mac was swapping memory to disk"
    } else if reason.contains("pressure") || reason.contains("memory") {
        "your Mac was low on memory"
    } else if reason.contains("windowserver") {
        "macOS reported a display problem"
    } else if reason.contains("unknown") || reason.contains("not_sampled") {
        "it couldn't confirm your Mac was healthy"
    } else {
        "an earlier screen change didn't finish"
    }
}

// MARK: - Automatic, safe healing

/// What `auto_heal` did, one line each; empty when nothing needed doing.
pub type Actions = Vec<String>;

/// Fixes what needs no operator authority, logs each action and returns them:
/// - ends `spaceo mcp` servers whose agent is gone (orphaned to launchd), and
/// - starts SpaceO's LaunchAgent daemon again when it is installed but not running.
///
/// It never stops a running daemon, clears a safety stop, or runs an `--operator` command.
pub fn auto_heal(spaceo: &Spaceo, health: &Health, source: &str) -> Actions {
    let mut actions = Vec::new();
    for process in processes::list() {
        if process.is_orphaned_mcp() && processes::terminate(process.pid) {
            actions.push(format!(
                "Ended SpaceO's MCP server {} (its agent had exited).",
                process.pid
            ));
        }
    }
    if health.state == State::DaemonDown
        && health.launch_agent_installed
        && !spaceo.repair_running()
        && !processes::list()
            .iter()
            .any(|process| process.kind == ProcessKind::Daemon)
    {
        match start_launch_agent(spaceo) {
            Ok(how) => actions.push(format!("Started SpaceO's background service ({how}).")),
            Err(error) => actions.push(format!(
                "Could not start SpaceO's background service: {error}"
            )),
        }
    }
    for action in &actions {
        spaceo.log(source, action);
    }
    actions
}

fn gui_domain() -> String {
    #[cfg(unix)]
    {
        // SAFETY: getuid has no preconditions.
        format!("gui/{}", unsafe { libc::getuid() })
    }
    #[cfg(not(unix))]
    {
        String::new()
    }
}

fn launchctl(args: &[&str], timeout: Duration) -> Result<Output, String> {
    run::run(Path::new("/bin/launchctl"), args, timeout)
}

fn launch_agent_loaded() -> bool {
    let target = format!("{}/{LAUNCH_AGENT_LABEL}", gui_domain());
    launchctl(&["print", &target], Duration::from_secs(5)).is_ok_and(|output| output.success)
}

/// Starts the LaunchAgent: kickstart when launchd has it, bootstrap when it was taken out.
fn start_launch_agent(spaceo: &Spaceo) -> Result<&'static str, String> {
    let domain = gui_domain();
    let output = if launch_agent_loaded() {
        let target = format!("{domain}/{LAUNCH_AGENT_LABEL}");
        (
            launchctl(&["kickstart", &target], Duration::from_secs(15))?,
            "kickstart",
        )
    } else {
        let plist = spaceo.launch_agent_plist();
        (
            launchctl(
                &["bootstrap", &domain, &plist.to_string_lossy()],
                Duration::from_secs(15),
            )?,
            "bootstrap",
        )
    };
    if output.0.success {
        Ok(output.1)
    } else {
        Err(output.0.text())
    }
}

// MARK: - Repair (the user's click only)

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// SpaceO works again.
    Fixed(String),
    /// SpaceO was already fine.
    NothingToDo(String),
    /// Repair did not start, and says why (agents still using SpaceO, a repair already running).
    Refused(String),
    /// Repair ran, and SpaceO still refuses screens; SpaceO's own words follow.
    StillStuck(String),
}

impl Outcome {
    pub fn ok(&self) -> bool {
        matches!(self, Self::Fixed(_) | Self::NothingToDo(_))
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Fixed(text)
            | Self::NothingToDo(text)
            | Self::Refused(text)
            | Self::StillStuck(text) => text,
        }
    }
}

/// Removes the running-repair marker when the repair ends, however it ends.
struct RepairGuard(PathBuf);

impl Drop for RepairGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// SpaceO's operator recovery, for the user's own Repair click or `ghostex spaceo repair` at a
/// terminal. Never call it on an agent's behalf: it runs `--operator` commands.
///
/// Stuck: refuses while agents hold sessions; otherwise takes the daemon out of launchd, stops it
/// and SpaceO Viewer, runs `spaceo safety clear-host-health --operator`, puts the daemon back,
/// and checks again. Not running: starts (or installs) the LaunchAgent. `progress` gets each step.
pub fn repair(spaceo: &Spaceo, source: &str, progress: &mut dyn FnMut(&str)) -> Outcome {
    let mut step = |text: &str| {
        spaceo.log(source, text);
        progress(text);
    };
    if spaceo.repair_running() {
        return Outcome::Refused(
            "A SpaceO repair is already running. Wait for it to finish.".to_string(),
        );
    }
    let marker = spaceo.repair_marker();
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, std::process::id().to_string());
    let _guard = RepairGuard(marker);

    step("Checking SpaceO…");
    let before = spaceo.check();
    let outcome = match before.state {
        State::Healthy => {
            Outcome::NothingToDo("SpaceO is working; nothing needed repairing.".to_string())
        }
        State::Unknown => Outcome::StillStuck(before.detail.clone()),
        State::DaemonDown => repair_daemon_down(spaceo, &mut step),
        State::Stuck => repair_latch(spaceo, &before, &mut step),
    };
    spaceo.log(source, &format!("Repair finished: {:?}", outcome));
    outcome
}

fn repair_daemon_down(spaceo: &Spaceo, step: &mut dyn FnMut(&str)) -> Outcome {
    if spaceo.launch_agent_plist().is_file() {
        step("Starting SpaceO's background service…");
        if let Err(error) = start_launch_agent(spaceo) {
            return Outcome::StillStuck(format!(
                "SpaceO's background service didn't start: {error}"
            ));
        }
    } else {
        step("Setting up SpaceO's background service…");
        match spaceo.spaceo(&["daemon", "install", "--yes"], Duration::from_secs(60)) {
            Ok(output) if output.success => {}
            Ok(output) => {
                return Outcome::StillStuck(format!(
                    "SpaceO's background service couldn't be set up: {}",
                    output.text()
                ))
            }
            Err(error) => return Outcome::StillStuck(error),
        }
    }
    wait_for_daemon(spaceo);
    verdict(spaceo, step)
}

fn repair_latch(spaceo: &Spaceo, before: &Health, step: &mut dyn FnMut(&str)) -> Outcome {
    step("Checking for agents using SpaceO…");
    if let Some(sessions) = live_sessions(spaceo).filter(|count| *count > 0) {
        return Outcome::Refused(format!(
            "{sessions} agent screen{} still open in SpaceO. Repair stops SpaceO, so let {} finish, then repair again.",
            if sessions == 1 { " is" } else { "s are" },
            if sessions == 1 { "it" } else { "them" },
        ));
    }
    let had_launch_agent = launch_agent_loaded();
    let viewer_was_running = processes::list()
        .iter()
        .any(|process| process.kind == ProcessKind::Viewer);

    step("Stopping SpaceO…");
    let stopped = stop_owners(spaceo, had_launch_agent, viewer_was_running);

    let mut cleared: Result<Output, String> = Err("SpaceO did not stop".to_string());
    if stopped.is_ok() {
        step("Clearing SpaceO's safety stop…");
        for attempt in 0..3 {
            cleared = spaceo.spaceo(
                &["safety", "clear-host-health", "--operator", "--json"],
                Duration::from_secs(180),
            );
            let owned = cleared.as_ref().is_ok_and(|output| {
                !output.success && output.text().contains("owns the display lifecycle")
            });
            if !owned || attempt == 2 {
                break;
            }
            // Something started a daemon again in the gap (an agent's MCP server asks for one).
            step("Another SpaceO process started; stopping it and trying again…");
            let _ = stop_owners(spaceo, false, false);
        }
    }

    step("Starting SpaceO again…");
    if had_launch_agent || spaceo.launch_agent_plist().is_file() {
        if let Err(error) = start_launch_agent(spaceo) {
            spaceo.log("repair", &format!("LaunchAgent did not start: {error}"));
        }
    }
    wait_for_daemon(spaceo);
    if viewer_was_running {
        let _ = run::run(
            Path::new("/usr/bin/open"),
            &["-g", "-b", VIEWER_BUNDLE_ID],
            Duration::from_secs(10),
        );
    }

    if let Err(error) = stopped {
        return Outcome::StillStuck(format!(
            "SpaceO couldn't be stopped, so its safety stop is still on. {error}"
        ));
    }
    match cleared {
        Ok(output) if output.success => verdict(spaceo, step),
        Ok(output) => Outcome::StillStuck(format!(
            "SpaceO refused to clear its safety stop{}. SpaceO said: {}",
            before
                .spaceo_reason
                .as_deref()
                .map(|reason| format!(" ({reason})"))
                .unwrap_or_default(),
            first_line(&output.text())
        )),
        Err(error) => Outcome::StillStuck(format!("SpaceO's recovery didn't run: {error}")),
    }
}

/// Takes the daemon out of launchd (so KeepAlive doesn't bring it straight back), stops any
/// daemon still answering, quits SpaceO Viewer, and waits until no daemon process is left.
fn stop_owners(spaceo: &Spaceo, launch_agent: bool, viewer: bool) -> Result<(), String> {
    if launch_agent {
        let target = format!("{}/{LAUNCH_AGENT_LABEL}", gui_domain());
        let _ = launchctl(&["bootout", &target], Duration::from_secs(60));
    }
    if viewer {
        let _ = run::run(
            Path::new("/usr/bin/osascript"),
            &[
                "-e",
                &format!("tell application id \"{VIEWER_BUNDLE_ID}\" to quit"),
            ],
            Duration::from_secs(15),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut asked_daemon_to_stop = false;
    loop {
        let left: Vec<Process> = processes::list()
            .into_iter()
            .filter(|process| matches!(process.kind, ProcessKind::Daemon | ProcessKind::Viewer))
            .collect();
        if left.is_empty() {
            return Ok(());
        }
        if !asked_daemon_to_stop
            && left
                .iter()
                .any(|process| process.kind == ProcessKind::Daemon)
        {
            asked_daemon_to_stop = true;
            let _ = spaceo.spaceo(
                &["daemon", "stop", "--operator", "--timeout", "20"],
                Duration::from_secs(30),
            );
            continue;
        }
        if Instant::now() >= deadline {
            let names: Vec<String> = left
                .iter()
                .map(|process| format!("{} (pid {})", process.command, process.pid))
                .collect();
            return Err(format!("Still running: {}", names.join(", ")));
        }
        thread::sleep(Duration::from_millis(250));
    }
}

/// How many sessions SpaceO is hosting; `None` when no daemon answers.
fn live_sessions(spaceo: &Spaceo) -> Option<usize> {
    let output = spaceo
        .spaceo(
            &["session", "list", "--operator", "--json"],
            Duration::from_secs(10),
        )
        .ok()
        .filter(|output| output.success)?;
    run::json_object(&output.stdout)?
        .get("sessions")?
        .as_array()
        .map(Vec::len)
}

fn wait_for_daemon(spaceo: &Spaceo) {
    let _ = spaceo.spaceo(
        &["daemon", "wait", "--timeout", "20"],
        Duration::from_secs(25),
    );
}

fn verdict(spaceo: &Spaceo, step: &mut dyn FnMut(&str)) -> Outcome {
    step("Checking SpaceO again…");
    let after = spaceo.check();
    match after.state {
        State::Healthy => {
            Outcome::Fixed("SpaceO is working again. Agents can get screens.".to_string())
        }
        _ => Outcome::StillStuck(after.summary()),
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no output")
        .to_string()
}

// MARK: - SpaceO's log

/// True when the daemon log lines `text` hold a failure worth checking SpaceO for: a refused
/// agent screen, a tripped safety stop, or the daemon stopping.
pub fn log_shows_failure(text: &str) -> bool {
    text.lines().any(|line| {
        line.contains("\"display_creation_failed\"")
            || line.contains("\"kind\":\"display.safety.tripped\"")
            || line.contains("\"kind\":\"daemon.stopped\"")
            || line.contains("\"kind\":\"daemon.signal-stop-failed\"")
    })
}

/// Follows SpaceO's daemon log: each call returns what was appended since the last one.
#[derive(Debug, Default)]
pub struct LogCursor {
    offset: Option<u64>,
}

impl LogCursor {
    /// New text since the last call; the first call only notes where the log ends.
    pub fn read_new(&mut self, path: &Path) -> String {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut file) = fs::File::open(path) else {
            return String::new();
        };
        let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        let start = match self.offset {
            None => {
                self.offset = Some(len);
                return String::new();
            }
            // A shorter file was rotated or truncated: read it from the start.
            Some(offset) if offset > len => 0,
            Some(offset) => offset,
        };
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(start)).is_ok() {
            let _ = file.take(256 * 1024).read_to_end(&mut bytes);
        }
        self.offset = Some(start + bytes.len() as u64);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

// MARK: - Time

fn civil(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let secs = seconds.rem_euclid(86_400) as u32;
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, secs / 3600, secs / 60 % 60, secs % 60)
}

fn unix_seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// `2026-10-25T02:05:25Z`.
pub fn iso_time(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = civil(unix_seconds(time));
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// `Oct 25`, in this Mac's time zone.
fn short_date(time: SystemTime) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let seconds = unix_seconds(time) + local_offset_seconds(time);
    let (_, month, day, ..) = civil(seconds);
    format!("{} {day}", MONTHS[(month as usize).saturating_sub(1) % 12])
}

fn local_offset_seconds(time: SystemTime) -> i64 {
    #[cfg(unix)]
    {
        let seconds = unix_seconds(time) as libc::time_t;
        // SAFETY: localtime_r writes only into `tm`.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if unsafe { libc::localtime_r(&seconds, &mut tm) }.is_null() {
            return 0;
        }
        tm.tm_gmtoff
    }
    #[cfg(not(unix))]
    {
        let _ = time;
        0
    }
}
