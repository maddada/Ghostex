use super::*;

use std::sync::OnceLock;

/*
CDXC:Worktrees 2026-10-31 DECISION:
`/api/draftWorktree` backs the worktree tray on top of a new chat's message box (Brewed Bytes on
Discord; the user's picks Q828 to Q832): This folder, New worktree from a base branch, or an
existing worktree, remembered per project. The worktree is made when the first message is sent,
never when it is picked, and it is a real worktree PROJECT (the one `createProjectWorktree`
registers) so both sidebars show the chat inside it. A failure keeps the message: nothing is sent
and whatever this request made is removed again.

The draft's own terminal does not survive the move. A session row belongs to one project and is
known everywhere by `projectId:sessionId` (twenty tables, every client's tabs, subscriptions and
drafts), so it cannot change project; the agent CLI's folder cannot change under it either. So the
move starts the same agent in the worktree project as a new session, with the draft's launch
rebuilt there the way the Run on row rebuilds it (`rebuild_draft_for_location`), sends the first
message through the ordinary chat send once its input box is up (images included), and closes the
empty draft once the client has opened the new session (`finish`), or after a grace period when
no client does. A draft has no conversation, so nothing is lost; the message travels with the
request.

The call answers at once and the work runs in the background, because a project's worktree setup
command can outlast the chat's 60-second request timeout: clients read the progress back with
`light: true` until it is `done` or `failed`.
SEE-ALSO: server/src/worktree_branch_name.rs (the name), packages/gx-chat-core/src/menus/draft_worktree.rs
(the tray), apps/desktop/src/app/native_chat/worktree_tray.rs, apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx.
*/

/// The project runtime setting that remembers the tray's last choice.
pub(crate) const DRAFT_WORKTREE_MEMORY_KEY: &str = "newChatWorktree";
/// The `/api/transitionSession` reason of the draft's close.
const DRAFT_MOVED_CLOSE_REASON: &str = "movedToWorktree";
/// How long a finished move waits for its client's `finish` before closing the draft itself.
const DRAFT_CLOSE_GRACE: Duration = Duration::from_secs(60);
/// How long a finished or failed move stays readable.
const MOVE_RESULT_TTL: Duration = Duration::from_secs(600);

/// Where a new chat runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftWorktreeMode {
    Folder,
    NewWorktree,
    ExistingWorktree,
}

impl DraftWorktreeMode {
    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "folder" => Some(Self::Folder),
            "newWorktree" => Some(Self::NewWorktree),
            "existingWorktree" => Some(Self::ExistingWorktree),
            _ => None,
        }
    }

    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::NewWorktree => "newWorktree",
            Self::ExistingWorktree => "existingWorktree",
        }
    }
}

/// The remembered choice of a project: its mode and, for a new worktree, its base branch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DraftWorktreeMemory {
    pub(crate) mode: Option<DraftWorktreeMode>,
    pub(crate) base_branch: Option<String>,
}

impl DraftWorktreeMemory {
    pub(crate) fn read(project: &Value) -> Self {
        let stored = project
            .pointer(&format!("/runtimeSettings/{DRAFT_WORKTREE_MEMORY_KEY}"))
            .cloned()
            .unwrap_or(Value::Null);
        Self {
            mode: stored
                .get("mode")
                .and_then(Value::as_str)
                .and_then(DraftWorktreeMode::from_wire),
            base_branch: stored
                .get("baseBranch")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|branch| !branch.is_empty())
                .map(str::to_string),
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "mode": self.mode.map(DraftWorktreeMode::as_wire),
            "baseBranch": self.base_branch,
        })
    }

    /// `project` with this choice remembered in its runtime settings, the rest kept.
    pub(crate) fn written_into(&self, project: &Value) -> Map<String, Value> {
        let mut runtime_settings = project
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.insert(DRAFT_WORKTREE_MEMORY_KEY.to_string(), self.to_value());
        runtime_settings
    }
}

/// How a move is going, as `light` reads report it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DraftMoveStatus {
    Creating {
        label: String,
    },
    Failed {
        message: String,
    },
    Done {
        project_id: String,
        session_id: String,
    },
}

