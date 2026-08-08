use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};

use crate::agents::{
    apply_created_session_identity, create_agent_session_params_for_project, read_agent_settings,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::list_previous_sessions;
use crate::sidebar_hud::sidebar_agent_buttons_from_projects;

/*
CDXC:BoardStartWork 2026-08-07:
`ghostex board start-work <bead-id>` is the daemon-owned dispatch for a Project
Board card: resolve the bead's board project, reuse any usable linked
conversation, otherwise create the visible agent session with the canonical
bead work prompt staged as the first user message and persist the conversation
link. The endpoint is the dispatch — external orchestrators call it INSTEAD of
launching a worker themselves, and repeated calls are idempotent
(`created: false` returns the existing usable session). The caller in server.rs
serializes calls behind a process-wide gate so two concurrent requests for the
same bead cannot both observe "no link" and create two workers.
*/

/// Result of resolving and dispatching one Project Board bead.
#[derive(Debug)]
pub struct StartBoardWorkOutcome {
    /// Whether this request created a new worker session.
    pub created: bool,
    /// `(projectId, sessionId)` of a newly created session so the caller can
    /// schedule the presentation delta and start the zmx provider.
    pub created_session: Option<(String, String)>,
    /// RPC payload returned to the caller.
    pub result: Value,
}

/// Resolve a board bead and reuse or create its linked worker session.
pub fn start_board_work(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    server_id: &str,
    params: &Map<String, Value>,
    bd_executable_path: &str,
) -> Result<StartBoardWorkOutcome, DomainStateError> {
    let bead_id = read_trimmed(params, "beadId").ok_or_else(|| {
        DomainStateError::bad_request("startBoardWork requires a bead id.")
    })?;
    let projects = repository.list_projects()?;
    let (board_project, bead) =
        resolve_board_project_and_bead(&projects, params, &bead_id, bd_executable_path)?;
    let board_project_id = project_id_of(&board_project)?;
    let store_projects = select_link_store_projects(&board_project, &projects);

    // Redline 2: any usable linked conversation — live, sleeping, or
    // restorable — is an idempotent success, never a second worker.
    if let Some((existing_project_id, existing_session_id)) =
        find_usable_linked_session(repository, db, server_id, &store_projects, &bead_id)?
    {
        return Ok(StartBoardWorkOutcome {
            created: false,
            created_session: None,
            result: json!({
                "created": false,
                "projectId": existing_project_id,
                "sessionId": existing_session_id,
            }),
        });
    }

    let agent_buttons = board_prompt_agent_options(&projects);
    let settings = read_agent_settings(db)?;
    let default_agent_id = settings
        .get("defaultPromptAgentId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let assignee = read_trimmed_value(&bead, "assignee");
    let agent_id = resolve_start_work_agent(
        read_trimmed(params, "agent").or_else(|| read_trimmed(params, "agentId")),
        assignee.as_deref(),
        &agent_buttons,
        &default_agent_id,
    );
    if agent_id.is_empty() {
        return Err(DomainStateError::bad_request(
            "No agent was resolved for this bead. Pass --agent or set a default prompt agent.",
        ));
    }
    let agent_button = agent_buttons.iter().find(|button| {
        button.get("agentId").and_then(Value::as_str) == Some(agent_id.as_str())
    });

    let canonical_bead_id = read_trimmed_value(&bead, "id").unwrap_or_else(|| bead_id.clone());
    let prompt = build_board_bead_work_prompt(
        &canonical_bead_id,
        &canonical_bead_id,
        read_trimmed_value(&bead, "title").as_deref().unwrap_or(""),
        read_trimmed_value(&bead, "description").as_deref(),
    );

    // Compose the create-agent path: the same parameter builder and repository
    // insert `/api/createAgentSession` uses, with the work prompt staged as the
    // gxserver first user message.
    let mut create_params = Map::new();
    create_params.insert("agentId".to_string(), Value::String(agent_id.clone()));
    create_params.insert(
        "projectId".to_string(),
        Value::String(board_project_id.clone()),
    );
    create_params.insert("surface".to_string(), Value::String("workspace".to_string()));
    create_params.insert("requireLaunchCommand".to_string(), Value::Bool(true));
    let mut launch_settings = Map::new();
    if let Some(command) = agent_button.and_then(|button| button.get("command")) {
        launch_settings.insert("agentCommand".to_string(), command.clone());
    }
    if let Some(icon) = agent_button.and_then(|button| button.get("icon")) {
        launch_settings.insert("icon".to_string(), icon.clone());
    }
    create_params.insert(
        "launchSettings".to_string(),
        Value::Object(launch_settings),
    );
    create_params.insert(
        "runtimeSettings".to_string(),
        json!({ "firstUserMessage": prompt }),
    );
    let normalized = create_agent_session_params_for_project(db, &board_project, &create_params)?;
    // Session identity and its board link are one durable operation. If link
    // persistence fails, rolling back the transaction prevents an unlinked
    // worker from being left behind for the next retry to duplicate.
    let transaction =
        Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(|error| {
            DomainStateError {
                code: "internalError",
                message: format!("Could not start board-work transaction: {error}"),
            }
        })?;
    let transaction_repository = DomainRepository::new(&transaction, server_id);
    let created_session = transaction_repository.create_session(&normalized, false)?;
    let session =
        apply_created_session_identity(&transaction_repository, &created_session, &normalized)?;
    let session_project_id = read_trimmed_value(&session, "projectId")
        .ok_or_else(|| DomainStateError::corrupt_state("Session missing projectId."))?;
    let session_id = read_trimmed_value(&session, "sessionId")
        .ok_or_else(|| DomainStateError::corrupt_state("Session missing sessionId."))?;

    write_bead_conversation_link(
        &transaction_repository,
        &board_project_id,
        &canonical_bead_id,
        &session,
        &session_project_id,
        &session_id,
        agent_button,
        &agent_id,
    )?;
    drop(transaction_repository);
    transaction.commit().map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not commit board-work transaction: {error}"),
    })?;

    Ok(StartBoardWorkOutcome {
        created: true,
        created_session: Some((session_project_id.clone(), session_id.clone())),
        result: json!({
            "created": true,
            "projectId": session_project_id,
            "sessionId": session_id,
        }),
    })
}

