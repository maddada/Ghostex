//! Bundled skills that were renamed: on the first start after the update that renamed one, a
//! machine that has the old name installed gets the new name installed the same way and the old
//! one removed, without the user doing anything.
//!
//! CDXC:Coordinators 2026-11-11 DECISION:
//! User: "please make /ghostex-orchestrator renamed to /ghostex-coordinator everywhere from next update and replace the skill with the new name". The product already called the role a coordinator (the CLI is `ghostex coordinator`, the app says Coordinator); only the skill kept the old word. So the skill is `ghostex-coordinator` and gxserver stops knowing `ghostex-orchestrator` (no install, no refresh). At startup, before the refresh from the signed skill list, [`migrate_renamed_ghostex_agent_skills`] moves each install over: the new skill is written to `~/.agents/skills` from the app bundle when it is missing, every agent folder that had the old skill (a link Ghostex made, or a copy older builds made) gets a link to the new one (none for an agent that reads `~/.agents/skills` itself), and only then are the old entries Ghostex made removed, so no coordinator is ever left without a playbook. An old copy is Ghostex's when every file in it byte-matches a revision Ghostex shipped (the retired skills' rule, agent_skills.rs `remove_retired_ghostex_agent_skills`). Anything else named `ghostex-orchestrator` is the user's and is never deleted: an edited copy, the user's own link (to their checkout, say) and the agent links that point at a kept copy stay where they are (that agent still gets the new skill linked beside them), are reported as kept in the `renamedAgentSkillsMoved` log line, and are left out of Ghostex's installs and refreshes from then on. Coordinators started before the rename named `/ghostex-orchestrator` in their first message; they keep their playbook through `ghostex coordinator guide`, which prints it from the app (coordinators/role.rs `coordinator_guide_for` says the skill's new name), and the changed guide gets them the "playbook changed" note with their next message (coordinators/playbook.rs `playbook_update_note`).
//! SEE-ALSO: server/src/agent_skills_remote.rs (the signed list: an app from before the rename skips `ghostex-coordinator` and keeps its own `ghostex-orchestrator`; this app skips `ghostex-orchestrator` in an older list), server/src/server/foreground.rs (runs it at startup).

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::{
    agent_skills::{
        agent_reads_canonical_skills, agent_skill_global_dir, link_skill_into_agent_root,
        path_string, CANONICAL_AGENT_SKILLS_DIR, GHOSTEX_SKILLS_CLI_AGENT_IDS,
    },
    agent_skills_remote::installed_skill_roots,
    paths::GxserverPaths,
};

/// One bundled skill that changed its name, maybe more than once.
struct RenamedSkill {
    /// Every name it had before, oldest first.
    old_names: &'static [&'static str],
    new: &'static str,
    /// The new skill's files as this build ships them: `(path inside the skill folder, text)`.
    new_files: &'static [(&'static str, &'static str)],
}

/// CDXC:AgentSkills 2026-11-20 DECISION:
/// User (Q4262 "B"): Ghostex's own skills are retired for CLI help, keeping one tiny skill named
/// `ghostex`, which is `ghostex-agent-guide` renamed. A machine with the old name gets `ghostex`
/// the way the coordinator rename did it. The coordinator skill itself is retired now (both its
/// names are in agent_skills.rs `RETIRED_GHOSTEX_AGENT_SKILL_NAMES`), so its rename is gone from
/// this list.
const RENAMED_SKILLS: &[RenamedSkill] = &[RenamedSkill {
    old_names: &["ghostex-agent-guide"],
    new: "ghostex",
    new_files: &[
        ("SKILL.md", include_str!("../../skills/ghostex/SKILL.md")),
        (
            "agents/openai.yaml",
            include_str!("../../skills/ghostex/agents/openai.yaml"),
        ),
    ],
}];

/// For each old name, the sha256 of every revision of its files Ghostex ever shipped (the
/// repository history at the rename, which covers the app bundles and every signed or main
/// download). A copy under an old name with no entry here counts as the user's and is kept.
/// To add a name: `git log --all --format=%H -- skills/<name>/<file>` and hash each revision's file.
const OLD_SKILL_SHIPPED_SHA256: &[(&str, &[&str])] =
    &[("ghostex-agent-guide", AGENT_GUIDE_SHIPPED_SHA256)];

fn shipped_sha256(old_name: &str) -> &'static [&'static str] {
    OLD_SKILL_SHIPPED_SHA256
        .iter()
        .find(|(name, _)| *name == old_name)
        .map_or(&[], |(_, hashes)| hashes)
}

