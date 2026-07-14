use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::Command as StdCommand,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::{domain::DomainStateError, paths::GxserverPaths, runtime::is_process_running};

pub const T3_RUNTIME_HOST: &str = "127.0.0.1";
// The managed T3 runtime is only ever reached over loopback (see
// `t3_http_request`, which connects to `T3_RUNTIME_HOST`). Bind the listener to
// loopback as well so the runtime is not reachable from other hosts, containers,
// or bridged interfaces. Do not widen this to `0.0.0.0` without an explicit,
// documented remote opt-in and verified auth on every T3 endpoint.
const T3_RUNTIME_LISTEN_HOST: &str = "127.0.0.1";
pub const T3_RUNTIME_PORT: u16 = 3774;
const T3_APP_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const T3_STARTUP_GRACE_INTERVAL: Duration = Duration::from_secs(30);
const T3_STARTUP_UNRESPONSIVE_RETENTION_SECONDS: u64 = 90;
const T3_STALE_RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const T3_APP_CLOSED_RUNTIME_SHUTDOWN_TIMEOUT_SECONDS: u64 = 180;
const T3_RUNTIME_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
const T3_RUNTIME_HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const T3_OWNER_BEARER_MAX_ATTEMPTS: u32 = 40;
const T3_OWNER_BEARER_RETRY_DELAY: Duration = Duration::from_millis(500);
const T3_OWNER_BEARER_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const T3_AUTH_PROVIDER: &str = "t3code";
const T3_MANAGED_HOME_DIRECTORY_NAME: &str = "managed-home-t3code-0.0.0";
const T3_APP_HEARTBEAT_FILE_NAME: &str = "ghostex-app-heartbeat";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3RuntimeStatusPayload {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    pub auth_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct T3RuntimeLaunchPlan {
    pub entrypoint_path: String,
    pub runtime_path: String,
}

#[derive(Clone, Debug)]
pub struct T3RuntimeStartRequest {
    pub cwd: PathBuf,
    pub plan: Option<T3RuntimeLaunchPlan>,
}

#[derive(Clone, Debug)]
pub struct T3ListeningProcess {
    pub command: String,
    pub parent_command: String,
    pub parent_pid: Option<u32>,
    pub pid: u32,
}

pub trait T3RuntimePortProbe: Send + Sync {
    fn listening_processes(&self) -> Vec<T3ListeningProcess>;
    fn process_age_seconds(&self, pid: u32) -> Option<u64>;
}

struct LsofT3RuntimePortProbe;

impl T3RuntimePortProbe for LsofT3RuntimePortProbe {
    fn listening_processes(&self) -> Vec<T3ListeningProcess> {
        let Ok(output) = StdCommand::new("/usr/sbin/lsof")
            .args(["-nP", &format!("-tiTCP:{T3_RUNTIME_PORT}"), "-sTCP:LISTEN"])
            .output()
        else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .map(|pid| {
                let parent_pid = process_parent_pid(pid);
                T3ListeningProcess {
                    command: process_command(pid),
                    parent_command: parent_pid.map(process_command).unwrap_or_default(),
                    parent_pid,
                    pid,
                }
            })
            .collect()
    }

    fn process_age_seconds(&self, pid: u32) -> Option<u64> {
        let output = StdCommand::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "etime="])
            .output()
            .ok()?;
        parse_ps_elapsed_seconds(&String::from_utf8_lossy(&output.stdout))
    }
}

fn process_command(pid: u32) -> String {
    StdCommand::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn process_parent_pid(pid: u32) -> Option<u32> {
    let output = StdCommand::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "ppid="])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|parent_pid| *parent_pid > 0)
}

fn parse_ps_elapsed_seconds(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (days, time_part) = match trimmed.split_once('-') {
        Some((days, time_part)) => (days.parse::<u64>().ok()?, time_part),
        None => (0, trimmed),
    };
    let parts: Vec<&str> = time_part.split(':').collect();
    if parts.len() != 2 && parts.len() != 3 {
        return None;
    }
    let seconds = parts[parts.len() - 1].parse::<u64>().ok()?;
    let minutes = parts[parts.len() - 2].parse::<u64>().ok()?;
    let hours = if parts.len() == 3 {
        parts[0].parse::<u64>().ok()?
    } else {
        0
    };
    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

#[derive(Clone, Debug)]
pub struct T3RuntimePaths {
    pub auth_state_file: PathBuf,
    pub heartbeat_file: PathBuf,
    pub launch_plan_file: PathBuf,
    pub runtime_storage_dir: PathBuf,
    pub t3_home_dir: PathBuf,
}

pub fn t3_runtime_paths_for(paths: &GxserverPaths) -> T3RuntimePaths {
    let shared_root = paths
        .root_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| paths.root_dir.clone());
    let runtime_storage_dir = shared_root.join("t3-runtime");
    let t3_home_dir = runtime_storage_dir.join(T3_MANAGED_HOME_DIRECTORY_NAME);
    T3RuntimePaths {
        auth_state_file: runtime_storage_dir.join("auth-state.json"),
        heartbeat_file: t3_home_dir.join(T3_APP_HEARTBEAT_FILE_NAME),
        launch_plan_file: paths.runtime_dir.join("t3-runtime-launch-plan.json"),
        runtime_storage_dir,
        t3_home_dir,
    }
}

#[derive(Default)]
struct T3RuntimeManagerInner {
    bearer_mint_task: Option<tokio::task::JoinHandle<()>>,
    bootstrap_credential: Option<String>,
    child_pid: Option<u32>,
    child_started_at: Option<String>,
    ensure_task: Option<tokio::task::JoinHandle<()>>,
    heartbeat_task: Option<tokio::task::JoinHandle<()>>,
    launch_attempt_started_at: Option<Instant>,
    owner_bearer_token: Option<String>,
    pane_session_ids: HashMap<String, BTreeSet<String>>,
}

enum T3EnsureDecision {
    AlreadyRunning,
    Spawn(T3ResolvedRuntimeCommand),
}

struct T3ResolvedRuntimeCommand {
    command: String,
    plan: T3RuntimeLaunchPlan,
}

enum T3BearerExchangeOutcome {
    Minted(String),
    Retry,
    Unauthorized,
}

#[derive(Clone)]
pub struct T3RuntimeManager {
    inner: Arc<Mutex<T3RuntimeManagerInner>>,
    probe: Arc<dyn T3RuntimePortProbe>,
    t3_paths: Arc<T3RuntimePaths>,
}

impl T3RuntimeManager {
    pub fn new(paths: &GxserverPaths) -> Self {
        Self::with_probe(paths, Arc::new(LsofT3RuntimePortProbe))
    }

