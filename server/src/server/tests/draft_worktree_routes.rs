use super::*;
use std::fs;

// A new chat's worktree tray (`/api/draftWorktree`, worktree_ops/draft_worktree.rs). Starting the
// moved agent needs a live zmx, so these cover what comes before it: the worktree project the
// draft moves into, and the rollback a failure runs.
// -----------------------------------------------------------------------

async fn parent_project(
    state: Arc<AppState>,
    token: &str,
    paths: &crate::paths::GxserverPaths,
    name: &str,
) -> (std::path::PathBuf, String) {
    let parent = paths.root_dir.join(name);
    create_git_repository_for_server_test(&parent);
    let project = add_project_path_for_server_test(state, token, &parent, Some("Parent")).await;
    (
        parent,
        project["projectId"]
            .as_str()
            .expect("projectId")
            .to_string(),
    )
}

fn context_for(state: &AppState, project_id: &str) -> ProjectWorktreeOperationContext {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    match resolve_project_worktree_operation_context(state, &params) {
        Ok(context) => context,
        Err(_) => panic!("worktree operation context"),
    }
}

fn move_request(project_id: &str, mode: DraftWorktreeMode, text: &str) -> DraftMoveRequest {
    DraftMoveRequest {
        project_id: project_id.to_string(),
        session_id: "S-draft".to_string(),
        mode,
        base_branch: None,
        worktree_key: None,
        text: text.to_string(),
        image_paths: Vec::new(),
    }
}

async fn project_count(state: Arc<AppState>, token: &str) -> usize {
    let projects = route_http(
        state,
        rpc_request("/api/listProjects", token, json!({ "params": {} })),
        "request-list-projects".to_string(),
    )
    .await;
    response_json(projects.response).await["result"]["projects"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default()
}

fn branch_exists(repository: &Path, branch: &str) -> bool {
    run_git_status_for_server_test(
        repository,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    )
    .status
    .success()
}

#[tokio::test]
async fn a_new_worktree_is_a_project_named_from_the_message_and_a_taken_name_gets_a_number() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let (parent, project_id) = parent_project(state.clone(), &token, &paths, "draft-parent").await;
    let context = context_for(&state, &project_id);
    let request = move_request(
        &project_id,
        DraftWorktreeMode::NewWorktree,
        "SPX-123 fix copy link",
    );

    let mut made = MadeForMove::default();
    let project = match prepare_worktree_for_move(&state, &context, &request, &mut made).await {
        Ok(project) => project,
        Err(error) => panic!("new worktree for the draft: {}", user_message(error)),
    };
    let path = project["path"].as_str().expect("path").to_string();
    assert!(
        path.ends_with("draft-parent-spx-123-fix-copy-link"),
        "{path}"
    );
    assert_eq!(project["name"], "Parent-spx-123-fix-copy-link");
    assert_eq!(project["worktree"]["parentProjectId"], json!(project_id));
    assert!(branch_exists(&parent, "spx-123-fix-copy-link"));
    assert_eq!(
        run_git_for_server_test(Path::new(&path), &["branch", "--show-current"]).trim(),
        "spx-123-fix-copy-link"
    );
    assert_eq!(
        made.registered_project_id.as_deref(),
        project["projectId"].as_str()
    );

    // The same message again: the name is taken, so Add Worktree's `-2` follows.
    let mut again = MadeForMove::default();
    let second = match prepare_worktree_for_move(&state, &context, &request, &mut again).await {
        Ok(project) => project,
        Err(_) => panic!("second worktree"),
    };
    assert!(
        second["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("draft-parent-spx-123-fix-copy-link-2")),
        "{}",
        second["path"]
    );
    assert!(branch_exists(&parent, "spx-123-fix-copy-link-2"));
    assert_eq!(project_count(state.clone(), &token).await, 3);
}

#[tokio::test]
async fn a_failed_setup_command_takes_back_the_branch_the_folder_and_the_project() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let (parent, project_id) =
        parent_project(state.clone(), &token, &paths, "rollback-parent").await;
    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/updateProject",
            &token,
            json!({ "params": { "projectId": project_id, "gitConfig": { "worktreeCommand": "exit 3" } } }),
        ),
        "request-set-failing-setup".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let context = context_for(&state, &project_id);
    let request = move_request(
        &project_id,
        DraftWorktreeMode::NewWorktree,
        "Fix the login loop",
    );

    let mut made = MadeForMove::default();
    let failed = prepare_worktree_for_move(&state, &context, &request, &mut made).await;
    assert!(failed.is_err(), "the setup command fails the move");
    let checkout = made.checkout.clone().expect("the move made a checkout");
    assert!(Path::new(&checkout.path).is_dir());
    assert!(made.registered_project_id.is_some());
    assert_eq!(project_count(state.clone(), &token).await, 2);

    rollback_draft_move(&state, &context, made).await;
    assert!(!Path::new(&checkout.path).exists(), "the folder is removed");
    assert!(
        !branch_exists(&parent, "fix-the-login-loop"),
        "the branch is deleted"
    );
    assert_eq!(
        project_count(state.clone(), &token).await,
        1,
        "the project is unregistered"
    );
    let worktrees = run_git_for_server_test(&parent, &["worktree", "list", "--porcelain"]);
    assert_eq!(worktrees.matches("worktree ").count(), 1, "{worktrees}");
}

