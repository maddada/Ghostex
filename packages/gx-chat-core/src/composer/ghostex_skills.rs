//! The More actions menu's Skills submenu: the Ghostex skills this session's agent has installed.
//!
//! CDXC:AgentSkills 2026-10-10 DECISION:
//! User: a "Skills" item, second from the bottom of the chat's and the terminal's ⋯ menus, opens a submenu listing Ghostex's own bundled skills and ends in "Configure / Install more"; the point is to tell users which skills Ghostex has. In the chat a row puts the skill into the chat box at the caret without sending; in the terminal it types the skill into the agent's input without pressing Enter. The phone's chat and terminal menus carry the same item. The list is the skills gxserver marks as Ghostex's in the session's skill read, never a copy kept in a client.
//! SEE-ALSO: server/src/session_chat_skills.rs, apps/desktop/src/app/native_chat/actions.rs, apps/desktop/src/app/render/terminal_agent_action_bar/skills_flyout.rs, apps/mobile/app/src/chat/native/composer/menus.ts, apps/mobile/app/src/terminal/.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::action::UserAction;
use crate::composer::references::insert_reference;
use crate::composer::trigger::{linked_skill_mention, Skill};
use crate::effect::Effect;
use crate::state::ChatState;

/// One row of the Skills submenu.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostexSkill {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What the agent's own input runs it with (`/name` for Claude, `$name` otherwise): what a
    /// terminal menu types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<String>,
}

/// The Ghostex skills among a session's skills (`readSessionChatSkills`), by name, each once. The
/// desktop terminal bar's flyout reads its rows through this too.
pub fn ghostex_skill_rows(skills: &[Skill]) -> Vec<GhostexSkill> {
    let mut rows: Vec<GhostexSkill> = Vec::new();
    for skill in skills.iter().filter(|skill| skill.ghostex) {
        if rows.iter().all(|row| row.name != skill.name) {
            rows.push(GhostexSkill {
                name: skill.name.clone(),
                description: skill.description.clone(),
                invocation: skill.invocation.clone(),
            });
        }
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

/// The installed Ghostex skills the chat's More actions menu lists.
pub fn ghostex_skills(state: &ChatState) -> Vec<GhostexSkill> {
    ghostex_skill_rows(state.composer.skills())
}

/// `insertSkill`: the skill's pill at the caret (`text`, `start`, `end` from the view), spelled
/// the way the `$` picker writes it for this agent. Nothing is sent.
pub fn insert_skill(state: &ChatState, action: &UserAction) -> Vec<Effect> {
    let param = |name: &str| action.param(name);
    let name = param("name").and_then(Value::as_str).unwrap_or_default();
    let Some(skill) = state
        .composer
        .skills()
        .iter()
        .find(|skill| skill.ghostex && skill.name == name)
    else {
        return Vec::new();
    };
    let text = param("text").and_then(Value::as_str).unwrap_or_default();
    let offset = |name: &str| param(name).and_then(Value::as_u64).unwrap_or_default() as usize;
    let mention = linked_skill_mention(skill, state.session.agent.as_deref());
    let edit = insert_reference(text, &mention, offset("start"), offset("end"));
    vec![Effect::SetComposerText {
        content: edit.text,
        caret: Some(edit.caret),
        from_history: false,
    }]
}
