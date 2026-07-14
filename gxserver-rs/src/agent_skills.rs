use std::{
    collections::{BTreeMap, HashSet},
    env, fs, io,
    path::{Component, Path, PathBuf},
    process::{ExitStatus, Output, Stdio},
    sync::{Arc, Mutex},
};

use serde_json::{json, Map, Value};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::mpsc,
    task::JoinHandle,
    time::{sleep, Duration},
};

use crate::{domain::DomainStateError, paths::{AgentPaths, GxserverPaths}};

pub const GHOSTEX_AGENT_SKILL_NAMES: &[&str] = &[
    /*
    CDXC:CodexSessionMove 2026-06-26-13:47:
    Ghostex ships `$ghostex-move-codex-session` as an app-bundled skill, so gxserver must accept it in the same install/status allow-list as the existing first-launch skills.
    */
    "ghostex-browser-use",
    "ghostex-computer-use",
    "ghostex-agent-orchestration",
    "ghostex-fable-5.5-orchestration",
    "ghostex-generate-title",
    "ghostex-manage-beads",
    "ghostex-move-codex-session",
];

pub const GHOSTEX_SKILLS_CLI_AGENT_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "cursor",
    "gemini-cli",
    "opencode",
    "pi",
    "antigravity",
    "antigravity-cli",
    "amp",
    "kiro-cli",
    "droid",
    "github-copilot",
    "qoder",
    "codebuddy",
    "rovodev",
    "hermes-agent",
];

const AGENT_SKILL_INSTALL_ENV_OVERRIDES: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_PROFILE",
    "CODEX_HOME",
    "CODEX_PROFILE",
];
const AGENT_SKILL_DISCOVERY_MAX_DEPTH: usize = 8;
const AGENT_SKILL_INSTALL_EXITED_READER_DRAIN_MS: u64 = 1_000;
/*
CDXC:AgentSkills 2026-06-22-07:21:
The TypeScript daemon used execFile maxBuffer=10 MiB for agent-skill installs. Keep gxserver-rs at the same stdout/stderr byte ceiling while retaining the Rust runner's explicit process termination and captured-output reporting.
*/
const AGENT_SKILL_INSTALL_OUTPUT_LIMIT_BYTES: usize = 10 * 1024 * 1024;
const AGENT_SKILL_INSTALL_POLL_MS: u64 = 25;
const AGENT_SKILL_INSTALL_TERMINATED_READER_DRAIN_MS: u64 = 250;
const AGENT_SKILL_INSTALL_TIMEOUT_MS: u64 = 5 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillDiscoveryRoot {
    pub path: PathBuf,
    pub providers: Vec<String>,
    pub source_kind: &'static str,
}

#[derive(Clone, Debug)]
struct AgentSkillLocation {
    directory_path: PathBuf,
    providers: Vec<String>,
    root_path: PathBuf,
    skill_file_path: PathBuf,
    skill_name: String,
    source_kind: &'static str,
}

/*
CDXC:AgentSkills 2026-06-19-13:59:
Rust owns the same Ghostex skill setup surface as TypeScript: local authenticated RPCs and CLI commands discover Claude/Codex/shared skill folders, delegate installs to the external `skills` CLI, and never persist stdout, stderr, command text, package paths, or environment values in gxserver logs.
*/
pub fn read_agent_skill_status(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let skill_names = normalize_agent_skill_names(read_string_array(params, "skillNames")?)?;
    read_agent_skill_status_for_names(paths, params, skill_names)
}

pub async fn install_agent_skills(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let package_source =
        normalize_package_source(params.get("packageSource").and_then(Value::as_str))?;
    let skill_names = normalize_agent_skill_names(read_string_array(params, "skillNames")?)?;
    if skill_names.is_empty() {
        return Err(DomainStateError::bad_request(
            "Agent skill installs require at least one Ghostex agent skill.",
        ));
    }
    let install_command = build_gxserver_agent_skill_install_command(
        &package_source,
        &skill_names,
        read_string_array(params, "agentIds")?,
    );
    let output = run_agent_skill_install_command(paths, &package_source, &install_command).await?;
    let mut status = read_agent_skill_status_for_names(paths, params, skill_names)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("installCommand".to_string(), json!(install_command));
    status.insert("packageSource".to_string(), json!(package_source));
    status.insert(
        "stderr".to_string(),
        json!(String::from_utf8_lossy(&output.stderr).to_string()),
    );
    status.insert(
        "stdout".to_string(),
        json!(String::from_utf8_lossy(&output.stdout).to_string()),
    );
    Ok(Value::Object(status))
}

