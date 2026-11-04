//! Building the model menu's inputs from family e1's session option catalog.
//!
//! Port of the `modelMenuContext` half of `computeNativeChatOptions`
//! (`packages/shared/session-chat-controller/native-options.ts:129`), and of the
//! `sessionChatSessionOptionCatalog(provider)` sweep `modelMenuEntries()` makes.
//!
//! Family e1 owns the catalog, the descriptors and the option state; this reads its public
//! `compute_native_chat_options` and never writes to `ChatState::menus`.

use serde_json::{json, Map, Value};

use crate::menus::option_catalog::{session_option_catalog, OptionDescriptor};
use crate::menus::option_menu::{
    is_mode_button, option_rows, visible_options, OptionRows as CatalogRows,
};
use crate::menus::option_values::{option_value_label, ChoiceSection, OptionState};
use crate::menus::options::{compute_native_chat_options, NativeChatOptions};
use crate::menus::picker::model_menu::{
    ModelChoice, ModelMenuCatalog, ModelMenuCatalogs, MODEL_MENU_PROVIDERS,
};
use crate::menus::picker::model_picker::ModelPickerProvider;
use crate::menus::picker::projection::ModelMenuContext;
use crate::menus::picker::traits::{
    OptionChoice as TraitChoice, OptionRows, ResolvedOptionDescriptor,
};
use crate::state::{ChatContext, ChatState};

/// Everything family e2 needs for one frame, derived from e1's compute.
pub struct MenuInputs {
    /// `sessionOptions.catalog?.modelIcon`, which chooses the context details catalog.
    pub agent_icon: Option<String>,
    /// `modelMenuContext`, or `None` for an agent outside the model catalog.
    pub menu: Option<ModelMenuContext>,
    /// Every provider's model lineup, as `modelMenuEntries()` sweeps them.
    pub catalogs: std::sync::Arc<ModelMenuCatalogs>,
    /// What `catalogs` was built from ([`catalogs_key`]).
    pub catalogs_key: (u64, bool),
}

/// Reads family e1's options and builds family e2's inputs.
pub fn menu_inputs(state: &ChatState, context: &ChatContext) -> MenuInputs {
    menu_inputs_from(state, &compute_native_chat_options(state, context))
}

/// [`menu_inputs`] from options the caller already computed.
pub fn menu_inputs_from(state: &ChatState, options: &NativeChatOptions) -> MenuInputs {
    let agent_icon = options
        .catalog
        .as_ref()
        .map(|catalog| catalog.model_icon.clone());
    let catalogs_key = catalogs_key(state, options.model_provider);
    // CDXC:SessionChat 2026-10-23 WHY: Sweeping every provider's option catalog was most of a
    // document's cost, and it ran three times per event (the picker settle and both assemblies of
    // the publish test), on every timer tick of every retained chat. It depends only on the agent
    // model catalog and on whether this is a Hermes chat, so the settle's sweep is reused while
    // both are unchanged. Every write to the catalog after that sweep bumps its generation; the
    // one that does not (the bundled seed in `picker::settle`) runs before it.
    let catalogs = if state.pickers.catalogs_key == Some(catalogs_key) {
        std::sync::Arc::clone(&state.pickers.catalogs)
    } else {
        std::sync::Arc::new(model_menu_catalogs(state, options.model_provider))
    };
    MenuInputs {
        menu: model_menu_context(state, options),
        catalogs,
        catalogs_key,
        agent_icon,
    }
}

/// What the provider sweep depends on: the agent model catalog's generation, and whether the
/// session is a Hermes chat (whose picker shows its own tab alone).
pub fn catalogs_key(state: &ChatState, session_provider: Option<&str>) -> (u64, bool) {
    (
        state.menus.model_catalog_generation,
        session_provider == Some(ModelPickerProvider::Hermes.as_str()),
    )
}

/// `sessionChatSessionOptionCatalog(provider)` for each provider in [`MODEL_MENU_PROVIDERS`].
///
/// CDXC:AgentProviders 2026-09-26 DECISION:
/// User (Bots mockup 08): a Hermes chat's model pill opens a picker "with one tab for this bot",
/// named after it, listing the bot's default model plus the models its sessions have used. Other
/// agents' tabs stay out of it, and the Hermes tab only exists in a Hermes chat, whose core alone
/// holds the profile's lineup.
fn model_menu_catalogs(state: &ChatState, session_provider: Option<&str>) -> ModelMenuCatalogs {
    let hermes = ModelPickerProvider::Hermes.as_str();
    let mut catalogs = ModelMenuCatalogs::new();
    for provider in MODEL_MENU_PROVIDERS {
        if session_provider == Some(hermes) && provider != ModelPickerProvider::Hermes {
            continue;
        }
        let Some(catalog) =
            session_option_catalog(&state.menus.model_catalog, Some(provider.as_str()))
        else {
            continue;
        };
        catalogs.insert(
            provider.as_str().to_string(),
            ModelMenuCatalog {
                model_icon: catalog.model_icon.clone(),
                choices: catalog
                    .model
                    .choice_list()
                    .iter()
                    .map(|choice| ModelChoice {
                        value: choice.value.clone(),
                        label: choice.label.clone(),
                        description: choice.description.clone(),
                        legacy: choice.group.as_deref()
                            == Some(crate::menus::catalog::LEGACY_GROUP_ID),
                    })
                    .collect(),
                agent_name: if provider == ModelPickerProvider::Hermes {
                    state
                        .menus
                        .model_catalog
                        .agents
                        .get(hermes)
                        .map(|agent| agent.name.clone())
                } else {
                    None
                },
            },
        );
    }
    catalogs
}

