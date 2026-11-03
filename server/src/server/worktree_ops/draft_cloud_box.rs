use super::*;

use std::sync::OnceLock;

use ghostex_gx_protocol::cloud_box::{
    cloud_box_machine_id, CloudBox, CloudBoxRemember, CloudBoxState, DraftCloudBox,
    CLOUD_BOX_RUNTIME_SETTINGS_KEY,
};

/*
CDXC:CloudBoxes 2026-11-02 DECISION:
User (Q1027 "third segment", Q1010 "per project", Q1009 "queue with progress", Q1013 "On sending a
message there. Not on opening the session."): Cloud Box is a third choice of the new chat's tray
(This folder | New worktree | Cloud Box), remembered per project like the other two. Picking it
starts and bills nothing: the draft only records `runtimeSettings.cloudBox {pending: true, ...}`
with the picker's choices. The first message starts the box: it is kept as the draft's first
message, `/api/cloudBoxes create` runs with the draft, and the tray follows the box's boot card
until the session exists on the box, then opens it there (`machineId` `cloudbox:<boxId>`) and
closes the draft. A failure gives the message back with the reason, like a worktree move.
SEE-ALSO: server/src/server/worktree_ops/draft_worktree.rs (the move this rides on),
server/src/cloud_boxes/create.rs (the box), packages/gx-chat-core/src/menus/cloud_box.rs (the
segment and the picker), apps/desktop/src/app/native_chat/box_picker.rs,
apps/mobile/app/src/chat/native/composer/BoxPicker.tsx.
*/

/// The tray mode of the Cloud Box segment.
pub(crate) const MODE_CLOUD_BOX: &str = "cloudBox";
/// The shared setting that turns Cloud Boxes on (D11, default on).
const ENABLED_SETTING: &str = "cloudBoxesEnabled";
/// How often a cloud move reads its box while the box starts.
const FOLLOW_EVERY: Duration = Duration::from_millis(800);
/// A box that has not finished its card by then is reported failed to the chat (it keeps
/// running and shows in Settings › Cloud Boxes).
const FOLLOW_LIMIT: Duration = Duration::from_secs(20 * 60);

/// The box each cloud move started, with its latest row, by the draft's `projectId:sessionId`.
fn cloud_moves() -> &'static Mutex<HashMap<String, CloudBox>> {
    static MOVES: OnceLock<Mutex<HashMap<String, CloudBox>>> = OnceLock::new();
    MOVES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn draft_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
}

fn remember_box(project_id: &str, session_id: &str, row: CloudBox) {
    if let Ok(mut moves) = cloud_moves().lock() {
        moves.insert(draft_key(project_id, session_id), row);
    }
}

pub(crate) fn cloud_boxes_enabled(state: &AppState) -> bool {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| settings.get(ENABLED_SETTING).and_then(Value::as_bool))
        .unwrap_or(true)
}

async fn cloud_call(
    state: &AppState,
    params: Value,
) -> std::result::Result<Value, DomainStateError> {
    let Value::Object(params) = params else {
        return Err(DomainStateError::bad_request(
            "Cloud Boxes params must be an object.",
        ));
    };
    crate::cloud_boxes::endpoint::dispatch(Arc::new(state.clone()), &params).await
}

/// The draft's own Cloud Box picks (`runtimeSettings.cloudBox`), when it has any.
fn draft_picks(session: &Value) -> Option<DraftCloudBox> {
    session
        .pointer(&format!(
            "/runtimeSettings/{CLOUD_BOX_RUNTIME_SETTINGS_KEY}"
        ))
        .cloned()
        .and_then(|stored| serde_json::from_value(stored).ok())
}

