//! The five quick-picker agents' names and icons.
//!
//! `packages/shared/session-chat-controller/native-model-picker.ts` and
//! `session-chat-presentation/model-menu.ts` both call `getDefaultSidebarAgentById(provider)` and
//! read only `name` and `icon`. The whole default agent list belongs to the sidebar, not to chat,
//! so only those two fields for the five providers a picker can open are here.
//!
//! SEE-ALSO: `packages/shared/sidebar-agents.ts` (`DEFAULT_SIDEBAR_AGENTS`),
//! `apps/desktop/src/app/helpers/sidebar/sidebar_defaults_types.rs`.

use crate::menus::picker::model_picker::ModelPickerProvider;

/// What one of the five agents is called and which logo it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickerAgent {
    pub name: &'static str,
    pub icon: &'static str,
}

/// `getDefaultSidebarAgentById(provider)`, narrowed to the fields chat reads.
pub fn picker_agent(provider: ModelPickerProvider) -> PickerAgent {
    match provider {
        ModelPickerProvider::OpenCode => PickerAgent { name:"OpenCode", icon:"opencode" },
        ModelPickerProvider::Codex => PickerAgent {
            name: "Codex",
            icon: "codex",
        },
        ModelPickerProvider::Claude => PickerAgent {
            name: "Claude",
            icon: "claude",
        },
        ModelPickerProvider::Cursor => PickerAgent {
            name: "Cursor CLI",
            icon: "cursor-cli",
        },
        ModelPickerProvider::Grok => PickerAgent {
            name: "Grok Build",
            icon: "grok-build",
        },
        ModelPickerProvider::Antigravity => PickerAgent {
            name: "Antigravity CLI",
            icon: "antigravity-cli",
        },
    }
}
