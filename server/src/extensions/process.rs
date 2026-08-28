use std::{
    collections::HashMap,
    io,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use axum::http::HeaderMap;

use crate::platform::shell::{command_shell, shell_quote};

use super::{
    new_extension_token, request_bearer_token, tokens_equal, ExtensionBadge, ExtensionError,
    ExtensionLaunchContext, ExtensionManifest, ExtensionResult, ExtensionRuntimeState,
    ExtensionRuntimeStatus,
};

const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const READINESS_REQUEST_TIMEOUT: Duration = Duration::from_millis(750);
const STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(crate) struct ExtensionRuntime {
    inner: Arc<Mutex<ExtensionRuntimeInner>>,
    api_url: String,
}

struct ExtensionRuntimeInner {
    statuses: HashMap<String, ExtensionRuntimeStatus>,
    badges: HashMap<String, ExtensionBadge>,
    processes: HashMap<String, ProcessControl>,
    tokens: HashMap<String, ExtensionToken>,
}

struct ProcessControl {
    generation: String,
    stop_tx: mpsc::Sender<mpsc::SyncSender<()>>,
}

struct ExtensionToken {
    extension_id: String,
    token: String,
}

struct LaunchSpec {
    command: String,
    cwd: PathBuf,
    extension_id: String,
    readiness_url: String,
    timeout: Duration,
    token: String,
    context: ExtensionLaunchContext,
}

impl ExtensionRuntime {
    pub(crate) fn new(api_url: String) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ExtensionRuntimeInner {
                statuses: HashMap::new(),
                badges: HashMap::new(),
                processes: HashMap::new(),
                tokens: HashMap::new(),
            })),
            api_url,
        }
    }

    pub(crate) fn status(&self, id: &str, manifest: &ExtensionManifest) -> ExtensionRuntimeStatus {
        if let Some(url) = spawnless_server_url(id, manifest, &self.api_url) {
            return ExtensionRuntimeStatus {
                state: ExtensionRuntimeState::Ready,
                url: Some(url),
                pid: None,
                error: None,
            };
        }
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.statuses.get(id).cloned())
            .unwrap_or_else(ExtensionRuntimeStatus::stopped)
    }

    pub(crate) fn badge(&self, id: &str) -> Option<ExtensionBadge> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.badges.get(id).cloned())
    }

    pub(crate) fn start(
        &self,
        payload_dir: &Path,
        manifest: &ExtensionManifest,
        context: ExtensionLaunchContext,
    ) -> ExtensionResult<ExtensionRuntimeStatus> {
        let id = manifest.name.as_str();
        let server = manifest.server.as_ref().ok_or_else(|| {
            ExtensionError::bad_request(format!("Extension {id:?} has no web server."))
        })?;
        if server.static_dir.is_some() || server.url.is_some() {
            return Ok(self.status(id, manifest));
        }
        let command_template = server.command.as_ref().ok_or_else(|| {
            ExtensionError::bad_request(format!("Extension {id:?} has no command server."))
        })?;
        let readiness = server.readiness.as_ref().ok_or_else(|| {
            ExtensionError::bad_request(format!("Extension {id:?} has no readiness probe."))
        })?;

        if self.is_running(id)? {
            return self.wait_for_start(
                id,
                Duration::from_secs(readiness.timeout_seconds.unwrap_or(60)),
            );
        }

        let port = allocate_loopback_port()?;
        let base_url = format!("http://127.0.0.1:{port}");
        let command = substitute_command(command_template, port, &context);
        let cwd = resolve_cwd(payload_dir, server.cwd.as_deref(), port, &context)?;
        let readiness_url = url::Url::parse(&base_url)
            .and_then(|base| base.join(&readiness.http_get))
            .map_err(|error| {
                ExtensionError::bad_request(format!(
                    "Invalid readiness URL for extension {id}: {error}"
                ))
            })?
            .to_string();
        let timeout = Duration::from_secs(readiness.timeout_seconds.unwrap_or(60));
        let token = new_extension_token();
        let generation = uuid::Uuid::new_v4().to_string();
        let (stop_tx, stop_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let spec = LaunchSpec {
            command,
            cwd,
            extension_id: id.to_string(),
            readiness_url,
            timeout,
            token: token.clone(),
            context,
        };

        {
            let mut inner = self.lock()?;
            if inner.processes.contains_key(id) {
                drop(inner);
                return self.wait_for_start(id, timeout);
            }
            inner.statuses.insert(
                id.to_string(),
                ExtensionRuntimeStatus {
                    state: ExtensionRuntimeState::Starting,
                    url: Some(base_url.clone()),
                    pid: None,
                    error: None,
                },
            );
            inner.processes.insert(
                id.to_string(),
                ProcessControl {
                    generation: generation.clone(),
                    stop_tx,
                },
            );
            inner.tokens.insert(
                id.to_string(),
                ExtensionToken {
                    extension_id: id.to_string(),
                    token,
                },
            );
        }

        let runtime = self.clone();
        thread::spawn(move || {
            monitor_process(runtime, generation, spec, stop_rx, started_tx);
        });

        match started_rx.recv_timeout(timeout + Duration::from_secs(2)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => self.status_for_id(id),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ExtensionError::internal(format!(
                "Timed out waiting for extension {id:?} runtime state."
            ))),
        }
    }

    pub(crate) fn stop(&self, id: &str) -> ExtensionResult<ExtensionRuntimeStatus> {
        let stop_tx = {
            let inner = self.lock()?;
            inner
                .processes
                .get(id)
                .map(|process| process.stop_tx.clone())
        };
        let Some(stop_tx) = stop_tx else {
            self.clear_runtime(id, ExtensionRuntimeStatus::stopped(), None);
            return Ok(ExtensionRuntimeStatus::stopped());
        };
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        stop_tx.send(reply_tx).map_err(|_| {
            ExtensionError::internal(format!(
                "Extension {id:?} process monitor stopped unexpectedly."
            ))
        })?;
        reply_rx.recv_timeout(STOP_WAIT_TIMEOUT).map_err(|_| {
            ExtensionError::internal(format!("Timed out stopping extension {id:?}."))
        })?;
        self.status_for_id(id)
    }

    pub(crate) fn stop_all(&self) {
        let ids = self
            .inner
            .lock()
            .map(|inner| inner.processes.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for id in ids {
            let _ = self.stop(&id);
        }
    }

    pub(crate) fn set_badge(
        &self,
        id: &str,
        lines: Vec<String>,
    ) -> ExtensionResult<ExtensionBadge> {
        if lines.len() > 3 || lines.iter().any(|line| line.chars().count() > 32) {
            return Err(ExtensionError::bad_request(
                "Extension badge accepts at most three lines of 32 characters each.",
            ));
        }
        let badge = ExtensionBadge { lines };
        let mut inner = self.lock()?;
        if badge.lines.is_empty() {
            inner.badges.remove(id);
        } else {
            inner.badges.insert(id.to_string(), badge.clone());
        }
        Ok(badge)
    }

    pub(crate) fn authorize(&self, headers: &HeaderMap, endpoint: &str) -> Option<String> {
        if !matches!(
            endpoint,
            "/api/listExtensions" | "/api/extensionStatus" | "/api/extensionBadge"
        ) {
            return None;
        }
        let provided = request_bearer_token(headers)?;
        let inner = self.inner.lock().ok()?;
        inner
            .tokens
            .values()
            .find(|entry| tokens_equal(provided, &entry.token))
            .map(|entry| entry.extension_id.clone())
    }

    fn is_running(&self, id: &str) -> ExtensionResult<bool> {
        Ok(self.lock()?.processes.contains_key(id))
    }

    fn wait_for_start(
        &self,
        id: &str,
        timeout: Duration,
    ) -> ExtensionResult<ExtensionRuntimeStatus> {
        let deadline = Instant::now() + timeout + Duration::from_secs(1);
        loop {
            let status = self.status_for_id(id)?;
            if status.state != ExtensionRuntimeState::Starting || Instant::now() >= deadline {
                return Ok(status);
            }
            thread::sleep(READINESS_POLL_INTERVAL);
        }
    }

    fn status_for_id(&self, id: &str) -> ExtensionResult<ExtensionRuntimeStatus> {
        Ok(self
            .lock()?
            .statuses
            .get(id)
            .cloned()
            .unwrap_or_else(ExtensionRuntimeStatus::stopped))
    }

    fn update_status(&self, id: &str, status: ExtensionRuntimeStatus) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.statuses.insert(id.to_string(), status);
        }
    }

    fn clear_runtime(&self, id: &str, status: ExtensionRuntimeStatus, generation: Option<&str>) {
        if let Ok(mut inner) = self.inner.lock() {
            if generation.is_some_and(|expected| {
                inner
                    .processes
                    .get(id)
                    .is_some_and(|process| process.generation != expected)
            }) {
                return;
            }
            inner.processes.remove(id);
            inner.tokens.remove(id);
            inner.badges.remove(id);
            inner.statuses.insert(id.to_string(), status);
        }
    }

    fn lock(&self) -> ExtensionResult<std::sync::MutexGuard<'_, ExtensionRuntimeInner>> {
        self.inner
            .lock()
            .map_err(|_| ExtensionError::internal("Extension runtime lock was poisoned."))
    }
}

