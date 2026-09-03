use super::*;
use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use tempfile::tempdir;

fn context(cwd: &Path) -> TypedOperationContext {
    let project = json!({
        "projectId": "P-test",
        "path": cwd.to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {},
    });
    TypedOperationContext {
        beads_cwd: None,
        cwd: cwd.to_string_lossy().to_string(),
        env_path: None,
        projects: vec![project],
    }
}

#[tokio::test]
async fn project_setup_action_errors_use_javascript_string_conversion() {
    let dir = tempdir().unwrap();
    let ctx = context(dir.path());
    let cases = [
        (
            None,
            "Unsupported project setup action: undefined".to_string(),
        ),
        (
            Some(Value::Null),
            "Unsupported project setup action: null".to_string(),
        ),
        (
            Some(json!(false)),
            "Unsupported project setup action: false".to_string(),
        ),
        (
            Some(json!({"action": "worktreeSetupCommand"})),
            "Unsupported project setup action: [object Object]".to_string(),
        ),
        (
            Some(json!(["worktreeSetupCommand", null, {"x": 1}])),
            "Unsupported project setup action: worktreeSetupCommand,,[object Object]".to_string(),
        ),
    ];

    for (action, expected_message) in cases {
        let mut params = Map::new();
        if let Some(action) = action {
            params.insert("action".to_string(), action);
        }
        let error = run_project_setup_command(&params, &ctx).await.unwrap_err();
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, expected_message);
    }
}

#[tokio::test]
async fn project_setup_source_project_id_lookup_does_not_trim() {
    let dir = tempdir().unwrap();
    let ctx = context(dir.path());
    let mut params = Map::new();
    params.insert("action".to_string(), json!("worktreeSetupCommand"));
    params.insert("setupCommandProjectId".to_string(), json!(" P-test "));

    let error = run_project_setup_command(&params, &ctx).await.unwrap_err();

    assert_eq!(error.code, "notFound");
    assert_eq!(error.message, "Project  P-test  does not exist.");
}

#[tokio::test]
async fn project_setup_blank_source_project_id_still_allows_source_project_path() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    let source_project = json!({
        "projectId": "P-source",
        "path": source.to_string_lossy(),
        "gitConfig": {
            "worktreeCommand": "printf source-selected"
        },
        "projectBoardConfig": {},
    });
    let target_project = json!({
        "projectId": "P-target",
        "path": target.to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {},
    });
    let ctx = TypedOperationContext {
        beads_cwd: None,
        cwd: target.to_string_lossy().to_string(),
        env_path: None,
        projects: vec![source_project, target_project],
    };
    let mut params = Map::new();
    params.insert("action".to_string(), json!("worktreeSetupCommand"));
    params.insert("setupCommandProjectId".to_string(), json!("   "));
    params.insert(
        "setupCommandProjectPath".to_string(),
        json!(source.to_string_lossy()),
    );

    let result = run_project_setup_command(&params, &ctx).await.unwrap();

    assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
    assert_eq!(
        result.get("stdout").and_then(Value::as_str),
        Some("source-selected")
    );
    assert_eq!(
        result
            .get("command")
            .and_then(|command| command.get("args"))
            .cloned(),
        Some(json!(["-lc", "<worktree setup command>"]))
    );
}

#[tokio::test]
async fn project_setup_endpoint_scope_project_id_lookup_does_not_trim() {
    let dir = tempdir().unwrap();
    let project = json!({
        "projectId": "P-test",
        "path": dir.path().to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {},
    });
    let mut params = Map::new();
    params.insert("action".to_string(), json!("worktreeSetupCommand"));
    params.insert("projectId".to_string(), json!(" P-test "));

    let error =
        dispatch_typed_operation_endpoint("/api/runProjectSetupCommand", &params, vec![project])
            .await
            .unwrap_err();

    assert_eq!(error.code, "notFound");
    assert_eq!(error.message, "Project  P-test  does not exist.");
    assert!(error.scope_rejection);
}

