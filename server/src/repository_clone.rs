use std::{
    collections::HashMap,
    env, fmt, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde_json::{json, Map, Value};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::{mpsc, Mutex},
    time::{sleep, Instant},
};
use uuid::Uuid;

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::DomainRepository,
    events::GxserverEventHub,
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
    paths::GxserverPaths,
    presentation::{build_presentation_project_delta, increment_presentation_revision},
    storage::open_gxserver_database,
};

const DEFAULT_REPOSITORY_HOST: &str = "github.com";
const REPOSITORY_CLONE_TIMEOUT_MS: u64 = 30 * 60_000;
const REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RepositoryCloneError {
    pub code: &'static str,
    pub message: String,
}

impl RepositoryCloneError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    fn dependency_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "dependencyUnavailable",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }
}

impl fmt::Display for RepositoryCloneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for RepositoryCloneError {}

#[derive(Clone, Default)]
pub struct RepositoryCloneJobManager {
    jobs: Arc<Mutex<HashMap<String, Value>>>,
}

#[derive(Clone)]
pub struct RepositoryCloneRuntime {
    pub event_hub: GxserverEventHub,
    pub logger: Arc<GxserverLogger>,
    pub paths: GxserverPaths,
    pub presentation_event_sequence: Arc<std::sync::Mutex<()>>,
    pub server_id: String,
}

#[derive(Debug)]
struct ParsedRepositoryCloneInput {
    clone_url: String,
    repository_name: String,
}

#[derive(Debug)]
struct CloneRunOutput {
    exit_code: i32,
    stderr: String,
    stdout: String,
}

/*
CDXC:GxserverRustPort 2026-06-16-00:49:
Phase 7 repository clone jobs remain gxserver-owned background work. Rust keeps the TypeScript preview/start/read/cancel lifecycle, rejects existing destinations before spawning Git, stores jobs in memory for initial parity, and writes only job ids plus booleans to persistent logs so clone URLs, branches, paths, argv, stdout, and stderr stay out of support bundles.

CDXC:RepositoryClone 2026-06-22-09:21:
Repository clone parity depends on matching TypeScript's URL token parsing, destination folder normalization, color-stripped Git environment, and active cancellation semantics. Canceling a running job must terminate the spawned Git process instead of only changing the in-memory job status.

CDXC:RemoteClone 2026-06-24-19:35:
Remote GPUI clone parity needs the daemon-owned clone job to register the cloned project and publish the authoritative project presentation delta after Git succeeds. Clients may refresh snapshots after completion, but the remote daemon remains the producer of project state and never relies on renderer paths as launch authority.
*/
pub async fn dispatch_repository_clone_endpoint(
    manager: RepositoryCloneJobManager,
    runtime: RepositoryCloneRuntime,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Result<Value, RepositoryCloneError> {
    match endpoint_path {
        "/api/previewRepositoryClone" => {
            Ok(json!({ "preview": preview_repository_clone(params)? }))
        }
        "/api/startRepositoryClone" => Ok(json!({ "job": manager.start(runtime, params).await? })),
        "/api/readRepositoryCloneJob" => {
            Ok(json!({ "job": manager.read(params.get("jobId")).await? }))
        }
        "/api/cancelRepositoryCloneJob" => {
            Ok(json!({ "job": manager.cancel(params.get("jobId")).await? }))
        }
        _ => Err(RepositoryCloneError::not_found(format!(
            "{endpoint_path} is not a gxserver repository clone endpoint."
        ))),
    }
}

impl RepositoryCloneJobManager {
    async fn start(
        &self,
        runtime: RepositoryCloneRuntime,
        params: &Map<String, Value>,
    ) -> Result<Value, RepositoryCloneError> {
        let preview = preview_repository_clone(params)?;
        if preview
            .get("destinationBlocked")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(RepositoryCloneError::bad_request(
                preview
                    .get("warning")
                    .and_then(Value::as_str)
                    .unwrap_or("Destination already exists.")
                    .to_string(),
            ));
        }
        let job_id = Uuid::new_v4().to_string();
        let job = json!({
            "jobId": job_id,
            "message": "Cloning repository.",
            "preview": preview,
            "startedAt": now_iso(),
            "state": "running",
        });
        self.jobs.lock().await.insert(job_id.clone(), job.clone());
        let jobs = self.jobs.clone();
        tokio::spawn(async move {
            run_clone_job(jobs, runtime, job_id).await;
        });
        Ok(job)
    }

    async fn read(&self, job_id: Option<&Value>) -> Result<Value, RepositoryCloneError> {
        let job_id = read_job_id(job_id)?;
        self.jobs.lock().await.get(&job_id).cloned().ok_or_else(|| {
            RepositoryCloneError::not_found(format!(
                "Repository clone job {job_id} does not exist."
            ))
        })
    }

    async fn cancel(&self, job_id: Option<&Value>) -> Result<Value, RepositoryCloneError> {
        let job_id = read_job_id(job_id)?;
        let mut jobs = self.jobs.lock().await;
        let job = jobs.get_mut(&job_id).ok_or_else(|| {
            RepositoryCloneError::not_found(format!(
                "Repository clone job {job_id} does not exist."
            ))
        })?;
        if job.get("state").and_then(Value::as_str) == Some("running") {
            if let Some(object) = job.as_object_mut() {
                object.insert("completedAt".to_string(), json!(now_iso()));
                object.insert("message".to_string(), json!("Repository clone canceled."));
                object.insert("state".to_string(), json!("canceled"));
            }
        }
        Ok(job.clone())
    }
}

