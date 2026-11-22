use super::*;
use std::fs;

// Sidebar V2 worktree sessions
// -----------------------------------------------------------------------

async fn worktree_session_context_for_test(
    state: Arc<AppState>,
    project_id: &str,
) -> ProjectWorktreeOperationContext {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    match resolve_project_worktree_operation_context(&state, &params) {
        Ok(context) => context,
        Err(_) => panic!("worktree operation context"),
    }
}

async fn set_worktree_command_for_test(
    state: Arc<AppState>,
    token: &str,
    project_id: &str,
    command: &str,
) {
    let response = route_http(
        state,
        rpc_request(
            "/api/updateProject",
            token,
            json!({
                "params": {
                    "gitConfig": { "worktreeCommand": command },
                    "projectId": project_id,
                }
            }),
        ),
        "request-set-worktree-command".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
}

#[tokio::test]
async fn worktree_session_checkout_creates_a_temp_branch_and_runs_the_setup_command() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("worktree-session-parent");
    create_git_repository_for_server_test(&parent);
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    set_worktree_command_for_test(
        state.clone(),
        &token,
        &project_id,
        "printf 'setup\\n' > setup-ran.txt",
    )
    .await;

    let context = worktree_session_context_for_test(state.clone(), &project_id).await;
    let request = normalize_worktree_session_create_request(&Map::new()).expect("request");
    let prepared = match prepare_worktree_session_checkout(&state, &context, &request).await {
        Ok(prepared) => prepared,
        Err(_) => panic!("prepare worktree checkout"),
    };

    assert!(prepared.created);
    assert!(
        worktree_sessions::is_worktree_temp_branch(&prepared.branch),
        "unexpected branch {}",
        prepared.branch
    );
    assert!(Path::new(&prepared.path).is_dir());
    assert!(
        Path::new(&prepared.path).join("setup-ran.txt").is_file(),
        "the project's worktree setup command runs inside the new checkout"
    );
    assert_eq!(
        run_git_for_server_test(Path::new(&prepared.path), &["branch", "--show-current"]).trim(),
        prepared.branch
    );
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{}", prepared.branch)
            ]
        )
        .status
        .code(),
        Some(0)
    );
    // The worktree is a session attribute, never a registered project.
    let projects = route_http(
        state.clone(),
        rpc_request("/api/listProjects", &token, json!({ "params": {} })),
        "request-list-after-worktree-session".to_string(),
    )
    .await;
    let body = response_json(projects.response).await;
    assert_eq!(body["result"]["projects"].as_array().unwrap().len(), 1);

    // An explicit base branch seeds the checkout from that branch's tip.
    run_git_for_server_test(&parent, &["checkout", "--quiet", "-b", "seed-branch"]);
    fs::write(parent.join("seed.txt"), "seed\n").expect("seed file");
    run_git_for_server_test(&parent, &["add", "seed.txt"]);
    run_git_for_server_test(&parent, &["commit", "-m", "seed"]);
    run_git_for_server_test(&parent, &["checkout", "--quiet", "-"]);
    let mut base_params = Map::new();
    base_params.insert("baseBranch".to_string(), json!("seed-branch"));
    let base_request = normalize_worktree_session_create_request(&base_params).expect("request");
    let based = match prepare_worktree_session_checkout(&state, &context, &base_request).await {
        Ok(prepared) => prepared,
        Err(_) => panic!("prepare worktree checkout from base branch"),
    };
    assert!(Path::new(&based.path).join("seed.txt").is_file());

    // Without a remote there is nothing to start from, and the refusal is
    // explicit instead of silently falling back to the local branch.
    let mut origin_params = Map::new();
    origin_params.insert("baseBranch".to_string(), json!("seed-branch"));
    origin_params.insert("startFromOrigin".to_string(), json!(true));
    let origin_request =
        normalize_worktree_session_create_request(&origin_params).expect("request");
    let error = prepare_worktree_session_checkout(&state, &context, &origin_request)
        .await
        .err()
        .expect("origin failure");
    match error {
        ProjectWorktreeOperationError::Domain(error) => {
            assert!(error.message.contains("origin/seed-branch"));
        }
        _ => panic!("expected a domain failure"),
    }
}