#[tokio::test]
async fn typed_operation_scope_matches_path_resolve_for_project_path_lookup() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let project = json!({
        "projectId": "P-test",
        "path": repo.to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {},
    });
    let unresolved_intermediate_path = dir
        .path()
        .join("missing-intermediate")
        .join("..")
        .join("repo");
    let mut params = Map::new();
    params.insert("action".to_string(), json!("worktreeSetupCommand"));
    params.insert(
        "projectPath".to_string(),
        json!(unresolved_intermediate_path.to_string_lossy()),
    );

    let result =
        dispatch_typed_operation_endpoint("/api/runProjectSetupCommand", &params, vec![project])
            .await
            .unwrap();

    assert_eq!(
        result.get("action").and_then(Value::as_str),
        Some("worktreeSetupCommand")
    );
    assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
}

#[tokio::test]
async fn project_setup_command_path_reports_file_without_echoing_path() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("not-a-directory");
    fs::write(&file_path, "x").unwrap();
    let ctx = context(dir.path());
    let mut params = Map::new();
    params.insert("action".to_string(), json!("worktreeSetupCommand"));
    params.insert(
        "setupCommandProjectPath".to_string(),
        json!(file_path.to_string_lossy()),
    );

    let error = run_project_setup_command(&params, &ctx).await.unwrap_err();

    assert_eq!(error.code, "badRequest");
    assert_eq!(error.message, "setupCommandProjectPath is not a directory.");
}

#[tokio::test]
async fn project_setup_default_lookup_normalizes_context_cwd() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let project = json!({
        "projectId": "P-test",
        "path": repo.to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {},
    });
    let ctx = TypedOperationContext {
        beads_cwd: None,
        cwd: repo.join(".").to_string_lossy().to_string(),
        env_path: None,
        projects: vec![project],
    };
    let mut params = Map::new();
    params.insert("action".to_string(), json!("worktreeSetupCommand"));

    let result = run_project_setup_command(&params, &ctx).await.unwrap();

    assert_eq!(
        result.get("action").and_then(Value::as_str),
        Some("worktreeSetupCommand")
    );
    assert_eq!(result.get("command"), None);
    assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
}

#[tokio::test]
async fn project_setup_default_lookup_validates_candidate_project_paths() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let stale = dir.path().join("stale");
    fs::create_dir(&repo).unwrap();
    let stale_project = json!({
        "projectId": "P-stale",
        "path": stale.to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {},
    });
    let project = json!({
        "projectId": "P-test",
        "path": repo.to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {},
    });
    let ctx = TypedOperationContext {
        beads_cwd: None,
        cwd: repo.to_string_lossy().to_string(),
        env_path: None,
        projects: vec![stale_project, project],
    };
    let mut params = Map::new();
    params.insert("action".to_string(), json!("worktreeSetupCommand"));

    let error = run_project_setup_command(&params, &ctx).await.unwrap_err();

    assert_eq!(error.code, "notFound");
    assert_eq!(
        error.message,
        format!("project.path does not exist: {}", stale.to_string_lossy())
    );
}

#[test]
fn git_commit_redacts_stdin_command_metadata() {
    let dir = tempdir().unwrap();
    let mut params = Map::new();
    params.insert("messageSubject".to_string(), json!("secret subject"));
    params.insert("messageBody".to_string(), json!("secret body"));
    params.insert("noVerify".to_string(), json!(true));
    let command = build_git_command("commit", &params, &dir.path().to_string_lossy()).unwrap();
    assert_eq!(
        command.summary().args,
        vec!["commit", "--no-verify", "-F", "<stdin>"]
    );
    assert!(command.stdin.unwrap().contains("secret subject"));
}