impl DraftMoveStatus {
    fn to_value(&self) -> Value {
        match self {
            Self::Creating { label } => json!({ "status": "creating", "label": label }),
            Self::Failed { message } => json!({ "status": "failed", "message": message }),
            Self::Done {
                project_id,
                session_id,
            } => json!({ "status": "done", "projectId": project_id, "sessionId": session_id }),
        }
    }
}

struct DraftMove {
    status: DraftMoveStatus,
    updated: Instant,
}

/// Every move this gxserver ran recently, by the draft's `projectId:sessionId`.
fn moves() -> &'static Mutex<HashMap<String, DraftMove>> {
    static MOVES: OnceLock<Mutex<HashMap<String, DraftMove>>> = OnceLock::new();
    MOVES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn move_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
}

fn read_move(project_id: &str, session_id: &str) -> Option<DraftMoveStatus> {
    let mut moves = moves().lock().ok()?;
    moves.retain(|_, entry| {
        matches!(entry.status, DraftMoveStatus::Creating { .. })
            || entry.updated.elapsed() < MOVE_RESULT_TTL
    });
    moves
        .get(&move_key(project_id, session_id))
        .map(|entry| entry.status.clone())
}

fn write_move(project_id: &str, session_id: &str, status: DraftMoveStatus) {
    if let Ok(mut moves) = moves().lock() {
        moves.insert(
            move_key(project_id, session_id),
            DraftMove {
                status,
                updated: Instant::now(),
            },
        );
    }
}

/// Claims the draft for one move; `false` while another is creating.
fn begin_move(project_id: &str, session_id: &str, label: String) -> bool {
    let Ok(mut moves) = moves().lock() else {
        return false;
    };
    let key = move_key(project_id, session_id);
    if matches!(
        moves.get(&key).map(|entry| &entry.status),
        Some(DraftMoveStatus::Creating { .. })
    ) {
        return false;
    }
    moves.insert(
        key,
        DraftMove {
            status: DraftMoveStatus::Creating { label },
            updated: Instant::now(),
        },
    );
    true
}

/// What a start asks for.
#[derive(Clone, Debug)]
pub(crate) struct DraftMoveRequest {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) mode: DraftWorktreeMode,
    pub(crate) base_branch: Option<String>,
    pub(crate) worktree_key: Option<String>,
    pub(crate) text: String,
    pub(crate) image_paths: Vec<String>,
}

fn text_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

impl DraftMoveRequest {
    pub(crate) fn read(params: &Map<String, Value>) -> std::result::Result<Self, DomainStateError> {
        let start = params
            .get("start")
            .and_then(Value::as_object)
            .ok_or_else(|| DomainStateError::bad_request("start must be an object."))?;
        let mode = start
            .get("mode")
            .and_then(Value::as_str)
            .and_then(DraftWorktreeMode::from_wire)
            .filter(|mode| *mode != DraftWorktreeMode::Folder)
            .ok_or_else(|| {
                DomainStateError::bad_request("start.mode must be newWorktree or existingWorktree.")
            })?;
        let text = start
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| {
                DomainStateError::bad_request("start.text must be the first message.")
            })?;
        let base_branch = match start.get("baseBranch") {
            None | Some(Value::Null) => None,
            Some(value) if value.as_str().map(str::trim) == Some("") => None,
            Some(value) => Some(normalize_project_worktree_git_ref(
                Some(value),
                "baseBranch",
            )?),
        };
        let worktree_key = text_param(start, "worktreeKey");
        if mode == DraftWorktreeMode::ExistingWorktree && worktree_key.is_none() {
            return Err(DomainStateError::bad_request(
                "Choose an existing worktree before sending.",
            ));
        }
        let image_paths = start
            .get("imagePaths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            project_id: read_project_id(params)?,
            session_id: read_session_id(params)?,
            mode,
            base_branch,
            worktree_key,
            text,
            image_paths,
        })
    }
}

fn open_repository(
    state: &AppState,
) -> std::result::Result<crate::storage::GxserverDatabase, DomainStateError> {
    open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })
}

/// The draft and its project, or why the tray does not apply.
fn read_draft(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> std::result::Result<(Value, Value), DomainStateError> {
    let db = open_repository(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository.get_project(project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            ))
        })?;
    Ok((project, session))
}

