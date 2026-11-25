//! Launch tokens: what an app's own calls (`/app/<id>/bridge`) must carry.
//!
//! CDXC:GhostexApps 2026-11-25 WHY:
//! A Live page's only grant is its unguessable id, and an app's id is printed into the chat that made it, so the id alone must not let anyone use an app's data or powers. Ghostex mints a token when it opens the app itself (`/api/openGhostexApp`, refused to agents' session tokens) and hands it to the page in the URL fragment, which no request, log or Referer carries. gxserver keeps tokens only in memory: a restart, the app's delete, or turning Ghostex Apps off drops them, and a token unused for 12 hours expires. The app's own script can read its token, which is fine: it only opens that app's own bridge.
//!
//! CDXC:GhostexApps 2026-11-26 WHY:
//! Fable's security review (finding 7): the 12 hours were counted from the last call, so a polling app kept its token for good, and a token outlived turning Ghostex Apps off and on and a new version of the app. Now a token lasts at most 12 hours from its mint and 2 hours unused, a publish drops the app's tokens (publish.rs), and the settings watch drops every token the moment it reads the switch off (server/shared_settings_watch.rs), besides the check on every call. Ghostex opens the app again for a fresh one.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use rand::TryRng;

/// A token stops working this long after it was minted, however much it is used.
const LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
/// And this long after its last use.
const IDLE_EXPIRY: Duration = Duration::from_secs(2 * 60 * 60);
/// At most this many live tokens per app (each open of the app makes one); the oldest goes first.
const PER_APP: usize = 16;

struct Entry {
    app_id: String,
    minted: Instant,
    last_used: Instant,
}

static TOKENS: Mutex<Option<HashMap<String, Entry>>> = Mutex::new(None);

fn with_tokens<T>(run: impl FnOnce(&mut HashMap<String, Entry>) -> T) -> T {
    let mut guard = TOKENS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let tokens = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    tokens.retain(|_, entry| live(entry, now));
    run(tokens)
}

fn live(entry: &Entry, now: Instant) -> bool {
    now.duration_since(entry.minted) < LIFETIME && now.duration_since(entry.last_used) < IDLE_EXPIRY
}

/// A new token for `app_id`: 128 random bits as hex, prefixed so it is recognisable in a report.
pub(super) fn mint(app_id: &str) -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system's random generator failed");
    let token = format!(
        "gxa_{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    with_tokens(|tokens| {
        let mut mine: Vec<(String, Instant)> = tokens
            .iter()
            .filter(|(_, entry)| entry.app_id == app_id)
            .map(|(token, entry)| (token.clone(), entry.minted))
            .collect();
        if mine.len() >= PER_APP {
            mine.sort_by_key(|(_, minted)| *minted);
            for (old, _) in mine.iter().take(mine.len() + 1 - PER_APP) {
                tokens.remove(old);
            }
        }
        let now = Instant::now();
        tokens.insert(
            token.clone(),
            Entry {
                app_id: app_id.to_string(),
                minted: now,
                last_used: now,
            },
        );
    });
    token
}

/// Whether `token` was minted for `app_id` and is still live; a match counts as a use.
pub(super) fn check(app_id: &str, token: &str) -> bool {
    if !token.starts_with("gxa_") {
        return false;
    }
    with_tokens(|tokens| match tokens.get_mut(token) {
        Some(entry) if entry.app_id == app_id => {
            entry.last_used = Instant::now();
            true
        }
        _ => false,
    })
}

/// Drops every token of `app_id` (the app was deleted or published again).
pub(super) fn revoke_app(app_id: &str) {
    with_tokens(|tokens| tokens.retain(|_, entry| entry.app_id != app_id));
}

/// Drops every token (Ghostex Apps was turned off).
pub(super) fn revoke_all() {
    with_tokens(HashMap::clear);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_opens_only_its_own_app_until_revoked() {
        let app = "a".repeat(32);
        let other = "b".repeat(32);
        let token = mint(&app);
        assert!(check(&app, &token));
        assert!(!check(&other, &token));
        assert!(!check(&app, "gxa_not-a-token"));
        assert!(!check(&app, ""));
        revoke_app(&app);
        assert!(!check(&app, &token));
    }

    #[test]
    fn keeps_a_bounded_number_per_app() {
        let app = "c".repeat(32);
        let first = mint(&app);
        for _ in 0..PER_APP {
            mint(&app);
        }
        assert!(!check(&app, &first));
    }

    #[test]
    fn use_does_not_keep_a_token_past_its_lifetime() {
        let now = Instant::now();
        let Some(old) = now.checked_sub(LIFETIME + Duration::from_secs(1)) else {
            return;
        };
        let entry = Entry {
            app_id: String::new(),
            minted: old,
            last_used: now,
        };
        assert!(!live(&entry, now));
        let idle = Entry {
            app_id: String::new(),
            minted: now,
            last_used: now,
        };
        assert!(live(&idle, now));
        assert!(!live(&idle, now + IDLE_EXPIRY));
    }
}