#[test]
fn typed_operation_log_details_omit_command_args_and_output() {
    let result = json!({
        "action": "commit",
        "command": {
            "args": ["commit", "-F", "<stdin>"],
            "cwd": "/Users/person/dev/private-project",
            "executable": "git",
        },
        "exitCode": 1,
        "stderr": "private stderr containing command text",
        "stdout": "private stdout containing command text",
    });

    let details = typed_operation_log_details(&result);

    assert_eq!(details.get("action"), Some(&json!("commit")));
    assert_eq!(details.get("argumentCount"), Some(&json!(3)));
    assert_eq!(details.get("commandBuilt"), Some(&json!(true)));
    assert_eq!(details.get("executable"), Some(&json!("git")));
    assert_eq!(details.get("exitCode"), Some(&json!(1)));
    assert_eq!(details.get("operationError"), Some(&Value::Null));
    assert!(!details.to_string().contains("private-project"));
    assert!(!details.to_string().contains("<stdin>"));
    assert!(!details.to_string().contains("private stdout"));
    assert!(!details.to_string().contains("private stderr"));
}

#[test]
fn git_pull_fast_forward_plans_ff_only_command() {
    let dir = tempdir().unwrap();
    let command = build_git_command(
        "pullFastForward",
        &Map::new(),
        &dir.path().to_string_lossy(),
    )
    .unwrap();
    assert_eq!(command.args, vec!["pull", "--ff-only"]);
    assert_eq!(command.summary().args, vec!["pull", "--ff-only"]);
    assert_eq!(
        normalize_git_action(Some(&json!("pullFastForward"))).unwrap(),
        "pullFastForward"
    );
}

#[test]
fn git_list_branches_plans_structured_branch_command() {
    /*
    CDXC:Worktrees 2026-06-24-11:32:
    Add Worktree branch selection must use a typed Git action that lists
    local and remote-tracking branch refs without exposing a generic shell.
    */
    let dir = tempdir().unwrap();
    let command =
        build_git_command("listBranches", &Map::new(), &dir.path().to_string_lossy()).unwrap();

    assert_eq!(
        command.args,
        vec![
            "for-each-ref",
            "--format=%(refname:short)%09%(refname)%09%(HEAD)",
            "refs/heads",
            "refs/remotes",
        ]
    );
    assert_eq!(
        normalize_git_action(Some(&json!("listBranches"))).unwrap(),
        "listBranches"
    );
}

#[test]
fn git_branch_list_parser_filters_symbolic_and_invalid_refs() {
    let branches = parse_git_branch_list(
        "main\trefs/heads/main\t*\n\
feature/worktree-base\trefs/heads/feature/worktree-base\t\n\
origin/main\trefs/remotes/origin/main\t\n\
origin/HEAD\trefs/remotes/origin/HEAD\t\n\
bad branch\trefs/heads/bad branch\t\n",
    );

    assert_eq!(
        branches,
        vec![
            json!({"current": true, "name": "main", "remote": false}),
            json!({"current": false, "name": "feature/worktree-base", "remote": false}),
            json!({"current": false, "name": "origin/main", "remote": true}),
        ]
    );
}

#[tokio::test]
async fn git_pull_fast_forward_executes_local_remote_update() {
    let root = tempdir().unwrap();
    let remote = root.path().join("remote.git");
    let seed = root.path().join("seed");
    let repo = root.path().join("repo");
    fs::create_dir(&seed).unwrap();

    run_git(["init", "--bare", remote.to_str().unwrap()], root.path());
    run_git(["init"], &seed);
    run_git(["checkout", "-b", "main"], &seed);
    run_git(["config", "user.email", "typed@example.invalid"], &seed);
    run_git(["config", "user.name", "Typed Operation Test"], &seed);
    fs::write(seed.join("file.txt"), "one\n").unwrap();
    run_git(["add", "file.txt"], &seed);
    run_git(["commit", "-m", "initial"], &seed);
    run_git(["remote", "add", "origin", remote.to_str().unwrap()], &seed);
    run_git(["push", "-u", "origin", "main"], &seed);
    run_git(
        [
            "--git-dir",
            remote.to_str().unwrap(),
            "symbolic-ref",
            "HEAD",
            "refs/heads/main",
        ],
        root.path(),
    );
    run_git(
        ["clone", remote.to_str().unwrap(), repo.to_str().unwrap()],
        root.path(),
    );

    fs::write(seed.join("file.txt"), "one\ntwo\n").unwrap();
    run_git(["add", "file.txt"], &seed);
    run_git(["commit", "-m", "second"], &seed);
    run_git(["push"], &seed);

    let ctx = context(&repo);
    let mut params = Map::new();
    params.insert("action".to_string(), json!("pullFastForward"));
    let result = run_git_action(&params, &ctx).await.unwrap();
    assert_eq!(
        result.get("action").and_then(Value::as_str),
        Some("pullFastForward")
    );
    assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
    assert_eq!(
        result
            .get("command")
            .and_then(|command| command.get("args"))
            .cloned()
            .unwrap(),
        json!(["pull", "--ff-only"])
    );
    assert_eq!(
        fs::read_to_string(repo.join("file.txt")).unwrap(),
        "one\ntwo\n"
    );
}

