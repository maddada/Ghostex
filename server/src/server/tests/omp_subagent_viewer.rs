//! An OMP session's Subagents rows open their child's transcript: the roster row's selector goes
//! through the shared `readSessionChat` subagent handler, on synthetic OMP subagent transcripts.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;
use crate::storage::open_gxserver_database;

const MODEL: &str = "anthropic/claude-sonnet-4-5";

fn at(second: u32) -> String {
    format!("2026-10-30T12:00:{second:02}.000Z")
}

/// Appends one entry, chained to the previous one the way OMP links its active branch.
fn append(path: &Path, mut row: Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let previous = fs::read_to_string(path).unwrap_or_default();
    let parent = previous
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| entry["type"] != "session")
        .filter_map(|entry| entry.get("id").cloned())
        .last()
        .unwrap_or(Value::Null);
    if row["type"] != "session" {
        row["parentId"] = parent;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{row}").unwrap();
}

fn message(id: &str, second: u32, message: Value) -> Value {
    json!({"type":"message","id":id,"timestamp":at(second),"message":message})
}

struct Lead {
    _temp: tempfile::TempDir,
    state: Arc<AppState>,
    project_id: String,
    session_id: String,
    /// `<lead>/`, where OMP writes the lead's subagents.
    folder: PathBuf,
}

/// A running OMP session whose lead transcript is on disk.
fn lead() -> Lead {
    let temp = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let root = temp
        .path()
        .join("sessions/2026-10-30T12-00-00-000Z_root.jsonl");
    append(
        &root,
        json!({"type":"session","version":3,"id":"root","timestamp":at(0),"cwd":"/work"}),
    );
    let db = open_gxserver_database(&paths).unwrap();
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({"name":"OMP subagents","path":temp.path()})
                .as_object()
                .unwrap(),
        )
        .unwrap();
    let project_id = project["projectId"].as_str().unwrap().to_string();
    let session = repository
        .create_session(
            json!({"projectId":project_id,"kind":"agent","agentId":"omp","cwd":temp.path(),
                "lifecycleState":"running",
                "runtimeSettings":{"agentSessionId":"root","agentSessionPath":root}})
            .as_object()
            .unwrap(),
            false,
        )
        .unwrap();
    Lead {
        folder: root.with_extension(""),
        _temp: temp,
        state,
        project_id,
        session_id: session["sessionId"].as_str().unwrap().to_string(),
    }
}

/// A child OMP spawned with `task`, in the middle of its run: it read its assignment and called a
/// tool.
fn spawn(lead: &Lead, name: &str) -> PathBuf {
    let child = lead.folder.join(format!("{name}.jsonl"));
    append(
        &child,
        json!({"type":"session","version":3,"id":name,"timestamp":at(1),"cwd":"/work"}),
    );
    append(
        &child,
        json!({"type":"session_init","id":"init","timestamp":at(1),"agent":"task",
            "resolvedModel":MODEL,"task":"Read the files.","tools":["read","yield"]}),
    );
    append(
        &child,
        json!({"type":"thinking_level_change","id":"level","timestamp":at(1),"thinkingLevel":"high"}),
    );
    append(
        &child,
        message(
            "prompt",
            2,
            json!({"role":"user","content":[{"type":"text","text":"Read the files."}]}),
        ),
    );
    append(
        &child,
        message(
            "call-read",
            3,
            json!({"role":"assistant","stopReason":"toolUse","content":[
                {"type":"toolCall","id":"c3","name":"read","arguments":{"path":"a.rs","i":"Reading a.rs"}}]}),
        ),
    );
    child
}

/// OMP ends a run on its final `yield`: the tool result is the last message, with no reply after it.
fn yield_result(child: &Path) {
    append(
        child,
        message(
            "call-yield",
            4,
            json!({"role":"assistant","stopReason":"toolUse",
                "content":[{"type":"toolCall","id":"c4","name":"yield","arguments":{}}]}),
        ),
    );
    append(
        child,
        message(
            "yield",
            5,
            json!({"role":"toolResult","toolCallId":"c4","toolName":"yield","isError":false,
                "details":{"status":"success","data":{}},"content":[{"type":"text","text":"Done."}]}),
        ),
    );
}

async fn read_child(lead: &Lead, selector: &str) -> (StatusCode, Value) {
    let response = crate::session_chat_subagent::handle_read_subagent(
        &lead.state,
        "/api/readSessionChat".into(),
        "read-child".into(),
        &lead.project_id,
        &lead.session_id,
        selector,
        300,
        None,
    )
    .await;
    let status = response.response.status();
    (status, response_json(response.response).await)
}

async fn run_state(lead: &Lead, selector: &str) -> Value {
    let (status, body) = read_child(lead, selector).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["result"]["lifecycle"]["state"].clone()
}