async fn run_clone_job(
    jobs: Arc<Mutex<HashMap<String, Value>>>,
    runtime: RepositoryCloneRuntime,
    job_id: String,
) {
    let preview = {
        let jobs = jobs.lock().await;
        jobs.get(&job_id)
            .and_then(|job| job.get("preview"))
            .cloned()
            .unwrap_or_else(|| json!({}))
    };
    let branch_specified = preview.get("branchName").and_then(Value::as_str).is_some();
    let clone_main_only = preview
        .get("cloneMainOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let shallow_clone = preview
        .get("shallowClone")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let _ = runtime.logger.log_routine(
        DiagnosticLogScenario::RepositoryClone,
        GxserverLogInput {
            level: LogLevel::Info,
            event: "repositoryClone.started".to_string(),
            server_id: Some(runtime.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "branchSpecified": branch_specified,
                "cloneMainOnly": clone_main_only,
                "jobId": job_id,
                "shallowClone": shallow_clone,
            })),
        },
    );

    let args = build_repository_clone_git_args(&preview);
    let parent_path = preview
        .get("parentPath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    /*
    CDXC:AddProjectDialog 2026-07-30:
    A destination typed in the Add Project dialog can name folders that do not
    exist yet, so the parent chain is created here, right before git runs in it.
    In the `parentPath` shape the parent was already validated as an existing
    directory, so this is a no-op there.
    */
    if let Err(error) = fs::create_dir_all(&parent_path) {
        mark_job_failed(
            &jobs,
            &runtime,
            &job_id,
            format!("Could not create the destination folder: {error}"),
            None,
            "badRequest",
        )
        .await;
        return;
    }
    let clone_result = run_git_clone_process(args, parent_path, jobs.clone(), job_id.clone()).await;
    match clone_result {
        Ok(output) => {
            if job_is_canceled(&jobs, &job_id).await {
                record_job_output(&jobs, &job_id, output).await;
                return;
            }
            if output.exit_code != 0 {
                mark_job_failed(
                    &jobs,
                    &runtime,
                    &job_id,
                    output
                        .stderr
                        .clone()
                        .or_else(&output.stdout)
                        .or_else(&format!("git clone exited {}.", output.exit_code)),
                    Some(output),
                    "dependencyUnavailable",
                )
                .await;
                return;
            }
            mark_job_adding(&jobs, &job_id).await;
            match add_cloned_project(&runtime, &preview) {
                Ok(project) => {
                    if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
                        let _ = publish_cloned_project_presentation(&runtime, project_id);
                    }
                    let project_path = project
                        .get("path")
                        .and_then(Value::as_str)
                        .or_else(|| preview.get("destinationPath").and_then(Value::as_str))
                        .unwrap_or_default()
                        .to_string();
                    let mut jobs = jobs.lock().await;
                    if let Some(job) = jobs.get_mut(&job_id).and_then(Value::as_object_mut) {
                        job.insert("completedAt".to_string(), json!(now_iso()));
                        job.insert("exitCode".to_string(), json!(output.exit_code));
                        job.insert("message".to_string(), json!("Repository cloned."));
                        job.insert("project".to_string(), project.clone());
                        job.insert("projectPath".to_string(), json!(project_path));
                        job.insert("state".to_string(), json!("completed"));
                        job.insert("stderr".to_string(), json!(output.stderr));
                        job.insert("stdout".to_string(), json!(output.stdout));
                    }
                    let _ = runtime.logger.log_routine(
                        DiagnosticLogScenario::RepositoryClone,
                        GxserverLogInput {
                            level: LogLevel::Info,
                            event: "repositoryClone.completed".to_string(),
                            server_id: Some(runtime.server_id.clone()),
                            request_id: None,
                            client: None,
                            duration_ms: None,
                            error: None,
                            details: Some(json!({
                                "jobId": job_id,
                                "projectId": project.get("projectId").and_then(Value::as_str),
                            })),
                        },
                    );
                }
                Err(error) => {
                    mark_job_failed(
                        &jobs,
                        &runtime,
                        &job_id,
                        error.to_string(),
                        Some(output),
                        "badRequest",
                    )
                    .await;
                }
            }
        }
        Err(error) => {
            if !job_is_canceled(&jobs, &job_id).await {
                mark_job_failed(&jobs, &runtime, &job_id, error.message, None, error.code).await;
            }
        }
    }
}

async fn record_job_output(
    jobs: &Arc<Mutex<HashMap<String, Value>>>,
    job_id: &str,
    output: CloneRunOutput,
) {
    let mut jobs = jobs.lock().await;
    if let Some(job) = jobs.get_mut(job_id).and_then(Value::as_object_mut) {
        job.insert("exitCode".to_string(), json!(output.exit_code));
        job.insert("stderr".to_string(), json!(output.stderr));
        job.insert("stdout".to_string(), json!(output.stdout));
    }
}

async fn mark_job_adding(jobs: &Arc<Mutex<HashMap<String, Value>>>, job_id: &str) {
    let mut jobs = jobs.lock().await;
    if let Some(job) = jobs.get_mut(job_id).and_then(Value::as_object_mut) {
        job.insert("message".to_string(), json!("Adding cloned repository."));
    }
}

async fn mark_job_failed(
    jobs: &Arc<Mutex<HashMap<String, Value>>>,
    runtime: &RepositoryCloneRuntime,
    job_id: &str,
    message: String,
    output: Option<CloneRunOutput>,
    error_code: &'static str,
) {
    {
        let mut jobs = jobs.lock().await;
        if let Some(job) = jobs.get_mut(job_id).and_then(Value::as_object_mut) {
            job.insert("completedAt".to_string(), json!(now_iso()));
            if let Some(output) = output {
                job.insert("exitCode".to_string(), json!(output.exit_code));
                job.insert("stderr".to_string(), json!(output.stderr));
                job.insert("stdout".to_string(), json!(output.stdout));
            }
            job.insert("error".to_string(), json!(message));
            job.insert(
                "message".to_string(),
                job.get("error")
                    .cloned()
                    .unwrap_or_else(|| json!("Repository clone failed.")),
            );
            job.insert("state".to_string(), json!("failed"));
        }
    }
    let _ = runtime.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "repositoryClone.failed".to_string(),
        server_id: Some(runtime.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "errorCode": error_code,
            "jobId": job_id,
        })),
    });
}

async fn job_is_canceled(jobs: &Arc<Mutex<HashMap<String, Value>>>, job_id: &str) -> bool {
    jobs.lock()
        .await
        .get(job_id)
        .and_then(|job| job.get("state"))
        .and_then(Value::as_str)
        == Some("canceled")
}

