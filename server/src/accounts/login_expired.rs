//! CDXC:AgentProviders 2026-10-26 DECISION:
//! User: an account must show as needing sign-in once one of its sessions reports "Login expired · Please run /login", in Settings > Accounts, `ghostex accounts` and wherever new sessions pick an account, so nothing new starts on it until it is signed in again. Its state used to come only from claude-swap's reading of the saved login (`usageStatus`), which never sees a running session's own login fail: on 2026-10-26 four sessions on account 4 stopped on "Login expired" while the account still read "ready". The recovery pass now marks the account (`login_expired_at`), discovery reports a marked account as `loginRequired`, and no automatic rule, pinned rule or account switch picks it. A sign-in through Ghostex (Settings > Accounts' Reconnect, the chat card's Reconnect) clears it, and so do a real reply from a session on the account and a claude-swap usage reading of its saved login taken afterwards, which prove the login works again (on 2026-10-26 account 4 was signed in again outside Ghostex two minutes after its sessions failed).
//! WHY: As with a disabled organization (disabled.rs), the screen alone cannot say which login failed, because a resumed CLI repaints the previous login's error; only a transcript error recorded after the session moved to its current account, under that account's own login, with no reply after it, marks the account. An error from before the account last signed in never marks it again. Claude only for now: Ghostex reads Codex's sign-in errors only from its screen, which a resumed Codex can repaint.
//! SEE-ALSO: helpers.rs (`discover_row`), store.rs (`login_expired`), default_account.rs, model.rs (`auto_switchable`), setup.rs (clears on sign-in), session_chat_account_sign_in.rs (the card's Reconnect).

use super::{disabled, model::*, store};
use crate::{
    domain::DomainStateError,
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

/// What Settings > Accounts and `ghostex accounts` say about a marked account.
pub(crate) const NOTICE: &str =
    "A session on this account reported that its login expired. Reconnect it to use it again.";

/// Whether a Claude transcript record is the API refusing the session's login as expired or
/// revoked ("Login expired · Please run /login", `authentication_failed`).
pub(crate) fn transcript_record_login_expired(record: &Value) -> bool {
    record["isApiErrorMessage"] == true
        && (record["error"] == "authentication_failed"
            || record["message"]["content"]
                .as_array()
                .is_some_and(|blocks| {
                    blocks.iter().any(|block| {
                        block["text"]
                            .as_str()
                            .is_some_and(|text| text.contains("Please run /login"))
                    })
                }))
}

/// The newest login error and the newest real reply in a Claude transcript tail, in milliseconds.
#[derive(Debug, Default, PartialEq, Eq)]
struct TranscriptLogin {
    expired_at: Option<i64>,
    replied_at: Option<i64>,
}

fn read_transcript(text: &str) -> TranscriptLogin {
    let mut found = TranscriptLogin::default();
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record["isSidechain"] == true {
            continue;
        }
        let Some(timestamp) = crate::session_chat::parse_timestamp(record.get("timestamp")) else {
            continue;
        };
        if transcript_record_login_expired(&record) {
            found.expired_at = Some(timestamp);
        } else if record["type"] == "assistant"
            && record["isApiErrorMessage"] != true
            && record["message"]["model"]
                .as_str()
                .is_some_and(|model| model != "<synthetic>")
        {
            found.replied_at = Some(timestamp);
        }
    }
    found
}

#[derive(Debug, PartialEq, Eq)]
enum Change {
    /// Mark the account, as of the error's own time.
    Mark(i64),
    Clear,
}

/// What one session's transcript says about its account's login. `since` is when the session
/// moved to the account, `marked_at` the account's mark, `confirmed_at` when its saved login was
/// last known to work (a sign-in, a reply after a mark, or a good usage reading of it), and `card`
/// whether the session shows the sign-in error card now.
fn decide(
    transcript: &TranscriptLogin,
    since: i64,
    marked_at: Option<i64>,
    confirmed_at: Option<i64>,
    card: bool,
) -> Option<Change> {
    if let Some(marked) = marked_at {
        return (transcript
            .replied_at
            .is_some_and(|replied| replied > marked)
            || confirmed_at.is_some_and(|confirmed| confirmed > marked))
        .then_some(Change::Clear);
    }
    let expired = transcript.expired_at?;
    (card
        && expired > since
        && confirmed_at.is_none_or(|confirmed| expired > confirmed)
        && transcript
            .replied_at
            .is_none_or(|replied| replied < expired))
    .then_some(Change::Mark(expired))
}