fn monitor_process(
    runtime: ExtensionRuntime,
    generation: String,
    spec: LaunchSpec,
    stop_rx: mpsc::Receiver<mpsc::SyncSender<()>>,
    started_tx: mpsc::SyncSender<()>,
) {
    let id = spec.extension_id.clone();
    let base_url = spec
        .readiness_url
        .parse::<url::Url>()
        .ok()
        .map(|url| format!("{}://{}", url.scheme(), url.authority()))
        .unwrap_or_default();
    let mut child = match spawn_child(&runtime.api_url, &spec) {
        Ok(child) => child,
        Err(error) => {
            runtime.clear_runtime(
                &id,
                failed_status(
                    base_url,
                    None,
                    format!("Could not start extension process: {error}"),
                ),
                Some(&generation),
            );
            let _ = started_tx.send(());
            return;
        }
    };
    let pid = child.id();
    runtime.update_status(
        &id,
        ExtensionRuntimeStatus {
            state: ExtensionRuntimeState::Starting,
            url: Some(base_url.clone()),
            pid: Some(pid),
            error: None,
        },
    );
    let deadline = Instant::now() + spec.timeout;
    loop {
        if let Ok(reply) = stop_rx.try_recv() {
            terminate_process_group(&mut child);
            runtime.clear_runtime(&id, ExtensionRuntimeStatus::stopped(), Some(&generation));
            let _ = reply.send(());
            let _ = started_tx.send(());
            return;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                runtime.clear_runtime(
                    &id,
                    failed_status(
                        base_url,
                        Some(pid),
                        format!("Extension process exited before readiness with {status}."),
                    ),
                    Some(&generation),
                );
                let _ = started_tx.send(());
                return;
            }
            Err(error) => {
                terminate_process_group(&mut child);
                runtime.clear_runtime(
                    &id,
                    failed_status(
                        base_url,
                        Some(pid),
                        format!("Could not inspect extension process: {error}"),
                    ),
                    Some(&generation),
                );
                let _ = started_tx.send(());
                return;
            }
            Ok(None) => {}
        }
        if readiness_is_ready(&spec.readiness_url) {
            break;
        }
        if Instant::now() >= deadline {
            terminate_process_group(&mut child);
            runtime.clear_runtime(
                &id,
                failed_status(
                    base_url,
                    Some(pid),
                    format!(
                        "Readiness probe timed out after {} seconds.",
                        spec.timeout.as_secs()
                    ),
                ),
                Some(&generation),
            );
            let _ = started_tx.send(());
            return;
        }
        thread::sleep(READINESS_POLL_INTERVAL);
    }

    runtime.update_status(
        &id,
        ExtensionRuntimeStatus {
            state: ExtensionRuntimeState::Ready,
            url: Some(base_url.clone()),
            pid: Some(pid),
            error: None,
        },
    );
    let _ = started_tx.send(());
    loop {
        match stop_rx.recv_timeout(READINESS_POLL_INTERVAL) {
            Ok(reply) => {
                terminate_process_group(&mut child);
                runtime.clear_runtime(&id, ExtensionRuntimeStatus::stopped(), Some(&generation));
                let _ = reply.send(());
                return;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                terminate_process_group(&mut child);
                runtime.clear_runtime(&id, ExtensionRuntimeStatus::stopped(), Some(&generation));
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                runtime.clear_runtime(
                    &id,
                    failed_status(
                        base_url,
                        Some(pid),
                        format!("Extension process exited with {status}."),
                    ),
                    Some(&generation),
                );
                return;
            }
            Err(error) => {
                terminate_process_group(&mut child);
                runtime.clear_runtime(
                    &id,
                    failed_status(
                        base_url,
                        Some(pid),
                        format!("Could not inspect extension process: {error}"),
                    ),
                    Some(&generation),
                );
                return;
            }
            Ok(None) => {}
        }
    }
}

