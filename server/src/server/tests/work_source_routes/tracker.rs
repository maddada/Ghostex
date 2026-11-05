//! A ticket command as its workspace's tracker: picked only through the local `/api/setWorkTracker`,
//! only while the workspace pins it and has no team; a clear drops the pick and a Switch moves it;
//! and the Create ticket route that clients knowing only Linear and GitHub call refuses it.

use super::*;

async fn pin(routes: &Routes, name: &str) -> String {
    let workspace_id = routes.workspace(name).await;
    let (status, body) = set_logged(
        routes,
        json!({ "workspaceId": workspace_id, "path": routes.program }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    workspace_id
}

async fn pick(routes: &Routes, workspace_id: &str, tracker: &str) -> (StatusCode, Value) {
    routes
        .call(
            "/api/setWorkTracker",
            json!({ "workspaceId": workspace_id, "tracker": tracker }),
        )
        .await
}

async fn tracker_of(routes: &Routes, workspace_id: &str) -> Value {
    let (status, body) = routes
        .call(
            "/api/readWorkTracker",
            json!({ "workspaceId": workspace_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["result"].clone()
}

/// What the workspaces document keeps as the workspace's `tracker`.
fn stored_tracker(routes: &Routes, workspace_id: &str) -> Value {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    crate::workspaces::read_sidebar_workspaces(&db).expect("workspaces")["workspaces"][workspace_id]
        ["tracker"]
        .clone()
}

#[tokio::test]
async fn a_pinned_command_becomes_the_tracker() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = pin(&routes, "Picked").await;
    let (status, body) = pick(&routes, &workspace_id, "fixture").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["tracker"], json!("fixture"));
    let tracker = tracker_of(&routes, &workspace_id).await;
    assert_eq!(tracker["tracker"], json!("fixture"));
    assert_eq!(tracker["source"], json!("workspace"));
    assert_eq!(
        read_source(&routes, &workspace_id).await["result"]["command"]["isTracker"],
        json!(true)
    );

    // Back to GitHub: the command is still pinned, no longer the tracker.
    let (status, body) = pick(&routes, &workspace_id, "github").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        read_source(&routes, &workspace_id).await["result"]["command"]["isTracker"],
        json!(false)
    );
}

#[tokio::test]
async fn set_work_tracker_refuses_an_unpinned_or_team_command() {
    let routes = Routes::new();
    routes.answer("good");
    pin(&routes, "Pinned").await;
    let bare = routes.workspace("Bare").await;
    let (status, body) = pick(&routes, &bare, "fixture").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        error_text(&body).contains("isn't this workspace's ticket command"),
        "{body}"
    );
    let (status, body) = pick(&routes, &bare, "nosuchsource").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(error_text(&body).starts_with("tracker must be"), "{body}");
    assert_eq!(stored_tracker(&routes, &bare), Value::Null);

    let team = pin(&routes, "Team").await;
    routes.join_team(&team);
    let (status, body) = pick(&routes, &team, "fixture").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error_text(&body), crate::work_mode::TEAM_REFUSAL);
    assert_eq!(stored_tracker(&routes, &team), Value::Null);
}

/// A command picked as the tracker applies only while its workspace pins it and has no team; the
/// workspace then uses the rule for one that never picked a tracker.
#[tokio::test]
async fn a_command_tracker_applies_only_while_pinned_outside_a_team() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = pin(&routes, "Joins").await;
    let (status, body) = pick(&routes, &workspace_id, "fixture").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    routes.join_team(&workspace_id);
    let tracker = tracker_of(&routes, &workspace_id).await;
    assert_eq!(tracker["tracker"], json!("github"));
    assert_eq!(tracker["source"], json!("default"));

    // Kept as text where nothing pins it: the workspaces document holds it, the tracker ignores it.
    let bare = routes.workspace("Unpinned").await;
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let current = crate::workspaces::read_sidebar_workspaces(&db).expect("workspaces");
    let next = crate::workspaces::update_workspace_in(
        &current,
        &bare,
        json!({ "tracker": "fixture" }).as_object().unwrap(),
    )
    .expect("patched");
    crate::workspaces::write_sidebar_workspaces(&db, &next).expect("written");
    assert_eq!(stored_tracker(&routes, &bare), json!("fixture"));
    assert_eq!(tracker_of(&routes, &bare).await["source"], json!("default"));
}

