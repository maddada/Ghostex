//! A session's hand-set tickets of a ticket command through `/api/setSessionWorkLinks`.

use super::*;

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

    // An added ticket joins the hand-set ones; its undo puts the hand-set list back.
    let (status, body) = routes
        .call(
            "/api/setSessionWorkLinks",
            json!({ "projectId": project_id, "sessionId": session_id, "addTickets": { "fixture": ["acm-14"] } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["workLinks"],
        json!({ "tickets": { "fixture": ["ACM-12", "OPS-3", "ACM-14"] } })
    );
    let mut undo = body["result"]["undo"].clone();
    assert_eq!(
        undo,
        json!({ "tickets": { "fixture": ["ACM-12", "OPS-3"] } })
    );
    undo["projectId"] = json!(project_id);
    undo["sessionId"] = json!(session_id);
    let (status, body) = routes.call("/api/setSessionWorkLinks", undo).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // In a team workspace a ticket is refused; a clear of the session's own list passes
    // (`a_stale_commands_tickets_can_still_be_cleared`).
    routes.join_team(&pinned);
    let (status, body) = routes
        .call(
            "/api/setSessionWorkLinks",
            link(&project_id, &session_id, json!({ "fixture": ["ACM-15"] })),
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