#[test]
fn worktree_target_stays_inside_family_root() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("repo");
    fs::create_dir(&source).unwrap();
    let ctx = context(&source);
    let mut params = Map::new();
    params.insert(
        "worktreePath".to_string(),
        json!(dir.path().join("repo-two").to_string_lossy()),
    );
    assert!(normalize_worktree_target_path(params.get("worktreePath"), &ctx).is_ok());
    params.insert(
        "worktreePath".to_string(),
        json!("/tmp/outside-worktree-family"),
    );
    assert_eq!(
        normalize_worktree_target_path(params.get("worktreePath"), &ctx)
            .unwrap_err()
            .code,
        "forbidden"
    );
}

#[test]
fn worktree_move_builds_a_git_worktree_move_command() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("repo");
    let worktree = dir.path().join("repo-old");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&worktree).unwrap();
    let destination = dir.path().join("repo-new");
    let ctx = context(&source);
    let mut params = Map::new();
    params.insert(
        "worktreePath".to_string(),
        json!(worktree.to_string_lossy()),
    );
    params.insert(
        "destinationPath".to_string(),
        json!(destination.to_string_lossy()),
    );

    let command = build_worktree_command("move", &params, &ctx).unwrap();

    assert_eq!(command.executable, "git");
    assert_eq!(
        command.args,
        vec![
            "worktree".to_string(),
            "move".to_string(),
            "--".to_string(),
            normalize_path_string(worktree.clone()),
            normalize_path_string(destination),
        ]
    );
    assert_eq!(command.cwd, source.to_string_lossy());
}

#[test]
fn worktree_move_rejects_an_existing_destination() {
    /*
    CDXC:Worktrees 2026-08-09-18:40:
    This is the regression test for the worst failure mode in the rename
    feature. `git worktree move A B` with B already present exits 0 and
    nests the worktree at B/A, so the operation "succeeds" while the folder
    lands somewhere nobody asked for and the registered project path becomes
    wrong. The guard has to refuse before git runs; reading the result back
    is too late.
    */
    let dir = tempdir().unwrap();
    let source = dir.path().join("repo");
    let worktree = dir.path().join("repo-old");
    let destination = dir.path().join("repo-taken");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&worktree).unwrap();
    fs::create_dir(&destination).unwrap();
    let ctx = context(&source);
    let mut params = Map::new();
    params.insert(
        "worktreePath".to_string(),
        json!(worktree.to_string_lossy()),
    );
    params.insert(
        "destinationPath".to_string(),
        json!(destination.to_string_lossy()),
    );

    let error = build_worktree_command("move", &params, &ctx).unwrap_err();

    assert_eq!(error.code, "badRequest");
    assert_eq!(error.message, "destinationPath already exists.");
}

