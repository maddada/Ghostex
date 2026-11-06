//! `/api/createWorkTicket`, as clients call it: a created ticket's answer, a create that ran and
//! failed answered rather than refused (its words never reach the request log), the refusals
//! Ghostex words itself, and the route's access.

use super::*;
use crate::protocol::{endpoint_for, is_remote_endpoint_allowed, ApiPermission, ListenerKind};

/// A project of `workspace_id`.
fn project_in(routes: &Routes, workspace_id: &str, name: &str) -> String {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let folder = routes.paths.app_data_dir.join(name);
    fs::create_dir_all(&folder).expect("folder");
    crate::domain::DomainRepository::new(&db, "test")
        .create_project(
            json!({ "name": name, "path": folder, "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("project")["projectId"]
        .as_str()
        .unwrap()
        .to_string()
}

fn calls(routes: &Routes) -> Vec<String> {
    let dir = command_work_dir(&routes.paths, Some("fixture")).expect("work dir");
    fs::read_to_string(dir.join("calls.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// A local and a remote client may create, as they create Linear tickets and GitHub issues.
#[test]
fn creating_a_command_ticket_is_remote_allowed() {
    let endpoint = endpoint_for("/api/createWorkTicket").expect("endpoint");
    assert!(matches!(endpoint.permission, ApiPermission::RemoteAllowed));
    assert!(is_remote_endpoint_allowed(
        ListenerKind::Remote,
        endpoint.permission
    ));
    assert!(endpoint.requires_auth);
}

/// The route answers the ticket the command made; a source that isn't a ticket command, or no
/// title, is refused and runs nothing.
#[tokio::test]
async fn the_route_answers_the_created_ticket_and_refuses_what_it_cant_create() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Pinned", "tok-create-route").await;
    let project_id = project_in(&routes, &workspace_id, "created");
    routes.answer("create-ok");
    let (status, body) = routes
        .call(
            "/api/createWorkTicket",
            json!({ "projectId": project_id, "source": "fixture", "title": "Try it" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"],
        json!({
            "source": "fixture",
            "id": "ACM-41",
            "label": "ACM-41",
            "url": "https://tracker.example/ACM-41",
            "branchName": "dev/acm-41",
        })
    );
    let before = calls(&routes);
    for (params, refused) in [
        (
            json!({ "projectId": project_id, "source": "linear", "title": "Try it" }),
            "Pass source: the id of the workspace's ticket command. Linear tickets and GitHub issues have their own routes.",
        ),
        (
            json!({ "projectId": project_id, "source": "fixture", "title": " " }),
            "A ticket needs a title.",
        ),
    ] {
        let (status, body) = routes.call("/api/createWorkTicket", params).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(error_text(&body), refused);
    }
    assert_eq!(calls(&routes), before);
}

/// A create that ran and failed is an answer: the command's masked words reach the person, and
/// the request log gets none of them. A timeout says the ticket may have been made.
#[tokio::test]
async fn a_failed_create_is_an_answer_never_a_request_error() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let token = "tok-create-failed";
    let workspace_id = with_token(&routes, "Pinned", token).await;
    let project_id = project_in(&routes, &workspace_id, "failed");
    routes.answer("error-token");
    let (status, body) = routes
        .logged_call(
            "/api/createWorkTicket",
            json!({ "projectId": project_id, "source": "fixture", "title": "Try it" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["mayHaveBeenCreated"], json!(false), "{body}");
    assert!(
        body["result"]["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("Fixture: rejected")),
        "{body}"
    );
    assert!(!body.to_string().contains(token), "{body}");
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(!log.contains(token), "{log}");
    assert!(!log.contains("rejected"), "{log}");
    // A timeout, with a time limit the test shortens.
    routes.answer("good");
    let other = with_token(&routes, "Slow", "tok-create-slow").await;
    let slow_project = project_in(&routes, &other, "slow");
    routes.answer("create-slow");
    let answer = crate::server::route_http::create_command_ticket(
        &routes.state,
        json!({ "projectId": slow_project, "source": "fixture", "title": "Try it" })
            .as_object()
            .unwrap(),
        std::time::Duration::from_millis(300),
        std::time::Instant::now() + crate::work_mode::CREATE_WRITER_WAIT,
    )
    .map_err(|error| error.message)
    .expect("answered");
    assert_eq!(answer["mayHaveBeenCreated"], json!(true), "{answer}");
    assert!(
        answer["error"]
            .as_str()
            .is_some_and(|error| error.contains("may have been created")),
        "{answer}"
    );
}