pub fn build_gxserver_agent_skill_install_command(
    package_source: &str,
    skill_names: &[String],
    agent_ids: Option<Vec<String>>,
) -> Vec<String> {
    let agent_ids = normalize_agent_skill_agent_ids(agent_ids);
    let mut command = vec![
        "npx".to_string(),
        "--yes".to_string(),
        "skills".to_string(),
        "add".to_string(),
        package_source.to_string(),
        "--skill".to_string(),
    ];
    command.extend(skill_names.iter().cloned());
    command.push("--global".to_string());
    command.push("--agent".to_string());
    command.extend(agent_ids);
    command.push("--copy".to_string());
    command.push("-y".to_string());
    command
}

pub fn create_gxserver_agent_skill_install_environment(
    paths: &GxserverPaths,
    environment: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut install_environment = environment.clone();
    install_environment.insert("HOME".to_string(), path_string(&paths.home_dir));
    for key in AGENT_SKILL_INSTALL_ENV_OVERRIDES {
        install_environment.remove(*key);
    }
    install_environment
}

pub fn create_gxserver_agent_skill_discovery_sources(
    paths: &GxserverPaths,
    repository_paths: &[String],
) -> Vec<AgentSkillDiscoveryRoot> {
    let mut sources = vec![
        AgentSkillDiscoveryRoot {
            path: paths.home_dir.join(".codex").join("skills"),
            providers: vec!["codex".to_string()],
            source_kind: "global",
        },
        AgentSkillDiscoveryRoot {
            path: AgentPaths::new(&paths.home_dir).skills_root,
            providers: vec!["agent-skills".to_string()],
            source_kind: "global",
        },
        AgentSkillDiscoveryRoot {
            path: paths.home_dir.join(".claude").join("skills"),
            providers: vec!["claude".to_string()],
            source_kind: "global",
        },
        AgentSkillDiscoveryRoot {
            path: paths.home_dir.join(".codex").join("plugins").join("cache"),
            providers: vec!["codex".to_string(), "agent-skills".to_string()],
            source_kind: "pluginCache",
        },
    ];
    for repository_path in repository_paths {
        let normalized_path = normalize_existing_absolute_path(repository_path);
        if normalized_path.as_os_str().is_empty() {
            continue;
        }
        sources.push(AgentSkillDiscoveryRoot {
            path: normalized_path.join(".agents").join("skills"),
            providers: vec!["agent-skills".to_string()],
            source_kind: "repository",
        });
        sources.push(AgentSkillDiscoveryRoot {
            path: normalized_path.join(".claude").join("skills"),
            providers: vec!["claude".to_string()],
            source_kind: "repository",
        });
    }
    unique_skill_discovery_roots(sources)
}

pub fn normalize_agent_skill_names(
    skill_names: Option<Vec<String>>,
) -> Result<Vec<String>, DomainStateError> {
    let source = skill_names.unwrap_or_else(|| {
        GHOSTEX_AGENT_SKILL_NAMES
            .iter()
            .map(|skill_name| (*skill_name).to_string())
            .collect()
    });
    let mut normalized = Vec::new();
    let valid: HashSet<&str> = GHOSTEX_AGENT_SKILL_NAMES.iter().copied().collect();
    let mut invalid = Vec::new();
    for skill_name in source {
        let trimmed = skill_name.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !valid.contains(trimmed) {
            invalid.push(trimmed.to_string());
            continue;
        }
        if !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }
    if invalid.is_empty() {
        Ok(normalized)
    } else {
        Err(DomainStateError::bad_request(format!(
            "Unknown Ghostex agent skill: {}",
            invalid.join(", ")
        )))
    }
}

pub fn normalize_agent_skill_agent_ids(agent_ids: Option<Vec<String>>) -> Vec<String> {
    let source = match agent_ids {
        Some(values) if !values.is_empty() => values,
        _ => GHOSTEX_SKILLS_CLI_AGENT_IDS
            .iter()
            .map(|agent_id| (*agent_id).to_string())
            .collect(),
    };
    let mut normalized = Vec::new();
    for agent_id in source {
        for part in agent_id.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !normalized.iter().any(|existing| existing == trimmed) {
                normalized.push(trimmed.to_string());
            }
        }
    }
    normalized
}