/*
Board-project resolution mirrors the shared-mount link-store contract in
shared/bead-conversation-links.ts: rows that mount the same Beads directory
(projectBoardConfig.beadsDirectory, else the project path) are one board. An
explicit projectId wins; otherwise every distinct store is probed with
`bd show` and the bead must live on exactly one of them.
*/
fn resolve_board_project_and_bead(
    projects: &[Value],
    params: &Map<String, Value>,
    bead_id: &str,
    bd_executable_path: &str,
) -> Result<(Value, Value), DomainStateError> {
    if let Some(project_id) = read_trimmed(params, "projectId") {
        let project = projects
            .iter()
            .find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(project_id.as_str())
            })
            .cloned()
            .ok_or_else(|| {
                DomainStateError::not_found(format!("Project {project_id} does not exist."))
            })?;
        let store = link_store_key(&project);
        if store.is_empty() {
            return Err(DomainStateError::bad_request(format!(
                "Project {project_id} has no Beads directory."
            )));
        }
        let bead = run_bd_show(bd_executable_path, &store, bead_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Bead {bead_id} was not found on the {project_id} project board."
            ))
        })?;
        return Ok((project, bead));
    }
    let mut matches: Vec<(Value, Value)> = Vec::new();
    let mut probed: HashMap<String, bool> = HashMap::new();
    for project in projects {
        let store = link_store_key(project);
        if store.is_empty() || probed.contains_key(&store) {
            continue;
        }
        if !Path::new(&store).join(".beads").is_dir() {
            probed.insert(store, false);
            continue;
        }
        let bead = run_bd_show(bd_executable_path, &store, bead_id)?;
        let found = bead.is_some();
        if let Some(bead) = bead {
            matches.push((project.clone(), bead));
        }
        probed.insert(store, found);
    }
    match matches.len() {
        0 => Err(DomainStateError::not_found(format!(
            "Bead {bead_id} was not found on any project board."
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(DomainStateError::bad_request(format!(
            "Bead {bead_id} exists on multiple project boards. Pass --project-id to choose one."
        ))),
    }
}

fn run_bd_show(
    bd_executable_path: &str,
    cwd: &str,
    bead_id: &str,
) -> Result<Option<Value>, DomainStateError> {
    const BD_SHOW_TIMEOUT: Duration = Duration::from_secs(10);

    let mut command = Command::new(bd_executable_path);
    command
        .args(["show", bead_id, "--json"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not run bd show: {error}"),
    })?;
    let mut stdout_reader = child.stdout.take().map(|mut stdout| {
        thread::spawn(move || {
            let mut output = Vec::new();
            stdout.read_to_end(&mut output).map(|_| output)
        })
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= BD_SHOW_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(reader) = stdout_reader.take() {
                    let _ = reader.join();
                }
                return Err(DomainStateError {
                    code: "internalError",
                    message: format!(
                        "bd show timed out after {} seconds.",
                        BD_SHOW_TIMEOUT.as_secs()
                    ),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(reader) = stdout_reader.take() {
                    let _ = reader.join();
                }
                return Err(DomainStateError {
                    code: "internalError",
                    message: format!("Could not wait for bd show: {error}"),
                });
            }
        }
    };
    let output = match stdout_reader {
        Some(reader) => reader
            .join()
            .map_err(|_| DomainStateError {
                code: "internalError",
                message: "bd show output reader panicked.".to_string(),
            })?
            .map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("Could not read bd show output: {error}"),
            })?,
        None => Vec::new(),
    };
    if !status.success() {
        // bd exits nonzero for an unknown issue id; that is "not on this board".
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output);
    let parsed: Value = match serde_json::from_str(stdout.trim()) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    // bd wraps some results as { data: ... }; unwrap to the issue object.
    let issue = match parsed {
        Value::Object(ref object) if object.contains_key("data") => {
            object.get("data").cloned().unwrap_or(Value::Null)
        }
        other => other,
    };
    let issue = match issue {
        Value::Array(items) => items.into_iter().next().unwrap_or(Value::Null),
        other => other,
    };
    Ok(issue.is_object().then_some(issue))
}

/*
CDXC:BoardStartWork 2026-08-07:
Phase 3 agent resolution: an explicit --agent always wins; a bead assignee is
honored only when it matches a configured board agent's id or name
case-insensitively (a role-name assignee that matches nothing falls through);
otherwise the saved default prompt agent runs the card.
*/
/// Choose the explicit, assignee-matched, or configured default agent.
pub(crate) fn resolve_start_work_agent(
    explicit_agent: Option<String>,
    assignee: Option<&str>,
    agent_buttons: &[Value],
    default_agent_id: &str,
) -> String {
    if let Some(agent) = explicit_agent.filter(|value| !value.is_empty()) {
        return agent;
    }
    if let Some(assignee) = assignee.map(str::trim).filter(|value| !value.is_empty()) {
        for button in agent_buttons {
            let agent_id = button.get("agentId").and_then(Value::as_str).unwrap_or("");
            let name = button.get("name").and_then(Value::as_str).unwrap_or("");
            if agent_id.eq_ignore_ascii_case(assignee) || name.eq_ignore_ascii_case(assignee) {
                return agent_id.to_string();
            }
        }
    }
    default_agent_id.to_string()
}

/// The configured agents a board prompt can run: the sidebar HUD registry
/// minus T3 and commandless entries, matching the board's agent dropdown.
fn board_prompt_agent_options(projects: &[Value]) -> Vec<Value> {
    sidebar_agent_buttons_from_projects(projects)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|button| {
            let agent_id = button.get("agentId").and_then(Value::as_str).unwrap_or("");
            let command = button.get("command").and_then(Value::as_str).unwrap_or("");
            agent_id != "t3" && !command.trim().is_empty()
        })
        .collect()
}