/*
Static and URL servers are the two web-extension shapes gxserver never spawns a
process for: a static bundle is already served from this process at
`/ext/{id}/`, and a URL server is a fixed remote page the manifest points at.
Both are ready the moment the extension is installed and enabled, so start is a
status read and stop has no process to terminate.
*/
fn spawnless_server_url(id: &str, manifest: &ExtensionManifest, api_url: &str) -> Option<String> {
    let server = manifest.server.as_ref()?;
    if let Some(url) = server.url.as_ref() {
        // Manifest validation already parsed this; hand callers the normalized
        // form so the scheme and host casing match what a browser reports back
        // for the loaded page.
        return url::Url::parse(url).ok().map(|parsed| parsed.to_string());
    }
    server
        .static_dir
        .as_ref()
        .map(|_| format!("{api_url}/ext/{id}/"))
}

fn spawn_child(api_url: &str, spec: &LaunchSpec) -> io::Result<Child> {
    let shell = command_shell();
    let mut command = Command::new(&shell.executable);
    command
        .args(shell.script_args(&spec.command))
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("GHOSTEX_EXTENSION_ID", &spec.extension_id)
        .env("GHOSTEX_SESSION_ID", &spec.context.session_id)
        .env("GHOSTEX_PROJECT_PATH", &spec.context.project_path)
        .env("GHOSTEX_PROJECT_NAME", &spec.context.project_name)
        .env(
            "GHOSTEX_WORKTREE",
            if spec.context.worktree { "1" } else { "0" },
        )
        .env("GHOSTEX_WORKTREE_BRANCH", &spec.context.worktree_branch)
        .env("GHOSTEX_API_URL", api_url)
        .env("GHOSTEX_API_TOKEN", &spec.token);
    configure_process_group(&mut command);
    command.spawn()
}

