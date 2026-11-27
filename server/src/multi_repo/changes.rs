//! Changes across several repos: each repo's diff is read on its own (`changesDiff` with
//! `singleRepo`), then its paths get the repo's place in front (`api/src/x.ts`), so the answer is
//! one patch an older phone or desktop can already draw, plus `repos` naming the groups for a
//! client that draws a header per repo. A revert or a commit names those same paths, and the repo
//! is read back off the front.

use serde_json::{json, Map, Value};

use super::RepoSet;
use crate::typed_operations::{TypedOperationContext, TypedOperationError};

/// The whole patch an answer carries (the single-repo answer's own limit).
const MAX_PATCH_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn repo_context(context: &TypedOperationContext, cwd: &str) -> TypedOperationContext {
    TypedOperationContext {
        beads_cwd: None,
        cwd: cwd.to_string(),
        env_path: context.env_path.clone(),
        projects: context.projects.clone(),
    }
}

fn with_prefix(rel: &str, path: &str) -> String {
    if rel.is_empty() {
        path.to_string()
    } else {
        format!("{rel}/{path}")
    }
}

/// `patch` with `rel/` put in front of every path in its file headers (never in hunk lines, which
/// can start with `--- a/` too).
pub(crate) fn prefix_patch(patch: &str, rel: &str) -> String {
    if rel.is_empty() {
        return patch.to_string();
    }
    let mut out = String::with_capacity(patch.len() + 64);
    let mut in_header = false;
    for line in patch.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            in_header = true;
            let rest = rest.replacen("a/", &format!("a/{rel}/"), 1).replacen(
                " b/",
                &format!(" b/{rel}/"),
                1,
            );
            out.push_str("diff --git ");
            out.push_str(&rest);
            continue;
        }
        if line.starts_with("@@") {
            in_header = false;
        }
        if in_header {
            if let Some(rest) = line.strip_prefix("--- a/") {
                out.push_str(&format!("--- a/{rel}/{rest}"));
                continue;
            }
            if let Some(rest) = line.strip_prefix("+++ b/") {
                out.push_str(&format!("+++ b/{rel}/{rest}"));
                continue;
            }
            if line.starts_with("Binary files ") {
                out.push_str(&line.replacen(" a/", &format!(" a/{rel}/"), 1).replacen(
                    " b/",
                    &format!(" b/{rel}/"),
                    1,
                ));
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

fn prefixed_list(value: Option<&Value>, rel: &str) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(|path| Value::String(with_prefix(rel, path)))
                .collect()
        })
        .unwrap_or_default()
}

/// `changesDiff` for a folder with several repos.
pub(crate) async fn multi_repo_diff(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
    set: &RepoSet,
) -> Result<Value, TypedOperationError> {
    let mut single = params.clone();
    single.insert("singleRepo".to_string(), json!(true));
    let mut patch = String::new();
    let mut truncated = false;
    let (mut uncommitted, mut untracked, mut tests, mut too_large) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut repos = Vec::new();
    let mut first: Option<Value> = None;
    for repo in &set.repos {
        let repo_context = repo_context(context, &repo.path);
        let answer = Box::pin(crate::typed_operations::git_changes::changes_diff(
            &single,
            &repo_context,
        ))
        .await?;
        if answer.get("isRepo").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let repo_patch = prefix_patch(
            answer.get("patch").and_then(Value::as_str).unwrap_or(""),
            &repo.rel,
        );
        let files = repo_patch.matches("diff --git ").count();
        // What Merge would bring into this repo's default branch: the same count Merge reads
        // (worktree_ops/multi_repo.rs `merge_session_folder`), so the client hides Merge when
        // every repo is at 0.
        let repo_path = std::path::PathBuf::from(&repo.path);
        let to_merge = tokio::task::spawn_blocking(move || {
            let default = super::git::default_branch(&repo_path)?;
            super::git::read(
                &repo_path,
                &["rev-list", "--count", &format!("{default}..HEAD")],
            )
            .and_then(|count| count.parse::<u64>().ok())
        })
        .await
        .ok()
        .flatten();
        if !patch.is_empty() && !patch.ends_with('\n') {
            patch.push('\n');
        }
        if patch.len() + repo_patch.len() > MAX_PATCH_BYTES {
            truncated = true;
        } else {
            patch.push_str(&repo_patch);
        }
        truncated |= answer.get("truncated").and_then(Value::as_bool) == Some(true);
        uncommitted.extend(prefixed_list(answer.get("uncommittedPaths"), &repo.rel));
        untracked.extend(prefixed_list(answer.get("untrackedPaths"), &repo.rel));
        tests.extend(prefixed_list(answer.get("testPaths"), &repo.rel));
        too_large.extend(prefixed_list(answer.get("tooLargePaths"), &repo.rel));
        repos.push(json!({
            "name": repo.name,
            "rel": repo.rel,
            "kind": repo.kind.as_str(),
            "branch": answer.get("branch"),
            "baseLabel": answer.get("baseLabel"),
            "defaultBranch": answer.get("defaultBranch"),
            "onDefaultBranch": answer.get("onDefaultBranch"),
            "files": files,
            "toMerge": to_merge,
        }));
        if first.is_none() {
            first = Some(answer);
        }
    }
    let Some(first) = first else {
        return Ok(json!({ "action": "changesDiff", "exitCode": 0, "isRepo": false }));
    };
    let field = |key: &str| first.get(key).cloned().unwrap_or(Value::Null);
    Ok(json!({
        "action": "changesDiff",
        "exitCode": 0,
        "isRepo": true,
        "multiRepo": true,
        "repos": repos,
        "scope": field("scope"),
        "base": field("base"),
        "baseLabel": field("baseLabel"),
        "branch": field("branch"),
        "defaultBranch": field("defaultBranch"),
        // A session folder is never on its repos' default branch; a project folder may be.
        "onDefaultBranch": field("onDefaultBranch"),
        "patch": patch,
        "truncated": truncated,
        "uncommittedPaths": uncommitted,
        "untrackedPaths": untracked,
        "testPaths": tests,
        "tooLargePaths": too_large,
    }))
}

