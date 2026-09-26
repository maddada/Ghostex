//! Building the quick picker's request from the catalog.
//!
//! Port of `packages/shared/session-chat-presentation/model-picker-request.ts`. The one
//! difference is the request id: the TypeScript called `crypto.randomUUID()`, and the core has no
//! random source, so the host supplies the id with the action that opens the picker.

use crate::menus::catalog::AgentModelCatalog;
use crate::menus::picker::model_picker::{
    EffortChoice, ModelPickerModel, ModelPickerProvider, ModelPickerRequest,
};

/// `quickPickerRank`: a value's place in the catalog's `quickPickerOrder`, else the order's
/// length, so rows the order does not name follow in catalog order.
///
/// CDXC:SessionChat 2026-09-22 DECISION: User: new models and quick picker changes must reach
/// customers without an app release. Card order, card names and hidden cards come from the
/// catalog (`quickPickerOrder`, `quickPickerLabel`, `quickPickerHidden`), which is where the
/// earlier decisions now live: Cursor's hand-picked order with Grok 4.7 above Grok 4.6,
/// Antigravity's Gemini-first order, and the Codex codename cards.
///
/// CDXC:SessionChat 2026-09-23 DECISION: User: "for claude we must always show the version for
/// each model please in quick picker and in the chat composer". Claude rows carry no
/// `quickPickerLabel`, so their cards read the versioned label ("Fable 5.1", "Sonnet 5"), the
/// same text the composer pill shows.
fn quick_picker_rank(order: &[String], value: &str) -> usize {
    order
        .iter()
        .position(|entry| entry == value)
        .unwrap_or(order.len())
}

/// `modelPickerProvider`: which quick picker an agent icon opens, if any.
///
/// CDXC:SessionChat 2026-09-09 DECISION: User: Cursor, Grok Build and Antigravity get the quick
/// picker with white accents and one standard icon for every model.
pub fn model_picker_provider(icon: Option<&str>) -> Option<ModelPickerProvider> {
    match icon? {
        "claude" => Some(ModelPickerProvider::Claude),
        "codex" => Some(ModelPickerProvider::Codex),
        "cursor-cli" | "cursor" => Some(ModelPickerProvider::Cursor),
        "grok-build" | "grok" => Some(ModelPickerProvider::Grok),
        "antigravity-cli" | "antigravity" => Some(ModelPickerProvider::Antigravity),
        "opencode" => Some(ModelPickerProvider::OpenCode),
        _ => None,
    }
}

/// `codexCardVersion`: Codex's cards show the codename big and the version under it, so
/// "GPT 6 Astra" with the card name "Astra" is "GPT 6".
fn codex_card_version(label: &str, card_label: Option<&str>) -> String {
    let Some(head) = card_label.and_then(|card| label.strip_suffix(card)) else {
        return label.to_string();
    };
    let trimmed = head.trim_end_matches([' ', '\t', '\n', '\r']);
    if trimmed.len() < head.len() {
        trimmed.to_string()
    } else {
        label.to_string()
    }
}

/// `createModelPickerRequest`, shared by the in-pane chat picker and the terminal's native modal
/// host. `request_id` replaces the `crypto.randomUUID()` the TypeScript generated.
pub fn create_model_picker_request(
    catalog: &AgentModelCatalog,
    provider: ModelPickerProvider,
    selected_model: Option<&str>,
    selected_effort: Option<&str>,
    request_id: String,
) -> Option<ModelPickerRequest> {
    let agent = catalog.agents.get(provider.as_str())?;
    // CDXC:SessionChat 2026-09-09 DECISION: User: keep only the selected models in the quick
    // picker, exclude Cursor Composer too, and retain every other model under Legacy in the
    // normal picker.
    let mut kept: Vec<_> = agent
        .models
        .iter()
        .filter(|model| model.group.is_none() && !model.quick_picker_hidden)
        .collect();
    // `Array.prototype.sort` is stable, so equal ranks keep catalog order.
    kept.sort_by_key(|model| quick_picker_rank(&agent.quick_picker_order, &model.value));
    let models: Vec<ModelPickerModel> = kept
        .into_iter()
        .map(|model| ModelPickerModel {
            value: model.value.clone(),
            label: model
                .quick_picker_label
                .clone()
                .unwrap_or_else(|| model.label.clone()),
            version: match provider {
                ModelPickerProvider::Codex => Some(codex_card_version(
                    &model.label,
                    model.quick_picker_label.as_deref(),
                )),
                _ => None,
            },
            efforts: model
                .efforts
                .iter()
                .map(|value| EffortChoice {
                    value: value.clone(),
                    label: catalog.effort_label(value),
                })
                .collect(),
            default_effort: model
                .default_effort
                .clone()
                .or_else(|| agent.default_effort.clone()),
        })
        .collect();
    // CDXC:SessionChat 2026-09-26 WHY:
    // A session on a hidden context twin (Claude's 200K `opus` beside the `opus[1m]` card) matched
    // no card, so the picker highlighted the catalog default and an effort-only change asked the
    // agent to switch to the other context size, which Claude 2.1.283's list cannot do. The twin's
    // card stands for the session's own size instead.
    let mut models = models;
    if let Some(selected) = selected_model.filter(|value| {
        !models.iter().any(|entry| entry.value == *value)
            && agent
                .models
                .iter()
                .any(|model| model.value == *value && model.group.is_none())
    }) {
        let twin = selected
            .strip_suffix("[1m]")
            .map(str::to_string)
            .unwrap_or_else(|| format!("{selected}[1m]"));
        if let Some(card) = models.iter_mut().find(|entry| entry.value == twin) {
            card.value = selected.to_string();
        }
    }
    // Detection may not have arrived yet. The catalog default is a starting cursor, not a claim
    // about the running agent.
    let catalog_default = agent
        .models
        .iter()
        .find(|model| model.default)
        .map(|model| model.value.as_str());
    let model = models
        .iter()
        .find(|entry| Some(entry.value.as_str()) == selected_model)
        .or_else(|| {
            models
                .iter()
                .find(|entry| Some(entry.value.as_str()) == catalog_default)
        })
        .or_else(|| models.first())?
        .clone();
    let effort = model
        .efforts
        .iter()
        .find(|entry| Some(entry.value.as_str()) == selected_effort)
        .or_else(|| {
            model
                .efforts
                .iter()
                .find(|entry| Some(&entry.value) == model.default_effort.as_ref())
        })
        .or_else(|| model.efforts.first())
        .map(|entry| entry.value.clone())
        .unwrap_or_default();
    let efforts = agent
        .efforts
        .iter()
        .map(|value| EffortChoice {
            value: value.clone(),
            label: catalog.effort_label(value),
        })
        .collect();
    Some(ModelPickerRequest {
        request_id,
        provider,
        models,
        efforts,
        model: model.value.clone(),
        effort,
    })
}
