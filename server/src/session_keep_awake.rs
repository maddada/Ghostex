/*
CDXC:KeepAwake 2026-08-19:
Auto Sleep ("Sleep inactive agents") is decided by whichever Ghostex client owns
the sidebar on that machine — today the gpui desktop app's sidebar runtime. That
client can only see ITS OWN visible/focused panes, so a session a phone is
actively attached to looked exactly like an abandoned idle terminal and was
slept out from under the phone mid-conversation.

The fix is a daemon-owned keep-awake lease. A remote client (Ghostex mobile over
SSH today; any client tomorrow) tells gxserver "I am attached to these sessions"
and renews on a timer. gxserver then DECLINES automatic sleeps for a leased
session — see `sleep_trigger_is_automatic` and its caller in `zmx.rs`.

Rules this module commits to:
- Leases are in-memory and TTL-bounded. A phone that loses its connection, is
  force-quit, or goes out of battery stops renewing and the session becomes
  auto-sleepable again within one TTL. Nothing to garbage collect on restart,
  and a stale lease can never permanently pin a terminal awake.
- Leases are per holder, so two phones on the same session cannot release each
  other's hold. The session's effective expiry is the LATEST live holder expiry.
- A lease only blocks AUTOMATIC sleeps. An explicit user Sleep — from the phone,
  the sidebar, or the CLI — always wins; declining a user action would be a bug,
  not a protection.
*/

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use chrono::Utc;

/// TTL applied when a caller does not send one. Comfortably longer than the
/// renewal cadence a client should use so one dropped heartbeat does not expose
/// the session to the next auto-sleep sweep.
pub const DEFAULT_KEEP_AWAKE_TTL_MS: i64 = 180_000;

/// Upper bound on a single lease. A client that wants a longer hold renews;
/// this is what keeps a crashed client from pinning a terminal awake for hours.
pub const MAX_KEEP_AWAKE_TTL_MS: i64 = 900_000;

/// Lower bound, so a bad/zero value cannot register an already-dead lease.
pub const MIN_KEEP_AWAKE_TTL_MS: i64 = 5_000;

/// Holder used when a client does not identify itself.
pub const DEFAULT_KEEP_AWAKE_HOLDER_ID: &str = "client";

type LeaseTable = HashMap<String, HashMap<String, i64>>;

fn leases() -> &'static Mutex<LeaseTable> {
    static LEASES: OnceLock<Mutex<LeaseTable>> = OnceLock::new();
    LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lease_key(project_id: &str, session_id: &str) -> String {
    format!("{}\u{1f}{}", project_id.trim(), session_id.trim())
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn normalize_ttl_ms(requested: Option<i64>) -> i64 {
    requested
        .unwrap_or(DEFAULT_KEEP_AWAKE_TTL_MS)
        .clamp(MIN_KEEP_AWAKE_TTL_MS, MAX_KEEP_AWAKE_TTL_MS)
}

pub fn normalize_holder_id(requested: Option<&str>) -> String {
    let trimmed = requested.unwrap_or("").trim();
    if trimmed.is_empty() {
        DEFAULT_KEEP_AWAKE_HOLDER_ID.to_string()
    } else {
        // Holder ids are opaque routing keys, never rendered and never logged as
        // user content; bound the length so a malformed client cannot grow the
        // table without limit.
        trimmed.chars().take(128).collect()
    }
}

/// Record (or renew) one holder's lease. Returns the session's effective
/// expiry across every live holder.
pub fn hold(project_id: &str, session_id: &str, holder_id: &str, ttl_ms: i64) -> i64 {
    let now = now_ms();
    let expires_at = now.saturating_add(normalize_ttl_ms(Some(ttl_ms)));
    let key = lease_key(project_id, session_id);
    let mut table = leases().lock().expect("keep-awake lease table poisoned");
    let holders = table.entry(key.clone()).or_default();
    holders.insert(normalize_holder_id(Some(holder_id)), expires_at);
    holders.retain(|_, expiry| *expiry > now);
    let effective = holders.values().copied().max().unwrap_or(expires_at);
    if holders.is_empty() {
        table.remove(&key);
    }
    effective
}

/// Drop one holder's lease. Other holders keep theirs.
pub fn release(project_id: &str, session_id: &str, holder_id: &str) {
    let now = now_ms();
    let key = lease_key(project_id, session_id);
    let mut table = leases().lock().expect("keep-awake lease table poisoned");
    let Some(holders) = table.get_mut(&key) else {
        return;
    };
    holders.remove(&normalize_holder_id(Some(holder_id)));
    holders.retain(|_, expiry| *expiry > now);
    if holders.is_empty() {
        table.remove(&key);
    }
}

/// The session's live keep-awake expiry, or `None` when nothing holds it awake.
/// Expired entries are pruned on read so the table cannot grow unbounded from
/// clients that never call `release`.
pub fn expires_at_ms(project_id: &str, session_id: &str) -> Option<i64> {
    let now = now_ms();
    let key = lease_key(project_id, session_id);
    let mut table = leases().lock().expect("keep-awake lease table poisoned");
    let holders = table.get_mut(&key)?;
    holders.retain(|_, expiry| *expiry > now);
    let effective = holders.values().copied().max();
    if holders.is_empty() {
        table.remove(&key);
    }
    effective
}

pub fn is_held_awake(project_id: &str, session_id: &str) -> bool {
    expires_at_ms(project_id, session_id).is_some()
}

/*
Only an automatic sweep is declined. `sleepTrigger` is the explicit contract
field; a request that omits it is a user action, which is also what every
pre-existing caller sends.
*/
pub fn sleep_trigger_is_automatic(sleep_trigger: Option<&str>) -> bool {
    sleep_trigger
        .map(|value| value.trim().eq_ignore_ascii_case("automatic"))
        .unwrap_or(false)
}
