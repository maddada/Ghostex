use super::*;

// ---------------------------------------------------------------------------
// Watchdog notice store (in memory only — never persisted, never in settings)
// ---------------------------------------------------------------------------

/*
CDXC:AgentScreenDetection 2026-08-19:
Watchdog notices (a send that could not be proven delivered) live here rather
than in the session registry: they describe a moment, not durable state, and a
daemon restart must not resurrect one. Keyed exactly like the send queues so
both halves of the feature address a session the same way.
*/
struct StoredWatchdogNotice {
    notice: SessionChatTerminalNotice,
    stored_at: Instant,
}

/*
CDXC:AgentScreenDetection 2026-08-19:
Retirement backstop. A watchdog notice is normally retired by the next send or
by a later verification, but a session nobody touches again would otherwise keep
one forever — and "your message from an hour ago never arrived" is noise, not
news. Expiry is checked lazily on read (there is no sweeper task), so an expired
entry is indistinguishable from an absent one for every consumer.
*/
const WATCHDOG_NOTICE_MAX_AGE: Duration = Duration::from_secs(600);

fn watchdog_notices() -> &'static Mutex<HashMap<String, StoredWatchdogNotice>> {
    static NOTICES: OnceLock<Mutex<HashMap<String, StoredWatchdogNotice>>> = OnceLock::new();
    NOTICES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn session_chat_notice_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

pub fn set_session_chat_watchdog_notice(
    project_id: &str,
    session_id: &str,
    mut notice: SessionChatTerminalNotice,
) {
    if let Ok(mut notices) = watchdog_notices().lock() {
        let key = session_chat_notice_key(project_id, session_id);
        /*
        CDXC:AgentScreenDetection 2026-08-19:
        Re-publishing the SAME verdict keeps the instance the client already
        knows: its `detectedAt` is that client's dismissal key, and the stored
        age is what expires the instance, so neither may be reset by a repeat.
        */
        let stored_at = match notices.get(&key) {
            Some(stored) if notice.same_notice(Some(&stored.notice)) => {
                notice.carry_forward_detected_at(Some(&stored.notice));
                stored.stored_at
            }
            _ => Instant::now(),
        };
        notices.insert(key, StoredWatchdogNotice { notice, stored_at });
    }
}

pub fn session_chat_watchdog_notice(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let mut notices = watchdog_notices().lock().ok()?;
    let key = session_chat_notice_key(project_id, session_id);
    let stored = notices.get(&key)?;
    if stored.stored_at.elapsed() >= WATCHDOG_NOTICE_MAX_AGE {
        notices.remove(&key);
        return None;
    }
    Some(stored.notice.clone())
}

/// Retires a watchdog notice. Returns the notice that was showing, so the
/// caller knows whether a clearing state frame is owed.
pub fn clear_session_chat_watchdog_notice(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let mut notices = watchdog_notices().lock().ok()?;
    let stored = notices.remove(&session_chat_notice_key(project_id, session_id))?;
    // An entry that had already expired was invisible to every reader, so
    // removing it here is not a change anybody is owed a frame for.
    (stored.stored_at.elapsed() < WATCHDOG_NOTICE_MAX_AGE).then_some(stored.notice)
}

/*
CDXC:AgentScreenDetection 2026-08-19:
Clean-screen retirement. A watchdog verdict about SCREEN state (the login
screen, the trust dialog, the crashed CLI, a queued input) is only true while
that screen is up, so the next capture that succeeds whole and classifies to
nothing is proof the state is over and the card must go. `deliveryFailed` is
deliberately exempt: it describes a message that was lost in the past, not
anything currently painted, so a clean screen says nothing about it and it keeps
its own retirement rules (the next send, a later verification, expiry).

Returns the notice that was retired, so a caller can tell whether a clearing
frame is owed; an already-expired entry was invisible to every reader and
therefore reports nothing.
*/
pub fn retire_session_chat_watchdog_notice_on_clean_screen(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let mut notices = watchdog_notices().lock().ok()?;
    let key = session_chat_notice_key(project_id, session_id);
    if notices.get(&key).is_none_or(|stored| {
        // `apiRefusal` shares the exemption: it too describes a past event
        // that no screen capture can confirm or deny.
        stored.notice.kind == SESSION_CHAT_NOTICE_DELIVERY_FAILED
            || stored.notice.kind == SESSION_CHAT_NOTICE_API_REFUSAL
    }) {
        return None;
    }
    let stored = notices.remove(&key)?;
    (stored.stored_at.elapsed() < WATCHDOG_NOTICE_MAX_AGE).then_some(stored.notice)
}

/*
A watchdog notice normally wins: it is both fresher and more specific than
whatever the screen classifier read at the same moment.

CDXC:SessionChat 2026-08-21: an ANSWERABLE screen notice is the
one exception, and it is not a close call. A watchdog notice reports a PAST
event ("your message could not be proven delivered") and its only advice is to
go look at the terminal; an answerable picker is the LIVE state that most
likely caused that event, and it can be resolved from the chat surface in one
click. Letting the past-event card mask it left the user staring at a
delivery-failed banner with the picker sitting unanswered on screen — the exact
dead end this feature exists to remove. `deliveryFailed` in particular is
exempt from clean-screen retirement, so it would have masked the picker for its
full 10-minute lifetime.
*/
pub fn merge_session_chat_terminal_notices(
    watchdog: Option<SessionChatTerminalNotice>,
    screen: Option<SessionChatTerminalNotice>,
) -> Option<SessionChatTerminalNotice> {
    if let Some(screen) = screen {
        if screen.is_answerable() {
            return Some(screen);
        }
        return watchdog.or(Some(screen));
    }
    watchdog
}