fn publish_cloned_project_presentation(
    runtime: &RepositoryCloneRuntime,
    project_id: &str,
) -> Result<(), RepositoryCloneError> {
    let db = open_gxserver_database(&runtime.paths)
        .map_err(|error| RepositoryCloneError::dependency_unavailable(error.to_string()))?;
    let repository = DomainRepository::new(&db, runtime.server_id.as_str());
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
    A just-cloned repository is the case where the `origin` remote matters most —
    it is by definition a checkout of a repository that exists elsewhere — so its
    remote is probed here, outside the sequencer below, and rides the very delta
    that announces the project. Same first-sighting rule as
    `schedule_presentation_project_delta`: this warms an unprobed path once and
    leaves refreshes to the background pass.
    */
    /*
    CDXC:ProjectRemoteCopy 2026-08-26:
    Both sidebar versions need the origin now: V2 groups by it and the classic
    project menu copies it. Probe before the clone's first presentation delta so
    either surface receives the URL immediately.
    */
    if let Ok(Some(project)) = repository.get_project(project_id) {
        crate::project_git_remote::ensure_project_git_remote_probed(&project, true);
        /*
        CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
        A just-cloned repository has its icon on disk already, so discovering it
        here means the delta that announces the project shows the project's own
        icon rather than a folder glyph that changes a minute later. Same
        first-sighting rule as the remote probe above.
        */
        crate::project_icon::ensure_project_icon_probed(&project);
    }
    let _event_sequence = runtime.presentation_event_sequence.lock().map_err(|_| {
        RepositoryCloneError::dependency_unavailable("Presentation event sequencer is poisoned.")
    })?;
    let delta = build_presentation_project_delta(&repository, project_id, "projectAdded")
        .map_err(|error| RepositoryCloneError::dependency_unavailable(error.to_string()))?;
    let revision = increment_presentation_revision(&db)
        .map_err(|error| RepositoryCloneError::dependency_unavailable(error.to_string()))?;
    runtime.event_hub.broadcast(json!({
        "delta": delta,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": runtime.server_id.clone(),
        "type": "presentationDelta",
    }));
    Ok(())
}

fn add_cloned_project(
    runtime: &RepositoryCloneRuntime,
    preview: &Value,
) -> Result<Value, RepositoryCloneError> {
    let destination_path = preview
        .get("destinationPath")
        .and_then(Value::as_str)
        .ok_or_else(|| RepositoryCloneError::bad_request("destinationPath is missing."))?;
    let normalized_path = normalize_existing_directory_path(
        Some(&Value::String(destination_path.to_string())),
        "destinationPath",
    )?;
    let db = open_gxserver_database(&runtime.paths)
        .map_err(|error| RepositoryCloneError::dependency_unavailable(error.to_string()))?;
    let repository = DomainRepository::new(&db, runtime.server_id.as_str());
    for project in repository
        .list_projects()
        .map_err(|error| RepositoryCloneError::dependency_unavailable(error.to_string()))?
    {
        if project.get("path").and_then(Value::as_str) == Some(normalized_path.as_str()) {
            return Ok(project);
        }
    }
    let mut params = Map::new();
    params.insert(
        "name".to_string(),
        preview
            .get("destinationFolderName")
            .cloned()
            .unwrap_or_else(|| json!("Repository")),
    );
    params.insert("path".to_string(), json!(normalized_path));
    repository
        .create_project(&params)
        .map_err(|error| RepositoryCloneError::dependency_unavailable(error.to_string()))
}