    pub fn with_probe(paths: &GxserverPaths, probe: Arc<dyn T3RuntimePortProbe>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(T3RuntimeManagerInner::default())),
            probe,
            t3_paths: Arc::new(t3_runtime_paths_for(paths)),
        }
    }

    pub fn status_snapshot(&self) -> T3RuntimeStatusPayload {
        let auth_ready = self.read_persisted_owner_bearer_token().is_some();
        let (tracked_pid, tracked_started_at) = {
            let inner = self.lock_inner();
            (inner.child_pid, inner.child_started_at.clone())
        };
        let tracked_alive = tracked_pid.filter(|pid| is_process_running(*pid));
        let listeners = self.probe.listening_processes();
        if let Some(listener) = listeners
            .iter()
            .find(|listener| is_any_t3_runtime_command(&listener.command))
        {
            let owned = tracked_alive.is_some()
                || (self.is_owned_t3_runtime_process(listener)
                    && self.has_recoverable_auth_material());
            let started_at = tracked_alive
                .and(tracked_started_at)
                .or_else(|| {
                    self.probe
                        .process_age_seconds(listener.pid)
                        .map(started_at_for_age_seconds)
                });
            return T3RuntimeStatusPayload {
                running: true,
                pid: Some(listener.pid),
                port: T3_RUNTIME_PORT,
                started_at,
                auth_ready,
                ownership: Some(if owned { "gxserver" } else { "external" }.to_string()),
            };
        }
        if let Some(pid) = tracked_alive {
            return T3RuntimeStatusPayload {
                running: true,
                pid: Some(pid),
                port: T3_RUNTIME_PORT,
                started_at: tracked_started_at,
                auth_ready,
                ownership: Some("gxserver".to_string()),
            };
        }
        T3RuntimeStatusPayload {
            running: false,
            pid: None,
            port: T3_RUNTIME_PORT,
            started_at: None,
            auth_ready,
            ownership: None,
        }
    }

    pub fn request_start(&self, request: T3RuntimeStartRequest) {
        let claim = {
            let mut inner = self.lock_inner();
            match inner.launch_attempt_started_at {
                Some(started_at) if started_at.elapsed() <= T3_STARTUP_GRACE_INTERVAL => None,
                _ => {
                    let claim = Instant::now();
                    inner.launch_attempt_started_at = Some(claim);
                    Some(claim)
                }
            }
        };
        let Some(claim) = claim else {
            return;
        };
        let manager = self.clone();
        let handle = tokio::spawn(async move {
            manager.run_ensure(request, claim).await;
        });
        self.lock_inner().ensure_task = Some(handle);
    }

    async fn run_ensure(&self, request: T3RuntimeStartRequest, claim: Instant) {
        let manager = self.clone();
        let plan = request.plan.clone();
        let decision =
            tokio::task::spawn_blocking(move || manager.prepare_launch(plan.as_ref())).await;
        match decision {
            Ok(Ok(T3EnsureDecision::AlreadyRunning)) => {
                self.spawn_owner_bearer_mint_task();
            }
            Ok(Ok(T3EnsureDecision::Spawn(resolved))) => {
                if self.spawn_runtime(&request.cwd, &resolved).await.is_ok() {
                    self.spawn_owner_bearer_mint_task();
                }
            }
            _ => {}
        }
        let mut inner = self.lock_inner();
        if inner.launch_attempt_started_at == Some(claim) {
            inner.launch_attempt_started_at = None;
        }
    }

    fn prepare_launch(
        &self,
        plan: Option<&T3RuntimeLaunchPlan>,
    ) -> Result<T3EnsureDecision, DomainStateError> {
        let tracked_alive = self
            .lock_inner()
            .child_pid
            .is_some_and(is_process_running);
        if tracked_alive {
            return Ok(T3EnsureDecision::AlreadyRunning);
        }
        if self.has_responsive_owned_runtime_listener() {
            return Ok(T3EnsureDecision::AlreadyRunning);
        }
        self.clear_stale_runtime_if_needed(false);
        Ok(T3EnsureDecision::Spawn(self.resolve_runtime_command(plan)?))
    }

    async fn spawn_runtime(
        &self,
        cwd: &Path,
        resolved: &T3ResolvedRuntimeCommand,
    ) -> Result<(), DomainStateError> {
        fs::create_dir_all(&self.t3_paths.t3_home_dir).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Failed to create the managed T3 home directory: {error}"),
        })?;
        self.touch_app_heartbeat();
        let credential = Uuid::new_v4().to_string().to_uppercase();
        self.write_auth_state(&json!({
            "desktopBootstrapToken": credential,
            "provider": T3_AUTH_PROVIDER,
        }))?;
        let mut bootstrap_bytes = serde_json::to_vec(&json!({
            "desktopBootstrapToken": credential,
            "host": T3_RUNTIME_HOST,
            "mode": "desktop",
            "noBrowser": true,
            "port": T3_RUNTIME_PORT,
            "t3Home": path_text(&self.t3_paths.t3_home_dir),
            "tailscaleServeEnabled": false,
            "tailscaleServePort": 443,
        }))
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Failed to encode the T3 bootstrap envelope: {error}"),
        })?;
        bootstrap_bytes.push(b'\n');
        let bootstrap_path = self.t3_paths.t3_home_dir.join(format!(
            "bootstrap-{}.json",
            Uuid::new_v4().to_string().to_uppercase()
        ));
        fs::write(&bootstrap_path, bootstrap_bytes).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Failed to write the T3 bootstrap file: {error}"),
        })?;
        {
            let mut inner = self.lock_inner();
            inner.bootstrap_credential = Some(credential);
            inner.owner_bearer_token = None;
        }

        let script = build_launch_script(
            &bootstrap_path,
            &self.t3_paths.heartbeat_file,
            &resolved.command,
        );
        let mut command = tokio::process::Command::new("/bin/zsh");
        command
            .arg("-lc")
            .arg(script)
            .current_dir(cwd)
            .env("T3CODE_AUTO_BOOTSTRAP_PROJECT_FROM_CWD", "false")
            .env("T3CODE_HOME", path_text(&self.t3_paths.t3_home_dir))
            .env("T3CODE_HOST", T3_RUNTIME_HOST)
            .env("T3CODE_NO_BROWSER", "true")
            .env("T3CODE_PORT", T3_RUNTIME_PORT.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        detach_runtime_process_group(&mut command);
        let mut child = command.spawn().map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Failed to launch the managed T3 runtime: {error}"),
        })?;
        let pid = child.id();
        let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        {
            let mut inner = self.lock_inner();
            inner.child_pid = pid;
            inner.child_started_at = Some(started_at);
        }
        self.persist_launch_plan(&resolved.plan);
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(read_runtime_output_bounded(stdout));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(read_runtime_output_bounded(stderr));
        }
        let manager = self.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            let mut inner = manager.lock_inner();
            if inner.child_pid == pid {
                inner.child_pid = None;
                inner.child_started_at = None;
            }
        });
        Ok(())
    }

    pub fn stop_runtime(&self) -> T3RuntimeStatusPayload {
        let tracked_pid = {
            let mut inner = self.lock_inner();
            let pid = inner.child_pid.take();
            inner.child_started_at = None;
            pid
        };
        if let Some(pid) = tracked_pid.filter(|pid| is_process_running(*pid)) {
            signal_runtime_process_group(pid, TerminationSignal::Terminate);
            let deadline = Instant::now() + T3_STALE_RUNTIME_SHUTDOWN_TIMEOUT;
            while Instant::now() < deadline && is_process_running(pid) {
                std::thread::sleep(Duration::from_millis(100));
            }
            if is_process_running(pid) {
                signal_runtime_process_group(pid, TerminationSignal::Kill);
            }
        }
        self.clear_stale_runtime_if_needed(true);
        self.status_snapshot()
    }

    pub fn update_panes(&self, client_id: String, session_ids: BTreeSet<String>) {
        let union_is_empty = {
            let mut inner = self.lock_inner();
            if session_ids.is_empty() {
                inner.pane_session_ids.remove(&client_id);
            } else {
                inner.pane_session_ids.insert(client_id, session_ids);
            }
            inner.pane_session_ids.values().all(BTreeSet::is_empty)
        };
        if union_is_empty {
            if let Some(task) = self.lock_inner().heartbeat_task.take() {
                task.abort();
            }
            return;
        }
        self.touch_app_heartbeat();
        let mut inner = self.lock_inner();
        let heartbeat_running = inner
            .heartbeat_task
            .as_ref()
            .is_some_and(|task| !task.is_finished());
        if !heartbeat_running {
            let manager = self.clone();
            inner.heartbeat_task = Some(tokio::spawn(async move {
                loop {
                    tokio::time::sleep(T3_APP_HEARTBEAT_INTERVAL).await;
                    let has_live_panes = {
                        let inner = manager.lock_inner();
                        inner.pane_session_ids.values().any(|ids| !ids.is_empty())
                    };
                    if !has_live_panes {
                        return;
                    }
                    manager.touch_app_heartbeat();
                }
            }));
        }
    }

    pub fn abort_background_tasks(&self) {
        let mut inner = self.lock_inner();
        for task in [
            inner.bearer_mint_task.take(),
            inner.ensure_task.take(),
            inner.heartbeat_task.take(),
        ]
        .into_iter()
        .flatten()
        {
            task.abort();
        }
    }

    fn spawn_owner_bearer_mint_task(&self) {
        {
            let inner = self.lock_inner();
            if inner
                .bearer_mint_task
                .as_ref()
                .is_some_and(|task| !task.is_finished())
            {
                return;
            }
        }
        if self.read_persisted_owner_bearer_token().is_some() {
            return;
        }
        let manager = self.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..T3_OWNER_BEARER_MAX_ATTEMPTS {
                let credential = manager
                    .current_bootstrap_credential()
                    .or_else(|| manager.read_persisted_desktop_bootstrap_token());
                if let Some(credential) = credential {
                    let exchange_credential = credential.clone();
                    let outcome = tokio::task::spawn_blocking(move || {
                        exchange_bootstrap_for_owner_bearer(&exchange_credential)
                    })
                    .await;
                    match outcome {
                        Ok(T3BearerExchangeOutcome::Minted(token)) => {
                            manager.clear_bootstrap_credential(&credential);
                            manager.remember_owner_bearer_token(&token);
                            return;
                        }
                        Ok(T3BearerExchangeOutcome::Unauthorized) => {
                            manager.discard_desktop_bootstrap_credential(&credential);
                            return;
                        }
                        _ => {}
                    }
                }
                tokio::time::sleep(T3_OWNER_BEARER_RETRY_DELAY).await;
            }
        });
        self.lock_inner().bearer_mint_task = Some(handle);
    }

    fn clear_stale_runtime_if_needed(&self, force_owned_runtime_stop: bool) {
        let listeners = self.probe.listening_processes();
        if listeners.is_empty() {
            return;
        }
        let heartbeat_age_seconds = self.app_heartbeat_age_seconds();
        for listener in &listeners {
            if !is_any_t3_runtime_command(&listener.command) {
                continue;
            }
            if !force_owned_runtime_stop
                && self
                    .should_retain_unresponsive_managed_runtime(listener.pid, heartbeat_age_seconds)
            {
                continue;
            }
            if is_t3_runtime_supervisor_command(&listener.parent_command) {
                if let Some(parent_pid) = listener.parent_pid {
                    signal_runtime_pid(parent_pid, TerminationSignal::Terminate);
                }
            }
            signal_runtime_pid(listener.pid, TerminationSignal::Terminate);
            self.wait_for_port_to_clear();
        }
        let remaining_owned_listeners: Vec<T3ListeningProcess> = self
            .probe
            .listening_processes()
            .into_iter()
            .filter(|listener| {
                self.is_owned_t3_runtime_process(listener)
                    && (force_owned_runtime_stop
                        || !self.should_retain_unresponsive_managed_runtime(
                            listener.pid,
                            heartbeat_age_seconds,
                        ))
            })
            .collect();
        for listener in &remaining_owned_listeners {
            if is_t3_runtime_supervisor_command(&listener.parent_command) {
                if let Some(parent_pid) = listener.parent_pid {
                    signal_runtime_pid(parent_pid, TerminationSignal::Kill);
                }
            }
            signal_runtime_pid(listener.pid, TerminationSignal::Kill);
        }
        if !remaining_owned_listeners.is_empty() {
            self.wait_for_port_to_clear();
        }
    }

    fn should_retain_unresponsive_managed_runtime(
        &self,
        pid: u32,
        heartbeat_age_seconds: Option<u64>,
    ) -> bool {
        let heartbeat_is_fresh = heartbeat_age_seconds
            .is_some_and(|age| age < T3_APP_CLOSED_RUNTIME_SHUTDOWN_TIMEOUT_SECONDS);
        if !heartbeat_is_fresh {
            return false;
        }
        if !self.has_recoverable_auth_material() {
            return false;
        }
        self.probe
            .process_age_seconds(pid)
            .is_some_and(|age| age <= T3_STARTUP_UNRESPONSIVE_RETENTION_SECONDS)
    }

    fn has_responsive_owned_runtime_listener(&self) -> bool {
        let owned_listeners: Vec<T3ListeningProcess> = self
            .probe
            .listening_processes()
            .into_iter()
            .filter(|listener| self.is_owned_t3_runtime_process(listener))
            .collect();
        if owned_listeners.is_empty() {
            return false;
        }
        if !probe_managed_runtime_endpoint("/.well-known/t3/environment")
            || !probe_managed_runtime_endpoint("/api/auth/session")
        {
            return false;
        }
        if owned_listeners
            .iter()
            .any(|listener| is_t3_runtime_supervisor_command(&listener.parent_command))
        {
            return true;
        }
        self.current_owner_bearer_token().is_some()
            || self.read_persisted_owner_bearer_token().is_some()
    }

    fn is_owned_t3_runtime_process(&self, process: &T3ListeningProcess) -> bool {
        if !is_any_t3_runtime_command(&process.command) {
            return false;
        }
        if is_t3_runtime_supervisor_command(&process.parent_command) {
            return true;
        }
        let normalized = process.command.to_lowercase();
        let mut owned_markers = vec![
            path_text(&self.t3_paths.t3_home_dir).to_lowercase(),
            "t3code-server/dist/bin.mjs".to_string(),
        ];
        if let Some(repo_root) = trimmed_env_var("ghostex_REPO_ROOT") {
            owned_markers.push(
                path_text(
                    &PathBuf::from(repo_root)
                        .join("t3code")
                        .join("apps/server/src/bin.ts"),
                )
                .to_lowercase(),
            );
        }
        if let Some(plan) = self.read_persisted_launch_plan() {
            owned_markers.push(plan.entrypoint_path.to_lowercase());
        }
        owned_markers
            .iter()
            .any(|marker| !marker.is_empty() && normalized.contains(marker))
    }

    fn resolve_runtime_command(
        &self,
        plan: Option<&T3RuntimeLaunchPlan>,
    ) -> Result<T3ResolvedRuntimeCommand, DomainStateError> {
        if let Some(plan) = plan {
            return Ok(resolved_command_for_plan(plan.clone()));
        }
        if let Some(plan) = self.read_persisted_launch_plan() {
            if Path::new(&plan.runtime_path).is_file() && Path::new(&plan.entrypoint_path).is_file()
            {
                return Ok(resolved_command_for_plan(plan));
            }
        }
        let repo_root = resolve_t3_repo_root().ok_or_else(|| DomainStateError {
            code: "internalError",
            message: "The managed T3 Code runtime source is missing. Pass nodePath and entrypointPath, initialize the t3code submodule, or set VSMUX_T3CODE_REPO_ROOT for development.".to_string(),
        })?;
        let entrypoint = repo_root.join("apps/server/src/bin.ts");
        if !repo_root.join("node_modules").exists() {
            return Err(DomainStateError {
                code: "internalError",
                message: format!(
                    "Managed T3 Code dependencies are missing. Run 'bun install' in {}.",
                    path_text(&repo_root)
                ),
            });
        }
        let bun_path = resolve_command_path("bun").ok_or_else(|| DomainStateError {
            code: "internalError",
            message: "Unable to resolve required runtime command: bun.".to_string(),
        })?;
        Ok(resolved_command_for_plan(T3RuntimeLaunchPlan {
            entrypoint_path: path_text(&entrypoint),
            runtime_path: bun_path,
        }))
    }

    fn persist_launch_plan(&self, plan: &T3RuntimeLaunchPlan) {
        let Some(parent) = self.t3_paths.launch_plan_file.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        if let Ok(text) = serde_json::to_string_pretty(plan) {
            let _ = fs::write(&self.t3_paths.launch_plan_file, format!("{text}\n"));
        }
    }

    fn read_persisted_launch_plan(&self) -> Option<T3RuntimeLaunchPlan> {
        let text = fs::read_to_string(&self.t3_paths.launch_plan_file).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn touch_app_heartbeat(&self) {
        let _ = fs::create_dir_all(&self.t3_paths.t3_home_dir);
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let _ = fs::write(&self.t3_paths.heartbeat_file, timestamp);
    }

    fn app_heartbeat_age_seconds(&self) -> Option<u64> {
        let text = fs::read_to_string(&self.t3_paths.heartbeat_file).ok()?;
        let timestamp = text.trim().parse::<f64>().ok()?;
        let age = chrono::Utc::now().timestamp() as f64 - timestamp;
        Some(age.max(0.0) as u64)
    }

    fn has_recoverable_auth_material(&self) -> bool {
        self.current_owner_bearer_token().is_some()
            || self.read_persisted_owner_bearer_token().is_some()
            || self.current_bootstrap_credential().is_some()
            || self.read_persisted_desktop_bootstrap_token().is_some()
    }

    fn current_bootstrap_credential(&self) -> Option<String> {
        self.lock_inner().bootstrap_credential.clone()
    }

    fn current_owner_bearer_token(&self) -> Option<String> {
        self.lock_inner().owner_bearer_token.clone()
    }

    fn clear_bootstrap_credential(&self, credential: &str) {
        let mut inner = self.lock_inner();
        if inner.bootstrap_credential.as_deref() == Some(credential) {
            inner.bootstrap_credential = None;
        }
    }

    fn remember_owner_bearer_token(&self, token: &str) {
        let bootstrap_credential = {
            let mut inner = self.lock_inner();
            inner.owner_bearer_token = Some(token.to_string());
            inner.bootstrap_credential.clone()
        };
        let mut state = Map::new();
        if let Some(bootstrap_credential) = bootstrap_credential {
            state.insert(
                "desktopBootstrapToken".to_string(),
                Value::String(bootstrap_credential),
            );
        }
        state.insert(
            "ownerBearerToken".to_string(),
            Value::String(token.to_string()),
        );
        state.insert(
            "provider".to_string(),
            Value::String(T3_AUTH_PROVIDER.to_string()),
        );
        let _ = self.write_auth_state(&Value::Object(state));
    }

    fn discard_desktop_bootstrap_credential(&self, credential: &str) {
        self.clear_bootstrap_credential(credential);
        let Some(state) = self.read_auth_state() else {
            return;
        };
        if state.get("provider").and_then(Value::as_str) != Some(T3_AUTH_PROVIDER)
            || state.get("desktopBootstrapToken").and_then(Value::as_str) != Some(credential)
        {
            return;
        }
        let mut next_state = Map::new();
        next_state.insert(
            "provider".to_string(),
            Value::String(T3_AUTH_PROVIDER.to_string()),
        );
        if let Some(owner_bearer_token) = state
            .get("ownerBearerToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            next_state.insert(
                "ownerBearerToken".to_string(),
                Value::String(owner_bearer_token.to_string()),
            );
        }
        let _ = self.write_auth_state(&Value::Object(next_state));
    }

    pub fn read_persisted_owner_bearer_token(&self) -> Option<String> {
        let state = self.read_auth_state()?;
        if state.get("provider").and_then(Value::as_str) != Some(T3_AUTH_PROVIDER) {
            return None;
        }
        state
            .get("ownerBearerToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    }

    fn read_persisted_desktop_bootstrap_token(&self) -> Option<String> {
        let state = self.read_auth_state()?;
        if state.get("provider").and_then(Value::as_str) != Some(T3_AUTH_PROVIDER) {
            return None;
        }
        state
            .get("desktopBootstrapToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    }

    fn read_auth_state(&self) -> Option<Map<String, Value>> {
        let text = fs::read_to_string(&self.t3_paths.auth_state_file).ok()?;
        serde_json::from_str::<Value>(&text)
            .ok()?
            .as_object()
            .cloned()
    }

    fn write_auth_state(&self, state: &Value) -> Result<(), DomainStateError> {
        fs::create_dir_all(&self.t3_paths.runtime_storage_dir).map_err(|error| {
            DomainStateError {
                code: "internalError",
                message: format!("Failed to create the T3 runtime storage directory: {error}"),
            }
        })?;
        let text = serde_json::to_string_pretty(state).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Failed to encode the T3 auth state: {error}"),
        })?;
        fs::write(&self.t3_paths.auth_state_file, text).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Failed to write the T3 auth state: {error}"),
        })
    }

    fn wait_for_port_to_clear(&self) {
        let deadline = Instant::now() + T3_STALE_RUNTIME_SHUTDOWN_TIMEOUT;
        while Instant::now() < deadline {
            if self.probe.listening_processes().is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, T3RuntimeManagerInner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub fn parse_t3_runtime_start_params(
    params: &Map<String, Value>,
) -> Result<T3RuntimeStartRequest, DomainStateError> {
    let cwd = params
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request("t3Runtime start requires a non-empty cwd string.")
        })?;
    let cwd = PathBuf::from(cwd);
    if !cwd.is_absolute() {
        return Err(DomainStateError::bad_request(
            "t3Runtime start cwd must be an absolute path.",
        ));
    }
    if !cwd.is_dir() {
        return Err(DomainStateError::bad_request(
            "t3Runtime start cwd must be an existing directory.",
        ));
    }
    let node_path = read_optional_path_param(params, "nodePath")?;
    let entrypoint_path = read_optional_path_param(params, "entrypointPath")?;
    let plan = match (node_path, entrypoint_path) {
        (None, None) => None,
        (Some(node_path), Some(entrypoint_path)) => {
            if !node_path.is_absolute() || !entrypoint_path.is_absolute() {
                return Err(DomainStateError::bad_request(
                    "t3Runtime start nodePath and entrypointPath must be absolute paths.",
                ));
            }
            if !is_executable_file(&node_path) {
                return Err(DomainStateError::bad_request(
                    "t3Runtime start nodePath must be an existing executable file.",
                ));
            }
            if !entrypoint_path.is_file() {
                return Err(DomainStateError::bad_request(
                    "t3Runtime start entrypointPath must be an existing file.",
                ));
            }
            Some(T3RuntimeLaunchPlan {
                entrypoint_path: path_text(&entrypoint_path),
                runtime_path: path_text(&node_path),
            })
        }
        _ => {
            return Err(DomainStateError::bad_request(
                "t3Runtime start requires nodePath and entrypointPath together.",
            ));
        }
    };
    Ok(T3RuntimeStartRequest { cwd, plan })
}

