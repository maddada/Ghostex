//! The agent skill an extension ships (`skill` in its manifest): checked when the payload is read,
//! installed when the user says yes on the install card (`approval.skill`), refreshed by an update
//! only when the user installed it, and removed with the extension.
//!
//! It goes where Ghostex's own skills go (agent_skills.rs `install_skill_folder`: one copy in
//! `~/.agents/skills`, linked into the agents that don't read that folder), with a marker file in
//! the copy naming the extension. A skill folder the extension did not put there (a Ghostex skill,
//! the user's own, another extension's) is never replaced or removed: the skill is left out and the
//! install answer says why.
//!
//! CDXC:Extensions 2026-11-14 SEE-ALSO: types.rs (`ExtensionSkillManifest`, the user's decision),
//! registry.rs (install, update and uninstall), apps/desktop/src/app/window/settings_modal/tabs/extensions/
//! (the install cards' question).

use std::{fs, path::Path};

use serde_json::{json, Value};

use super::{validate_relative_path, ExtensionError, ExtensionResult, ExtensionSkillManifest};
use crate::agent_skills::{
    read_frontmatter_name, CANONICAL_AGENT_SKILLS_DIR, GHOSTEX_AGENT_SKILL_NAMES,
    RETIRED_GHOSTEX_AGENT_SKILL_NAMES,
};

/// The file in the installed copy that says which extension put it there.
const OWNER_MARKER: &str = ".ghostex-extension";

pub(crate) fn validate_skill_manifest(skill: &ExtensionSkillManifest) -> ExtensionResult<()> {
    let name = skill.name.as_str();
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        return Err(ExtensionError::bad_request(format!(
            "skill.name {name:?} must be 1-64 lowercase letters, digits and dashes."
        )));
    }
    validate_relative_path(&skill.path, "skill.path")
}

/// The skill folder holds `SKILL.md`, whose frontmatter `name` is the manifest's.
pub(crate) fn validate_skill_payload(
    payload_dir: &Path,
    skill: &ExtensionSkillManifest,
) -> ExtensionResult<()> {
    let skill_file = payload_dir.join(&skill.path).join("SKILL.md");
    let markdown = fs::read_to_string(&skill_file).map_err(|_| {
        ExtensionError::bad_request(format!("skill.path {:?} has no SKILL.md.", skill.path))
    })?;
    match read_frontmatter_name(&markdown) {
        Some(name) if name == skill.name => Ok(()),
        Some(name) => Err(ExtensionError::bad_request(format!(
            "SKILL.md is named {name:?}, but the manifest's skill.name is {:?}.",
            skill.name
        ))),
        None => Err(ExtensionError::bad_request(
            "SKILL.md needs a `name:` in its frontmatter.",
        )),
    }
}

fn owned_by(skill_dir: &Path, extension_id: &str) -> bool {
    let is_real_dir = fs::symlink_metadata(skill_dir)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
    is_real_dir
        && fs::read_to_string(skill_dir.join(OWNER_MARKER))
            .is_ok_and(|owner| owner.trim() == extension_id)
}

/// Why the skill can't go in without replacing someone else's, or `None` when it can.
fn clash(home: &Path, extension_id: &str, name: &str) -> Option<String> {
    if GHOSTEX_AGENT_SKILL_NAMES.contains(&name)
        || RETIRED_GHOSTEX_AGENT_SKILL_NAMES.contains(&name)
    {
        return Some(format!("Ghostex has its own skill named {name}."));
    }
    let canonical = home.join(CANONICAL_AGENT_SKILLS_DIR).join(name);
    let ours = owned_by(&canonical, extension_id);
    if fs::symlink_metadata(&canonical).is_ok() && !ours {
        return Some(format!("Your agents already have a skill named {name}."));
    }
    // An agent's own folder may hold a skill of that name too; only a link to our copy may stay.
    let canonical_real = fs::canonicalize(&canonical).ok();
    for root in crate::agent_skills_remote::installed_skill_roots(home) {
        let entry = root.join(name);
        if entry == canonical || fs::symlink_metadata(&entry).is_err() {
            continue;
        }
        let points_at_ours =
            ours && canonical_real.is_some() && fs::canonicalize(&entry).ok() == canonical_real;
        if !points_at_ours {
            return Some(format!("Your agents already have a skill named {name}."));
        }
    }
    None
}

/// Installs (or refreshes) the extension's skill. `Err` is the plain reason it was left out.
pub(crate) fn install_skill(
    home: &Path,
    extension_id: &str,
    payload_dir: &Path,
    skill: &ExtensionSkillManifest,
) -> Result<(), String> {
    if let Some(reason) = clash(home, extension_id, &skill.name) {
        return Err(reason);
    }
    let source = payload_dir.join(&skill.path);
    crate::agent_skills::install_skill_folder(home, &source, &skill.name)
        .map_err(|error| error.message)?;
    let canonical = home.join(CANONICAL_AGENT_SKILLS_DIR).join(&skill.name);
    fs::write(canonical.join(OWNER_MARKER), extension_id)
        .map_err(|error| format!("Couldn't finish installing the skill: {error}"))
}

/// Removes the extension's skill when the copy is still the one it installed.
pub(crate) fn remove_skill(home: &Path, extension_id: &str, name: &str) {
    let canonical = home.join(CANONICAL_AGENT_SKILLS_DIR).join(name);
    if owned_by(&canonical, extension_id) {
        let _ = crate::machine_integrations::uninstall_skill(home, name);
    }
}

/// What an install did with the skill, for the install answer (`skill` beside `extension`).
pub(crate) fn skill_report(name: &str, outcome: &Result<bool, String>) -> Value {
    match outcome {
        Ok(installed) => json!({ "name": name, "installed": installed }),
        Err(reason) => json!({ "name": name, "installed": false, "reason": reason }),
    }
}