/// `ghostex-agent-guide` SKILL.md and agents/openai.yaml, every revision in the repository.
const AGENT_GUIDE_SHIPPED_SHA256: &[&str] = &[
    // SKILL.md
    "29a01ff3e22ad40724b6d4bae3c441fb0d696aeae5791920cf299a9e649bdd33",
    "30f6f3d6a8c77db5674525307d5760defc0973804af06f0794fcc64f1e5a5653",
    "617875154f1cc034f7ad8b33d670e975e897ab92d52a91bf86b6e77a360fc90a",
    "8ca24e71c86d1017324e952eb823f99ae274759aff981bcb9f8d593d4f53204e",
    "b23e81113dad3179c39702dd3a528013960f6a2c084d55601806261c382540cf",
    // agents/openai.yaml
    "56105dabf7d64743559aca47db681402e3c36d9f15e637d0ffd70f1a4713eb61",
];

/// What one start did, for the log line.
#[derive(Default)]
struct Report {
    installed: Vec<String>,
    linked: Vec<String>,
    removed: Vec<String>,
    kept: Vec<Value>,
    failed: Vec<Value>,
}

impl Report {
    fn keep(&mut self, path: &Path, reason: &str) {
        self.kept
            .push(json!({ "path": path_string(path), "reason": reason }));
    }

    fn fail(&mut self, path: &Path, error: impl ToString) {
        self.failed
            .push(json!({ "path": path_string(path), "error": error.to_string() }));
    }
}

/// Startup migration: moves every install of a renamed bundled skill to its new name (see the
/// module note). Returns `None` when there was nothing named the old way, else what it did.
pub fn migrate_renamed_ghostex_agent_skills(paths: &GxserverPaths) -> Option<Value> {
    let home = paths.agent_config_home_dir();
    let mut report = Report::default();
    let mut found = false;
    for skill in RENAMED_SKILLS {
        found |= migrate_one(home, skill, &mut report);
    }
    found.then(|| {
        json!({
            "installed": report.installed,
            "linked": report.linked,
            "removed": report.removed,
            "kept": report.kept,
            "failed": report.failed,
        })
    })
}