/// Whether the tray applies to this session at all, before git is asked.
fn tray_applies(project: &Value, session: &Value) -> bool {
    crate::agents::session_is_draft(session)
        && session.get("kind").and_then(Value::as_str) == Some("agent")
        && crate::agentbox::pending_session_agentbox(session).is_none()
        && crate::agentbox::session_agentbox(session).is_none()
        && project
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| !path.trim().is_empty())
        && project.get("visibility").and_then(Value::as_str) != Some("hidden")
        && project.get("systemKind").is_none_or(Value::is_null)
}

fn lifecycle_params(project_id: &str, session_id: &str) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params
}

/// The branch the project's own checkout is on, `None` when detached or unreadable.
async fn source_branch(context: &ProjectWorktreeOperationContext) -> Option<String> {
    let branch = run_project_git_action(
        &context.projects,
        "branch",
        &context.source_path,
        Map::new(),
    )
    .await
    .ok()?;
    if exit_code(&branch) != 0 {
        return None;
    }
    branch
        .get("stdout")
        .and_then(Value::as_str)
        .and_then(normalize_branch_name)
}

/// `/api/draftWorktree {projectId, sessionId, light?, remember?, start?, finish?}`.
///
/// The answer is `{available, currentBranch, branches, worktrees, remembered, move}`; a `light`
/// read (the client's poll while a move runs) carries only `move`.
pub(crate) async fn draft_worktree(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let project_id = read_project_id(params)?;
    let session_id = read_session_id(params)?;
    if params.get("finish").and_then(Value::as_bool) == Some(true) {
        // `moved`: the host started this draft's replacement in another project or machine, which
        // no move on this gxserver knows about.
        let moved = params.get("moved").and_then(Value::as_bool) == Some(true);
        let closed = finish_draft_move(state, &project_id, &session_id, moved);
        if closed {
            super::draft_cloud_box::forget_cloud_move(&project_id, &session_id);
        }
        return Ok(json!({ "closed": closed }));
    }
    let starts_cloud_box = params
        .get("start")
        .and_then(|start| start.get("mode"))
        .and_then(Value::as_str)
        == Some(super::draft_cloud_box::MODE_CLOUD_BOX);
    if starts_cloud_box {
        super::draft_cloud_box::start_cloud_box_move(state, params).await?;
        return Ok(json!({ "move": move_value(&project_id, &session_id) }));
    }
    if params.get("start").is_some_and(Value::is_object) {
        let request = DraftMoveRequest::read(params)?;
        start_draft_move(state, request)?;
        return Ok(json!({ "move": move_value(&project_id, &session_id) }));
    }
    if params.get("light").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({ "move": move_value(&project_id, &session_id) }));
    }
    let (project, session) = read_draft(state, &project_id, &session_id)?;
    if let Some(remember) = params.get("remember").and_then(Value::as_object) {
        let cloud =
            super::draft_cloud_box::remember_cloud_choice(state, &project, &session, remember)
                .await?;
        if !cloud {
            // Read again: the Cloud Box memory was just written into the same runtime settings.
            let (project, _) = read_draft(state, &project_id, &session_id)?;
            remember_choice(state, &project, remember)?;
        }
    }
    let (project, session) = match params.get("remember") {
        Some(_) => read_draft(state, &project_id, &session_id)?,
        None => (project, session),
    };
    read_tray(state, &project, &session).await
}

fn move_value(project_id: &str, session_id: &str) -> Value {
    read_move(project_id, session_id)
        .map(|status| {
            super::draft_cloud_box::decorate_move(project_id, session_id, status.to_value())
        })
        .unwrap_or(Value::Null)
}

