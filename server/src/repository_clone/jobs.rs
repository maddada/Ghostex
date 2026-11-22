use std::{collections::HashMap, fs, sync::Arc};

use serde_json::{json, Map, Value};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::DomainRepository,
    logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel},
    presentation::{build_presentation_project_delta, increment_presentation_revision},
    storage::open_gxserver_database,
};

use super::*;

/*
CDXC:RepoStructure 2026-06-16-00:49:
Phase 7 repository clone jobs remain gxserver-owned background work. Rust keeps the TypeScript preview/start/read/cancel lifecycle, rejects existing destinations before spawning Git, stores jobs in memory for initial parity, and writes only job ids plus booleans to persistent logs so clone URLs, branches, paths, argv, stdout, and stderr stay out of support bundles.

CDXC:AddProject 2026-06-22-09:21:
Repository clone parity depends on matching TypeScript's URL token parsing, destination folder normalization, color-stripped Git environment, and active cancellation semantics. Canceling a running job must terminate the spawned Git process instead of only changing the in-memory job status.

CDXC:AddProject 2026-06-24-19:35:
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
        let mut job = json!({
            "jobId": job_id,
            "message": "Cloning repository.",
            "preview": preview,
            "startedAt": now_iso(),
            "state": "running",
        });
        // The workspace of the window the clone was started from, where the project lands.
        if let Some(workspace_id) = params.get("workspaceId").and_then(Value::as_str) {
            job["workspaceId"] = json!(workspace_id);
        }
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
    let (preview, workspace_id) = {
        let jobs = jobs.lock().await;
        let job = jobs.get(&job_id);
        (
            job.and_then(|job| job.get("preview"))
                .cloned()
                .unwrap_or_else(|| json!({})),
            job.and_then(|job| job.get("workspaceId")).cloned(),
        )
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
    CDXC:AddProject 2026-07-30:
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
                    summarize_clone_failure(
                        &output.stderr.clone().or_else(&output.stdout),
                        output.exit_code,
                    ),
                    Some(output),
                    "dependencyUnavailable",
                )
                .await;
                return;
            }
            mark_job_adding(&jobs, &job_id).await;
            match add_cloned_project(&runtime, &preview, workspace_id.as_ref()) {
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

pub(super) async fn job_is_canceled(
    jobs: &Arc<Mutex<HashMap<String, Value>>>,
    job_id: &str,
) -> bool {
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
    CDXC:StateSync 2026-07-29-00:00:
    A just-cloned repository is the case where the `origin` remote matters most —
    it is by definition a checkout of a repository that exists elsewhere — so its
    remote is probed here, outside the sequencer below, and rides the very delta
    that announces the project. Same first-sighting rule as
    `schedule_presentation_project_delta`: this warms an unprobed path once and
    leaves refreshes to the background pass.
    */
    /*
    CDXC:Git 2026-08-26:
    Both sidebar versions need the origin now: V2 groups by it and the classic
    project menu copies it. Probe before the clone's first presentation delta so
    either surface receives the URL immediately.
    */
    if let Ok(Some(project)) = repository.get_project(project_id) {
        crate::project_git_remote::ensure_project_git_remote_probed(&project, true);
        /*
        CDXC:Icons 2026-07-29 (discovered icons):
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
    workspace_id: Option<&Value>,
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
    if let Some(workspace_id) = workspace_id {
        params.insert("workspaceId".to_string(), workspace_id.clone());
    }
    repository
        .create_project(&params)
        .and_then(|project| {
            crate::workspaces::place_added_project(&repository, &db, project, &params)
        })
        .map_err(|error| RepositoryCloneError::dependency_unavailable(error.to_string()))
}
