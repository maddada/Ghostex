use super::*;
use std::fs;

#[tokio::test]
async fn delete_worktree_project_route_removes_clean_checkout_and_local_branch() {
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("delete-worktree-parent");
    let worktree = paths.root_dir.join("delete-worktree-parent-feature");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "feature-clean"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feature-clean",
        ],
    );

    let parent_project = add_project_path_for_server_test(
        state.clone(),
        &token,
        &parent,
        Some("Delete Worktree Parent"),
    )
    .await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;
    assert_eq!(
        worktree_project["worktree"]["parentProjectId"],
        parent_project["projectId"]
    );

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/deleteWorktreeProject",
            &token,
            json!({
                "params": {
                    "deleteLocalBranch": true,
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-delete-clean-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    assert_eq!(
        body["result"]["checkoutRemoval"],
        json!({ "forced": false, "retriedForSubmodules": false })
    );
    assert_eq!(body["result"]["warnings"], json!([]));
    assert_eq!(
        body["result"]["project"]["projectId"],
        worktree_project["projectId"]
    );
    assert!(!worktree.exists());
    assert_eq!(
        run_git_status_for_server_test(&parent, &["rev-parse", "--verify", "feature-clean"])
            .status
            .code(),
        Some(128)
    );

    let projects = route_http(
        state,
        rpc_request("/api/listProjects", &token, json!({ "params": {} })),
        "request-list-after-delete-clean".to_string(),
    )
    .await;
    let body = response_json(projects.response).await;
    assert!(!body["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["projectId"] == worktree_project["projectId"]));
}

#[tokio::test]
async fn renaming_a_worktree_updates_the_project_path_and_every_session_cwd() {
    /*
    CDXC:Worktrees 2026-08-09-18:40:
    The lockstep contract. A rename that moves the folder but leaves the
    database describing the old one is worse than no feature: the sidebar row
    points at a dead path, `start_session_provider` refuses to start anything
    there, and the V2 worktree marker silently stops being renameable. Assert
    the whole set in one pass — project path, derived label, re-detected
    worktree metadata, every session cwd, and the marker path — because they
    only have value together.
    */
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("rename-worktree-parent");
    let worktree = paths.root_dir.join("rename-worktree-parent-old");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "ghostex/0123abcd"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "ghostex/0123abcd",
        ],
    );

    let parent_project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;
    assert_eq!(
        worktree_project["worktree"]["parentProjectId"],
        parent_project["projectId"]
    );

    let marker = worktree_sessions::worktree_session_marker_value(
        "ghostex/0123abcd",
        &path_to_string(&worktree),
        "Codex Session",
        "2026-07-29T00:00:00.000Z",
    );
    // A new session's cwd has to be a folder (`/api/createSession` refuses a missing one).
    std::fs::create_dir_all(worktree.join("packages/app")).expect("session folder");
    let created = route_http(
        state.clone(),
        rpc_request(
            "/api/createSession",
            &token,
            json!({
                "params": {
                    "cwd": path_to_string(&worktree.join("packages/app")),
                    "kind": "terminal",
                    "projectId": worktree_project["projectId"],
                    "runtimeSettings": {
                        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY: marker,
                    },
                    "title": "Codex Session",
                }
            }),
        ),
        "request-create-rename-session".to_string(),
    )
    .await;
    assert_eq!(created.response.status(), StatusCode::OK);
    let session = response_json(created.response).await["result"]["session"].clone();

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "feat/kanban-assignee",
                    "projectId": worktree_project["projectId"],
                    "renameBranch": true
                }
            }),
        ),
        "request-rename-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    let renamed = paths
        .root_dir
        .join("rename-worktree-parent-feat-kanban-assignee");

    assert_eq!(body["result"]["movedFolder"], json!(true));
    assert_eq!(
        body["result"]["renamedBranch"],
        json!("feat/kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["path"],
        json!(path_to_string(&renamed))
    );
    assert_eq!(
        body["result"]["project"]["name"],
        json!("Parent-feat-kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["worktree"]["name"],
        json!("rename-worktree-parent-feat-kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["worktree"]["branch"],
        json!("feat/kanban-assignee")
    );
    assert_eq!(
        body["result"]["project"]["worktree"]["createdAt"],
        worktree_project["worktree"]["createdAt"],
        "a rename is not a new checkout"
    );
    assert!(renamed.is_dir());
    assert!(!worktree.exists());
    assert_eq!(
        run_git_for_server_test(&renamed, &["branch", "--show-current"]).trim(),
        "feat/kanban-assignee"
    );

    let sessions = route_http(
        state,
        rpc_request(
            "/api/listSessions",
            &token,
            json!({ "params": { "projectId": worktree_project["projectId"] } }),
        ),
        "request-list-renamed-sessions".to_string(),
    )
    .await;
    let sessions = response_json(sessions.response).await;
    let moved = sessions["result"]["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|candidate| candidate["sessionId"] == session["sessionId"])
        .cloned()
        .expect("renamed session");
    assert_eq!(
        moved["cwd"],
        json!(path_to_string(&renamed.join("packages/app"))),
        "a cwd inside the moved folder follows it"
    );
    let moved_marker = &moved["runtimeSettings"][worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY];
    assert_eq!(moved_marker["path"], json!(path_to_string(&renamed)));
    assert_eq!(moved_marker["branch"], json!("feat/kanban-assignee"));
    assert_eq!(moved_marker["initialTitle"], json!("Codex Session"));
}

/// A worktree given another parent by hand gets its checkout's parent back when a rename records
/// its new state (`apply_renamed_worktree_project_state`, here without moving the folder, which git
/// would do from the hand-set parent): the worktree metadata is detected again inside its
/// transaction, which moves the project, so the rename takes the placement lock before the
/// transaction, and the change is written rather than refused.
#[tokio::test]
async fn a_rename_that_gives_a_worktree_its_checkouts_parent_back_is_recorded() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("reparented-parent");
    let worktree = paths.root_dir.join("reparented-parent-old");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "ghostex/4567cdef"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "ghostex/4567cdef",
        ],
    );
    let parent_project =
        add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;
    {
        let db = crate::storage::open_gxserver_database(&paths).expect("db");
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        // Next to the parent, in the path form the parent and the worktree were registered under.
        let other_folder = std::path::Path::new(parent_project["path"].as_str().unwrap())
            .with_file_name("reparented-other");
        std::fs::create_dir_all(&other_folder).expect("other folder");
        let other = repository
            .create_project(
                json!({ "name": "Other", "path": path_to_string(&other_folder) })
                    .as_object()
                    .unwrap(),
            )
            .expect("other project");
        let mut reparented = worktree_project["worktree"].clone();
        reparented["parentProjectId"] = other["projectId"].clone();
        reparented["parentProjectName"] = other["name"].clone();
        reparented["parentProjectPath"] = other["path"].clone();
        repository
            .update_project(
                json!({ "projectId": worktree_project["projectId"], "worktree": reparented })
                    .as_object()
                    .unwrap(),
            )
            .expect("reparented");
    }

    let plan = RenameWorktreeProjectPlan {
        destination_path: worktree_project["path"].as_str().unwrap().to_string(),
        moves_folder: false,
        params: RenameWorktreeProjectParams {
            name: "old".to_string(),
            project_id: worktree_project["projectId"].as_str().unwrap().to_string(),
            rename_branch: false,
        },
        parent_path: parent_project["path"].as_str().unwrap().to_string(),
        parent_project_name: "Parent".to_string(),
        projects: Vec::new(),
        worktree_branch: None,
        worktree_path: worktree_project["path"].as_str().unwrap().to_string(),
        worktree_path_resolved: None,
    };
    let project = match apply_renamed_worktree_project_state(&state, &plan, None) {
        Ok(project) => project,
        Err(_) => panic!("the rename's state was not recorded"),
    };
    assert_eq!(
        project["worktree"]["parentProjectId"],
        parent_project["projectId"]
    );
}

