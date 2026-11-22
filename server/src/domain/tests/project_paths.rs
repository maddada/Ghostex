use super::*;
use serde_json::{json, Value};
use std::path::PathBuf;

#[test]
fn add_project_path_normalizes_nullish_fallback_and_deduplicates_path_syntax() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let parent = temp.path().join("paths");
    let repo_path = parent.join("repo");
    let fallback_path = parent.join("fallback");
    std::fs::create_dir_all(&repo_path).expect("repo dir");
    std::fs::create_dir_all(&fallback_path).expect("fallback dir");

    let first_input = repo_path.join("..").join("repo").join(".");
    let first = repository
        .add_project_path(
            json!({ "path": path_str(&first_input) })
                .as_object()
                .expect("first params"),
        )
        .expect("first add");
    assert_eq!(value_str(&first, "path"), path_str(&repo_path));
    assert_eq!(value_str(&first, "name"), "repo");

    let trailing_input = format!("{}/", path_str(&repo_path));
    let second = repository
        .add_project_path(
            json!({ "path": trailing_input })
                .as_object()
                .expect("second params"),
        )
        .expect("second add");
    assert_eq!(
        value_str(&second, "projectId"),
        value_str(&first, "projectId")
    );
    assert_eq!(repository.list_projects().expect("projects").len(), 1);

    let fallback = repository
        .add_project_path(
            json!({ "path": null, "projectPath": path_str(&fallback_path) })
                .as_object()
                .expect("fallback params"),
        )
        .expect("null path falls back to projectPath");
    assert_eq!(value_str(&fallback, "path"), path_str(&fallback_path));

    let empty_error = repository
        .add_project_path(
            json!({ "path": "", "projectPath": path_str(&repo_path) })
                .as_object()
                .expect("empty params"),
        )
        .expect_err("blank path does not fall back");
    assert_eq!(empty_error.code, "badRequest");
    assert_eq!(empty_error.message, "path must be a non-empty path.");
}

#[test]
fn add_project_path_creates_workspace_root_when_create_if_missing_is_requested() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let missing = temp.path().join("created-parent").join("created-project");

    let without_flag = repository
        .add_project_path(
            json!({ "path": path_str(&missing) })
                .as_object()
                .expect("params"),
        )
        .expect_err("missing path is rejected without the flag");
    assert_eq!(without_flag.code, "notFound");
    assert!(!missing.exists());

    let project = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&missing) })
                .as_object()
                .expect("params"),
        )
        .expect("missing path created and registered");
    assert_eq!(value_str(&project, "path"), path_str(&missing));
    assert_eq!(value_str(&project, "name"), "created-project");
    assert!(missing.is_dir());

    let repeated = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&missing) })
                .as_object()
                .expect("params"),
        )
        .expect("second add is idempotent");
    assert_eq!(
        value_str(&repeated, "projectId"),
        value_str(&project, "projectId")
    );

    let file_path = temp.path().join("create-if-missing-file");
    std::fs::write(&file_path, "file\n").expect("file");
    let file_error = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&file_path) })
                .as_object()
                .expect("params"),
        )
        .expect_err("existing file is still rejected");
    assert_eq!(file_error.code, "badRequest");
    assert_eq!(
        file_error.message,
        format!("path is not a directory: {}", path_str(&file_path))
    );

    let unwritable = file_path.join("child");
    let create_error = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&unwritable) })
                .as_object()
                .expect("params"),
        )
        .expect_err("mkdir failure surfaces the workspace-root message");
    assert_eq!(create_error.code, "badRequest");
    assert_eq!(
        create_error.message,
        format!("Failed to create workspace root: {}", path_str(&unwritable))
    );
}

