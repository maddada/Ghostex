//! Step 4 (git) and step 5 (agent history). Both run after the database names the new folder,
//! report what they could not do instead of undoing the move, and are safe to run again.

use std::path::{Path, PathBuf};

use super::checks::git;
use super::journal::Journal;

/// `git worktree repair` from each moved checkout, the submodule fix in worktrees that had
/// submodules set up (risk 7), then `git status` in every checkout and worktree as the check.
pub(crate) fn repair_git(journal: &Journal) -> Vec<String> {
    let root = journal.root();
    let mut notes = Vec::new();
    let moved = |path: &str| PathBuf::from(root.rebase(path).unwrap_or_else(|| path.to_string()));
    for checkout in &journal.checkouts {
        let checkout_now = moved(checkout);
        let linked: Vec<PathBuf> = journal
            .worktrees
            .iter()
            .filter(|worktree| &worktree.checkout == checkout)
            .map(|worktree| moved(&worktree.path))
            .filter(|path| path.exists())
            .collect();
        let mut args: Vec<String> = vec!["worktree".into(), "repair".into()];
        args.extend(linked.iter().map(|path| path.to_string_lossy().to_string()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(error) = git(&checkout_now, &args) {
            notes.push(format!(
                "git worktree repair in {} failed: {error}. Run it there by hand.",
                checkout_now.display()
            ));
        }
    }
    for worktree in journal
        .worktrees
        .iter()
        .filter(|worktree| worktree.has_submodules)
    {
        let path = moved(&worktree.path);
        if !path.exists() {
            continue;
        }
        for args in [
            &["submodule", "absorbgitdirs"][..],
            &["submodule", "update", "--init", "--recursive", "--no-fetch"][..],
        ] {
            if let Err(error) = git(&path, args) {
                notes.push(format!(
                    "git {} in {} failed: {error}",
                    args.join(" "),
                    path.display()
                ));
            }
        }
    }
    let mut checked: Vec<PathBuf> = journal.checkouts.iter().map(|path| moved(path)).collect();
    checked.extend(
        journal
            .worktrees
            .iter()
            .map(|worktree| moved(&worktree.path)),
    );
    for path in checked.iter().filter(|path| path.exists()) {
        if let Err(error) = git(path, &["status", "--porcelain=v1"]) {
            notes.push(format!(
                "git status in {} fails after the move: {error}",
                path.display()
            ));
        }
    }
    notes
}

/// Copies `from` into `to`, file by file, skipping files that already exist there; each file is
/// written to a temporary name and renamed. Returns how many files it copied.
fn copy_missing(from: &Path, to: &Path) -> std::io::Result<usize> {
    let mut copied = 0;
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copied += copy_missing(&source, &target)?;
        } else if kind.is_file() && !target.exists() {
            let temporary = to.join(format!(
                ".{}.gx-move.tmp",
                entry.file_name().to_string_lossy()
            ));
            std::fs::copy(&source, &temporary)?;
            std::fs::rename(&temporary, &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// CDXC:Projects 2026-11-19 WHY: Claude resumes a conversation by its id from any folder and keeps writing the transcript it already has (verified with Claude Code 2.1.296, worktree_sessions.rs), so transcripts and `agentSessionPath` stay where they are: a copy would be a second file with the same id that goes stale (risk 11 of docs/2026-11-19/project-move-folder/PLAN.md, the review's answer 2). Only the project's `memory/` folder is copied to the new folder's name in each Claude profile, Ghostex's lookups find the old conversations through the former paths, and a folder some profile trusted is trusted again at its new path.
pub(crate) fn carry_agent_history(journal: &Journal) -> Vec<String> {
    let mut notes = Vec::new();
    if journal.case_only {
        return notes;
    }
    let new_keys = crate::resume_lookup::claude_project_folder_names(&journal.new_path);
    let Some(new_key) = new_keys.first().filter(|key| key.len() <= 200) else {
        notes.push("The new folder's path is too long for Ghostex to work out Claude's folder name, so Claude's project memory wasn't copied.".to_string());
        return notes;
    };
    let mut seen = Vec::new();
    let mut copied = 0;
    let mut old_folders = Vec::new();
    for projects in crate::agent_transcripts::claude_project_roots() {
        let resolved = std::fs::canonicalize(&projects).unwrap_or_else(|_| projects.clone());
        if seen.contains(&resolved) {
            continue;
        }
        seen.push(resolved);
        for old_key in &journal.claude_old_keys {
            let old_folder = projects.join(old_key);
            if !old_folder.is_dir() {
                continue;
            }
            // One folder under two spellings on a disk that ignores letter case.
            let resolved =
                std::fs::canonicalize(&old_folder).unwrap_or_else(|_| old_folder.clone());
            if seen.contains(&resolved) {
                continue;
            }
            seen.push(resolved);
            old_folders.push(old_folder.to_string_lossy().to_string());
            let memory = old_folder.join("memory");
            if memory.is_dir() {
                match copy_missing(&memory, &projects.join(new_key).join("memory")) {
                    Ok(count) => copied += count,
                    Err(error) => notes.push(format!(
                        "Claude's project memory in {} could not be copied: {error}",
                        memory.display()
                    )),
                }
            }
        }
    }
    if copied > 0 {
        notes.push(format!(
            "Copied {copied} Claude project memory file(s) to the new folder's name."
        ));
    }
    if !old_folders.is_empty() {
        notes.push(format!(
            "Claude's conversations stay in {} and resume by id from the new folder; Claude's own resume list outside Ghostex shows them only from the old path.",
            old_folders.join(", ")
        ));
    }
    if !journal.trusted_by.is_empty() {
        notes.extend(crate::agent_folder_trust::carry_folder_trust_for_move(
            &journal.trusted_by,
            Path::new(&journal.new_path),
        ));
        notes.push(format!(
            "The new folder is trusted for {} as the old one was.",
            journal.trusted_by.join(" and ")
        ));
    }
    if journal.trusted_by.iter().any(|agent| agent == "claude") {
        notes.push("Claude's allowed tools and MCP servers set for the old folder stay under the old path in its settings.".to_string());
    }
    notes.push("Codex resumes these sessions in the new folder; its own codex resume list shows the old ones with --all.".to_string());
    notes
}