#[test]
fn a_session_cwd_recorded_through_the_resolved_path_form_still_follows_the_move() {
    /*
    CDXC:Worktrees 2026-08-10:
    Nothing forces a session's stored `cwd` to use the same spelling of a
    folder that the project row happens to carry, and on macOS every path
    under `/tmp` or `/var` has two: `/tmp/rt-old` and `/private/tmp/rt-old`.
    Compared lexically against the project row's spelling alone, a cwd
    recorded through the other one matched no prefix, was left untouched, and
    pointed into a folder that had just moved — which breaks that session's
    next cold start, because `start_session_provider` bakes the cwd into the
    generated run script. Both spellings of the old folder rebase; anything
    genuinely outside it still does not.
    */
    let plan = RenameWorktreeProjectPlan {
        destination_path: "/tmp/rt-new".to_string(),
        moves_folder: true,
        params: RenameWorktreeProjectParams {
            name: "new".to_string(),
            project_id: "project-1".to_string(),
            rename_branch: false,
        },
        parent_path: "/tmp/rt".to_string(),
        parent_project_name: "Parent".to_string(),
        projects: Vec::new(),
        worktree_branch: None,
        worktree_path: "/tmp/rt-old".to_string(),
        worktree_path_resolved: Some("/private/tmp/rt-old".to_string()),
    };

    assert_eq!(
        rebase_renamed_worktree_path("/tmp/rt-old/packages/app", &plan).as_deref(),
        Some("/tmp/rt-new/packages/app"),
        "the spelling the project row carries"
    );
    assert_eq!(
        rebase_renamed_worktree_path("/private/tmp/rt-old/packages/app", &plan).as_deref(),
        Some("/tmp/rt-new/packages/app"),
        "the spelling the filesystem resolves to"
    );
    assert_eq!(
        rebase_renamed_worktree_path("/private/tmp/rt-old", &plan).as_deref(),
        Some("/tmp/rt-new")
    );
    assert_eq!(
        rebase_renamed_worktree_path("/private/tmp/rt-older/src", &plan),
        None,
        "a sibling that merely shares a prefix is not inside the moved folder"
    );
    assert_eq!(
        rebase_renamed_worktree_path("/tmp/somewhere-else", &plan),
        None
    );
}