pub fn parse_t3_runtime_panes_params(
    params: &Map<String, Value>,
) -> Result<(String, BTreeSet<String>), DomainStateError> {
    let client_id = params
        .get("clientId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request("t3Runtime panes requires a non-empty clientId string.")
        })?;
    let session_values = params
        .get("sessionIds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            DomainStateError::bad_request("t3Runtime panes requires a sessionIds array.")
        })?;
    let mut session_ids = BTreeSet::new();
    for value in session_values {
        let session_id = value.as_str().ok_or_else(|| {
            DomainStateError::bad_request("t3Runtime panes sessionIds must contain only strings.")
        })?;
        let session_id = session_id.trim();
        if !session_id.is_empty() {
            session_ids.insert(session_id.to_string());
        }
    }
    Ok((client_id.to_string(), session_ids))
}

fn read_optional_path_param(
    params: &Map<String, Value>,
    key: &str,
) -> Result<Option<PathBuf>, DomainStateError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(PathBuf::from(trimmed)))
            }
        }
        Some(_) => Err(DomainStateError::bad_request(format!(
            "t3Runtime start {key} must be a string."
        ))),
    }
}

fn resolved_command_for_plan(plan: T3RuntimeLaunchPlan) -> T3ResolvedRuntimeCommand {
    T3ResolvedRuntimeCommand {
        command: format!(
            "{} {}",
            shell_quote(&plan.runtime_path),
            shell_quote(&plan.entrypoint_path)
        ),
        plan,
    }
}