#[test]
fn worktree_move_rejects_a_destination_outside_the_family_directory() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("repo");
    let worktree = dir.path().join("repo-old");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&worktree).unwrap();
    let ctx = context(&source);
    let mut params = Map::new();
    params.insert(
        "worktreePath".to_string(),
        json!(worktree.to_string_lossy()),
    );
    params.insert(
        "destinationPath".to_string(),
        json!("/tmp/outside-worktree-family-rename"),
    );

    let error = build_worktree_command("move", &params, &ctx).unwrap_err();

    assert_eq!(error.code, "forbidden");
    assert!(error.message.contains("destinationPath"));

    params.insert(
        "destinationPath".to_string(),
        json!(source.to_string_lossy()),
    );
    assert_eq!(
        build_worktree_command("move", &params, &ctx)
            .unwrap_err()
            .code,
        "forbidden",
        "the main checkout is never a destination"
    );
}

#[test]
fn worktree_rename_branch_builds_a_git_branch_dash_m_command() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("repo");
    fs::create_dir(&source).unwrap();
    let ctx = context(&source);
    let mut params = Map::new();
    params.insert("branch".to_string(), json!("ghostex/0123abcd"));
    params.insert("newBranch".to_string(), json!("feat/kanban-assignee"));

    let command = build_worktree_command("renameBranch", &params, &ctx).unwrap();

    assert_eq!(command.executable, "git");
    assert_eq!(
        command.args,
        vec![
            "branch".to_string(),
            "-m".to_string(),
            "--".to_string(),
            "ghostex/0123abcd".to_string(),
            "feat/kanban-assignee".to_string(),
        ]
    );
    assert!(
        !command.args.iter().any(|arg| arg == "-M"),
        "force rename would clobber an existing branch and does not help with namespace collisions"
    );
}

#[test]
fn unsupported_worktree_actions_are_still_rejected() {
    /*
    CDXC:Worktrees 2026-08-09-18:40:
    The rename feature widened the worktree allowlist. Prove it is still an
    allowlist and did not become a wildcard.
    */
    for action in ["remove-branch", "moveAll", "renameBranchForce", "submodule"] {
        let error = normalize_worktree_action(Some(&json!(action))).unwrap_err();
        assert_eq!(error.code, "badRequest", "{action}");
    }
    for action in [
        "create",
        "hasPopulatedSubmodules",
        "move",
        "renameBranch",
        "remove",
    ] {
        assert_eq!(
            normalize_worktree_action(Some(&json!(action))).unwrap(),
            action
        );
    }
}