/// Whether claude-swap read the saved login's usage after `marked`, so that login works: its own
/// status `ok`, a live reading (not the last good one kept through a failure), fetched later.
pub(crate) fn read_after(status: &str, live: bool, fetched_at: Option<&str>, marked: &str) -> bool {
    status == "ok"
        && live
        && millis(fetched_at)
            .zip(millis(Some(marked)))
            .is_some_and(|(fetched, marked)| fetched > marked)
}

/// When claude-swap last read account `id`'s usage with its saved login, if that reading worked.
fn reading_at(id: &str) -> Option<i64> {
    super::runtime::current_snapshot()
        .accounts
        .iter()
        .find(|account| super::endpoint::account_id(account) == id)
        .filter(|account| {
            account.status == "ready" && account.usage_error.is_none() && !account.usage_stale
        })
        .and_then(|account| millis(account.usage_updated_at.as_deref()))
}

/// Whether `account`'s saved login is proven to work after `since` (milliseconds): it signed in
/// through Ghostex or was seen working again (`login_confirmed_at`), or claude-swap read its usage
/// with it, after then. Reconnect restarts without a browser sign-in only then.
pub(crate) fn proven_since(account: &SavedAccount, since: i64) -> bool {
    proven(
        millis(account.login_confirmed_at.as_deref()),
        reading_at(&account.id),
        since,
    )
}

fn proven(confirmed_at: Option<i64>, read_at: Option<i64>, since: i64) -> bool {
    confirmed_at.max(read_at).is_some_and(|at| at > since)
}

fn millis(value: Option<&str>) -> Option<i64> {
    value
        .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
        .map(|time| time.timestamp_millis())
}

fn transcript_tail(session: &Value) -> Option<String> {
    let agent = crate::session_chat::resolve_session_chat_transcript_agent(Some("claude"))?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str),
        session
            .pointer("/runtimeSettings/agentSessionPath")
            .and_then(Value::as_str),
    )?;
    crate::session_chat_options::transcript_tail_text(&path).ok()
}

/// Recovery-pass step for one running session: marks its account when the session shows the
/// sign-in error card for a login that failed on that account, and clears the mark once a session
/// on the account answers again. Never owns the session; the other recovery steps still run.
pub(crate) fn observe(
    state: &AppState,
    db: &rusqlite::Connection,
    old: &Value,
    provider: Provider,
) -> Result<(), DomainStateError> {
    let (Some(pid), Some(sid)) = (old["projectId"].as_str(), old["sessionId"].as_str()) else {
        return Ok(());
    };
    if provider != Provider::Claude
        || super::launch::session_uses_own_login(provider, old)
        || old["lifecycleState"].as_str() != Some("running")
        || disabled::switch_in_progress(old)
    {
        return Ok(());
    }
    let registry = store::read(db)?;
    let Some(id) = disabled::current_account(state, &registry, old) else {
        return Ok(());
    };
    let Some(account) = registry.accounts.iter().find(|a| a.id == id) else {
        return Ok(());
    };
    let marked_at = millis(account.login_expired_at.as_deref());
    let card = crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid)
        .is_some_and(|notice| notice.sign_in_error());
    // Reading the transcript is the costly part: only a marked account or a card needs it.
    if marked_at.is_none() && !card || disabled::written_by_other_login(state, &registry, old, &id)
    {
        return Ok(());
    }
    let Some(text) = transcript_tail(old) else {
        return Ok(());
    };
    let since = millis(
        old.pointer("/runtimeSettings/accountSuppressedUsageNoticeAt")
            .and_then(Value::as_str),
    )
    .unwrap_or(i64::MIN);
    let confirmed_at = millis(account.login_confirmed_at.as_deref()).max(reading_at(&id));
    let Some(change) = decide(
        &read_transcript(&text),
        since,
        marked_at,
        confirmed_at,
        card,
    ) else {
        return Ok(());
    };
    let _gate = state.accounts.mutation_gate();
    let mut registry = store::read(db)?;
    let Some(account) = registry.accounts.iter_mut().find(|a| a.id == id) else {
        return Ok(());
    };
    let event = match change {
        Change::Mark(at) => {
            if account.login_expired_at.is_some() {
                return Ok(());
            }
            account.login_expired_at = DateTime::<Utc>::from_timestamp_millis(at)
                .map(|at| at.to_rfc3339())
                .or_else(|| Some(Utc::now().to_rfc3339()));
            "accountLoginExpired"
        }
        Change::Clear => {
            account.login_expired_at = None;
            account.login_confirmed_at = Some(Utc::now().to_rfc3339());
            "accountLoginWorksAgain"
        }
    };
    let name = account.name.clone();
    store::write(db, &registry)?;
    state.accounts.invalidate();
    log(
        state,
        event,
        json!({ "accountId": id, "name": name, "projectId": pid, "sessionId": sid }),
    );
    Ok(())
}