fn resolve_t3_repo_root() -> Option<PathBuf> {
    let configured_root = trimmed_env_var("VSMUX_T3CODE_REPO_ROOT")
        .or_else(|| trimmed_env_var("ghostex_T3CODE_REPO_ROOT"));
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(configured_root) = configured_root {
        candidates.push(PathBuf::from(configured_root));
    } else {
        if let Some(repo_root) = trimmed_env_var("ghostex_REPO_ROOT") {
            candidates.push(PathBuf::from(repo_root).join("t3code"));
        }
        if let Ok(current_dir) = env::current_dir() {
            candidates.push(current_dir.join("t3code"));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.join("apps/server/src/bin.ts").is_file())
}

fn trimmed_env_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_command_path(command: &str) -> Option<String> {
    let output = StdCommand::new("/bin/zsh")
        .args(["-lc", &format!("command -v {}", shell_quote(command))])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|resolved| !resolved.is_empty())
        .map(str::to_string)
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn is_any_t3_runtime_command(command: &str) -> bool {
    let normalized = command.to_lowercase();
    normalized.contains("t3")
        && normalized.contains("--mode desktop")
        && normalized.contains(&format!("--port {T3_RUNTIME_PORT}"))
}

fn is_t3_runtime_supervisor_command(command: &str) -> bool {
    let normalized = command.to_lowercase();
    normalized.contains("t3-runtime-supervisor.js")
        && normalized.contains(".vscode/extensions/maddada.vsmux")
}

fn build_launch_script(bootstrap_path: &Path, heartbeat_path: &Path, runtime_command: &str) -> String {
    let bootstrap = shell_quote(&path_text(bootstrap_path));
    let heartbeat = shell_quote(&path_text(heartbeat_path));
    let shutdown_timeout = T3_APP_CLOSED_RUNTIME_SHUTDOWN_TIMEOUT_SECONDS.to_string();
    [
        format!("exec 3< {bootstrap}"),
        format!("rm -f {bootstrap}"),
        format!(
            "(\n  runtime_pid=\"\"\n  {runtime_command} --mode desktop --host {T3_RUNTIME_LISTEN_HOST} --port {T3_RUNTIME_PORT} --no-browser --bootstrap-fd 3 &\n  runtime_pid=$!\n  (\n    while kill -0 \"$runtime_pid\" 2>/dev/null; do\n      now=$(date +%s)\n      last=$(cat {heartbeat} 2>/dev/null || echo \"$now\")\n      if [ $((now - last)) -ge {shutdown_timeout} ]; then\n        kill \"$runtime_pid\" 2>/dev/null\n        sleep 2\n        kill -9 \"$runtime_pid\" 2>/dev/null\n        exit 0\n      fi\n      sleep 15\n    done\n  ) &\n  monitor_pid=$!\n  wait \"$runtime_pid\"\n  runtime_status=$?\n  kill \"$monitor_pid\" 2>/dev/null\n  exit \"$runtime_status\"\n)"
        ),
    ]
    .join("\n")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn started_at_for_age_seconds(age_seconds: u64) -> String {
    (chrono::Utc::now() - chrono::Duration::seconds(age_seconds as i64))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn read_runtime_output_bounded<R>(mut reader: R)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut captured: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let remaining = T3_RUNTIME_OUTPUT_LIMIT_BYTES.saturating_sub(captured.len());
                if remaining > 0 {
                    captured.extend_from_slice(&buffer[..read.min(remaining)]);
                }
            }
        }
    }
}