/*
CDXC:AddProjectDialog 2026-07-30:
The Add Project dialog's clone step asks for ONE destination path the user typed
or browsed to. A missing path (`~/projects/my-app`) is the folder Git creates,
and an existing empty directory is cloned into directly. An existing non-empty
directory (`~/projects/`) is the selected parent, so the repository name is
appended and Git creates `~/projects/my-app`. The resolved path is then split
into the parent Git runs in and the leaf folder it creates; missing parent
chains are created by the job immediately before Git runs.

The older `parentPath` + `destinationFolderName` shape is unchanged, including
its stricter rule that ANY existing destination blocks the clone; the Clone
Repository modal depends on that warning. `destinationBlocked` is what the start
endpoint reads, so the two shapes can disagree about what "exists" means without
either one bending to the other.
*/
fn preview_repository_clone(params: &Map<String, Value>) -> Result<Value, RepositoryCloneError> {
    let repository_input = read_required_string(
        params
            .get("repositoryInput")
            .filter(|value| !value.is_null())
            .or_else(|| params.get("remoteUrl")),
        "repositoryInput",
    )?;
    let parsed = parse_repository_clone_input(&repository_input)
        .ok_or_else(|| RepositoryCloneError::bad_request("Enter a Git repository to clone."))?;
    let default_folder_name = normalize_repository_destination_folder_name(
        Some(&Value::String(parsed.repository_name.clone())),
        "repository",
    )?;
    let destination_path_input = params
        .get("destinationPath")
        .filter(|value| !value.is_null());
    let (parent_path, destination_folder_name, destination_path, allow_empty_destination) =
        match destination_path_input {
            Some(input) => {
                let requested_path = normalize_absolute_path(Some(input), "destinationPath")?;
                let requested_destination = read_destination_status(&requested_path)?;
                let destination_path = if requested_destination.exists
                    && requested_destination.kind.as_deref() == Some("directory")
                    && requested_destination.is_empty == Some(false)
                {
                    normalize_path_string(Path::new(&requested_path).join(&default_folder_name))
                } else {
                    requested_path
                };
                let path = PathBuf::from(&destination_path);
                let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                    return Err(RepositoryCloneError::bad_request(
                        "destinationPath must name a folder to create.",
                    ));
                };
                (
                    normalize_path_string(parent),
                    name.to_string_lossy().to_string(),
                    destination_path,
                    true,
                )
            }
            None => {
                let parent_value = params
                    .get("parentPath")
                    .or_else(|| params.get("folderPath"));
                let parent_path = normalize_existing_directory_path(parent_value, "parentPath")?;
                let requested_folder = params
                    .get("destinationFolderName")
                    .or_else(|| params.get("newFolderName"));
                let destination_folder_name = normalize_repository_destination_folder_name(
                    requested_folder,
                    &default_folder_name,
                )?;
                let destination_path =
                    normalize_path_string(Path::new(&parent_path).join(&destination_folder_name));
                (
                    parent_path,
                    destination_folder_name,
                    destination_path,
                    false,
                )
            }
        };
    if !is_path_inside(&parent_path, &destination_path) || destination_path == parent_path {
        return Err(RepositoryCloneError::bad_request(
            "destinationFolderName must create a child folder inside parentPath.",
        ));
    }
    let destination = read_destination_status(&destination_path)?;
    let destination_blocked = destination.exists
        && !(allow_empty_destination
            && destination.kind.as_deref() == Some("directory")
            && destination.is_empty == Some(true));
    let branch_name = normalize_repository_branch_name(params.get("branchName"))?;
    let mut preview = Map::new();
    if let Some(branch_name) = branch_name {
        preview.insert("branchName".to_string(), json!(branch_name));
    }
    preview.insert(
        "cloneMainOnly".to_string(),
        json!(params.get("cloneMainOnly").and_then(Value::as_bool) == Some(true)),
    );
    preview.insert("cloneUrl".to_string(), json!(parsed.clone_url));
    preview.insert("defaultFolderName".to_string(), json!(default_folder_name));
    preview.insert("destinationBlocked".to_string(), json!(destination_blocked));
    preview.insert("destinationExists".to_string(), json!(destination.exists));
    if let Some(kind) = destination.kind.clone() {
        preview.insert("destinationExistsKind".to_string(), json!(kind));
    }
    preview.insert(
        "destinationFolderName".to_string(),
        json!(destination_folder_name),
    );
    if let Some(is_empty) = destination.is_empty {
        preview.insert("destinationIsEmpty".to_string(), json!(is_empty));
    }
    preview.insert(
        "destinationPath".to_string(),
        json!(destination_path.clone()),
    );
    preview.insert("parentPath".to_string(), json!(parent_path));
    preview.insert("repositoryName".to_string(), json!(parsed.repository_name));
    preview.insert(
        "shallowClone".to_string(),
        json!(params.get("shallowClone").and_then(Value::as_bool) == Some(true)),
    );
    if destination_blocked {
        let warning = if allow_empty_destination {
            if destination.kind.as_deref() == Some("directory") {
                "Destination path already exists and is not empty.".to_string()
            } else {
                "Destination path already exists and is not a directory.".to_string()
            }
        } else {
            format!(
                "A {} already exists at {}. Choose a new folder name before cloning.",
                destination
                    .kind
                    .clone()
                    .unwrap_or_else(|| "filesystem item".to_string()),
                destination_path
            )
        };
        preview.insert("warning".to_string(), json!(warning));
    }
    Ok(Value::Object(preview))
}

fn build_repository_clone_git_args(preview: &Value) -> Vec<String> {
    let mut args = vec!["clone".to_string()];
    if let Some(branch) = preview.get("branchName").and_then(Value::as_str) {
        args.extend(["--branch".to_string(), branch.to_string()]);
    }
    if preview
        .get("cloneMainOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        args.push("--single-branch".to_string());
    }
    if preview
        .get("shallowClone")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        args.extend(["--depth".to_string(), "1".to_string()]);
    }
    args.push(
        preview
            .get("cloneUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    args.push(
        preview
            .get("destinationFolderName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    args
}

async fn run_git_clone_process(
    args: Vec<String>,
    cwd: String,
    jobs: Arc<Mutex<HashMap<String, Value>>>,
    job_id: String,
) -> Result<CloneRunOutput, RepositoryCloneError> {
    run_clone_process("git", args, cwd, jobs, job_id).await
}

async fn run_clone_process(
    executable: &str,
    args: Vec<String>,
    cwd: String,
    jobs: Arc<Mutex<HashMap<String, Value>>>,
    job_id: String,
) -> Result<CloneRunOutput, RepositoryCloneError> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(repository_clone_environment())
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        RepositoryCloneError::dependency_unavailable(format!("Could not start git clone: {error}"))
    })?;
    let (limit_sender, mut limit_receiver) = mpsc::unbounded_channel();
    let stdout_task = child
        .stdout
        .take()
        .map(|stdout| tokio::spawn(read_capped_output(stdout, "stdout", limit_sender.clone())));
    let stderr_task = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_capped_output(stderr, "stderr", limit_sender)));
    let deadline = Instant::now() + Duration::from_millis(REPOSITORY_CLONE_TIMEOUT_MS);
    let mut terminal_error: Option<RepositoryCloneError> = None;
    let mut sigkill_deadline: Option<Instant> = None;
    let mut sigkill_sent = false;
    let mut termination_requested = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                return Err(RepositoryCloneError::dependency_unavailable(format!(
                    "git clone failed: {error}"
                )));
            }
        }
        while let Ok(limit_error) = limit_receiver.try_recv() {
            if terminal_error.is_none() {
                terminal_error = Some(limit_error);
                if !termination_requested {
                    sigkill_deadline = request_child_termination(&mut child);
                    termination_requested = true;
                }
            }
        }
        if terminal_error.is_none() && Instant::now() >= deadline {
            terminal_error = Some(RepositoryCloneError::dependency_unavailable(format!(
                "git clone timed out after {REPOSITORY_CLONE_TIMEOUT_MS}ms."
            )));
            if !termination_requested {
                sigkill_deadline = request_child_termination(&mut child);
                termination_requested = true;
            }
        }
        if terminal_error.is_none()
            && !termination_requested
            && job_is_canceled(&jobs, &job_id).await
        {
            sigkill_deadline = request_child_termination(&mut child);
            termination_requested = true;
        }
        if let Some(deadline) = sigkill_deadline {
            if !sigkill_sent && Instant::now() >= deadline {
                let _ = child.start_kill();
                sigkill_sent = true;
            }
        }
        tokio::select! {
            Some(limit_error) = limit_receiver.recv() => {
                if terminal_error.is_none() {
                    terminal_error = Some(limit_error);
                    if !termination_requested {
                        sigkill_deadline = request_child_termination(&mut child);
                        termination_requested = true;
                    }
                }
            }
            _ = sleep(Duration::from_millis(25)) => {}
        }
    };
    let stdout = join_output_task(stdout_task).await;
    let stderr = join_output_task(stderr_task).await;
    while let Ok(limit_error) = limit_receiver.try_recv() {
        if terminal_error.is_none() {
            terminal_error = Some(limit_error);
        }
    }
    if let Some(error) = terminal_error {
        return Err(error);
    }
    let was_canceled = job_is_canceled(&jobs, &job_id).await;
    Ok(CloneRunOutput {
        exit_code: status.code().unwrap_or(if was_canceled { 130 } else { 1 }),
        stderr,
        stdout,
    })
}

