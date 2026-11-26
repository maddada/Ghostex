//! Saved Gemini usage accounts, retired: their Google tokens are deleted at gxserver's start, an
//! Antigravity entry takes their place in the sidebar, and one bell row says so.
//!
//! CDXC:AgentProviders 2026-11-26 DECISION:
//! User, on the Antigravity compliance page (Q5148, "delete and tell"): a Gemini account's refresh
//! token was minted for the Antigravity app's own OAuth client, so on update Ghostex deletes it
//! and tells the user once. The step runs while a Gemini entry is saved and removes every one, so
//! it runs once; the Antigravity entry it adds reads agy's own `/usage` (antigravity.rs), shown
//! where the Gemini meter was (in the sidebar if any Gemini account was, off if all were off).

use super::store::{self, Credential, UsageAccount};
use super::UsageProvider;
use crate::notification_feed::store::{
    account_notification_session_id, insert_notification_feed_row, NewNotificationFeedRow,
    ACCOUNT_NOTIFICATION_PROJECT_ID, NOTIFICATION_FEED_KIND_CUSTOM,
};
use crate::server::AppState;

/// What a client that still asks for a Gemini sign-in is told.
pub(crate) const RETIRED: &str =
    "Ghostex no longer reads Gemini usage from Google directly. Add Antigravity to see the usage of the Antigravity CLI on this computer.";

/// Deletes every saved Gemini account (and its tokens) and posts the one bell row about it.
pub(crate) fn retire_gemini_accounts(state: &AppState) {
    let Some(antigravity_id) = retire(&state.paths) else {
        return;
    };
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let row = NewNotificationFeedRow {
        project_id: ACCOUNT_NOTIFICATION_PROJECT_ID.into(),
        session_id: account_notification_session_id(&antigravity_id),
        kind: NOTIFICATION_FEED_KIND_CUSTOM,
        title: "Gemini usage account removed".into(),
        subtitle: "Antigravity".into(),
        body: "Ghostex no longer reads Gemini usage from Google directly, so it deleted your Gemini sign-in. The Antigravity meter now shows the usage of the Antigravity CLI signed in on this computer.".into(),
        agent_name: Some("antigravity".into()),
        attention_event_id: None,
    };
    if insert_notification_feed_row(&db, row).is_ok() {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
}

/// The store change: the Gemini entries go, an Antigravity entry stays or is added. The id of the
/// Antigravity entry when anything was removed.
fn retire(paths: &crate::paths::GxserverPaths) -> Option<String> {
    let _lock = store::lock();
    let mut accounts = store::read(paths).ok()?;
    let gemini: Vec<UsageAccount> = accounts
        .iter()
        .filter(|account| account.provider == UsageProvider::Gemini)
        .cloned()
        .collect();
    if gemini.is_empty() {
        return None;
    }
    accounts.retain(|account| account.provider != UsageProvider::Gemini);
    let identity = "antigravity:this-computer";
    let id = super::account_id(UsageProvider::Antigravity, identity);
    if !accounts.iter().any(|account| account.id == id) {
        accounts.push(UsageAccount {
            id: id.clone(),
            provider: UsageProvider::Antigravity,
            identity: identity.into(),
            name: "Antigravity".into(),
            email: String::new(),
            show_in_titlebar: gemini.iter().any(|account| account.show_in_titlebar),
            user_disabled: gemini.iter().all(|account| account.user_disabled),
            credential: Credential::default(),
        });
    }
    store::write(paths, &accounts).ok()?;
    for account in &gemini {
        super::forget_reading(&account.id);
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    /// Every source file of the usage-only accounts.
    const SOURCES: [&str; 9] = [
        include_str!("antigravity.rs"),
        include_str!("copilot.rs"),
        include_str!("cursor.rs"),
        include_str!("http.rs"),
        include_str!("mod.rs"),
        include_str!("retire_gemini.rs"),
        include_str!("signin.rs"),
        include_str!("store.rs"),
        include_str!("../../acp/host.rs"),
    ];

    /// CDXC:AgentProviders 2026-11-26: Ghostex never calls Google's internal services, never signs
    /// in with another app's OAuth client and never sends another app's user agent (the retired
    /// Gemini usage account did all three). The words are split so this test doesn't find itself.
    #[test]
    fn no_google_internal_endpoint_client_or_user_agent_is_left() {
        let banned = [
            ["cloudcode", "-pa"].concat(),
            ["googleapis", ".com"].concat(),
            ["retrieveUser", "QuotaSummary"].concat(),
            ["loadCode", "Assist"].concat(),
            ["fetchAvailable", "Models"].concat(),
            ["GOC", "SPX-"].concat(),
            ["apps.google", "usercontent.com"].concat(),
            ["aidev", "_client"].concat(),
            ["\"anti", "gravity/"].concat(),
            ["\"ze", "d\""].concat(),
        ];
        for source in SOURCES {
            for word in &banned {
                assert!(!source.contains(word.as_str()), "{word} is back");
            }
        }
    }
}
