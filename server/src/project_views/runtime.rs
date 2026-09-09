use super::resolve::{resolve, text, Plan};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    net::TcpListener,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) struct Instance {
    pub state: Mutex<Value>,
    pub report: Mutex<Option<PathBuf>>,
    pub token: String,
    fingerprint: String,
    stop: AtomicBool,
    done: AtomicBool,
    owners: Mutex<HashMap<String, Instant>>,
}
fn instances() -> &'static Mutex<HashMap<String, Arc<Instance>>> {
    static INSTANCES: OnceLock<Mutex<HashMap<String, Arc<Instance>>>> = OnceLock::new();
    INSTANCES.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(super) fn report_instance(token: &str) -> Option<Arc<Instance>> {
    instances()
        .lock()
        .ok()?
        .values()
        .find(|i| i.token == token)
        .cloned()
}
pub(crate) fn stop_all() {
    let live = instances()
        .lock()
        .map(|registry| registry.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for instance in &live {
        instance.stop.store(true, Ordering::SeqCst);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while live
        .iter()
        .any(|instance| !instance.done.load(Ordering::SeqCst))
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(50));
    }
}

/// CDXC:Extensions 2026-09-09 SEE-ALSO:
/// Project views use a checkout-and-view runtime with leased desktop owners, unlike installed extensions' one-process-per-extension contract.
/// The desktop renews ownership in apps/desktop/src/app/project_views.rs; closing its last owner stops only this runtime's process group.
pub(super) fn operation(params: Value, api_port: u16) -> Result<Value> {
    let root = std::fs::canonicalize(text(&params, "projectPath"))
        .context("The project directory is unavailable.")?;
    let id = text(&params["view"], "id");
    if !id.starts_with("custom-view-") || id.len() > 160 {
        bail!("Invalid view ID.");
    }
    let owner = text(&params, "owner");
    if owner.is_empty() || owner.len() > 200 {
        bail!("A view owner is required.");
    }
    let key = format!("{}\n{id}", root.display());
    let operation = text(&params, "operation");
    if !["describe", "start", "status", "stop", "restart", "release"].contains(&operation) {
        bail!("Unknown project view operation.");
    }
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(format!(
            "{}{}",
            params["view"],
            json!([params["binding"], params["repositoryOriginUrl"]])
        ))
    );
    let mut registry = instances()
        .lock()
        .map_err(|_| anyhow::anyhow!("View runtime unavailable."))?;
    if let Some(existing) = registry.get(&key).cloned() {
        if operation == "release" {
            existing.owners.lock().unwrap().remove(owner);
            return Ok(existing.state.lock().unwrap().clone());
        }
        if operation == "stop" {
            existing.stop.store(true, Ordering::SeqCst);
            existing.state.lock().unwrap()["state"] = json!("stopped");
            return Ok(existing.state.lock().unwrap().clone());
        }
        existing
            .owners
            .lock()
            .unwrap()
            .insert(owner.to_string(), Instant::now());
        if existing.fingerprint == fingerprint && operation != "restart" {
            let state = existing.state.lock().unwrap().clone();
            if !["stopped", "unconfigured"].contains(&text(&state, "state")) || operation != "start"
            {
                return Ok(state);
            }
        }
        existing.stop.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(15);
        while !existing.done.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if !existing.done.load(Ordering::SeqCst) {
            bail!("The previous command is still stopping. Retry shortly.");
        }
        registry.remove(&key);
    }
    if operation == "release" || operation == "stop" {
        return Ok(json!({"state":"stopped","available":true}));
    }
    let plan = match resolve(&params) {
        Ok(plan) => plan,
        Err(error) => {
            return Ok(
                json!({"state":"unconfigured","available":error.downcast_ref::<super::resolve::SetupRequired>().is_some(),"error":format!("{error:#}")}),
            )
        }
    };
    if text(&params["view"]["source"], "kind") == "website"
        || params["view"].get("source").is_none()
    {
        return Ok(json!({"state":"ready","available":true,"url":plan.url}));
    }
    if operation == "describe" || operation == "status" {
        return Ok(json!({"state":"stopped","available":true,"plan":plan.value()}));
    }
    let instance = Arc::new(Instance {
        state: Mutex::new(
            json!({"state":"starting","available":true,"plan":plan.value(),"output":""}),
        ),
        report: Mutex::new(None),
        token: uuid::Uuid::new_v4().to_string(),
        fingerprint,
        stop: AtomicBool::new(false),
        done: AtomicBool::new(false),
        owners: Mutex::new(HashMap::from([(owner.to_string(), Instant::now())])),
    });
    registry.insert(key, instance.clone());
    thread::spawn(move || {
        let result = run(&instance, plan, api_port);
        if let Err(error) = result {
            let mut state = instance.state.lock().unwrap();
            state["state"] = json!("failed");
            state["error"] = json!(format!("{error:#}"));
        }
        instance.done.store(true, Ordering::SeqCst);
    });
    Ok(json!({"state":"starting","available":true}))
}
fn expired(instance: &Instance) -> bool {
    let mut owners = instance.owners.lock().unwrap();
    owners.retain(|_, last| last.elapsed() < Duration::from_secs(45));
    owners.is_empty() || instance.stop.load(Ordering::SeqCst)
}
fn stopped(instance: &Instance) {
    *instance.report.lock().unwrap() = None;
    let mut state = instance.state.lock().unwrap();
    state["state"] = json!("stopped");
    state["url"] = Value::Null;
}
fn run(instance: &Arc<Instance>, mut plan: Plan, api_port: u16) -> Result<()> {
    let mut port = None;
    if plan.report.is_none() {
        if plan.url.contains("{port}") {
            let listener = TcpListener::bind(("127.0.0.1", 0))?;
            let allocated = listener.local_addr()?.port().to_string();
            plan.command = plan.command.replace("{port}", &allocated);
            plan.url = plan.url.replace("{port}", &allocated);
        }
        let parsed = url::Url::parse(&plan.url)?;
        let expected = parsed
            .port_or_known_default()
            .context("Ready URL needs a port.")?;
        let host = parsed
            .host_str()
            .unwrap_or("127.0.0.1")
            .trim_matches(['[', ']']);
        let guard=TcpListener::bind((host,expected)).with_context(||format!("Port {expected} is already in use. Configure a different port or stop the conflicting service."))?;
        drop(guard);
        port = Some(expected);
    }
    instance.state.lock().unwrap()["plan"] = plan.value();
    let child = if plan.command.is_empty() {
        None
    } else {
        #[cfg(not(windows))]
        let mut command = {
            let shell = crate::platform::shell::command_shell();
            let mut command = Command::new(&shell.executable);
            command.args(shell.script_args(&plan.command));
            command
        };
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-Command", &plan.command]);
            command
        };
        command
            .current_dir(&plan.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("BROWSER", "none")
            .env("CI", "true");
        crate::extensions::process::configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .context("Could not start the project command.")?;
        capture(child.stdout.take(), instance.clone());
        capture(child.stderr.take(), instance.clone());
        Some(child)
    };
    let mut child = OwnedProcess(child);
    let started = Instant::now();
    let mut ready = false;
    loop {
        if expired(instance) {
            stopped(instance);
            return Ok(());
        }
        if let Some(process) = child.as_mut() {
            if let Some(status) = process.try_wait()? {
                crate::extensions::process::terminate_process_group(process);
                if plan.report.is_none() || !status.success() {
                    bail!("Command exited with {status}. Open command output for details.");
                }
                child.0 = None;
            }
        }
        if !ready {
            if let Some((directory, entry)) = &plan.report {
                if child.is_none() {
                    let directory = std::fs::canonicalize(directory)
                        .context("Report directory was not generated.")?;
                    if !directory.starts_with(&plan.root) {
                        bail!("Report directory resolves outside this checkout.");
                    }
                    let file = std::fs::canonicalize(directory.join(entry))
                        .context("Report entry page was not generated.")?;
                    if !file.starts_with(&directory) || !file.is_file() {
                        bail!("Invalid report entry page.");
                    }
                    *instance.report.lock().unwrap() = Some(directory);
                    let encoded = entry
                        .split('/')
                        .map(|s| {
                            url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("/");
                    plan.url = format!(
                        "http://127.0.0.1:{api_port}/project-view-report/{}/{encoded}",
                        instance.token
                    );
                    ready = true;
                }
            } else if let (Some(process), Some(port)) = (child.as_ref(), port) {
                let agent = ureq::AgentBuilder::new()
                    .timeout(Duration::from_millis(500))
                    .redirects(0)
                    .build();
                if agent.get(&plan.url).call().is_ok() && listener_owned(port, process.id())? {
                    ready = true;
                }
            }
            if ready {
                let mut state = instance.state.lock().unwrap();
                state["state"] = json!("ready");
                state["url"] = json!(plan.url);
            } else if started.elapsed() > Duration::from_secs(plan.timeout) {
                bail!(
                    "The command did not become ready within {} seconds.",
                    plan.timeout
                );
            }
        }
        thread::sleep(Duration::from_millis(if ready { 1000 } else { 300 }));
    }
}
fn capture(pipe: Option<impl Read + Send + 'static>, instance: Arc<Instance>) {
    let Some(mut pipe) = pipe else { return };
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(count) = pipe.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let mut state = instance.state.lock().unwrap();
            let mut output = state["output"].as_str().unwrap_or_default().to_string();
            output.push_str(&String::from_utf8_lossy(&buffer[..count]));
            if output.len() > 32768 {
                let mut start = output.len() - 32768;
                while !output.is_char_boundary(start) {
                    start += 1;
                }
                output = output[start..].to_string();
            }
            state["output"] = json!(output);
        }
    });
}
#[cfg(unix)]
fn listener_owned(port: u16, pid: u32) -> Result<bool> {
    let listeners = crate::portless::listener_discovery::read_all_tcp_listeners()?;
    for listener in listeners.into_iter().filter(|l| l.port == port) {
        if let Some(owner) = listener.pid {
            if unsafe { libc::getpgid(owner as libc::pid_t) } == pid as libc::pid_t {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

struct OwnedProcess(Option<std::process::Child>);
impl std::ops::Deref for OwnedProcess {
    type Target = Option<std::process::Child>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for OwnedProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            crate::extensions::process::terminate_process_group(child);
        }
    }
}

#[cfg(windows)]
fn listener_owned(port: u16, root_pid: u32) -> Result<bool> {
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'; $parents = @{{}}; Get-CimInstance Win32_Process | ForEach-Object {{ $parents[[uint32]$_.ProcessId] = [uint32]$_.ParentProcessId }}; $owned = $false; Get-NetTCPConnection -State Listen -LocalPort {port} -ErrorAction SilentlyContinue | ForEach-Object {{ $candidate = [uint32]$_.OwningProcess; $seen = @{{}}; while ($candidate -gt 0 -and !$seen.ContainsKey($candidate)) {{ if ($candidate -eq {root_pid}) {{ $owned = $true; break }}; $seen[$candidate] = $true; $candidate = [uint32]$parents[$candidate] }} }}; if ($owned) {{ exit 0 }} else {{ exit 1 }}"#
    );
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::extensions::process::configure_process_group(&mut command);
    let mut child = OwnedProcess(Some(command.spawn()?));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.as_mut().unwrap().try_wait()? {
            child.0 = None;
            return Ok(status.success());
        }
        if Instant::now() >= deadline {
            bail!("Timed out checking the dev server's listening process.");
        }
        thread::sleep(Duration::from_millis(50));
    }
}
