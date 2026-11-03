//! The ticket command endpoints, called the way clients call them: only this computer's clients
//! may reach them, a team workspace can't set a command or its credentials, a secret value never
//! comes back, the generic workspace writes never pick a command as the tracker, deleting a
//! workspace removes its command, and a failed check logs one line with no command output.

use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::protocol::{endpoint_for, is_remote_endpoint_allowed, ApiPermission, ListenerKind};
use crate::work_mode::{
    command_work_dir, mutate_credentials, read_credentials, read_work_sources,
    PATH_SPELLS_CREDENTIAL, SAVED_BUT_SHORT_SECRET_UNREMOVABLE, SHORT_SECRET_REMOVED,
    SHORT_SECRET_UNREMOVABLE, SWITCH_OFFER,
};

struct Routes {
    _temp: tempfile::TempDir,
    paths: GxserverPaths,
    state: Arc<AppState>,
    token: String,
    program: String,
}

impl Routes {
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
        Self {
            program: path_to_string(&program),
            _temp: temp,
            paths,
            state,
            token,
        }
    }

    /// How the fixture answers before its `mode` credential is set.
    fn answer(&self, mode: &str) {
        for source in [None, Some("fixture")] {
            let dir = command_work_dir(&self.paths, source).expect("work dir");
            fs::write(dir.join("mode.txt"), mode).expect("write mode");
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

    /// `call` through the whole request path, request log included.
    async fn logged_call(&self, path: &str, params: Value) -> (StatusCode, Value) {
        let response = handle_http_request(
            self.state.clone(),
            rpc_request(path, &self.token, json!({ "params": params })),
        )
        .await
        .expect("answered");
        let status = response.status();
        (status, response_json(response).await)
    }

    async fn workspace(&self, name: &str) -> String {
        let (status, body) = self
            .call("/api/createWorkspace", json!({ "name": name }))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["result"]["workspaceId"].as_str().unwrap().to_string()
    }

    fn join_team(&self, workspace_id: &str) {
        mutate_credentials(&self.paths, |credentials| {
            credentials.insert(
                "teamSync".to_string(),
                json!({ workspace_id: {
                    "deploymentUrl": "https://team.example",
                    "memberToken": "member-token",
                } }),
            );
        })
        .expect("join");
    }
}

fn error_text(body: &Value) -> String {
    body["message"].as_str().unwrap_or_default().to_string()
}

#[test]
fn the_ticket_command_endpoints_are_for_this_computer_only() {
    for path in [
        "/api/readWorkSource",
        "/api/setWorkSource",
        "/api/setWorkSourceCredentials",
    ] {
        let endpoint = endpoint_for(path).expect(path);
        assert!(
            matches!(endpoint.permission, ApiPermission::FullLocal),
            "{path}"
        );
        assert!(!is_remote_endpoint_allowed(
            ListenerKind::Remote,
            endpoint.permission
        ));
        assert!(endpoint.requires_auth, "{path}");
    }
}

#[tokio::test]
async fn setting_a_command_shows_its_name_keys_and_absolute_path() {
    let routes = Routes::new();
    routes.answer("good");
    // Workspace generations are process-wide, so each test sets its own workspace's command.
    let workspace_id = routes.workspace("Work").await;
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let command = &body["result"]["command"];
    assert_eq!(body["result"]["workspaceId"], json!(workspace_id));
    assert_eq!(command["sourceId"], json!("fixture"));
    assert_eq!(command["name"], json!("Fixture"));
    assert_eq!(
        command["ids"],
        json!({ "format": "key-number", "keys": ["ACM", "OPS"] })
    );
    assert_eq!(command["resolvedPath"], json!(routes.program));
    assert!(command["icon"]
        .as_str()
        .unwrap()
        .starts_with("data:image/svg+xml;base64,"));
    assert_eq!(command["isTracker"], json!(false));
    let (_, read) = routes
        .call("/api/readWorkSource", json!({ "workspaceId": "Work" }))
        .await;
    assert_eq!(read["result"], body["result"]);
    // Without a workspace it is the default one's, which has none.
    let (_, default) = routes.call("/api/readWorkSource", json!({})).await;
    assert_eq!(default["result"]["workspaceId"], json!("personal"));
    assert_eq!(default["result"]["command"], Value::Null);
}

#[tokio::test]
async fn a_team_workspace_refuses_a_ticket_command() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = routes.workspace("Team work").await;
    let (status, _) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    routes.join_team(&workspace_id);
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(error_text(&body).contains("connected to a team"), "{body}");
    let (status, body) = routes
        .call(
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": workspace_id, "values": { "token": "tok-team" } }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(error_text(&body).contains("connected to a team"), "{body}");
    assert!(!read_credentials(&routes.paths).contains_key("workspaceSourceCredentials"));
    // It can still be read, and a command set before joining can be cleared.
    let (status, body) = routes
        .call(
            "/api/readWorkSource",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["team"], json!(true));
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["command"], Value::Null);
}

#[tokio::test]
async fn a_secret_never_comes_back() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = routes.workspace("Work").await;
    routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    let (status, body) = routes
        .call(
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": workspace_id, "values": { "mode": "good", "token": "tok-route-secret" } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.to_string().contains("tok-route-secret"), "{body}");
    let credentials = body["result"]["command"]["credentials"].as_array().unwrap();
    let field = |name: &str| {
        credentials
            .iter()
            .find(|field| field["name"] == json!(name))
            .unwrap()
            .clone()
    };
    assert_eq!(field("token")["set"], json!(true));
    assert!(field("token").get("value").is_none());
    assert_eq!(field("mode")["value"], json!("good"));
    let (_, read) = routes
        .call(
            "/api/readWorkSource",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert!(!read.to_string().contains("tok-route-secret"), "{read}");
    // Clearing the command keeps the credentials.
    routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": null }),
        )
        .await;
    assert!(
        read_credentials(&routes.paths)["workspaceSourceCredentials"][&workspace_id]["fixture"]
            .get("token")
            .is_some()
    );
}

