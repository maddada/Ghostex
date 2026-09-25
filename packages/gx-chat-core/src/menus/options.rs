//! Everything the composer's option pills publish, in one pass over the state.
//!
//! Port of `packages/shared/session-chat-controller/native-options.ts` and
//! `packages/shared/session-chat-controller/session-options.ts`. The TypeScript was a React hook
//! chain (`useMemo` over the catalog, `useLayoutEffect` for the detection, a store with
//! listeners); here it is one function of the state, because the state already holds what the
//! hooks memoized.

use serde::Serialize;
use serde_json::Value;

use crate::menus::accounts_data::AccountsState;
use crate::menus::option_catalog::{
    session_option_catalog, OptionDescriptor, SessionOptionCatalog,
};
use crate::menus::option_menu::{
    is_shift_tab_mode_cycler, option_menu_sections, options_may_resolve, visible_options,
    OptionCaps,
};
use crate::menus::option_menus::{
    native_option_menus, DraftAgent, NativeOptionMenus, OptionMenuParams,
};
use crate::menus::option_pills::{account_indicator, option_pill_values, options_title};
use crate::menus::option_values::{current_model_value, OptionState};
use crate::state::{ChatContext, ChatState};

/// `modelPickerProvider(icon)`, as the option pills need it: the provider's wire spelling.
///
/// The rule itself is family e2's (`crate::menus::picker::request::model_picker_provider`); this
/// is the same answer with `ModelPickerProvider::as_str` applied, because the pills publish
/// `modelProvider` as a plain string. It was a second copy of the table until 2026-09-22.
pub fn model_picker_provider(icon: Option<&str>) -> Option<&'static str> {
    crate::menus::picker::request::model_picker_provider(icon).map(|provider| provider.as_str())
}

/// `modelPickerSupportsSessionScope(provider)`, keyed by the wire spelling.
///
/// A provider string the catalog does not know answers false, which is what the enum's own
/// `model_picker_supports_session_scope` cannot be handed at all.
pub fn picker_supports_session_scope(provider: &str) -> bool {
    crate::menus::picker::model_picker::ModelPickerProvider::from_wire(provider)
        .is_some_and(crate::menus::picker::model_picker::model_picker_supports_session_scope)
}

/// The pill labels the composer draws, in the key order `JSON.stringify` writes them.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionLabels {
    pub model: Option<String>,
    pub model_display: Option<String>,
    pub options: Option<String>,
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_icon: Option<String>,
    pub fast: bool,
    pub plan: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_indicator: Option<String>,
    pub options_title: String,
    pub options_tooltip: String,
    pub model_quick_picker: bool,
    pub show_model: bool,
    pub show_options: bool,
}

/// The `sessionOptions` document key: the catalog's serializable half, the descriptors for the
/// model believed to be running, and the current values.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOptionsDocument {
    pub session_key: Option<String>,
    pub catalog: Option<PublishedCatalog>,
    pub option_descriptors: Vec<OptionDescriptor>,
    pub state: OptionState,
}

/// What `JSON.stringify` leaves of a catalog once its two functions are dropped.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedCatalog {
    pub model: OptionDescriptor,
    pub model_icon: String,
}

/// Everything family e1 computes for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeChatOptions {
    pub catalog: Option<SessionOptionCatalog>,
    pub option_descriptors: Vec<OptionDescriptor>,
    pub state: OptionState,
    pub session_key: Option<String>,
    /// The picker lineup, or `None` for an agent outside the model catalog.
    pub model_provider: Option<&'static str>,
    /// `canQueue`: the daemon can apply a model choice for this session.
    pub can_pick_model: bool,
    /// The agent accepts queued option changes, so Plan and Fast stay live while it works.
    pub queued_controls: bool,
    pub can_send_key: bool,
    pub option_menus: NativeOptionMenus,
    pub option_labels: OptionLabels,
    pub session_options: SessionOptionsDocument,
}

impl NativeChatOptions {
    /// The caps every menu rule reads.
    pub fn caps(&self) -> OptionCaps {
        OptionCaps {
            can_send_key: self.can_send_key,
            can_pick_model: self.can_pick_model,
            queued_controls: self.queued_controls,
        }
    }
}

