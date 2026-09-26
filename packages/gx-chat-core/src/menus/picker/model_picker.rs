//! The quick model picker's data and geometry.
//!
//! Port of `packages/shared/session-chat-presentation/model-picker.ts`: the request the picker is
//! opened with, the scope rules, the stage layout and the two cursor moves.

use serde::{Deserialize, Serialize};

/// One row of the quick picker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPickerModel {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub efforts: Vec<EffortChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
}

/// One effort level, with the catalog's label for it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortChoice {
    pub value: String,
    pub label: String,
}

/// The agents whose lineup the quick picker can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelPickerProvider {
    Codex,
    Claude,
    Cursor,
    Grok,
    Antigravity,
    #[serde(rename = "opencode")]
    OpenCode,
}

impl ModelPickerProvider {
    /// The wire spelling, which is also the catalog's agent id and the favorites key's prefix.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Grok => "grok",
            Self::Antigravity => "antigravity",
            Self::OpenCode => "opencode",
        }
    }

    /// The provider with this id, or `None`.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            "cursor" => Some(Self::Cursor),
            "grok" => Some(Self::Grok),
            "antigravity" => Some(Self::Antigravity),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }
}

/// Everything the picker was opened with. Built once, then never changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPickerRequest {
    /// Supplied by the host, never generated here: the core has no random source.
    pub request_id: String,
    pub provider: ModelPickerProvider,
    pub models: Vec<ModelPickerModel>,
    pub efforts: Vec<EffortChoice>,
    pub model: String,
    pub effort: String,
}

/// Where the cursor is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPickerSelection {
    pub model: String,
    pub effort: String,
}

/// Whether a pick changes the agent's saved default or only this session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelSelectionScope {
    Session,
    /// Omitted on the wire means this, which is what every pick did before the scope existed.
    #[default]
    Default,
}

impl ModelSelectionScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Default => "default",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "session" => Some(Self::Session),
            "default" => Some(Self::Default),
            _ => None,
        }
    }
}

/// Whether this agent's own picker can apply a choice without changing its saved default.
///
/// Claude Code's `/model` list answers `s` with "for this session only"; Codex's picker writes
/// `model` and `model_reasoning_effort` into `~/.codex/config.toml` on every confirm.
pub fn model_picker_supports_session_scope(provider: ModelPickerProvider) -> bool {
    matches!(provider, ModelPickerProvider::Claude | ModelPickerProvider::OpenCode)
}

/// CDXC:SessionChat 2026-09-21 DECISION:
/// User: "left-clicking on a model selected, as always, the default. Right-clicking should just
/// apply that for that session." The Session-only model picks setting and the Also set as default
/// switch are removed. This supersedes the 2026-09-19 decision that a setting and a per-session
/// switch chose the scope. The big picker keeps the same split on the keyboard: Enter saves the
/// default, Shift+Enter applies to the session alone.
pub fn model_pick_scope(
    provider: Option<ModelPickerProvider>,
    secondary: bool,
) -> ModelSelectionScope {
    match provider {
        Some(provider) if secondary && model_picker_supports_session_scope(provider) => {
            ModelSelectionScope::Session
        }
        _ => ModelSelectionScope::Default,
    }
}

/// Shown for every agent whose picker cannot apply a choice to one session: Codex, Cursor, Grok,
/// Antigravity.
pub const MODEL_PICKER_DEFAULT_SCOPE_ONLY_REASON: &str =
    "This agent's model picker always saves the choice as its default.";

/// A pane size the host measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneSize {
    pub width: f64,
    pub height: f64,
}

/// Everything `modelPickerLayout` returns, in the document's own spelling.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPickerLayout {
    pub narrow: bool,
    pub viewport_height: f64,
    pub stage_width: f64,
    pub short: bool,
    pub scale: f64,
    pub visible_models: f64,
    pub first_visible: f64,
    pub stage_height: f64,
    pub rail_offset: f64,
    pub center_y: f64,
    pub effort_split: f64,
}

