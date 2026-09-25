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

/// CDXC:SessionChat 2026-09-05 DECISION:
/// User: add Split Right below Close After Done in the chat composer's More menu.
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
        id: "splitSessionRight",
        label: "Split Right",
        hotkey: Some("splitSessionRight"),
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

/// Whether an action id is one the app shell offers at all.
pub fn is_host_action(id: &str) -> bool {
    DEFINITIONS.iter().any(|definition| definition.id == id)
}