fn probe_managed_runtime_endpoint(path: &str) -> bool {
    match t3_http_request("GET", path, None, T3_RUNTIME_HEALTH_PROBE_TIMEOUT) {
        Some((200, body)) => !body.is_empty(),
        _ => false,
    }
}

fn exchange_bootstrap_for_owner_bearer(credential: &str) -> T3BearerExchangeOutcome {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:token-exchange")
        .append_pair("subject_token", credential)
        .append_pair(
            "subject_token_type",
            "urn:t3:params:oauth:token-type:environment-bootstrap",
        )
        .append_pair(
            "requested_token_type",
            "urn:ietf:params:oauth:token-type:access_token",
        )
        .append_pair(
            "scope",
            "orchestration:read orchestration:operate terminal:operate review:write relay:read access:read access:write relay:write",
        )
        .append_pair("client_label", "Ghostex Native")
        .append_pair("client_device_type", "desktop")
        .append_pair("client_os", "macOS")
        .finish();
    match t3_http_request(
        "POST",
        "/oauth/token",
        Some(("application/x-www-form-urlencoded", body.as_bytes())),
        T3_OWNER_BEARER_REQUEST_TIMEOUT,
    ) {
        Some((200, body)) => match parse_access_token(&body) {
            Some(token) => T3BearerExchangeOutcome::Minted(token),
            None => T3BearerExchangeOutcome::Retry,
        },
        Some((401, _)) => T3BearerExchangeOutcome::Unauthorized,
        _ => T3BearerExchangeOutcome::Retry,
    }
}