fn read_agent_skill_status_for_names(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
    skill_names: Vec<String>,
) -> Result<Value, DomainStateError> {
    let repository_paths = read_string_array(params, "repositoryPaths")?.unwrap_or_default();
    let roots = create_gxserver_agent_skill_discovery_sources(paths, &repository_paths);
    let mut locations_by_skill: BTreeMap<String, Vec<Value>> = skill_names
        .iter()
        .map(|skill_name| (skill_name.clone(), Vec::new()))
        .collect();
    for root in &roots {
        for location in discover_skill_locations(root, &skill_names)? {
            if let Some(locations) = locations_by_skill.get_mut(&location.skill_name) {
                locations.push(location_to_value(&location));
            }
        }
    }
    let skills = skill_names
        .iter()
        .map(|skill_name| {
            let locations = locations_by_skill.remove(skill_name).unwrap_or_default();
            json!({
                "installed": !locations.is_empty(),
                "locations": locations,
                "skillName": skill_name,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "generatedAt": now_iso(),
        "homeDir": path_string(&paths.home_dir),
        "roots": roots.iter().map(root_to_value).collect::<Vec<_>>(),
        "skills": skills,
        "type": "agentSkillStatus",
    }))
}

async fn run_agent_skill_install_command(
    paths: &GxserverPaths,
    package_source: &str,
    install_command: &[String],
) -> Result<Output, DomainStateError> {
    run_agent_skill_install_command_with_limits(
        paths,
        package_source,
        install_command,
        Duration::from_millis(AGENT_SKILL_INSTALL_TIMEOUT_MS),
        AGENT_SKILL_INSTALL_OUTPUT_LIMIT_BYTES,
    )
    .await
}

/*
CDXC:AgentSkills 2026-06-19-18:45:
Agent skill installs run third-party CLI code through a local RPC. Reject a normalized-empty skill list before command construction, and collect bounded stdout/stderr with a timeout so a stalled or noisy installer cannot hang gxserver or allocate unbounded memory. Return captured output to the caller only; persistent logs must not receive command text or raw output.
Package and repository sources accept the same leading-tilde shorthand users type elsewhere; expand `~` before absolutizing so bundled skill installs and status checks resolve against the user home instead of the process cwd.
*/
async fn run_agent_skill_install_command_with_limits(
    paths: &GxserverPaths,
    package_source: &str,
    install_command: &[String],
    timeout_duration: Duration,
    output_limit_bytes: usize,
) -> Result<Output, DomainStateError> {
    let Some((executable, args)) = install_command.split_first() else {
        return Err(DomainStateError::bad_request(
            "Agent skill install command is empty.",
        ));
    };
    let mut command = Command::new(executable);
    command.args(args);
    if !is_uri_like(package_source) {
        if let Some(parent) = Path::new(package_source).parent() {
            command.current_dir(parent);
        }
    }
    let current_environment: BTreeMap<String, String> = env::vars().collect();
    let install_environment =
        create_gxserver_agent_skill_install_environment(paths, &current_environment);
    command.env_clear();
    command.envs(install_environment);
    configure_agent_skill_install_command_process_group(&mut command);
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Agent skill install command could not start: {error}"),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| DomainStateError {
        code: "internalError",
        message: "Agent skill install command stdout pipe was unavailable.".to_string(),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| DomainStateError {
        code: "internalError",
        message: "Agent skill install command stderr pipe was unavailable.".to_string(),
    })?;
    let (limit_sender, mut limit_receiver) = mpsc::unbounded_channel();
    let stdout_output = SharedCappedChildOutput::new();
    let stderr_output = SharedCappedChildOutput::new();
    let stdout_reader = tokio::spawn(read_capped_child_output(
        stdout,
        "stdout",
        output_limit_bytes,
        limit_sender.clone(),
        stdout_output.clone(),
    ));
    let stderr_reader = tokio::spawn(read_capped_child_output(
        stderr,
        "stderr",
        output_limit_bytes,
        limit_sender,
        stderr_output.clone(),
    ));
    let mut timeout_sleep = Box::pin(sleep(timeout_duration));
    let mut termination = None;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Agent skill install command wait failed: {error}"),
        })? {
            break status;
        }
        tokio::select! {
            _ = &mut timeout_sleep => {
                termination = Some(AgentSkillInstallTermination::Timeout);
                break terminate_agent_skill_install_child(&mut child).await?;
            }
            stream_name = limit_receiver.recv() => {
                if let Some(stream_name) = stream_name {
                    termination = Some(AgentSkillInstallTermination::OutputLimit { stream_name });
                    break terminate_agent_skill_install_child(&mut child).await?;
                }
            }
            _ = sleep(Duration::from_millis(AGENT_SKILL_INSTALL_POLL_MS)) => {}
        }
    };
    let reader_drain_timeout = Some(Duration::from_millis(if termination.is_some() {
        AGENT_SKILL_INSTALL_TERMINATED_READER_DRAIN_MS
    } else {
        AGENT_SKILL_INSTALL_EXITED_READER_DRAIN_MS
    }));
    finish_capped_child_output_reader(stdout_reader, "stdout", reader_drain_timeout).await?;
    finish_capped_child_output_reader(stderr_reader, "stderr", reader_drain_timeout).await?;
    let stdout = stdout_output.snapshot();
    let stderr = stderr_output.snapshot();
    let output_limit_termination = termination.or({
        if stdout.limit_exceeded {
            Some(AgentSkillInstallTermination::OutputLimit {
                stream_name: "stdout",
            })
        } else if stderr.limit_exceeded {
            Some(AgentSkillInstallTermination::OutputLimit {
                stream_name: "stderr",
            })
        } else {
            None
        }
    });
    let output = Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    };
    if let Some(termination) = output_limit_termination {
        return Err(agent_skill_install_termination_error(
            termination,
            timeout_duration,
            output_limit_bytes,
            &output,
        ));
    }
    if output.status.success() {
        Ok(output)
    } else {
        Err(DomainStateError {
            code: "internalError",
            message: append_agent_skill_install_output(
                format!(
                    "Agent skill install command failed with exit code {}.",
                    output
                        .status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                &output,
            ),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentSkillInstallTermination {
    OutputLimit { stream_name: &'static str },
    Timeout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CappedChildOutput {
    bytes: Vec<u8>,
    limit_exceeded: bool,
}

#[derive(Clone, Debug)]
struct SharedCappedChildOutput {
    state: Arc<Mutex<CappedChildOutput>>,
}

impl SharedCappedChildOutput {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CappedChildOutput {
                bytes: Vec::new(),
                limit_exceeded: false,
            })),
        }
    }

    fn append(&self, bytes: &[u8], limit_bytes: usize) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let remaining = limit_bytes.saturating_sub(state.bytes.len());
        if remaining > 0 {
            state
                .bytes
                .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        }
        if bytes.len() > remaining {
            state.limit_exceeded = true;
        }
        state.limit_exceeded
    }

    fn snapshot(&self) -> CappedChildOutput {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

async fn read_capped_child_output<R>(
    mut stream: R,
    stream_name: &'static str,
    limit_bytes: usize,
    limit_sender: mpsc::UnboundedSender<&'static str>,
    output: SharedCappedChildOutput,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        if output.append(&chunk[..read], limit_bytes) {
            let _ = limit_sender.send(stream_name);
            return Ok(());
        }
    }
}

async fn finish_capped_child_output_reader(
    mut reader: JoinHandle<io::Result<()>>,
    stream_name: &str,
    drain_timeout: Option<Duration>,
) -> Result<(), DomainStateError> {
    let result = if let Some(drain_timeout) = drain_timeout {
        tokio::select! {
            result = &mut reader => result,
            _ = sleep(drain_timeout) => {
                reader.abort();
                return Ok(());
            }
        }
    } else {
        reader.await
    };
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(DomainStateError {
            code: "internalError",
            message: format!("Agent skill install command {stream_name} read failed: {error}"),
        }),
        Err(error) => Err(DomainStateError {
            code: "internalError",
            message: format!("Agent skill install command {stream_name} reader failed: {error}"),
        }),
    }
}

async fn terminate_agent_skill_install_child(
    child: &mut tokio::process::Child,
) -> Result<ExitStatus, DomainStateError> {
    kill_agent_skill_install_process_group(child);
    let _ = child.start_kill();
    child.wait().await.map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Agent skill install command termination failed: {error}"),
    })
}

