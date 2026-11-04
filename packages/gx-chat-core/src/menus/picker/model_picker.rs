//! The model providers, the selection a pick carries, and the scope rules.
//!
//! Port of the surviving half of `packages/shared/session-chat-presentation/model-picker.ts`.

use serde::{Deserialize, Serialize};

/// The agents whose model lineup the model menu can draw.
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
    Hermes,
    Pi,
    Omp,
    Empryo,
    Dsh,
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
            Self::Hermes => "hermes",
            Self::Pi => "pi",
            Self::Omp => "omp",
            Self::Empryo => "empryo",
            Self::Dsh => "dsh",
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
            "hermes" => Some(Self::Hermes),
            "pi" => Some(Self::Pi),
            "omp" => Some(Self::Omp),
            "empryo" => Some(Self::Empryo),
            "dsh" => Some(Self::Dsh),
            _ => None,
        }
    }
}

/// A model and effort choice.
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
/// Claude Code's `/model` list answers `s` with "for this session only", and Codex 0.157 added the
/// same key to its reasoning lists ("for this conversation"); gxserver refuses the session pick on
/// an older Codex rather than saving the default. SEE-ALSO: server/src/session_chat_model_selection.rs
/// read_scope (the 2026-09-27 decision).
pub fn model_picker_supports_session_scope(provider: ModelPickerProvider) -> bool {
    matches!(
        provider,
        ModelPickerProvider::Claude | ModelPickerProvider::Codex | ModelPickerProvider::OpenCode
    )
}

/// CDXC:SessionChat 2026-09-21 DECISION:
/// User: "left-clicking on a model selected, as always, the default. Right-clicking should just
/// apply that for that session." The Session-only model picks setting and the Also set as default
/// switch are removed. This supersedes the 2026-09-19 decision that a setting and a per-session
/// switch chose the scope. The big picker keeps the same split on the keyboard: Enter saves the
/// default, Option+Enter applies to the session alone (see the model pop-up's keys.rs DECISION).
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

/// Shown for every agent whose picker cannot apply a choice to one session: Cursor, Grok,
/// Antigravity, and Empryo, whose `/models` saves the model as its default (its effort is per tab).
pub const MODEL_PICKER_DEFAULT_SCOPE_ONLY_REASON: &str =
    "This agent's model picker always saves the choice as its default.";

/// Hermes is the other way round: `/model` without `--global` never saves a default, and the
/// picker never sends `--global`, so every pick applies to this session alone. Pi's `/model` and
/// `/thinking` and OMP's `/switch` are session-only the same way.
pub const MODEL_PICKER_SESSION_SCOPE_ONLY_REASON: &str = "Every pick applies to this session only.";

/// Why an agent's picker has one scope, for every agent without the session-only choice.
pub fn model_picker_scope_reason(provider: ModelPickerProvider) -> &'static str {
    match provider {
        ModelPickerProvider::Hermes | ModelPickerProvider::Pi | ModelPickerProvider::Omp => {
            MODEL_PICKER_SESSION_SCOPE_ONLY_REASON
        }
        _ => MODEL_PICKER_DEFAULT_SCOPE_ONLY_REASON,
    }
}

/// `modelPickerProvider`: the model family an agent icon belongs to, if any.
pub fn model_picker_provider(icon: Option<&str>) -> Option<ModelPickerProvider> {
    match icon? {
        "claude" => Some(ModelPickerProvider::Claude),
        "codex" => Some(ModelPickerProvider::Codex),
        "cursor-cli" | "cursor" => Some(ModelPickerProvider::Cursor),
        "grok-build" | "grok" => Some(ModelPickerProvider::Grok),
        "antigravity-cli" | "antigravity" => Some(ModelPickerProvider::Antigravity),
        "opencode" => Some(ModelPickerProvider::OpenCode),
        "hermes-agent" => Some(ModelPickerProvider::Hermes),
        "pi" => Some(ModelPickerProvider::Pi),
        "omp" => Some(ModelPickerProvider::Omp),
        "empryo" => Some(ModelPickerProvider::Empryo),
        "dsh" => Some(ModelPickerProvider::Dsh),
        _ => None,
    }
}
