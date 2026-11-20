//! Step 1: the plan, and every refusal, before anything changes. Nothing here writes.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::fs_ops;
use super::journal::{Journal, JournalWorktree, Phase};
use super::path_match::{split_parts, MovedRoot};
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;

/// What the checks found besides the plan: warnings for the result and the dry run.
pub(crate) struct Checked {
    pub(crate) journal: Journal,
    pub(crate) warnings: Vec<String>,
}

fn refuse(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message)
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("git could not run: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// The new path as given: absolute, without `.`/`..` parts or a trailing separator.
fn read_new_path(raw: &str) -> Result<PathBuf, DomainStateError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(refuse("Give the folder's new path."));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(refuse(format!(
            "The new path must be a full path, like {}.",
            if cfg!(windows) {
                r"C:\dev\Kiwimake"
            } else {
                "/Users/you/dev/Kiwimake"
            }
        )));
    }
    if path.components().any(|part| {
        matches!(
            part,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) {
        return Err(refuse("The new path can't contain . or .. parts."));
    }
    let text = raw.trim_end_matches(|c| c == '/' || (cfg!(windows) && c == '\\'));
    Ok(PathBuf::from(if text.is_empty() { raw } else { text }))
}

pub(crate) fn plan(
    paths: &GxserverPaths,
    repository: &DomainRepository<'_>,
    project_id: &str,
    new_path_raw: &str,
    force_open_files: bool,
    dry_run: bool,
) -> Result<Checked, DomainStateError> {
    let mut warnings = Vec::new();
    let project = repository.get_project(project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    let name = text(&project, "name").unwrap_or(project_id).to_string();
    if project.get("worktree").is_some_and(|worktree| {
        worktree.is_object() && !worktree.as_object().is_some_and(|map| map.is_empty())
    }) {
        return Err(refuse("This project is a worktree. Rename it with Rename Worktree in the sidebar, which moves its folder and branch."));
    }
    if text(&project, "systemKind").is_some() {
        return Err(refuse(
            "This is one of Ghostex's own projects; its folder can't be moved.",
        ));
    }
    let old_path = text(&project, "path")
        .ok_or_else(|| refuse("This project has no folder."))?
        .to_string();
    let old = PathBuf::from(&old_path);
    if fs_ops::is_link(&old) {
        return Err(refuse(format!("{old_path} is a link to another folder. Move the folder it points to instead, then point the project at it.")));
    }
    if !old.is_dir() {
        return Err(refuse(format!("{old_path} isn't there any more. Locate the folder first (the project's Locate folder), then move it.")));
    }
    let new = read_new_path(new_path_raw)?;
    let new_path = new.to_string_lossy().to_string();
    let home = crate::resume_lookup::home_dir();

    // Spellings and letter case, while the old folder still exists (risk 4).
    let mut old_spellings = vec![old_path.clone()];
    if let Ok(resolved) = std::fs::canonicalize(&old) {
        let resolved = resolved.to_string_lossy().to_string();
        let resolved = resolved
            .strip_prefix(r"\\?\")
            .map(str::to_string)
            .unwrap_or(resolved);
        if !old_spellings.contains(&resolved) {
            old_spellings.push(resolved);
        }
    }
    let ignore_case = cfg!(windows) || ghostex_paths::same_folder::volume_ignores_case(&old);
    let root = MovedRoot {
        old_spellings: old_spellings.clone(),
        new_path: new_path.clone(),
        ignore_case,
    };

    // The destination.
    let windows = cfg!(windows);
    let same_text_ignoring_case = {
        let a = split_parts(&old_path, windows);
        let b = split_parts(&new_path, windows);
        a.len() == b.len()
            && a.iter()
                .zip(&b)
                .all(|(x, y)| x.to_lowercase() == y.to_lowercase())
    };
    if same_text_ignoring_case && old_path == new_path {
        return Err(refuse("The project is already in that folder."));
    }
    let case_only =
        same_text_ignoring_case && new.exists() && fs_ops::same_folder_on_disk(&old, &new);
    if !case_only {
        if std::fs::symlink_metadata(&new).is_ok() {
            return Err(refuse(format!(
                "{new_path} already exists. Pick a name that isn't taken."
            )));
        }
        if root.contains(&new_path) {
            return Err(refuse(
                "The new folder can't be inside the folder being moved.",
            ));
        }
        let new_root = MovedRoot {
            old_spellings: vec![new_path.clone()],
            new_path: String::new(),
            ignore_case,
        };
        if old_spellings
            .iter()
            .any(|spelling| new_root.contains(spelling))
        {
            return Err(refuse("The folder can't move into a folder inside itself."));
        }
    }
    let new_parent = new
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| refuse("The new path needs a parent folder."))?;
    if !new_parent.is_dir() {
        return Err(refuse(format!(
            "{} doesn't exist. Create it first, or pick another place.",
            new_parent.display()
        )));
    }
    if fs_ops::is_system_folder(&new, &home) || fs_ops::is_system_folder(&old, &home) {
        return Err(refuse(
            "A project can't be moved into or out of a system folder or your home folder itself.",
        ));
    }
    for own in [
        &paths.app_state_dir,
        &paths.app_config_dir,
        &paths.app_data_dir,
        &paths.root_dir,
    ] {
        let own_root = MovedRoot {
            old_spellings: vec![own.to_string_lossy().to_string()],
            new_path: String::new(),
            ignore_case,
        };
        if own_root.contains(&new_path) {
            return Err(refuse(
                "A project can't be moved into Ghostex's own folders.",
            ));
        }
    }
    if let Some(held) = holds_ghostex_or_agent_state(paths, &home, &root) {
        return Err(refuse(format!(
            "{old_path} holds {held}, which must stay where it is while Ghostex runs. Move the project's folder with Ghostex quit, or leave it where it is."
        )));
    }
    if !fs_ops::same_volume(&old, &new_parent) {
        return Err(refuse("The new folder is on another drive. Ghostex only moves a folder within one drive; copy it yourself and add the copy as a project instead."));
    }

    // Projects the move touches, and who is registered where.
    let projects = repository.list_projects_without_session_history()?;
    let new_spot = MovedRoot {
        old_spellings: vec![new_path.clone()],
        new_path: String::new(),
        ignore_case,
    };
    let mut project_ids = vec![project_id.to_string()];
    let mut sibling_worktrees = Vec::new();
    let old_parent = old
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default();
    for other in &projects {
        let Some(other_id) = text(other, "projectId") else {
            continue;
        };
        if other_id == project_id {
            continue;
        }
        let other_path = text(other, "path").unwrap_or_default();
        if !case_only
            && !other_path.is_empty()
            && new_spot
                .rest_below(other_path)
                .is_some_and(|rest| rest.is_empty())
        {
            return Err(refuse(format!(
                "{new_path} is already the folder of the project {}.",
                text(other, "name").unwrap_or(other_id)
            )));
        }
        let is_worktree_of_this = other
            .pointer("/worktree/parentProjectId")
            .and_then(Value::as_str)
            == Some(project_id);
        let inside = !other_path.is_empty() && root.contains(other_path);
        if is_worktree_of_this || inside {
            project_ids.push(other_id.to_string());
        }
        if is_worktree_of_this && !inside {
            let parent = Path::new(other_path)
                .parent()
                .map(|parent| parent.to_string_lossy().to_string())
                .unwrap_or_default();
            if parent.to_lowercase() == old_parent.to_lowercase() {
                sibling_worktrees.push(text(other, "name").unwrap_or(other_path).to_string());
            }
        }
    }

    // Parent change with Ghostex worktrees beside the folder (risk 15).
    let parent_changes = {
        let a = split_parts(&old_parent, windows);
        let new_parent_text = new_parent.to_string_lossy().to_string();
        let b = split_parts(&new_parent_text, windows);
        !(a.len() == b.len()
            && a.iter().zip(&b).all(|(x, y)| {
                if ignore_case {
                    x.to_lowercase() == y.to_lowercase()
                } else {
                    x == y
                }
            }))
    };
    if parent_changes && !sibling_worktrees.is_empty() {
        return Err(refuse(format!(
            "This project has worktrees next to its folder ({}). Rename the folder in place (same parent folder), or remove the worktrees first.",
            sibling_worktrees.join(", ")
        )));
    }

    // Sessions running (risk 5, 12).
    let running = running_sessions(repository, &project_ids, &root, &projects)?;
    if !running.is_empty() {
        return Err(refuse(format!(
            "Close or sleep these sessions first, they're running in this folder: {}.",
            running.join("; ")
        )));
    }

    // Processes outside Ghostex with their working folder inside (risk 8), or with files open
    // inside it (Opus review, finding 6): an editor saving into the old path afterwards would
    // recreate it beside the moved folder.
    let open = fs_ops::processes_using(
        &|path| root.contains(path),
        std::time::Duration::from_secs(15),
    );
    let list = |names: &[String]| {
        let shown: Vec<_> = names.iter().take(6).cloned().collect();
        let more = names.len().saturating_sub(shown.len());
        format!(
            "{}{}",
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        )
    };
    match open {
        Ok(open) => {
            if !open.cwd.is_empty() && !force_open_files {
                return Err(refuse(format!(
                    "These programs are working inside the folder: {}. Close them (or cd out of it), then try again; --force-open-files moves it anyway.",
                    list(&open.cwd)
                )));
            }
            if !open.files.is_empty() && !force_open_files {
                let message = format!(
                    "These programs have files in the folder open: {}. Close the files (or the programs), then try again; --force-open-files moves it anyway.",
                    list(&open.files)
                );
                if !dry_run {
                    return Err(refuse(message));
                }
                warnings.push(message);
            }
        }
        Err(()) => warnings.push("Ghostex couldn't finish checking which programs have files open in the folder (lsof took too long). Close editors using it before the move.".to_string()),
    }
    // Ghostex's own Files editor: unsaved changes would be saved into the old path.
    let editor = ghostex_editor_files(&project_ids);
    if !editor.unsaved.is_empty() {
        let message = format!(
            "Ghostex's Files editor has unsaved changes in {}. Save or discard them first.",
            list(&editor.unsaved)
        );
        if !dry_run {
            return Err(refuse(message));
        }
        warnings.push(message);
    }
    if !editor.open.is_empty() {
        warnings.push(format!(
            "Ghostex's Files editor has {} file(s) of this project open; reopen them from the Files list after the move if one shows as missing.",
            editor.open.len()
        ));
    }

    // Cloud-synced folders (risk 9).
    let old_sync = fs_ops::sync_root_of(&old, &home);
    let new_sync = fs_ops::sync_root_of(&new, &home);
    let same_sync = match (&old_sync, &new_sync) {
        (Some(a), Some(b)) => {
            a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
        }
        (None, None) => true,
        _ => false,
    };
    if !same_sync {
        return Err(refuse("The folder would move into or out of a cloud-synced folder (iCloud, OneDrive, Dropbox, Google Drive). Move it within the same synced folder, or let the sync app move it."));
    }
    if old_sync.is_some() {
        match fs_ops::first_placeholder(&old, std::time::Duration::from_secs(15)) {
            Ok(Some(file)) => {
                return Err(refuse(format!("{} is only in the cloud and not downloaded. Download the whole folder (Keep Downloaded / Always keep on this device) first.", file.display())))
            }
            Ok(None) => warnings.push("The folder is in a cloud-synced folder. The sync app will see the move; let it finish before opening the project elsewhere.".to_string()),
            Err(()) => return Err(refuse("The folder is in a cloud-synced folder and is too large to check that every file is downloaded. Download it fully, or move it outside the synced folder yourself.")),
        }
    }

    if let Some(warning) = fs_ops::long_path_warning(&old, &new) {
        warnings.push(warning);
    }

    // Git: every checkout inside the folder and its linked worktrees (risk 7, 15).
    let mut checkouts = Vec::new();
    let mut worktrees = Vec::new();
    let mut checkout_roots: Vec<String> = vec![old_path.clone()];
    for other in &projects {
        if let (Some(id), Some(path)) = (text(other, "projectId"), text(other, "path")) {
            if id != project_id
                && project_ids.iter().any(|known| known == id)
                && root.contains(path)
            {
                checkout_roots.push(path.to_string());
            }
        }
    }
    for checkout in checkout_roots {
        let checkout_path = PathBuf::from(&checkout);
        let dot_git = checkout_path.join(".git");
        if dot_git.is_file() && checkout == old_path {
            return Err(refuse("This folder is a git worktree of another checkout. Move it with git worktree move, then point the project at it."));
        }
        if !dot_git.is_dir() {
            continue;
        }
        let Ok(list) = git(&checkout_path, &["worktree", "list", "--porcelain"]) else {
            continue;
        };
        checkouts.push(checkout.clone());
        let mut current: Option<String> = None;
        let mut first = true;
        for line in list.lines().chain(std::iter::once("")) {
            if let Some(path) = line.strip_prefix("worktree ") {
                current = Some(path.to_string());
            } else if line.starts_with("locked") {
                if let (Some(path), false) = (&current, first) {
                    return Err(refuse(format!("The worktree {path} is locked (git worktree lock). Unlock it with git worktree unlock first.")));
                }
            } else if line.is_empty() {
                if let Some(path) = current.take() {
                    if !first {
                        let has_submodules =
                            crate::worktree_sessions::worktree_has_populated_submodules(&path);
                        worktrees.push(JournalWorktree {
                            checkout: checkout.clone(),
                            path,
                            has_submodules,
                        });
                    }
                    first = false;
                }
            }
        }
    }

    let claude_old_keys = crate::resume_lookup::claude_project_folder_names(&old_path)
        .into_iter()
        .filter(|key| key.len() <= 200)
        .collect();
    // Codex keys a repository's trust by its root, which is this folder for a checkout.
    // Under every spelling of the folder: agents record the one their process saw.
    let mut trusted_by: Vec<String> = Vec::new();
    for spelling in &old_spellings {
        for agent in crate::agent_folder_trust::folder_trusted_by(Path::new(spelling)) {
            if !trusted_by.contains(&agent) {
                trusted_by.push(agent);
            }
        }
    }

    let _ = name;
    Ok(Checked {
        journal: Journal {
            version: 1,
            project_id: project_id.to_string(),
            old_path,
            new_path,
            old_spellings,
            ignore_case,
            case_only,
            project_ids,
            checkouts,
            worktrees,
            claude_old_keys,
            trusted_by,
            phase: Phase::Planned,
            started_at: crate::domain::now_iso(),
            notes: Vec::new(),
        },
        warnings,
    })
}

/// Running sessions of the touched projects or with a cwd inside the folder, as
/// `"title" (agent) in Project`.
pub(crate) fn running_sessions(
    repository: &DomainRepository<'_>,
    project_ids: &[String],
    root: &MovedRoot,
    projects: &[Value],
) -> Result<Vec<String>, DomainStateError> {
    let mut found = Vec::new();
    for session in repository.list_sessions(None)? {
        // `unknown` (right after a gxserver restart, before its probe) may well be running.
        if !matches!(
            session.get("lifecycleState").and_then(Value::as_str),
            Some("running" | "unknown")
        ) {
            continue;
        }
        let session_project = text(&session, "projectId").unwrap_or_default();
        let touched = project_ids.iter().any(|id| id == session_project)
            || text(&session, "cwd").is_some_and(|cwd| root.contains(cwd));
        if !touched {
            continue;
        }
        let project_name = projects
            .iter()
            .find(|project| text(project, "projectId") == Some(session_project))
            .and_then(|project| text(project, "name"))
            .unwrap_or(session_project);
        let agent = text(&session, "agentId").unwrap_or("terminal");
        found.push(format!(
            "\"{}\" ({agent}) in {project_name}",
            text(&session, "title").unwrap_or("Untitled")
        ));
    }
    Ok(found)
}

/// What of Ghostex itself or an agent's own state lives inside the folder, if anything: gxserver's
/// state and settings folders, the running gxserver program, `~/.claude` (and every Claude
/// profile's projects folder) and `~/.codex` (or `CODEX_HOME`).
///
/// CDXC:Projects 2026-11-20 WHY: a move only refused moves into Ghostex's folders, so the folder holding gxserver's database, the dev build that is running, or an agent's conversations could still be renamed away under them (Opus review, finding 7).
fn holds_ghostex_or_agent_state(
    paths: &GxserverPaths,
    home: &Path,
    root: &MovedRoot,
) -> Option<String> {
    let mut places: Vec<(PathBuf, &str)> = vec![
        (paths.app_state_dir.clone(), "Ghostex's state"),
        (paths.app_config_dir.clone(), "Ghostex's settings"),
        (paths.app_data_dir.clone(), "Ghostex's data"),
        (paths.root_dir.clone(), "gxserver's state"),
        (home.join(".claude"), "Claude's settings and conversations"),
        (home.join(".codex"), "Codex's settings and conversations"),
    ];
    if let Ok(program) = std::env::current_exe() {
        places.push((program, "the gxserver program that is running"));
    }
    for (variable, what) in [
        ("CODEX_HOME", "Codex's settings and conversations"),
        ("CLAUDE_CONFIG_DIR", "Claude's settings and conversations"),
    ] {
        if let Some(value) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            places.push((PathBuf::from(value), what));
        }
    }
    for projects in crate::agent_transcripts::claude_project_roots() {
        places.push((projects, "Claude's conversations"));
    }
    places.into_iter().find_map(|(place, what)| {
        let mut spellings = vec![place.to_string_lossy().to_string()];
        if let Ok(resolved) = std::fs::canonicalize(&place) {
            spellings.push(resolved.to_string_lossy().to_string());
        }
        spellings
            .iter()
            .any(|spelling| root.contains(spelling))
            .then(|| format!("{what} ({})", place.display()))
    })
}

/// Files Ghostex's Files editor holds for the projects: those with unsaved changes, and those
/// open. Read from the desktop's client storage (apps/desktop/src/app/native_docs/storage.rs,
/// `docsDrafts` and `docsOpenFiles`); nothing when it can't be read.
#[derive(Default)]
pub(crate) struct EditorFiles {
    pub(crate) unsaved: Vec<String>,
    pub(crate) open: Vec<String>,
}

pub(crate) fn ghostex_editor_files(project_ids: &[String]) -> EditorFiles {
    let file = ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("client-storage.sqlite3");
    editor_files_in(&file, project_ids)
}

fn editor_files_in(file: &Path, project_ids: &[String]) -> EditorFiles {
    let mut found = EditorFiles::default();
    if !file.is_file() {
        return found;
    }
    let Ok(db) = rusqlite::Connection::open_with_flags(
        file,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return found;
    };
    let _ = db.busy_timeout(std::time::Duration::from_secs(2));
    let raw = |store: &str, key: String| -> Option<Value> {
        let row: String = db
            .query_row(
                "SELECT value FROM records WHERE store = ?1 AND key = ?2",
                [store, key.as_str()],
                |row| row.get(0),
            )
            .ok()?;
        let row: Value = serde_json::from_str(&row).ok()?;
        serde_json::from_str(row.get("raw")?.as_str()?).ok()
    };
    for project_id in project_ids {
        if let Some(Value::Object(drafts)) =
            raw("docsDrafts", format!("ghostex.manage.drafts.{project_id}"))
        {
            for (path, draft) in drafts {
                let text = draft.get("draft").and_then(Value::as_str).unwrap_or("");
                let saved = draft
                    .get("savedContent")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !text.is_empty() && text != saved && !found.unsaved.contains(&path) {
                    found.unsaved.push(path);
                }
            }
        }
        if let Some(Value::Array(open)) = raw(
            "docsOpenFiles",
            format!("ghostex.manage.openFiles.{project_id}"),
        ) {
            for path in open.iter().filter_map(Value::as_str) {
                if !path.starts_with(".ghostex-review") && !found.open.iter().any(|p| p == path) {
                    found.open.push(path.to_string());
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_editor_s_unsaved_changes_and_open_files_are_read() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("client-storage.sqlite3");
        let db = rusqlite::Connection::open(&file).unwrap();
        db.execute_batch(
            "CREATE TABLE records (key TEXT PRIMARY KEY, store TEXT NOT NULL, value TEXT NOT NULL);",
        )
        .unwrap();
        let put = |store: &str, key: &str, raw: Value| {
            let row = serde_json::json!({ "key": key, "store": store, "raw": raw.to_string() });
            db.execute(
                "INSERT INTO records VALUES (?1, ?2, ?3)",
                [key, store, &row.to_string()],
            )
            .unwrap();
        };
        put(
            "docsDrafts",
            "ghostex.manage.drafts.p1",
            serde_json::json!({
                "src/a.rs": { "draft": "edited", "savedContent": "saved" },
                "src/b.rs": { "draft": "same", "savedContent": "same" },
                "src/c.rs": { "draft": "", "savedContent": "text" },
            }),
        );
        put(
            "docsOpenFiles",
            "ghostex.manage.openFiles.p1",
            serde_json::json!(["src/a.rs", "README.md", ".ghostex-review/x"]),
        );
        put(
            "docsDrafts",
            "ghostex.manage.drafts.other",
            serde_json::json!({ "x.rs": { "draft": "edited", "savedContent": "saved" } }),
        );
        let found = editor_files_in(&file, &["p1".to_string()]);
        assert_eq!(found.unsaved, vec!["src/a.rs".to_string()]);
        assert_eq!(
            found.open,
            vec!["src/a.rs".to_string(), "README.md".to_string()]
        );
        let none = editor_files_in(&temp.path().join("missing.sqlite3"), &["p1".to_string()]);
        assert!(none.unsaved.is_empty() && none.open.is_empty());
    }
}
