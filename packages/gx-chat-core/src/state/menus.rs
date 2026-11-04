//! Family e's state: the model picker, the option pills and their menus, accounts, context details
//! and the fork branch picker.
//!
//! **This file belongs to family e (menus, pickers, options, accounts, context).** No other family
//! edits it. Family e1 owns the option, account and settings fields; family e2 owns
//! [`MenusState::picker`] and [`MenusState::context`], the two boxes its subdirectories fill.
//!
//! Read from, never write to: `ChatState::session::selected_options` (family a merges it by
//! evidence priority and `detectedAt`; the document key `selectedOptions` is yours to publish),
//! `ChatState::session::account_switch`, `ChatState::session::pending_model_selection`,
//! `ChatState::session::available_agents`, `ChatState::session::switchable_agents`,
//! `ChatState::session::screen_probed` (latched: it decides between a loading skeleton and a plain
//! unset pill).

use serde_json::Value;

use crate::menus::account_switch::AccountSwitchState;
use crate::menus::catalog::AgentModelCatalog;
use crate::menus::option_store::OptionStore;

/// What the menus, pickers, options, accounts and context surfaces remember between frames.
/// `case 'switchDraftAgent'` (`native-host.ts:888`), one `await` at a time.
#[derive(Clone, Debug, PartialEq)]
pub struct DraftAgentSwitch {
    pub agent_id: String,
    /// The model and effort the launch line carries (`agentModel`, `agentEffort`), when the pick
    /// came from a Claude or Codex row of the model menu.
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The gxserver call once the flush has answered; `None` while the flush is in flight.
    pub request_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenusState {
    /// The agent model catalog in effect, pushed in by the host. Empty until the first push, which
    /// is what leaves a session with no pills rather than pills naming models it cannot run.
    pub model_catalog: AgentModelCatalog,
    /// How many times a catalog was ADOPTED, whatever its contents.
    ///
    /// `adoptAgentModelCatalog` parses into a fresh object and `replaceCatalog` swaps it in
    /// unconditionally, so every push gives `useMemo(..., [agent, agentModelCatalog])` a new
    /// identity and rebuilds the option store even when the document is byte identical.
    pub model_catalog_generation: u64,
    /// The key the option values are stored under, `None` for a session with no key yet.
    ///
    /// CDXC:Drafts 2026-08-28 WHY:
    /// A session that has never been a draft in this client keeps the original key, so every
    /// existing session still reads exactly what it stored. A draft appends `#<agentId>`, which is
    /// what makes switching its agent CLI start from that agent's own values instead of carrying
    /// the previous CLI's dispatched model. The suffix LATCHES for the life of this core:
    /// promotion (the first send) stops the daemon sending `availableAgents`, and without the
    /// latch the key would move back mid-session and drop a dispatched value gxserver has not
    /// confirmed yet.
    pub session_key: Option<String>,
    /// The draft agent id the storage key latched onto, with the session key it was latched for.
    pub latched_draft_agent: Option<(String, Option<String>)>,
    /// The context meter's clock (`native-context.ts:51`, `useState(Date.now)`).
    ///
    /// Latched, not live: the meter's countdown labels move only when this does, which is once
    /// when the controller first renders, whenever the settings or the context preferences are
    /// adopted, and every 30 seconds on its own interval. Each of those is a state change the live
    /// brain publishes on, whether or not a label moved.
    pub meter_now_ms: Option<f64>,
    /// The clock of the controller's last render, which `nativeAccountPanel` reads (its own
    /// `Date.now()` runs inside `computeNativeChatControls`). A publish that re-renders nothing
    /// ships the panel as that render drew it, so its minute labels hold until the next render.
    pub panel_clock_ms: Option<f64>,
    /// A `switchDraftAgent` in flight: `await composer('flush')`, then the call, then `refresh()`.
    pub draft_agent_switch: Option<DraftAgentSwitch>,
    /// The Run on row of a draft (where it will run: this computer or an agentbox box).
    pub run_location: crate::menus::run_location::RunLocationState,
    /// The worktree tray of a draft (This folder, a new worktree or an existing one).
    pub draft_worktree: crate::menus::draft_worktree::DraftWorktreeState,
    /// The option values and the changes still in flight.
    pub options: OptionStore,
    /// The agent the option store was built for, so an agent change rebuilds it.
    pub options_agent: Option<String>,
    /// The catalog generation the option store was built for.
    pub options_catalog_generation: u64,
    /// How many times the option store was rebuilt, which is `applyDetected`'s own identity.
    pub options_store_generation: u64,
    /// The `(selected options, option store)` generations the detection effect last ran for, or
    /// `None` when it has not run at all. That pair is
    /// `[sessionOptions.applyDetected, chat.selectedOptions]`.
    pub applied_detection: Option<(u64, u64)>,
    /// `seed.optionStates`: the per-key stored option state, as the boot read handed it over and
    /// as every write since has updated it. Keyed by the SCOPED option key, not the session key.
    pub stored_options: Value,
    /// `true` once the boot read has answered, so a rebuild does not seed from nothing.
    pub options_seeded: bool,

    /// The last `agentAccounts` answer, absent until the first read lands.
    pub accounts: Option<Value>,
    /// The last account read's failure, absent while it succeeded.
    pub account_error: Option<String>,
    /// A read is in flight, which the panel draws as busy.
    pub accounts_busy: bool,
    /// The generation of the account read in flight, so a stale answer is dropped.
    pub accounts_generation: u64,
    /// The accounts read in flight, so its answer reaches family e and nothing else.
    pub accounts_request: Option<u64>,
    /// When the 30 second poll's `setInterval` next fires (its `timer.at`), or `None` while no
    /// provider is known and the effect armed nothing.
    pub accounts_poll_due_ms: Option<i64>,
    /// What the poll was keyed on, so a provider or switch change re-reads at once.
    pub accounts_key: Option<String>,
    /// The account-switch card's own machine.
    pub account_switch: AccountSwitchState,

    /// The descriptor id of the option dispatch in flight, or `None`.
    pub option_dispatch_id: Option<String>,
    /// A cyclic mode change is walking the TUI, which holds sending until it lands.
    pub option_switching: bool,
    /// Whether the transport can type a raw key into the agent (`chat.sendKey`).
    pub can_send_key: bool,
    /// The option dispatch walking its steps, or `None`.
    pub dispatch_run: Option<crate::menus::dispatch_run::OptionDispatchRun>,
}

/// The state a chat opens with.
///
/// Hand-written rather than derived because `can_send_key` starts TRUE: every gxserver transport
/// accepts a raw key, and a host whose transport does not clears it. A derived `false` would hide
/// the Shift+Tab permission cycler and every other keystroke row on a `ChatState::default()`,
/// which is what `ChatCore::new` builds.
impl Default for MenusState {
    fn default() -> Self {
        Self {
            model_catalog: AgentModelCatalog::default(),
            model_catalog_generation: 0,
            session_key: None,
            latched_draft_agent: None,
            meter_now_ms: None,
            panel_clock_ms: None,
            draft_agent_switch: None,
            run_location: Default::default(),
            draft_worktree: Default::default(),
            options: OptionStore::default(),
            options_agent: None,
            options_catalog_generation: 0,
            options_store_generation: 0,
            applied_detection: None,
            stored_options: Value::Null,
            options_seeded: false,
            accounts: None,
            account_error: None,
            accounts_busy: false,
            accounts_generation: 0,
            accounts_request: None,
            accounts_poll_due_ms: None,
            accounts_key: None,
            account_switch: AccountSwitchState::default(),
            option_dispatch_id: None,
            option_switching: false,
            dispatch_run: None,
            can_send_key: true,
        }
    }
}

impl MenusState {
    /// The state a chat opens with.
    pub fn new() -> Self {
        Self::default()
    }
}