fn agent_skill_install_termination_error(
    termination: AgentSkillInstallTermination,
    timeout_duration: Duration,
    output_limit_bytes: usize,
    output: &Output,
) -> DomainStateError {
    let message = match termination {
        AgentSkillInstallTermination::Timeout => format!(
            "Agent skill install command timed out after {}ms.",
            timeout_duration.as_millis()
        ),
        AgentSkillInstallTermination::OutputLimit { stream_name } => format!(
            "Agent skill install command {stream_name} exceeded {output_limit_bytes} bytes and was terminated."
        ),
    };
    DomainStateError {
        code: "internalError",
        message: append_agent_skill_install_output(message, output),
    }
}

fn append_agent_skill_install_output(message: String, output: &Output) -> String {
    let mut parts = vec![message];
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        parts.push(format!("stderr:\n{stderr}"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        parts.push(format!("stdout:\n{stdout}"));
    }
    parts.join("\n")
}

fn configure_agent_skill_install_command_process_group(command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }

    #[cfg(not(unix))]
    let _ = command;
}

#[cfg(unix)]
fn kill_agent_skill_install_process_group(child: &tokio::process::Child) {
    let Some(process_id) = child.id() else {
        return;
    };
    let process_group_id = process_id as libc::pid_t;
    if process_group_id <= 0 {
        return;
    }
    unsafe {
        libc::kill(-process_group_id, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_agent_skill_install_process_group(_child: &tokio::process::Child) {}

fn discover_skill_locations(
    root: &AgentSkillDiscoveryRoot,
    skill_names: &[String],
) -> Result<Vec<AgentSkillLocation>, DomainStateError> {
    if !is_existing_directory(&root.path) {
        return Ok(Vec::new());
    }
    let wanted: HashSet<&str> = skill_names.iter().map(String::as_str).collect();
    let mut matches = Vec::new();
    let mut seen_real_paths = HashSet::new();
    walk_skill_root(
        root,
        &root.path,
        AGENT_SKILL_DISCOVERY_MAX_DEPTH,
        &mut seen_real_paths,
        &mut |directory_path, skill_file_path| {
            let candidates = read_skill_name_candidates(directory_path, skill_file_path);
            let Some(skill_name) = candidates
                .iter()
                .find(|candidate| wanted.contains(candidate.as_str()))
                .cloned()
            else {
                return Ok(());
            };
            matches.push(AgentSkillLocation {
                directory_path: directory_path.to_path_buf(),
                providers: root.providers.clone(),
                root_path: root.path.clone(),
                skill_file_path: skill_file_path.to_path_buf(),
                skill_name,
                source_kind: root.source_kind,
            });
            Ok(())
        },
    )?;
    Ok(matches)
}

fn walk_skill_root<F>(
    root: &AgentSkillDiscoveryRoot,
    directory_path: &Path,
    remaining_depth: usize,
    seen_real_paths: &mut HashSet<PathBuf>,
    visit_skill: &mut F,
) -> Result<(), DomainStateError>
where
    F: FnMut(&Path, &Path) -> Result<(), DomainStateError>,
{
    let resolved_path = match fs::canonicalize(directory_path) {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    if !seen_real_paths.insert(resolved_path) {
        return Ok(());
    }
    let skill_file_path = directory_path.join("SKILL.md");
    if is_existing_file(&skill_file_path) {
        visit_skill(directory_path, &skill_file_path)?;
        return Ok(());
    }
    if remaining_depth == 0 {
        return Ok(());
    }
    let mut entries = match fs::read_dir(directory_path) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return Ok(()),
    };
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name == "node_modules" || name == ".git" {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }
        if file_type.is_symlink() && root.source_kind == "repository" {
            continue;
        }
        walk_skill_root(
            root,
            &entry.path(),
            remaining_depth - 1,
            seen_real_paths,
            visit_skill,
        )?;
    }
    Ok(())
}

fn read_skill_name_candidates(directory_path: &Path, skill_file_path: &Path) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(name) = directory_path.file_name().and_then(|name| name.to_str()) {
        push_unique_candidate(&mut candidates, name.trim());
    }
    let markdown = fs::read_to_string(skill_file_path).unwrap_or_default();
    if let Some(name) = read_frontmatter_name(&markdown) {
        push_unique_candidate(&mut candidates, name.trim());
    }
    if let Some(name) = read_first_markdown_heading(&markdown) {
        push_unique_candidate(&mut candidates, name.trim());
    }
    candidates
}

fn read_frontmatter_name(markdown: &str) -> Option<String> {
    let mut lines = markdown.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        if line.trim() == "---" {
            return None;
        }
        let trimmed = line.trim();
        let Some(value) = trimmed.strip_prefix("name:") else {
            continue;
        };
        let value = value.trim();
        let name = if let Some(rest) = value.strip_prefix('"') {
            rest.split('"').next().unwrap_or_default()
        } else if let Some(rest) = value.strip_prefix('\'') {
            rest.split('\'').next().unwrap_or_default()
        } else {
            value
        };
        if !name.trim().is_empty() {
            return Some(name.trim().to_string());
        }
    }
    None
}