#[test]
fn beads_board_filters_non_object_rows() {
    let (issues, stdout) =
        parse_beads_board_output(r#"{"data":[{"id":"A"},1,{"id":"B"}]}"#).unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(stdout, r#"[{"id":"A"},{"id":"B"}]"#);
}

#[test]
fn beads_board_rejects_explicit_row_and_response_limits() {
    let row_error = parse_beads_board_output_with_limits(
        r#"[{"id":"A"},{"id":"B"},{"id":"C"}]"#,
        BeadsBoardLimits {
            response_limit_bytes: 1024,
            row_limit: 2,
        },
    )
    .unwrap_err();
    assert_eq!(row_error.code, "badRequest");
    assert!(row_error.message.contains("2-row limit"));
    assert_eq!(
        row_error
            .details
            .as_ref()
            .and_then(|details| details.get("rowLimit")),
        Some(&json!(2))
    );

    let response_error = parse_beads_board_output_with_limits(
        r#"[{"id":"A","title":"response body is too large"}]"#,
        BeadsBoardLimits {
            response_limit_bytes: 16,
            row_limit: 10,
        },
    )
    .unwrap_err();
    assert_eq!(response_error.code, "badRequest");
    assert!(response_error
        .message
        .contains("16-byte serialized JSON limit"));
    assert_eq!(
        response_error
            .details
            .as_ref()
            .and_then(|details| details.get("responseLimitBytes")),
        Some(&json!(16))
    );
}

#[test]
fn beads_hook_script_matches_typescript_contract() {
    let script = build_ghostex_beads_git_hook_script(
        "pre-commit",
        "/usr/local/bin/bd",
        "/tmp/project/.beads",
    );
    assert!(script.contains("BD_BIN='/usr/local/bin/bd'"));
    assert!(script.contains("BEADS_DIR_VALUE='/tmp/project/.beads'"));
    assert!(script.contains("HOOK_NAME='pre-commit'"));
    assert!(script.contains("export BEADS_DIR=\"$BEADS_DIR_VALUE\""));
    assert!(script.contains("export BD_GIT_HOOK=1"));
    assert!(script.contains("hooks run \"$HOOK_NAME\""));
    assert!(!script.contains("bd sync"));
    assert!(!script.contains("issues.jsonl"));

    let quoted = shell_single_quote("/tmp/it's/bd");
    assert_eq!(quoted, "'/tmp/it'\\''s/bd'");
}

#[tokio::test]
async fn ensure_beads_hooks_installs_scripts_in_common_git_directory() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let system_bin = dir.path().join("system-bin");
    let bd = system_bin.join("bd");
    let beads_dir = repo.join(".beads");
    let hook_log = dir.path().join("hook.log");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&beads_dir).unwrap();
    fs::create_dir_all(&system_bin).unwrap();
    run_git(["init"], &repo);
    fs::write(
        &bd,
        format!(
            "#!/bin/sh\n\
if [ \"$1\" = \"where\" ]; then\n\
  printf '%s\\n' '{{\"path\":\"{}\"}}'\n\
  exit 0\n\
fi\n\
if [ \"$1\" = \"hooks\" ] && [ \"$2\" = \"run\" ]; then\n\
  printf '%s\\n' \"$BEADS_DIR|$BD_GIT_HOOK|$3\" >> {}\n\
  exit 0\n\
fi\n\
exit 0\n",
            beads_dir.to_string_lossy(),
            shell_single_quote(&hook_log.to_string_lossy()),
        ),
    )
    .unwrap();
    chmod_executable_if_supported(&bd).unwrap();

    let ctx = context(&repo);
    let result = ensure_beads_git_hooks_with_executable(&ctx, &bd.to_string_lossy())
        .await
        .unwrap();
    assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
    assert_eq!(
        result.get("stdout").and_then(Value::as_str),
        Some("installed")
    );

    let hooks_path = repo.join(".git").join("ghostex-hooks");
    assert_eq!(
        run_git_output(["config", "--get", "core.hooksPath"], &repo).trim(),
        hooks_path.to_string_lossy()
    );
    let pre_commit = fs::read_to_string(hooks_path.join("pre-commit")).unwrap();
    assert!(pre_commit.contains("BD_BIN="));
    assert!(pre_commit.contains("BEADS_DIR_VALUE="));
    assert!(pre_commit.contains("HOOK_NAME='pre-commit'"));
    assert!(pre_commit.contains("hooks run \"$HOOK_NAME\""));
    assert!(!pre_commit.contains("issues.jsonl"));

    let hook_output = StdCommand::new(hooks_path.join("pre-commit"))
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        hook_output.status.success(),
        "hook failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&hook_output.stdout),
        String::from_utf8_lossy(&hook_output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&hook_log).unwrap().trim(),
        format!("{}|1|pre-commit", beads_dir.to_string_lossy())
    );
}