#[tokio::test]
async fn renaming_a_worktree_refuses_a_taken_folder_and_a_taken_branch() {
    /*
    CDXC:Worktrees 2026-08-09-18:40:
    Both refusals must land BEFORE anything is touched, and both must say
    which name is in the way. The folder case is the important one: with the
    destination already present, `git worktree move` exits 0 and nests the
    checkout one level deeper, so "no error" would otherwise mean "the folder
    is somewhere nobody asked for".
    */
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("rename-guard-parent");
    let worktree = paths.root_dir.join("rename-guard-parent-old");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "ghostex/0123abcd"]);
    run_git_for_server_test(&parent, &["branch", "feat/taken"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "ghostex/0123abcd",
        ],
    );
    fs::create_dir_all(paths.root_dir.join("rename-guard-parent-busy")).expect("busy dir");

    add_project_path_for_server_test(state.clone(), &token, &parent, Some("Parent")).await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let taken_folder = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "busy",
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-rename-taken-folder".to_string(),
    )
    .await;
    assert_eq!(taken_folder.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(taken_folder.response).await;
    assert_eq!(
        body["message"],
        json!("A folder named \"rename-guard-parent-busy\" already exists next to the project.")
    );
    assert!(worktree.is_dir(), "the worktree never moved");

    let taken_branch = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "feat/taken",
                    "projectId": worktree_project["projectId"],
                    "renameBranch": true
                }
            }),
        ),
        "request-rename-taken-branch".to_string(),
    )
    .await;
    assert_eq!(taken_branch.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(taken_branch.response).await;
    assert_eq!(
        body["message"],
        json!("Branch \"feat/taken\" already exists.")
    );
    assert!(
        worktree.is_dir(),
        "a refused branch rename never moves the folder"
    );
    assert_eq!(
        run_git_for_server_test(&worktree, &["branch", "--show-current"]).trim(),
        "ghostex/0123abcd"
    );

    let nothing = route_http(
        state.clone(),
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "old",
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-rename-nothing".to_string(),
    )
    .await;
    assert_eq!(nothing.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(nothing.response).await;
    assert_eq!(body["message"], json!("Nothing to rename."));

    /*
    CDXC:Worktrees 2026-08-10:
    Asking to rename the branch to the name it already carries, on a folder
    that is already correct, changes nothing — and reporting success for it
    tells the user something happened. The checkbox being ticked is not
    enough to call it a rename.
    */
    run_git_for_server_test(&parent, &["branch", "-m", "ghostex/0123abcd", "old"]);
    let no_op_branch = route_http(
        state,
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "old",
                    "projectId": worktree_project["projectId"],
                    "renameBranch": true
                }
            }),
        ),
        "request-rename-noop-branch".to_string(),
    )
    .await;
    assert_eq!(no_op_branch.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(no_op_branch.response).await;
    assert_eq!(body["message"], json!("Nothing to rename."));
}

