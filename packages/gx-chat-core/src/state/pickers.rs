//! Family e2's state: the model picker, the model menu, model selection, model favorites, the
//! fork branch family, and the context meter, editor and details.
//!
//! **This file belongs to family e2.** Family e1 owns `state/menus.rs` and the rest of
//! `src/menus/`; e2 owns `src/menus/picker/` and `src/menus/context/` and this struct. No other
//! family edits it.
//!
//! Read from, never write to: `ChatState::session::selected_options` (family a merges it by
//! evidence priority and `detectedAt`), `ChatState::session::pending_model_selection`,
//! `ChatState::session::available_agents`, `ChatState::session::agent_session_id`,
//! `ChatState::core::title` and `ChatState::core::hide_account_emails`.

use serde_json::Value;

use crate::menus::catalog::AgentModelCatalog;
use crate::menus::context::preferences::default_preferences;
use crate::menus::context::{ContextDetailsAgent, ContextDetailsPreferences, ContextEditorState};
use crate::menus::picker::model_menu::ModelMenuCatalogs;
use crate::menus::picker::projection::model_menu_projection;
use crate::menus::picker::{
    ForkBranch, ModelMenuContext, ModelMenuRow, ModelMenuView, ModelSelectionIntent,
    ModelSelectionState,
};

/// What family e2's surfaces remember between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickersState {
    /// The model menu's inputs, or `None` for an agent outside the model catalog.
    ///
    /// Family e1's `computeNativeChatOptions` fills this from the session option catalog; e2
    /// reads it, publishes it and picks against it.
    pub model_menu_context: Option<ModelMenuContext>,
    /// Every provider's model lineup, as e1's session option catalog answers it.
    pub catalogs: std::sync::Arc<ModelMenuCatalogs>,
    /// What [`Self::catalogs`] was built from (`crate::menus::picker::inputs::catalogs_key`), so
    /// the document reuses it instead of sweeping every provider's catalog again.
    pub catalogs_key: Option<(u64, bool)>,
    /// The merged model pill's menu: which tab is open and what was typed into its search.
    ///
    /// `tab` of `None` is "not opened yet", which is what makes the opening tab the session's own
    /// agent (`modelMenuOpeningTab`). The renderer resets it by sending `modelMenuView` with a
    /// null tab.
    pub model_menu_view: ModelMenuView,
    /// The starred models, the one list the person owns rather than a session or a renderer.
    ///
    /// Stored as the raw `provider:value` keys, which is the record format
    /// `ghostex.model-favorites` has on disk; it must not change.
    pub model_favorites: Vec<String>,
    /// Whether the favorites list has been read back from storage at least once.
    pub model_favorites_loaded: bool,
    /// The model selection outbox and what it is waiting for.
    pub model_selection: ModelSelectionState,
    /// `seed.modelOutboxes`: the stored intent per SCOPED option key, as the boot read handed it
    /// over and as every write since has updated it.
    pub model_outboxes: serde_json::Value,
    /// The scoped key the outbox above was last read for, so a draft session that latches its
    /// agent re-reads rather than keeping the plain key's record.
    pub model_outbox_key: Option<String>,
    /// The delivery in flight: the request id and the intent id it carries.
    ///
    /// `computeModelSelectionOutbox`'s delivery `useEffect` keeps one operation per session key in
    /// a module-level map and refuses to start a second; this is that map's one entry.
    pub model_selection_request: Option<(u64, String)>,
    /// The branch family this conversation belongs to, read once per chat.
    pub fork_branches: ForkBranchesState,
    /// The context meter, its editor and its status line.
    pub context: ContextState,
    /// The held-messages card and what it was built from (`menus/picker/hold_card.rs`).
    pub model_hold: Option<crate::menus::picker::hold_card::ModelHoldMemo>,
    /// An OpenCode model picked in a draft running another agent, waiting for the switch to land.
    pub draft_handoff_model: Option<DraftHandoffModel>,
}

/// A model the draft's next agent starts on, queued once the chat reads the session as that
/// agent (`crate::menus::picker::settle`).
#[derive(Clone, Debug, PartialEq)]
pub struct DraftHandoffModel {
    pub provider: crate::menus::picker::model_picker::ModelPickerProvider,
    pub selection: crate::menus::picker::model_picker::ModelPickerSelection,
    /// Past this the switch is taken to have failed and the pick is dropped.
    pub until_ms: i64,
}