/*
CDXC:BoardStartWork 2026-08-07:
Rust port of `buildAgentWorkPrompt` in native/sidebar/project-board-shared.ts —
the canonical bead work prompt. The endpoint owns this copy for the CLI path;
`native/sidebar/board-start-work-prompt-parity.test.ts` asserts both templates
stay line-identical so the Rust and TypeScript prompts cannot drift apart
silently.
*/
/// Render the canonical Project Board worker prompt.
pub(crate) fn build_board_bead_work_prompt(
    bead_id: &str,
    display_id: &str,
    title: &str,
    description: Option<&str>,
) -> String {
    let description = description
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("No prompt provided.");
    [
        format!("Work on bead {bead_id} ({display_id}): {title}"),
        String::new(),
        description.to_string(),
        String::new(),
        "After each turn where you made progress on this bead, add a bead comment summarizing what you did:".to_string(),
        format!("- `gx bd comment {bead_id} \"<summary>\"`"),
        "- Focus on user-facing requirements delivered and high-level technical approach.".to_string(),
        "- Do not list specific files or line numbers.".to_string(),
        "- End the comment with `Agent: <agent name>` and `Session: <saved agent CLI session id>` lines so the ticket view can show the agent after the user name and the resumable agent session id at the bottom.".to_string(),
        String::new(),
        "Status workflow for this project board:".to_string(),
        format!("- Park for later: `gx bd update {bead_id} --status backlog`"),
        format!("- When you start: `gx bd update {bead_id} --status in_progress`"),
        format!("- When implementation is ready for test: `gx bd update {bead_id} --status test`"),
        format!("- When ready for review: `gx bd update {bead_id} --status review`"),
        format!("- When done: `gx bd close {bead_id}`"),
    ]
    .join("\n")
}