// Symlinks are the whole subject of this test, and `std::os::unix::fs` does
// not exist off unix — without the gate the module stops compiling there.
#[cfg(unix)]
#[tokio::test]
async fn renaming_explains_a_worktree_registered_through_a_different_path_form() {
    /*
    CDXC:Worktrees 2026-08-09-18:40:
    Reproduces a real failure from manual testing on macOS: the project was
    registered as `/tmp/rt` while its worktree resolved to
    `/private/tmp/rt-old`, because `git worktree list` reports the symlink-
    resolved path and the project kept the typed one. The typed operation
    compares paths lexically by design, so it refused with a sentence about
    `worktreePath` that meant nothing to the user. The rename must explain
    which two things disagree instead of forwarding that.
    */
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let real_root = paths.root_dir.join("symlink-family");
    fs::create_dir_all(&real_root).expect("real root");
    let parent = real_root.join("rt");
    let worktree = real_root.join("rt-old");
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(&parent, &["branch", "feat/old"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feat/old",
        ],
    );

    // The parent is registered through a symlinked alias of the same folder,
    // exactly as `/tmp/rt` aliases `/private/tmp/rt`.
    let alias_root = paths.root_dir.join("symlink-alias");
    std::os::unix::fs::symlink(&real_root, &alias_root).expect("symlink");
    let aliased_parent = alias_root.join("rt");

    add_project_path_for_server_test(state.clone(), &token, &aliased_parent, Some("Parent")).await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let response = route_http(
        state,
        rpc_request(
            "/api/renameWorktreeProject",
            &token,
            json!({
                "params": {
                    "name": "renamed",
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-rename-symlinked-family".to_string(),
    )
    .await;

    assert_eq!(response.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response.response).await;
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("registered under different paths"),
        "expected the mismatch explained, got: {message}"
    );
    assert!(
        !message.contains("worktreePath"),
        "the internal typed-operation sentence must not reach the user: {message}"
    );
    assert!(worktree.is_dir(), "nothing was touched");
}

#[tokio::test]
async fn delete_worktree_project_route_force_removes_dirty_checkout_and_warns_for_remote() {
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let remote = paths.root_dir.join("delete-worktree-origin.git");
    let parent = paths.root_dir.join("delete-worktree-remote-parent");
    let worktree = paths.root_dir.join("delete-worktree-remote-parent-feature");
    run_git_for_server_test(
        &paths.root_dir,
        &["init", "--bare", path_to_string(&remote).as_str()],
    );
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &["remote", "add", "origin", path_to_string(&remote).as_str()],
    );
    run_git_for_server_test(&parent, &["push", "-u", "origin", "HEAD:main"]);
    run_git_for_server_test(&parent, &["branch", "feature-remote"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feature-remote",
        ],
    );
    fs::write(worktree.join("dirty.txt"), "not committed\n").expect("dirty file");

    add_project_path_for_server_test(
        state.clone(),
        &token,
        &parent,
        Some("Delete Worktree Remote Parent"),
    )
    .await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let response = route_http(
        state.clone(),
        rpc_request(
            "/api/deleteWorktreeProject",
            &token,
            json!({
                "params": {
                    "deleteRemoteBranch": true,
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-delete-dirty-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    assert_eq!(
        body["result"]["checkoutRemoval"],
        json!({ "forced": true, "retriedForSubmodules": false })
    );
    assert!(body["result"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "remoteBranchDeleteFailed"));
    assert!(!worktree.exists());

    let projects = route_http(
        state,
        rpc_request("/api/listProjects", &token, json!({ "params": {} })),
        "request-list-after-delete-dirty".to_string(),
    )
    .await;
    let body = response_json(projects.response).await;
    assert!(!body["result"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["projectId"] == worktree_project["projectId"]));
}

#[tokio::test]
async fn delete_worktree_project_route_retries_clean_initialized_submodule_with_force() {
    if !git_available() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let submodule = paths.root_dir.join("delete-worktree-submodule-source");
    let parent = paths.root_dir.join("delete-worktree-submodule-parent");
    let worktree = paths
        .root_dir
        .join("delete-worktree-submodule-parent-feature");
    create_git_repository_for_server_test(&submodule);
    create_git_repository_for_server_test(&parent);
    run_git_for_server_test(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            path_to_string(&submodule).as_str(),
            "deps/submodule",
        ],
    );
    run_git_for_server_test(&parent, &["commit", "-m", "add submodule"]);
    run_git_for_server_test(&parent, &["branch", "feature-submodule"]);
    run_git_for_server_test(
        &parent,
        &[
            "worktree",
            "add",
            path_to_string(&worktree).as_str(),
            "feature-submodule",
        ],
    );
    run_git_for_server_test(
        &worktree,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "--init",
            "--recursive",
        ],
    );

    add_project_path_for_server_test(
        state.clone(),
        &token,
        &parent,
        Some("Delete Worktree Submodule Parent"),
    )
    .await;
    let worktree_project =
        add_project_path_for_server_test(state.clone(), &token, &worktree, None).await;

    let response = route_http(
        state,
        rpc_request(
            "/api/deleteWorktreeProject",
            &token,
            json!({
                "params": {
                    "projectId": worktree_project["projectId"]
                }
            }),
        ),
        "request-delete-submodule-worktree".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    assert_eq!(
        body["result"]["checkoutRemoval"],
        json!({ "forced": true, "retriedForSubmodules": true })
    );
    assert!(!worktree.exists());
}