#[test]
fn beads_where_output_requires_absolute_directory() {
    let dir = tempdir().unwrap();
    let beads_dir = dir.path().join(".beads");
    fs::create_dir(&beads_dir).unwrap();
    let stdout = format!(r#"{{"path":"{}"}}"#, beads_dir.to_string_lossy());
    assert_eq!(
        normalize_beads_where_directory(&stdout).unwrap(),
        beads_dir.to_string_lossy()
    );

    let relative = normalize_beads_where_directory(r#"{"path":".beads"}"#).unwrap_err();
    assert_eq!(relative.code, "badRequest");
    assert!(relative.message.contains("absolute storage path"));

    let file_path = dir.path().join("not-a-dir");
    fs::write(&file_path, "x").unwrap();
    let file_stdout = format!(r#"{{"path":"{}"}}"#, file_path.to_string_lossy());
    let file_error = normalize_beads_where_directory(&file_stdout).unwrap_err();
    assert_eq!(file_error.code, "badRequest");
    assert!(file_error.message.contains("not a directory"));
}

#[test]
fn beads_commands_match_typescript_nullish_argument_behavior() {
    let dir = tempdir().unwrap();
    let ctx = context(dir.path());

    let mut create_params = Map::new();
    create_params.insert("title".to_string(), json!("Create from board"));
    create_params.insert("description".to_string(), Value::Null);
    create_params.insert("priority".to_string(), Value::Null);
    let create =
        build_beads_command_with_executable("create", &create_params, &ctx, "/tmp/bd").unwrap();
    assert_eq!(
        create.args,
        vec![
            "create",
            "--title",
            "Create from board",
            "--description",
            "",
            "--priority",
            "2",
            "--type",
            "task",
            "--json",
        ]
    );

    let mut update_description_params = Map::new();
    update_description_params.insert("issueId".to_string(), json!("gxserver-15"));
    update_description_params.insert("description".to_string(), Value::Null);
    let update_description = build_beads_command_with_executable(
        "updateDescription",
        &update_description_params,
        &ctx,
        "/tmp/bd",
    )
    .unwrap();
    assert_eq!(
        update_description.args,
        vec!["update", "gxserver-15", "--description", "", "--json"]
    );

    let mut update_params = Map::new();
    update_params.insert("issueId".to_string(), json!("gxserver-15"));
    update_params.insert("description".to_string(), Value::Null);
    let update =
        build_beads_command_with_executable("update", &update_params, &ctx, "/tmp/bd").unwrap();
    assert_eq!(
        update.args,
        vec!["update", "gxserver-15", "--description", "null", "--json"]
    );
}

#[test]
fn beads_board_scope_controls_command_cwd_only_for_board_calls() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let board_dir = dir.path().join("board");
    fs::create_dir(&repo).unwrap();
    fs::create_dir(&board_dir).unwrap();
    let project = json!({
        "projectId": "P-test",
        "path": repo.to_string_lossy(),
        "gitConfig": {},
        "projectBoardConfig": {
            "beadsDirectory": board_dir.to_string_lossy()
        },
    });

    let mut board_params = Map::new();
    board_params.insert("action".to_string(), json!("list"));
    board_params.insert("projectBoardScope".to_string(), json!(true));
    board_params.insert("projectId".to_string(), json!("P-test"));
    let board_context = resolve_project_operation_context(
        "/api/runBeadsAction",
        &board_params,
        vec![project.clone()],
    )
    .unwrap();
    let board_command =
        build_beads_command_with_executable("list", &board_params, &board_context, "/tmp/bd")
            .unwrap();
    assert_eq!(board_command.cwd, board_dir.to_string_lossy());

    let mut probe_params = Map::new();
    probe_params.insert("action".to_string(), json!("storageExists"));
    probe_params.insert("projectId".to_string(), json!("P-test"));
    let probe_context = resolve_project_operation_context(
        "/api/runBeadsAction",
        &probe_params,
        vec![project.clone()],
    )
    .unwrap();
    assert_eq!(probe_context.beads_cwd, None);

    let worktree_context =
        resolve_project_operation_context("/api/runWorktreeAction", &board_params, vec![project])
            .unwrap();
    assert_eq!(worktree_context.beads_cwd, None);
}

#[test]
fn beads_label_arguments_reject_non_arrays() {
    let dir = tempdir().unwrap();
    let ctx = context(dir.path());

    let mut create_params = Map::new();
    create_params.insert("title".to_string(), json!("Create from board"));
    create_params.insert("labels".to_string(), json!("ui"));
    let create_error =
        build_beads_command_with_executable("create", &create_params, &ctx, "/tmp/bd").unwrap_err();
    assert_eq!(create_error.code, "badRequest");
    assert!(create_error.message.contains("labels must be an array"));

    let mut set_params = Map::new();
    set_params.insert("issueId".to_string(), json!("gxserver-15"));
    set_params.insert("labels".to_string(), json!("ui"));
    let set_error =
        build_beads_command_with_executable("setLabels", &set_params, &ctx, "/tmp/bd").unwrap_err();
    assert_eq!(set_error.code, "badRequest");
    assert!(set_error.message.contains("labels must be an array"));
}

#[test]
fn beads_show_requests_comment_bodies() {
    let dir = tempdir().unwrap();
    let ctx = context(dir.path());
    let mut params = Map::new();
    params.insert("issueId".to_string(), json!("gxserver-57"));

    let command = build_beads_command_with_executable("show", &params, &ctx, "/tmp/bd").unwrap();

    assert_eq!(
        command.args,
        vec!["show", "gxserver-57", "--include-comments", "--json"]
    );
}

#[test]
fn beads_show_output_normalizes_enveloped_single_issue_with_comments() {
    let (issue, stdout) = parse_beads_show_output(
        r#"{"data":[{"id":"gxserver-57","comment_count":1,"comments":[{"id":1,"text":"hello"}]}]}"#,
    )
    .unwrap();

    assert_eq!(issue.get("id"), Some(&json!("gxserver-57")));
    assert_eq!(
        issue
            .get("comments")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap(),
        json!({"id":"gxserver-57","comment_count":1,"comments":[{"id":1,"text":"hello"}]})
    );
}

#[test]
fn typed_operation_environment_applies_beads_json_envelope() {
    let env = typed_operation_environment(
        Some("/tmp/bin"),
        &[
            ("BD_JSON_ENVELOPE".to_string(), "1".to_string()),
            ("NO_COLOR".to_string(), "1".to_string()),
        ],
    );
    assert!(env.contains(&("PATH".to_string(), "/tmp/bin".to_string())));
    assert!(env.contains(&("BD_JSON_ENVELOPE".to_string(), "1".to_string())));
    assert!(!env.iter().any(|(key, _)| key == "NO_COLOR"));
}

#[test]
fn beads_list_all_labels_plans_board_list_command() {
    let dir = tempdir().unwrap();
    let ctx = context(dir.path());
    let command =
        build_beads_command_with_executable("listAllLabels", &Map::new(), &ctx, "/tmp/bd").unwrap();
    assert_eq!(command.args, vec!["list", "--all", "--json"]);
}

#[test]
fn beads_label_counts_derive_sorted_counts_from_board_output() {
    let (issues, _) = parse_beads_board_output(
        r#"{"data":[{"id":"gxserver-1","labels":["ui"," mac ","",null,"ui"]},{"id":"gxserver-2","labels":["backend","Backend"]},{"id":"gxserver-3","labels":"ignored"},{"id":"gxserver-4","labels":["z","Z","a","A","aa"]},1]}"#,
    )
    .unwrap();
    let labels = derive_beads_label_counts(&issues);
    assert_eq!(
        serde_json::to_string(&labels).unwrap(),
        r#"[{"count":1,"label":"a"},{"count":1,"label":"A"},{"count":1,"label":"aa"},{"count":1,"label":"backend"},{"count":1,"label":"Backend"},{"count":1,"label":"mac"},{"count":2,"label":"ui"},{"count":1,"label":"z"},{"count":1,"label":"Z"}]"#
    );

    let (empty_issues, _) = parse_beads_board_output("").unwrap();
    assert_eq!(
        serde_json::to_string(&derive_beads_label_counts(&empty_issues)).unwrap(),
        "[]"
    );
    let error = parse_beads_board_output("{").unwrap_err();
    assert_eq!(error.code, "badRequest");
    assert_eq!(error.message, "Beads board output was not valid JSON.");
}

fn run_git<I, S>(args: I, cwd: &Path)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = run_git_raw(args, cwd);
    assert!(
        output.status.success(),
        "git failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_git_output<I, S>(args: I, cwd: &Path) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = run_git_raw(args, cwd);
    assert!(
        output.status.success(),
        "git failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_git_raw<I, S>(args: I, cwd: &Path) -> std::process::Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let output = StdCommand::new("git")
        .args(&args)
        .current_dir(cwd)
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!(
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}