fn parse_access_token(body: &[u8]) -> Option<String> {
    let payload: Value = serde_json::from_slice(body).ok()?;
    payload
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

fn t3_http_request(
    method: &str,
    path: &str,
    body: Option<(&str, &[u8])>,
    timeout: Duration,
) -> Option<(u16, Vec<u8>)> {
    let address = format!("{T3_RUNTIME_HOST}:{T3_RUNTIME_PORT}");
    let stream = TcpStream::connect_timeout(&address.parse().ok()?, timeout).ok()?;
    let mut stream = stream;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;

    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nCache-Control: no-cache\r\n"
    );
    if let Some((content_type, payload)) = body {
        request.push_str(&format!("Content-Type: {content_type}\r\n"));
        request.push_str(&format!("Content-Length: {}\r\n", payload.len()));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    if let Some((_, payload)) = body {
        stream.write_all(payload).ok()?;
    }

    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let separator = response.windows(4).position(|window| window == b"\r\n\r\n")?;
    let headers = String::from_utf8_lossy(&response[..separator]).to_string();
    let raw_body = response[separator + 4..].to_vec();
    let status = headers
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse::<u16>()
        .ok()?;
    let body = if has_chunked_transfer_encoding(&headers) {
        decode_chunked_body(&raw_body)?
    } else {
        raw_body
    };
    Some((status, body))
}

fn has_chunked_transfer_encoding(headers: &str) -> bool {
    headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
    })
}

fn decode_chunked_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut index = 0;
    let mut output = Vec::new();
    loop {
        let line_end = body
            .get(index..)?
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|offset| index + offset)?;
        let size_text = std::str::from_utf8(&body[index..line_end])
            .ok()?
            .split(';')
            .next()?
            .trim()
            .to_string();
        let size = usize::from_str_radix(&size_text, 16).ok()?;
        index = line_end + 2;
        if size == 0 {
            return Some(output);
        }
        let chunk_end = index.checked_add(size).filter(|end| *end <= body.len())?;
        output.extend_from_slice(&body[index..chunk_end]);
        index = chunk_end;
        if body.get(index..index + 2) != Some(b"\r\n") {
            return None;
        }
        index += 2;
    }
}

enum TerminationSignal {
    Kill,
    Terminate,
}

#[cfg(unix)]
fn signal_runtime_pid(pid: u32, signal: TerminationSignal) {
    let signal = match signal {
        TerminationSignal::Kill => libc::SIGKILL,
        TerminationSignal::Terminate => libc::SIGTERM,
    };
    unsafe {
        libc::kill(pid as libc::pid_t, signal);
    }
}

#[cfg(not(unix))]
fn signal_runtime_pid(_pid: u32, _signal: TerminationSignal) {}

#[cfg(unix)]
fn signal_runtime_process_group(pid: u32, signal: TerminationSignal) {
    let signal_value = match signal {
        TerminationSignal::Kill => libc::SIGKILL,
        TerminationSignal::Terminate => libc::SIGTERM,
    };
    unsafe {
        libc::kill(-(pid as libc::pid_t), signal_value);
        libc::kill(pid as libc::pid_t, signal_value);
    }
}

