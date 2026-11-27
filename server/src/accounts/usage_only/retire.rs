//! Retired usage-only accounts: saved Gemini and Antigravity entries are removed at gxserver's
//! start (a Gemini entry with its Google tokens), and one bell row says so.
//!
//! CDXC:AgentProviders 2026-11-26 DECISION:
//! User, on the Antigravity compliance page (Q5148, "delete and tell"): a Gemini account's refresh
//! token was minted for the Antigravity app's own OAuth client, so on update Ghostex deletes it
//! and tells the user once.
//!
//! CDXC:AgentProviders 2026-11-27 DECISION:
//! User: "zero internal Google API calls, zero credential scraping, and zero clientInfo spoofing
//! for Antigravity", and "Limits come back only if Antigravity exposes them over ACP." Antigravity
//! supports only ACP for external clients. The Antigravity meter that replaced Gemini's ran
//! `agy -p /usage` (Q5146 option A): not ACP, and agy asks Google's quota service for Ghostex on
//! every read. So it is gone too: a saved Antigravity entry is removed, a sign-in to it is
//! refused, and nothing reads Antigravity usage. The step runs while either entry is saved and
//! removes every one, so it runs once. Local token counts from Antigravity's own on-disk history
//! (Find, transcripts) are not affected.

use super::store::{self, UsageAccount};
use super::UsageProvider;
use crate::notification_feed::store::{
    account_notification_session_id, insert_notification_feed_row, NewNotificationFeedRow,
    ACCOUNT_NOTIFICATION_PROJECT_ID, NOTIFICATION_FEED_KIND_CUSTOM,
};
use crate::server::AppState;

/// What a client that still asks for a Gemini or Antigravity sign-in is told.
pub(crate) const RETIRED: &str =
    "Ghostex no longer reads Antigravity or Gemini usage. Antigravity offers other apps only ACP, which has no usage yet.";

/// Whether `provider` is one Ghostex no longer reads, lists or signs in to.
pub(crate) fn is_retired(provider: UsageProvider) -> bool {
    matches!(provider, UsageProvider::Gemini | UsageProvider::Antigravity)
}

/// Deletes every saved Gemini and Antigravity entry (and any tokens) and posts the one bell row.
pub(crate) fn retire_usage_accounts(state: &AppState) {
    let Some((first_id, had_gemini)) = retire(&state.paths) else {
        return;
    };
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let (title, body) = if had_gemini {
        (
            "Gemini usage account removed",
            "Ghostex no longer reads Gemini or Antigravity usage, so it deleted your Gemini sign-in. Antigravity offers other apps only ACP, which has no usage yet.",
        )
    } else {
        (
            "Antigravity usage meter removed",
            "Ghostex no longer reads Antigravity usage. Antigravity offers other apps only ACP, which has no usage yet, so the meter is gone. Antigravity chats are not affected.",
        )
    };
    let row = NewNotificationFeedRow {
        project_id: ACCOUNT_NOTIFICATION_PROJECT_ID.into(),
        session_id: account_notification_session_id(&first_id),
        kind: NOTIFICATION_FEED_KIND_CUSTOM,
        title: title.into(),
        subtitle: "Antigravity".into(),
        body: body.into(),
        agent_name: Some("antigravity".into()),
        attention_event_id: None,
    };
    if insert_notification_feed_row(&db, row).is_ok() {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
}

/// The store change: every retired entry goes. The first removed id and whether a Gemini entry
/// was among them, when anything was removed.
fn retire(paths: &crate::paths::GxserverPaths) -> Option<(String, bool)> {
    let _lock = store::lock();
    let mut accounts = store::read(paths).ok()?;
    let retired: Vec<UsageAccount> = accounts
        .iter()
        .filter(|account| is_retired(account.provider))
        .cloned()
        .collect();
    let first = retired.first()?.id.clone();
    accounts.retain(|account| !is_retired(account.provider));
    store::write(paths, &accounts).ok()?;
    for account in &retired {
        super::forget_reading(&account.id);
    }
    let had_gemini = retired
        .iter()
        .any(|account| account.provider == UsageProvider::Gemini);
    Some((first, had_gemini))
}

#[cfg(test)]
mod tests {
    /// Every source file of the usage-only accounts, the CLIProxyAPI hub readers, the ACP host and
    /// the setup sign-in.
    const SOURCES: [&str; 13] = [
        include_str!("copilot.rs"),
        include_str!("cursor.rs"),
        include_str!("http.rs"),
        include_str!("mod.rs"),
        include_str!("retire.rs"),
        include_str!("signin.rs"),
        include_str!("store.rs"),
        include_str!("../hub/client.rs"),
        include_str!("../hub/mod.rs"),
        include_str!("../hub/usage_reads.rs"),
        include_str!("../../acp/host.rs"),
        include_str!("../../acp/mod.rs"),
        include_str!("../../agent_sign_in.rs"),
    ];

    /// CDXC:AgentProviders 2026-11-26: Ghostex never calls Google's internal services, never signs
    /// in with another app's OAuth client and never sends another app's user agent (the retired
    /// Gemini usage account did all three), and never runs agy's `/usage` (2026-11-27). The words
    /// are split so this test doesn't find itself.
    #[test]
    fn no_google_internal_endpoint_client_or_user_agent_is_left() {
        let banned = [
            ["cloudcode", "-pa"].concat(),
            ["v1", "internal"].concat(),
            ["googleapis", ".com"].concat(),
            ["retrieveUser", "QuotaSummary"].concat(),
            ["loadCode", "Assist"].concat(),
            ["fetchAvailable", "Models"].concat(),
            ["GOC", "SPX-"].concat(),
            ["apps.google", "usercontent.com"].concat(),
            ["aidev", "_client"].concat(),
            ["\"anti", "gravity/"].concat(),
            ["\"ze", "d\""].concat(),
            ["\"/", "usage\""].concat(),
        ];
        for source in SOURCES {
            for word in &banned {
                assert!(!source.contains(word.as_str()), "{word} is back");
            }
        }
    }
}