/// What the tray's Cloud Box segment and its picker need, read on every full tray read:
/// `{enabled, remembered, draft, providers, snapshots, boxes, sizes, spend, agentbox}`. Each part
/// the registry cannot answer is left empty, so the segment still shows and explains.
pub(crate) async fn cloud_box_facts(state: &AppState, project: &Value, session: &Value) -> Value {
    if !cloud_boxes_enabled(state) {
        return json!({ "enabled": false });
    }
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let part = |value: std::result::Result<Value, DomainStateError>, key: &str| {
        value
            .ok()
            .and_then(|value| value.get(key).cloned())
            .unwrap_or(Value::Null)
    };
    let (providers, snapshots, boxes, sizes, spend, settings) = tokio::join!(
        cloud_call(state, json!({ "action": "providers" })),
        cloud_call(
            state,
            json!({ "action": "snapshots", "projectId": project_id })
        ),
        cloud_call(state, json!({ "action": "list", "projectId": project_id })),
        cloud_call(state, json!({ "action": "sizes" })),
        cloud_call(state, json!({ "action": "spend" })),
        cloud_call(
            state,
            json!({ "action": "projectSettings", "projectId": project_id })
        ),
    );
    let settings = part(settings, "settings");
    let remembered = settings.get("remember").and_then(Value::as_str) == Some("cloud");
    json!({
        "enabled": true,
        "remembered": remembered,
        "settings": settings,
        "draft": draft_picks(session),
        "providers": part(providers, "providers"),
        "snapshots": part(snapshots, "snapshots"),
        "boxes": part(boxes, "boxes"),
        "sizes": part(sizes, "sizes"),
        "spend": spend.unwrap_or(Value::Null),
    })
}

/// Writes the project's remembered tray choice: the Cloud Box segment (`cloud`) or one of the
/// other two (`local`). Only a change is written.
pub(crate) async fn remember_project_choice(
    state: &AppState,
    project_id: &str,
    cloud: bool,
) -> std::result::Result<(), DomainStateError> {
    let current = cloud_call(
        state,
        json!({ "action": "projectSettings", "projectId": project_id }),
    )
    .await?;
    let mut settings = current
        .get("settings")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let wanted = if cloud {
        CloudBoxRemember::Cloud
    } else {
        CloudBoxRemember::Local
    };
    let wanted = serde_json::to_value(wanted).unwrap_or(Value::Null);
    if settings.get("remember") == Some(&wanted) {
        return Ok(());
    }
    if let Some(object) = settings.as_object_mut() {
        object.insert("remember".to_string(), wanted);
    }
    cloud_call(
        state,
        json!({ "action": "projectSettings", "projectId": project_id, "settings": settings }),
    )
    .await?;
    Ok(())
}

/// Replaces the draft's `runtimeSettings.cloudBox` (`None` removes it), keeping the rest.
fn write_draft_picks(
    state: &AppState,
    session: &Value,
    picks: Option<&DraftCloudBox>,
) -> std::result::Result<(), DomainStateError> {
    let project_id = value_text(session, "projectId")?;
    let session_id = value_text(session, "sessionId")?;
    let mut runtime_settings = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let current = runtime_settings
        .get(CLOUD_BOX_RUNTIME_SETTINGS_KEY)
        .cloned();
    match picks {
        Some(picks) => {
            let value = serde_json::to_value(picks).unwrap_or(Value::Null);
            if current.as_ref() == Some(&value) {
                return Ok(());
            }
            runtime_settings.insert(CLOUD_BOX_RUNTIME_SETTINGS_KEY.to_string(), value);
        }
        None => {
            if current.is_none() {
                return Ok(());
            }
            runtime_settings.remove(CLOUD_BOX_RUNTIME_SETTINGS_KEY);
        }
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update)?;
    Ok(())
}