/// Clears the mark of every account whose saved login claude-swap has read successfully since,
/// whether or not a session on it is running. Runs at the start of each recovery pass.
pub(crate) fn heal(state: &AppState, db: &rusqlite::Connection) -> Result<(), DomainStateError> {
    let registry = store::read(db)?;
    let healed: Vec<String> = registry
        .accounts
        .iter()
        .filter(|account| {
            millis(account.login_expired_at.as_deref())
                .zip(reading_at(&account.id))
                .is_some_and(|(marked, read)| read > marked)
        })
        .map(|account| account.id.clone())
        .collect();
    if healed.is_empty() {
        return Ok(());
    }
    let _gate = state.accounts.mutation_gate();
    let mut registry = store::read(db)?;
    for account in registry
        .accounts
        .iter_mut()
        .filter(|account| healed.contains(&account.id))
    {
        account.login_expired_at = None;
        account.login_confirmed_at = Some(Utc::now().to_rfc3339());
    }
    store::write(db, &registry)?;
    state.accounts.invalidate();
    log(
        state,
        "accountLoginWorksAgain",
        json!({ "accountIds": healed }),
    );
    Ok(())
}

/// The account in `provider`'s slot `selector` signed in again through Ghostex (setup.rs): it no
/// longer needs signing in, and the errors its sessions showed before this never mark it again.
pub(crate) fn signed_in(
    state: &AppState,
    db: &rusqlite::Connection,
    provider: Provider,
    selector: &str,
) -> Result<(), DomainStateError> {
    let _gate = state.accounts.mutation_gate();
    let mut registry = store::read(db)?;
    let mut cleared = false;
    for account in registry
        .accounts
        .iter_mut()
        .filter(|a| a.provider == provider && a.selector == selector)
    {
        cleared |= account.login_expired_at.take().is_some();
        account.login_confirmed_at = Some(Utc::now().to_rfc3339());
    }
    store::write(db, &registry)?;
    state.accounts.invalidate();
    if cleared {
        log(
            state,
            "accountLoginSignedIn",
            json!({ "provider": provider, "selector": selector }),
        );
    }
    Ok(())
}

