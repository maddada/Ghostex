//! The GBot playbook (`skills/ghostex-gbots/`), installed for the agent a GBot chat runs on.
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
//! SEE-ALSO: server/src/agent_skills.rs (`GBOT_ONLY_GHOSTEX_AGENT_SKILL_NAMES`).

use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

use super::session::GBOTS_SKILL;

/// The `runtimeSettings` key a GBot chat opened from another computer carries
/// (computer/remote_chat.rs `HOME_KEY`).
const HOME_KEY: &str = "gbotHome";

const SKILL_MD: &str = include_str!("../../../skills/ghostex-gbots/SKILL.md");
const SKILL_OPENAI_YAML: &str = include_str!("../../../skills/ghostex-gbots/agents/openai.yaml");

/// Installs the GBot playbook for `family` when it is missing.
pub(crate) fn ensure_gbots_skill_installed(
    paths: &GxserverPaths,
    family: &str,
) -> Result<PathBuf, DomainStateError> {
    crate::coordinators::ensure_bundled_skill_installed(
        paths,
        family,
        GBOTS_SKILL,
        SKILL_MD,
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
