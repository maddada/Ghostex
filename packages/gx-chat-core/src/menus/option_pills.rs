//! The composer footer pills: what the model pill and the options pill read.
//!
//! Port of `packages/shared/session-chat-presentation/option-pills.ts`.

use serde::Serialize;

use crate::menus::accounts_data::AccountsState;
use crate::menus::catalog::{
    truncate_agent_model_label, AgentModelCatalog, AGENT_MODEL_LABEL_MAX_CHARS,
};
use crate::menus::option_catalog::{OptionDescriptor, SessionOptionCatalog, MODES_SECTION_LABEL};
use crate::menus::option_menu::{is_mode_button, OptionSection};
use crate::menus::option_values::{option_value_label, options_pill_label, OptionState};

/// The account mark the model pill draws over the provider logo: the account this session is
/// bound to, by its own two-character indicator or, without one, its slot selector. `-` hides the
/// mark, and a session with no bound account (or an agent that has no accounts) leaves the logo
/// plain.
///
/// SEE-ALSO: packages/core-ui/project-agent-launcher-icon.tsx (deleted 2026-10-01),
/// packages/core-ui/accounts/indicator.tsx (deleted 2026-10-01), apps/desktop/src/app/native_chat/option_pills.rs.
pub fn account_indicator(accounts: Option<&AccountsState>) -> Option<String> {
    let accounts = accounts?;
    let session_account_id = accounts
        .session
        .as_ref()
        .and_then(|session| session.account_id.as_deref());
    let active = accounts.accounts.iter().find(|account| {
        account.id.as_deref().is_some() && account.id.as_deref() == session_account_id
    })?;
    // `account.indicator || account.selector`: a blank indicator falls through to the selector.
    let value = match active.indicator.as_deref() {
        Some(indicator) if !indicator.is_empty() => indicator,
        _ => active.selector.as_deref().unwrap_or(""),
    };
    if value.is_empty() || value == "-" {
        None
    } else {
        Some(value.to_string())
    }
}

/// `sessionChatOptionsTitle`: the options pill's tooltip, one section name per configured group.
pub fn options_title(
    sections: &[OptionSection],
    fast: bool,
    plan: bool,
    picker_shortcut_suffix: &str,
) -> String {
    let mut parts: Vec<String> = sections
        .iter()
        .filter(|section| section.label != MODES_SECTION_LABEL)
        .map(|section| {
            let suffix = if section
                .descriptors
                .iter()
                .any(|descriptor| descriptor.id == "effort")
            {
                picker_shortcut_suffix
            } else {
                ""
            };
            format!("{}{suffix}", section.label)
        })
        .collect();
    if fast {
        parts.push("Fast enabled".to_string());
    }
    if plan {
        parts.push("Plan mode".to_string());
    }
    let joined = parts.join(" • ");
    if joined.is_empty() {
        "Options".to_string()
    } else {
        joined
    }
}

/// `sessionChatOptionPillValues`, in the key order `JSON.stringify` writes it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionPillValues {
    /// The model label, or `null` when nothing is known.
    pub model: Option<String>,
    /// The same label cut to the pill's width.
    pub model_display: Option<String>,
    /// Every other known value joined by " · ".
    pub options: Option<String>,
    /// The Shift+Tab mode's label, or `null`.
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_icon: Option<String>,
    pub fast: bool,
    pub plan: bool,
}

/// `sessionChatOptionPillValues`.
pub fn option_pill_values(
    model_catalog: &AgentModelCatalog,
    catalog: Option<&SessionOptionCatalog>,
    descriptors: &[OptionDescriptor],
    state: &OptionState,
) -> OptionPillValues {
    let model =
        catalog.and_then(|catalog| option_value_label(model_catalog, &catalog.model, state));
    let mode = descriptors
        .iter()
        .find(|descriptor| is_mode_button(descriptor));
    let others: Vec<OptionDescriptor> = descriptors
        .iter()
        .filter(|descriptor| !is_mode_button(descriptor))
        .cloned()
        .collect();
    let icon = catalog.map(|catalog| catalog.model_icon.clone());
    let codex = icon.as_deref() == Some("codex");
    OptionPillValues {
        model_display: model
            .as_deref()
            .map(|label| truncate_agent_model_label(label, AGENT_MODEL_LABEL_MAX_CHARS)),
        model,
        options: options_pill_label(model_catalog, &others, state),
        mode: mode.and_then(|mode| option_value_label(model_catalog, mode, state)),
        mode_value: mode.and_then(|mode| state.get(&mode.id).map(|entry| entry.value.clone())),
        agent_icon: icon,
        fast: codex && state.get("fastMode").map(|entry| entry.value.as_str()) == Some("on"),
        plan: codex && state.get("mode").map(|entry| entry.value.as_str()) == Some("plan"),
    }
}