async fn read_capped_output<R>(
    mut reader: R,
    stream_name: &'static str,
    limit_sender: mpsc::UnboundedSender<RepositoryCloneError>,
) -> String
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut total_bytes = 0usize;
    let mut reported_limit = false;
    let mut buffer = [0u8; 8192];
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => break,
        };
        let next_bytes = total_bytes.saturating_add(read);
        let remaining = REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES.saturating_sub(total_bytes);
        if remaining > 0 {
            output.extend_from_slice(&buffer[..read.min(remaining)]);
        }
        if next_bytes > REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES && !reported_limit {
            let _ = limit_sender.send(RepositoryCloneError::dependency_unavailable(format!(
                "git clone {stream_name} exceeded {REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES} bytes."
            )));
            reported_limit = true;
        }
        total_bytes = next_bytes;
    }
    String::from_utf8_lossy(&output).trim().to_string()
}

async fn join_output_task(task: Option<tokio::task::JoinHandle<String>>) -> String {
    match task {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    }
}

fn request_child_termination(child: &mut Child) -> Option<Instant> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            return Some(Instant::now() + Duration::from_millis(1_000));
        }
    }
    let _ = child.start_kill();
    None
}

fn repository_clone_environment() -> Vec<(String, String)> {
    let mut environment: Vec<(String, String)> = env::vars().collect();
    environment.retain(|(key, _)| {
        !matches!(
            key.as_str(),
            "ANSI_COLORS_DISABLED" | "NO_COLOR" | "NODE_DISABLE_COLORS"
        )
    });
    environment
}

pub(crate) fn canonical_repository_lookup_url(input: &str) -> Option<String> {
    let parsed = parse_repository_clone_input(input)?;
    if let Some(rest) = parsed.clone_url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return Some(format!("https://{host}/{path}"));
    }
    if let Some((host, path)) = parse_ssh_repository_token(&parsed.clone_url) {
        return Some(format!("https://{host}/{path}"));
    }
    Some(parsed.clone_url)
}

fn parse_repository_clone_input(input: &str) -> Option<ParsedRepositoryCloneInput> {
    let token = extract_repository_input_token(input)?;
    if token
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("git@"))
    {
        let (host, repository_path) = token[4..].split_once(':')?;
        let repository_path = normalize_repository_path(repository_path);
        if host.trim().is_empty() || repository_path.is_empty() {
            return None;
        }
        return Some(ParsedRepositoryCloneInput {
            clone_url: format!("git@{}:{repository_path}", host.trim()),
            repository_name: repository_name_from_path(&repository_path),
        });
    }
    if let Some((host, path)) = parse_ssh_repository_token(&token) {
        let repository_path = normalize_repository_path(path);
        if host.trim().is_empty() || repository_path.is_empty() {
            return None;
        }
        return Some(ParsedRepositoryCloneInput {
            clone_url: format!("ssh://git@{}/{repository_path}", host.trim()),
            repository_name: repository_name_from_path(&repository_path),
        });
    }
    if let Some((host, path)) = parse_http_repository_token(&token) {
        let repository_path = normalize_repository_path(path);
        if host.trim().is_empty() || repository_path.is_empty() || !host.contains('.') {
            return None;
        }
        return Some(ParsedRepositoryCloneInput {
            clone_url: format!("https://{}/{repository_path}", host.trim()),
            repository_name: repository_name_from_path(&repository_path),
        });
    }
    let shorthand_path = normalize_repository_path(&token);
    if shorthand_path.split('/').count() < 2 {
        return None;
    }
    Some(ParsedRepositoryCloneInput {
        clone_url: format!("https://{DEFAULT_REPOSITORY_HOST}/{shorthand_path}"),
        repository_name: repository_name_from_path(&shorthand_path),
    })
}

fn extract_repository_input_token(input: &str) -> Option<String> {
    let tokens: Vec<String> = input
        .split_whitespace()
        .map(clean_repository_input_token)
        .filter(|token| !token.is_empty())
        .collect();
    let gh_clone_index = tokens
        .windows(3)
        .position(|window| window == ["gh", "repo", "clone"]);
    if let Some(index) = gh_clone_index {
        return tokens[index + 3..]
            .iter()
            .find(|token| is_repository_like_token(token))
            .cloned();
    }
    tokens
        .into_iter()
        .find(|token| is_repository_like_token(token))
}

fn clean_repository_input_token(token: &str) -> String {
    token
        .trim()
        .trim_start_matches(['<', '(', '"', '\'', '`'])
        .trim_end_matches(['>', ')', ',', '.', '"', '\'', '`'])
        .to_string()
}