fn text_field(value: &Map<String, Value>, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The picker's choices as the draft keeps them, from `remember.cloudBox` (or `start.cloudBox`).
fn picks_from(
    params: Option<&Map<String, Value>>,
    current: Option<DraftCloudBox>,
) -> DraftCloudBox {
    let mut picks = current.unwrap_or_default();
    picks.pending = true;
    let Some(params) = params else {
        return picks;
    };
    if params.contains_key("provider") {
        picks.provider = text_field(params, "provider")
            .map(|provider| ghostex_gx_protocol::cloud_box::CloudProviderId::from_wire(&provider));
    }
    if params.contains_key("snapshotId") {
        picks.snapshot_id = text_field(params, "snapshotId");
    }
    if params.contains_key("size") {
        picks.size = text_field(params, "size");
    }
    if params.contains_key("reuseBoxId") {
        picks.reuse_box_id = text_field(params, "reuseBoxId");
    }
    if let Some(blank) = params.get("blank").and_then(Value::as_bool) {
        picks.blank = blank.then_some(true);
    }
    if let Some(worktree) = params.get("worktree").and_then(Value::as_bool) {
        picks.worktree = worktree.then_some(true);
    }
    picks
}

/// A tray pick: `remember.mode` is the segment; with Cloud Box the draft keeps its picker
/// choices, and leaving the segment drops them.
pub(crate) async fn remember_cloud_choice(
    state: &AppState,
    project: &Value,
    session: &Value,
    remember: &Map<String, Value>,
) -> std::result::Result<bool, DomainStateError> {
    let cloud = remember.get("mode").and_then(Value::as_str) == Some(MODE_CLOUD_BOX);
    let project_id = value_text(project, "projectId")?;
    if cloud_boxes_enabled(state) {
        // An older project without Cloud Box settings is left as it is until the user picks.
        remember_project_choice(state, &project_id, cloud).await?;
    }
    if cloud {
        let picks = picks_from(
            remember.get("cloudBox").and_then(Value::as_object),
            draft_picks(session),
        );
        write_draft_picks(state, session, Some(&picks))?;
    } else {
        write_draft_picks(state, session, None)?;
    }
    Ok(cloud)
}

/// The first message of a Cloud Box draft: kept as the draft's first message, the box created
/// for the draft, and the tray told to follow it. Answers at once; the box starts in the
/// background (Q1009).
pub(crate) async fn start_cloud_box_move(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<(), DomainStateError> {
    let project_id = read_project_id(params)?;
    let session_id = read_session_id(params)?;
    let start = params
        .get("start")
        .and_then(Value::as_object)
        .ok_or_else(|| DomainStateError::bad_request("start must be an object."))?;
    let text = start
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| DomainStateError::bad_request("start.text must be the first message."))?;
    let (_, session) =
        super::draft_worktree::read_draft_for_cloud_box(state, &project_id, &session_id)?;
    if !cloud_boxes_enabled(state) {
        return Err(DomainStateError::bad_request(
            "Cloud Boxes is turned off. Turn it on in Settings › Extensions.",
        ));
    }
    let mut picks = picks_from(
        start.get("cloudBox").and_then(Value::as_object),
        draft_picks(&session),
    );
    // The first message's pictures go onto the box with it (`boot_steps::send_images`).
    picks.image_paths = start
        .get("imagePaths")
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .filter(|path| !path.trim().is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if !super::draft_worktree::begin_cloud_box_move(
        &project_id,
        &session_id,
        "Starting a cloud box…".to_string(),
    ) {
        return Err(DomainStateError {
            code: "invalidState",
            message: "This chat is already starting a cloud box.".to_string(),
        });
    }
    let result = async {
        // The box's own session starts with this message (`prepare-session` reads it from the
        // draft), and a chat opened later sees the draft has sent it.
        let mut runtime = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime.insert("firstUserMessage".to_string(), json!(text));
        runtime.insert(
            CLOUD_BOX_RUNTIME_SETTINGS_KEY.to_string(),
            serde_json::to_value(&picks).unwrap_or(Value::Null),
        );
        let mut marked = session.clone();
        if let Some(object) = marked.as_object_mut() {
            object.insert(
                "runtimeSettings".to_string(),
                Value::Object(runtime.clone()),
            );
        }
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        DomainRepository::new(&db, state.metadata.server_id.as_str()).update_session(&{
            let mut update = Map::new();
            update.insert("projectId".to_string(), json!(project_id));
            update.insert("sessionId".to_string(), json!(session_id));
            update.insert("runtimeSettings".to_string(), Value::Object(runtime));
            update
        })?;
        let mut create = json!({
            "action": "create",
            "projectId": project_id,
            "draft": { "projectId": project_id, "sessionId": session_id },
        });
        if let Some(provider) = &picks.provider {
            create["provider"] = json!(provider);
        }
        if let Some(snapshot) = &picks.snapshot_id {
            create["snapshotId"] = json!(snapshot);
        }
        if let Some(size) = &picks.size {
            create["size"] = json!(size);
        }
        if picks.blank == Some(true) {
            create["blank"] = json!(true);
        }
        if let Some(reuse) = &picks.reuse_box_id {
            create["reuseBoxId"] = json!(reuse);
        }
        let created = cloud_call(state, create).await?;
        let row: CloudBox = serde_json::from_value(created.get("box").cloned().unwrap_or_default())
            .map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("Cloud Boxes answered an unreadable box: {error}"),
            })?;
        let mut marked_picks = picks.clone();
        marked_picks.box_id = Some(row.box_id.clone());
        let _ = write_draft_picks(state, &marked, Some(&marked_picks));
        Ok::<CloudBox, DomainStateError>(row)
    }
    .await;
    let row = match result {
        Ok(row) => row,
        Err(error) => {
            super::draft_worktree::end_cloud_box_move(
                &project_id,
                &session_id,
                DraftMoveStatus::Failed {
                    message: error.message.clone(),
                },
            );
            return Err(error);
        }
    };
    remember_box(&project_id, &session_id, row.clone());
    let state = state.clone();
    tokio::spawn(async move {
        let status = follow_box(&state, &project_id, &session_id, &row.box_id).await;
        let done = matches!(status, DraftMoveStatus::Done { .. });
        super::draft_worktree::end_cloud_box_move(&project_id, &session_id, status);
        if done {
            super::draft_worktree::close_cloud_box_draft_later(&state, &project_id, &session_id)
                .await;
        }
    });
    Ok(())
}