/// The draft and its project for a Cloud Box start, refused when the tray no longer applies.
pub(crate) fn read_draft_for_cloud_box(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> std::result::Result<(Value, Value), DomainStateError> {
    let (project, session) = read_draft(state, project_id, session_id)?;
    if !tray_applies(&project, &session) {
        return Err(DomainStateError {
            code: "invalidState",
            message: "This chat has already started, so it can no longer start a cloud box."
                .to_string(),
        });
    }
    Ok((project, session))
}

/// Claims the draft for a Cloud Box start (`false` while another move runs).
pub(crate) fn begin_cloud_box_move(project_id: &str, session_id: &str, label: String) -> bool {
    begin_move(project_id, session_id, label)
}

/// Records how a Cloud Box start ended.
pub(crate) fn end_cloud_box_move(project_id: &str, session_id: &str, status: DraftMoveStatus) {
    write_move(project_id, session_id, status);
}

/// Closes a Cloud Box draft whose session started on its box once the grace period passes, in
/// case no client finishes it.
pub(crate) async fn close_cloud_box_draft_later(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    tokio::time::sleep(DRAFT_CLOSE_GRACE).await;
    let (state, project_id, session_id) = (
        state.clone(),
        project_id.to_string(),
        session_id.to_string(),
    );
    let _ = tokio::task::spawn_blocking(move || {
        if finish_draft_move(&state, &project_id, &session_id, false) {
            super::draft_cloud_box::forget_cloud_move(&project_id, &session_id);
        }
    })
    .await;
}

fn remember_choice(
    state: &AppState,
    project: &Value,
    remember: &Map<String, Value>,
) -> std::result::Result<(), DomainStateError> {
    let mode = remember
        .get("mode")
        .and_then(Value::as_str)
        .and_then(DraftWorktreeMode::from_wire)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "remember.mode must be folder, newWorktree or existingWorktree.",
            )
        })?;
    let mut memory = DraftWorktreeMemory::read(project);
    memory.mode = Some(mode);
    if let Some(branch) = text_param(remember, "baseBranch") {
        memory.base_branch = Some(normalize_project_worktree_git_ref(
            Some(&Value::String(branch)),
            "remember.baseBranch",
        )?);
    }
    let project_id = value_text(project, "projectId")?;
    let db = open_repository(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(memory.written_into(project)),
    );
    repository.update_project(&update)?;
    Ok(())
}

async fn read_tray(
    state: &AppState,
    project: &Value,
    session: &Value,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let project_id = value_text(project, "projectId")?;
    let session_id = value_text(session, "sessionId")?;
    let memory = DraftWorktreeMemory::read(project);
    let project_name = project
        .get("title")
        .or_else(|| project.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    // CDXC:Worktrees 2026-10-31 DECISION: User (Q847): the tray also picks the project and machine. A draft in a folder that is not a git checkout has no worktree choices but can still move, so the answer says `movable` apart from `available`, and the tray shows the project chip alone there.
    let unavailable = |movable: bool| {
        json!({
            "available": false,
            "movable": movable,
            "projectName": project_name,
            "remembered": memory.to_value(),
            "move": move_value(&project_id, &session_id),
        })
    };
    if !tray_applies(project, session) {
        return Ok(unavailable(false));
    }
    let cloud_box = super::draft_cloud_box::cloud_box_facts(state, project, session).await;
    let mut context_params = Map::new();
    context_params.insert("projectId".to_string(), json!(project_id));
    // A folder that is not a git checkout lists no worktrees and no branches: no tray.
    let Ok(context) = resolve_project_worktree_operation_context(state, &context_params) else {
        return Ok(unavailable(true));
    };
    let (Ok(options), Ok(branches)) = (
        project_worktree_options(&context).await,
        project_worktree_base_branches(&context).await,
    ) else {
        return Ok(unavailable(true));
    };
    let current_branch = source_branch(&context).await;
    let worktrees: Vec<Value> = options
        .into_iter()
        .filter(|option| !option.is_current_project)
        .map(|option| {
            json!({
                "name": option.name,
                "branch": option.branch,
                "worktreeKey": option.worktree_key,
            })
        })
        .collect();
    Ok(json!({
        "available": true,
        "movable": true,
        "projectName": project_name,
        "currentBranch": current_branch,
        "branches": branches,
        "worktrees": worktrees,
        "remembered": memory.to_value(),
        "move": move_value(&project_id, &session_id),
        "cloudBox": cloud_box,
    }))
}

fn start_draft_move(
    state: &AppState,
    request: DraftMoveRequest,
) -> std::result::Result<(), DomainStateError> {
    let (project, session) = read_draft(state, &request.project_id, &request.session_id)?;
    if !tray_applies(&project, &session) {
        return Err(DomainStateError {
            code: "invalidState",
            message: "This chat has already started, so it can no longer move to a worktree."
                .to_string(),
        });
    }
    let label = match request.mode {
        DraftWorktreeMode::ExistingWorktree => "Opening the worktree…".to_string(),
        _ => match request.base_branch.as_deref() {
            Some(base) => format!("Creating a worktree from {base}…"),
            None => "Creating a worktree…".to_string(),
        },
    };
    if !begin_move(&request.project_id, &request.session_id, label) {
        return Err(DomainStateError {
            code: "invalidState",
            message: "This chat is already moving to a worktree.".to_string(),
        });
    }
    let state = state.clone();
    tokio::spawn(async move {
        let (project_id, session_id) = (request.project_id.clone(), request.session_id.clone());
        let status = match run_draft_move(&state, &request, &session).await {
            Ok((new_project_id, new_session_id)) => DraftMoveStatus::Done {
                project_id: new_project_id,
                session_id: new_session_id,
            },
            Err(message) => {
                log_worktree_session_failure(
                    &state,
                    "draftWorktreeMoveFailed",
                    &project_id,
                    &session_id,
                    &message,
                );
                DraftMoveStatus::Failed { message }
            }
        };
        let done = matches!(status, DraftMoveStatus::Done { .. });
        write_move(&project_id, &session_id, status);
        if done {
            // No client may be watching (a CLI call, a closed window): close the draft anyway.
            tokio::time::sleep(DRAFT_CLOSE_GRACE).await;
            let state = state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                finish_draft_move(&state, &project_id, &session_id, false)
            })
            .await;
        }
    });
    Ok(())
}

