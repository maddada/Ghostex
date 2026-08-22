use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::domain::{normalize_path_for_comparison, now_iso, path_file_name};

#[derive(Debug, Clone)]
struct GitWorktreeEntry {
    bare: bool,
    branch: Option<String>,
    path: String,
}

pub(crate) fn find_project_by_path_in(projects: &[Value], normalized_path: &str) -> Option<Value> {
    projects
        .iter()
        .find(|project| project.get("path").and_then(Value::as_str) == Some(normalized_path))
        .cloned()
}

#[derive(Clone)]
struct GitWorktreeTopologyProbe {
    entries: Vec<GitWorktreeEntry>,
    worktree_root: String,
}

const GIT_WORKTREE_TOPOLOGY_PROBE_TTL: Duration = Duration::from_secs(60);

#[allow(clippy::type_complexity)]
fn git_worktree_topology_probe_cache(
) -> &'static Mutex<HashMap<String, (Instant, Option<GitWorktreeTopologyProbe>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Option<GitWorktreeTopologyProbe>)>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn probe_git_worktree_topology(project_path: &str) -> Option<GitWorktreeTopologyProbe> {
    /*
    Every addProjectPath re-detects worktree metadata, including registration
    repairs for paths that are already known, so clients that re-register
    projects turn each call into git subprocess spawns. Git topology only
    changes on init/worktree edits; cache probes (including non-repo results)
    per path briefly instead of spawning git each time.
    */
    if let Ok(cache) = git_worktree_topology_probe_cache().lock() {
        if let Some((probed_at, probe)) = cache.get(project_path) {
            if probed_at.elapsed() < GIT_WORKTREE_TOPOLOGY_PROBE_TTL {
                return probe.clone();
            }
        }
    }
    let probe = run_git_worktree_topology_probe(project_path);
    if let Ok(mut cache) = git_worktree_topology_probe_cache().lock() {
        cache.insert(project_path.to_string(), (Instant::now(), probe.clone()));
    }
    probe
}

fn run_git_worktree_topology_probe(project_path: &str) -> Option<GitWorktreeTopologyProbe> {
    if run_git(project_path, &["rev-parse", "--is-inside-work-tree"]) != "true" {
        return None;
    }
    let worktree_root =
        normalize_path_for_comparison(&run_git(project_path, &["rev-parse", "--show-toplevel"]));
    if worktree_root.is_empty() {
        return None;
    }
    let entries = parse_git_worktree_list_porcelain(&run_git(
        project_path,
        &["worktree", "list", "--porcelain"],
    ));
    Some(GitWorktreeTopologyProbe {
        entries,
        worktree_root,
    })
}

pub(crate) fn detect_registered_git_worktree_metadata(
    projects: &[Value],
    project_path: &str,
    project_name: &str,
) -> Option<Map<String, Value>> {
    let probe = probe_git_worktree_topology(project_path)?;
    let worktree_root = probe.worktree_root;
    let entries = probe.entries;
    let current_entry = entries
        .iter()
        .find(|entry| normalize_path_for_comparison(&entry.path) == worktree_root)?;
    let main_entry = entries.iter().find(|entry| !entry.bare)?;
    let main_path = normalize_path_for_comparison(&main_entry.path);
    if main_path.is_empty() || worktree_root == main_path {
        return None;
    }

    let parent_project = projects.iter().find(|project| {
        let Some(project_path) = project.get("path").and_then(Value::as_str) else {
            return false;
        };
        if project.get("worktree").is_some() {
            return false;
        }
        normalize_path_for_comparison(project_path) == main_path
    })?;
    let parent_project_id = parent_project.get("projectId").and_then(Value::as_str)?;
    let parent_project_name = parent_project.get("name").and_then(Value::as_str)?;
    let parent_project_path = parent_project.get("path").and_then(Value::as_str)?;
    let worktree_name = path_file_name(&worktree_root).unwrap_or_else(|| project_name.to_string());

    let mut metadata = Map::new();
    metadata.insert(
        "branch".to_string(),
        Value::String(normalize_git_worktree_branch(
            current_entry.branch.as_deref(),
        )),
    );
    metadata.insert("createdAt".to_string(), Value::String(now_iso()));
    metadata.insert("name".to_string(), Value::String(worktree_name));
    metadata.insert(
        "parentProjectId".to_string(),
        Value::String(parent_project_id.to_string()),
    );
    metadata.insert(
        "parentProjectName".to_string(),
        Value::String(parent_project_name.to_string()),
    );
    metadata.insert(
        "parentProjectPath".to_string(),
        Value::String(parent_project_path.to_string()),
    );
    Some(metadata)
}

pub(crate) fn are_project_worktree_metadata_equal(
    current: Option<&Value>,
    expected: &Map<String, Value>,
) -> bool {
    let Some(current) = current.and_then(Value::as_object) else {
        return false;
    };
    [
        "branch",
        "name",
        "parentProjectId",
        "parentProjectName",
        "parentProjectPath",
    ]
    .into_iter()
    .all(|key| current.get(key) == expected.get(key))
}

fn parse_git_worktree_list_porcelain(stdout: &str) -> Vec<GitWorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_entry: Option<GitWorktreeEntry> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current_entry.take() {
                if !entry.path.is_empty() {
                    entries.push(entry);
                }
            }
            current_entry = Some(GitWorktreeEntry {
                bare: false,
                branch: None,
                path: path.trim().to_string(),
            });
            continue;
        }

        let Some(entry) = current_entry.as_mut() else {
            continue;
        };
        if line == "bare" {
            entry.bare = true;
        } else if let Some(branch) = line.strip_prefix("branch ") {
            entry.branch = Some(branch.trim().to_string());
        }
    }

    if let Some(entry) = current_entry {
        if !entry.path.is_empty() {
            entries.push(entry);
        }
    }
    entries
}

fn normalize_git_worktree_branch(branch: Option<&str>) -> String {
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

fn run_git(cwd: &str, args: &[&str]) -> String {
    let Ok(output) = Command::new("git").args(args).current_dir(cwd).output() else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