/// Reads the box until its card finishes: the box session's ids when it exists, else the reason.
async fn follow_box(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    box_id: &str,
) -> DraftMoveStatus {
    let started = Instant::now();
    loop {
        tokio::time::sleep(FOLLOW_EVERY).await;
        let row = match cloud_call(state, json!({ "action": "get", "boxId": box_id })).await {
            Ok(value) => {
                serde_json::from_value::<CloudBox>(value.get("box").cloned().unwrap_or_default())
                    .ok()
            }
            Err(_) => None,
        };
        if let Some(row) = row {
            remember_box(project_id, session_id, row.clone());
            let progress = row.progress.as_ref();
            if let Some(error) = progress.and_then(|progress| progress.error.clone()) {
                return DraftMoveStatus::Failed { message: error };
            }
            if row.state == CloudBoxState::Failed {
                return DraftMoveStatus::Failed {
                    message: row
                        .state_detail
                        .clone()
                        .unwrap_or_else(|| format!("{} could not start.", row.name)),
                };
            }
            let finished = progress.is_none_or(|progress| progress.finished_at.is_some());
            if finished {
                return match row
                    .sessions
                    .iter()
                    .find(|session| session.home_session_id.as_deref() == Some(session_id))
                {
                    Some(box_session) => DraftMoveStatus::Done {
                        project_id: box_session.project_id.clone(),
                        session_id: box_session.session_id.clone(),
                    },
                    None => DraftMoveStatus::Failed {
                        message: format!(
                            "{} started, but the chat did not start on it. Try again.",
                            row.name
                        ),
                    },
                };
            }
        }
        if started.elapsed() > FOLLOW_LIMIT {
            return DraftMoveStatus::Failed {
                message:
                    "The cloud box took too long to start. It is listed in Settings › Cloud Boxes."
                        .to_string(),
            };
        }
    }
}

/// What a cloud move adds to the tray's `move`: the box row (its boot card, Q1009) and, once
/// done, the box's machine id the session opens on.
pub(crate) fn decorate_move(project_id: &str, session_id: &str, mut value: Value) -> Value {
    let Some(row) = cloud_moves()
        .lock()
        .ok()
        .and_then(|moves| moves.get(&draft_key(project_id, session_id)).cloned())
    else {
        return value;
    };
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "machineId".to_string(),
            json!(cloud_box_machine_id(&row.box_id)),
        );
        object.insert(
            "cloudBox".to_string(),
            serde_json::to_value(&row).unwrap_or(Value::Null),
        );
    }
    value
}

/// Forgets a draft's cloud move once the draft is closed.
pub(crate) fn forget_cloud_move(project_id: &str, session_id: &str) {
    if let Ok(mut moves) = cloud_moves().lock() {
        moves.remove(&draft_key(project_id, session_id));
    }
}