/// The message a failure shows: one line, never a path.
pub(crate) fn user_message(error: ProjectWorktreeOperationError) -> String {
    let message = match error {
        ProjectWorktreeOperationError::Domain(error) => error.message,
        ProjectWorktreeOperationError::ProjectPath(_) => {
            "This project's folder could not be used for a worktree.".to_string()
        }
        ProjectWorktreeOperationError::Typed(error) => error.message,
    };
    let line = message
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if line.is_empty() {
        "Could not create the worktree.".to_string()
    } else {
        line
    }
}

/// What a move made, so a failure can take it back.
#[derive(Default)]
pub(crate) struct MadeForMove {
    /// The checkout and branch `git worktree add` made (a new worktree only).
    pub(crate) checkout: Option<PreparedWorktreeCheckout>,
    /// The worktree project this move registered (not one that was registered before).
    pub(crate) registered_project_id: Option<String>,
}

async fn run_draft_move(
    state: &AppState,
    request: &DraftMoveRequest,
    draft: &Value,
) -> std::result::Result<(String, String), String> {
    let mut context_params = Map::new();
    context_params.insert("projectId".to_string(), json!(request.project_id));
    let context =
        resolve_project_worktree_operation_context(state, &context_params).map_err(user_message)?;
    let mut made = MadeForMove::default();
    let result = async {
        let worktree_project =
            prepare_worktree_for_move(state, &context, request, &mut made).await?;
        start_moved_session(state, &worktree_project, request, draft).await
    }
    .await;
    match result {
        Ok(created) => Ok(created),
        Err(error) => {
            rollback_draft_move(state, &context, made).await;
            Err(user_message(error))
        }
    }
}

/// The worktree project the draft moves into: a new one, or an existing worktree's. What it made
/// is recorded in `made` as it goes, so [`rollback_draft_move`] can take it back.
pub(crate) async fn prepare_worktree_for_move(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &DraftMoveRequest,
    made: &mut MadeForMove,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    match request.mode {
        DraftWorktreeMode::ExistingWorktree => {
            open_existing_for_move(state, context, request, made).await
        }
        _ => create_new_for_move(state, context, request, made).await,
    }
}

