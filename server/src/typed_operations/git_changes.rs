//! The Changes view's two Git actions: `changesDiff` reads every changed file of a project as one
//! patch for a scope, and `changesRevert` undoes one file or one hunk of it in the working tree.
//!
//! CDXC:Changes 2026-10-25 WHY:
//! Both actions compute the diff themselves from the scope's base so a revert only ever undoes a
//! hunk gxserver itself would show: the client names the file and the hunk text it saw, and a hunk
//! that no longer matches is refused instead of guessed at. "Against main" diffs the working tree
//! against `merge-base(<default branch>, HEAD)` (on the default branch itself, against HEAD), the
//! same base Session details' Changes count uses (`probe_git_status_with`); "Uncommitted" diffs it
//! against HEAD. Untracked files are part of both, as new files, because they are what an agent
//! most often adds. Renames are not detected (`-M`): a renamed file is a deletion and an addition,
//! so every hunk the view shows can be reverted on its own path.
use serde_json::{json, Map, Value};

use super::values::{
    normalize_relative_file_path, run_process_command, CommandOutput, ProcessCommand,
    TypedOperationContext, TypedOperationError,
};
use crate::session_git_status::{resolve_default_branch, run_git_probe_command};

/// Git's empty tree: the base of a repository with no commit yet.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// Untracked files past this many are left out of the patch (build output nobody ignored).
const MAX_UNTRACKED_FILES: usize = 300;
/// An untracked file larger than this is listed without its contents.
const MAX_UNTRACKED_FILE_BYTES: u64 = 1024 * 1024;
/// The whole patch the view receives.
const MAX_PATCH_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChangesScope {
    AgainstMain,
    Uncommitted,
}

impl ChangesScope {
    fn read(params: &Map<String, Value>) -> Self {
        match params.get("scope").and_then(Value::as_str) {
            Some("uncommitted") => Self::Uncommitted,
            _ => Self::AgainstMain,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::AgainstMain => "main",
            Self::Uncommitted => "uncommitted",
        }
    }
}

/// What a scope compares the working tree against.
struct ChangesBase {
    /// A commit (or the empty tree) to pass to `git diff`.
    rev: String,
    /// What the toolbar names it: the default branch, or `HEAD`.
    label: String,
    branch: Option<String>,
    default_branch: Option<String>,
}

async fn git(
    context: &TypedOperationContext,
    args: Vec<String>,
    preserve_stdout: bool,
) -> Result<CommandOutput, TypedOperationError> {
    let mut command = ProcessCommand::new("git", args, context.cwd.as_str())
        .with_env("GIT_OPTIONAL_LOCKS", "0")
        .with_env("GIT_TERMINAL_PROMPT", "0")
        .with_env("GIT_LITERAL_PATHSPECS", "1");
    if preserve_stdout {
        command = command.with_preserved_stdout_whitespace();
    }
    run_process_command(&command, context).await
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| arg.to_string()).collect()
}

async fn git_text(context: &TypedOperationContext, list: &[&str]) -> Option<String> {
    let output = git(context, args(list), false).await.ok()?;
    (output.exit_code == 0)
        .then(|| output.stdout.trim().to_string())
        .filter(|text| !text.is_empty())
}

