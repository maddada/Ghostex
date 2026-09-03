use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::beads::ensure_beads_git_hooks;
use super::values::{
    display_unknown_value, is_allowed_git_ref, is_path_inside, normalize_absolute_path,
    normalize_git_ref, normalize_optional_git_ref, normalize_path_string, run_process_command,
    typed_result, ProcessCommand, TypedOperationContext, TypedOperationError,
};

pub(crate) async fn run_worktree_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_worktree_action(params.get("action"))?;
    if action == "pathExists" {
        let worktree_path = normalize_worktree_target_path(params.get("worktreePath"), context)?;
        let exists = Path::new(&worktree_path).exists();
        return Ok(json!({
            "action": action,
            "exitCode": if exists { 0 } else { 1 },
            "stderr": "",
            "stdout": if exists { "true" } else { "false" },
        }));
    }
    if action == "ensureBeadsHooks" {
        return ensure_beads_git_hooks(context).await;
    }
    if action == "hasPopulatedSubmodules" {
        /*
        CDXC:Worktrees 2026-08-09-18:40:
        `git worktree move` hard-refuses a worktree with populated submodules,
        and the only honest answer is to say so before the user names a folder.
        This probe stays worktree-scoped (it only ever inspects a path already
        proven to be inside the caller's worktree family) instead of widening
        the git action list with a general `submodule` primitive. It follows
        `pathExists`'s convention: exit code 0 means "yes".
        */
        let worktree_path = normalize_existing_worktree_path(params.get("worktreePath"), context)?;
        let populated = crate::worktree_sessions::worktree_has_populated_submodules(&worktree_path);
        return Ok(json!({
            "action": action,
            "exitCode": if populated { 0 } else { 1 },
            "stderr": "",
            "stdout": if populated { "true" } else { "false" },
        }));
    }
    if action == "renameBranch" {
        /*
        CDXC:Worktrees 2026-08-09-18:40:
        `refs/heads/<x>` cannot exist while `refs/heads/<x>/…` does, in either
        direction, and `git branch -m` reports that as a bare
        `fatal: branch rename failed`. Probe both directions here, before the
        command is built, so the caller gets a sentence naming the ref that is
        in the way instead of a raw git failure.
        */
        let new_branch = normalize_git_ref(params.get("newBranch"), "newBranch")?;
        if let Some(blocker) =
            crate::worktree_sessions::worktree_branch_namespace_blocker(&context.cwd, &new_branch)
        {
            return Err(TypedOperationError::bad_request(format!(
                "Git cannot use that branch name: \"{blocker}\" already exists."
            )));
        }
    }
    let command = build_worktree_command(&action, params, context)?;
    let output = run_process_command(&command, context).await?;
    let mut result = typed_result(&action, &command, output);
    if action == "list" && result.get("exitCode").and_then(Value::as_i64) == Some(0) {
        let stdout = result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        result.as_object_mut().expect("result object").insert(
            "worktrees".to_string(),
            Value::Array(parse_git_worktree_list_porcelain(&stdout)),
        );
    }
    Ok(result)
}