#[test]
fn add_project_path_rejects_invalid_path_inputs_with_typescript_messages() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let missing_input = temp
        .path()
        .join("missing-parent")
        .join("..")
        .join("missing");
    let missing_normalized = temp.path().join("missing");
    let file_path = temp.path().join("not-a-directory");
    std::fs::write(&file_path, "file\n").expect("file");

    let cases = vec![
        (
            json!({ "path": null }),
            "badRequest",
            "path must be a non-empty path.".to_string(),
        ),
        (
            json!({ "path": 42 }),
            "badRequest",
            "path must be a non-empty path.".to_string(),
        ),
        (
            json!({ "path": "   " }),
            "badRequest",
            "path must be a non-empty path.".to_string(),
        ),
        (
            json!({ "path": "relative/repo" }),
            "badRequest",
            "path must be an absolute path or start with ~/".to_string(),
        ),
        (
            json!({ "path": path_str(&missing_input) }),
            "notFound",
            format!("path does not exist: {}", path_str(&missing_normalized)),
        ),
        (
            json!({ "path": path_str(&file_path) }),
            "badRequest",
            format!("path is not a directory: {}", path_str(&file_path)),
        ),
    ];

    for (params, code, message) in cases {
        let error = repository
            .add_project_path(params.as_object().expect("params"))
            .expect_err("invalid add path rejected");
        assert_eq!(error.code, code);
        assert_eq!(error.message, message);
    }
}

#[test]
fn normalize_existing_directory_path_expands_home_shortcut() {
    let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    else {
        return;
    };
    let normalized = normalize_existing_directory_path(Some(&json!("~")), "path")
        .expect("home shortcut normalized");
    assert_eq!(normalized, path_to_string(&resolve_path_syntax(home)));
}