/// A new branch and checkout off the base branch, registered as a worktree project, its setup
/// command run: what Add Worktree's Create New does, named by the first message.
async fn create_new_for_move(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &DraftMoveRequest,
    made: &mut MadeForMove,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let base_ref = match request.base_branch.clone() {
        Some(base) => base,
        None => source_branch(context).await.ok_or_else(|| {
            DomainStateError::bad_request("Choose a base branch for the worktree.")
        })?,
    };
    let hint = crate::worktree_branch_name::worktree_name_hint(
        &request.text,
        &crate::worktree_branch_name::ReferenceScope {
            project_id: &context.source_project_id,
            cwd: &context.source_path,
        },
    );
    let hint = normalize_project_slug_label(&hint, "nameHint")?;
    let target = resolve_unique_project_worktree_target(context, &hint).await?;
    let mut create_params = Map::new();
    create_params.insert("baseRef".to_string(), Value::String(base_ref));
    create_params.insert("branch".to_string(), Value::String(target.branch.clone()));
    create_params.insert(
        "worktreePath".to_string(),
        Value::String(target.path.clone()),
    );
    // From here on the branch and the folder are this move's: a failed add may leave either.
    made.checkout = Some(PreparedWorktreeCheckout {
        branch: target.branch.clone(),
        created: true,
        path: target.path.clone(),
    });
    let create = run_project_worktree_action(
        &context.projects,
        "create",
        &context.source_path,
        create_params,
    )
    .await?;
    if exit_code(&create) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&create, "git worktree add failed."),
            scope_rejection: false,
        }
        .into());
    }
    let parent_name = context
        .parent_project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Project");
    let project = register_project_worktree_path(
        state,
        &target.path,
        &format!("{parent_name}-{}", target.name),
        "projectAdded",
    )?;
    made.registered_project_id = Some(value_text(&project, "projectId")?);
    prepare_registered_worktree_project(state, &project, Some(&context.source_project_id)).await?;
    Ok(project)
}

/// An existing worktree of the project, picked by its key from the latest list: registered as a
/// worktree project when it is not one yet, no branch made and no setup command run (Add
/// Worktree's Open Existing).
async fn open_existing_for_move(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &DraftMoveRequest,
    made: &mut MadeForMove,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let key = request.worktree_key.clone().unwrap_or_default();
    let selected = project_worktree_options(context)
        .await?
        .into_iter()
        .find(|option| option.worktree_key == key)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "That worktree is no longer in this project's worktree list.",
            )
        })?;
    let project =
        register_project_worktree_path(state, &selected.path, &selected.name, "projectAdded")?;
    if !selected.is_registered {
        made.registered_project_id = Some(value_text(&project, "projectId")?);
    }
    prepare_registered_worktree_project(state, &project, None).await?;
    Ok(project)
}

/// The draft's agent started in the worktree project, then the first message sent to it.
async fn start_moved_session(
    state: &AppState,
    worktree_project: &Value,
    request: &DraftMoveRequest,
    draft: &Value,
) -> std::result::Result<(String, String), ProjectWorktreeOperationError> {
    let blocking_state = state.clone();
    let project = worktree_project.clone();
    let draft = draft.clone();
    let text = request.text.clone();
    let created = tokio::task::spawn_blocking(move || {
        create_and_start_moved_session(&blocking_state, &project, &draft, &text)
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Worktree session task failed: {error}"),
    })??;
    let (project_id, session_id) = created.clone();
    let (state, text, images) = (
        state.clone(),
        request.text.clone(),
        request.image_paths.clone(),
    );
    tokio::spawn(async move {
        crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
            &state.paths,
            state.metadata.server_id.as_str(),
            &project_id,
            &session_id,
            crate::session_chat_composer::SessionChatComposerWaitPolicy {
                settle_ms: 0,
                timeout_ms: crate::server::GXSERVER_PROVIDER_COMPOSER_WAIT_TIMEOUT_MS,
                unknown_hold_ms: GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_READY_DELAY_MS,
            },
        )
        .await;
        if let Err(error) = crate::session_chat_queue_runtime::send_session_chat_message_internal(
            &state,
            &project_id,
            &session_id,
            &text,
            &images,
            crate::session_chat_queue_runtime::SessionChatMessageSource::Composer,
        )
        .await
        {
            log_worktree_session_failure(
                &state,
                "draftWorktreeFirstMessageFailed",
                &project_id,
                &session_id,
                &error.message,
            );
        }
    });
    Ok(created)
}