fn is_repository_like_token(token: &str) -> bool {
    if token.is_empty() || token.starts_with('-') {
        return false;
    }
    if token
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("git@"))
    {
        return token[4..]
            .split_once(':')
            .is_some_and(|(host, path)| !host.is_empty() && !path.is_empty());
    }
    if let Some(rest) = strip_ascii_case_prefix(token, "ssh://") {
        return has_repository_like_scheme_path(rest);
    }
    if let Some(rest) = strip_ascii_case_prefix(token, "http://")
        .or_else(|| strip_ascii_case_prefix(token, "https://"))
    {
        return has_repository_like_scheme_path(rest);
    }
    if looks_like_host_path(token) {
        return true;
    }
    token.split_once('/').is_some_and(|(owner, path)| {
        !owner.is_empty() && !path.is_empty() && !path.starts_with('/')
    })
}

fn looks_like_host_path(token: &str) -> bool {
    token
        .split_once('/')
        .is_some_and(|(host, path)| host.contains('.') && !path.is_empty())
}

fn has_repository_like_scheme_path(rest: &str) -> bool {
    rest.char_indices()
        .any(|(index, ch)| ch == '/' && index > 0 && index < rest.len() - 1)
}

fn parse_ssh_repository_token(token: &str) -> Option<(&str, &str)> {
    let rest = strip_ascii_case_prefix(token, "ssh://")?;
    let slash_index = rest.find('/')?;
    let authority = &rest[..slash_index];
    let path = &rest[slash_index + 1..];
    if authority.is_empty() || path.is_empty() {
        return None;
    }
    let host = match authority.find('@') {
        Some(index) if index > 0 => &authority[index + 1..],
        _ => authority,
    };
    if host.is_empty() {
        return None;
    }
    Some((host, path))
}

fn parse_http_repository_token(token: &str) -> Option<(&str, &str)> {
    let without_scheme = strip_ascii_case_prefix(token, "https://")
        .or_else(|| strip_ascii_case_prefix(token, "http://"))
        .unwrap_or(token);
    let (host, path) = without_scheme.split_once('/')?;
    if host.contains('.') && !path.is_empty() {
        Some((host, path))
    } else {
        None
    }
}

fn normalize_repository_path(repository_path: &str) -> String {
    let before_hash = repository_path.split('#').next().unwrap_or_default();
    let before_query = before_hash.split('?').next().unwrap_or_default();
    let before_git_suffix = normalize_git_suffix(before_query);
    let mut segments: Vec<String> = Vec::new();
    for segment in before_git_suffix.split('/') {
        let segment = decode_repository_path_segment(segment);
        if segment.is_empty() {
            continue;
        }
        let lower = segment.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "-" | "branches"
                | "commit"
                | "commits"
                | "issues"
                | "pull"
                | "pulls"
                | "releases"
                | "src"
                | "tree"
                | "wiki"
        ) {
            break;
        }
        segments.push(segment.to_string());
    }
    let mut normalized = segments.join("/");
    if let Some(stripped) = normalized.strip_suffix(".git") {
        normalized = format!("{stripped}.git");
    } else if !normalized.is_empty() {
        normalized.push_str(".git");
    }
    normalized
}

fn repository_name_from_path(path: &str) -> String {
    let repository_name = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
        .unwrap_or("repository");
    repository_name
        .strip_suffix(".git")
        .unwrap_or(repository_name)
        .to_string()
}

fn strip_ascii_case_prefix<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    input
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .then(|| &input[prefix.len()..])
}

fn normalize_git_suffix(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let mut search_start = 0usize;
    while let Some(relative_index) = lower[search_start..].find(".git") {
        let index = search_start + relative_index;
        let rest = &path[index + 4..];
        if rest.is_empty() || rest.starts_with('/') {
            return format!("{}.git", &path[..index]);
        }
        search_start = index + 4;
    }
    path.to_string()
}

fn decode_repository_path_segment(segment: &str) -> String {
    let segment = segment.trim();
    percent_decode_utf8(segment).unwrap_or_else(|| segment.to_string())
}

fn percent_decode_utf8(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex_value(*byte))?;
            let low = bytes.get(index + 2).and_then(|byte| hex_value(*byte))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn normalize_repository_destination_folder_name(
    input: Option<&Value>,
    fallback: &str,
) -> Result<String, RepositoryCloneError> {
    let raw_name = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback);
    let mut normalized = String::new();
    let mut previous_space = false;
    let mut previous_separator = false;
    for ch in raw_name.chars() {
        if matches!(ch, '/' | ':' | '\\') {
            if !previous_separator {
                normalized.push('-');
            }
            previous_separator = true;
            previous_space = false;
        } else if ch.is_whitespace() {
            if !previous_space {
                normalized.push(' ');
                previous_space = true;
            }
            previous_separator = false;
        } else {
            normalized.push(ch);
            previous_space = false;
            previous_separator = false;
        }
    }
    let normalized = normalized.trim().to_string();
    if normalized.is_empty() || normalized.chars().all(|ch| ch == '.') {
        return Err(RepositoryCloneError::bad_request(
            "newFolderName must be a valid folder name.",
        ));
    }
    Ok(normalized)
}

fn normalize_repository_branch_name(
    input: Option<&Value>,
) -> Result<Option<String>, RepositoryCloneError> {
    let Some(value) = input else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let branch_name = value
        .as_str()
        .ok_or_else(|| RepositoryCloneError::bad_request("branchName must be a string."))?;
    let branch_name = branch_name.trim();
    if branch_name.is_empty() {
        return Ok(None);
    }
    if !is_repository_branch_name_valid(branch_name) {
        return Err(RepositoryCloneError::bad_request(
            "branchName must be a valid Git branch name.",
        ));
    }
    Ok(Some(branch_name.to_string()))
}

fn is_repository_branch_name_valid(branch_name: &str) -> bool {
    branch_name.encode_utf16().count() <= 255
        && branch_name != "@"
        && !branch_name.starts_with(['-', '/'])
        && !branch_name.ends_with(['/', '.'])
        && !branch_name.contains("..")
        && !branch_name.contains("@{")
        && !branch_name.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
                || matches!(ch, '\u{0}'..='\u{1F}' | '\u{7F}')
        })
        && branch_name.split('/').all(|segment| {
            !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
        })
}

#[derive(Debug)]
struct DestinationStatus {
    exists: bool,
    is_empty: Option<bool>,
    kind: Option<String>,
}

