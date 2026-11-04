//! Changes to where a project is placed, through the real routes, against a background read
//! paused before its command starts: each waits for the start, and the read keeps nothing.

use super::*;

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
