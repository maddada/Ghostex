//! Step 4 (git) and step 5 (agent history). Both run after the database names the new folder,
//! report what they could not do instead of undoing the move, and are safe to run again.

use std::path::{Path, PathBuf};

use super::checks::git;
use super::journal::Journal;

/// `git worktree repair` from each moved checkout, `git submodule absorbgitdirs` in worktrees that
/// had submodules set up (risk 7), then `git status` in every checkout and worktree as the check.
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
        // CDXC:Projects 2026-11-20 WHY: `absorbgitdirs` alone repoints the submodules' `.git`. A `git submodule update` after it would check out the commit the worktree recorded, resetting a submodule's own local commits on its detached HEAD (and setting up submodules the user left uninitialised), so it is not run (Opus review of move-project-folder, finding 1; `repair_keeps_worktree_submodule_commits` below).
        if let Err(error) = git(&path, &["submodule", "absorbgitdirs"]) {
            notes.push(format!(
                "git submodule absorbgitdirs in {} failed: {error}",
                path.display()
            ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_folder_move::journal::{JournalWorktree, Phase};

    /// git with a fixed identity and none of the user's settings (signing, hooks).
    fn run_git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "protocol.file.allow=always",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit_file(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
        run_git(dir, &["add", name]);
        run_git(dir, &["commit", "-qm", name]);
    }

    /// Opus review of move-project-folder, finding 1: a worktree's submodule with its own local
    /// commit (the usual detached-HEAD submodule work) keeps it, and its uncommitted edit, through
    /// the repair after its main checkout's folder moved.
    #[test]
    fn repair_keeps_worktree_submodule_commits() {
        let temp = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(temp.path()).unwrap();
        let library = dir.join("lib-src");
        std::fs::create_dir(&library).unwrap();
        run_git(&library, &["init", "-q", "-b", "main"]);
        commit_file(&library, "lib.txt", "one");

        let repo = dir.join("Repo");
        std::fs::create_dir(&repo).unwrap();
        run_git(&repo, &["init", "-q", "-b", "main"]);
        commit_file(&repo, "app.txt", "app");
        run_git(&repo, &["submodule", "add", "-q", "../lib-src", "sm"]);
        run_git(&repo, &["commit", "-qm", "submodule"]);
        let feature = dir.join("Repo-feat");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feat",
                feature.to_str().unwrap(),
            ],
        );
        run_git(&feature, &["submodule", "update", "--init", "-q"]);
        let submodule = feature.join("sm");
        commit_file(&submodule, "local.txt", "a local commit");
        std::fs::write(submodule.join("lib.txt"), "an uncommitted edit").unwrap();
        let local_commit = run_git(&submodule, &["rev-parse", "HEAD"]);

        let kiwi = dir.join("Kiwi");
        std::fs::rename(&repo, &kiwi).unwrap();
        let old_path = repo.to_string_lossy().to_string();
        let journal = Journal {
            version: 1,
            project_id: "p".to_string(),
            old_path: old_path.clone(),
            new_path: kiwi.to_string_lossy().to_string(),
            old_spellings: vec![old_path.clone()],
            ignore_case: false,
            case_only: false,
            project_ids: vec!["p".to_string()],
            checkouts: vec![old_path.clone()],
            worktrees: vec![JournalWorktree {
                checkout: old_path,
                path: feature.to_string_lossy().to_string(),
                has_submodules: true,
            }],
            claude_old_keys: Vec::new(),
            trusted_by: Vec::new(),
            phase: Phase::Recorded,
            started_at: String::new(),
            notes: Vec::new(),
        };
        let notes = repair_git(&journal);
        assert!(notes.is_empty(), "{notes:?}");

        assert_eq!(run_git(&submodule, &["rev-parse", "HEAD"]), local_commit);
        assert_eq!(
            std::fs::read_to_string(submodule.join("lib.txt")).unwrap(),
            "an uncommitted edit"
        );
        assert!(run_git(&submodule, &["status", "--porcelain"]).contains("lib.txt"));
        run_git(&feature, &["status", "--porcelain"]);
        run_git(&kiwi, &["status", "--porcelain"]);
        let link = std::fs::read_to_string(submodule.join(".git")).unwrap();
        assert!(link.contains("Kiwi"), "{link}");
    }
}
