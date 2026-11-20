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

/// The skill's front matter: manual-only, like every Ghostex skill (2026-11-17, Q1 "C").
const SKILL_FRONT_MATTER: &str = "---
name: ghostex-gbots
description: >-
  The GBot playbook. Use it only in a GBot's own chat in Ghostex, when your
  first message names this skill and says who you are.
disable-model-invocation: true
---

";
const SKILL_OPENAI_YAML: &str = "interface:
  display_name: 'Ghostex GBot'
  short_description: 'How a GBot works in Ghostex: its brief, memory, team and reports'
  default_prompt: 'Use $ghostex-gbots to work as this GBot: run ghostex gbots brief first and follow it.'

policy:
  allow_implicit_invocation: false
";

/// The skill's files as [`ensure_gbots_skill_installed`] writes them, so the startup pass for
/// retired skills knows such a copy is Ghostex's (agent_skills.rs).
pub(crate) fn retired_gbots_skill_files_written_by_this_build() -> [String; 2] {
    [
        format!("{SKILL_FRONT_MATTER}{}", super::GBOT_PLAYBOOK_MD),
        SKILL_OPENAI_YAML.to_string(),
    ]
}

/// Installs the GBot playbook as a skill for `family` when it is missing.
fn ensure_gbots_skill_installed(
    paths: &GxserverPaths,
    family: &str,
) -> Result<PathBuf, DomainStateError> {
    let [skill_md, _] = retired_gbots_skill_files_written_by_this_build();
    crate::coordinators::ensure_bundled_skill_installed(
        paths,
        family,
        GBOTS_SKILL,
        &skill_md,
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