/// The ordinary create path (the same `create_session`, identity pass and provider start
/// `createWorktreeSession` uses) for the draft's launch rebuilt in the worktree project.
fn create_and_start_moved_session(
    state: &AppState,
    worktree_project: &Value,
    draft: &Value,
    text: &str,
) -> std::result::Result<(String, String), ProjectWorktreeOperationError> {
    let db = open_repository(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project_id = value_text(worktree_project, "projectId")?;
    let mut moved = draft.clone();
    if let Some(row) = moved.as_object_mut() {
        row.insert("projectId".to_string(), json!(project_id));
        row.insert(
            "cwd".to_string(),
            worktree_project.get("path").cloned().unwrap_or(Value::Null),
        );
    }
    let mut create_params =
        crate::agents::rebuild_draft_for_location(&db, worktree_project, &moved, None, Some(text))?;
    create_params.insert("projectId".to_string(), json!(project_id));
    create_params.insert(
        "cwd".to_string(),
        worktree_project.get("path").cloned().unwrap_or(Value::Null),
    );
    create_params.insert("surface".to_string(), json!("workspace"));
    let created = repository.create_session(&create_params, false)?;
    let identity =
        apply_created_session_identity(&repository, &created, &create_params).and_then(|session| {
            Ok((
                value_text(&session, "projectId")?,
                value_text(&session, "sessionId")?,
            ))
        });
    let (project_id, session_id) = match identity {
        Ok(identity) => identity,
        Err(error) => {
            remove_created_worktree_session_row(&repository, &created);
            return Err(error.into());
        }
    };
    let agent_settings = read_agent_settings(&db)?;
    let zmx_context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    let lifecycle = lifecycle_params(&project_id, &session_id);
    if let Err(error) = dispatch_zmx_lifecycle_endpoint(
        &repository,
        "/api/startSessionProvider",
        &lifecycle,
        &zmx_context,
        &agent_settings,
    ) {
        let _ = dispatch_zmx_lifecycle_endpoint(
            &repository,
            "/api/killSession",
            &lifecycle,
            &zmx_context,
            &agent_settings,
        );
        let _ = repository.remove_session(&lifecycle);
        return Err(worktree_session_zmx_error(error).into());
    }
    let _ = schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id);
    Ok((project_id, session_id))
}

/// Takes back what a failed move made: the project it registered, then the checkout and the
/// branch it created. Best effort, like `rollback_worktree_session_checkout`: the move has already
/// failed, so a cleanup failure must not replace its reason.
pub(crate) async fn rollback_draft_move(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    made: MadeForMove,
) {
    if let Some(project_id) = made.registered_project_id.as_deref() {
        if remove_worktree_project_row(state, project_id).is_ok() {
            let _ = schedule_deleted_worktree_project_delta(state, project_id);
        }
    }
    let Some(checkout) = made.checkout else {
        return;
    };
    let mut remove_params = Map::new();
    remove_params.insert(
        "worktreePath".to_string(),
        Value::String(checkout.path.clone()),
    );
    remove_params.insert("force".to_string(), Value::Bool(true));
    let _ = run_project_worktree_action(
        &context.projects,
        "remove",
        &context.parent_path,
        remove_params,
    )
    .await;
    let _ =
        run_project_worktree_action(&context.projects, "prune", &context.parent_path, Map::new())
            .await;
    // The branch is this move's: `resolve_unique_project_worktree_target` picked a name no branch had.
    let mut branch_params = Map::new();
    branch_params.insert("branch".to_string(), Value::String(checkout.branch.clone()));
    let _ = run_project_git_action(
        &context.projects,
        "deleteLocalBranchForce",
        &context.parent_path,
        branch_params,
    )
    .await;
}

