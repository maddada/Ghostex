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
    /// Every source file of the usage-only accounts, the CLIProxyAPI hub readers, the ACP host, the
    /// setup sign-in, and (since the Fable review's finding 9) the model lineups and pickers, the
    /// agent update check, the title and Catch me up jobs, Ghostex AI's welcome card and the agent
    /// CLI catalog.
    const SOURCES: [&str; 22] = [
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
        include_str!("../hub/launch.rs"),
        include_str!("../../agent_model_lineups.rs"),
        include_str!("../../agent_model_pickers.rs"),
        include_str!("../../agent_model_catalog.rs"),
        include_str!("../../agent_cli/latest.rs"),
        include_str!("../../server/title_generation/title_command.rs"),
        include_str!("../../session_chat_catch_up.rs"),
        include_str!("../../chief/welcome.rs"),
        include_str!("../../../../packages/shared/agent-cli-catalog.json"),
    ];

    /// The one Google address a source may name: the public download bucket agy's update manifest
    /// points at, which the update check only compares a manifest's link with and never calls
    /// (agent_cli/latest.rs).
    const ALLOWED: &str = "https://storage.googleapis.com/antigravity-public/antigravity-cli/";

    /// The one Google host Ghostex fetches from for Antigravity: agy's own auto-updater, read for
    /// this computer's update manifest (agent_cli/latest.rs). A changed or added address fails here.
    const ALLOWED_FETCH_HOST: &str = "antigravity-cli-auto-updater-974169037036.us-central1.run.app";

    /// CDXC:AgentProviders 2026-11-27 (Fable re-review, finding 9): the update check's address is
    /// agy's updater host and nothing else, and no other host on Google's app hosting appears in
    /// the sources (the word is split so this test doesn't find itself).
    #[test]
    fn the_agy_update_check_fetches_only_the_updater_host() {
        let catalog = &crate::agent_cli::catalog::CATALOG;
        let agy = catalog
            .iter()
            .find(|definition| definition.agent_id == "antigravity")
            .expect("the Antigravity entry");
        assert!(agy.npm_package.is_none());
        let url = &agy.latest_version.as_ref().expect("its update check").url;
        let host = url::Url::parse(&url.replace("{agyTarget}", "darwin_arm64"))
            .expect("an address")
            .host_str()
            .map(str::to_string);
        assert_eq!(host.as_deref(), Some(ALLOWED_FETCH_HOST), "{url}");
        let app_hosting = ["run", ".app"].concat();
        for source in SOURCES {
            let without = source.replace(ALLOWED_FETCH_HOST, "");
            assert!(!without.contains(&app_hosting), "another {app_hosting} host is named");
        }
    }

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
            let source = source.replace(ALLOWED, "");
            for word in &banned {
                assert!(!source.contains(word.as_str()), "{word} is back");
            }
        }
    }

    /// CDXC:AgentProviders 2026-11-27: nothing on a timer runs agy outside ACP (Fable review,
    /// findings 1, 5 and 9). The lineup pass leaves Antigravity out (Q5210), the picker read has no
    /// Antigravity branch, the title and Catch me up jobs don't offer it (Q5212), and Ghostex AI's
    /// welcome card doesn't read its sign-in.
    #[test]
    fn no_background_loop_runs_agy() {
        assert!(!crate::agent_model_lineups::TIMED_AGENTS.contains(&"antigravity"));
        assert!(!include_str!("../../agent_model_pickers.rs").contains("\"antigravity\""));
        let welcome = include_str!("../../chief/welcome.rs");
        let sign_in_agents = welcome
            .lines()
            .find(|line| line.starts_with("const SIGN_IN_AGENTS"))
            .expect("the welcome card's sign-in list");
        assert!(!sign_in_agents.contains("antigravity"), "{sign_in_agents}");
        use crate::server::{
            argument_title_generation_command, normalize_title_generation_agent,
        };
        assert_eq!(normalize_title_generation_agent(Some("antigravity")), "claude");
        assert_eq!(argument_title_generation_command("antigravity", "agy", "'x'"), None);
    }
}