/// The rows the Subagents strip shows for this session, from gxserver's roster through the chat
/// core.
fn roster_rows(lead: &Lead) -> Vec<ghostex_gx_chat_core::extras::agent_fleet::AgentFleetRow> {
    let db = open_gxserver_database(&lead.state.paths).unwrap();
    let repository = DomainRepository::new(&db, &lead.state.metadata.server_id);
    let session = repository
        .get_session(&lead.project_id, &lead.session_id)
        .unwrap()
        .unwrap();
    let fleet = crate::session_chat_omp_fleet::read_omp_fleet(&session)
        .unwrap()
        .unwrap();
    let context = ghostex_gx_chat_core::state::ChatContext {
        now_ms: chrono::Utc::now().timestamp_millis() as f64,
        ..Default::default()
    };
    ghostex_gx_chat_core::extras::agent_fleet::agent_fleet_rows(
        Some(&fleet.to_value()),
        Some("omp"),
        &context,
    )
    .unwrap()
    .rows
}

#[tokio::test]
async fn an_omp_row_opens_its_childs_transcript_with_its_model() {
    let lead = lead();
    spawn(&lead, "Worker");

    let rows = roster_rows(&lead);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].selector, "Worker");

    let (status, body) = read_child(&lead, &rows[0].selector).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let page = &body["result"];
    assert_eq!(page["agent"], "omp");
    assert_eq!(page["subagent"]["id"], "Worker");
    assert_eq!(page["subagent"]["name"], "Worker");
    assert_eq!(page["subagent"]["model"], MODEL);
    assert_eq!(page["subagent"]["effort"], "high");
    assert!(page["messages"].to_string().contains("Read the files."));
    assert_eq!(page["lifecycle"]["state"], "working");
    assert_eq!(page["lifecycle"]["turnId"], "call-read");
}

#[tokio::test]
async fn a_finished_omp_child_reads_finished() {
    let lead = lead();

    // The result it handed back (`<name>.md`), written after its final `yield`.
    let handed = spawn(&lead, "Handed");
    yield_result(&handed);
    assert_eq!(run_state(&lead, "Handed").await, "working");
    fs::write(lead.folder.join("Handed.md"), "Done.").unwrap();
    assert_eq!(run_state(&lead, "Handed").await, "completed");

    // Its session was disposed.
    let exited = spawn(&lead, "Exited");
    yield_result(&exited);
    append(
        &exited,
        json!({"type":"custom","id":"exit","timestamp":at(6),"customType":"session_exit",
            "data":{"reason":"dispose","kind":"normal","recordedAt":at(6)}}),
    );
    let (status, body) = read_child(&lead, "Exited").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["lifecycle"]["state"], "completed");
    assert_eq!(body["result"]["lifecycle"]["turnId"], "exit");

    // It was killed mid-run; its row leaves the roster, its transcript stays readable.
    spawn(&lead, "Killed");
    assert_eq!(run_state(&lead, "Killed").await, "working");
    fs::write(lead.folder.join("Killed.jsonl.tombstone"), "").unwrap();
    assert_eq!(run_state(&lead, "Killed").await, "completed");

    let rows = roster_rows(&lead);
    let names = rows
        .iter()
        .map(|row| row.selector.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["Exited", "Handed"]);
    assert!(rows.iter().all(|row| !row.working));
    assert!(
        rows.iter()
            .find(|row| row.selector == "Handed")
            .unwrap()
            .done
    );
}

/// OMP keeps a task name verbatim, so a child can be named like a roster snapshot.
#[tokio::test]
async fn an_omp_child_named_like_a_roster_snapshot_opens() {
    let lead = lead();
    spawn(&lead, "fleet:Worker");
    let rows = roster_rows(&lead);
    assert_eq!(rows[0].selector, "fleet:Worker");
    let (status, body) = read_child(&lead, &rows[0].selector).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["subagent"]["id"], "fleet:Worker");
}

/// A backslash is part of a file name outside Windows: the child stays in the roster, as it did
/// before rows opened, and opens.
#[cfg(unix)]
#[tokio::test]
async fn an_omp_child_with_a_backslash_in_its_name_stays_listed_and_opens() {
    let lead = lead();
    spawn(&lead, "Docs\\Tests");
    let rows = roster_rows(&lead);
    let selectors = rows
        .iter()
        .map(|row| row.selector.as_str())
        .collect::<Vec<_>>();
    assert_eq!(selectors, ["Docs\\Tests"]);
    let (status, body) = read_child(&lead, "Docs\\Tests").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["subagent"]["id"], "Docs\\Tests");
}

#[tokio::test]
async fn an_unknown_omp_subagent_gives_the_existing_error() {
    let lead = lead();
    spawn(&lead, "Worker");
    fs::write(lead.folder.join("Worker.md"), "Done.").unwrap();
    for selector in [
        "Nobody",
        "../Worker",
        "Worker/..",
        "/Worker",
        "Worker/",
        "Worker\0",
        ".Worker",
        "Worker.md",
    ] {
        let (status, body) = read_child(&lead, selector).await;
        assert_ne!(status, StatusCode::OK, "{selector}: {body}");
        assert_eq!(body["error"], "subagentTranscriptUnavailable", "{selector}");
        assert_eq!(
            body["message"], "This subagent's transcript is not available yet.",
            "{selector}"
        );
    }
}