#[test]
fn create_session_project_resolution_uses_nullish_path_fallback() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let repo_path = temp.path().join("session-project");
    std::fs::create_dir_all(&repo_path).expect("repo dir");
    let project = repository
        .add_project_path(
            json!({ "path": path_str(&repo_path) })
                .as_object()
                .expect("project params"),
        )
        .expect("project added");
    let project_id = value_str(&project, "projectId").to_string();

    let cwd_input = repo_path.join("..").join("session-project").join(".");
    let session = repository
        .create_session(
            json!({
                "cwd": path_str(&cwd_input),
                "projectId": "project-local",
                "projectPath": null,
                "title": "Terminal",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("session created");
    assert_eq!(value_str(&session, "projectId"), project_id);
    assert_eq!(value_str(&session, "cwd"), path_str(&cwd_input));

    let empty_error = repository
        .create_session(
            json!({
                "cwd": path_str(&repo_path),
                "projectId": "project-local",
                "projectPath": "",
                "title": "Terminal",
            })
            .as_object()
            .expect("empty session params"),
            false,
        )
        .expect_err("blank projectPath does not fall back to cwd");
    assert_eq!(empty_error.code, "badRequest");
    assert_eq!(
        empty_error.message,
        "Invalid gxserver project ID: project-local."
    );
}

#[test]
fn missing_project_folder_blocks_session_insertion_until_relocated() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let original_path = temp.path().join("original-project");
    let relocated_path = temp.path().join("relocated-project");
    std::fs::create_dir_all(&original_path).expect("original project dir");
    let project = repository
        .add_project_path(
            json!({ "name": "Moved Project", "path": path_str(&original_path) })
                .as_object()
                .expect("project params"),
        )
        .expect("project added");
    let project_id = value_str(&project, "projectId").to_string();
    std::fs::rename(&original_path, &relocated_path).expect("move project dir");

    assert_eq!(project_path_state(&project), ProjectPathState::Missing);
    let error = repository
        .create_session(
            json!({ "projectId": project_id, "title": "Terminal" })
                .as_object()
                .expect("session params"),
            false,
        )
        .expect_err("missing project folder blocks session creation");
    assert_eq!(error.code, "projectPathUnavailable");
    assert!(repository
        .list_sessions(Some(&project_id))
        .expect("sessions")
        .is_empty());

    let relocated = repository
        .relocate_project(
            json!({ "path": path_str(&relocated_path), "projectId": project_id })
                .as_object()
                .expect("relocate params"),
        )
        .expect("project relocated");
    assert_eq!(value_str(&relocated, "projectId"), project_id);
    assert_eq!(value_str(&relocated, "name"), "Moved Project");
    assert_eq!(value_str(&relocated, "path"), path_str(&relocated_path));
    assert_eq!(project_path_state(&relocated), ProjectPathState::Available);

    let session = repository
        .create_session(
            json!({ "projectId": project_id, "title": "Terminal" })
                .as_object()
                .expect("session params"),
            false,
        )
        .expect("session created after relocation");
    assert_eq!(value_str(&session, "projectId"), project_id);
}

#[test]
fn add_project_path_attaches_and_repairs_linked_worktree_metadata() {
    if !git_available() {
        return;
    }

    let (temp, db) = open_test_database();
    let root = temp.path();
    let repo_path = root.join("registered-main");
    let worktree_path = root.join("registered-main-feature");
    let orphan_worktree_path = root.join("registered-main-orphan");
    std::fs::create_dir_all(&repo_path).expect("repo dir");
    run_git_for_test(&repo_path, &["init"]);
    run_git_for_test(
        &repo_path,
        &["config", "user.email", "ghostex@example.invalid"],
    );
    run_git_for_test(&repo_path, &["config", "user.name", "Ghostex Test"]);
    std::fs::write(repo_path.join("README.md"), "main\n").expect("write readme");
    run_git_for_test(&repo_path, &["add", "README.md"]);
    run_git_for_test(&repo_path, &["commit", "-m", "Initial commit"]);
    run_git_for_test(
        &repo_path,
        &[
            "worktree",
            "add",
            "-b",
            "feature/existing-worktree",
            path_str(&worktree_path),
        ],
    );
    run_git_for_test(
        &repo_path,
        &[
            "worktree",
            "add",
            "-b",
            "feature/orphan-worktree",
            path_str(&orphan_worktree_path),
        ],
    );

    let repository = DomainRepository::new(&db, "S7k");
    let main_params = json!({ "name": "Registered Main", "path": path_str(&repo_path) });
    let main_project = repository
        .add_project_path(main_params.as_object().expect("main params"))
        .expect("main project");
    let main_project_id = value_str(&main_project, "projectId").to_string();

    let worktree_params = json!({ "path": path_str(&worktree_path) });
    let worktree_project = repository
        .add_project_path(worktree_params.as_object().expect("worktree params"))
        .expect("worktree project");
    assert_eq!(
        value_str(&worktree_project, "path"),
        path_str(&worktree_path)
    );
    let metadata = object_field(&worktree_project, "worktree");
    assert_eq!(
        metadata.get("parentProjectId").and_then(Value::as_str),
        Some(main_project_id.as_str())
    );
    assert_eq!(
        metadata.get("parentProjectName").and_then(Value::as_str),
        Some("Registered Main")
    );
    assert_eq!(
        metadata.get("parentProjectPath").and_then(Value::as_str),
        Some(path_str(&repo_path))
    );
    assert_eq!(
        metadata.get("branch").and_then(Value::as_str),
        Some("feature/existing-worktree")
    );
    assert!(metadata.get("createdAt").and_then(Value::as_str).is_some());

    let second_add = repository
        .add_project_path(worktree_params.as_object().expect("worktree params"))
        .expect("second worktree add");
    assert_eq!(
        value_str(&second_add, "projectId"),
        value_str(&worktree_project, "projectId")
    );

    let orphan_params =
        json!({ "name": "Orphan Worktree", "path": path_str(&orphan_worktree_path) });
    let orphan_project = repository
        .create_project(orphan_params.as_object().expect("orphan params"))
        .expect("orphan project");
    assert!(orphan_project.get("worktree").is_none());
    let repaired = repository
        .add_project_path(orphan_params.as_object().expect("orphan params"))
        .expect("repaired worktree project");
    assert_eq!(
        value_str(&repaired, "projectId"),
        value_str(&orphan_project, "projectId")
    );
    let repaired_metadata = object_field(&repaired, "worktree");
    assert_eq!(
        repaired_metadata
            .get("parentProjectId")
            .and_then(Value::as_str),
        Some(main_project_id.as_str())
    );
    assert_eq!(
        repaired_metadata.get("branch").and_then(Value::as_str),
        Some("feature/orphan-worktree")
    );
}
