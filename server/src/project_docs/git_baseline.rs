use serde_json::{json, Value};
use std::{fs, path::Path, process::Stdio};

use super::*;

fn run_git(arguments: &[&str], cwd: &Path) -> Option<(i32, Vec<u8>)> {
    let output = crate::platform::process::background_command("git")
        .args(arguments)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some((output.status.code().unwrap_or(-1), output.stdout))
}

fn unavailable_git_baseline(reason: &str) -> Value {
    json!({
        "available": false,
        "baseText": Value::Null,
        "headOid": Value::Null,
        "maxBytesExceeded": Value::Null,
        "reason": reason,
        "tracked": false,
    })
}

fn renderable_git_baseline(
    base_text: Option<String>,
    head_oid: Option<&str>,
    max_bytes_exceeded: Option<bool>,
    reason: Option<&str>,
    tracked: bool,
) -> Value {
    json!({
        "available": true,
        "baseText": base_text,
        "headOid": head_oid,
        "maxBytesExceeded": max_bytes_exceeded,
        "reason": reason,
        "tracked": tracked,
    })
}

fn git_output(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout).trim().to_string()
}

pub(super) fn git_baseline(root: &Path, file: &Path, _relative_path: &str) -> Value {
    let Some(parent) = file.parent() else {
        return unavailable_git_baseline("not-repo");
    };
    let Some((0, stdout)) = run_git(&["rev-parse", "--show-toplevel"], parent) else {
        return unavailable_git_baseline("not-repo");
    };
    let Ok(repo_root) = fs::canonicalize(git_output(&stdout)) else {
        return unavailable_git_baseline("not-repo");
    };
    if !repo_root.starts_with(root) {
        return unavailable_git_baseline("not-repo");
    }
    let Ok(git_path) = file.strip_prefix(&repo_root) else {
        return unavailable_git_baseline("not-repo");
    };
    let git_path = git_path.to_string_lossy().to_string();
    match run_git(&["check-ignore", "-q", "--", &git_path], &repo_root) {
        Some((0, _)) => return unavailable_git_baseline("ignored"),
        Some((1, _)) => {}
        Some(_) => return unavailable_git_baseline("error"),
        None => return unavailable_git_baseline("git-unavailable"),
    }
    let tracked = run_git(
        &["ls-files", "--error-unmatch", "--", &git_path],
        &repo_root,
    )
    .is_some_and(|(exit, _)| exit == 0);
    let head_oid = run_git(&["rev-parse", "--verify", "HEAD"], &repo_root)
        .filter(|(exit, _)| *exit == 0)
        .map(|(_, stdout)| git_output(&stdout))
        .filter(|value| !value.is_empty());
    if !tracked || head_oid.is_none() {
        return renderable_git_baseline(None, head_oid.as_deref(), None, None, tracked);
    }
    let head_oid = head_oid.unwrap();
    if let Some(cached) = ghostex_docs::baseline::get(&repo_root, &git_path, &head_oid) {
        return cached;
    }
    let head_spec = format!("{head_oid}:{git_path}");
    let Some((0, size)) = run_git(&["cat-file", "-s", &head_spec], &repo_root) else {
        return renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    };
    if git_output(&size)
        .parse::<u64>()
        .is_ok_and(|size| size > GIT_BASELINE_MAX_BYTES as u64)
    {
        return renderable_git_baseline(
            None,
            Some(&head_oid),
            Some(true),
            Some("too-large"),
            tracked,
        );
    }
    let Some((0, baseline)) = run_git(&["cat-file", "-p", &head_spec], &repo_root) else {
        return renderable_git_baseline(None, Some(&head_oid), None, Some("error"), tracked);
    };
    if baseline.len() > GIT_BASELINE_MAX_BYTES {
        return renderable_git_baseline(
            None,
            Some(&head_oid),
            Some(true),
            Some("too-large"),
            tracked,
        );
    }
    if baseline.contains(&0) {
        return renderable_git_baseline(None, Some(&head_oid), None, Some("binary"), tracked);
    }
    let baseline = renderable_git_baseline(
        Some(String::from_utf8_lossy(&baseline).to_string()),
        Some(&head_oid),
        None,
        None,
        tracked,
    );
    ghostex_docs::baseline::insert(&repo_root, &git_path, &head_oid, &baseline);
    baseline
}