fn read_destination_status(
    destination_path: &str,
) -> Result<DestinationStatus, RepositoryCloneError> {
    match fs::symlink_metadata(destination_path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                let is_empty = fs::read_dir(destination_path)
                    .map_err(|error| RepositoryCloneError::bad_request(error.to_string()))?
                    .next()
                    .is_none();
                Ok(DestinationStatus {
                    exists: true,
                    is_empty: Some(is_empty),
                    kind: Some("directory".to_string()),
                })
            } else {
                Ok(DestinationStatus {
                    exists: true,
                    is_empty: None,
                    kind: Some(if metadata.is_file() { "file" } else { "other" }.to_string()),
                })
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DestinationStatus {
            exists: false,
            is_empty: None,
            kind: None,
        }),
        Err(error) => Err(RepositoryCloneError::bad_request(error.to_string())),
    }
}

fn read_required_string(
    input: Option<&Value>,
    field: &str,
) -> Result<String, RepositoryCloneError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(RepositoryCloneError::bad_request(format!(
            "{field} must be a non-empty string."
        )));
    }
    Ok(text.to_string())
}

fn read_job_id(input: Option<&Value>) -> Result<String, RepositoryCloneError> {
    read_required_string(input, "jobId")
}

fn normalize_existing_directory_path(
    input: Option<&Value>,
    field: &str,
) -> Result<String, RepositoryCloneError> {
    let path = normalize_absolute_path(input, field)?;
    let metadata = fs::metadata(&path)
        .map_err(|_| RepositoryCloneError::not_found(format!("{field} does not exist: {path}")))?;
    if metadata.is_dir() {
        Ok(path)
    } else {
        Err(RepositoryCloneError::bad_request(format!(
            "{field} is not a directory: {path}"
        )))
    }
}