fn read_first_markdown_heading(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let rest = line.strip_prefix('#')?;
        let first = rest.chars().next()?;
        if !first.is_whitespace() {
            return None;
        }
        let heading = rest.trim();
        if heading.is_empty() {
            None
        } else {
            Some(heading.to_string())
        }
    })
}

fn push_unique_candidate(candidates: &mut Vec<String>, candidate: &str) {
    if candidate.is_empty() {
        return;
    }
    if !candidates.iter().any(|existing| existing == candidate) {
        candidates.push(candidate.to_string());
    }
}

fn normalize_package_source(package_source: Option<&str>) -> Result<String, DomainStateError> {
    let normalized = package_source.unwrap_or_default().trim();
    if normalized.is_empty() {
        return Err(DomainStateError::bad_request(
            "Agent skill installs require --source <skills-package-path>.",
        ));
    }
    if is_uri_like(normalized) {
        return Ok(normalized.to_string());
    }
    Ok(path_string(&normalize_absolute_path(expand_home_prefix(
        PathBuf::from(normalized),
    ))))
}

fn normalize_existing_absolute_path(candidate: &str) -> PathBuf {
    let normalized = candidate.trim();
    if normalized.is_empty() {
        return PathBuf::new();
    }
    normalize_absolute_path(expand_home_prefix(PathBuf::from(normalized)))
}

