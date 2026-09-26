/*
CDXC:AgentScreenDetection 2026-09-26 WHY:
An idle followed session re-reads its screen on the 30s tier, so a dialog the
user opens in the terminal (Claude's "Switch model?" after picking a row in
`/model`) reached the chat up to 30 seconds late, measured at 29s, and a dialog
answered in the terminal left its card standing just as long. Nothing else
announces such a screen: no hook fires, the transcript records nothing, and the
statusline only changes once the switch is confirmed. The full probe cannot
simply run every second while idle, because it also reads the transcript tail
(up to 6 MB) and the statusline and task stores. So each idle tick takes the
live grid alone, with no scrollback, and the reconcile loop runs the full probe
only when that grid differs from the previous look.
*/

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// True when the session's live terminal grid changed since the previous call.
/// The first look only seeds, because the probe that started the follower
/// already read that screen.
pub type SessionChatScreenChangeWatch = Arc<dyn Fn() -> bool + Send + Sync>;

/// A look is one socket read of one screenful; a daemon that has not answered
/// by then is left to the steady probe.
const SESSION_CHAT_SCREEN_LOOK_DEADLINE: Duration = Duration::from_secs(2);

/// Windows reaches its daemons through a spawned process per capture, which is
/// too heavy to pay every second for an idle session, so it keeps the steady
/// tier alone.
#[cfg(unix)]
pub(crate) fn session_chat_screen_change_watch(
    zmx_name: String,
) -> Option<SessionChatScreenChangeWatch> {
    let observed: Mutex<Option<u64>> = Mutex::new(None);
    Some(Arc::new(move || {
        // A look still waiting on a slow daemon owns the lock; skipping keeps
        // stalled reads from piling up one per tick.
        let Ok(mut observed) = observed.try_lock() else {
            return false;
        };
        let Ok(capture) = crate::zmx::read_zmx_session_grid_capture(&zmx_name) else {
            return false;
        };
        let mut hasher = DefaultHasher::new();
        capture.text.hash(&mut hasher);
        let fingerprint = hasher.finish();
        let changed = observed.is_some_and(|previous| previous != fingerprint);
        *observed = Some(fingerprint);
        changed
    }))
}

#[cfg(not(unix))]
pub(crate) fn session_chat_screen_change_watch(
    _zmx_name: String,
) -> Option<SessionChatScreenChangeWatch> {
    None
}

/// Takes one look off the async reconcile loop. `false` when there is no watch,
/// the screen is unchanged, or the daemon did not answer in time.
pub(crate) async fn session_chat_screen_changed(
    watch: Option<&SessionChatScreenChangeWatch>,
) -> bool {
    let Some(watch) = watch.cloned() else {
        return false;
    };
    matches!(
        tokio::time::timeout(
            SESSION_CHAT_SCREEN_LOOK_DEADLINE,
            tokio::task::spawn_blocking(move || watch()),
        )
        .await,
        Ok(Ok(true))
    )
}