#[cfg(not(unix))]
fn signal_runtime_process_group(_pid: u32, _signal: TerminationSignal) {}

fn detach_runtime_process_group(command: &mut tokio::process::Command) {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }

    #[cfg(not(unix))]
    let _ = command;
}

#[cfg(test)]
pub struct StaticT3RuntimePortProbe {
    pub listeners: Vec<T3ListeningProcess>,
}

#[cfg(test)]
impl T3RuntimePortProbe for StaticT3RuntimePortProbe {
    fn listening_processes(&self) -> Vec<T3ListeningProcess> {
        self.listeners.clone()
    }

    fn process_age_seconds(&self, _pid: u32) -> Option<u64> {
        None
    }
}

#[cfg(test)]
pub fn test_t3_runtime_manager(paths: &GxserverPaths) -> T3RuntimeManager {
    T3RuntimeManager::with_probe(
        paths,
        Arc::new(StaticT3RuntimePortProbe {
            listeners: Vec::new(),
        }),
    )
}

#[cfg(test)]
impl T3RuntimeManager {
    pub fn heartbeat_task_is_running(&self) -> bool {
        self.lock_inner()
            .heartbeat_task
            .as_ref()
            .is_some_and(|task| !task.is_finished())
    }

    pub fn pane_session_union(&self) -> BTreeSet<String> {
        self.lock_inner()
            .pane_session_ids
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    pub fn t3_paths(&self) -> &T3RuntimePaths {
        &self.t3_paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;

    fn temp_manager() -> (tempfile::TempDir, T3RuntimeManager) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let manager = test_t3_runtime_manager(&paths);
        (temp, manager)
    }

    #[test]
    fn t3_runtime_paths_derive_from_gxserver_root_parent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let t3_paths = t3_runtime_paths_for(&paths);

        let shared_root = temp.path().join(".ghostex");
        assert_eq!(t3_paths.runtime_storage_dir, shared_root.join("t3-runtime"));
        assert_eq!(
            t3_paths.t3_home_dir,
            shared_root
                .join("t3-runtime")
                .join("managed-home-t3code-0.0.0")
        );
        assert_eq!(
            t3_paths.auth_state_file,
            shared_root.join("t3-runtime").join("auth-state.json")
        );
        assert_eq!(
            t3_paths.heartbeat_file,
            t3_paths.t3_home_dir.join("ghostex-app-heartbeat")
        );
    }

    #[test]
    fn stopped_status_serializes_the_frozen_contract_shape() {
        let (_temp, manager) = temp_manager();

        let status = serde_json::to_value(manager.status_snapshot()).expect("status json");

        assert_eq!(
            status,
            serde_json::json!({
                "running": false,
                "port": 3774,
                "authReady": false,
            })
        );
    }

    #[test]
    fn auth_ready_follows_persisted_owner_bearer_token() {
        let (_temp, manager) = temp_manager();
        assert!(!manager.status_snapshot().auth_ready);

        fs::create_dir_all(&manager.t3_paths().runtime_storage_dir).expect("storage dir");
        fs::write(
            &manager.t3_paths().auth_state_file,
            r#"{"provider":"t3code","ownerBearerToken":"bearer-1"}"#,
        )
        .expect("auth state");
        assert!(manager.status_snapshot().auth_ready);

        fs::write(
            &manager.t3_paths().auth_state_file,
            r#"{"provider":"other","ownerBearerToken":"bearer-1"}"#,
        )
        .expect("auth state");
        assert!(!manager.status_snapshot().auth_ready);
    }

    #[test]
    fn start_params_require_absolute_existing_cwd_and_paired_plan() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cwd = path_text(temp.path());

        let missing_cwd = parse_t3_runtime_start_params(&Map::new()).expect_err("missing cwd");
        assert_eq!(missing_cwd.code, "badRequest");

        let relative_cwd = parse_t3_runtime_start_params(
            serde_json::json!({ "cwd": "relative/path" })
                .as_object()
                .expect("params"),
        )
        .expect_err("relative cwd");
        assert_eq!(relative_cwd.code, "badRequest");

        let missing_directory = parse_t3_runtime_start_params(
            serde_json::json!({ "cwd": temp.path().join("missing").to_string_lossy() })
                .as_object()
                .expect("params"),
        )
        .expect_err("missing directory");
        assert_eq!(missing_directory.code, "badRequest");

        let one_sided_plan = parse_t3_runtime_start_params(
            serde_json::json!({ "cwd": cwd, "nodePath": "/usr/bin/env" })
                .as_object()
                .expect("params"),
        )
        .expect_err("one-sided plan");
        assert_eq!(one_sided_plan.code, "badRequest");

        let missing_node = parse_t3_runtime_start_params(
            serde_json::json!({
                "cwd": cwd,
                "entrypointPath": path_text(&temp.path().join("missing-bin.mjs")),
                "nodePath": path_text(&temp.path().join("missing-node")),
            })
            .as_object()
            .expect("params"),
        )
        .expect_err("missing node");
        assert_eq!(missing_node.code, "badRequest");