fn normalize_absolute_path(path: PathBuf) -> PathBuf {
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    normalize_path_lexically(absolute)
}

fn normalize_path_lexically(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn unique_skill_discovery_roots(
    sources: Vec<AgentSkillDiscoveryRoot>,
) -> Vec<AgentSkillDiscoveryRoot> {
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    for mut source in sources {
        source.path = normalize_absolute_path(expand_home_prefix(source.path));
        let key = format!(
            "{}:{}:{}",
            source.source_kind,
            path_string(&source.path),
            source.providers.join(",")
        );
        if seen.insert(key) {
            roots.push(source);
        }
    }
    roots
}

fn expand_home_prefix(path: PathBuf) -> PathBuf {
    let path_text = path_string(&path);
    if path_text == "~" {
        return env::var_os("HOME").map(PathBuf::from).unwrap_or(path);
    }
    if let Some(rest) = path_text.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path
}

fn read_string_array(
    params: &Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, DomainStateError> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let Some(values) = value.as_array() else {
        return Err(DomainStateError::bad_request(format!(
            "{key} must be an array of strings."
        )));
    };
    let mut strings = Vec::new();
    for value in values {
        let Some(text) = value.as_str() else {
            return Err(DomainStateError::bad_request(format!(
                "{key} must be an array of strings."
            )));
        };
        strings.push(text.to_string());
    }
    Ok(Some(strings))
}

fn root_to_value(root: &AgentSkillDiscoveryRoot) -> Value {
    json!({
        "path": path_string(&root.path),
        "providers": root.providers.clone(),
        "sourceKind": root.source_kind,
    })
}

fn location_to_value(location: &AgentSkillLocation) -> Value {
    json!({
        "directoryPath": path_string(&location.directory_path),
        "providers": location.providers.clone(),
        "rootPath": path_string(&location.root_path),
        "skillFilePath": path_string(&location.skill_file_path),
        "sourceKind": location.source_kind,
    })
}

fn is_existing_directory(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn is_existing_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn is_uri_like(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    for character in chars {
        if character == ':' {
            return true;
        }
        if !character.is_ascii_alphanumeric()
            && character != '+'
            && character != '.'
            && character != '-'
        {
            return false;
        }
    }
    false
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use std::io;

    #[test]
    fn install_command_delegates_to_skills_cli_for_default_agents() {
        let command = build_gxserver_agent_skill_install_command(
            "/Applications/Ghostex.app/Contents/Resources/CLI/skills",
            &["ghostex-browser-use".to_string()],
            None,
        );
        assert_eq!(
            &command[0..8],
            [
                "npx",
                "--yes",
                "skills",
                "add",
                "/Applications/Ghostex.app/Contents/Resources/CLI/skills",
                "--skill",
                "ghostex-browser-use",
                "--global",
            ]
        );
        assert!(command.iter().any(|item| item == "--copy"));
        assert!(command.iter().any(|item| item == "-y"));
        assert!(command.iter().any(|item| item == "claude-code"));
        assert!(command.iter().any(|item| item == "codex"));
        assert!(command.iter().any(|item| item == "cursor"));
        assert!(command.iter().any(|item| item == "gemini-cli"));
        assert!(command.iter().any(|item| item == "opencode"));
        assert!(command.iter().any(|item| item == "pi"));
    }

    #[test]
    fn install_environment_removes_profile_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let environment = BTreeMap::from([
            ("CLAUDE_CONFIG_DIR".to_string(), "/tmp/profile".to_string()),
            ("CLAUDE_PROFILE".to_string(), "personal".to_string()),
            ("CODEX_HOME".to_string(), "/tmp/codex-profile".to_string()),
            ("CODEX_PROFILE".to_string(), "personal".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ]);
        let install_environment =
            create_gxserver_agent_skill_install_environment(&paths, &environment);
        assert_eq!(
            install_environment.get("HOME"),
            Some(&path_string(temp.path()))
        );
        assert_eq!(
            install_environment.get("PATH"),
            Some(&"/usr/bin".to_string())
        );
        assert_eq!(install_environment.get("CLAUDE_CONFIG_DIR"), None);
        assert_eq!(install_environment.get("CLAUDE_PROFILE"), None);
        assert_eq!(install_environment.get("CODEX_HOME"), None);
        assert_eq!(install_environment.get("CODEX_PROFILE"), None);
    }

    #[tokio::test]
    async fn install_agent_skills_rejects_explicit_empty_skill_names() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        for skill_names in [json!([]), json!([" ", "\n"])] {
            let params = json!({
                "packageSource": path_string(temp.path()),
                "skillNames": skill_names,
            });
            let error = install_agent_skills(&paths, params.as_object().expect("params"))
                .await
                .expect_err("empty skills should fail before command execution");
            assert_eq!(error.code, "badRequest");
            assert!(error.message.contains("at least one Ghostex agent skill"));
        }
    }

    #[test]
    fn package_and_repository_paths_expand_home_before_absolutizing() {
        let Some(home) = env::var_os("HOME") else {
            return;
        };
        let home = PathBuf::from(home);
        assert_eq!(
            normalize_package_source(Some("~/skills")).expect("package source"),
            path_string(&home.join("skills"))
        );
        assert_eq!(
            normalize_existing_absolute_path("~/repo"),
            home.join("repo")
        );
    }

    #[test]
    fn install_output_limit_matches_typescript_execfile_max_buffer() {
        assert_eq!(AGENT_SKILL_INSTALL_OUTPUT_LIMIT_BYTES, 10 * 1024 * 1024);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn install_command_success_returns_bounded_stdout_and_stderr() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let output = run_agent_skill_install_command_with_limits(
            &paths,
            &path_string(temp.path()),
            &shell_command("printf success; printf diagnostic >&2"),
            Duration::from_secs(2),
            1024,
        )
        .await
        .expect("command output");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "success");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "diagnostic");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn install_command_error_preserves_bounded_stdout_and_stderr() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let error = run_agent_skill_install_command_with_limits(
            &paths,
            &path_string(temp.path()),
            &shell_command("printf before-fail; printf install-error >&2; exit 7"),
            Duration::from_secs(2),
            1024,
        )
        .await
        .expect_err("command should fail");
        assert_eq!(error.code, "internalError");
        assert!(error.message.contains("exit code 7"));
        assert!(error.message.contains("stdout:\nbefore-fail"));
        assert!(error.message.contains("stderr:\ninstall-error"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn install_command_times_out_and_returns_captured_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let error = run_agent_skill_install_command_with_limits(
            &paths,
            &path_string(temp.path()),
            &shell_command("printf started; sleep 2"),
            Duration::from_millis(150),
            1024,
        )
        .await
        .expect_err("command should time out");
        assert_eq!(error.code, "internalError");
        assert!(error.message.contains("timed out"));
        assert!(error.message.contains("stdout:\nstarted"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn install_command_kills_process_on_output_limit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let error = run_agent_skill_install_command_with_limits(
            &paths,
            &path_string(temp.path()),
            &shell_command("yes agent-skill-output-limit"),
            Duration::from_secs(5),
            128,
        )
        .await
        .expect_err("command should exceed output cap");
        assert_eq!(error.code, "internalError");
        assert!(error.message.contains("stdout exceeded 128 bytes"));
        assert!(error.message.contains("stdout:\nagent-skill"));
    }

    #[test]
    fn agent_skill_status_checks_global_repository_and_plugin_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("repo");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        write_skill(
            &temp
                .path()
                .join(".claude")
                .join("skills")
                .join("ghostex-browser-use"),
            "ghostex-browser-use",
        );
        write_skill(
            &temp
                .path()
                .join(".agents")
                .join("skills")
                .join("ghostex-computer-use"),
            "ghostex-computer-use",
        );
        write_skill(
            &temp
                .path()
                .join(".codex")
                .join("plugins")
                .join("cache")
                .join("plugin")
                .join("skills")
                .join("ghostex-generate-title"),
            "ghostex-generate-title",
        );
        write_skill(
            &repo
                .path()
                .join(".agents")
                .join("skills")
                .join("ghostex-agent-orchestration"),
            "ghostex-agent-orchestration",
        );
        let status = read_agent_skill_status(
            &paths,
            json!({ "repositoryPaths": [path_string(repo.path())] })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let skills = status
            .get("skills")
            .and_then(Value::as_array)
            .expect("skills");
        assert_eq!(skill_installed(skills, "ghostex-browser-use"), Some(true));
        assert_eq!(skill_installed(skills, "ghostex-computer-use"), Some(true));
        assert_eq!(
            skill_installed(skills, "ghostex-agent-orchestration"),
            Some(true)
        );
        assert_eq!(
            skill_installed(skills, "ghostex-generate-title"),
            Some(true)
        );
        assert_eq!(skill_installed(skills, "ghostex-manage-beads"), Some(false));
        let roots = status
            .get("roots")
            .and_then(Value::as_array)
            .expect("roots");
        assert!(roots.iter().any(|root| {
            root.get("path")
                == Some(&json!(path_string(
                    &temp.path().join(".codex").join("skills")
                )))
        }));
        assert!(roots.iter().any(|root| {
            root.get("path")
                == Some(&json!(path_string(
                    &repo.path().join(".agents").join("skills")
                )))
        }));
    }

    #[test]
    fn skill_name_candidates_match_basename_frontmatter_or_heading() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("different-name");
        fs::create_dir_all(&skill_dir).expect("mkdir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: ghostex-browser-use\n---\n# ghostex-computer-use\n",
        )
        .expect("write skill");
        let candidates = read_skill_name_candidates(&skill_dir, &skill_dir.join("SKILL.md"));
        assert!(candidates.iter().any(|name| name == "different-name"));
        assert!(candidates.iter().any(|name| name == "ghostex-browser-use"));
        assert!(candidates.iter().any(|name| name == "ghostex-computer-use"));
    }

    #[test]
    fn repository_roots_skip_symlinked_skill_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("repo");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let target = temp.path().join("target-skill");
        write_skill(&target, "ghostex-browser-use");
        let link = repo
            .path()
            .join(".agents")
            .join("skills")
            .join("ghostex-browser-use");
        fs::create_dir_all(link.parent().expect("parent")).expect("mkdir");
        symlink_dir(&target, &link).expect("symlink");
        let status = read_agent_skill_status(
            &paths,
            json!({ "repositoryPaths": [path_string(repo.path())] })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let skills = status
            .get("skills")
            .and_then(Value::as_array)
            .expect("skills");
        assert_eq!(skill_installed(skills, "ghostex-browser-use"), Some(false));
    }

    fn write_skill(directory_path: &Path, skill_name: &str) {
        fs::create_dir_all(directory_path).expect("mkdir");
        fs::write(
            directory_path.join("SKILL.md"),
            format!("# {skill_name}\n\nUse this skill for tests.\n"),
        )
        .expect("write skill");
    }

    fn skill_installed(skills: &[Value], skill_name: &str) -> Option<bool> {
        skills
            .iter()
            .find(|skill| skill.get("skillName").and_then(Value::as_str) == Some(skill_name))
            .and_then(|skill| skill.get("installed"))
            .and_then(Value::as_bool)
    }

    #[cfg(unix)]
    fn shell_command(script: &str) -> Vec<String> {
        vec!["sh".to_string(), "-c".to_string(), script.to_string()]
    }

    #[cfg(unix)]
    fn symlink_dir(source: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(source, link)
    }

    #[cfg(windows)]
    fn symlink_dir(source: &Path, link: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_dir(source, link)
    }
}