fn allocate_loopback_port() -> ExtensionResult<u16> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| {
            ExtensionError::internal(format!("Could not allocate extension port: {error}"))
        })
}

fn substitute_command(template: &str, port: u16, context: &ExtensionLaunchContext) -> String {
    template
        .replace("{port}", &port.to_string())
        .replace("{projectPath}", &shell_quote(&context.project_path))
        .replace("{projectName}", &shell_quote(&context.project_name))
}

fn resolve_cwd(
    payload_dir: &Path,
    template: Option<&str>,
    port: u16,
    context: &ExtensionLaunchContext,
) -> ExtensionResult<PathBuf> {
    let Some(template) = template else {
        return Ok(payload_dir.to_path_buf());
    };
    let value = template
        .replace("{port}", &port.to_string())
        .replace("{projectPath}", &context.project_path)
        .replace("{projectName}", &context.project_name);
    let path = PathBuf::from(value);
    let resolved = if path.is_absolute() {
        path
    } else {
        payload_dir.join(path)
    };
    if !resolved.is_dir() {
        return Err(ExtensionError::bad_request(format!(
            "Extension working directory does not exist: {}",
            resolved.display()
        )));
    }
    Ok(resolved)
}

fn readiness_is_ready(url: &str) -> bool {
    ureq::get(url)
        .timeout(READINESS_REQUEST_TIMEOUT)
        .call()
        .is_ok_and(|response| response.status() == 200)
}

fn failed_status(url: String, pid: Option<u32>, error: String) -> ExtensionRuntimeStatus {
    ExtensionRuntimeStatus {
        state: ExtensionRuntimeState::Failed,
        url: (!url.is_empty()).then_some(url),
        pid,
        error: Some(error),
    }
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0200);
    }
}

fn terminate_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let process_group_id = child.id() as libc::pid_t;
        if process_group_id > 0 {
            unsafe {
                libc::kill(-process_group_id, libc::SIGKILL);
            }
        }
    }

    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = child.kill();
    let _ = child.wait();
}