/// `computeNativeChatOptions`.
pub fn compute_native_chat_options(state: &ChatState, _context: &ChatContext) -> NativeChatOptions {
    let menus = &state.menus;
    let agent = state.session.agent.as_deref();
    let catalog = session_option_catalog(&menus.model_catalog, agent);
    let values = menus.options.state().clone();
    let option_descriptors = match &catalog {
        Some(catalog) => {
            let model_value = current_model_value(catalog, &values);
            catalog.options_for_model(&model_value)
        }
        None => Vec::new(),
    };
    let provider =
        model_picker_provider(catalog.as_ref().map(|catalog| catalog.model_icon.as_str()));
    // `chat.pendingModelSelection !== undefined && provider !== undefined`: absent means the
    // daemon never carried the field, which is the "cannot queue a model" state.
    let can_pick_model = !state.session.pending_model_selection.is_absent() && provider.is_some();
    let queued_controls = matches!(
        catalog.as_ref().map(|catalog| catalog.model_icon.as_str()),
        Some("codex") | Some("claude")
    );
    let can_send_key = menus.can_send_key;
    let draft_agents = DraftAgent::list(state.session.available_agents.as_ref());
    let draft_agent_id = state.session.session_agent_id.clone();
    // `selectionError: modelSelection.selectionError` (`native-options.ts:112`), which is the
    // pending selection's own `errorMessage` once it has failed (`model-selection.ts:223`). It was
    // hardcoded to `None` here, so the option pill's model menu could never draw the "Not applied"
    // heading the model menu two fields away draws from the same value.
    let selection_error = state.pickers.model_selection.selection_error.clone();
    let menu_params = OptionMenuParams {
        quick_picker: provider.is_some(),
        can_pick_model,
        working: crate::session::working::is_working(state),
        can_send_key,
        draft_agents: draft_agents.clone(),
        draft_agent_id: draft_agent_id.clone(),
        has_provider: provider.is_some(),
        selection_error: selection_error.clone(),
    };
    let option_menus =
        native_option_menus(catalog.as_ref(), &values, &option_descriptors, &menu_params);
    let pill_values = option_pill_values(
        &menus.model_catalog,
        catalog.as_ref(),
        &option_descriptors,
        &values,
    );
    let sections = option_menu_sections(
        &visible_options(
            &option_descriptors,
            OptionCaps {
                can_send_key,
                can_pick_model,
                queued_controls: provider.is_some(),
            },
        )
        .into_iter()
        .filter(|descriptor| !is_shift_tab_mode_cycler(descriptor))
        .collect::<Vec<_>>(),
    );
    let draft_agent = draft_agents.as_ref().and_then(|agents| {
        agents
            .iter()
            .find(|agent| Some(agent.agent_id.as_str()) == draft_agent_id.as_deref())
    });
    let accounts = menus.accounts.as_ref().map(AccountsState::from_value);
    let mut option_labels = OptionLabels {
        model: pill_values.model.clone(),
        model_display: pill_values.model_display.clone(),
        options: pill_values.options.clone(),
        mode: pill_values.mode.clone(),
        mode_value: pill_values.mode_value.clone(),
        agent_icon: pill_values.agent_icon.clone(),
        fast: pill_values.fast,
        plan: pill_values.plan,
        account_indicator: account_indicator(accounts.as_ref()),
        options_title: options_title(&sections, pill_values.fast, pill_values.plan, ""),
        options_tooltip: options_title(
            &sections,
            pill_values.fast,
            pill_values.plan,
            if provider.is_some() {
                " ({shortcut})"
            } else {
                ""
            },
        ),
        model_quick_picker: provider.is_some(),
        show_model: catalog.is_some() || !option_menus.model.is_empty(),
        show_options: !option_menus.options.is_empty()
            || (pill_values.options.is_none()
                && catalog
                    .as_ref()
                    .is_some_and(|catalog| options_may_resolve(catalog, can_send_key))),
    };
    // An agent outside the model catalog keeps its own pill: the draft's own row names it.
    if catalog.is_none() {
        if let Some(draft_agent) = draft_agent {
            option_labels.model = Some(draft_agent.name.clone());
            option_labels.model_display = Some(draft_agent.name.clone());
            option_labels.agent_icon = draft_agent.icon.clone();
        }
    }
    let session_options = SessionOptionsDocument {
        // `computeSessionChatOptions` publishes the STORAGE key, which a draft session suffixes
        // with its agent id, not the bare session key the host handed over.
        session_key: menus.options.storage_key.clone(),
        catalog: catalog.as_ref().map(|catalog| PublishedCatalog {
            model: catalog.model.clone(),
            model_icon: catalog.model_icon.clone(),
        }),
        option_descriptors: option_descriptors.clone(),
        state: values.clone(),
    };
    NativeChatOptions {
        catalog,
        option_descriptors,
        state: values,
        session_key: menus.options.storage_key.clone(),
        model_provider: provider,
        can_pick_model,
        queued_controls,
        can_send_key,
        option_menus,
        option_labels,
        session_options,
    }
}

/// The storage key the option values are kept under, latching a draft's agent suffix.
///
/// CDXC:Drafts 2026-08-28 WHY:
/// The suffix latches for the life of this core: promotion (the first send) stops the daemon
/// sending `availableAgents`, and without the latch the key would move back mid-session and drop
/// a dispatched value gxserver has not confirmed yet. A later reload of a promoted session lands
/// on the plain key again, by which time detection is the authority anyway.
pub fn option_storage_key(
    latched: &mut Option<(String, Option<String>)>,
    session_key: Option<&str>,
    draft_agent_id: Option<&str>,
) -> Option<String> {
    if let Some(draft_agent_id) = draft_agent_id.filter(|id| !id.is_empty()) {
        *latched = Some((draft_agent_id.to_string(), session_key.map(str::to_string)));
    }
    let session_key = session_key?;
    let storage_agent_id = latched.as_ref().and_then(|(agent_id, latched_key)| {
        if latched_key.as_deref() == Some(session_key) {
            Some(agent_id.as_str())
        } else {
            None
        }
    });
    Some(match storage_agent_id {
        Some(agent_id) => format!("{session_key}#{agent_id}"),
        None => session_key.to_string(),
    })
}

/// The `accounts` answer parsed, for the surfaces that need more than the raw value.
pub fn accounts_state(state: &ChatState) -> Option<AccountsState> {
    state.menus.accounts.as_ref().map(AccountsState::from_value)
}

/// `chat.selectedOptions` as the option store folds it.
pub fn selected_options(state: &ChatState) -> Option<&Value> {
    state.session.selected_options.as_ref()
}