/// What an entry named the old way is, and so what happens to it.
enum OldEntry {
    /// A copy Ghostex shipped, unchanged: replaced by the new skill.
    Shipped,
    /// A link Ghostex made in an agent's folder to the `~/.agents/skills` copy.
    LinkToCanonical,
    /// The user's: an edited copy, or a link to somewhere else. Never touched.
    Users(&'static str),
}

fn migrate_one(home: &Path, skill: &RenamedSkill, report: &mut Report) -> bool {
    let canonical_root = home.join(CANONICAL_AGENT_SKILLS_DIR);
    let canonical_new = canonical_root.join(skill.new);
    let roots = installed_skill_roots(home);
    let (canonical_root_ref, roots_ref) = (&canonical_root, &roots);
    // Every entry under an old name, in every skill folder: (old name, root, path, kind).
    let entries: Vec<(&str, PathBuf, PathBuf, OldEntry)> = skill
        .old_names
        .iter()
        .filter(|old| **old != skill.new)
        .flat_map(|old| {
            let canonical_old = canonical_root_ref.join(old);
            roots_ref.iter().filter_map(move |root| {
                let path = root.join(old);
                let in_canonical = root == canonical_root_ref;
                let kind = classify(&path, in_canonical, &canonical_old, shipped_sha256(old))?;
                Some((*old, root.clone(), path, kind))
            })
        })
        .collect();
    if entries.is_empty() {
        return false;
    }
    // 1. The new skill in `~/.agents/skills`, from this build, before anything old goes.
    if !canonical_new.join("SKILL.md").is_file() {
        if fs::symlink_metadata(&canonical_new).is_ok() {
            // Something of the user's holds the new name (a link that leads nowhere, a folder
            // without SKILL.md): leave everything as it is.
            report.fail(
                &canonical_new,
                "Something that is not the skill is already there, so the old skill was kept.",
            );
            return true;
        }
        match write_skill_folder(&canonical_new, skill.new_files) {
            Ok(()) => report.installed.push(path_string(&canonical_new)),
            Err(error) => {
                report.fail(&canonical_new, error);
                return true;
            }
        }
    }
    // Whether the `~/.agents/skills` copy under `old` goes (Ghostex's) or is already gone, so the
    // agent links to it are Ghostex's to move too.
    let canonical_old_goes = |old: &str| {
        entries.iter().any(|(name, root, _, kind)| {
            *name == old && *root == canonical_root && matches!(kind, OldEntry::Shipped)
        }) || fs::symlink_metadata(canonical_root.join(old)).is_err()
    };
    // 2. Every agent folder that had an old skill from Ghostex gets the new one the same way, and
    // only then loses the old entry.
    for (old, root, path, kind) in &entries {
        if *root == canonical_root {
            continue;
        }
        // A Ghostex link to a kept copy stays, but its agent still gets the new skill beside it.
        let keep_old = match kind {
            OldEntry::Users(reason) => {
                report.keep(path, reason);
                continue;
            }
            OldEntry::LinkToCanonical if !canonical_old_goes(old) => {
                report.keep(path, "It links to the kept copy in ~/.agents/skills.");
                true
            }
            _ => false,
        };
        let new_path = root.join(skill.new);
        if !reads_canonical(home, root) && fs::symlink_metadata(&new_path).is_err() {
            match link_skill_into_agent_root(&canonical_new, &new_path) {
                Ok(true) => report.linked.push(path_string(&new_path)),
                Ok(false) => {}
                Err(error) => {
                    report.fail(&new_path, error.message);
                    continue;
                }
            }
        }
        if keep_old {
            continue;
        }
        let removed = if matches!(kind, OldEntry::LinkToCanonical) {
            ghostex_agent_sync::fsx::remove_link(path)
        } else {
            fs::remove_dir_all(path)
        };
        match removed {
            Ok(()) => report.removed.push(path_string(path)),
            Err(error) => report.fail(path, error),
        }
    }
    // 3. Last, the old copies in `~/.agents/skills`.
    for (_, root, path, kind) in &entries {
        if *root != canonical_root {
            continue;
        }
        match kind {
            OldEntry::Shipped => match fs::remove_dir_all(path) {
                Ok(()) => report.removed.push(path_string(path)),
                Err(error) => report.fail(path, error),
            },
            OldEntry::Users(reason) => report.keep(path, reason),
            OldEntry::LinkToCanonical => {}
        }
    }
    true
}

/// What the entry at `path` is, or `None` when nothing is there.
fn classify(
    path: &Path,
    in_canonical_root: bool,
    canonical_old: &Path,
    shipped: &[&str],
) -> Option<OldEntry> {
    let metadata = fs::symlink_metadata(path).ok()?;
    Some(if metadata.file_type().is_symlink() {
        if !in_canonical_root && link_points_at(path, canonical_old) {
            OldEntry::LinkToCanonical
        } else {
            OldEntry::Users("It is a link of your own, so Ghostex left it.")
        }
    } else if metadata.is_dir() && is_unmodified_shipped_copy(path, shipped) {
        OldEntry::Shipped
    } else {
        OldEntry::Users("It was changed after Ghostex installed it, so Ghostex left it.")
    })
}

/// True when the link at `link` leads to `target`, read from the link itself so a link whose
/// target is already gone still counts.
fn link_points_at(link: &Path, target: &Path) -> bool {
    use ghostex_agent_sync::fsx;
    if fsx::same_entry(link, target) {
        return true;
    }
    let (Ok(destination), Some(parent)) = (fs::read_link(link), link.parent()) else {
        return false;
    };
    let real = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let resolved = fsx::normalize(&real(parent).join(destination));
    let wanted = match (target.parent(), target.file_name()) {
        (Some(root), Some(name)) => real(root).join(name),
        _ => target.to_path_buf(),
    };
    resolved == fsx::normalize(&wanted)
}

/// True when the folder holds only files Ghostex shipped for the old skill (SKILL.md and an
/// optional agents/openai.yaml), each byte for byte as one shipped revision.
fn is_unmodified_shipped_copy(dir: &Path, shipped: &[&str]) -> bool {
    use sha2::{Digest, Sha256};
    let matches = |path: &Path| {
        fs::read(path)
            .map(|bytes| shipped.contains(&hex::encode(Sha256::digest(&bytes)).as_str()))
            .unwrap_or(false)
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut saw_skill_md = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name == "SKILL.md" {
            if !matches(&entry.path()) {
                return false;
            }
            saw_skill_md = true;
        } else if name == "agents" {
            let Ok(agent_entries) = fs::read_dir(entry.path()) else {
                return false;
            };
            for agent_entry in agent_entries.flatten() {
                if agent_entry.file_name() != "openai.yaml" || !matches(&agent_entry.path()) {
                    return false;
                }
            }
        } else if name != ".DS_Store" {
            return false;
        }
    }
    saw_skill_md
}

/// True when `root` is the own folder of an agent that reads `~/.agents/skills` itself.
fn reads_canonical(home: &Path, root: &Path) -> bool {
    GHOSTEX_SKILLS_CLI_AGENT_IDS.iter().any(|agent_id| {
        agent_skill_global_dir(home, agent_id).is_some_and(|dir| dir == root)
            && agent_reads_canonical_skills(home, agent_id)
    })
}

/// Writes the skill's files into a folder beside `destination` and renames it into place, so a
/// failure half way never leaves a torn skill.
fn write_skill_folder(destination: &Path, files: &[(&str, &str)]) -> std::io::Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| std::io::Error::other("the skill folder has no parent"))?;
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let staging = parent.join(format!(".{name}.ghostex-rename-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let written = files.iter().try_for_each(|(relative, text)| {
        let path = staging.join(relative);
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(path, text)
    });
    let result = written.and_then(|()| fs::rename(&staging, destination));
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}