#[tokio::test]
async fn worktree_session_checkout_rolls_back_when_the_setup_command_fails() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("worktree-session-rollback-parent");
    create_git_repository_for_server_test(&parent);
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let project_id = project["projectId"]
        .as_str()
        .expect("projectId")
        .to_string();
    set_worktree_command_for_test(state.clone(), &token, &project_id, "exit 3").await;

    let context = worktree_session_context_for_test(state.clone(), &project_id).await;
    let request = normalize_worktree_session_create_request(&Map::new()).expect("request");
    let error = prepare_worktree_session_checkout(&state, &context, &request)
        .await
        .err()
        .expect("setup failure");
    match error {
        ProjectWorktreeOperationError::Typed(error) => {
            assert!(error.message.contains("Worktree setup command failed."));
        }
        _ => panic!("expected a typed operation failure"),
    }

    let worktrees =
        run_git_for_server_test(&parent, &["worktree", "list", "--porcelain"]).to_string();
    assert_eq!(
        worktrees.matches("worktree ").count(),
        1,
        "the failed checkout is removed again: {worktrees}"
    );
    let branches = run_git_for_server_test(&parent, &["branch", "--list", "ghostex/*"]);
    assert!(
        branches.trim().is_empty(),
        "the temp branch is deleted too: {branches}"
    );
    let siblings = fs::read_dir(&paths.root_dir)
        .expect("root dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("worktree-session-rollback-parent-")
        })
        .count();
    assert_eq!(siblings, 0, "no stray worktree directory survives");
}

#[tokio::test]
async fn create_worktree_session_route_rejects_a_foreign_existing_worktree_path() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("worktree-session-foreign-parent");
    let foreign = paths.root_dir.join("worktree-session-foreign-other");
    create_git_repository_for_server_test(&parent);
    create_git_repository_for_server_test(&foreign);
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/createWorktreeSession",
            &token,
            json!({
                "params": {
                    "existingWorktree": { "path": path_to_string(&foreign) },
                    "projectId": project["projectId"],
                }
            }),
        ),
        "request-create-worktree-session-foreign".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response.response).await;
    assert_eq!(body["error"], json!("badRequest"));
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("is not a worktree of this project"));

    let missing = route_http(
        state,
        rpc_request(
            "/api/createWorktreeSession",
            &token,
            json!({
                "params": {
                    "existingWorktree": { "path": path_to_string(&paths.root_dir.join("nope")) },
                    "projectId": project["projectId"],
                }
            }),
        ),
        "request-create-worktree-session-missing".to_string(),
    )
    .await;
    assert_eq!(missing.response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn remove_session_worktree_route_answers_dirty_before_removing_and_force_overrides() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("remove-session-worktree-dirty-parent");
    let worktree = paths
        .root_dir
        .join("remove-session-worktree-dirty-parent-0123abcd");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&worktree).as_str(),
        ],
    );
    fs::write(worktree.join("README.md"), "dirty\n").expect("dirty file");
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

    let dirty = route_http(
        state.clone(),
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&worktree),
                }
            }),
        ),
        "request-remove-session-worktree-dirty".to_string(),
    )
    .await;
    assert_eq!(dirty.response.status(), StatusCode::OK);
    let body = response_json(dirty.response).await;
    assert_eq!(body["result"]["removed"], json!(false));
    assert_eq!(body["result"]["dirty"], json!(true));
    assert_eq!(
        body["result"]["warnings"],
        json!(["This worktree has uncommitted changes."])
    );
    assert!(
        worktree.is_dir(),
        "a dirty worktree is never removed silently"
    );

    let forced = route_http(
        state,
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "force": true,
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&worktree),
                }
            }),
        ),
        "request-remove-session-worktree-force".to_string(),
    )
    .await;
    assert_eq!(forced.response.status(), StatusCode::OK);
    let body = response_json(forced.response).await;
    assert_eq!(body["result"]["removed"], json!(true));
    assert_eq!(body["result"]["dirty"], json!(true));
    assert_eq!(body["result"]["warnings"], json!([]));
    assert!(!worktree.exists());
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
        )
        .status
        .code(),
        Some(128),
        "force deletes the managed temp branch too"
    );
}

#[tokio::test]
async fn remove_session_worktree_route_keeps_branches_it_does_not_manage() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("remove-session-worktree-clean-parent");
    let managed = paths
        .root_dir
        .join("remove-session-worktree-clean-parent-0123abcd");
    let foreign = paths
        .root_dir
        .join("remove-session-worktree-clean-parent-feature");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&managed).as_str(),
        ],
    );
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "feature-work",
            path_to_string(&foreign).as_str(),
        ],
    );
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;

    let removed = route_http(
        state.clone(),
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&managed),
                }
            }),
        ),
        "request-remove-session-worktree-clean".to_string(),
    )
    .await;
    assert_eq!(removed.response.status(), StatusCode::OK);
    let body = response_json(removed.response).await;
    assert_eq!(body["result"]["removed"], json!(true));
    assert_eq!(body["result"]["dirty"], json!(false));
    assert_eq!(body["result"]["warnings"], json!([]));
    assert!(!managed.exists());
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
        )
        .status
        .code(),
        Some(128)
    );

    let untouched = route_http(
        state.clone(),
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&foreign),
                }
            }),
        ),
        "request-remove-session-worktree-foreign-branch".to_string(),
    )
    .await;
    assert_eq!(untouched.response.status(), StatusCode::OK);
    let body = response_json(untouched.response).await;
    assert_eq!(body["result"]["removed"], json!(true));
    assert!(!foreign.exists());
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/feature-work"]
        )
        .status
        .code(),
        Some(0),
        "a branch gxserver did not mint survives the worktree removal"
    );

    let outside = route_http(
        state,
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&parent),
                }
            }),
        ),
        "request-remove-session-worktree-main".to_string(),
    )
    .await;
    assert_eq!(
        outside.response.status(),
        StatusCode::BAD_REQUEST,
        "the project's own checkout is not a removable worktree"
    );
}