/// `changesRevert` for a folder with several repos: the repo is read off the front of the path.
pub(crate) async fn multi_repo_revert(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
    set: &RepoSet,
) -> Result<Value, TypedOperationError> {
    let path = params
        .get("filePath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .replace('\\', "/");
    // A submodule's own path is its parent's pointer, so `group_paths` places it there.
    let groups = group_paths(set, std::slice::from_ref(&path));
    let Some((repo, inside)) = groups.into_iter().next().and_then(|(repo, paths)| {
        paths
            .into_iter()
            .next()
            .map(|inside| (repo.clone(), inside))
    }) else {
        return Err(TypedOperationError::bad_request(
            "This file is in none of the project's repos.".to_string(),
        ));
    };
    let mut single = params.clone();
    single.insert("singleRepo".to_string(), json!(true));
    single.insert("filePath".to_string(), json!(inside));
    let repo_context = repo_context(context, &repo.path);
    Box::pin(crate::typed_operations::git_changes::changes_revert(
        &single,
        &repo_context,
    ))
    .await
}

/// Groups project-relative paths (`api/src/x.ts`) by the repo they are in, each with its path
/// inside that repo. Paths in no repo are dropped.
pub(crate) fn group_paths<'a>(
    set: &'a RepoSet,
    paths: &[String],
) -> Vec<(&'a super::RepoEntry, Vec<String>)> {
    let mut groups: Vec<(&super::RepoEntry, Vec<String>)> = Vec::new();
    for path in paths {
        let path = path.replace('\\', "/");
        let Some((repo, inside)) = set.repo_for_rel_path(&path) else {
            continue;
        };
        if inside.is_empty() {
            // A submodule's own path in its parent: the parent's new pointer.
            if let Some(parent) = set
                .repos
                .iter()
                .filter(|candidate| {
                    candidate.rel.is_empty() || path.starts_with(&format!("{}/", candidate.rel))
                })
                .filter(|candidate| candidate.rel != repo.rel)
                .max_by_key(|candidate| candidate.rel.len())
            {
                let inside_parent = if parent.rel.is_empty() {
                    path.clone()
                } else {
                    path[parent.rel.len() + 1..].to_string()
                };
                push_group(&mut groups, parent, inside_parent);
            }
            continue;
        }
        push_group(&mut groups, repo, inside.to_string());
    }
    groups
}

fn push_group<'a>(
    groups: &mut Vec<(&'a super::RepoEntry, Vec<String>)>,
    repo: &'a super::RepoEntry,
    path: String,
) {
    match groups
        .iter_mut()
        .find(|(existing, _)| existing.rel == repo.rel)
    {
        Some((_, paths)) => paths.push(path),
        None => groups.push((repo, vec![path])),
    }
}

/// `repoStates`: the state of each repo of the session folder `cwd` is in, for Delete Worktree's
/// row per repo: `{multiRepo, branch, repos: [{name, rel, kind, exists, uncommitted, status,
/// remoteBranchExists}]}`. `multiRepo` is false (and nothing else is sent) for a plain worktree.
pub(crate) fn repo_states(cwd: &str) -> Value {
    let Some((root, record)) = super::record::find(std::path::Path::new(cwd)) else {
        return json!({ "action": "repoStates", "exitCode": 0, "multiRepo": false });
    };
    let states = super::session_folder::repo_states(&root, &record);
    let repos: Vec<Value> = record
        .repos
        .iter()
        .zip(states)
        .map(|(repo, state)| {
            let status = if state.exists {
                super::git::read(
                    &state.path,
                    &["status", "--short", "--ignore-submodules=dirty"],
                )
                .unwrap_or_default()
            } else {
                String::new()
            };
            let remote_branch_exists = state.exists
                && super::git::run(
                    &state.path,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("refs/remotes/origin/{}", record.branch),
                    ],
                    super::git::GIT_TIMEOUT,
                )
                .ok;
            json!({
                "name": repo.name,
                "rel": repo.rel,
                "kind": repo.kind,
                "exists": state.exists,
                "uncommitted": state.uncommitted,
                "status": status,
                "remoteBranchExists": remote_branch_exists,
            })
        })
        .collect();
    json!({
        "action": "repoStates",
        "exitCode": 0,
        "multiRepo": true,
        "branch": record.branch,
        "repos": repos,
    })
}
