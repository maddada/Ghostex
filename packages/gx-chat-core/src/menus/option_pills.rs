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

/// The model pill's tooltip line while the account this session is bound to needs signing in on
/// the session's computer: gxserver reads it `loginRequired` or `identityChanged`, or
/// `unavailable` for an account synced from another computer that was never signed in on this one
/// (a plain `unavailable`, a saved login missing for another reason, is not called signed out).
/// `computer` is the session's computer's name, `None` for this computer. The account's name is
/// masked when Settings hides account emails. `None` while the account is signed in, or the
/// session has none.
///
/// CDXC:AgentProviders 2026-11-19 DECISION:
/// User (Q4198 A, the signed-out accounts page): "i need to know (per machine) if the session is
/// active or not and if i send will it work or not". The chat box says it on hover: the model
/// pill's account mark turns amber and its tooltip names the computer the account is signed out
/// on and that sending won't work until it signs in there. The accounts the chat reads are the
/// session's own computer's (its gxserver answers `agentAccounts`), so this is that computer's
/// state, not this one's.
pub fn account_sign_in_note(
    accounts: Option<&AccountsState>,
    computer: Option<&str>,
    hide_account_emails: bool,
) -> Option<String> {
    let accounts = accounts?;
    let session_account_id = accounts
        .session
        .as_ref()
        .and_then(|session| session.account_id.as_deref())?;
    let active = accounts
        .accounts
        .iter()
        .find(|account| account.id.as_deref() == Some(session_account_id))?;
    let signed_out = match active.status.as_deref() {
        Some("loginRequired" | "identityChanged") => true,
        Some("unavailable") => active.synced_from.is_some(),
        _ => false,
    };
    if !signed_out {
        return None;
    }
    let label = account_indicator(Some(accounts)).unwrap_or_else(|| {
        if active.name.trim().is_empty() {
            "This account".to_string()
        } else {
            active.name.trim().to_string()
        }
    });
    let place = match computer.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("on {name}"),
        None => "on this computer".to_string(),
    };
    let label = crate::menus::native_accounts::account_text(hide_account_emails, &label);
    Some(format!(
        "{label} is signed out {place}. Sending won't work until it signs in there."
    ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn accounts(status: &str) -> AccountsState {
        AccountsState::from_value(&json!({
            "accounts": [
                {"id": "claude-ms", "provider": "claude", "indicator": "MS", "name": "MS", "registered": true, "status": "ready"},
                {"id": "claude-st", "provider": "claude", "indicator": "ST", "name": "ST", "registered": true, "status": status},
            ],
            "session": {"provider": "claude", "accountId": "claude-st"},
        }))
    }

    #[test]
    fn a_signed_out_account_on_a_remote_computer_names_it() {
        assert_eq!(
            account_sign_in_note(Some(&accounts("loginRequired")), Some("Windows PC"), false)
                .as_deref(),
            Some("ST is signed out on Windows PC. Sending won't work until it signs in there.")
        );
        assert_eq!(
            account_sign_in_note(Some(&accounts("identityChanged")), None, false).as_deref(),
            Some("ST is signed out on this computer. Sending won't work until it signs in there.")
        );
    }

    #[test]
    fn a_signed_in_or_unbound_session_has_no_note() {
        assert_eq!(
            account_sign_in_note(Some(&accounts("ready")), None, false),
            None
        );
        let mut unbound = accounts("loginRequired");
        unbound.session = None;
        assert_eq!(account_sign_in_note(Some(&unbound), None, false), None);
        assert_eq!(account_sign_in_note(None, None, false), None);
    }

    /// A plain `unavailable` is not signed out; one synced from another computer is.
    #[test]
    fn unavailable_is_signed_out_only_when_synced() {
        assert_eq!(
            account_sign_in_note(Some(&accounts("unavailable")), None, false),
            None
        );
        let mut synced = accounts("unavailable");
        synced.accounts[1].synced_from = Some("Windows PC".into());
        assert!(account_sign_in_note(Some(&synced), None, false).is_some());
    }

    /// An account named by its email is masked when Settings hides account emails.
    #[test]
    fn the_note_masks_an_email_name() {
        let mut named = accounts("loginRequired");
        named.accounts[1].indicator = Some("-".into());
        named.accounts[1].selector = None;
        named.accounts[1].name = "someone@example.com".into();
        let note = account_sign_in_note(Some(&named), None, true).unwrap();
        assert!(!note.contains("someone@example.com"), "{note}");
        assert!(note.starts_with("s•••e@"), "{note}");
    }
}