#[tokio::test]
async fn remove_session_worktree_route_refuses_a_registered_worktree_project() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths
        .root_dir
        .join("remove-session-worktree-registered-parent");
    let registered = paths
        .root_dir
        .join("remove-session-worktree-registered-parent-0123abcd");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&registered).as_str(),
        ],
    );
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    // The V1 flow's registration: the worktree is a project in its own right.
    add_project_path_for_server_test(state.clone(), &token, &registered, Some("Worktree")).await;

    let refused = route_http(
        state,
        rpc_request(
            "/api/removeSessionWorktree",
            &token,
            json!({
                "params": {
                    "projectId": project["projectId"],
                    "worktreePath": path_to_string(&registered),
                }
            }),
        ),
        "request-remove-session-worktree-registered".to_string(),
    )
    .await;
    assert_eq!(refused.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(refused.response).await;
    assert_eq!(body["error"], json!("badRequest"));
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("registered as its own project"),
        "the refusal points at the project delete flow: {}",
        body["message"]
    );
    assert!(
        registered.is_dir(),
        "a registered worktree project's checkout survives the refusal"
    );
    assert_eq!(
        run_git_status_for_server_test(
            &parent,
            &["rev-parse", "--verify", "refs/heads/ghostex/0123abcd"]
        )
        .status
        .code(),
        Some(0),
        "its branch survives too"
    );
}

#[tokio::test]
async fn worktree_branch_rename_pass_renames_only_a_titled_temp_branch() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("rename-parent");
    let worktree = paths.root_dir.join("rename-parent-0123abcd");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            "-b",
            "ghostex/0123abcd",
            path_to_string(&worktree).as_str(),
        ],
    );
    let project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let marker = worktree_sessions::worktree_session_marker_value(
        "ghostex/0123abcd",
        &path_to_string(&worktree),
        "Codex Session",
        "2026-07-29T00:00:00.000Z",
    );
    let created = route_http(
        state.clone(),
        rpc_request(
            "/api/createSession",
            &token,
            json!({
                "params": {
                    "cwd": path_to_string(&worktree),
                    "kind": "terminal",
                    "projectId": project["projectId"],
                    "runtimeSettings": {
                        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker.clone(),
                    },
                    "title": "Codex Session",
                }
            }),
        ),
        "request-create-worktree-session-row".to_string(),
    )
    .await;
    assert_eq!(created.response.status(), StatusCode::OK);
    let session = response_json(created.response).await["result"]["session"].clone();

    // A row still carrying its creation title is not due a rename.
    run_worktree_branch_rename_once(&state).expect("rename pass");
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/0123abcd"
    );

    let renamed = route_http(
        state.clone(),
        rpc_request(
            "/api/updateSession",
            &token,
            json!({
                "params": {
                    "projectId": session["projectId"],
                    "runtimeSettings": {
                        "titleSource": "generated",
                        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker,
                    },
                    "sessionId": session["sessionId"],
                    "title": "Fix the flaky login test",
                }
            }),
        ),
        "request-title-worktree-session-row".to_string(),
    )
    .await;
    assert_eq!(renamed.response.status(), StatusCode::OK);

    run_worktree_branch_rename_once(&state).expect("rename pass");
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/fix-the-flaky-login-test"
    );

    // The marker now records the new branch, so the next pass is a no-op.
    let listed = route_http(
        state.clone(),
        rpc_request(
            "/api/listSessions",
            &token,
            json!({ "params": { "projectId": session["projectId"] } }),
        ),
        "request-list-renamed-worktree-session".to_string(),
    )
    .await;
    let body = response_json(listed.response).await;
    let stored = body["result"]["sessions"][0].clone();
    assert_eq!(
        stored["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY]["branch"],
        json!("ghostex/fix-the-flaky-login-test")
    );
    assert!(
        stored["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY]["renamedAt"]
            .is_string()
    );
    run_worktree_branch_rename_once(&state).expect("rename pass");
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/fix-the-flaky-login-test"
    );
}