fn log(state: &AppState, event: &str, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Info,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expired(at: &str) -> String {
        json!({
            "type": "assistant", "timestamp": at, "isApiErrorMessage": true,
            "error": "authentication_failed",
            "message": {"model": "<synthetic>", "content": [{"type": "text", "text": "Login expired · Please run /login"}]}
        })
        .to_string()
    }

    fn reply(at: &str) -> String {
        json!({
            "type": "assistant", "timestamp": at,
            "message": {"model": "claude-opus-5-5", "content": [{"type": "text", "text": "Done."}]}
        })
        .to_string()
    }

    fn ms(at: &str) -> i64 {
        millis(Some(at)).unwrap()
    }

    #[test]
    fn reads_the_login_error_claude_wrote_on_2026_10_26() {
        // The record G4zxg's own transcript holds, trimmed to the fields the check reads.
        let record: Value = serde_json::from_str(r#"{"isSidechain":false,"type":"assistant","timestamp":"2026-10-26T12:24:56.460Z","message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"Login expired · Please run /login"}]},"error":"authentication_failed","isApiErrorMessage":true}"#).unwrap();
        assert!(transcript_record_login_expired(&record));
        let other_error = json!({"isApiErrorMessage": true, "error": "rate_limit", "message": {"content": [{"type": "text", "text": "API Error: 529"}]}});
        assert!(!transcript_record_login_expired(&other_error));
        let typed = json!({"type": "user", "message": {"content": "Please run /login"}});
        assert!(!transcript_record_login_expired(&typed));
    }

    #[test]
    fn a_login_error_after_the_last_reply_marks_the_account() {
        let text = [
            reply("2026-10-26T12:20:00Z"),
            expired("2026-10-26T12:24:56Z"),
        ]
        .join("\n");
        let transcript = read_transcript(&text);
        assert_eq!(
            decide(&transcript, i64::MIN, None, None, true),
            Some(Change::Mark(ms("2026-10-26T12:24:56Z")))
        );
        // Without the card the screen no longer shows the error (the agent was restarted or the
        // user moved on), so the transcript alone marks nothing.
        assert_eq!(decide(&transcript, i64::MIN, None, None, false), None);
    }

    #[test]
    fn a_reply_after_the_error_or_an_error_from_before_marks_nothing() {
        let answered = [
            expired("2026-10-26T12:24:56Z"),
            reply("2026-10-26T12:30:00Z"),
        ]
        .join("\n");
        assert_eq!(
            decide(&read_transcript(&answered), i64::MIN, None, None, true),
            None
        );
        let text = expired("2026-10-26T12:24:56Z");
        let transcript = read_transcript(&text);
        // The session moved to this account after the error: the error was another login's.
        assert_eq!(
            decide(&transcript, ms("2026-10-26T12:27:00Z"), None, None, true),
            None
        );
        // The account signed in again after the error; the repainted error is old news.
        assert_eq!(
            decide(
                &transcript,
                i64::MIN,
                None,
                Some(ms("2026-10-26T12:27:06Z")),
                true
            ),
            None
        );
    }

    #[test]
    fn a_good_reading_of_the_saved_login_after_the_error_outranks_it() {
        // 2026-10-26: account 4's sessions failed at 12:24:40, and claude-swap read the saved
        // login's usage at 12:27:06 after it signed in again outside Ghostex.
        let marked = "2026-10-26T12:24:40.500+00:00";
        assert!(read_after("ok", true, Some("2026-10-26T12:27:06Z"), marked));
        assert!(!read_after(
            "ok",
            true,
            Some("2026-10-26T12:20:00Z"),
            marked
        ));
        assert!(!read_after(
            "ok",
            false,
            Some("2026-10-26T12:27:06Z"),
            marked
        ));
        assert!(!read_after(
            "token_expired",
            true,
            Some("2026-10-26T12:27:06Z"),
            marked
        ));
        assert!(!read_after("ok", true, None, marked));
        let text = expired("2026-10-26T12:24:40.500Z");
        let transcript = read_transcript(&text);
        let read = Some(ms("2026-10-26T12:27:06Z"));
        assert_eq!(decide(&transcript, i64::MIN, None, read, true), None);
        assert_eq!(
            decide(&transcript, i64::MIN, Some(ms(marked)), read, true),
            Some(Change::Clear)
        );
    }

    #[test]
    fn only_a_sign_in_or_a_reading_after_the_error_proves_the_saved_login() {
        let error = ms("2026-10-26T12:24:38Z");
        // Nothing since the error: Reconnect opens the browser sign-in.
        assert!(!proven(None, None, error));
        assert!(!proven(
            Some(ms("2026-10-26T12:00:00Z")),
            Some(ms("2026-10-26T12:20:00Z")),
            error
        ));
        // A sign-in (or the account seen working) after the error: Reconnect only restarts.
        assert!(proven(Some(ms("2026-10-26T12:27:06Z")), None, error));
        // A good usage reading of the saved login after the error does too.
        assert!(proven(None, Some(ms("2026-10-26T12:27:06Z")), error));
    }

    #[test]
    fn a_reply_after_the_mark_clears_it() {
        let marked = Some(ms("2026-10-26T12:24:56Z"));
        let still_failing = expired("2026-10-26T12:24:56Z");
        assert_eq!(
            decide(
                &read_transcript(&still_failing),
                i64::MIN,
                marked,
                None,
                true
            ),
            None
        );
        let working = [
            expired("2026-10-26T12:24:56Z"),
            reply("2026-10-26T12:40:00Z"),
        ]
        .join("\n");
        assert_eq!(
            decide(&read_transcript(&working), i64::MIN, marked, None, false),
            Some(Change::Clear)
        );
    }
}