#[tokio::test]
async fn an_existing_worktree_is_opened_as_its_project_with_no_new_branch() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let (parent, project_id) =
        parent_project(state.clone(), &token, &paths, "existing-parent").await;
    let existing = paths.root_dir.join("existing-parent-feature");
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "feature-x",
            existing.to_str().expect("path"),
        ],
    );
    fs::write(existing.join("marker.txt"), "x\n").expect("marker");
    let branches_before = run_git_for_server_test(&parent, &["branch", "--list"]);
    let context = context_for(&state, &project_id);
    let key = project_worktree_options(&context)
        .await
        .ok()
        .into_iter()
        .flatten()
        .map(project_worktree_option_json)
        .find(|option| option["branch"] == "feature-x")
        .and_then(|option| option["worktreeKey"].as_str().map(str::to_string))
        .expect("the existing worktree is listed");
    let mut request = move_request(&project_id, DraftWorktreeMode::ExistingWorktree, "carry on");
    request.worktree_key = Some(key);

    let mut made = MadeForMove::default();
    let project = match prepare_worktree_for_move(&state, &context, &request, &mut made).await {
        Ok(project) => project,
        Err(_) => panic!("open the existing worktree"),
    };
    assert!(made.checkout.is_none(), "no checkout or branch is made");
    assert!(
        made.registered_project_id.is_some(),
        "it was not a project before"
    );
    assert_eq!(project["worktree"]["parentProjectId"], json!(project_id));
    assert!(project["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("existing-parent-feature")));
    assert_eq!(
        run_git_for_server_test(&parent, &["branch", "--list"]),
        branches_before
    );

    // A failure after opening unregisters the project it registered, and leaves the checkout.
    rollback_draft_move(&state, &context, made).await;
    assert!(existing.join("marker.txt").is_file());
    assert!(branch_exists(&parent, "feature-x"));
    assert_eq!(project_count(state.clone(), &token).await, 1);

    // A worktree that was already a project stays one when a move into it fails.
    let mut opened_before = MadeForMove::default();
    let _ = prepare_worktree_for_move(&state, &context, &request, &mut opened_before).await;
    let mut reopened = MadeForMove::default();
    let context = context_for(&state, &project_id);
    if prepare_worktree_for_move(&state, &context, &request, &mut reopened)
        .await
        .is_err()
    {
        panic!("reopen the existing worktree");
    }
    assert!(reopened.registered_project_id.is_none());
    rollback_draft_move(&state, &context, reopened).await;
    assert_eq!(project_count(state.clone(), &token).await, 2);
}