#[tokio::test]
async fn generic_workspace_writes_refuse_a_command_tracker() {
    let routes = Routes::new();
    let (status, body) = routes
        .call(
            "/api/createWorkspace",
            json!({ "name": "Work", "tracker": "fixture" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(error_text(&body).contains("tracker must be"), "{body}");
    let workspace_id = routes.workspace("Work").await;
    let (status, body) = routes
        .call(
            "/api/updateWorkspace",
            json!({ "workspaceId": workspace_id, "tracker": "fixture" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = routes
        .call(
            "/api/updateWorkspace",
            json!({ "workspaceId": workspace_id, "tracker": "github" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = routes
        .call(
            "/api/updateWorkspace",
            json!({ "workspaceId": workspace_id, "tracker": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    // The refusal is those routes' own: the workspace write the local-only `setWorkTracker` goes
    // through refuses no tracker text.
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let current = crate::workspaces::read_sidebar_workspaces(&db).expect("workspaces");
    let patch = json!({ "tracker": "fixture" });
    assert!(crate::workspaces::update_workspace_in(
        &current,
        &workspace_id,
        patch.as_object().unwrap()
    )
    .is_ok());
}

#[tokio::test]
async fn deleting_a_workspace_removes_its_command_and_its_credentials() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = routes.workspace("Work").await;
    routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    routes
        .call(
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": workspace_id, "values": { "token": "tok-deleted" } }),
        )
        .await;
    let (status, body) = routes
        .call(
            "/api/deleteWorkspace",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let db = open_gxserver_database(&routes.paths).expect("db");
    let sources = read_work_sources(&db).expect("read");
    assert!(!sources.workspaces.contains_key(&workspace_id));
    assert_eq!(sources.retired[0].id, "fixture");
    assert!(!read_credentials(&routes.paths).contains_key("workspaceSourceCredentials"));
}

#[tokio::test]
async fn a_failed_check_logs_one_line_without_the_commands_output() {
    let routes = Routes::new();
    routes.answer("error-stderr");
    let workspace_id = routes.workspace("Work").await;
    let set = json!({ "workspaceId": workspace_id, "path": routes.program });
    // A failed check is an answer, not a request error: a request error's message goes to the
    // request log (`http_dispatch.rs`), and this one quotes the command's stderr.
    let (status, body) = routes.call("/api/setWorkSource", set.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let error = body["result"]["check"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(error.contains("can't reach the server"), "{body}");
    assert_eq!(body["result"]["command"], Value::Null);
    routes.answer("error-json");
    let (status, body) = routes.call("/api/setWorkSource", set).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["result"]["check"]["error"]
        .as_str()
        .is_some_and(|error| error.contains("was rejected")));
    let log = fs::read_to_string(&routes.paths.log_file).expect("log");
    let lines: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("work_source_failed"))
        .collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines[0].contains("ticket source command exited"),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains("\"status\":\"status 3\""), "{}", lines[0]);
    assert!(
        lines[1].contains("ticket source authentication failed"),
        "{}",
        lines[1]
    );
    assert!(!log.contains("can't reach the server"), "{log}");
    assert!(!log.contains("was rejected"), "{log}");
}

#[tokio::test]
async fn a_failed_check_never_reaches_the_request_log() {
    let routes = Routes::new();
    routes.answer("error-stderr");
    let workspace_id = routes.workspace("Work").await;
    let response = handle_http_request(
        routes.state.clone(),
        rpc_request(
            "/api/setWorkSource",
            &routes.token,
            json!({ "params": { "workspaceId": workspace_id, "path": routes.program } }),
        ),
    )
    .await
    .expect("answered");
    assert_eq!(response.status(), StatusCode::OK);
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(!log.contains("can't reach the server"), "{log}");
}

/// `/api/setWorkSource` through the handler that writes the request log, as a client sends it.
async fn set_logged(routes: &Routes, params: Value) -> (StatusCode, Value) {
    let response = handle_http_request(
        routes.state.clone(),
        rpc_request(
            "/api/setWorkSource",
            &routes.token,
            json!({ "params": params }),
        ),
    )
    .await
    .expect("answered");
    let status = response.status();
    (status, response_json(response).await)
}

/// A workspace with the fixture set and `token` stored, secret.
async fn with_token(routes: &Routes, name: &str, token: &str) -> String {
    let workspace_id = routes.workspace(name).await;
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = routes
        .call(
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": workspace_id, "values": { "token": token } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    workspace_id
}

#[tokio::test]
async fn a_path_holding_a_stored_credential_is_refused_without_showing_it() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Work", "abc123").await;
    for path in ["bin/abc123", "/opt/abc123/tracker"] {
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": path }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        // The refusal quotes the typed path through the mask.
        let masked = format!(
            "\"{}\" {PATH_SPELLS_CREDENTIAL}",
            path.replace("abc123", "••••")
        );
        assert_eq!(error_text(&body), masked, "{path}");
        let logged = logged_errors(&routes);
        assert!(
            logged
                .last()
                .is_some_and(|logged| says_only(logged, &masked)),
            "{logged:?}"
        );
    }
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(!log.contains("abc123"), "{log}");
    assert!(log.contains("••••"), "{log}");
    assert_eq!(
        read_work_sources(&open_gxserver_database(&routes.paths).expect("db"))
            .expect("read")
            .workspaces[&workspace_id]
            .path,
        routes.program
    );
}

/// Every refusal of a set leaves through the workspace's stored values, like a credential save's:
/// here the team refusal, with `team` stored, and the changed-id refusal, with `links` stored.
#[tokio::test]
async fn a_set_refusal_never_shows_a_stored_value() {
    let routes = Routes::new();
    routes.answer("good");
    let team = with_token(&routes, "Team work", "team").await;
    routes.join_team(&team);
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": team, "path": routes.program }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error_text(&body),
        "A workspace connected to a •••• uses its ••••'s tracker; ticket commands are for this computer's workspaces."
    );

    let renamed = with_token(&routes, "Work", "links").await;
    routes.answer("info-renamed");
    for params in [
        json!({ "workspaceId": renamed, "path": routes.program }),
        json!({ "workspaceId": renamed, "check": true }),
    ] {
        let (status, body) = set_logged(&routes, params).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(
            error_text(&body),
            format!("This command now calls itself \"renamed\"; this workspace's •••• use \"fixture\". {SWITCH_OFFER}")
        );
    }
    let logged = logged_errors(&routes);
    assert_eq!(logged.len(), 3, "{logged:?}");
    for error in &logged {
        assert!(
            !error.contains("team") && !error.contains("links"),
            "{error}"
        );
    }
}

/// A program written next to the fixture.
fn program_beside(routes: &Routes, name: &str, body: &str) -> String {
    let program = Path::new(&routes.program).with_file_name(name);
    fs::write(&program, format!("#!/bin/sh\n{body}\n")).expect("write");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
    path_to_string(&program)
}

/// After a clear the workspace keeps `token`, and the command isn't given it. A failing `info`
/// that prints the value anyway is masked with what the workspace keeps, in the answer and in the
/// request log, wherever it prints it.
#[tokio::test]
async fn a_failed_check_after_a_clear_never_shows_a_kept_value() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Work", "abc123").await;
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for (name, body) in [
        (
            "error-object",
            r#"printf '{ "error": { "code": "down", "message": "cannot reach abc123" } }\n'; exit 1"#,
        ),
        ("stderr", "echo 'cannot reach abc123' >&2; exit 3"),
        ("unreadable-stdout", "echo 'not an answer: abc123'"),
    ] {
        let program = program_beside(&routes, name, body);
        let (status, answer) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}: {answer}");
        assert!(
            answer["result"]["check"]["error"].is_string(),
            "{name}: {answer}"
        );
        assert!(!answer.to_string().contains("abc123"), "{name}: {answer}");
    }
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(
        log.contains("work_source_failed"),
        "the failures are logged: {log}"
    );
    assert!(!log.contains("abc123"), "{log}");
}

/// A program in `folder` beside the fixture that answers `info` with `open_id` and `token` not
/// secret, and once a `closed` file sits next to it, with `closed_id` and `token` secret.
fn closing_source(
    routes: &Routes,
    folder: &str,
    open_id: &str,
    closed_id: &str,
) -> (String, PathBuf) {
    let folder = Path::new(&routes.program).with_file_name(folder);
    fs::create_dir_all(&folder).expect("folder");
    let program = folder.join("tracker");
    fs::write(
        &program,
        format!(
            r#"#!/bin/sh
here=$(dirname "$0")
touch "$here/ran"
if [ -e "$here/closed" ]; then secret=true; id={closed_id}; else secret=false; id={open_id}; fi
printf '{{ "id": "%s", "name": "Opener", "ids": {{ "format": "number" }}, "credentials": [ {{ "name": "token", "label": "Token", "secret": %s }} ] }}\n' "$id" "$secret"
"#
        ),
    )
    .expect("write");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
    (path_to_string(&program), folder)
}

async fn read_source(routes: &Routes, workspace_id: &str) -> Value {
    let (status, body) = routes
        .call(
            "/api/readWorkSource",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn save_logged(routes: &Routes, workspace_id: &str, values: Value) -> (StatusCode, Value) {
    let response = handle_http_request(
        routes.state.clone(),
        rpc_request(
            "/api/setWorkSourceCredentials",
            &routes.token,
            json!({ "params": { "workspaceId": workspace_id, "values": values } }),
        ),
    )
    .await
    .expect("answered");
    let status = response.status();
    (status, response_json(response).await)
}

/// `token` is stored while declared not secret: a path that holds it is refused before it runs,
/// and a check whose `info` then declares it secret finds it nowhere. The other order: with the
/// path set first, the value is refused when saved.
#[tokio::test]
async fn a_value_that_isnt_secret_and_a_path_never_meet() {
    let routes = Routes::new();
    let (open, open_folder) = closing_source(&routes, "open", "opener", "opener");
    let (spelled, spelled_folder) = closing_source(&routes, "abc123", "opener", "opener");
    let workspace_id = routes.workspace("Work").await;
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": workspace_id, "path": open }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "abc123" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": workspace_id, "path": spelled }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // Not secret yet, so the path is quoted as typed.
    assert_eq!(
        error_text(&body),
        format!("\"{spelled}\" {PATH_SPELLS_CREDENTIAL}")
    );
    assert!(!spelled_folder.join("ran").exists(), "it never ran");

    fs::write(open_folder.join("closed"), "").expect("closed");
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": workspace_id, "check": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for body in [body, read_source(&routes, &workspace_id).await] {
        let command = &body["result"]["command"];
        assert_eq!(command["path"], json!(open), "{body}");
        assert_eq!(command["credentials"][0]["secret"], json!(true), "{body}");
        assert_eq!(command["credentials"][0]["set"], json!(true), "{body}");
        assert!(!body.to_string().contains("abc123"), "{body}");
    }

    // The other order, in another workspace.
    let other = routes.workspace("Other").await;
    let (status, body) =
        set_logged(&routes, json!({ "workspaceId": other, "path": spelled })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = save_logged(&routes, &other, json!({ "token": "abc123" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error_text(&body),
        "token can't be saved: its value also appears in the command's path, which Ghostex can't hide."
    );
    fs::write(spelled_folder.join("closed"), "").expect("closed");
    let (status, body) = set_logged(&routes, json!({ "workspaceId": other, "check": true })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let body = read_source(&routes, &other).await;
    assert_eq!(
        body["result"]["command"]["credentials"][0]["secret"],
        json!(true)
    );
    assert_eq!(
        body["result"]["command"]["credentials"][0]["set"],
        json!(false)
    );

    let logged = logged_errors(&routes);
    assert_eq!(logged.len(), 2, "{logged:?}");
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(!log.contains("abc123"), "{log}");
}

/// The same after a credential save whose own check declares `token` secret and answers with
/// another id (the record is tightened, and the id offered): the value is never in the path.
#[tokio::test]
async fn a_credential_check_that_makes_a_value_secret_finds_none_in_the_path() {
    let routes = Routes::new();
    let (open, open_folder) = closing_source(&routes, "open", "opener", "closer");
    let (spelled, _) = closing_source(&routes, "abc123", "opener", "closer");
    let workspace_id = routes.workspace("Work").await;
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": workspace_id, "path": open }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "abc123" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": workspace_id, "path": spelled }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // Not secret yet, so the path is quoted as typed.
    assert_eq!(
        error_text(&body),
        format!("\"{spelled}\" {PATH_SPELLS_CREDENTIAL}")
    );

    fs::write(open_folder.join("closed"), "").expect("closed");
    let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "abc123" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for body in [body, read_source(&routes, &workspace_id).await] {
        let command = &body["result"]["command"];
        assert_eq!(command["path"], json!(open), "{body}");
        assert_eq!(command["offeredId"], json!("closer"), "{body}");
        assert_eq!(command["credentials"][0]["secret"], json!(true), "{body}");
        assert!(!body.to_string().contains("abc123"), "{body}");
    }
    let logged = logged_errors(&routes);
    assert_eq!(logged.len(), 1, "{logged:?}");
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(!log.contains("abc123"), "{log}");
}

/// A value shorter than 3 characters isn't kept out of the path. Saved while not secret, it sits
/// in the path; once an accepted `info` declares it secret, through a check, a set or a credential
/// save's check, it is removed, since it can't be kept secret. The answer and `lastError` say so,
/// and the path, which no credential is in any more, is shown as it is.
#[tokio::test]
async fn a_short_value_an_accepted_answer_makes_secret_is_removed() {
    let routes = Routes::new();
    for how in ["check", "set", "save"] {
        let (program, folder) = closing_source(&routes, &format!("ж-{how}"), "opener", "opener");
        let workspace_id = routes.workspace(how).await;
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "ж" })).await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        assert_eq!(body["result"]["command"]["path"], json!(program), "{how}");

        fs::write(folder.join("closed"), "").expect("closed");
        let (status, body) = match how {
            "check" => {
                set_logged(
                    &routes,
                    json!({ "workspaceId": workspace_id, "check": true }),
                )
                .await
            }
            "set" => {
                set_logged(
                    &routes,
                    json!({ "workspaceId": workspace_id, "path": program }),
                )
                .await
            }
            _ => save_logged(&routes, &workspace_id, json!({ "token": "ж" })).await,
        };
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        for body in [body, read_source(&routes, &workspace_id).await] {
            let command = &body["result"]["command"];
            assert_eq!(command["path"], json!(program), "{how}: {body}");
            assert_eq!(command["lastError"], json!(SHORT_SECRET_REMOVED), "{how}");
            let token = &command["credentials"][0];
            assert_eq!(token["secret"], json!(true), "{how}: {body}");
            assert_eq!(token["set"], json!(false), "{how}: {body}");
        }
    }
    assert!(!Value::Object(read_credentials(&routes.paths))
        .to_string()
        .contains('ж'));
    assert_eq!(logged_errors(&routes), Vec::<String>::new());
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(!log.contains('ж'), "{log}");
}

/// A value too short to be kept out of the pinned id (`zq` in `zq-source`), saved while not
/// secret. An `info` that answers with another id and declares it secret is refused (a check, a
/// set without `--switch`, a credential save's own check) and still removes the value, saying so
/// first. A value that short is never hidden, so the refusal, `lastError` and the request log show
/// the pinned id as it is.
#[tokio::test]
async fn a_refused_answer_removes_a_short_value_it_makes_secret() {
    let refusal = format!(
        "{SHORT_SECRET_REMOVED} This command now calls itself \"other\"; this workspace's links use \"zq-source\". {SWITCH_OFFER}"
    );
    let routes = Routes::new();
    for how in ["check", "set", "save"] {
        let (program, folder) =
            closing_source(&routes, &format!("pinned-{how}"), "zq-source", "other");
        let workspace_id = routes.workspace(how).await;
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "zq" })).await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        assert_eq!(
            body["result"]["command"]["sourceId"],
            json!("zq-source"),
            "{how}"
        );

        fs::write(folder.join("closed"), "").expect("closed");
        let last_error = match how {
            "save" => {
                let (status, body) =
                    save_logged(&routes, &workspace_id, json!({ "token": "zq" })).await;
                assert_eq!(status, StatusCode::OK, "{how}: {body}");
                let last_error = format!(
                    "{SHORT_SECRET_REMOVED} This command now calls itself \"other\"; this workspace's links use \"zq-source\"."
                );
                assert_eq!(
                    body["result"]["command"]["lastError"],
                    json!(last_error),
                    "{how}"
                );
                last_error
            }
            _ => {
                let params = match how {
                    "check" => json!({ "workspaceId": workspace_id, "check": true }),
                    _ => json!({ "workspaceId": workspace_id, "path": program }),
                };
                let (status, body) = set_logged(&routes, params).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{how}: {body}");
                assert_eq!(error_text(&body), refusal, "{how}");
                refusal.clone()
            }
        };
        let body = read_source(&routes, &workspace_id).await;
        let command = &body["result"]["command"];
        assert_eq!(command["lastError"], json!(last_error), "{how}: {body}");
        assert_eq!(command["offeredId"], json!("other"), "{how}: {body}");
        assert_eq!(command["sourceId"], json!("zq-source"), "{how}: {body}");
        let token = &command["credentials"][0];
        assert_eq!(token["secret"], json!(true), "{how}: {body}");
        assert_eq!(token["set"], json!(false), "{how}: {body}");
    }
    assert!(!Value::Object(read_credentials(&routes.paths))
        .to_string()
        .contains(r#""zq""#));
    let logged = logged_errors(&routes);
    assert_eq!(logged.len(), 2, "{logged:?}");
    assert!(
        logged.iter().all(|logged| says_only(logged, &refusal)),
        "{logged:?}"
    );
}

/// A program in `folder` beside the fixture that prints `open` (exiting 0), and once a `closed`
/// file sits next to it prints `closed` and exits with `closed_status`.
fn two_faced_source(
    routes: &Routes,
    folder: &str,
    open: &str,
    closed: &str,
    closed_status: u8,
) -> (String, PathBuf) {
    let folder = Path::new(&routes.program).with_file_name(folder);
    fs::create_dir_all(&folder).expect("folder");
    let program = folder.join("tracker");
    fs::write(
        &program,
        format!(
            "#!/bin/sh\nif [ -e \"$(dirname \"$0\")/closed\" ]; then\ncat <<'EOF'\n{closed}\nEOF\nexit {closed_status}\nfi\ncat <<'EOF'\n{open}\nEOF\n"
        ),
    )
    .expect("write");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
    (path_to_string(&program), folder)
}

/// `zq`, saved while not secret for `zq-source`, is kept when no record declares its field: after
/// a clear, or for the old id after a Switch, or once the pinned record stops listing it. An
/// `info` that then declares it secret removes it, since it can't be kept secret, and says so
/// first: a set after a clear whose answer changes the id format, the same set failing with an
/// error that prints it, a credential save's own check answering with the old id, and a check or
/// a save's check answering with another id. A value that short is never hidden, so each says
/// what it says with `zq` as it is; a short value whose field stays not secret is kept.
#[tokio::test]
async fn a_short_value_kept_by_the_workspace_is_removed_once_an_answer_makes_it_secret() {
    let routes = Routes::new();
    let open = r#"{ "id": "zq-source", "name": "Zq", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "Token", "secret": false } ] }"#;
    let changed_format = r#"{ "id": "zq-source", "name": "Zq", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "token", "label": "Token" } ] }"#;
    let failing = r#"{ "error": { "code": "down", "message": "cannot reach zq" }, "credentials": [ { "name": "token", "label": "Token" } ] }"#;
    let mut answers = Vec::new();
    for (how, closed, status) in [("format", changed_format, 0), ("failing", failing, 1)] {
        let (program, folder) =
            two_faced_source(&routes, &format!("kept-{how}"), open, closed, status);
        let workspace_id = routes.workspace(how).await;
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "zq" })).await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": null }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        fs::write(folder.join("closed"), "").expect("closed");
        let (_, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
        answers.push((how, body));
        answers.push((how, read_source(&routes, &workspace_id).await));
    }

    // After a Switch to `other`, a credential save's check answers with the old id again.
    let other = r#"{ "id": "other", "name": "Other", "ids": { "format": "number" }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false } ] }"#;
    let back = r#"{ "id": "zq-source", "name": "Zq", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "Token" }, { "name": "mode", "label": "Mode", "secret": false } ] }"#;
    let (first, _) = two_faced_source(&routes, "kept-switch-first", open, open, 0);
    let (second, folder) = two_faced_source(&routes, "kept-switch-second", other, back, 0);
    let workspace_id = routes.workspace("switch").await;
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": workspace_id, "path": first }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "zq" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": workspace_id, "path": second, "switch": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["command"]["sourceId"], json!("other"));
    fs::write(folder.join("closed"), "").expect("closed");
    let (status, body) = save_logged(&routes, &workspace_id, json!({ "mode": "plain" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    answers.push(("switch", body));
    answers.push(("switch", read_source(&routes, &workspace_id).await));

    // The pinned record stops declaring `token`, so its value is kept. A check, or a credential
    // save's own check, answering with another id then declares it secret, and what it words
    // quotes the pinned id. The value is removed, so the summary then shows that id as it is.
    for how in ["check", "save"] {
        let answer = Path::new(&routes.program).with_file_name(format!("kept-dropped-{how}.json"));
        fs::write(&answer, open).expect("answer");
        let program = program_beside(
            &routes,
            &format!("kept-dropped-{how}"),
            &format!("cat '{}'", answer.display()),
        );
        let workspace_id = routes.workspace(&format!("dropped-{how}")).await;
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "zq" })).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let mode = r#"{ "name": "mode", "label": "Mode", "secret": false }"#;
        fs::write(
            &answer,
            format!(r#"{{ "id": "zq-source", "name": "Zq", "ids": {{ "format": "number" }}, "credentials": [ {mode} ] }}"#),
        )
        .expect("answer");
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "check": true }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        fs::write(
            &answer,
            format!(r#"{{ "id": "renamed", "name": "Zq", "ids": {{ "format": "number" }}, "credentials": [ {{ "name": "token", "label": "Token" }}, {mode} ] }}"#),
        )
        .expect("answer");
        let worded = if how == "check" {
            let (status, body) = set_logged(
                &routes,
                json!({ "workspaceId": workspace_id, "check": true }),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            answers.push(("dropped", body));
            format!("{SHORT_SECRET_REMOVED} This command now calls itself \"renamed\"; this workspace's links use \"zq-source\". {SWITCH_OFFER}")
        } else {
            let (status, body) = save_logged(&routes, &workspace_id, json!({ "mode": "p" })).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            format!("{SHORT_SECRET_REMOVED} This command now calls itself \"renamed\"; this workspace's links use \"zq-source\".")
        };
        let body = read_source(&routes, &workspace_id).await;
        let command = &body["result"]["command"];
        assert_eq!(command["lastError"], json!(worded), "{how}: {body}");
        assert_eq!(command["sourceId"], json!("zq-source"), "{how}: {body}");
        if how == "save" {
            let mode = &command["credentials"][0];
            assert_eq!(mode["name"], json!("mode"), "{body}");
            assert_eq!(mode["value"], json!("p"), "{body}");
        }
    }

    let said = [
        "\"zq-source\" sends key-number ids now, and this computer already has it with number ids. Clear it everywhere to change its id format.".to_string(),
        String::new(),
        "The ticket command: cannot reach zq".to_string(),
        String::new(),
        "This command now calls itself \"zq-source\"; this workspace's links use \"other\".".to_string(),
        "This command now calls itself \"zq-source\"; this workspace's links use \"other\".".to_string(),
        format!(
            "This command now calls itself \"renamed\"; this workspace's links use \"zq-source\". {SWITCH_OFFER}"
        ),
    ];
    assert_eq!(answers.len(), said.len());
    for ((how, body), said) in answers.iter().zip(said) {
        let shown = [
            &body["result"]["check"]["error"],
            &body["result"]["command"]["lastError"],
            &body["message"],
        ];
        if !said.is_empty() {
            let said = json!(format!("{SHORT_SECRET_REMOVED} {said}"));
            assert!(shown.contains(&&said), "{how}: {body}");
        }
    }
    assert!(!Value::Object(read_credentials(&routes.paths))
        .to_string()
        .contains(r#""zq""#));
}

/// `zqx`, saved while not secret for `zq-source`, is kept only in storage once a Switch moves the
/// workspace to `other`: the command isn't given it. A failing `info` that declares its field
/// secret and prints it, on a check and on a credential save's own check, shows it masked in the
/// answer, `lastError`, the request log and the failure log line, since a stored value of 3 or
/// more characters whose field no record declares is kept as secret (`stored_values`). It is long
/// enough to keep secret, so it stays stored.
#[tokio::test]
async fn a_failing_answer_masks_a_value_kept_only_in_storage() {
    let routes = Routes::new();
    let open = r#"{ "id": "zq-source", "name": "Zq", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "Token", "secret": false } ] }"#;
    let other = r#"{ "id": "other", "name": "Other", "ids": { "format": "number" }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false } ] }"#;
    let failing = r#"{ "error": { "code": "down", "message": "cannot reach zqx" }, "credentials": [ { "name": "token", "label": "Token" } ] }"#;
    for how in ["check", "save"] {
        let (first, _) = two_faced_source(&routes, &format!("long-first-{how}"), open, open, 0);
        let (second, folder) =
            two_faced_source(&routes, &format!("long-second-{how}"), other, failing, 1);
        let workspace_id = routes.workspace(how).await;
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": first }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "zqx" })).await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": second, "switch": true }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        assert_eq!(
            body["result"]["command"]["sourceId"],
            json!("other"),
            "{how}"
        );

        fs::write(folder.join("closed"), "").expect("closed");
        let (status, body) = match how {
            "check" => {
                set_logged(
                    &routes,
                    json!({ "workspaceId": workspace_id, "check": true }),
                )
                .await
            }
            _ => save_logged(&routes, &workspace_id, json!({ "mode": "plain" })).await,
        };
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let error = body["result"]["check"]["error"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert_eq!(error, "other: cannot reach ••••", "{how}: {body}");
        assert!(!body.to_string().contains("zqx"), "{how}: {body}");
        let after = read_source(&routes, &workspace_id).await;
        assert_eq!(
            after["result"]["command"]["lastError"],
            json!(error),
            "{how}: {after}"
        );
        assert!(!after.to_string().contains("zqx"), "{how}: {after}");
    }
    assert!(Value::Object(read_credentials(&routes.paths))
        .to_string()
        .contains(r#""zqx""#));
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    let failures = log
        .lines()
        .filter(|line| line.contains("work_source_failed"))
        .count();
    assert_eq!(failures, 2, "{log}");
    // The failure log lines' fixed summaries are the only errors logged.
    assert_eq!(
        logged_errors(&routes),
        vec!["ticket source reported an error"; 2]
    );
    assert!(!log.contains("zqx"), "{log}");
}

/// A short value an answer makes secret while the credentials file can't be written (the command
/// makes its folder read-only as it answers). A check is refused in fixed words and changes
/// nothing, the value and the record's flags included. A credential save wrote its values before
/// its check ran: `xy` replaces `zq` and stays, the check's answer is dropped, so the record isn't
/// tightened and `xy` shows as not secret, and the answer and `lastError` say so in fixed words.
/// An id offered earlier for a Switch is left alone by both.
#[tokio::test]
async fn a_short_value_that_cant_be_removed_drops_the_answer() {
    let routes = Routes::new();
    let auth = &routes.paths.auth_dir;
    let closed = Path::new(&routes.program).with_file_name("unremovable-closed");
    let offer = Path::new(&routes.program).with_file_name("unremovable-offer");
    let program = program_beside(
        &routes,
        "unremovable",
        &format!(
            r#"id=opener; secret=false
if [ -e '{}' ]; then chmod 500 '{}'; secret=true; elif [ -e '{}' ]; then id=renamed; fi
printf '{{ "id": "%s", "name": "Opener", "ids": {{ "format": "number" }}, "credentials": [ {{ "name": "token", "label": "Token", "secret": %s }} ] }}\n' "$id" "$secret""#,
            closed.display(),
            auth.display(),
            offer.display()
        ),
    );
    for how in ["check", "save"] {
        let _ = fs::remove_file(&closed);
        let _ = fs::remove_file(&offer);
        let workspace_id = routes.workspace(how).await;
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        let (status, body) = save_logged(&routes, &workspace_id, json!({ "token": "zq" })).await;
        assert_eq!(status, StatusCode::OK, "{how}: {body}");
        fs::write(&offer, "").expect("offer");
        let (status, body) = set_logged(
            &routes,
            json!({ "workspaceId": workspace_id, "check": true }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{how}: {body}");
        fs::remove_file(&offer).expect("no offer");
        let before = read_source(&routes, &workspace_id).await;
        let offered = &before["result"]["command"]["offeredId"];
        assert_eq!(offered, &json!("renamed"), "{how}: {before}");

        fs::write(&closed, "").expect("closed");
        let (status, body) = match how {
            "check" => {
                set_logged(
                    &routes,
                    json!({ "workspaceId": workspace_id, "check": true }),
                )
                .await
            }
            _ => save_logged(&routes, &workspace_id, json!({ "token": "xy" })).await,
        };
        fs::set_permissions(auth, fs::Permissions::from_mode(0o700)).expect("writable");
        let after = read_source(&routes, &workspace_id).await;
        let token = &after["result"]["command"]["credentials"][0];
        assert_eq!(token["secret"], json!(false), "{how}: {after}");
        assert_eq!(
            &after["result"]["command"]["offeredId"], offered,
            "{how}: {after}"
        );
        if how == "check" {
            assert_eq!(status, StatusCode::BAD_REQUEST, "{how}: {body}");
            assert_eq!(error_text(&body), SHORT_SECRET_UNREMOVABLE, "{how}");
            assert_eq!(after, before, "{how}");
            assert_eq!(token["value"], json!("zq"), "{how}: {after}");
        } else {
            assert_eq!(status, StatusCode::OK, "{how}: {body}");
            let last_error = json!(SAVED_BUT_SHORT_SECRET_UNREMOVABLE);
            assert_eq!(body["result"]["command"]["lastError"], last_error, "{body}");
            assert_eq!(body["result"]["check"], Value::Null, "{body}");
            assert_eq!(
                after["result"]["command"]["lastError"], last_error,
                "{after}"
            );
            assert_eq!(token["value"], json!("xy"), "{after}");
            assert_eq!(token["set"], json!(true), "{after}");
        }
    }
}

/// A workspace named like the credential it stores, renamed: the stale name sent to each endpoint
/// is answered in fixed words, never repeated in the answer or the request log.
#[tokio::test]
async fn an_unknown_workspace_is_never_repeated_back() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "abc123", "abc123").await;
    let (status, body) = routes
        .call(
            "/api/updateWorkspace",
            json!({ "workspaceId": workspace_id, "name": "Work" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for (path, params) in [
        ("/api/readWorkSource", json!({ "workspaceId": "abc123" })),
        (
            "/api/setWorkSource",
            json!({ "workspaceId": "abc123", "path": routes.program }),
        ),
        (
            "/api/setWorkSource",
            json!({ "workspaceId": "abc123", "check": true }),
        ),
        (
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": "abc123", "values": { "token": "tok-new-9" } }),
        ),
    ] {
        let response = handle_http_request(
            routes.state.clone(),
            rpc_request(path, &routes.token, json!({ "params": params })),
        )
        .await
        .expect("answered");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let body = response_json(response).await;
        assert_eq!(
            error_text(&body),
            "No workspace matched that id or name.",
            "{path}"
        );
        assert!(!body.to_string().contains("abc123"), "{path}: {body}");
    }
    let logged = logged_errors(&routes);
    assert_eq!(logged.len(), 4, "{logged:?}");
    for error in &logged {
        assert!(!error.contains("abc123"), "{error}");
    }
}

/// The tracker and workspace routes answer a workspace they can't find in the same fixed words,
/// never repeating the typed reference in the answer or the request log.
#[tokio::test]
async fn the_tracker_and_workspace_routes_name_no_typed_workspace() {
    let routes = Routes::new();
    let typed = "ws-typed-ref-55";
    let calls = [
        ("/api/readWorkTracker", json!({ "workspaceId": typed })),
        (
            "/api/setWorkTracker",
            json!({ "workspaceId": typed, "tracker": "github" }),
        ),
        (
            "/api/updateWorkspace",
            json!({ "workspaceId": typed, "name": "Work" }),
        ),
        ("/api/deleteWorkspace", json!({ "workspaceId": typed })),
        (
            "/api/moveMachineToWorkspace",
            json!({ "workspaceId": typed, "machineId": "machine-1" }),
        ),
    ];
    for (path, params) in &calls {
        let (status, body) = routes.logged_call(path, params.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
        assert_eq!(
            error_text(&body),
            "No workspace matched that id or name.",
            "{path}"
        );
        assert!(!body.to_string().contains(typed), "{path}: {body}");
    }
    let logged = logged_errors(&routes);
    assert_eq!(logged.len(), calls.len(), "{logged:?}");
    for error in &logged {
        assert!(!error.contains(typed), "{error}");
    }
}

/// A ticket id that isn't one of the command's is refused without quoting it: the id typed here is
/// the workspace's stored credential, and every request error's message is logged.
#[tokio::test]
async fn a_typed_command_ticket_id_is_never_repeated_back() {
    let routes = Routes::new();
    routes.answer("good");
    with_token(&routes, "Work", "tok-typed-77").await;
    let (status, body) = routes
        .logged_call(
            "/api/readWorkItem",
            json!({ "source": "fixture", "ticketId": "tok-typed-77" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error_text(&body),
        "That isn't a fixture ticket ID: a key and a number, like ABC-12."
    );
    assert!(!body.to_string().contains("tok-typed-77"), "{body}");
    let logged = logged_errors(&routes);
    assert!(
        logged.iter().any(|error| error.contains("ticket ID")),
        "the refusal is in the request log: {logged:?}"
    );
    assert!(
        !fs::read_to_string(&routes.paths.log_file)
            .unwrap_or_default()
            .contains("tok-typed-77"),
        "{logged:?}"
    );
}

#[tokio::test]
async fn a_check_runs_on_the_server_with_the_path_it_has() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = routes.workspace("Work").await;
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "check": true }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error_text(&body),
        "This workspace has no ticket command to check."
    );
    // Another client's path is what a later check runs.
    let other = format!("{}-copy", routes.program);
    fs::copy(&routes.program, &other).expect("copy");
    for path in [&routes.program, &other] {
        let (status, body) = routes
            .call(
                "/api/setWorkSource",
                json!({ "workspaceId": workspace_id, "path": path }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "check": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["command"]["path"], json!(other));
    // A check names no path, and a path is never a check.
    for params in [
        json!({ "workspaceId": workspace_id, "check": true, "path": other }),
        json!({ "workspaceId": workspace_id }),
    ] {
        let (status, body) = routes.call("/api/setWorkSource", params).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn a_secret_too_short_to_keep_is_refused_without_its_value_reaching_the_log() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = routes.workspace("Work").await;
    routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    let save = |values: Value| {
        handle_http_request(
            routes.state.clone(),
            rpc_request(
                "/api/setWorkSourceCredentials",
                &routes.token,
                json!({ "params": { "workspaceId": workspace_id, "values": values } }),
            ),
        )
    };
    for value in ["ж", " ж€ "] {
        let response = save(json!({ "token": value })).await.expect("answered");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{value:?}");
        let body = response_json(response).await;
        assert!(
            error_text(&body).starts_with("API token: a value this short can't be kept secret"),
            "{body}"
        );
        assert!(!body.to_string().contains('ж'), "{body}");
    }
    let kept = save(json!({ "token": "ж€✓" })).await.expect("answered");
    assert_eq!(kept.status(), StatusCode::OK);
    // `mode` is declared not secret.
    let open = save(json!({ "mode": "§" })).await.expect("answered");
    assert_eq!(open.status(), StatusCode::OK);
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(
        log.contains("value this short"),
        "the refusal is logged: {log}"
    );
    assert!(!log.contains('ж') && !log.contains('€'), "{log}");
}

/// The errors the request log has for refused requests, oldest first.
fn logged_errors(routes: &Routes) -> Vec<String> {
    fs::read_to_string(&routes.paths.log_file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|entry| entry["error"].as_str().map(str::to_string))
        .collect()
}

/// Whether a logged error says nothing but `answer`'s words, in order: the log's own redaction
/// may replace some of them with `[redacted:…]`, never add any.
fn says_only(logged: &str, answer: &str) -> bool {
    let mut words = answer.split_whitespace();
    logged
        .strip_prefix("badRequest: ")
        .unwrap_or(logged)
        .split_whitespace()
        .filter(|word| !word.starts_with("[redacted:"))
        .all(|word| words.any(|said| said == word))
}

#[tokio::test]
async fn a_refusal_masks_a_value_it_can_and_says_the_same_for_any_short_one() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = routes.workspace("Work").await;
    routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    let refusal = |values: Value| {
        let routes = &routes;
        let workspace_id = workspace_id.clone();
        async move {
            let response = handle_http_request(
                routes.state.clone(),
                rpc_request(
                    "/api/setWorkSourceCredentials",
                    &routes.token,
                    json!({ "params": { "workspaceId": workspace_id, "values": values } }),
                ),
            )
            .await
            .expect("answered");
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            error_text(&response_json(response).await)
        }
    };
    // No text can hide a one-letter value: a short secret gets one fixed sentence, the same for
    // every such value, in the answer and in the log.
    let short = refusal(json!({ "token": "A" })).await;
    assert_eq!(
        short,
        "API token: a value this short can't be kept secret; a secret needs at least 3 characters. If it isn't secret, the command's info should mark it \"secret\": false."
    );
    assert_eq!(refusal(json!({ "token": "zz" })).await, short);
    // A value that can be masked is, from the moment it arrives: here it is its own field's name.
    let named = refusal(json!({ "token": "token" })).await;
    assert!(
        named.ends_with(" can't be saved: its value also appears in the command's credential names, which Ghostex can't hide."),
        "{named}"
    );
    assert!(!named.contains("token"), "{named}");
    let logged = logged_errors(&routes);
    assert_eq!(logged.len(), 3, "{logged:?}");
    assert_eq!(logged[0], logged[1]);
    assert!(says_only(&logged[0], &short), "{logged:?}");
    assert!(says_only(&logged[2], &named), "{logged:?}");
    assert!(!logged[2].contains("token"), "{logged:?}");
}

/// What a row of `nothing_a_credential_save_returns_shows_a_value` sets up before its request.
#[derive(Clone, Copy, PartialEq)]
enum Before {
    /// The fixture as the workspace's command, then the row's stored values.
    Command,
    /// A workspace without a command.
    NoCommand,
    /// `Command`, then the workspace's command is cleared; its credentials stay.
    Cleared,
    /// `Command`, then the workspace joins a team.
    Team,
    /// `Command`, then a folder takes the credentials file's place, so it can't be read.
    FileIsFolder,
    /// `Command`, then a folder takes the place of the file the credentials are written to
    /// before it replaces theirs, so the write fails the same way for every user.
    WriteFails,
    /// `Command`, then a SQLite trigger refuses writing the command's record, quoting the value
    /// the row stores.
    Trigger,
    /// `Command`, then a SQLite trigger leaves the command's record unreadable once the save has
    /// written it twice, so the route's summary fails.
    SummaryFails,
    /// `FAILING_SOURCE` as the command, then the row's stored values; its `info` fails as `fail`
    /// says from the request on.
    Failing(&'static str),
}

struct Row {
    name: &'static str,
    before: Before,
    /// Saved first, and accepted.
    stored: Value,
    /// The workspace the request names, when not the row's own.
    workspace: Option<&'static str>,
    values: Value,
    /// The refusal's text, or the failed check's, when the row pins it.
    answer: Option<&'static str>,
}

/// A command with two secret fields and two that aren't, whose `info` fails as the `fail.txt` in
/// its folder says: with an error object, with stderr, with an answer the contract refuses, or,
/// for the `declare-` ones, while declaring `server` secret.
const FAILING_SOURCE: &str = r#"#!/bin/sh
[ "$1" = info ] || exit 1
declares='"credentials": [ { "name": "server", "label": "Server" } ]'
case "$(cat fail.txt 2>/dev/null)" in
reported) printf '{ "error": { "code": "auth", "message": "rejected %s" } }\n' "$GHOSTEX_CREDENTIAL_KEY"; exit 1 ;;
declare-reported) printf '{ %s, "error": { "code": "down", "message": "can'\''t reach %s" } }\n' "$declares" "$GHOSTEX_CREDENTIAL_SERVER"; exit 1 ;;
declare-stderr) printf '{ %s }\n' "$declares"; echo "can't reach $GHOSTEX_CREDENTIAL_SERVER" >&2; exit 3 ;;
plain-stderr) echo "can't reach $GHOSTEX_CREDENTIAL_SERVER" >&2; exit 3 ;;
contract) printf '{ "id": "%s", "name": "Echoer", "ids": { "format": "number" } }\n' "$GHOSTEX_CREDENTIAL_KEY"; exit 0 ;;
esac
printf '{ "id": "echoer", "name": "Echoer", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "Token" }, { "name": "key", "label": "Key" }, { "name": "server", "label": "Server", "secret": false }, { "name": "region", "label": "Region", "secret": false } ] }\n'
"#;

/// A `server` value longer than any cap a failure's text is cut to.
fn long_server() -> String {
    format!("srv-{}", "0123456789".repeat(40))
}

/// Every path a credential save can answer with, one row each: the answer and the request log's
/// errors hold no secret value, whether stored or turned secret by the failing check, no proposed
/// value of 3 or more characters, and no start of a long one. A new path needs only a new row.
#[tokio::test]
async fn nothing_a_credential_save_returns_shows_a_value() {
    let long = Box::leak(long_server().into_boxed_str());
    let rows = [
        Row {
            name: "no workspace by that name (the route)",
            before: Before::Command,
            stored: json!({}),
            workspace: Some("Elsewhere"),
            values: json!({ "token": "matched" }),
            answer: Some("No workspace •••• that id or name."),
        },
        Row {
            name: "no command yet",
            before: Before::NoCommand,
            stored: json!({}),
            workspace: None,
            values: json!({ "token": "first" }),
            answer: Some("Set the ticket command ••••."),
        },
        Row {
            name: "no command, with credentials kept after it was cleared",
            before: Before::Cleared,
            stored: json!({ "token": "command" }),
            workspace: None,
            values: json!({ "token": "tok-new-6" }),
            answer: Some("Set the ticket •••• first."),
        },
        Row {
            name: "the team refusal, holding a proposed value",
            before: Before::Team,
            stored: json!({}),
            workspace: None,
            values: json!({ "token": "team" }),
            answer: Some("A workspace connected to a •••• uses its ••••'s tracker; ticket commands are for this computer's workspaces."),
        },
        Row {
            name: "the team refusal, holding a stored value",
            before: Before::Team,
            stored: json!({ "token": "team" }),
            workspace: None,
            values: json!({ "token": "tok-new-5" }),
            answer: Some("A workspace connected to a •••• uses its ••••'s tracker; ticket commands are for this computer's workspaces."),
        },
        Row {
            name: "an unknown field overlapping a stored secret",
            before: Before::Command,
            stored: json!({ "token": "ж€✓" }),
            workspace: None,
            values: json!({ "aж€✓b": "aж€✓b" }),
            answer: Some("\"••••\" isn't a credential this command asks for."),
        },
        Row {
            name: "not text, with a label holding a proposed value",
            before: Before::Command,
            stored: json!({}),
            workspace: None,
            values: json!({ "mode": "API", "token": 7 }),
            answer: Some("•••• token must be text or null."),
        },
        Row {
            name: "longer than 4 KB",
            before: Before::Command,
            stored: json!({}),
            workspace: None,
            values: json!({ "token": "k".repeat(4097) }),
            answer: Some("API token is longer than 4 KB."),
        },
        Row {
            name: "more than one line",
            before: Before::Command,
            stored: json!({}),
            workspace: None,
            values: json!({ "token": "two\nlines" }),
            answer: Some("API token must be a single line."),
        },
        Row {
            name: "a value Ghostex can't mask",
            before: Before::Command,
            stored: json!({}),
            workspace: None,
            values: json!({ "token": "token" }),
            answer: Some("•••• can't be saved: its value also appears in the command's credential names, which Ghostex can't hide."),
        },
        Row {
            name: "the stored values can't be read",
            before: Before::FileIsFolder,
            stored: json!({ "token": "tok-old-4" }),
            workspace: None,
            values: json!({ "token": "unread" }),
            // Its words around the platform's reason are pinned by the store's own test.
            answer: None,
        },
        Row {
            name: "the credentials file can't be written",
            before: Before::WriteFails,
            stored: json!({ "token": "tok-old-4" }),
            workspace: None,
            values: json!({ "token": "unwritten" }),
            answer: None,
        },
        Row {
            name: "a storage error quoting a stored secret the save removes",
            before: Before::Trigger,
            stored: json!({ "token": "tok-gone-2" }),
            workspace: None,
            values: json!({ "token": null }),
            answer: Some("SQLite ticket sources error: stopped by a trigger at ••••"),
        },
        Row {
            name: "the route's summary fails",
            before: Before::SummaryFails,
            stored: json!({ "token": "tok-old-4" }),
            workspace: None,
            values: json!({ "token": "summary" }),
            answer: None,
        },
        Row {
            name: "an error object holding a stored secret inside a proposed one",
            before: Before::Failing("reported"),
            stored: json!({ "token": "ж€✓" }),
            workspace: None,
            values: json!({ "key": "xж€✓x" }),
            answer: Some("echoer: rejected ••••. Check its credentials in Settings."),
        },
        Row {
            name: "an error object holding a long value the failing answer declares secret",
            before: Before::Failing("declare-reported"),
            stored: json!({ "server": long }),
            workspace: None,
            values: json!({ "key": "key-val-8" }),
            answer: Some("echoer: can't reach ••••"),
        },
        Row {
            name: "stderr holding a long value the failing answer declares secret",
            before: Before::Failing("declare-stderr"),
            stored: json!({ "server": long }),
            workspace: None,
            values: json!({ "key": "key-val-8" }),
            answer: None,
        },
        Row {
            name: "a contract complaint about a value",
            before: Before::Failing("contract"),
            stored: json!({ "key": "keyval-42" }),
            workspace: None,
            values: json!({ "token": "tok-chk-3" }),
            answer: Some("echoer answered something Ghostex can't read (id contains a credential)"),
        },
    ];
    for row in rows {
        check_row(row).await;
    }
}

/// A workspace with `program` as its command.
async fn set_command(routes: &Routes, workspace_id: &str, program: &str) {
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": program }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["result"].get("check").is_none(), "{body}");
}

/// `FAILING_SOURCE`, written next to the fixture.
fn failing_source(routes: &Routes) -> String {
    let program = Path::new(&routes.program).with_file_name("failing-source");
    fs::write(&program, FAILING_SOURCE).expect("write");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
    path_to_string(&program)
}

/// From now on `FAILING_SOURCE` fails as `fail` says.
fn fail_as(routes: &Routes, fail: &str) {
    let dir = command_work_dir(&routes.paths, Some("echoer")).expect("work dir");
    fs::write(dir.join("fail.txt"), fail).expect("write fail");
}

async fn check_row(row: Row) {
    let name = row.name;
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = routes.workspace("Work").await;
    match row.before {
        Before::NoCommand => {}
        Before::Failing(_) => set_command(&routes, &workspace_id, &failing_source(&routes)).await,
        _ => set_command(&routes, &workspace_id, &routes.program).await,
    }
    if row.stored != json!({}) {
        let (status, body) = routes
            .call(
                "/api/setWorkSourceCredentials",
                json!({ "workspaceId": workspace_id, "values": row.stored }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{name}: {body}");
        assert!(body["result"].get("check").is_none(), "{name}: {body}");
    }
    let file = routes.paths.auth_dir.join("work-mode-credentials.json");
    let db = || open_gxserver_database(&routes.paths).expect("db");
    match row.before {
        Before::Cleared => {
            let (status, body) = routes
                .call(
                    "/api/setWorkSource",
                    json!({ "workspaceId": workspace_id, "path": null }),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{name}: {body}");
        }
        Before::Team => routes.join_team(&workspace_id),
        Before::FileIsFolder => {
            fs::remove_file(&file).expect("remove");
            fs::create_dir(&file).expect("folder");
        }
        Before::WriteFails => fs::create_dir(file.with_extension("json.tmp")).expect("folder"),
        Before::Trigger => db()
            .execute_batch(
                "CREATE TRIGGER refuse_sources BEFORE UPDATE ON metadata \
                 WHEN NEW.key = 'workTicketSources' BEGIN SELECT RAISE(ABORT, 'stopped by a trigger at tok-gone-2'); END;",
            )
            .expect("trigger"),
        // A blob where the record's text should be: reading it fails, writing it doesn't.
        Before::SummaryFails => db()
            .execute_batch(
                "CREATE TABLE source_writes (count INTEGER); INSERT INTO source_writes VALUES (0); \
                 CREATE TRIGGER spoil_sources AFTER UPDATE ON metadata \
                 WHEN NEW.key = 'workTicketSources' BEGIN \
                 UPDATE source_writes SET count = count + 1; \
                 UPDATE metadata SET value = X'7B' WHERE key = 'workTicketSources' \
                 AND (SELECT count FROM source_writes) = 2; END;",
            )
            .expect("trigger"),
        Before::Failing(fail) => fail_as(&routes, fail),
        _ => {}
    }
    let response = handle_http_request(
        routes.state.clone(),
        rpc_request(
            "/api/setWorkSourceCredentials",
            &routes.token,
            json!({ "params": {
                "workspaceId": row.workspace.unwrap_or(&workspace_id),
                "values": row.values,
            } }),
        ),
    )
    .await
    .expect("answered");
    let status = response.status();
    let body = response_json(response).await;
    // A refusal, or for a failed check the check's error.
    let said = if let Before::Failing(fail) = row.before {
        assert_eq!(status, StatusCode::OK, "{name}: {body}");
        let check = &body["result"]["check"];
        if fail == "contract" {
            assert!(check["detail"].is_string(), "{name}: {body}");
        }
        check["error"].as_str().expect("a failed check").to_string()
    } else {
        assert_ne!(status, StatusCode::OK, "{name}: {body}");
        error_text(&body)
    };
    if let Some(answer) = row.answer {
        assert_eq!(said, answer, "{name}");
    }
    // Every value here is secret by the end, or proposed: a value that stays not secret is shown
    // by design (`a_failed_check_makes_secret_exactly_what_it_declares`).
    let values: Vec<String> = [&row.stored, &row.values]
        .into_iter()
        .filter_map(Value::as_object)
        .flat_map(|values| values.values())
        .filter_map(|value| value.as_str().map(|text| text.trim().to_string()))
        .filter(|value| value.chars().count() >= 3)
        .flat_map(|value| {
            let start: String = value.chars().take(24).collect();
            [value, start]
        })
        .collect();
    let logged = logged_errors(&routes);
    let answer = body.to_string();
    for value in &values {
        let value = value.as_str();
        assert!(!answer.contains(value), "{name}: {value:?} in {answer}");
        assert!(
            logged.iter().all(|error| !error.contains(value)),
            "{name}: {value:?} in {logged:?}"
        );
    }
}

/// What a failed `ghostex issues source check` (`/api/setWorkSource`) leaves secret: exactly the
/// fields the runner masked its failure with. A field the failing answer declares secret is
/// masked in its error object and its stderr, and is secret in the record; a field it doesn't
/// declare stays as it was, and one accepted as not secret is still shown, being not secret by
/// its declaration.
#[tokio::test]
async fn a_failed_check_makes_secret_exactly_what_it_declares() {
    for fail in ["declare-reported", "declare-stderr", "plain-stderr"] {
        let routes = Routes::new();
        let workspace_id = routes.workspace("Work").await;
        let program = failing_source(&routes);
        set_command(&routes, &workspace_id, &program).await;
        let (status, body) = routes
            .call(
                "/api/setWorkSourceCredentials",
                json!({ "workspaceId": workspace_id, "values": {
                    "server": "srv-host-9", "region": "region-eu-7",
                } }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{fail}: {body}");
        fail_as(&routes, fail);
        let (status, body) = routes
            .call(
                "/api/setWorkSource",
                json!({ "workspaceId": workspace_id, "path": program }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{fail}: {body}");
        let error = body["result"]["check"]["error"].as_str().expect("failed");
        let field = |name: &str| {
            body["result"]["command"]["credentials"]
                .as_array()
                .expect("credentials")
                .iter()
                .find(|field| field["name"] == name)
                .cloned()
                .expect("declared")
        };
        let declared = fail != "plain-stderr";
        assert_eq!(field("server")["secret"], declared, "{fail}: {body}");
        assert_eq!(
            body.to_string().contains("srv-host-9"),
            !declared,
            "{fail}: {body}"
        );
        match fail {
            "declare-reported" => assert_eq!(error, "echoer: can't reach ••••"),
            "declare-stderr" => assert!(
                error.ends_with("is hidden because it could show a credential."),
                "{error}"
            ),
            _ => assert_eq!(error, "echoer: can't reach srv-host-9"),
        }
        assert_eq!(field("region")["secret"], false, "{fail}: {body}");
        assert_eq!(field("region")["value"], "region-eu-7", "{fail}: {body}");
    }
}

/// Everything a workspace deletion must leave as it was when it fails: the workspace, its
/// command and its command's credentials.
async fn assert_all_survived(routes: &Routes, workspace_id: &str) {
    let (_, read) = routes.call("/api/readWorkspaces", json!({})).await;
    assert!(
        read["result"]["sidebarWorkspaces"]["workspaces"]
            .get(workspace_id)
            .is_some(),
        "{read}"
    );
    let db = open_gxserver_database(&routes.paths).expect("db");
    assert!(read_work_sources(&db)
        .expect("read")
        .workspaces
        .contains_key(workspace_id));
    assert!(
        read_credentials(&routes.paths)["workspaceSourceCredentials"][workspace_id]["fixture"]
            .get("token")
            .is_some()
    );
}

async fn workspace_with_command(routes: &Routes) -> String {
    routes.answer("good");
    let workspace_id = routes.workspace("Work").await;
    routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    routes
        .call(
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": workspace_id, "values": { "token": "tok-keep-3" } }),
        )
        .await;
    workspace_id
}

#[tokio::test]
async fn a_deletion_that_cant_remove_the_credentials_changes_nothing() {
    let routes = Routes::new();
    let workspace_id = workspace_with_command(&routes).await;
    // The credentials file can't be replaced.
    fs::set_permissions(&routes.paths.auth_dir, fs::Permissions::from_mode(0o500)).expect("chmod");
    let (status, body) = routes
        .call(
            "/api/deleteWorkspace",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    fs::set_permissions(&routes.paths.auth_dir, fs::Permissions::from_mode(0o700)).expect("chmod");
    assert_ne!(status, StatusCode::OK, "{body}");
    assert_all_survived(&routes, &workspace_id).await;
}

#[tokio::test]
async fn a_deletion_that_cant_write_the_workspaces_puts_the_command_back() {
    let routes = Routes::new();
    let workspace_id = workspace_with_command(&routes).await;
    // Writing the workspaces document fails, after the command and its credentials were removed.
    let db = open_gxserver_database(&routes.paths).expect("db");
    db.execute_batch(
        "CREATE TRIGGER refuse_workspaces BEFORE UPDATE ON metadata \
         WHEN NEW.key = 'sidebarWorkspaces' BEGIN SELECT RAISE(ABORT, 'refused for the test'); END;",
    )
    .expect("trigger");
    let (status, body) = routes
        .call(
            "/api/deleteWorkspace",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert_ne!(status, StatusCode::OK, "{body}");
    db.execute_batch("DROP TRIGGER refuse_workspaces;")
        .expect("drop");
    assert_all_survived(&routes, &workspace_id).await;
}

#[tokio::test]
async fn a_format_change_that_turns_a_field_secret_hides_its_value() {
    let routes = Routes::new();
    let flipped = routes._temp.path().join("flipped");
    let program = routes._temp.path().join("format-flip");
    fs::write(
        &program,
        format!(
            r#"#!/bin/sh
if [ -f '{}' ]; then
  printf '{{ "id": "fixture", "name": "Fixture", "ids": {{ "format": "number" }}, "credentials": [ {{ "name": "mode", "label": "Mode" }} ] }}\n'
else
  printf '{{ "id": "fixture", "name": "Fixture", "ids": {{ "format": "key-number", "keys": ["ACM"] }}, "credentials": [ {{ "name": "mode", "label": "Mode", "secret": false }} ] }}\n'
fi
"#,
            flipped.display()
        ),
    )
    .expect("write");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
    let program = path_to_string(&program);
    let workspace_id = routes.workspace("Work").await;
    let set = json!({ "workspaceId": workspace_id, "path": program });
    routes.call("/api/setWorkSource", set.clone()).await;
    routes
        .call(
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": workspace_id, "values": { "mode": "route-mode-visible" } }),
        )
        .await;
    let (_, read) = routes
        .call(
            "/api/readWorkSource",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert!(
        read.to_string().contains("route-mode-visible"),
        "not secret yet: {read}"
    );
    fs::write(&flipped, "").expect("flip");
    let (status, body) = routes.call("/api/setWorkSource", set).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(error_text(&body).contains("sends number ids now"), "{body}");
    let (_, read) = routes
        .call(
            "/api/readWorkSource",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert!(!read.to_string().contains("route-mode-visible"), "{read}");
}

/// A background read of `fixture`, pinned in `workspace_id`, for `project_id`, paused before its
/// command starts; then `path` (a project route) with `update`. The change must wait for the
/// start, land after it, and leave the read keeping nothing; `project_id` is then placed in
/// `moved_to`.
async fn update_races_a_paused_read(
    routes: &Routes,
    path: &str,
    workspace_id: &str,
    project_id: &str,
    update: Value,
    moved_to: Option<&str>,
) {
    let work_dir = command_work_dir(&routes.paths, Some("fixture")).expect("work dir");
    fs::write(work_dir.join("mode.txt"), "wait-release").expect("mode");
    let plan = crate::work_mode::JobPlan {
        workspace_id: workspace_id.to_string(),
        source: crate::work_mode::CommandSourceId::known("fixture").expect("interned"),
        generation: crate::work_mode::work_source_generation(workspace_id),
    };
    let job = crate::work_mode::ExactRead {
        plan: plan.clone(),
        ids: vec!["ACM-12".to_string()],
        awaiting_pull_request: Vec::new(),
        projects: vec![project_id.to_string()],
        paths: routes.paths.clone(),
    };
    let (reached, paused) = std::sync::mpsc::channel();
    let (resume, release) = std::sync::mpsc::channel();
    crate::work_mode::hook_before_spawn(
        workspace_id,
        crate::work_mode::BeforeSpawn::Pause { reached, release },
    );
    let worker = std::thread::spawn(move || crate::work_mode::run_exact_read(&job));
    paused
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("prepared");

    // A route that moves projects takes the store's writer first (`place_projects_after`); any
    // other change to a project's placement takes the placement lock in the repository. Either
    // notice says the change reached a lock the paused read holds.
    let moving = crate::work_mode::notify_placement_writer(
        &routes.paths,
        crate::work_mode::PlacementChange::Move,
    );
    let placing = crate::domain::repository::project::notify_project_placement(project_id);
    let update = tokio::spawn(route_http(
        routes.state.clone(),
        rpc_request(path, &routes.token, json!({ "params": update })),
        "request-update".to_string(),
    ));
    tokio::task::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while moving.try_recv().is_err() && placing.try_recv().is_err() {
            assert!(
                std::time::Instant::now() < deadline,
                "the change never reached a lock"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    })
    .await
    .expect("joined");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!update.is_finished(), "the update waits for the start");
    resume.send(()).expect("resume");
    let response = tokio::time::timeout(std::time::Duration::from_secs(20), update)
        .await
        .expect("the update lands once the command started")
        .expect("joined");
    assert_eq!(response.response.status(), StatusCode::OK);
    fs::write(work_dir.join("release"), "").expect("release");
    fs::remove_file(work_dir.join("mode.txt")).expect("mode back");
    tokio::task::spawn_blocking(move || worker.join().expect("the read ends"))
        .await
        .expect("joined");

    if let Some(moved_to) = moved_to {
        assert_eq!(
            crate::work_mode::cached_project_workspace_id(project_id).as_deref(),
            Some(moved_to)
        );
    }
    assert!(crate::work_mode::cached_command_ticket(&plan, "ACM-12").is_none());
}

/// Two workspaces, the fixture pinned as `fixture` in the first.
async fn two_workspaces_pinning_in_the_first(routes: &Routes) -> (String, String) {
    routes.answer("good");
    let first = routes.workspace("First").await;
    let second = routes.workspace("Second").await;
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": first, "path": routes.program }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (first, second)
}

/// A project row made directly, then every project placed.
fn placed_project(routes: &Routes, params: Value) -> String {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "test");
    let mut params = params;
    params["path"] = json!(routes.paths.app_data_dir);
    let project = repository
        .create_project(params.as_object().unwrap())
        .expect("project");
    crate::work_mode::replace_project_workspaces(&routes.paths, &repository, &db).expect("placed");
    project["projectId"].as_str().unwrap().to_string()
}

/// `/api/updateProject`, which remote clients may call, can set a project's workspace in its
/// `launchSettings`: it waits, like a move, for a background read that checked the project's
/// placement until its command starts, and the read then keeps nothing of its answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_update_that_moves_it_waits_for_a_reads_command_to_start() {
    let routes = Routes::new();
    let (first, second) = two_workspaces_pinning_in_the_first(&routes).await;
    let project_id = placed_project(
        &routes,
        json!({ "name": "Racing", "launchSettings": { "workspaceId": first } }),
    );
    update_races_a_paused_read(
        &routes,
        "/api/updateProject",
        &first,
        &project_id,
        json!({ "projectId": project_id, "launchSettings": { "workspaceId": second } }),
        Some(&second),
    )
    .await;
}

/// A worktree project is in its parent's workspace, so an update that gives it a parent in
/// another workspace moves it, and waits the same way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_worktree_update_to_a_parent_elsewhere_waits_for_a_reads_command_to_start() {
    let routes = Routes::new();
    let (first, second) = two_workspaces_pinning_in_the_first(&routes).await;
    let parent_in_first = placed_project(
        &routes,
        json!({ "name": "Main", "launchSettings": { "workspaceId": first } }),
    );
    let parent_in_second = placed_project(
        &routes,
        json!({ "name": "Other", "launchSettings": { "workspaceId": second } }),
    );
    let worktree = placed_project(
        &routes,
        json!({ "name": "Branch", "worktree": { "parentProjectId": parent_in_first } }),
    );
    assert_eq!(
        crate::work_mode::cached_project_workspace_id(&worktree),
        Some(first.clone())
    );
    update_races_a_paused_read(
        &routes,
        "/api/updateProject",
        &first,
        &worktree,
        json!({ "projectId": worktree, "worktree": { "parentProjectId": parent_in_second } }),
        Some(&second),
    )
    .await;
}

/// Removing a project moves its worktrees to their own workspace, here the default one, and waits
/// the same way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_a_worktrees_parent_waits_for_a_reads_command_to_start() {
    let routes = Routes::new();
    let (first, _) = two_workspaces_pinning_in_the_first(&routes).await;
    let parent = placed_project(
        &routes,
        json!({ "name": "Main", "launchSettings": { "workspaceId": first } }),
    );
    let worktree = placed_project(
        &routes,
        json!({ "name": "Branch", "worktree": { "parentProjectId": parent } }),
    );
    update_races_a_paused_read(
        &routes,
        "/api/removeProject",
        &first,
        &worktree,
        json!({ "projectId": parent }),
        Some(crate::workspaces::DEFAULT_WORKSPACE_ID),
    )
    .await;
}

/// Removing a recent project removes its row: a read that checked it waits, and keeps nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_a_recent_project_waits_for_a_reads_command_to_start() {
    let routes = Routes::new();
    let (first, _) = two_workspaces_pinning_in_the_first(&routes).await;
    let project_id = placed_project(
        &routes,
        json!({ "name": "Recent", "launchSettings": { "workspaceId": first } }),
    );
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    crate::domain::DomainRepository::new(&db, "test")
        .close_project_to_recent(&project_id)
        .expect("closed to recent");
    update_races_a_paused_read(
        &routes,
        "/api/removeRecentProject",
        &first,
        &project_id,
        json!({ "projectId": project_id }),
        None,
    )
    .await;
}

/// `/api/addProjectPath` for a folder registered earlier without worktree metadata, which is a
/// worktree of a project in another workspace, repairs the row into that worktree: it moves to
/// its parent's workspace, and a read that checked it waits and keeps nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn re_adding_a_path_that_is_a_worktree_elsewhere_waits_for_a_reads_command_to_start() {
    let routes = Routes::new();
    let (first, second) = two_workspaces_pinning_in_the_first(&routes).await;
    let git = |args: &[&str], cwd: &Path| {
        let status = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    };
    let checkout = routes.paths.app_data_dir.join("main-checkout");
    fs::create_dir_all(&checkout).expect("checkout");
    git(&["init", "-q", "-b", "main"], &checkout);
    git(&["commit", "-q", "--allow-empty", "-m", "init"], &checkout);
    let worktree_path = routes.paths.app_data_dir.join("feature-worktree");
    git(
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            &worktree_path.display().to_string(),
        ],
        &checkout,
    );
    let checkout = fs::canonicalize(&checkout).expect("canonical");
    let worktree_path = fs::canonicalize(&worktree_path).expect("canonical");
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "test");
    repository
        .create_project(
            json!({ "name": "Main", "path": checkout, "launchSettings": { "workspaceId": second } })
                .as_object()
                .unwrap(),
        )
        .expect("parent");
    let registered = repository
        .create_project(
            json!({ "name": "Feature", "path": worktree_path, "launchSettings": { "workspaceId": first } })
                .as_object()
                .unwrap(),
        )
        .expect("registered without worktree metadata");
    assert!(registered.get("worktree").is_none_or(Value::is_null));
    let project_id = registered["projectId"].as_str().unwrap().to_string();
    crate::work_mode::replace_project_workspaces(&routes.paths, &repository, &db).expect("placed");
    update_races_a_paused_read(
        &routes,
        "/api/addProjectPath",
        &first,
        &project_id,
        json!({ "path": worktree_path }),
        None,
    )
    .await;
    let repaired = repository
        .get_project(&project_id)
        .expect("read")
        .expect("project");
    assert!(
        repaired["worktree"]["parentProjectId"].is_string(),
        "{repaired}"
    );
}

/// Moving a project, or deleting the workspace it is in, places it in its new workspace before
/// the route publishes it, so its cards read that workspace's ticket command at once.
#[tokio::test]
async fn a_moved_project_is_placed_in_its_new_workspace_before_it_publishes() {
    let routes = Routes::new();
    let first = routes.workspace("First").await;
    let second = routes.workspace("Second").await;
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let project = crate::domain::DomainRepository::new(&db, "test")
        .create_project(
            json!({
                "name": "Placed",
                "path": routes.paths.app_data_dir,
                "launchSettings": { "workspaceId": first },
            })
            .as_object()
            .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap();

    let (status, body) = routes
        .call(
            "/api/moveProjectToWorkspace",
            json!({ "projectId": project_id, "workspaceId": second }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        crate::work_mode::cached_project_workspace_id(project_id),
        Some(second.clone())
    );

    let (status, body) = routes
        .call("/api/deleteWorkspace", json!({ "workspaceId": second }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        crate::work_mode::cached_project_workspace_id(project_id).as_deref(),
        Some(crate::workspaces::DEFAULT_WORKSPACE_ID)
    );
}

/// A session of `workspace_id` with a project of its own.
fn session_in(routes: &Routes, workspace_id: &str, name: &str) -> (String, String) {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "test");
    let folder = routes.paths.app_data_dir.join(name);
    fs::create_dir_all(&folder).expect("folder");
    let project = repository
        .create_project(
            json!({ "name": name, "path": folder, "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap().to_string();
    let session = repository
        .create_session(
            json!({
                "projectId": project_id,
                "kind": "agent",
                "agentId": "codex",
                "title": name,
                "lifecycleState": "running",
            })
            .as_object()
            .unwrap(),
            false,
        )
        .expect("session");
    (
        project_id,
        session["sessionId"].as_str().unwrap().to_string(),
    )
}

/// `/api/setSessionWorkLinks`, which remote clients may call, links a session to tickets of its
/// workspace's ticket command only: not another workspace's, not an id nobody pins, not in a team
/// workspace. Each id is kept as the command reads it, and one that isn't its is refused without
/// being quoted.
#[tokio::test]
async fn session_links_name_only_the_ticket_command_of_the_sessions_workspace() {
    let routes = Routes::new();
    routes.answer("good");
    let pinned = routes.workspace("Pinned").await;
    let other = routes.workspace("Other").await;
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": pinned, "path": routes.program }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (project_id, session_id) = session_in(&routes, &pinned, "linked");
    let (other_project, other_session) = session_in(&routes, &other, "elsewhere");
    let link = |project_id: &str, session_id: &str, tickets: Value| json!({ "projectId": project_id, "sessionId": session_id, "tickets": tickets });

    let (status, body) = routes
        .call(
            "/api/setSessionWorkLinks",
            link(
                &project_id,
                &session_id,
                json!({ "fixture": ["acm-12", "ACM-12", "ops-3"] }),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["workLinks"],
        json!({ "tickets": { "fixture": ["ACM-12", "OPS-3"] } })
    );

    let typed = "tok-typed-link-77";
    let (status, body) = routes
        .call(
            "/api/setSessionWorkLinks",
            link(&project_id, &session_id, json!({ "fixture": [typed] })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error_text(&body),
        "That isn't a fixture ticket ID: a key and a number, like ABC-12."
    );
    assert!(!body.to_string().contains(typed), "{body}");

    for (project, session, tickets) in [
        (
            &other_project,
            &other_session,
            json!({ "fixture": ["ACM-1"] }),
        ),
        (
            &project_id,
            &session_id,
            json!({ "never-pinned": ["ACM-1"] }),
        ),
    ] {
        let (status, body) = routes
            .call("/api/setSessionWorkLinks", link(project, session, tickets))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(error_text(&body), crate::work_mode::TICKET_LINKS_REFUSED);
        assert!(!body.to_string().contains("never-pinned"), "{body}");
    }

    routes.join_team(&pinned);
    let (status, body) = routes
        .call(
            "/api/setSessionWorkLinks",
            link(&project_id, &session_id, json!({ "fixture": null })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error_text(&body), crate::work_mode::TICKET_LINKS_REFUSED);

    // What the refusals left: the links set first.
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let session = crate::domain::DomainRepository::new(&db, "test")
        .get_session(&project_id, &session_id)
        .expect("read")
        .expect("session");
    assert_eq!(
        session["runtimeSettings"]["workLinks"],
        json!({ "tickets": { "fixture": ["ACM-12", "OPS-3"] } })
    );
}
