//! The composer menu's host actions: things only the app shell can do.
//!
//! Port of `packages/shared/session-chat-presentation/actions.ts`. `hotkey` is the Ghostex hotkey
//! setting's id, not a resolved chord: the renderer looks the chord up itself, which is why the
//! document can be built without a hotkey catalog.

use ghostex_gx_protocol::Tri;

use crate::document::HostAction;

/// One offered action, in menu order.
struct Definition {
    id: &'static str,
    label: &'static str,
    hotkey: Option<&'static str>,
}

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: remove Split Right from the chat's More actions menu (and from the sidebar session menu), superseding the 2026-09-05 request that added it below Close After Done. Option+Shift+D still splits the focused session right.
/// The renderers list the session rows in this order.
const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "rename",
        label: "Rename",
        hotkey: Some("renameActiveSession"),
    },
    Definition {
        id: "sleep",
        label: "Sleep",
        hotkey: Some("sleepFocusedSession"),
    },
    Definition {
        id: "delayedActions",
        label: "Delayed actions",
        hotkey: Some("delayedSend"),
    },
    Definition {
        id: "closeAfterDone",
        label: "Close After Done",
        hotkey: Some("closeAfterDone"),
    },
    Definition {
        id: "fork",
        label: "Fork Session",
        hotkey: Some("forkSession"),
    },
    Definition {
        id: "fullReload",
        label: "Full Reload",
        hotkey: Some("reloadSession"),
    },
    Definition {
        id: "switchAccount",
        label: "Switch Account",
        hotkey: None,
    },
    Definition {
        id: "promptEditor",
        label: "Prompt editor",
        hotkey: Some("promptEditor"),
    },
    Definition {
        id: "stashPrompt",
        label: "Stash prompt",
        hotkey: Some("stashPrompt"),
    },
    Definition {
        id: "stashedPrompts",
        label: "Saved prompts",
        hotkey: Some("stashedPrompts"),
    },
    Definition {
        id: "attachPath",
        label: "Attach a file or folder",
        hotkey: Some("attachFileOrFolder"),
    },
    Definition {
        id: "exportTranscript",
        label: "Handoff / Export",
        hotkey: Some("exportTranscript"),
    },
    // CDXC:Coordinators 2026-11-09 WHY: the user: "i still don't know how a user can see the
    // coordinator playbook". A coordinator's chat offers View playbook in its More actions menu,
    // opening the same read-only page as the coordinator's sidebar menu row; only coordinators get
    // it (composer/document.rs).
    Definition {
        id: "viewPlaybook",
        label: "View playbook",
        hotkey: None,
    },
];

/// Actions the composer's own toolbar already offers, so the menu does not repeat them.
const COMPOSER_MENU_EXCLUDED: &[&str] = &[
    "attachPath",
    "promptEditor",
    "stashPrompt",
    "stashedPrompts",
];

/// Actions that act on the agent rather than on the session row.
const AGENT_ACTIONS: &[&str] = &["fork", "fullReload", "rename", "sleep", "switchAccount"];

/// The rows the document's `hostActions` carries.
pub fn composer_host_actions() -> Vec<HostAction> {
    DEFINITIONS
        .iter()
        .filter(|definition| !COMPOSER_MENU_EXCLUDED.contains(&definition.id))
        .map(|definition| HostAction {
            id: definition.id.to_string(),
            label: definition.label.to_string(),
            hotkey: match definition.hotkey {
                Some(hotkey) => Tri::Value(hotkey.to_string()),
                None => Tri::Absent,
            },
            group: if AGENT_ACTIONS.contains(&definition.id) {
                "agent".to_string()
            } else {
                "session".to_string()
            },
            extra: Default::default(),
        })
        .collect()
}

/// CDXC:SessionFork 2026-10-17 WHY:
/// Fork Session is offered only for the agents gxserver can fork (`build_agent_fork_plan` in server/src/agents/fork.rs, the sidebar's `supports_fork` in packages/gx-core/src/sidebar_menu/capabilities.rs). OMP has no fork flag, only its own in-session `/fork`, so the chat offered Fork and gxserver refused it. A session whose agent is not known yet keeps the row.
pub fn session_supports_fork(state: &crate::state::ChatState) -> bool {
    crate::composer::side_chat::agent_family(state).is_none_or(|agent| {
        matches!(
            agent.as_str(),
            "codex" | "claude" | "pi" | "empryo" | "copilot" | "grok"
        )
    })
}

/// Whether an action id is one the app shell offers at all.
pub fn is_host_action(id: &str) -> bool {
    DEFINITIONS.iter().any(|definition| definition.id == id)
}
