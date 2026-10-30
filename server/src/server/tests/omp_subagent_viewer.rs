//! The subagent viewer for an OMP session, through the real `readSessionChat` subagent handler and a
//! synthetic OMP 18.13.3 transcript.

use super::*;
use crate::domain::DomainRepository;
use crate::storage::open_gxserver_database;
use std::io::Write;

fn at(second: u32) -> String {
    format!("2026-10-30T12:00:{second:02}.000Z")
}

/// Appends one session entry, chained to the previous one the way OMP links its active branch.
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
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap(),
        "{row}"
    )
    .unwrap();
}

fn message(id: &str, second: u32, message: Value) -> Value {
    json!({"type":"message","id":id,"timestamp":at(second),"message":message})
}

async fn read_child(state: &AppState, project_id: &str, session_id: &str) -> Value {
    let response = crate::session_chat_subagent::handle_read_subagent(
        state,
        "/api/readSessionChat".into(),
        "read-worker".into(),
        project_id,
        session_id,
        "Worker",
        300,
        None,
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    response_json(response.response).await["result"].clone()
}

#[tokio::test]
async fn omp_viewer_lifecycle_follows_the_child_run() {
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
    let child = temp
        .path()
        .join("sessions/2026-10-30T12-00-00-000Z_root/Worker.jsonl");
    append(
        &child,
        json!({"type":"session","version":3,"id":"Worker","timestamp":at(1),"cwd":"/work"}),
    );
    append(
        &child,
        json!({"type":"session_init","id":"init","timestamp":at(1),"agent":"custom-worker",
            "task":"Work.","systemPrompt":["Role."],"tools":["read","yield"]}),
    );
    append(
        &child,
        message(
            "prompt",
            2,
            json!({"role":"user","content":[{"type":"text","text":"Work."}]}),
        ),
    );
    append(
        &child,
        message(
            "call-yield",
            3,
            json!({"role":"assistant","stopReason":"toolUse",
                "content":[{"type":"toolCall","id":"c3","name":"yield","arguments":{}}]}),
        ),
    );
    append(
        &child,
        message(
            "yield",
            4,
            json!({"role":"toolResult","toolCallId":"c3","toolName":"yield","isError":false,
                "details":{"status":"success","data":{}},"content":[{"type":"text","text":"Done."}]}),
        ),
    );
    let (project_id, session_id) = {
        let db = open_gxserver_database(&paths).unwrap();
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let project = repository
            .create_project(
                json!({"name":"OMP viewer","path":temp.path()})
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
        let project_id = project["projectId"].as_str().unwrap().to_string();
        let session = repository
            .create_session(
                json!({"projectId":project_id,"kind":"agent","agentId":"omp","cwd":temp.path(),
                    "runtimeSettings":{"agentSessionId":"root","agentSessionPath":root}})
                .as_object()
                .unwrap(),
                false,
            )
            .unwrap();
        (
            project_id,
            session["sessionId"].as_str().unwrap().to_string(),
        )
    };

    // The final `yield` ends the run.
    let finished = read_child(&state, &project_id, &session_id).await;
    assert_eq!(finished["agent"], "omp");
    assert_eq!(finished["subagent"]["name"], "Worker");
    assert_eq!(finished["subagent"]["agentType"], "custom-worker");
    assert_eq!(finished["lifecycle"]["state"], "completed");
    assert_eq!(finished["lifecycle"]["turnId"], "yield");

    // A background result wakes it, and its reply shows the run resumed.
    append(
        &child,
        json!({"type":"custom_message","id":"async","timestamp":at(5),"customType":"async-result",
            "content":"Background job finished.","display":true}),
    );
    append(
        &child,
        message(
            "resume",
            6,
            json!({"role":"assistant","stopReason":"toolUse",
                "content":[{"type":"toolCall","id":"c6","name":"read","arguments":{}}]}),
        ),
    );
    let resumed = read_child(&state, &project_id, &session_id).await;
    assert_eq!(resumed["lifecycle"]["state"], "working");
    assert_eq!(resumed["lifecycle"]["turnId"], "resume");

    // Disposing the child's session ends it again.
    append(
        &child,
        json!({"type":"custom","id":"exit","timestamp":at(7),"customType":"session_exit",
            "data":{"reason":"dispose","kind":"normal","recordedAt":at(7)}}),
    );
    let exited = read_child(&state, &project_id, &session_id).await;
    assert_eq!(exited["lifecycle"]["state"], "completed");
    assert_eq!(exited["lifecycle"]["turnId"], "exit");

    // A follow-up that ends in a closing reply, then a background result and a reply after it:
    // Pi's own turn marker still says completed, the child works again.
    append(
        &child,
        message(
            "follow-up",
            8,
            json!({"role":"user","content":[{"type":"text","text":"More."}]}),
        ),
    );
    append(
        &child,
        message(
            "answer",
            9,
            json!({"role":"assistant","stopReason":"stop","content":[{"type":"text","text":"Done."}]}),
        ),
    );
    let answered = read_child(&state, &project_id, &session_id).await;
    assert_eq!(answered["lifecycle"]["state"], "completed");
    assert_eq!(answered["lifecycle"]["turnId"], "answer");
    append(
        &child,
        json!({"type":"custom_message","id":"async-2","timestamp":at(10),"customType":"async-result",
            "content":"Background job finished.","display":true}),
    );
    append(
        &child,
        message(
            "resume-2",
            11,
            json!({"role":"assistant","stopReason":"toolUse",
                "content":[{"type":"toolCall","id":"c11","name":"read","arguments":{}}]}),
        ),
    );
    let woken = read_child(&state, &project_id, &session_id).await;
    assert_eq!(woken["lifecycle"]["state"], "working");
    assert_eq!(woken["lifecycle"]["turnId"], "resume-2");
}