/// CDXC:AgentProviders 2026-09-07 DECISION:
/// After an account switch, hide the usage-limit message already shown for the previous login. Only a different message may appear; timestamps and repeated screen captures do not make it new.
/// CDXC:AgentProviders 2026-09-11 WHY:
/// The same wording is what the new login prints when it runs out too, so identity alone hid every later Fable limit on the account the session was switched to, and the switch pass never saw it.
/// The suppression therefore carries the switch time; `session_chat_notice_progress::refresh` lifts it when the transcript records a usage limit after that time, which is a new event, not a repaint.
/// Superseding the identity match of the same day: the resumed CLI repaints the old limit from its transcript with different glyphs and wording ("⏺ … esc or type to cancel" for the "⚠ … esc to cancel" spinner line), so an identity captured before the switch never matched the replay.
/// The card came back after the switch, blocked the composer with "Clear it in the terminal before sending", and held the continuation dot. The suppression is now by kind: every usage-limit notice stays hidden until the transcript proves a new limit.
/// SEE-ALSO: server/src/accounts/endpoint.rs (select), server/src/accounts/recovery.rs (restore_session), server/src/session_chat_options/detector.rs (detect_blocking, composer readiness).
fn suppressed_account_usage_notices(
) -> &'static Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>> {
    static NOTICES: OnceLock<Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>> =
        OnceLock::new();
    NOTICES.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(crate) fn suppress_account_usage_notice(
    project_id: &str,
    session_id: &str,
    since: chrono::DateTime<chrono::Utc>,
) {
    if let Ok(mut notices) = suppressed_account_usage_notices().lock() {
        notices.insert(session_chat_notice_key(project_id, session_id), since);
    }
}
/// The switch time usage-limit notices have been hidden since, when a switch is being hidden.
pub(crate) fn account_usage_notice_suppression(
    project_id: &str,
    session_id: &str,
) -> Option<chrono::DateTime<chrono::Utc>> {
    suppressed_account_usage_notices()
        .lock()
        .ok()
        .and_then(|notices| {
            notices
                .get(&session_chat_notice_key(project_id, session_id))
                .copied()
        })
}
pub(crate) fn lift_account_usage_notice_suppression(project_id: &str, session_id: &str) {
    if let Ok(mut notices) = suppressed_account_usage_notices().lock() {
        notices.remove(&session_chat_notice_key(project_id, session_id));
    }
}
/// Whether an account switch is hiding this notice: only usage limits and a disabled account's error are hidden, the two notices a resumed CLI repaints from the previous login, and only while the switch suppression is in place.
pub(crate) fn account_usage_notice_suppressed(
    project_id: &str,
    session_id: &str,
    notice: &SessionChatTerminalNotice,
) -> bool {
    (notice.kind == SESSION_CHAT_NOTICE_USAGE_LIMIT || notice.account_disabled())
        && account_usage_notice_suppression(project_id, session_id).is_some()
}

/// Store lookup and merge for read/frame paths holding a screen classification.
/// A notice the cache keeps but no client may see: a usage notice an account
/// switch is hiding, a trust prompt on a remembered folder that the
/// detection funnel is answering itself, or Claude's Bypass Permissions
/// warning while Ghostex accepts it (session_chat_claude_bypass_consent.rs). The cache keeps it so the send path
/// and the queue still treat the screen as blocked until it is gone.
pub fn session_chat_notice_hidden(
    project_id: &str,
    session_id: &str,
    notice: &SessionChatTerminalNotice,
) -> bool {
    notice.auto_trust
        || account_usage_notice_suppressed(project_id, session_id, notice)
        || crate::session_chat_claude_bypass_consent::bypass_consent_being_answered(
            project_id, session_id, notice,
        )
}

pub fn resolve_session_chat_terminal_notice(
    project_id: &str,
    session_id: &str,
    screen: Option<SessionChatTerminalNotice>,
) -> Option<SessionChatTerminalNotice> {
    let visible = |notice: &SessionChatTerminalNotice| {
        !session_chat_notice_hidden(project_id, session_id, notice)
            && crate::session_chat_notice_progress::visible(project_id, session_id, notice)
    };
    let screen = screen.filter(visible);
    let failed_send = crate::session_chat_send_failure::send_failure_card(project_id, session_id);
    // A failed send is the card unless the agent's screen says something only it can answer (a
    // sign-in, a usage limit, a dialog), which comes first; the message keeps its reason on its own
    // row meanwhile. An agent that exited is what the failed send's card already says, with the
    // way out, and a card saying a new message waits would contradict it.
    let screen_says_more = screen.as_ref().is_some_and(|notice| {
        notice.kind != SESSION_CHAT_NOTICE_AGENT_EXITED && !notice.start_failed()
    });
    match failed_send.filter(|_| !screen_says_more) {
        Some(card) => Some(card),
        None => merge_session_chat_terminal_notices(
            session_chat_watchdog_notice(project_id, session_id).filter(visible),
            screen,
        ),
    }
    .map(|mut notice| {
        if notice.kind == SESSION_CHAT_NOTICE_USAGE_LIMIT {
            notice.account_wait = crate::accounts::limit_wait::notice_wait(project_id, session_id);
        }
        notice
    })
}
