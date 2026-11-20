//! The GBot playbook as a skill, for a chat another computer opens here whose first message still
//! names `ghostex-gbots` (a computer whose Ghostex predates `ghostex gbots guide`).
//!
//! CDXC:GBots 2026-11-09 WHY:
//! Every GBot chat starts with "Read and adhere to this skill: $ghostex-gbots" (session.rs
//! `first_message`), but only the orchestrator playbook was installed for it, so the chat looked
//! for a skill that was not there. The playbook is now installed the way the orchestrator's is
//! (coordinators/role.rs `ensure_bundled_skill_installed`): when a GBot chat starts on this
//! computer (session.rs), and when another computer's GBot opens a chat on this one
//! (computer/remote_chat.rs marks it `gbotHome`; the `/api/createAgentSession` route calls
//! [`install_for_visiting_chat`]), since that computer's agent reads its own skills folder. A chat
//! on a Cloud Box needs none: its first message carries the whole brief (computer/cloud_box.rs).
//!
//! CDXC:AgentSkills 2026-11-20 DECISION:
//! User (Q4262 "B", "make sure we don't break stuff"): Ghostex's own skills are retired for CLI help, so a GBot chat started here reads its playbook with `ghostex gbots guide` and nothing installs the skill for it. A chat that an older home computer opens here still says "$ghostex-gbots", so a visiting chat keeps getting the skill, written from this build's playbook (gbots/playbook.md) with the skill's front matter; the startup pass removes it again with the other retired skills.
//! SEE-ALSO: server/src/agent_skills.rs (`RETIRED_GHOSTEX_AGENT_SKILL_NAMES`).

use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

use super::session::GBOTS_SKILL;

/// The `runtimeSettings` key a GBot chat opened from another computer carries
/// (computer/remote_chat.rs `HOME_KEY`).
const HOME_KEY: &str = "gbotHome";

/// The skill's front matter before the body's sha256: manual-only, like every Ghostex skill
/// (2026-11-17, Q1 "C").
const SKILL_FRONT_MATTER_HEAD: &str = "---
name: ghostex-gbots
description: >-
  The GBot playbook. Use it only in a GBot's own chat in Ghostex, when your
  first message names this skill and says who you are.
disable-model-invocation: true
";
/// The front matter line that carries the body's sha256, so any later Ghostex knows an unedited
/// copy is its own whatever playbook it was written from.
const BODY_SHA256_LINE: &str = "# ghostex-body-sha256: ";
const SKILL_OPENAI_YAML: &str = "interface:
  display_name: 'Ghostex GBot'
  short_description: 'How a GBot works in Ghostex: its brief, memory, team and reports'
  default_prompt: 'Use $ghostex-gbots to work as this GBot: run ghostex gbots brief first and follow it.'

policy:
  allow_implicit_invocation: false
";

fn sha256_hex(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// SKILL.md as this build writes it: the front matter, the body's sha256, then the playbook.
fn skill_md() -> String {
    let body = super::GBOT_PLAYBOOK_MD;
    format!(
        "{SKILL_FRONT_MATTER_HEAD}{BODY_SHA256_LINE}{}\n---\n\n{body}",
        sha256_hex(body)
    )
}

/// Whether `text` is a SKILL.md some Ghostex wrote here for a visiting chat and nobody edited:
/// its front matter is this one and its body still has the sha256 the front matter names.
///
/// CDXC:AgentSkills 2026-11-20 WHY:
/// Opus review, finding 4: the copy used to match only this build's playbook, so once
/// gbots/playbook.md changed, a later build neither refreshed it nor removed it with the retired
/// skills. The sha256 line lets any build recognise its own unedited copy.
pub(crate) fn is_unedited_gbots_skill_md(text: &str) -> bool {
    let Some(rest) = text.strip_prefix(SKILL_FRONT_MATTER_HEAD) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(BODY_SHA256_LINE) else {
        return false;
    };
    let Some((sha, body)) = rest.split_once("\n---\n\n") else {
        return false;
    };
    sha == sha256_hex(body)
}

/// Whether `text` is the openai.yaml written beside it.
pub(crate) fn is_gbots_skill_openai_yaml(text: &str) -> bool {
    text == SKILL_OPENAI_YAML
}

/// Installs the GBot playbook as a skill for `family`: written when missing, and written again
/// when the copy there is an older one Ghostex wrote or shipped and nobody edited.
fn ensure_gbots_skill_installed(
    paths: &GxserverPaths,
    family: &str,
) -> Result<PathBuf, DomainStateError> {
    let current = skill_md();
    let skill_file = paths
        .agent_config_home_dir()
        .join(crate::agent_skills::CANONICAL_AGENT_SKILLS_DIR)
        .join(GBOTS_SKILL)
        .join("SKILL.md");
    if let Ok(existing) = std::fs::read_to_string(&skill_file) {
        let ours = is_unedited_gbots_skill_md(&existing)
            || crate::agent_skills::matches_shipped_retired_skill_file(&skill_file);
        if ours && existing != current {
            let _ = std::fs::remove_file(&skill_file);
        }
    }
    crate::coordinators::ensure_bundled_skill_installed(
        paths,
        family,
        GBOTS_SKILL,
        &current,
        SKILL_OPENAI_YAML,
    )
}

/// For a session another computer's GBot just created here (`runtimeSettings.gbotHome`), installs
/// the playbook its first message names. Anything else is left alone; a failed install is reported
/// by the chat, not by the create.
pub(crate) fn install_for_visiting_chat(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
    project: &Value,
    session: &Value,
) {
    let visiting = params
        .get("runtimeSettings")
        .and_then(|runtime| runtime.get(HOME_KEY))
        .is_some_and(Value::is_object);
    if !visiting {
        return;
    }
    if let Some(family) = crate::agents::session_agent_family_id(project, session) {
        let _ = ensure_gbots_skill_installed(paths, &family);
    }
}
