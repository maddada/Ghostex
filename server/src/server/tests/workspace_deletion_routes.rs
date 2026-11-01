//! Deleting a workspace through `/api/deleteWorkspace` also removes its ticket command and that
//! command's credentials, and only its own; a deletion that fails leaves the workspace, its
//! command and its credentials as they were. The command is set through the store here.

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::work_mode::{
    command_work_dir, read_credentials, read_work_sources, set_work_source,
    set_work_source_credentials,
};

struct Deletion {
    _temp: tempfile::TempDir,
    paths: GxserverPaths,
    state: Arc<AppState>,
    token: String,
    program: String,
}

impl Deletion {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().join("gx")));
        let state = test_app_state(paths.clone());
        let token = state.auth_token.clone();
        let program = temp.path().join("fixture-source");
        fs::write(
            &program,
            include_str!("../../work_mode/tests/command_source.sh"),
        )
        .expect("write fixture");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
        for source in [None, Some("fixture")] {
            let dir = command_work_dir(&paths, source).expect("work dir");
            fs::write(dir.join("mode.txt"), "good").expect("write mode");
        }
        Self {
            program: path_to_string(&program),
            _temp: temp,
            paths,
            state,
            token,
        }
    }

    async fn call(&self, path: &str, params: Value) -> (StatusCode, Value) {
        let response = route_http(
            self.state.clone(),
            rpc_request(path, &self.token, json!({ "params": params })),
            format!("request-{path}"),
        )
        .await;
        let status = response.response.status();
        (status, response_json(response.response).await)
    }

    /// A new workspace with the fixture as its command and a stored `token`.
    async fn workspace_with_command(&self, name: &str) -> String {
        let (status, body) = self
            .call("/api/createWorkspace", json!({ "name": name }))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let workspace_id = body["result"]["workspaceId"].as_str().unwrap().to_string();
        let db = open_gxserver_database(&self.paths).expect("db");
        let no_references = || Ok(BTreeSet::new());
        set_work_source(
            &db,
            &self.paths,
            &workspace_id,
            &self.program,
            false,
            &no_references,
        )
        .expect("pinned");
        set_work_source_credentials(
            &db,
            &self.paths,
            &workspace_id,
            json!({ "token": "tok-keep-3" }).as_object().unwrap(),
        )
        .expect("stored");
        workspace_id
    }

    async fn delete(&self, workspace_id: &str) -> (StatusCode, Value) {
        self.call(
            "/api/deleteWorkspace",
            json!({ "workspaceId": workspace_id }),
        )
        .await
    }

    /// Whether the workspace, its command and its command's credentials are all there.
    async fn all_there(&self, workspace_id: &str) -> bool {
        let (_, read) = self.call("/api/readWorkspaces", json!({})).await;
        let db = open_gxserver_database(&self.paths).expect("db");
        read["result"]["sidebarWorkspaces"]["workspaces"]
            .get(workspace_id)
            .is_some()
            && read_work_sources(&db)
                .expect("read")
                .workspaces
                .contains_key(workspace_id)
            && read_credentials(&self.paths)["workspaceSourceCredentials"][workspace_id]["fixture"]
                .get("token")
                .is_some()
    }
}

#[tokio::test]
async fn deleting_a_workspace_removes_only_its_own_command_and_credentials() {
    let deletion = Deletion::new();
    let gone = deletion.workspace_with_command("Gone").await;
    let kept = deletion.workspace_with_command("Kept").await;
    let (status, body) = deletion.delete(&gone).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let db = open_gxserver_database(&deletion.paths).expect("db");
    assert!(!read_work_sources(&db)
        .expect("read")
        .workspaces
        .contains_key(&gone));
    assert!(
        read_credentials(&deletion.paths)["workspaceSourceCredentials"]
            .get(&gone)
            .is_none()
    );
    assert!(deletion.all_there(&kept).await);
}

#[tokio::test]
async fn a_deletion_that_cant_write_the_workspaces_puts_the_command_back() {
    let deletion = Deletion::new();
    let workspace_id = deletion.workspace_with_command("Work").await;
    // Writing the workspaces document fails, after the command and its credentials were removed.
    let db = open_gxserver_database(&deletion.paths).expect("db");
    db.execute_batch(
        "CREATE TRIGGER refuse_workspaces BEFORE UPDATE ON metadata \
         WHEN NEW.key = 'sidebarWorkspaces' BEGIN SELECT RAISE(ABORT, 'refused for the test'); END;",
    )
    .expect("trigger");
    let (status, body) = deletion.delete(&workspace_id).await;
    assert_ne!(status, StatusCode::OK, "{body}");
    db.execute_batch("DROP TRIGGER refuse_workspaces;")
        .expect("drop");
    assert!(deletion.all_there(&workspace_id).await);
}

#[tokio::test]
async fn a_deletion_that_cant_remove_the_credentials_changes_nothing() {
    let deletion = Deletion::new();
    let workspace_id = deletion.workspace_with_command("Work").await;
    // The credentials file can't be replaced.
    let auth_dir = &deletion.paths.auth_dir;
    fs::set_permissions(auth_dir, fs::Permissions::from_mode(0o500)).expect("chmod");
    let (status, body) = deletion.delete(&workspace_id).await;
    fs::set_permissions(auth_dir, fs::Permissions::from_mode(0o700)).expect("chmod");
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(deletion.all_there(&workspace_id).await);
}

/// A deletion that can't read the credentials file answers with an error that names what is
/// wrong in fixed words: the answer and the request log never hold a key read from the file,
/// which in a damaged file can be any text, a credential among them.
#[tokio::test]
async fn a_deletion_error_never_repeats_a_key_from_the_credentials_file() {
    let deletion = Deletion::new();
    let workspace_id = deletion.workspace_with_command("Work").await;
    let file = deletion.paths.auth_dir.join("work-mode-credentials.json");
    for damaged in [
        r#"{"workspaceSourceCredentials":{"tok-key-leak-7":"fixture"}}"#.to_string(),
        format!(r#"{{"workspaceSourceCredentials":{{"{workspace_id}":{{"tok-src-leak-8":1}}}}}}"#),
    ] {
        fs::write(&file, &damaged).expect("damage");
        let response = handle_http_request(
            deletion.state.clone(),
            rpc_request(
                "/api/deleteWorkspace",
                &deletion.token,
                json!({ "params": { "workspaceId": workspace_id } }),
            ),
        )
        .await
        .expect("answered");
        assert_ne!(response.status(), StatusCode::OK);
        let body = response_json(response).await.to_string();
        assert!(body.contains("can't be read"), "{body}");
        let log = fs::read_to_string(&deletion.paths.log_file).unwrap_or_default();
        assert!(
            log.contains("isn't a JSON object"),
            "the error is logged: {log}"
        );
        for key in ["tok-key-leak-7", "tok-src-leak-8"] {
            assert!(!body.contains(key), "{key} in {body}");
            assert!(!log.contains(key), "{key} in the log");
        }
        assert_eq!(fs::read_to_string(&file).expect("read"), damaged);
    }
}