fn link_store_key(project: &Value) -> String {
    let configured = project
        .get("projectBoardConfig")
        .and_then(Value::as_object)
        .and_then(|config| config.get("beadsDirectory"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let directory = configured.or_else(|| project.get("path").and_then(Value::as_str));
    directory
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string()
}

fn select_link_store_projects(board_project: &Value, projects: &[Value]) -> Vec<Value> {
    let board_store_key = link_store_key(board_project);
    let board_project_id = board_project.get("projectId").and_then(Value::as_str);
    let mut store_projects = vec![board_project.clone()];
    if board_store_key.is_empty() {
        return store_projects;
    }
    for candidate in projects {
        if candidate.get("projectId").and_then(Value::as_str) != board_project_id
            && link_store_key(candidate) == board_store_key
        {
            store_projects.push(candidate.clone());
        }
    }
    store_projects
}

/// Trailing-suffix bead match key from shared/bead-conversation-links.ts:
/// Beads prefix renames rewrite `zmux-95421485` to `ghostex-95421485`, so
/// links match on the suffix after the last `-`.
fn bead_link_match_key(bead_id: &str) -> String {
    let normalized = bead_id.trim().to_lowercase();
    match normalized.rfind('-') {
        Some(index) if index > 0 && index + 1 < normalized.len() => {
            normalized[index + 1..].to_string()
        }
        _ => normalized,
    }
}

struct LinkedSessionReference {
    project_id: String,
    session_id: String,
    updated_at: String,
}

fn read_active_bead_links(store_projects: &[Value], bead_id: &str) -> Vec<LinkedSessionReference> {
    let match_key = bead_link_match_key(bead_id);
    let mut references = Vec::new();
    for project in store_projects {
        let store_project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let links = project
            .get("projectBoardConfig")
            .and_then(Value::as_object)
            .and_then(|config| config.get("beadConversationLinks"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for link in links {
            let link_bead_id = link.get("beadId").and_then(Value::as_str).unwrap_or("");
            let ghostex_session_id = link
                .get("ghostexSessionId")
                .and_then(Value::as_str)
                .unwrap_or("");
            if link_bead_id.is_empty()
                || ghostex_session_id.is_empty()
                || bead_link_match_key(link_bead_id) != match_key
                || link.get("status").and_then(Value::as_str) == Some("archived")
            {
                continue;
            }
            let (project_id, session_id) = resolve_link_session_reference(
                &link,
                ghostex_session_id,
                store_project_id,
            );
            references.push(LinkedSessionReference {
                project_id,
                session_id,
                updated_at: link
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }
    // Prefer the most recently updated conversation, like the board does.
    references.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    references
}

/// Session-owner resolution order from shared/bead-conversation-links.ts:
/// a `combined-session:` scoped id, then the explicit `sessionProjectId`
/// written since PR #87, then the legacy zmx persistence name, then the
/// storing row as the last resort.
fn resolve_link_session_reference(
    link: &Value,
    ghostex_session_id: &str,
    store_project_id: &str,
) -> (String, String) {
    if let Some((project_id, session_id)) = parse_scoped_session_id(ghostex_session_id) {
        return (project_id, session_id);
    }
    if let Some(session_project_id) = read_trimmed_value(link, "sessionProjectId") {
        return (session_project_id, ghostex_session_id.to_string());
    }
    if let Some(persistence_name) = read_trimmed_value(link, "sessionPersistenceName") {
        let parts: Vec<&str> = persistence_name.split('-').collect();
        if parts.len() == 3
            && parts[0].starts_with('S')
            && parts[1].starts_with('P')
            && parts[2].starts_with('G')
            && parts[2] == ghostex_session_id
        {
            return (parts[1].to_string(), parts[2].to_string());
        }
    }
    let link_project_id = read_trimmed_value(link, "projectId")
        .unwrap_or_else(|| store_project_id.to_string());
    (link_project_id, ghostex_session_id.to_string())
}

fn parse_scoped_session_id(session_id: &str) -> Option<(String, String)> {
    let payload = session_id.strip_prefix("combined-session:")?;
    let separator = payload.find(':')?;
    Some((
        percent_decode(&payload[..separator]),
        percent_decode(&payload[separator + 1..]),
    ))
}

fn percent_decode(value: &str) -> String {
    if !value.contains('%') {
        return value.to_string();
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/*
Redline 2's "usable" definition, evaluated daemon-side: a linked conversation
whose session row still exists (live or sleeping, in any project) is reused
directly; a link whose row is gone is still usable when the daemon's
previous-session history can restore it — the same restore contract the board's
resumable-link affordance reads.
*/
fn find_usable_linked_session(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    server_id: &str,
    store_projects: &[Value],
    bead_id: &str,
) -> Result<Option<(String, String)>, DomainStateError> {
    for reference in read_active_bead_links(store_projects, bead_id) {
        if repository
            .get_session(&reference.project_id, &reference.session_id)?
            .is_some()
        {
            return Ok(Some((reference.project_id, reference.session_id)));
        }
        let mut previous_params = Map::new();
        previous_params.insert(
            "projectId".to_string(),
            Value::String(reference.project_id.clone()),
        );
        previous_params.insert(
            "query".to_string(),
            Value::String(reference.session_id.clone()),
        );
        previous_params.insert("limit".to_string(), json!(20));
        let previous = list_previous_sessions(db, server_id, &previous_params)?;
        let restorable = previous
            .get("results")
            .and_then(Value::as_array)
            .is_some_and(|results| {
                results.iter().any(|row| {
                    row.get("sessionId").and_then(Value::as_str)
                        == Some(reference.session_id.as_str())
                })
            });
        if restorable {
            return Ok(Some((reference.project_id, reference.session_id)));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn write_bead_conversation_link(
    repository: &DomainRepository<'_>,
    board_project_id: &str,
    bead_id: &str,
    session: &Value,
    session_project_id: &str,
    session_id: &str,
    agent_button: Option<&Value>,
    agent_id: &str,
) -> Result<(), DomainStateError> {
    // Writes stay on the board's own row (shared-mount reads span the rows).
    // Re-read the row so a concurrent projectBoardConfig write is not lost.
    let link_project = repository
        .get_project(board_project_id)?
        .ok_or_else(|| DomainStateError::corrupt_state("Board project missing from store."))?;
    let ghostex_session_id = if session_project_id == board_project_id {
        session_id.to_string()
    } else {
        format!("combined-session:{session_project_id}:{session_id}")
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let link_id = [board_project_id, bead_id, &ghostex_session_id]
        .iter()
        .map(|part| sanitize_link_id_part(part))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(":");
    let mut link = Map::new();
    link.insert("agentId".to_string(), Value::String(agent_id.to_string()));
    if let Some(name) = agent_button.and_then(|button| button.get("name")).cloned() {
        link.insert("agentName".to_string(), name);
    }
    if let Some(agent_session_id) = read_trimmed_value(session, "agentSessionId") {
        link.insert("agentSessionId".to_string(), Value::String(agent_session_id));
    }
    if let Some(agent_session_path) = read_trimmed_value(session, "agentSessionPath") {
        link.insert(
            "agentSessionPath".to_string(),
            Value::String(agent_session_path),
        );
    }
    link.insert("beadDisplayId".to_string(), Value::String(bead_id.to_string()));
    link.insert("beadId".to_string(), Value::String(bead_id.to_string()));
    link.insert("createdAt".to_string(), Value::String(now.clone()));
    link.insert(
        "ghostexSessionId".to_string(),
        Value::String(ghostex_session_id),
    );
    link.insert("id".to_string(), Value::String(link_id.clone()));
    link.insert(
        "projectId".to_string(),
        Value::String(board_project_id.to_string()),
    );
    if let Some(zmx_name) = read_trimmed_value(session, "zmxName") {
        link.insert("sessionPersistenceName".to_string(), Value::String(zmx_name));
    }
    link.insert(
        "sessionPersistenceProvider".to_string(),
        Value::String("zmx".to_string()),
    );
    link.insert(
        "sessionProjectId".to_string(),
        Value::String(session_project_id.to_string()),
    );
    link.insert("status".to_string(), Value::String("active".to_string()));
    link.insert("updatedAt".to_string(), Value::String(now));

    let mut board_config = link_project
        .get("projectBoardConfig")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut links = board_config
        .get("beadConversationLinks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(existing) = links.iter_mut().find(|existing| {
        existing.get("id").and_then(Value::as_str) == Some(link_id.as_str())
    }) {
        let created_at = existing.get("createdAt").cloned();
        let mut merged = existing.as_object().cloned().unwrap_or_default();
        for (key, value) in &link {
            merged.insert(key.clone(), value.clone());
        }
        if let Some(created_at) = created_at {
            merged.insert("createdAt".to_string(), created_at);
        }
        *existing = Value::Object(merged);
    } else {
        links.push(Value::Object(link));
    }
    board_config.insert("beadConversationLinks".to_string(), Value::Array(links));
    let mut update = Map::new();
    update.insert(
        "projectId".to_string(),
        Value::String(board_project_id.to_string()),
    );
    update.insert(
        "projectBoardConfig".to_string(),
        Value::Object(board_config),
    );
    repository.update_project(&update)?;
    Ok(())
}

fn sanitize_link_id_part(part: &str) -> String {
    // Port of createBeadConversationLinkId's `[^a-z0-9_-]+` → `-` collapse.
    let mut sanitized = String::with_capacity(part.len());
    let mut previous_was_dash = false;
    for character in part.trim().chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            sanitized.push(character);
            previous_was_dash = false;
        } else if !previous_was_dash {
            sanitized.push('-');
            previous_was_dash = true;
        }
    }
    sanitized.trim_matches('-').to_string()
}

fn project_id_of(project: &Value) -> Result<String, DomainStateError> {
    read_trimmed_value(project, "projectId")
        .ok_or_else(|| DomainStateError::corrupt_state("Project missing projectId."))
}

fn read_trimmed(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_trimmed_value(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };
    use std::fs;
    use std::sync::{Arc, Barrier, Mutex};

    const TEST_SERVER_ID: &str = "S7k";
    const TEST_BEAD_ID: &str = "gx-95421499";

    struct TestBoard {
        _temp: tempfile::TempDir,
        paths: crate::paths::GxserverPaths,
        bd_path: String,
        project_id: String,
    }

    fn make_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).expect("bd metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("bd permissions");
    }

    /// A board fixture: gxserver storage in a tempdir (explicit home, no HOME
    /// override), one project whose beadsDirectory holds a fake `.beads`
    /// workspace, and a fake `bd` that knows exactly one bead.
    fn create_test_board(assignee: Option<&str>) -> TestBoard {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        if let Some(config_dir) = paths.config_file.parent() {
            fs::create_dir_all(config_dir).expect("config dir");
        }
        initialize_gxserver_storage(&paths).expect("storage init");

        let board_dir = temp.path().join("board");
        fs::create_dir_all(board_dir.join(".beads")).expect("board dir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("repo dir");

        let assignee_field = assignee
            .map(|value| format!("\"assignee\":\"{value}\","))
            .unwrap_or_default();
        let bd_path = temp.path().join("bd");
        fs::write(
            &bd_path,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"show\" ] && [ \"$2\" = \"{TEST_BEAD_ID}\" ]; then\n  echo '{{\"data\":{{\"id\":\"{TEST_BEAD_ID}\",\"title\":\"Fix the flux capacitor\",{assignee_field}\"description\":\"Reverse the polarity.\",\"status\":\"open\"}}}}'\n  exit 0\nfi\necho 'issue not found' >&2\nexit 1\n"
            ),
        )
        .expect("write bd");
        make_executable(&bd_path);

        let db = open_gxserver_database(&paths).expect("open db");
        let repository = DomainRepository::new(&db, TEST_SERVER_ID);
        let project = repository
            .create_project(
                json!({
                    "name": "Board",
                    "path": repo_dir.to_string_lossy(),
                    "projectBoardConfig": { "beadsDirectory": board_dir.to_string_lossy() },
                })
                .as_object()
                .expect("project params"),
            )
            .expect("project created");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("projectId")
            .to_string();
        TestBoard {
            _temp: temp,
            paths,
            bd_path: bd_path.to_string_lossy().to_string(),
            project_id,
        }
    }

    fn run_start_board_work(
        board: &TestBoard,
        params: &Map<String, Value>,
    ) -> Result<StartBoardWorkOutcome, DomainStateError> {
        let db = open_gxserver_database(&board.paths).expect("open db");
        let repository = DomainRepository::new(&db, TEST_SERVER_ID);
        start_board_work(&repository, &db, TEST_SERVER_ID, params, &board.bd_path)
    }

    fn bead_params() -> Map<String, Value> {
        let mut params = Map::new();
        params.insert("beadId".to_string(), json!(TEST_BEAD_ID));
        params
    }

    fn add_board_project(board: &TestBoard, name: &str) -> String {
        let board_dir = board._temp.path().join(format!("{name}-board"));
        fs::create_dir_all(board_dir.join(".beads")).expect("second board dir");
        let repo_dir = board._temp.path().join(format!("{name}-repo"));
        fs::create_dir_all(&repo_dir).expect("second repo dir");
        let db = open_gxserver_database(&board.paths).expect("open db");
        let repository = DomainRepository::new(&db, TEST_SERVER_ID);
        repository
            .create_project(
                json!({
                    "name": name,
                    "path": repo_dir.to_string_lossy(),
                    "projectBoardConfig": { "beadsDirectory": board_dir.to_string_lossy() },
                })
                .as_object()
                .expect("project params"),
            )
            .expect("second project created")
            .get("projectId")
            .and_then(Value::as_str)
            .expect("second projectId")
            .to_string()
    }

    fn read_board_links(board: &TestBoard) -> Vec<Value> {
        let db = open_gxserver_database(&board.paths).expect("open db");
        let repository = DomainRepository::new(&db, TEST_SERVER_ID);
        let projects = repository.list_projects().expect("projects");
        projects
            .iter()
            .find(|project| {
                project.get("projectId").and_then(Value::as_str)
                    == Some(board.project_id.as_str())
            })
            .and_then(|project| project.get("projectBoardConfig"))
            .and_then(|config| config.get("beadConversationLinks"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn start_board_work_creates_session_link_and_prompt() {
        let board = create_test_board(None);
        let outcome = run_start_board_work(&board, &bead_params()).expect("start work");
        assert!(outcome.created);
        let session_id = outcome
            .result
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("sessionId")
            .to_string();
        assert_eq!(outcome.result.get("created"), Some(&json!(true)));
        assert_eq!(
            outcome.result.get("projectId"),
            Some(&json!(board.project_id))
        );

        let db = open_gxserver_database(&board.paths).expect("open db");
        let repository = DomainRepository::new(&db, TEST_SERVER_ID);
        let session = repository
            .get_session(&board.project_id, &session_id)
            .expect("session lookup")
            .expect("session exists");
        assert_eq!(session.get("kind"), Some(&json!("agent")));
        let first_user_message = session
            .get("runtimeSettings")
            .and_then(|settings| settings.get("firstUserMessage"))
            .and_then(Value::as_str)
            .expect("staged prompt");
        assert!(first_user_message
            .starts_with(&format!("Work on bead {TEST_BEAD_ID} ({TEST_BEAD_ID}): Fix the flux capacitor")));
        assert!(first_user_message.contains("Reverse the polarity."));
        assert!(first_user_message.contains(&format!("`gx bd close {TEST_BEAD_ID}`")));

        let links = read_board_links(&board);
        assert_eq!(links.len(), 1);
        let link = &links[0];
        assert_eq!(link.get("beadId"), Some(&json!(TEST_BEAD_ID)));
        assert_eq!(link.get("ghostexSessionId"), Some(&json!(session_id)));
        assert_eq!(link.get("sessionProjectId"), Some(&json!(board.project_id)));
        assert_eq!(link.get("status"), Some(&json!("active")));
    }

    #[test]
    fn start_board_work_rolls_back_session_when_link_persistence_fails() {
        let board = create_test_board(None);
        let db = open_gxserver_database(&board.paths).expect("open db");
        db.execute_batch(
            r#"
            CREATE TEMP TRIGGER fail_board_link_update
            BEFORE UPDATE OF projectBoardConfigJson ON projects
            BEGIN
              SELECT RAISE(ABORT, 'forced board-link persistence failure');
            END;
            "#,
        )
        .expect("install failure trigger");
        let repository = DomainRepository::new(&db, TEST_SERVER_ID);

        let error = start_board_work(
            &repository,
            &db,
            TEST_SERVER_ID,
            &bead_params(),
            &board.bd_path,
        )
        .expect_err("link persistence must fail");

        assert_eq!(error.code, "internalError");
        assert!(repository
            .list_sessions(Some(&board.project_id))
            .expect("sessions")
            .is_empty());
        assert!(read_board_links(&board).is_empty());
    }

    #[test]
    fn start_board_work_reuses_existing_usable_linked_session() {
        let board = create_test_board(None);
        let first = run_start_board_work(&board, &bead_params()).expect("first start");
        let first_session_id = first
            .result
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("sessionId")
            .to_string();

        let second = run_start_board_work(&board, &bead_params()).expect("second start");
        assert!(!second.created);
        assert!(second.created_session.is_none());
        assert_eq!(second.result.get("created"), Some(&json!(false)));
        assert_eq!(
            second.result.get("sessionId"),
            Some(&json!(first_session_id))
        );
        assert_eq!(
            second.result.get("projectId"),
            Some(&json!(board.project_id))
        );
        assert_eq!(read_board_links(&board).len(), 1);
    }

    #[test]
    fn start_board_work_uses_explicit_project_when_bead_exists_on_multiple_boards() {
        let board = create_test_board(None);
        let second_project_id = add_board_project(&board, "Second");
        let mut params = bead_params();
        params.insert("projectId".to_string(), json!(second_project_id));

        let outcome = run_start_board_work(&board, &params).expect("explicit board start");

        assert_eq!(outcome.result.get("projectId"), Some(&json!(second_project_id)));
    }

    #[test]
    fn start_board_work_rejects_ambiguous_unqualified_bead() {
        let board = create_test_board(None);
        add_board_project(&board, "Second");

        let error = run_start_board_work(&board, &bead_params()).expect_err("ambiguous bead");

        assert_eq!(error.code, "badRequest");
        assert!(error.message.contains("exists on multiple project boards"));
    }

    #[test]
    fn start_board_work_serializes_concurrent_calls_into_one_session() {
        // The endpoint-level race guard: both callers run the full
        // check-then-create sequence, but behind the shared gate the second
        // observes the first call's link and reuses its session.
        let board = Arc::new(create_test_board(None));
        let gate = Arc::new(Mutex::new(()));
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let board = Arc::clone(&board);
            let gate = Arc::clone(&gate);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let _guard = gate.lock().expect("gate");
                run_start_board_work(&board, &bead_params()).expect("start work")
            }));
        }
        let outcomes: Vec<StartBoardWorkOutcome> = handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect();
        let created_count = outcomes.iter().filter(|outcome| outcome.created).count();
        assert_eq!(created_count, 1);
        let session_ids: Vec<&str> = outcomes
            .iter()
            .map(|outcome| {
                outcome
                    .result
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .expect("sessionId")
            })
            .collect();
        assert_eq!(session_ids[0], session_ids[1]);
        assert_eq!(read_board_links(&board).len(), 1);
    }

    #[test]
    fn start_board_work_rejects_unknown_bead() {
        let board = create_test_board(None);
        let mut params = Map::new();
        params.insert("beadId".to_string(), json!("gx-does-not-exist"));
        let error = run_start_board_work(&board, &params).expect_err("unknown bead");
        assert_eq!(error.code, "notFound");
        assert!(error.message.contains("gx-does-not-exist"));
    }

    #[test]
    fn start_board_work_uses_matching_assignee_agent() {
        let board = create_test_board(Some("Claude"));
        let outcome = run_start_board_work(&board, &bead_params()).expect("start work");
        let session_id = outcome
            .result
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("sessionId")
            .to_string();
        let db = open_gxserver_database(&board.paths).expect("open db");
        let repository = DomainRepository::new(&db, TEST_SERVER_ID);
        let session = repository
            .get_session(&board.project_id, &session_id)
            .expect("session lookup")
            .expect("session exists");
        assert_eq!(session.get("agentId"), Some(&json!("claude")));
    }

    #[test]
    fn resolve_start_work_agent_follows_documented_chain() {
        let agents = vec![
            json!({ "agentId": "claude", "command": "claude", "name": "Claude" }),
            json!({ "agentId": "codex", "command": "codex", "name": "Codex" }),
        ];
        // 1. Explicit --agent always wins.
        assert_eq!(
            resolve_start_work_agent(Some("codex".to_string()), Some("Claude"), &agents, "claude"),
            "codex"
        );
        // 2. Assignee matches a configured agent id or name case-insensitively.
        assert_eq!(
            resolve_start_work_agent(None, Some("CLAUDE"), &agents, "codex"),
            "claude"
        );
        assert_eq!(
            resolve_start_work_agent(None, Some("Codex"), &agents, "claude"),
            "codex"
        );
        // 3. A role-name assignee that matches nothing falls through to the
        //    project default.
        assert_eq!(
            resolve_start_work_agent(None, Some("Researcher"), &agents, "codex"),
            "codex"
        );
        assert_eq!(resolve_start_work_agent(None, None, &agents, "codex"), "codex");
    }

    #[test]
    fn board_bead_work_prompt_matches_the_board_template() {
        let prompt = build_board_bead_work_prompt("gx-12", "gx-12", "Do the thing", None);
        assert!(prompt.starts_with("Work on bead gx-12 (gx-12): Do the thing"));
        assert!(prompt.contains("No prompt provided."));
        assert!(prompt.contains("- `gx bd comment gx-12 \"<summary>\"`"));
        assert!(prompt.contains("- When done: `gx bd close gx-12`"));
    }
}
