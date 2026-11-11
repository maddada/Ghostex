//! Bundled skills that were renamed: on the first start after the update that renamed one, a
//! machine that has the old name installed gets the new name installed the same way and the old
//! one removed, without the user doing anything.
//!
//! CDXC:Coordinators 2026-11-11 DECISION:
//! User: "please make /ghostex-orchestrator renamed to /ghostex-coordinator everywhere from next update and replace the skill with the new name". The product already called the role a coordinator (the CLI is `ghostex coordinator`, the app says Coordinator); only the skill kept the old word. So the skill is `ghostex-coordinator` and gxserver stops knowing `ghostex-orchestrator` (no install, no refresh). At startup, before the refresh from the signed skill list, [`migrate_renamed_ghostex_agent_skills`] moves each install over: the new skill is written to `~/.agents/skills` from the app bundle when it is missing, every agent folder that had the old skill (a link Ghostex made, or a copy older builds made) gets a link to the new one (none for an agent that reads `~/.agents/skills` itself), and only then are the old entries Ghostex made removed, so no coordinator is ever left without a playbook. An old copy is Ghostex's when every file in it byte-matches a revision Ghostex shipped (the retired skills' rule, agent_skills.rs `remove_retired_ghostex_agent_skills`). Anything else named `ghostex-orchestrator` is the user's and is never deleted: an edited copy, the user's own link (to their checkout, say) and the agent links that point at a kept copy stay where they are, are reported as kept in the `renamedAgentSkillsMoved` log line, and are left out of Ghostex's installs and refreshes from then on. Coordinators started before the rename named `/ghostex-orchestrator` in their first message; they keep their playbook through `ghostex coordinator guide`, which prints it from the app (coordinators/role.rs `coordinator_guide_for` says the skill's new name), and the changed guide gets them the "playbook changed" note with their next message (coordinators/playbook.rs `playbook_update_note`).
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

const RENAMED_SKILLS: &[RenamedSkill] = &[RenamedSkill {
    old_names: crate::coordinators::COORDINATOR_SKILL_OLD_NAMES,
    new: crate::coordinators::COORDINATOR_SKILL_NAME,
    new_files: &[
        ("SKILL.md", crate::coordinators::COORDINATOR_SKILL_MD),
        (
            "agents/openai.yaml",
            crate::coordinators::COORDINATOR_SKILL_OPENAI_YAML,
        ),
    ],
}];

/// For each old name, the sha256 of every revision of its files Ghostex ever shipped (the
/// repository history at the rename, which covers the app bundles and every signed or main
/// download). A copy under an old name with no entry here counts as the user's and is kept.
/// To add a name: `git log --all --format=%H -- skills/<name>/<file>` and hash each revision's file.
const OLD_SKILL_SHIPPED_SHA256: &[(&str, &[&str])] =
    &[("ghostex-orchestrator", ORCHESTRATOR_SHIPPED_SHA256)];

fn shipped_sha256(old_name: &str) -> &'static [&'static str] {
    OLD_SKILL_SHIPPED_SHA256
        .iter()
        .find(|(name, _)| *name == old_name)
        .map_or(&[], |(_, hashes)| hashes)
}

