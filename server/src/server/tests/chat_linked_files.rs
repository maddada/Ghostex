//! Issue #462: a remote project's Files view opens a text file outside the project only when the
//! session's own chat links to it (CDXC:Docs 2026-11-07 in server/src/project_docs/chat_file.rs).
//! Driven through `/api/runProjectDocsAction` with a real session and transcript.

use super::*;
use std::fs;

struct Fixture {
    _temp: tempfile::TempDir,
    state: Arc<AppState>,
    token: String,
    project_id: String,
    session_id: String,
    other_session_id: String,
    outside: std::path::PathBuf,
}

/// A project, a session whose chat links four files outside it (a log, a binary, an oversized log
/// and one through a different spelling of its folder), and a second session that links nothing.
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().join("gxserver")));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let project_path = temp.path().join("project");
    let outside = temp.path().join("scratchpad");
    fs::create_dir_all(&project_path).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("dev-8788.log"), "listening on 8788\n").unwrap();
    fs::write(outside.join("secret.log"), "never linked\n").unwrap();
    fs::write(outside.join("core.bin"), [0x7f, b'E', b'L', b'F', 0, 0, 1]).unwrap();
    fs::write(
        outside.join("huge.log"),
        vec![b'x'; crate::project_docs::CHAT_FILE_MAX_BYTES as usize + 10],
    )
    .unwrap();
    fs::write(outside.join("spelled.log"), "same file\n").unwrap();
    let dir = outside.to_string_lossy();
    let reply = format!(
        "The dev server log is `{dir}/dev-8788.log`. The core dump is `{dir}/core.bin` and the full \
         trace is `{dir}/huge.log`. Also see `{dir}/../scratchpad/spelled.log`."
    );
    let transcript = temp.path().join("transcript.jsonl");
    let lines = [
        json!({"type": "user", "uuid": "u1", "message": {"role": "user", "content": "Where is the log?"}}),
        json!({"type": "assistant", "uuid": "a1", "message": {"role": "assistant", "content": [{"type": "text", "text": reply}]}}),
    ];
    fs::write(
        &transcript,
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let empty_transcript = temp.path().join("empty.jsonl");
    fs::write(
        &empty_transcript,
        format!(
            "{}\n",
            json!({"type": "user", "uuid": "u2", "message": {"role": "user", "content": "Hello"}})
        ),
    )
    .unwrap();

    let db = crate::storage::open_gxserver_database(&paths).expect("open db");
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let project = repository
        .create_project(
            json!({"name": "Remote", "path": project_path})
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap().to_string();
    let session = |title: &str, transcript: &std::path::Path| {
        repository
            .create_session(
                json!({
                    "agentId": "claude",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {"agentName": "claude", "agentSessionPath": transcript},
                    "title": title,
                })
                .as_object()
                .unwrap(),
                true,
            )
            .expect("session")["sessionId"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let session_id = session("Linked", &transcript);
    let other_session_id = session("Quiet", &empty_transcript);
    drop(db);
    Fixture {
        _temp: temp,
        state,
        token,
        project_id,
        session_id,
        other_session_id,
        outside,
    }
}

impl Fixture {
    async fn docs(&self, action: &str, session_id: Option<&str>, path: &str) -> Value {
        let mut params = json!({
            "action": action,
            "requestId": "req-1",
            "projectId": self.project_id,
            "path": path,
            "scope": "project",
        });
        if let Some(session_id) = session_id {
            params["sessionId"] = json!(session_id);
        }
        let routed = route_http(
            self.state.clone(),
            rpc_request(
                "/api/runProjectDocsAction",
                &self.token,
                json!({ "params": params }),
            ),
            "docs".to_string(),
        )
        .await;
        assert_eq!(routed.response.status(), StatusCode::OK);
        response_json(routed.response).await["result"].clone()
    }

    fn file(&self, name: &str) -> String {
        self.outside.join(name).to_string_lossy().into_owned()
    }
}

#[tokio::test]
async fn a_file_the_chat_links_opens_read_only() {
    let fixture = fixture();
    let log = fixture.file("dev-8788.log");
    let read = fixture.docs("read", Some(&fixture.session_id), &log).await;
    assert_eq!(read["file"]["content"], "listening on 8788\n", "{read}");
    assert_eq!(read["file"]["readOnly"], true);
    assert_eq!(read["file"]["path"], log.as_str());
    assert_eq!(read["requestId"], "req-1");
    let stat = fixture.docs("stat", Some(&fixture.session_id), &log).await;
    assert_eq!(stat["file"]["size"], 18, "{stat}");
}

#[tokio::test]
async fn the_same_file_spelled_another_way_still_opens() {
    let fixture = fixture();
    let read = fixture
        .docs(
            "read",
            Some(&fixture.session_id),
            &fixture.file("spelled.log"),
        )
        .await;
    assert_eq!(read["file"]["content"], "same file\n", "{read}");
}

#[tokio::test]
async fn a_file_the_chat_never_linked_is_refused() {
    let fixture = fixture();
    let secret = fixture.file("secret.log");
    for action in ["read", "stat"] {
        let refused = fixture
            .docs(action, Some(&fixture.session_id), &secret)
            .await;
        assert_eq!(
            refused["error"],
            crate::project_docs::CHAT_FILE_NOT_NAMED,
            "{refused}"
        );
        assert!(refused.get("file").is_none());
    }
    // Linked by one session, asked for by another.
    let elsewhere = fixture
        .docs(
            "read",
            Some(&fixture.other_session_id),
            &fixture.file("dev-8788.log"),
        )
        .await;
    assert_eq!(elsewhere["error"], crate::project_docs::CHAT_FILE_NOT_NAMED);
    // A session that does not exist names nothing.
    let unknown = fixture
        .docs(
            "read",
            Some("no-such-session"),
            &fixture.file("dev-8788.log"),
        )
        .await;
    assert_eq!(unknown["error"], crate::project_docs::CHAT_FILE_NOT_NAMED);
    // Without a session an absolute path is no Docs path at all, as before.
    let bare = fixture
        .docs("read", None, &fixture.file("dev-8788.log"))
        .await;
    assert!(bare.get("file").is_none(), "{bare}");
    assert!(bare["error"].is_string(), "{bare}");
}

#[tokio::test]
async fn a_linked_binary_or_oversized_file_is_refused() {
    let fixture = fixture();
    for action in ["read", "stat"] {
        let binary = fixture
            .docs(action, Some(&fixture.session_id), &fixture.file("core.bin"))
            .await;
        assert_eq!(
            binary["error"],
            crate::project_docs::CHAT_FILE_NOT_TEXT,
            "{binary}"
        );
        let huge = fixture
            .docs(action, Some(&fixture.session_id), &fixture.file("huge.log"))
            .await;
        assert_eq!(
            huge["error"],
            crate::project_docs::CHAT_FILE_TOO_LARGE,
            "{huge}"
        );
    }
}

#[tokio::test]
async fn a_linked_file_still_cannot_be_written() {
    let fixture = fixture();
    let log = fixture.file("dev-8788.log");
    for action in ["save", "delete", "rename"] {
        let mut params = json!({
            "action": action,
            "requestId": "req-w",
            "projectId": fixture.project_id,
            "sessionId": fixture.session_id,
            "path": log,
            "newPath": "moved.log",
            "content": "overwritten",
        });
        params["scope"] = json!("project");
        let routed = route_http(
            fixture.state.clone(),
            rpc_request(
                "/api/runProjectDocsAction",
                &fixture.token,
                json!({ "params": params }),
            ),
            "docs-write".to_string(),
        )
        .await;
        let body = response_json(routed.response).await;
        assert!(body["result"]["error"].is_string(), "{action}: {body}");
    }
    assert_eq!(fs::read_to_string(&log).unwrap(), "listening on 8788\n");
}