/// Closes the draft a finished move left behind; `false` when there is none to close.
fn finish_draft_move(state: &AppState, project_id: &str, session_id: &str, moved: bool) -> bool {
    if !moved
        && !matches!(
            read_move(project_id, session_id),
            Some(DraftMoveStatus::Done { .. })
        )
    {
        return false;
    }
    let Ok((_, session)) = read_draft(state, project_id, session_id) else {
        return false;
    };
    // A draft someone prompted after all (typed in its terminal) is theirs: leave it.
    if !crate::agents::session_is_draft(&session) {
        return false;
    }
    let mut params = lifecycle_params(project_id, session_id);
    params.insert("action".into(), json!("close"));
    params.insert("reason".into(), json!(DRAFT_MOVED_CLOSE_REASON));
    let response = dispatch_zmx_lifecycle_http_blocking(
        state,
        "/api/transitionSession".to_string(),
        "draft-worktree-move".to_string(),
        params,
    );
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let _ =
            schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
    }
    response.response.status().is_success()
}

pub(crate) async fn handle_draft_worktree_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match draft_worktree(state, &params).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => project_worktree_operation_error_response(endpoint_path, request_id, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_is_remembered_in_the_projects_runtime_settings() {
        let project = json!({
            "projectId": "P1",
            "runtimeSettings": { "other": 1 },
        });
        assert_eq!(
            DraftWorktreeMemory::read(&project),
            DraftWorktreeMemory::default()
        );
        let memory = DraftWorktreeMemory {
            mode: Some(DraftWorktreeMode::NewWorktree),
            base_branch: Some("develop".to_string()),
        };
        let runtime = memory.written_into(&project);
        assert_eq!(runtime.get("other"), Some(&json!(1)));
        let stored = json!({ "projectId": "P1", "runtimeSettings": runtime });
        assert_eq!(DraftWorktreeMemory::read(&stored), memory);
        // Back to This folder keeps the base branch for next time.
        let folder = DraftWorktreeMemory {
            mode: Some(DraftWorktreeMode::Folder),
            ..memory.clone()
        };
        let stored = json!({ "runtimeSettings": folder.written_into(&stored) });
        assert_eq!(
            DraftWorktreeMemory::read(&stored).mode,
            Some(DraftWorktreeMode::Folder)
        );
        assert_eq!(
            DraftWorktreeMemory::read(&stored).base_branch.as_deref(),
            Some("develop")
        );
    }

    #[test]
    fn a_start_needs_a_message_and_an_existing_worktree_needs_its_key() {
        let base = |start: Value| {
            let mut params = Map::new();
            params.insert("projectId".into(), json!("P3lv0"));
            params.insert("sessionId".into(), json!("G12k8"));
            params.insert("start".into(), start);
            DraftMoveRequest::read(&params)
        };
        assert!(base(json!({ "mode": "newWorktree", "text": "  " })).is_err());
        assert!(base(json!({ "mode": "folder", "text": "hi" })).is_err());
        assert!(base(json!({ "mode": "existingWorktree", "text": "hi" })).is_err());
        let request = base(json!({
            "mode": "existingWorktree",
            "text": "hi",
            "worktreeKey": "k1",
            "imagePaths": ["/tmp/a.png"],
        }))
        .expect("existing worktree start");
        assert_eq!(request.mode, DraftWorktreeMode::ExistingWorktree);
        assert_eq!(request.worktree_key.as_deref(), Some("k1"));
        assert_eq!(request.image_paths, vec!["/tmp/a.png".to_string()]);
        let request = base(json!({ "mode": "newWorktree", "text": "hi", "baseBranch": "main" }))
            .expect("new worktree start");
        assert_eq!(request.base_branch.as_deref(), Some("main"));
    }

    #[test]
    fn one_move_at_a_time_and_its_result_stays_readable() {
        let (project, session) = ("Ptest-move", "Stest-move");
        assert!(begin_move(project, session, "Creating…".to_string()));
        assert!(!begin_move(project, session, "Creating…".to_string()));
        write_move(
            project,
            session,
            DraftMoveStatus::Failed {
                message: "nope".to_string(),
            },
        );
        assert_eq!(
            move_value(project, session),
            json!({ "status": "failed", "message": "nope" })
        );
        // A failed move can be tried again.
        assert!(begin_move(project, session, "Creating…".to_string()));
        write_move(
            project,
            session,
            DraftMoveStatus::Done {
                project_id: "P2".to_string(),
                session_id: "S2".to_string(),
            },
        );
        assert_eq!(
            move_value(project, session),
            json!({ "status": "done", "projectId": "P2", "sessionId": "S2" })
        );
    }
}