impl PickersState {
    /// The outbox entry, else the pending selection gxserver reports, which is what the picker
    /// opens on and what `modelSelectionUnchanged` compares against (`model-selection.ts`).
    pub fn desired_selection(&self) -> Option<&ModelSelectionIntent> {
        self.model_selection.desired()
    }

    /// `modelMenuProjection(context, modelMenuView)`. The agent model catalog is family e1's
    /// (`ChatState::menus::model_catalog`), so the caller hands it over.
    pub fn model_menu_projection(
        &self,
        menu: &ModelMenuContext,
        catalog: &AgentModelCatalog,
    ) -> Value {
        model_menu_projection(
            menu,
            &self.model_menu_view,
            &self.catalogs,
            &self.model_favorites,
            catalog,
        )
    }

    /// The rows the open menu shows, which is what a `modelMenuPick` looks its key up in.
    pub fn model_menu_rows(&self, menu: &ModelMenuContext) -> Vec<ModelMenuRow> {
        crate::menus::picker::projection::model_menu_visible_rows(
            menu,
            &self.model_menu_view,
            &self.catalogs,
            &self.model_favorites,
        )
    }
}

/// The branch switcher's memory.
///
/// CDXC:SessionFork 2026-09-18 WHY:
/// The family is read ONCE per chat and kept: it only changes when a session is forked or
/// retired, both of which land the user on a different session and therefore a different chat
/// state. A daemon that predates `/api/sessionForkBranches`, or a call that failed, leaves the
/// strip unrendered rather than showing an empty menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForkBranchesState {
    /// The read has been issued; it is never issued again.
    pub asked: bool,
    /// The id of the one read, while it is in flight.
    pub request_id: Option<u64>,
    /// The daemon's own order (newest activity first). An empty answer is never adopted.
    pub branches: Vec<ForkBranch>,
}

/// The context meter and the row editor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextState {
    /// The saved row preferences, per agent. Claude and Codex are saved independently.
    pub preferences: ContextPreferencesByAgent,
    /// The row editor's draft, or `None` when the dialog is closed.
    pub editor: Option<ContextEditorState>,
}

/// One preferences record per agent.
///
/// CDXC:AgentProviders 2026-09-08 DECISION:
/// User: keep the same Claude UI and status line, but save popover and status-line settings
/// independently for Claude and Codex.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextPreferencesByAgent {
    pub claude: ContextDetailsPreferences,
    pub codex: ContextDetailsPreferences,
    pub cursor: ContextDetailsPreferences,
    pub hermes: ContextDetailsPreferences,
    pub pi: ContextDetailsPreferences,
    pub basic: ContextDetailsPreferences,
}

/// Each agent's recommended record until storage answers, like the TypeScript's
/// `normalizeSessionChatContextDetailsPreferences(null, agent)`.
impl Default for ContextPreferencesByAgent {
    fn default() -> Self {
        Self {
            claude: default_preferences(ContextDetailsAgent::Claude),
            codex: default_preferences(ContextDetailsAgent::Codex),
            cursor: default_preferences(ContextDetailsAgent::Cursor),
            hermes: default_preferences(ContextDetailsAgent::Hermes),
            pi: default_preferences(ContextDetailsAgent::Pi),
            basic: default_preferences(ContextDetailsAgent::Basic),
        }
    }
}

impl ContextPreferencesByAgent {
    /// The record for one agent.
    pub fn get(&self, agent: ContextDetailsAgent) -> &ContextDetailsPreferences {
        match agent {
            ContextDetailsAgent::Claude => &self.claude,
            ContextDetailsAgent::Codex => &self.codex,
            ContextDetailsAgent::Cursor => &self.cursor,
            ContextDetailsAgent::Hermes => &self.hermes,
            ContextDetailsAgent::Pi => &self.pi,
            ContextDetailsAgent::Basic => &self.basic,
        }
    }

    /// The record for one agent, to replace it.
    pub fn get_mut(&mut self, agent: ContextDetailsAgent) -> &mut ContextDetailsPreferences {
        match agent {
            ContextDetailsAgent::Claude => &mut self.claude,
            ContextDetailsAgent::Codex => &mut self.codex,
            ContextDetailsAgent::Cursor => &mut self.cursor,
            ContextDetailsAgent::Hermes => &mut self.hermes,
            ContextDetailsAgent::Pi => &mut self.pi,
            ContextDetailsAgent::Basic => &mut self.basic,
        }
    }
}
