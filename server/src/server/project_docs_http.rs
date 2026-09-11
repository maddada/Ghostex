use super::*;
use std::sync::OnceLock;
use tokio::sync::Semaphore;

/// CDXC:Docs 2026-09-11 WHY:
/// Directory scans and Git processes can block for milliseconds or longer; they must not occupy the server's async request workers.
/// Bound concurrency before spawning so a burst of refreshes cannot exhaust the blocking pool.
pub(super) async fn handle(
    state: Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body_json: Value,
) -> RoutedResponse {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let permit = SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(4)))
        .clone()
        .acquire_owned()
        .await
        .expect("Docs semaphore stays open");
    let task_state = state.clone();
    let task_path = endpoint_path.clone();
    let task_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        handle_domain_http(
            &task_state,
            task_path,
            task_request_id,
            &body_json,
            |repository, _db, params, _| {
                let project_id = read_project_id(params)?;
                let project = repository.get_project(&project_id)?.ok_or_else(|| {
                    DomainStateError::not_found(format!("Project {project_id} does not exist."))
                })?;
                let project_path = project
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| {
                        DomainStateError::bad_request("Project has no filesystem path.")
                    })?;
                /*
                CDXC:Docs 2026-08-09:
                Docs reads the project's own folder plus its configured Docs
                directory (then the Global Default). Resolving here keeps
                `run_project_docs_action` taking a plain root.

                CDXC:Docs 2026-08-09: a bad Docs directory no longer
                fails the request. It comes back as one unavailable mount inside
                the listing, so the project's own docs still show and the panel
                still names the path that could not be opened.
                */
                Ok(project_docs::run_project_docs_action(
                    &project_docs::resolve_project_docs_root(&project, project_path),
                    params,
                ))
            },
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("Docs request failed: {error}"),
            },
        ),
    }
}
