//! CDXC:AgentProviders 2026-11-01 WHY:
//! A Claude account switch resumes the session's conversation with `cswap run <N> --share-history -- --resume <id>`, which reads the transcript from the projects folder of the profile it starts. `--share-history` links a profile's `projects/` to `~/.claude/projects` only once nothing runs in that profile, because merging an existing private folder moves files out from under a live Claude (claude-swap's `_prepare_history_share`). Account 5's profile kept its private folder from before Ghostex passed the flag, and some Claude has run in it ever since (two sessions started there on 2026-10-25 and 2026-10-31, and the `claude daemon run` background daemon a coordinator's Claude spawned there on 2026-10-31 for a background session, which outlives the session that started it), so cswap printed "Not sharing projects yet: another session is using this profile" on every launch. Every conversation started on account 5 lived only in that folder, and switching it to another account ended in "No conversation found" with the session left on account 5 and its usage-limit card still up. Before any switch moves a conversation, the transcript is hard-linked into the projects folder the target account's Claude reads: one file under two names, so the resumed CLI and any later switch back append to the same conversation, and claude-swap's eventual merge (which drops a profile file when the shared folder already has one of that name) keeps the other name. A switch whose conversation cannot be linked there is refused before anything changes.
//! SEE-ALSO: server/src/accounts/endpoint.rs (`select`), server/src/accounts/wake_account.rs (`rebind_exhausted`), server/src/accounts/default_login.rs (which profile `cswap run` starts), server/src/accounts/claude_resets.rs (`swap_root`, `ClaudeSlot::session_dir`).

use super::{claude_resets, model::SavedAccount};
use crate::domain::DomainStateError;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Makes the Claude conversation `session` resumes readable from every projects folder Claude can
/// start in for `target` (the slot's own profile, and `~/.claude` when the default login is that
/// account, which is where `cswap run` starts it). A session with no conversation on this computer
/// yet has nothing to bring.
pub(crate) fn bring_claude_conversation(
    home: &Path,
    session: &Value,
    target: &SavedAccount,
) -> Result<(), DomainStateError> {
    let Some(id) = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .filter(|id| is_conversation_id(id))
    else {
        return Ok(());
    };
    let file_name = format!("{id}.jsonl");
    let Some(source) = newest_copy(&projects_roots(home), &file_name) else {
        return Ok(());
    };
    let Some(project_folder) = source.parent().and_then(Path::file_name) else {
        return Ok(());
    };
    let slot = claude_resets::ClaudeSlot::read(home, &target.selector)
        .map_err(DomainStateError::bad_request)?;
    // CDXC:AgentProviders 2026-11-04 WHY:
    // A profile with no `projects/` yet gets cswap's link to `~/.claude/projects` the next time `cswap run --share-history` starts it, so the conversation goes there. Creating the folder instead made it a private one cswap then had to merge before it could link it: on 2026-11-03 account 2's profile held nothing but such a folder, and its later conversations stayed out of the shared history until a quiet moment for that merge.
    let profile_projects = PathBuf::from(slot.session_dir()).join("projects");
    let shared = home.join(".claude/projects");
    let mut targets = vec![if std::fs::symlink_metadata(&profile_projects).is_ok() {
        profile_projects
    } else {
        shared.clone()
    }];
    if slot.names_default_login(home) && !targets.contains(&shared) {
        targets.push(shared);
    }
    let label = super::launch::account_label(&target.selector, &target.name);
    for root in targets {
        let folder = root.join(project_folder);
        let dest = folder.join(&file_name);
        link_file(&source, &dest).map_err(|reason| {
            DomainStateError::bad_request(format!(
                "This conversation can't move to {label}: {reason} The session stays on its current account."
            ))
        })?;
        // Subagent transcripts and saved tool output: the resume works without them, so a file
        // that cannot be linked only leaves that detail behind.
        if let Some(parent) = source.parent() {
            link_tree(&parent.join(id), &folder.join(id));
        }
    }
    Ok(())
}

/// Claude names conversations by UUID; anything else is not a file name to look for.
fn is_conversation_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
}

/// `~/.claude/projects` and the projects folder of every claude-swap profile.
fn projects_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![home.join(".claude/projects")];
    if let Ok(profiles) = std::fs::read_dir(claude_resets::swap_root(home).join("sessions")) {
        roots.extend(
            profiles
                .flatten()
                .map(|profile| profile.path().join("projects")),
        );
    }
    roots
}

/// The most recently written copy of `file_name` in any project folder of `roots`. Profiles that
/// share history reach the same file through their linked `projects/`, so this is usually one file.
fn newest_copy(roots: &[PathBuf], file_name: &str) -> Option<PathBuf> {
    roots
        .iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flat_map(|folders| folders.flatten())
        .map(|folder| folder.path().join(file_name))
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// Gives `source` the extra name `dest`, unless `dest` already is that file.
fn link_file(source: &Path, dest: &Path) -> Result<(), String> {
    match (std::fs::metadata(source), std::fs::metadata(dest)) {
        (Ok(from), Ok(to)) if same_file(&from, &to) => return Ok(()),
        (Ok(_), Ok(_)) => {
            return Err(format!(
                "that account already has a different copy of it at {}.",
                dest.display()
            ))
        }
        (Err(error), _) => return Err(format!("its transcript could not be read ({error}).")),
        _ => {}
    }
    if let Some(folder) = dest.parent() {
        create_private_dir(folder)
            .map_err(|error| format!("{} could not be created ({error}).", folder.display()))?;
    }
    std::fs::hard_link(source, dest)
        .map_err(|error| format!("{} could not be linked ({error}).", dest.display()))
}

/// Links every file under `source` that `dest` does not have yet.
fn link_tree(source: &Path, dest: &Path) {
    let Ok(entries) = std::fs::read_dir(source) else {
        return;
    };
    for entry in entries.flatten() {
        let (from, to) = (entry.path(), dest.join(entry.file_name()));
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => link_tree(&from, &to),
            Ok(kind) if kind.is_file() && !to.exists() => {
                let _ = to
                    .parent()
                    .map_or(Ok(()), create_private_dir)
                    .and_then(|_| std::fs::hard_link(&from, &to));
            }
            _ => {}
        }
    }
}

/// Claude Code keeps its history folders at 0700.
fn create_private_dir(folder: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(folder)
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}