        let no_plan = parse_t3_runtime_start_params(
            serde_json::json!({ "cwd": cwd })
                .as_object()
                .expect("params"),
        )
        .expect("cwd-only start");
        assert!(no_plan.plan.is_none());
    }

    #[test]
    fn panes_params_require_client_id_and_string_session_ids() {
        let missing_client = parse_t3_runtime_panes_params(
            serde_json::json!({ "sessionIds": [] })
                .as_object()
                .expect("params"),
        )
        .expect_err("missing clientId");
        assert_eq!(missing_client.code, "badRequest");

        let missing_sessions = parse_t3_runtime_panes_params(
            serde_json::json!({ "clientId": "gpui" })
                .as_object()
                .expect("params"),
        )
        .expect_err("missing sessionIds");
        assert_eq!(missing_sessions.code, "badRequest");

        let non_string_sessions = parse_t3_runtime_panes_params(
            serde_json::json!({ "clientId": "gpui", "sessionIds": ["G1", 4] })
                .as_object()
                .expect("params"),
        )
        .expect_err("non-string session id");
        assert_eq!(non_string_sessions.code, "badRequest");

        let (client_id, session_ids) = parse_t3_runtime_panes_params(
            serde_json::json!({ "clientId": "gpui", "sessionIds": ["G2", "G1", "G2", "  "] })
                .as_object()
                .expect("params"),
        )
        .expect("valid panes params");
        assert_eq!(client_id, "gpui");
        assert_eq!(
            session_ids,
            BTreeSet::from(["G1".to_string(), "G2".to_string()])
        );
    }

    #[tokio::test]
    async fn panes_bookkeeping_unions_clients_and_governs_the_heartbeat_file() {
        let (_temp, manager) = temp_manager();
        assert!(!manager.t3_paths().heartbeat_file.exists());

        manager.update_panes(
            "gpui".to_string(),
            BTreeSet::from(["G1".to_string(), "G2".to_string()]),
        );
        assert!(manager.heartbeat_task_is_running());
        let heartbeat = fs::read_to_string(&manager.t3_paths().heartbeat_file).expect("heartbeat");
        assert!(heartbeat.trim().parse::<i64>().is_ok());
        assert_eq!(
            manager.pane_session_union(),
            BTreeSet::from(["G1".to_string(), "G2".to_string()])
        );

        manager.update_panes("macos".to_string(), BTreeSet::from(["G3".to_string()]));
        assert_eq!(
            manager.pane_session_union(),
            BTreeSet::from(["G1".to_string(), "G2".to_string(), "G3".to_string()])
        );

        manager.update_panes("gpui".to_string(), BTreeSet::new());
        assert!(manager.heartbeat_task_is_running());
        assert_eq!(
            manager.pane_session_union(),
            BTreeSet::from(["G3".to_string()])
        );

        manager.update_panes("macos".to_string(), BTreeSet::new());
        assert!(!manager.heartbeat_task_is_running());
        assert!(manager.pane_session_union().is_empty());
        assert!(manager.t3_paths().heartbeat_file.exists());
    }

    #[test]
    fn stop_when_not_running_is_a_clean_no_op_status() {
        let (_temp, manager) = temp_manager();

        let status = manager.stop_runtime();

        assert!(!status.running);
        assert!(status.pid.is_none());
        assert_eq!(status.port, T3_RUNTIME_PORT);
        assert!(status.started_at.is_none());
        assert!(status.ownership.is_none());
    }

    #[test]
    fn t3_listener_classification_matches_macos_rules() {
        assert!(is_any_t3_runtime_command(
            "/usr/local/bin/node /apps/t3code-server/dist/bin.mjs --mode desktop --host 0.0.0.0 --port 3774 --no-browser"
        ));
        assert!(!is_any_t3_runtime_command(
            "/usr/local/bin/node server.js --port 3774"
        ));
        assert!(is_t3_runtime_supervisor_command(
            "node /Users/person/.vscode/extensions/maddada.vsmux-1.0/dist/t3-runtime-supervisor.js"
        ));
        assert!(!is_t3_runtime_supervisor_command(
            "node t3-runtime-supervisor.js"
        ));

        let (_temp, manager) = temp_manager();
        let owned = T3ListeningProcess {
            command: format!(
                "bun run --t3 --mode desktop --port 3774 {}",
                path_text(&manager.t3_paths().t3_home_dir)
            ),
            parent_command: "/bin/zsh".to_string(),
            parent_pid: Some(10),
            pid: 11,
        };
        assert!(manager.is_owned_t3_runtime_process(&owned));

        let foreign = T3ListeningProcess {
            command: "node /Users/other/t3 --mode desktop --port 3774".to_string(),
            parent_command: "/bin/zsh".to_string(),
            parent_pid: Some(10),
            pid: 12,
        };
        assert!(!manager.is_owned_t3_runtime_process(&foreign));
    }

    #[test]
    fn running_status_reports_external_listener_without_auth_material() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let manager = T3RuntimeManager::with_probe(
            &paths,
            Arc::new(StaticT3RuntimePortProbe {
                listeners: vec![T3ListeningProcess {
                    command: "node /Users/other/t3 --mode desktop --port 3774".to_string(),
                    parent_command: "/bin/zsh".to_string(),
                    parent_pid: Some(10),
                    pid: 4242,
                }],
            }),
        );

        let status = manager.status_snapshot();

        assert!(status.running);
        assert_eq!(status.pid, Some(4242));
        assert_eq!(status.ownership.as_deref(), Some("external"));
        assert!(status.started_at.is_none());
    }

    #[test]
    fn launch_script_matches_the_macos_wrapper_shape() {
        let script = build_launch_script(
            Path::new("/tmp/t3/bootstrap-x.json"),
            Path::new("/tmp/t3/ghostex-app-heartbeat"),
            "'/usr/local/bin/node' '/apps/t3code-server/dist/bin.mjs'",
        );

        assert!(script.starts_with("exec 3< '/tmp/t3/bootstrap-x.json'\nrm -f '/tmp/t3/bootstrap-x.json'\n("));
        assert!(script.contains(
            "'/usr/local/bin/node' '/apps/t3code-server/dist/bin.mjs' --mode desktop --host 127.0.0.1 --port 3774 --no-browser --bootstrap-fd 3 &"
        ));
        assert!(script.contains("last=$(cat '/tmp/t3/ghostex-app-heartbeat' 2>/dev/null || echo \"$now\")"));
        assert!(script.contains("if [ $((now - last)) -ge 180 ]; then"));
        assert!(script.contains("kill -9 \"$runtime_pid\" 2>/dev/null"));
        assert!(script.contains("wait \"$runtime_pid\""));
    }

    #[test]
    fn discard_desktop_bootstrap_credential_keeps_the_owner_bearer() {
        let (_temp, manager) = temp_manager();
        manager
            .write_auth_state(&serde_json::json!({
                "desktopBootstrapToken": "one-time",
                "ownerBearerToken": "bearer-1",
                "provider": "t3code",
            }))
            .expect("auth state");

        manager.discard_desktop_bootstrap_credential("one-time");

        let state = manager.read_auth_state().expect("auth state");
        assert_eq!(state.get("provider"), Some(&Value::String("t3code".into())));
        assert_eq!(
            state.get("ownerBearerToken"),
            Some(&Value::String("bearer-1".into()))
        );
        assert!(state.get("desktopBootstrapToken").is_none());
        assert!(manager.read_persisted_desktop_bootstrap_token().is_none());
    }

    #[test]
    fn ps_elapsed_time_parses_macos_etime_formats() {
        assert_eq!(parse_ps_elapsed_seconds("00:42"), Some(42));
        assert_eq!(parse_ps_elapsed_seconds("01:02:03"), Some(3_723));
        assert_eq!(parse_ps_elapsed_seconds("2-01:02:03"), Some(176_523));
        assert_eq!(parse_ps_elapsed_seconds(""), None);
        assert_eq!(parse_ps_elapsed_seconds("garbage"), None);
    }
}