/// `modelPickerLayout`.
pub fn model_picker_layout(
    request: &ModelPickerRequest,
    model_index: f64,
    pane_size: PaneSize,
    controls_height: f64,
    pointer_rail_start: Option<f64>,
) -> ModelPickerLayout {
    let model_count = request.models.len() as f64;
    let effort_count = request.efforts.len() as f64;
    let narrow = pane_size.width <= 920.0;
    let viewport_height = 1.0f64.max(pane_size.height - controls_height - 24.0);
    let stage_width = 1180.0f64.max((effort_count / 2.0).ceil() * 284.0 + 328.0);
    let width_scale = 0.01f64.max(
        1.0f64.min((pane_size.width - 28.0) / if narrow { 240.0 } else { stage_width - 120.0 }),
    );
    let short = viewport_height - 24.0 < 3.0 * 142.0 * width_scale;
    let scale = 0.01f64
        .max(width_scale.min((viewport_height - 24.0) / if short { 200.0 } else { 3.0 * 142.0 }));
    let visible_models = if short {
        1.0
    } else {
        model_count.min(3.0f64.max(((viewport_height - 24.0) / (142.0 * scale)).floor()))
    };
    let first_visible = if short {
        model_index
    } else {
        pointer_rail_start.unwrap_or_else(|| {
            0.0f64.max(
                (model_count - visible_models).min(model_index - (visible_models / 2.0).floor()),
            )
        })
    };
    let stage_height = viewport_height / scale;
    let rail_offset = (stage_height - visible_models * 142.0) / 2.0 - first_visible * 142.0;
    let center_y = 71.0 + model_index * 142.0 + rail_offset;
    let effort_split = (effort_count / 2.0).ceil();
    ModelPickerLayout {
        narrow,
        viewport_height,
        stage_width,
        short,
        scale,
        visible_models,
        first_visible,
        stage_height,
        rail_offset,
        center_y,
        effort_split,
    }
}

/// `modelPickerChooseModel`: the selection after moving the cursor to `index`, keeping the effort
/// when the new model offers it.
pub fn model_picker_choose_model(
    request: &ModelPickerRequest,
    selection: &ModelPickerSelection,
    index: i64,
) -> Option<ModelPickerSelection> {
    let next = index_of(&request.models, index)?;
    let effort = if next
        .efforts
        .iter()
        .any(|effort| effort.value == selection.effort)
    {
        selection.effort.clone()
    } else {
        next.efforts
            .iter()
            .find(|effort| Some(&effort.value) == next.default_effort.as_ref())
            .or_else(|| next.efforts.first())
            .map(|effort| effort.value.clone())
            .unwrap_or_default()
    };
    Some(ModelPickerSelection {
        model: next.value.clone(),
        effort,
    })
}

/// `modelPickerChooseEffort`: `None` when the model in the selection does not offer that effort.
pub fn model_picker_choose_effort(
    request: &ModelPickerRequest,
    selection: &ModelPickerSelection,
    index: i64,
) -> Option<ModelPickerSelection> {
    let next = index_of(&request.efforts, index)?;
    let model = request
        .models
        .iter()
        .find(|entry| entry.value == selection.model)?;
    if !model.efforts.iter().any(|entry| entry.value == next.value) {
        return None;
    }
    Some(ModelPickerSelection {
        model: selection.model.clone(),
        effort: next.value.clone(),
    })
}

/// `modelPickerNextEffortIndex`: the next effort in `direction` the current model can take.
pub fn model_picker_next_effort_index(
    request: &ModelPickerRequest,
    selection: &ModelPickerSelection,
    direction: i64,
) -> Option<i64> {
    let effort_index = request
        .efforts
        .iter()
        .position(|effort| effort.value == selection.effort)
        .map(|index| index as i64)
        .unwrap_or(-1);
    let mut index = effort_index + direction;
    while index >= 0 && index < request.efforts.len() as i64 {
        if model_picker_choose_effort(request, selection, index).is_some() {
            return Some(index);
        }
        index += direction;
    }
    None
}

/// `list[index]` with JavaScript's out-of-range behaviour: a negative or too large index is
/// `undefined`, never a panic and never a wrap.
fn index_of<T>(list: &[T], index: i64) -> Option<&T> {
    usize::try_from(index)
        .ok()
        .and_then(|index| list.get(index))
}