/// `ghostex-orchestrator` SKILL.md and agents/openai.yaml, every revision in the repository.
const ORCHESTRATOR_SHIPPED_SHA256: &[&str] = &[
    // SKILL.md
    "076e970ca97a0e095b8e40d9d5ef41cfce1dd736b1292bd692d14b87dd998b39",
    "078af2c0157cf5c734aca3a3ace62a341e5634e24d40f035289679d951457c9a",
    "0bc3ab186a027e5227270ddab1b75a69e7dcb1aa6a45d70f979695905cb6568e",
    "0c882a7121cf90e7822686d58f4bdbca749af5b5c9204c30023d83247a13c52f",
    "10ae8295fe7cd7c9b5b5b789cf70636ced9d7722487ed4acfd0a1336bca6f057",
    "119e4a9ba673a428f42bcf330b70639e502f41e550fdd0743b6691956502eb40",
    "19fc10b6e57756ab9a522064116627bc8e85b73d7395700013dfa66f1d31a19c",
    "1f8adefac0b8221fe36bb234015d55742a0525540f56bc36a52fa40e15868be0",
    "20fc6028952f94702f87b67e23f3e0d17734c06fa243939835f0c9918be94f1d",
    "2456a7141af0ffe905aed1716a12b3f9316d4fb0223fa04cfea988583bcf43a3",
    "374da00e358d070928e9f0689f17c93ec17b78d7b29779b8b4f5d400f6957112",
    "3ea0ffcf02ba61cd6c7cde37536bac1111bdd4c9598607826912eaab19a0aa7d",
    "3fa5a60e9247571b02dc16841086697877e197fb34ab32cb0d32017d481877a5",
    "4230be6a17cc81bc629e40166cd00ab5cd4c1dc7750402bc7e4c04276bece1fa",
    "4274f9f878e7f800436d2b1329ac990ab210e21366526519d131ada33e7187b4",
    "4a78e1e0ce2d29a44c739a609d5b6e0085a8fa65ce27f309f371f6de001c617c",
    "522de3699297777f08a3a3ef544f0b764ece9647f1a031f8b448cb016321c9a8",
    "5a5c9505f0524e2d7b9f922270f9a440cdedaebac8b0e9d9440dce79bce1de67",
    "6636af6c5d16f492088ae5ff3c71032349fcab1c3159d8d70e9a76ca4673cd95",
    "6907e220ff052bc285094d61224e0fdc33d99fb88e16994131fa990eedaffc44",
    "7116e1d99d332f4ce53d3be4af87671360d5b65fc31ece5f686754238f89bf89",
    "71b60fedb5ab6b4f7a48c36cd982d5fbde9b8f1087804c8d580033aa4dd85c7e",
    "8b5966bbba0d4e4a53c03a0966c6edc14f2e6cc07dffad0b077c691379468271",
    "92a9950828d35d07cc6a2f185c8a80a4c06a9a9ede8e99efe2e62007756d4878",
    "976506de92bf272e01992a8c5c10a31c9b07ef1bcb86995a44c1660a2775e275",
    "999366ae30ced11e0e7ccf100734926b17492c307645d0093d8c10d596979966",
    "a15305ed73f880b05ea70899bec593709f2c2a5d4ebf969d4a73cbe83e0e059b",
    "a53fb829b92abe8cad0dfa02439a090c60ae87c19efdb20c89be89e21d931482",
    "a89b0557f2ead9761c12568858e4c57f27a8e8e1178b81854b239386c39007f6",
    "ae21d2f7f83b4366a3ec94b93089fca40a89fec47b4a84759ab0da1ee63c8612",
    "b0cc1df8d758b4dfdee3ee10c05f0a1ac4681169e4a6337f30065fa241cea372",
    "be8ed6575c22eb7d7676ad4209a267ef02e8ec7906001c0c0e0ab6b6f7459774",
    "c19401bdaa6a3dd34ae593ef11a145f4b6a5d85858abaf65d4aed4b0c132a792",
    "ca1fa662e509639422d7b6247a93faf80f4b21c521fc87d76e2f0fb04800a870",
    "ceabf993fc30d358ef44d9e5f3d30f7ba85fc289dfe7c55120fc929848fd36c2",
    "cfbf36a59f49a9ab5b15e3666311f1aaf29fa224f2cd5de0a0b2fcbb922c45bc",
    "d4f060fd70a467b3482589aff33e09e66bc1a5ab3339b6c30c1b9c1bc6f9dd8d",
    "d5d60d7a5902c381978345e0af2a38e9208d8afdfdf33fe9deaf15b3c0c12424",
    "d7320f297aed811ff972cee86beefe781376192b154e84a3e3b8be0db52a0d3c",
    "dc512c998c3a3da11dfb2ce0b7978d636e514585ca4793399e79f8938856c0e6",
    "eddf33bbd0b1af56be052048ba070f7f8efee29677c73a5926dfa4313afc85cf",
    "eecb57226afe8aef8a9734c385e3f0c2bb51c0723994c89a3f2235c8cb4b7856",
    "ef7258981b9a5ae0945d333b63de7af2ba34cc6f75cc2997735ff469d3fe3688",
    "f019e6b24967eaa1a7c548d1af5b2e74badf9f75b5555c47133fff4d9e4710f0",
    "f1188ed34b108ad334dfbd79ad5f04d67633014e0340faa38320ad56285c41ab",
    "fdf53e3a08b142a37c9ac2db110fe1036482438842915b8eaa2fff225d1f5803",
    // agents/openai.yaml
    "d892999cf5e0f577d2213c25b8f5cc2c4fa52eb1e4c14f116e110d98a498e6ef",
    "e54d93e7414e18e5d22edb0205b20a77c18d8e3af8ecd7adae988b2fcc238b0c",
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
        match kind {
            OldEntry::Users(reason) => {
                report.keep(path, reason);
                continue;
            }
            OldEntry::LinkToCanonical if !canonical_old_goes(old) => {
                report.keep(path, "It links to the kept copy in ~/.agents/skills.");
                continue;
            }
            _ => {}
        }
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