/// Agents outside the model catalog keep their own model and options pills; the picker needs a
/// provider's lineup.
fn model_menu_context(state: &ChatState, options: &NativeChatOptions) -> Option<ModelMenuContext> {
    let provider = ModelPickerProvider::from_wire(options.model_provider?)?;
    let catalog = options.catalog.as_ref()?;
    let caps = options.caps();
    let descriptors = visible_options(&options.option_descriptors, caps);
    let model_id = catalog.model.id.clone();
    let model_value = options
        .state
        .get(&model_id)
        .map(|entry| entry.value.clone());
    let effort_value = options.state.get("effort").map(|entry| entry.value.clone());
    let selection_error = state.pickers.model_selection.selection_error.clone();
    let mut raw = Map::new();
    raw.insert("provider".into(), json!(provider.as_str()));
    raw.insert("modelId".into(), json!(model_id));
    // `modelDefault: undefined` is a key `JSON.stringify` leaves out.
    if let Some(default_value) = &catalog.model.default_value {
        raw.insert("modelDefault".into(), json!(default_value));
    }
    raw.insert("modelLabel".into(), json!(options.option_labels.model));
    raw.insert(
        "descriptors".into(),
        serde_json::to_value(&descriptors).unwrap_or(Value::Null),
    );
    raw.insert(
        "state".into(),
        serde_json::to_value(&options.state).unwrap_or(Value::Null),
    );
    raw.insert(
        "caps".into(),
        json!({
            "canPickModel": caps.can_pick_model,
            "queuedControls": caps.queued_controls,
            "canSendKey": caps.can_send_key,
        }),
    );
    raw.insert("selectionError".into(), json!(selection_error));
    raw.insert("disabled".into(), json!(!options.can_pick_model));
    let draft = state.session.available_agents.is_some();
    raw.insert("draft".into(), json!(draft));
    Some(ModelMenuContext {
        provider: Some(provider),
        model_id: Some(catalog.model.id.clone()),
        model_label: options.option_labels.model.clone(),
        descriptors: resolved_descriptors(state, options, &descriptors),
        model_value,
        effort_value,
        selection_error,
        disabled: !options.can_pick_model,
        session_key: options.session_key.clone(),
        draft,
        orchestrator: crate::menus::options::limits_orchestrator_efforts(state),
        raw: Value::Object(raw),
    })
}

/// `others`: the visible descriptors without the Mode button's descriptor, each resolved against the
/// session's option state the way `modelMenuTraits` resolves them one at a time.
fn resolved_descriptors(
    state: &ChatState,
    options: &NativeChatOptions,
    descriptors: &[OptionDescriptor],
) -> Vec<ResolvedOptionDescriptor> {
    let caps = options.caps();
    descriptors
        .iter()
        .filter(|descriptor| !is_mode_button(descriptor))
        .map(|descriptor| ResolvedOptionDescriptor {
            id: descriptor.id.clone(),
            label: descriptor.label.clone(),
            action_label: descriptor.action_label.clone(),
            default_value: descriptor.default_value.clone(),
            held_value: held_value(&options.state, &descriptor.id),
            value_label: option_value_label(&state.menus.model_catalog, descriptor, &options.state),
            rows: resolve_rows(option_rows(descriptor, &options.state, caps)),
        })
        .collect()
}

fn held_value(state: &OptionState, id: &str) -> Option<String> {
    state.get(id).map(|entry| entry.value.clone())
}

/// `rows.sections.flatMap((section) => section.choices)`: the model menu's footer buttons are one
/// flat list, because a button either toggles or opens a side list.
fn resolve_rows(rows: CatalogRows) -> OptionRows {
    match rows {
        CatalogRows::Action { .. } => OptionRows::Action,
        CatalogRows::Toggle {
            checked,
            value,
            disabled,
            exit_plan,
            ..
        } => OptionRows::Toggle {
            checked,
            value,
            disabled,
            exit_plan,
        },
        CatalogRows::Choices { sections, current } => OptionRows::Choices {
            choices: sections
                .iter()
                .flat_map(|section| match section {
                    ChoiceSection::Choices { choices, .. } => choices.iter(),
                    ChoiceSection::Group { choices, .. } => choices.iter(),
                })
                .map(|choice| TraitChoice {
                    value: choice.value.clone(),
                    label: choice.label.clone(),
                })
                .collect(),
            current,
        },
    }
}