pub(crate) fn build_worktree_command(
    action: &str,
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<ProcessCommand, TypedOperationError> {
    let cwd = context.cwd.as_str();
    Ok(match action {
        "create" => {
            let worktree_path =
                normalize_worktree_target_path(params.get("worktreePath"), context)?;
            let branch = normalize_optional_git_ref(params.get("branch"), "branch")?;
            let base_ref = normalize_optional_git_ref(params.get("baseRef"), "baseRef")?;
            let mut args = vec!["worktree".to_string(), "add".to_string(), worktree_path];
            if let Some(branch) = branch {
                args.extend(["-b".to_string(), branch]);
            }
            if let Some(base_ref) = base_ref {
                args.push(base_ref);
            }
            ProcessCommand::new("git", args, cwd)
        }
        "list" => ProcessCommand::new("git", vec!["worktree", "list", "--porcelain"], cwd),
        "move" => {
            let worktree_path =
                normalize_existing_worktree_path(params.get("worktreePath"), context)?;
            let destination_path =
                normalize_worktree_destination_path(params.get("destinationPath"), context)?;
            ProcessCommand::new(
                "git",
                vec![
                    "worktree".to_string(),
                    "move".to_string(),
                    "--".to_string(),
                    worktree_path,
                    destination_path,
                ],
                cwd,
            )
        }
        "prune" => ProcessCommand::new("git", vec!["worktree", "prune"], cwd),
        "remove" => {
            let worktree_path =
                normalize_existing_worktree_path(params.get("worktreePath"), context)?;
            let mut args = vec!["worktree".to_string(), "remove".to_string()];
            if params.get("force").and_then(Value::as_bool) == Some(true) {
                args.push("--force".to_string());
            }
            args.extend(["--".to_string(), worktree_path]);
            ProcessCommand::new("git", args, cwd)
        }
        "renameBranch" => {
            /*
            CDXC:Worktrees 2026-08-09-18:40:
            `-m`, never `-M`. Force does not help with the ref-namespace
            collision this operation actually hits, and it would let a rename
            silently clobber an existing branch — losing commits, not saving a
            click.
            */
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            let new_branch = normalize_git_ref(params.get("newBranch"), "newBranch")?;
            ProcessCommand::new(
                "git",
                vec![
                    "branch".to_string(),
                    "-m".to_string(),
                    "--".to_string(),
                    branch,
                    new_branch,
                ],
                cwd,
            )
        }
        "switch" => ProcessCommand::new(
            "git",
            vec![
                "switch".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        _ => {
            return Err(TypedOperationError::bad_request(format!(
                "Unsupported worktree action: {action}"
            )))
        }
    })
}

pub(crate) fn normalize_worktree_action(
    input: Option<&Value>,
) -> Result<String, TypedOperationError> {
    let action = input.and_then(Value::as_str).unwrap_or("undefined");
    match action {
        "create"
        | "ensureBeadsHooks"
        | "hasPopulatedSubmodules"
        | "list"
        | "move"
        | "pathExists"
        | "prune"
        | "remove"
        | "renameBranch"
        | "switch" => Ok(action.to_string()),
        _ => Err(TypedOperationError::bad_request(format!(
            "Unsupported worktree action: {}",
            display_unknown_value(input)
        ))),
    }
}

pub(crate) fn normalize_worktree_target_path(
    input: Option<&Value>,
    context: &TypedOperationContext,
) -> Result<String, TypedOperationError> {
    normalize_worktree_family_path(input, "worktreePath", context)
}

fn normalize_worktree_family_path(
    input: Option<&Value>,
    field: &str,
    context: &TypedOperationContext,
) -> Result<String, TypedOperationError> {
    let worktree_path = normalize_absolute_path(input, field)?;
    let family_root = Path::new(&context.cwd)
        .parent()
        .unwrap_or_else(|| Path::new(&context.cwd))
        .to_string_lossy()
        .to_string();
    if !is_path_inside(&family_root, &worktree_path) {
        return Err(TypedOperationError::forbidden(format!(
            "{field} must stay inside the source project worktree family directory."
        )));
    }
    if normalize_path_string(PathBuf::from(&worktree_path))
        == normalize_path_string(PathBuf::from(&context.cwd))
    {
        return Err(TypedOperationError::forbidden(format!(
            "{field} cannot be the source project directory."
        )));
    }
    Ok(worktree_path)
}

fn normalize_worktree_destination_path(
    input: Option<&Value>,
    context: &TypedOperationContext,
) -> Result<String, TypedOperationError> {
    let destination_path = normalize_worktree_family_path(input, "destinationPath", context)?;
    /*
    CDXC:Worktrees 2026-08-09-18:40:
    `git worktree move A B` with B already present is NOT an error: git moves the
    worktree to B/A and exits 0, so the checkout silently lands one level deeper
    than anyone asked for and the registered project path becomes wrong — a
    "successful" rename that leaves the sidebar pointing at a directory that is
    not a worktree. Refusing an existing destination outright is the only way to
    keep "exit 0" meaning "the worktree is exactly where we said".
    */
    if Path::new(&destination_path).exists() {
        return Err(TypedOperationError::bad_request(
            "destinationPath already exists.",
        ));
    }
    Ok(destination_path)
}

pub(crate) fn normalize_existing_worktree_path(
    input: Option<&Value>,
    context: &TypedOperationContext,
) -> Result<String, TypedOperationError> {
    let worktree_path = normalize_worktree_target_path(input, context)?;
    let metadata = fs::metadata(&worktree_path).map_err(|_| {
        TypedOperationError::not_found(format!("worktreePath does not exist: {worktree_path}"))
    })?;
    if metadata.is_dir() {
        Ok(worktree_path)
    } else {
        Err(TypedOperationError::bad_request(
            "worktreePath is not a directory.",
        ))
    }
}

fn parse_git_worktree_list_porcelain(stdout: &str) -> Vec<Value> {
    let mut entries = Vec::new();
    let mut current: Option<Map<String, Value>> = None;
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                if entry
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| !path.is_empty())
                {
                    entries.push(Value::Object(entry));
                }
            }
            let mut entry = Map::new();
            entry.insert("bare".to_string(), json!(false));
            entry.insert("detached".to_string(), json!(false));
            entry.insert("path".to_string(), json!(path.trim()));
            current = Some(entry);
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if line == "bare" {
            entry.insert("bare".to_string(), json!(true));
        } else if line == "detached" {
            entry.insert("detached".to_string(), json!(true));
        } else if let Some(branch) = line.strip_prefix("branch ") {
            entry.insert(
                "branch".to_string(),
                json!(normalize_worktree_branch(Some(branch.trim()))),
            );
        }
    }
    if let Some(entry) = current {
        if entry
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| !path.is_empty())
        {
            entries.push(Value::Object(entry));
        }
    }
    for entry in &mut entries {
        if entry.get("branch").is_none() {
            if let Some(object) = entry.as_object_mut() {
                object.insert("branch".to_string(), json!("detached"));
            }
        }
    }
    entries
}

pub(crate) fn parse_git_branch_list(stdout: &str) -> Vec<Value> {
    let mut branches = Vec::new();
    let mut seen: HashMap<String, bool> = HashMap::new();
    for line in stdout.lines() {
        let mut columns = line.split('\t');
        let name = columns.next().unwrap_or_default().trim();
        let ref_name = columns.next().unwrap_or_default().trim();
        let head_marker = columns.next().unwrap_or_default().trim();
        if name.is_empty()
            || ref_name.is_empty()
            || name.ends_with("/HEAD")
            || ref_name.ends_with("/HEAD")
            || !is_allowed_git_ref(name)
            || seen.contains_key(name)
        {
            continue;
        }
        seen.insert(name.to_string(), true);
        branches.push(json!({
            "current": head_marker == "*",
            "name": name,
            "remote": ref_name.starts_with("refs/remotes/"),
        }));
    }
    branches
}

fn normalize_worktree_branch(branch: Option<&str>) -> String {
    branch
        .map(|branch| {
            branch
                .trim()
                .strip_prefix("refs/heads/")
                .unwrap_or(branch.trim())
        })
        .filter(|branch| !branch.is_empty())
        .unwrap_or("detached")
        .to_string()
}