#[tokio::test]
async fn a_clear_drops_the_command_tracker_and_a_switch_moves_it() {
    let routes = Routes::new();
    routes.answer("good");
    let cleared = pin(&routes, "Cleared").await;
    pick(&routes, &cleared, "fixture").await;
    let (status, body) = set_logged(&routes, json!({ "workspaceId": cleared, "path": null })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(stored_tracker(&routes, &cleared), Value::Null);

    let switched = pin(&routes, "Switched").await;
    pick(&routes, &switched, "fixture").await;
    routes.answer("info-renamed");
    let (status, body) = set_logged(
        &routes,
        json!({ "workspaceId": switched, "path": routes.program, "switch": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["command"]["sourceId"], json!("renamed"));
    assert_eq!(stored_tracker(&routes, &switched), json!("renamed"));
    assert_eq!(
        tracker_of(&routes, &switched).await["tracker"],
        json!("renamed")
    );
}

/// The workspaces document refuses writes until the returned trigger is dropped.
fn refuse_workspace_writes(routes: &Routes) -> crate::storage::GxserverDatabase {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    db.execute_batch(
        "CREATE TRIGGER refuse_workspaces BEFORE UPDATE ON metadata \
         WHEN NEW.key = 'sidebarWorkspaces' BEGIN SELECT RAISE(ABORT, 'refused for the test'); END;",
    )
    .expect("trigger");
    db
}

/// The command each workspace pins, in the record and in the copy in memory.
fn pinned_id(routes: &Routes, workspace_id: &str) -> (Option<String>, Option<String>) {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    (
        read_work_sources(&db)
            .expect("record")
            .workspaces
            .get(workspace_id)
            .map(|command| command.source_id.clone()),
        crate::work_mode::workspace_command(workspace_id).map(|command| command.source_id.clone()),
    )
}

/// Picks the workspace's command as its tracker without the route, which also starts a card pass
/// in the background: its reads and writes would race the test's own.
fn pick_here(routes: &Routes, workspace_id: &str) {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    crate::work_mode::pick_command_tracker(&db, &routes.paths, workspace_id, "fixture")
        .expect("picked");
}

/// The record and the tracker change as one: a clear or a Switch whose tracker write fails keeps
/// both as they were, and the copy in memory and the generation with them; once the write works,
/// the same request does both.
#[tokio::test]
async fn a_clear_or_switch_whose_tracker_write_fails_changes_neither() {
    let routes = Routes::new();
    routes.answer("good");
    let pinned = (Some("fixture".to_string()), Some("fixture".to_string()));

    let cleared = pin(&routes, "ClearFails").await;
    pick_here(&routes, &cleared);
    let generation = crate::work_mode::work_source_generation(&cleared);
    let refused = refuse_workspace_writes(&routes);
    let clear = json!({ "workspaceId": cleared, "path": null });
    let (status, body) = set_logged(&routes, clear.clone()).await;
    assert_ne!(status, StatusCode::OK, "{body}");
    assert_eq!(pinned_id(&routes, &cleared), pinned);
    assert_eq!(stored_tracker(&routes, &cleared), json!("fixture"));
    assert_eq!(
        crate::work_mode::work_source_generation(&cleared),
        generation
    );
    refused
        .execute_batch("DROP TRIGGER refuse_workspaces;")
        .expect("dropped");
    let (status, body) = set_logged(&routes, clear).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(pinned_id(&routes, &cleared), (None, None));
    assert_eq!(stored_tracker(&routes, &cleared), Value::Null);

    let switched = pin(&routes, "SwitchFails").await;
    pick_here(&routes, &switched);
    routes.answer("info-renamed");
    let generation = crate::work_mode::work_source_generation(&switched);
    let refused = refuse_workspace_writes(&routes);
    let switch = json!({ "workspaceId": switched, "path": routes.program, "switch": true });
    let (status, body) = set_logged(&routes, switch.clone()).await;
    assert_ne!(status, StatusCode::OK, "{body}");
    assert_eq!(pinned_id(&routes, &switched), pinned);
    assert_eq!(stored_tracker(&routes, &switched), json!("fixture"));
    assert_eq!(
        crate::work_mode::work_source_generation(&switched),
        generation
    );
    refused
        .execute_batch("DROP TRIGGER refuse_workspaces;")
        .expect("dropped");
    let (status, body) = set_logged(&routes, switch).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let renamed = Some("renamed".to_string());
    assert_eq!(pinned_id(&routes, &switched), (renamed.clone(), renamed));
    assert_eq!(stored_tracker(&routes, &switched), json!("renamed"));
}

/// The Create ticket window and the `create-ticket` CLI make a Linear ticket for any tracker that
/// isn't GitHub; for a command tracker the route answers as the command's own create would.
#[tokio::test]
async fn a_linear_ticket_is_refused_where_a_command_is_the_tracker() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = pin(&routes, "Creates").await;
    pick(&routes, &workspace_id, "fixture").await;
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let folder = routes.paths.app_data_dir.join("creates");
    fs::create_dir_all(&folder).expect("folder");
    let project = crate::domain::DomainRepository::new(&db, "test")
        .create_project(
            json!({ "name": "creates", "path": folder, "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap();
    crate::work_mode::store_linear_api_key(
        &routes.paths,
        crate::work_mode::LinearKeyScope::Project(project_id),
        Some("lin_api_creates"),
    )
    .expect("key");
    let (status, body) = routes
        .call(
            "/api/createLinearIssue",
            json!({ "projectId": project_id, "title": "New", "teamId": "team-1" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error_text(&body),
        "Ghostex can't create Fixture tickets yet."
    );
}

/// Without a Linear key too, the answer is the command's, not "Set a Linear API key first", which
/// would send the person to set up the wrong tracker.
#[tokio::test]
async fn a_linear_ticket_is_refused_where_a_command_is_the_tracker_without_a_linear_key() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = pin(&routes, "Keyless").await;
    pick(&routes, &workspace_id, "fixture").await;
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let folder = routes.paths.app_data_dir.join("keyless");
    fs::create_dir_all(&folder).expect("folder");
    let project = crate::domain::DomainRepository::new(&db, "test")
        .create_project(
            json!({ "name": "keyless", "path": folder, "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let (status, body) = routes
        .call(
            "/api/createLinearIssue",
            json!({ "projectId": project["projectId"], "title": "New", "teamId": "team-1" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        error_text(&body),
        "Ghostex can't create Fixture tickets yet."
    );
}

/// `/api/listWorkItems` answers `refreshing: true` while a command tracker's open list is owed
/// (its refresh only queues it), then the command's tickets, which can't be started yet.
#[tokio::test]
async fn the_work_list_lists_a_command_trackers_tickets() {
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Listed", "tok-listed").await;
    let (status, body) = pick(&routes, &workspace_id, "fixture").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "test");
    let folder = routes.paths.app_data_dir.join("listed");
    fs::create_dir_all(&folder).expect("folder");
    let project = repository
        .create_project(
            json!({ "name": "listed", "path": folder, "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    crate::work_mode::replace_project_workspaces(&routes.paths, &repository, &db).expect("placed");
    let list = json!({ "projectIds": [project["projectId"]] });

    let (status, first) = routes.call("/api/listWorkItems", list.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["result"]["refreshing"], json!(true), "{first}");
    let answer = work_list_once_read(&routes, &list).await;
    let rows: Vec<&Value> = answer["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["source"] == "fixture")
        .collect();
    assert_eq!(rows.len(), 1, "{answer}");
    assert_eq!(rows[0]["id"], "ACM-12");
    // Start work can read it.
    assert_eq!(rows[0]["canStart"], true);
}

/// `/api/listWorkItems` once it has stopped refreshing: every open list it owes has been read.
async fn work_list_once_read(routes: &Routes, list: &Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let (_, answer) = routes.call("/api/listWorkItems", list.clone()).await;
        if answer["result"]["refreshing"] == json!(false) {
            return answer;
        }
        assert!(std::time::Instant::now() < deadline, "{answer}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// A ticket command's details are read in a workspace that pins it. With two workspaces pinning
/// `fixture` and both answering `ACM-12`, a request naming neither a project nor the row's
/// `sourceKey`, or naming a project whose workspace has no command, is refused, and either one
/// reads that workspace's row.
#[tokio::test]
async fn a_command_tickets_details_need_their_workspace() {
    let routes = Routes::new();
    routes.answer("good");
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "test");
    let mut placed = Vec::new();
    for name in ["details-a", "details-b", "details-bare"] {
        let workspace_id = if name == "details-bare" {
            routes.workspace(name).await
        } else {
            let workspace_id = with_token(&routes, name, &format!("tok-{name}")).await;
            let (status, body) = pick(&routes, &workspace_id, "fixture").await;
            assert_eq!(status, StatusCode::OK, "{body}");
            workspace_id
        };
        let folder = routes.paths.app_data_dir.join(name);
        fs::create_dir_all(&folder).expect("folder");
        let project = repository
            .create_project(
                json!({ "name": name, "path": folder, "launchSettings": { "workspaceId": workspace_id } })
                    .as_object()
                    .unwrap(),
            )
            .expect("project");
        placed.push((
            workspace_id,
            project["projectId"].as_str().unwrap().to_string(),
        ));
    }
    crate::work_mode::replace_project_workspaces(&routes.paths, &repository, &db).expect("placed");
    // Its tickets name no PR, so the details read nothing from GitHub.
    routes.answer("title-kept");
    let project_ids: Vec<&str> = placed.iter().map(|(_, id)| id.as_str()).collect();
    work_list_once_read(&routes, &json!({ "projectIds": project_ids })).await;
    let read = |extra: Value| {
        let mut params =
            json!({ "source": "fixture", "ticketId": "ACM-12", "projectIds": project_ids });
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        routes.logged_call("/api/readWorkItem", params)
    };

    let (pinned, bare) = placed.split_at(2);
    for refused in [json!({}), json!({ "projectId": bare[0].1 })] {
        let (status, body) = read(refused.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}: {body}");
        assert_eq!(error_text(&body), crate::work_mode::COMMAND_DETAILS_REFUSAL);
    }
    for (workspace_id, project_id) in pinned {
        let key = crate::work_mode::source_key(workspace_id, "fixture");
        for extra in [
            json!({ "projectId": project_id }),
            json!({ "sourceKey": key }),
        ] {
            let (status, body) = read(extra.clone()).await;
            assert_eq!(status, StatusCode::OK, "{extra}: {body}");
            assert_eq!(
                body["result"]["item"]["sourceKey"],
                json!(key),
                "{extra}: {body}"
            );
        }
    }
}