async fn resolve_base(
    context: &TypedOperationContext,
    scope: ChangesScope,
) -> Result<Option<ChangesBase>, TypedOperationError> {
    if git_text(context, &["rev-parse", "--is-inside-work-tree"])
        .await
        .as_deref()
        != Some("true")
    {
        return Ok(None);
    }
    let head = git_text(context, &["rev-parse", "--verify", "--quiet", "HEAD"]).await;
    let branch = git_text(context, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await;
    let cwd = context.cwd.clone();
    let default_branch = tokio::task::spawn_blocking(move || {
        resolve_default_branch(&move |args: &[&str]| run_git_probe_command(&cwd, args))
    })
    .await
    .ok()
    .flatten();
    let Some(head) = head else {
        return Ok(Some(ChangesBase {
            rev: EMPTY_TREE.to_string(),
            label: "HEAD".to_string(),
            branch,
            default_branch: default_branch.map(|branch| branch.name),
        }));
    };
    let merge_base = match (&default_branch, scope) {
        (Some(default), ChangesScope::AgainstMain)
            if branch.as_deref() != Some(default.name.as_str()) =>
        {
            git_text(context, &["merge-base", default.git_ref.as_str(), "HEAD"]).await
        }
        _ => None,
    };
    let (rev, label) = match (merge_base, &default_branch) {
        (Some(base), Some(default)) => (base, default.name.clone()),
        _ => (head, "HEAD".to_string()),
    };
    Ok(Some(ChangesBase {
        rev,
        label,
        branch,
        default_branch: default_branch.map(|branch| branch.name),
    }))
}

fn diff_args(base: &str, ignore_whitespace: bool, binary: bool, path: Option<&str>) -> Vec<String> {
    let mut list = args(&[
        "-c",
        "core.quotePath=false",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-renames",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ]);
    if ignore_whitespace {
        list.push("-w".to_string());
    }
    if binary {
        list.push("--binary".to_string());
    }
    list.push(base.to_string());
    list.push("--".to_string());
    if let Some(path) = path {
        list.push(path.to_string());
    }
    list
}

fn untracked_diff_args(path: &str) -> Vec<String> {
    args(&[
        "-c",
        "core.quotePath=false",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-index",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--",
        "/dev/null",
        path,
    ])
}

/// `git status --porcelain -z`: every path with uncommitted work, the new name of a rename.
fn parse_status_paths(stdout: &str) -> (Vec<String>, Vec<String>) {
    let mut changed = Vec::new();
    let mut untracked = Vec::new();
    let mut entries = stdout.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let code = &entry[..2];
        let path = entry[3..].to_string();
        if code.starts_with('R') || code.starts_with('C') {
            // The original path follows as its own entry.
            entries.next();
        }
        if code == "??" {
            untracked.push(path.clone());
        }
        changed.push(path);
    }
    (changed, untracked)
}

pub(crate) async fn changes_diff(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let scope = ChangesScope::read(params);
    let ignore_whitespace = params.get("ignoreWhitespace").and_then(Value::as_bool) == Some(true);
    let Some(base) = resolve_base(context, scope).await? else {
        return Ok(json!({ "action": "changesDiff", "exitCode": 0, "isRepo": false }));
    };
    let diff = git(
        context,
        diff_args(&base.rev, ignore_whitespace, false, None),
        true,
    )
    .await?;
    let mut truncated = diff.error.is_some();
    let mut patch = diff.stdout;
    let status = git(
        context,
        args(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
        true,
    )
    .await?;
    let (uncommitted, untracked) = parse_status_paths(&status.stdout);
    let mut too_large = Vec::new();
    for path in untracked.iter().take(MAX_UNTRACKED_FILES) {
        if patch.len() >= MAX_PATCH_BYTES {
            truncated = true;
            break;
        }
        let size = std::fs::metadata(std::path::Path::new(&context.cwd).join(path))
            .map(|meta| meta.len())
            .unwrap_or(0);
        if size > MAX_UNTRACKED_FILE_BYTES {
            too_large.push(path.clone());
            continue;
        }
        let output = git(context, untracked_diff_args(path), true).await?;
        // `--no-index` exits 1 when the files differ, which is always here.
        if output.stdout.trim().is_empty() {
            continue;
        }
        if !patch.is_empty() && !patch.ends_with('\n') {
            patch.push('\n');
        }
        patch.push_str(&output.stdout);
    }
    if untracked.len() > MAX_UNTRACKED_FILES {
        truncated = true;
    }
    if patch.len() > MAX_PATCH_BYTES {
        truncated = true;
        let mut end = MAX_PATCH_BYTES;
        while !patch.is_char_boundary(end) {
            end -= 1;
        }
        // Keep whole files only.
        let cut = patch[..end]
            .rfind("\ndiff --git ")
            .map_or(end, |index| index + 1);
        patch.truncate(cut);
    }
    Ok(json!({
        "action": "changesDiff",
        "exitCode": 0,
        "isRepo": true,
        "scope": scope.name(),
        "base": base.rev,
        "baseLabel": base.label,
        "branch": base.branch,
        "defaultBranch": base.default_branch,
        "onDefaultBranch": base.branch.is_some() && base.branch == base.default_branch,
        "patch": patch,
        "truncated": truncated,
        "uncommittedPaths": uncommitted,
        "untrackedPaths": untracked,
        "tooLargePaths": too_large,
    }))
}

/// A file patch's header (everything before its first hunk) and its hunks, each with its header
/// line and the lines under it.
fn split_hunks(patch: &str) -> (String, Vec<String>) {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@") {
            hunks.push(line.to_string());
        } else if let Some(hunk) = hunks.last_mut() {
            hunk.push_str(line);
        } else {
            header.push_str(line);
        }
    }
    (header, hunks)
}

fn refuse(message: &str) -> TypedOperationError {
    TypedOperationError::bad_request(message.to_string())
}

pub(crate) async fn changes_revert(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let scope = ChangesScope::read(params);
    let ignore_whitespace = params.get("ignoreWhitespace").and_then(Value::as_bool) == Some(true);
    let path = normalize_relative_file_path(params.get("filePath"))?;
    let hunk = params
        .get("hunk")
        .and_then(Value::as_str)
        .filter(|hunk| !hunk.trim().is_empty());
    let Some(base) = resolve_base(context, scope).await? else {
        return Err(refuse("This folder is not a Git repository."));
    };
    let tracked = git(
        context,
        args(&["ls-files", "--error-unmatch", "--", &path]),
        false,
    )
    .await?
    .exit_code
        == 0;
    let in_base = git(
        context,
        args(&["cat-file", "-e", &format!("{}:{}", base.rev, path)]),
        false,
    )
    .await?
    .exit_code
        == 0;
    if !tracked && !in_base {
        // An untracked file is one new-file hunk: reverting it, or that hunk, deletes it.
        if let Some(hunk) = hunk {
            let output = git(context, untracked_diff_args(&path), true).await?;
            let (_, hunks) = split_hunks(&output.stdout);
            if !hunks
                .iter()
                .any(|candidate| candidate.trim_end() == hunk.trim_end())
            {
                return Err(refuse(
                    "This change is no longer in the file. Refresh and try again.",
                ));
            }
        }
        let full = std::path::Path::new(&context.cwd).join(&path);
        std::fs::remove_file(&full).map_err(|error| {
            TypedOperationError::dependency_unavailable(format!("Could not delete {path}: {error}"))
        })?;
        return Ok(json!({ "action": "changesRevert", "exitCode": 0, "deleted": true }));
    }
    let file_patch = git(
        context,
        diff_args(&base.rev, ignore_whitespace, hunk.is_none(), Some(&path)),
        true,
    )
    .await?
    .stdout;
    if file_patch.trim().is_empty() {
        return Err(refuse("This file has no changes left to revert."));
    }
    let patch = match hunk {
        None => file_patch,
        Some(hunk) => {
            let (header, hunks) = split_hunks(&file_patch);
            let Some(found) = hunks
                .into_iter()
                .find(|candidate| candidate.trim_end() == hunk.trim_end())
            else {
                return Err(refuse(
                    "This change is no longer in the file. Refresh and try again.",
                ));
            };
            let mut patch = header;
            patch.push_str(&found);
            if !patch.ends_with('\n') {
                patch.push('\n');
            }
            patch
        }
    };
    let mut apply = args(&["apply", "-R", "--whitespace=nowarn"]);
    if ignore_whitespace {
        apply.push("--ignore-whitespace".to_string());
    }
    let command = ProcessCommand::new("git", apply, context.cwd.as_str())
        .with_env("GIT_TERMINAL_PROMPT", "0")
        .with_stdin(patch);
    let output = run_process_command(&command, context).await?;
    if output.exit_code != 0 {
        return Err(TypedOperationError::bad_request(format!(
            "Git could not revert this change: {}",
            output.stderr.lines().next().unwrap_or("the file changed")
        )));
    }
    // A staged copy of the change would still be committed; put the index back to HEAD for this
    // file (the working tree keeps everything else).
    if tracked {
        let staged = git(
            context,
            args(&["diff", "--cached", "--quiet", "--", &path]),
            false,
        )
        .await?
        .exit_code
            != 0;
        if staged {
            git(context, args(&["reset", "-q", "--", &path]), false).await?;
        }
    }
    Ok(json!({ "action": "changesRevert", "exitCode": 0, "deleted": false }))
}