fn normalize_absolute_path(
    input: Option<&Value>,
    field: &str,
) -> Result<String, RepositoryCloneError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(RepositoryCloneError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(text);
    if !expanded.is_absolute() {
        return Err(RepositoryCloneError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    Ok(normalize_path_string(expanded))
}

fn expand_user_path(input: &str) -> PathBuf {
    if input == "~" {
        return home_dir();
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(input)
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn normalize_path_string(path: impl AsRef<Path>) -> String {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(Path::new("/")),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            std::path::Component::Normal(segment) => normalized.push(segment),
        }
    }
    normalized.to_string_lossy().to_string()
}

fn is_path_inside(parent_path: &str, candidate_path: &str) -> bool {
    let parent = Path::new(parent_path);
    let candidate = Path::new(candidate_path);
    parent == candidate || candidate.starts_with(parent)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

trait NonEmptyString {
    fn or_else(self, fallback: &str) -> String;
}

impl NonEmptyString for String {
    fn or_else(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn preview_repository_clone_accepts_remote_url_and_destination_path() {
        let dir = tempdir().unwrap();
        let destination = dir.path().join("nested").join("my-app");
        let mut params = Map::new();
        params.insert(
            "remoteUrl".to_string(),
            json!("git@github.com:factory-ai/ghostex.git"),
        );
        params.insert(
            "destinationPath".to_string(),
            json!(destination.to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("cloneUrl").and_then(Value::as_str),
            Some("git@github.com:factory-ai/ghostex.git")
        );
        assert_eq!(
            preview.get("destinationFolderName").and_then(Value::as_str),
            Some("my-app")
        );
        assert_eq!(
            preview.get("destinationPath").and_then(Value::as_str),
            Some(normalize_path_string(&destination).as_str())
        );
        assert_eq!(
            preview.get("parentPath").and_then(Value::as_str),
            Some(normalize_path_string(dir.path().join("nested")).as_str())
        );
        assert_eq!(
            preview.get("destinationBlocked").and_then(Value::as_bool),
            Some(false)
        );
        assert!(preview.get("warning").is_none());
    }

    #[test]
    fn preview_repository_clone_uses_an_empty_destination_and_appends_to_a_populated_one() {
        let dir = tempdir().unwrap();
        let empty = dir.path().join("empty-destination");
        fs::create_dir_all(&empty).unwrap();
        let populated = dir.path().join("populated-destination");
        fs::create_dir_all(&populated).unwrap();
        fs::write(populated.join("README.md"), "occupied\n").unwrap();

        let mut params = Map::new();
        params.insert(
            "remoteUrl".to_string(),
            json!("git@github.com:factory-ai/ghostex.git"),
        );
        params.insert(
            "destinationPath".to_string(),
            json!(empty.to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("destinationExists").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            preview.get("destinationBlocked").and_then(Value::as_bool),
            Some(false)
        );

        params.insert(
            "destinationPath".to_string(),
            json!(populated.to_string_lossy()),
        );
        let nested = preview_repository_clone(&params).unwrap();
        assert_eq!(
            nested.get("destinationBlocked").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            nested.get("destinationPath").and_then(Value::as_str),
            Some(normalize_path_string(populated.join("ghostex")).as_str())
        );
        assert_eq!(
            nested.get("parentPath").and_then(Value::as_str),
            Some(normalize_path_string(&populated).as_str())
        );
        assert_eq!(
            nested.get("destinationFolderName").and_then(Value::as_str),
            Some("ghostex")
        );
        assert!(nested.get("warning").is_none());

        fs::create_dir_all(populated.join("ghostex")).unwrap();
        fs::write(populated.join("ghostex").join("README.md"), "occupied\n").unwrap();
        let blocked = preview_repository_clone(&params).unwrap();
        assert_eq!(
            blocked.get("destinationBlocked").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            blocked.get("warning").and_then(Value::as_str),
            Some("Destination path already exists and is not empty.")
        );

        let file_destination = dir.path().join("file-destination");
        fs::write(&file_destination, "file\n").unwrap();
        params.insert(
            "destinationPath".to_string(),
            json!(file_destination.to_string_lossy()),
        );
        let file_blocked = preview_repository_clone(&params).unwrap();
        assert_eq!(
            file_blocked
                .get("destinationBlocked")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            file_blocked.get("warning").and_then(Value::as_str),
            Some("Destination path already exists and is not a directory.")
        );
    }

    #[test]
    fn preview_repository_clone_keeps_the_parent_path_shape_blocking_on_any_existing_destination() {
        let dir = tempdir().unwrap();
        let existing = dir.path().join("ghostex");
        fs::create_dir_all(&existing).unwrap();
        let mut params = Map::new();
        params.insert(
            "repositoryInput".to_string(),
            json!("https://github.com/factory-ai/ghostex"),
        );
        params.insert(
            "parentPath".to_string(),
            json!(dir.path().to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("destinationBlocked").and_then(Value::as_bool),
            Some(true)
        );
        assert!(preview
            .get("warning")
            .and_then(Value::as_str)
            .is_some_and(|warning| warning.starts_with("A directory already exists at ")));
    }

    #[test]
    fn preview_repository_clone_normalizes_github_browser_url() {
        let dir = tempdir().unwrap();
        let mut params = Map::new();
        params.insert(
            "repositoryInput".to_string(),
            json!("https://github.com/factory-ai/ghostex/tree/main"),
        );
        params.insert(
            "parentPath".to_string(),
            json!(dir.path().to_string_lossy()),
        );
        params.insert("shallowClone".to_string(), json!(true));
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("cloneUrl").and_then(Value::as_str),
            Some("https://github.com/factory-ai/ghostex.git")
        );
        assert_eq!(
            preview.get("destinationFolderName").and_then(Value::as_str),
            Some("ghostex")
        );
        assert_eq!(
            preview.get("shallowClone").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn preview_repository_clone_matches_typescript_url_path_and_name_edges() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("parent");
        fs::create_dir(&parent).unwrap();
        let parent_with_segments = parent.join("..").join("parent");
        let mut params = Map::new();
        params.insert(
            "repositoryInput".to_string(),
            json!("HTTPS://GitHub.com/factory-ai/ghost%65x.git/blob/main?tab=readme"),
        );
        params.insert(
            "parentPath".to_string(),
            json!(parent_with_segments.to_string_lossy()),
        );
        params.insert(
            "destinationFolderName".to_string(),
            json!("repo//copy: final\\name"),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("cloneUrl").and_then(Value::as_str),
            Some("https://GitHub.com/factory-ai/ghostex.git")
        );
        assert_eq!(
            preview.get("defaultFolderName").and_then(Value::as_str),
            Some("ghostex")
        );
        assert_eq!(
            preview.get("destinationFolderName").and_then(Value::as_str),
            Some("repo-copy- final-name")
        );
        assert_eq!(
            preview.get("parentPath").and_then(Value::as_str),
            Some(parent.to_string_lossy().as_ref())
        );
        assert_eq!(
            preview.get("destinationPath").and_then(Value::as_str),
            Some(
                parent
                    .join("repo-copy- final-name")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    #[test]
    fn repository_clone_input_parsing_matches_typescript_token_edges() {
        let ssh =
            parse_repository_clone_input("ssh://git@GitHub.com/Factory-AI/Ghost%65x.GIT/tree/main")
                .unwrap();
        assert_eq!(ssh.clone_url, "ssh://git@GitHub.com/Factory-AI/Ghostex.git");
        assert_eq!(ssh.repository_name, "Ghostex");

        let scheme_without_dotted_host =
            parse_repository_clone_input("http://internal/repo").unwrap();
        assert_eq!(
            scheme_without_dotted_host.clone_url,
            "https://github.com/http:/internal/repo.git"
        );

        assert!(parse_repository_clone_input("owner//repo").is_none());
    }

    #[test]
    fn preview_rejects_existing_destination() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("repo")).unwrap();
        let mut params = Map::new();
        params.insert("repositoryInput".to_string(), json!("owner/repo"));
        params.insert(
            "parentPath".to_string(),
            json!(dir.path().to_string_lossy()),
        );
        let preview = preview_repository_clone(&params).unwrap();
        assert_eq!(
            preview.get("destinationExists").and_then(Value::as_bool),
            Some(true)
        );
        assert!(preview
            .get("warning")
            .and_then(Value::as_str)
            .unwrap()
            .contains("already exists"));
    }

    #[test]
    fn repository_branch_validation_matches_clone_contract() {
        assert!(is_repository_branch_name_valid("feature/demo"));
        assert!(is_repository_branch_name_valid(&"é".repeat(255)));
        assert!(!is_repository_branch_name_valid(&"\u{1F680}".repeat(128)));
        assert!(!is_repository_branch_name_valid("../bad"));
        assert!(!is_repository_branch_name_valid("bad branch"));
    }

    #[tokio::test]
    async fn clone_process_terminates_when_job_is_canceled() {
        if !Path::new("/bin/sh").exists() {
            return;
        }
        let dir = tempdir().unwrap();
        let started_path = dir.path().join("started");
        let script_path = dir.path().join("fake-git.sh");
        fs::write(
            &script_path,
            r#"trap 'exit 130' TERM
printf 'clone started\n'
: > "$1"
while :; do :; done
"#,
        )
        .unwrap();
        let jobs = Arc::new(Mutex::new(HashMap::new()));
        let job_id = "job-cancel".to_string();
        jobs.lock().await.insert(
            job_id.clone(),
            json!({
                "jobId": job_id,
                "state": "running"
            }),
        );
        let run = tokio::spawn(run_clone_process(
            "/bin/sh",
            vec![
                script_path.to_string_lossy().to_string(),
                started_path.to_string_lossy().to_string(),
            ],
            dir.path().to_string_lossy().to_string(),
            jobs.clone(),
            job_id.clone(),
        ));
        for _ in 0..100 {
            if started_path.exists() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(started_path.exists());
        {
            let mut jobs = jobs.lock().await;
            jobs.get_mut(&job_id)
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert("state".to_string(), json!("canceled"));
        }
        let output = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("canceled clone should exit")
            .expect("join")
            .expect("clone output");
        assert_eq!(output.exit_code, 130);
        assert_eq!(output.stdout, "clone started");
    }
}
