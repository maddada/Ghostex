//! CDXC:AgentProviders 2026-11-01 DECISION:
//! User: "Yes, build it" (automatic account restore). When a saved Claude account reads Needs sign-in or has no saved login, but this computer still holds a working copy of that login, Ghostex puts it back on its own instead of failing the session: once when the accounts list reads the account that way, and once before a start, wake, resume or account switch would run on it. Never in a loop: a copy that was put back and still reads signed out is not tried again, and the account keeps its usual message plus one sentence saying whether a copy existed.
//! CDXC:AgentProviders 2026-11-01 WHY:
//! A cswap slot keeps two copies of its login: the saved copy (the `claude-swap` Keychain item cswap calls the backup) and the slot's session profile (`sessions/<N>-<email>`), where `cswap run <N>` starts Claude. Claude refreshes the profile's copy and nothing writes it back while the profile is marked stale, so on 2026-11-01 account 4's saved copy was a spent generation cswap had quarantined ("relogin_required"), while its quiet profile held a newer, working login; the next `cswap run 4` would have deleted that login and re-seeded the profile from the dead copy. The stale mark came from `cswap switch` (each switch backs the default login up into the slot it leaves and marks that slot's live profile stale), which is why Ghostex never switches the default login to restore an account: that touches every other account's sessions. It saves the profile's login into the slot instead, with cswap's own `add` reading the credential straight from the profile (`CLAUDE_SECURESTORAGE_CONFIG_DIR`) and the identity from a temporary copy of its `.claude.json` (cswap refuses `add` with `CLAUDE_CONFIG_DIR` inside its sessions folder). That clears cswap's quarantine and stale mark, and the next `cswap run` seeds the profile's `.credentials.json` from the restored copy. Ghostex reads each copy only to compare which generation it is (a hash of its refresh token) and when it expires; it never prints, logs or writes a token.
//! SEE-ALSO: server/src/accounts/default_login.rs (starts, and routing a dead default login into the slot's profile), server/src/accounts/helpers.rs (the accounts list), server/src/accounts/wake_account.rs (a wake moves off a Needs sign-in account only when nothing could be restored), server/src/ghostex_cli/account_login.rs (the same `cswap add` from a temporary profile after a browser sign-in).
use super::{claude_resets::ClaudeSlot, model::Provider};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// One generation of a Claude login: which refresh-token lineage it is (a hash, never the token)
/// and when its access token expires. Every refresh issues a token that expires later than the
/// last, so the later `expires_at` is the newer generation (cswap orders them the same way).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Generation {
    fingerprint: String,
    expires_at: i64,
}

impl Generation {
    fn read(credentials: &str) -> Option<Self> {
        use sha2::{Digest, Sha256};
        let value: Value = serde_json::from_str(credentials).ok()?;
        let oauth = value.get("claudeAiOauth")?;
        oauth["accessToken"].as_str().filter(|t| !t.is_empty())?;
        let lineage = oauth["refreshToken"].as_str().unwrap_or(credentials);
        Some(Self {
            fingerprint: hex::encode(Sha256::digest(lineage.as_bytes())),
            // Claude writes whole milliseconds, a token refreshed through cswap carries a fraction.
            expires_at: oauth["expiresAt"].as_f64().unwrap_or(0.0) as i64,
        })
    }

    fn newer_than(&self, other: Option<&Generation>) -> bool {
        other.is_none_or(|other| {
            other.fingerprint != self.fingerprint && self.expires_at > other.expires_at
        })
    }
}

/// What this computer holds for one slot whose account reads Needs sign-in.
#[derive(Debug, Default)]
pub(super) struct SlotState {
    /// The saved copy, `None` when cswap holds none.
    backup: Option<Generation>,
    /// The Keychain answered neither the copy nor "no such item" (locked, timed out).
    unreadable: bool,
    /// The login in the slot's session profile.
    profile: Option<Generation>,
    /// The profile's `.claude.json` names the slot's own account.
    profile_is_account: bool,
    /// A Claude is running on the profile, or one of its records could not be read.
    profile_live: bool,
    /// This profile login was already put back once.
    tried: bool,
    /// The saved copy is a login Ghostex put back.
    restored_backup: bool,
}

/// What a restore does, or why it cannot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Plan {
    /// The slot's profile holds a newer login of this account than the saved copy: save it.
    SaveProfileLogin,
    /// The login was put back and the account reads ready again.
    Restored,
    /// A newer login is in the profile, but a Claude is running on it; tried once it stops.
    ProfileBusy,
    /// The newer login was put back before and the account still reads signed out.
    AlreadyTried,
    /// There is no copy of this login on this computer.
    NoSavedLogin,
    /// The saved copy is the only one and it is signed out.
    SavedLoginSignedOut,
    /// The Keychain could not be read, so nothing is decided.
    Unknown,
}

/// The restore for a slot that reads Needs sign-in.
pub(super) fn plan(state: &SlotState) -> Plan {
    if state.unreadable {
        return Plan::Unknown;
    }
    match &state.profile {
        Some(profile) if state.profile_is_account && profile.newer_than(state.backup.as_ref()) => {
            if state.tried {
                Plan::AlreadyTried
            } else if state.profile_live {
                Plan::ProfileBusy
            } else {
                Plan::SaveProfileLogin
            }
        }
        _ if state.backup.is_none() => Plan::NoSavedLogin,
        _ if state.restored_backup => Plan::AlreadyTried,
        _ => Plan::SavedLoginSignedOut,
    }
}

/// Whether a start on the default login, which names this slot's account and reads signed out to
/// cswap, should run in the slot's own profile instead: the saved copy is a newer generation than
/// the default login's (or the default login holds none), so the profile seeded from it works.
pub(super) fn saved_copy_ahead_of_default(
    backup: Option<&Generation>,
    default: Option<&Generation>,
) -> bool {
    backup.is_some_and(|backup| backup.newer_than(default))
}

/// The sentence a Needs sign-in account adds to its notice once a restore was looked for.
pub(super) fn notice(plan: Plan) -> Option<&'static str> {
    Some(match plan {
        Plan::NoSavedLogin => "Ghostex found no copy of this login on this computer to restore.",
        Plan::SavedLoginSignedOut => {
            "Ghostex found only the saved copy of this login, and it is signed out too."
        }
        Plan::AlreadyTried => {
            "Ghostex restored the newer copy of this login it found, and that one is signed out too."
        }
        Plan::ProfileBusy => {
            "A newer copy of this login is in use by a running Claude; Ghostex restores it once that Claude stops."
        }
        Plan::SaveProfileLogin | Plan::Restored | Plan::Unknown => return None,
    })
}

/// Profile logins already put back, by slot and generation, so none is tried twice.
static TRIED: LazyLock<Mutex<HashSet<(String, String)>>> = LazyLock::new(Default::default);
/// The last plan per slot and when it was made, so a burst of starts and list reads checks once.
static LAST: LazyLock<Mutex<HashMap<String, (Instant, Plan)>>> = LazyLock::new(Default::default);
const RECHECK_AFTER: Duration = Duration::from_secs(60);

/// The last restore plan for a slot, for its notice.
pub(super) fn last_plan(selector: &str) -> Option<Plan> {
    LAST.lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(selector)
        .map(|(_, plan)| *plan)
}

/// Restores slot `selector` when the accounts snapshot reads it Needs sign-in, before a session
/// starts, wakes, resumes or switches onto it. True when its login was put back.
pub(crate) fn before_start(home: &Path, selector: &str) -> bool {
    let snapshot = super::runtime::current_snapshot();
    let needs_sign_in = snapshot.accounts.iter().any(|account| {
        account.provider == Provider::Claude
            && account.selector == selector
            && account.status == "loginRequired"
    });
    if !needs_sign_in || super::store::user_disabled(Provider::Claude, selector) {
        return false;
    }
    let restored = attempt(home, selector) == Plan::Restored;
    if restored {
        super::runtime::invalidate_snapshot();
    }
    restored
}

/// Restores every slot `cswap list --json` reads signed out (`relogin_required`, `no_credentials`)
/// that is not the default login (default_login.rs routes that one). True when any login was put
/// back, so the caller lists again.
pub(super) fn heal_listed(home: &Path, data: &Value) -> bool {
    let Some(rows) = data["accounts"].as_array() else {
        return false;
    };
    let mut restored = false;
    for row in rows {
        let Some(number) = row["number"].as_u64().map(|n| n.to_string()) else {
            continue;
        };
        let signed_out = matches!(
            row["usageStatus"].as_str(),
            Some("relogin_required" | "no_credentials")
        );
        if !signed_out
            || row["active"] == true
            || super::store::user_disabled(Provider::Claude, &number)
        {
            continue;
        }
        restored |= attempt(home, &number) == Plan::Restored;
    }
    restored
}

/// Whether the default login names slot `selector`'s account, cswap reads that login signed out
/// (the snapshot says Needs sign-in), and the slot's saved copy is newer than it.
pub(super) fn default_login_behind_saved_copy(home: &Path, slot: &ClaudeSlot) -> bool {
    let needs_sign_in = super::runtime::current_snapshot()
        .accounts
        .iter()
        .any(|account| {
            account.provider == Provider::Claude
                && account.selector == slot.selector()
                && account.status == "loginRequired"
        });
    needs_sign_in && saved_copy_newer_than_default(home, slot)
}

/// The slot's saved copy is a newer generation than the default login's.
pub(super) fn saved_copy_newer_than_default(home: &Path, slot: &ClaudeSlot) -> bool {
    let Ok(backup) = saved_copy(home, slot) else {
        return false;
    };
    let Ok(default) = profile_login(&home.join(".claude"), Some("Claude Code-credentials")) else {
        return false;
    };
    saved_copy_ahead_of_default(backup.as_ref(), default.as_ref())
}

fn attempt(home: &Path, selector: &str) -> Plan {
    {
        let last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, plan)) = last.get(selector) {
            if at.elapsed() < RECHECK_AFTER {
                return *plan;
            }
        }
    }
    let plan = ClaudeSlot::read(home, selector)
        .map(|slot| restore(home, &slot))
        .unwrap_or(Plan::Unknown);
    LAST.lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(selector.to_string(), (Instant::now(), plan));
    plan
}

fn restore(home: &Path, slot: &ClaudeSlot) -> Plan {
    let state = read_state(home, slot);
    let plan = plan(&state);
    if plan != Plan::SaveProfileLogin {
        return plan;
    }
    if let Some(profile) = &state.profile {
        // Recorded before the save, so a failed or still-signed-out login is never tried again.
        TRIED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((slot.selector().to_string(), profile.fingerprint.clone()));
    }
    if save_profile_login(home, slot) {
        Plan::Restored
    } else {
        Plan::AlreadyTried
    }
}

fn read_state(home: &Path, slot: &ClaudeSlot) -> SlotState {
    let session = PathBuf::from(slot.session_dir());
    let backup = saved_copy(home, slot);
    let profile = profile_login(&session, None);
    let profile_generation = profile.as_ref().ok().cloned().flatten();
    let tried_login = |generation: Option<&Generation>| {
        generation.is_some_and(|generation| {
            TRIED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(&(slot.selector().to_string(), generation.fingerprint.clone()))
        })
    };
    let tried = tried_login(profile_generation.as_ref());
    let restored_backup = tried_login(backup.as_ref().ok().and_then(Option::as_ref));
    SlotState {
        unreadable: backup.is_err() || profile.is_err(),
        backup: backup.ok().flatten(),
        profile: profile_generation,
        profile_is_account: profile_names_account(&session, slot),
        profile_live: profile_live(&session),
        tried,
        restored_backup,
    }
}

/// cswap's saved copy of the slot's login: its `claude-swap` Keychain item, else its file store.
fn saved_copy(home: &Path, slot: &ClaudeSlot) -> Result<Option<Generation>, ()> {
    let account = format!("account-{}-{}", slot.selector(), slot.email());
    if let Some(text) = keychain("claude-swap", &account)? {
        return Ok(Generation::read(&text));
    }
    let vault = super::claude_resets::swap_root(home).join(format!(
        "credentials/.creds-{}-{}.enc",
        slot.selector(),
        slot.email()
    ));
    Ok(std::fs::read_to_string(vault).ok().and_then(|text| {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .ok()?;
        Generation::read(&String::from_utf8(bytes).ok()?)
    }))
}

/// The login Claude reads for config folder `dir`: its Keychain item (named after the folder, or
/// `service` for the default login), which shadows the folder's `.credentials.json`.
fn profile_login(dir: &Path, service: Option<&str>) -> Result<Option<Generation>, ()> {
    let user = std::env::var("USER").unwrap_or_default();
    let service = service
        .map(str::to_string)
        .unwrap_or_else(|| super::claude_resets::hashed_service(&dir.to_string_lossy()));
    if !user.is_empty() {
        if let Some(text) = keychain(&service, &user)? {
            return Ok(Generation::read(&text));
        }
    }
    Ok(std::fs::read_to_string(dir.join(".credentials.json"))
        .ok()
        .and_then(|text| Generation::read(&text)))
}

fn profile_names_account(session: &Path, slot: &ClaudeSlot) -> bool {
    std::fs::read(session.join(".claude.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .is_some_and(|config| {
            let account = &config["oauthAccount"];
            let email = account["emailAddress"].as_str().unwrap_or_default();
            let organization = account["organizationUuid"].as_str().unwrap_or_default();
            format!("{}:{organization}", email.to_lowercase()) == slot.identity()
        })
}

/// A Claude runs on the profile: Claude writes `sessions/<pid>.json` there while it runs. A record
/// that cannot be read counts as running, as cswap counts it.
fn profile_live(session: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(session.join("sessions")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            return false;
        }
        std::fs::read(&path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
            .and_then(|record| record["pid"].as_u64())
            .and_then(|pid| u32::try_from(pid).ok())
            .is_none_or(crate::platform::process::process_is_alive)
    })
}

/// A Keychain item's secret, `None` when there is no such item, `Err` when the Keychain did not
/// answer. Off macOS there is no Keychain.
fn keychain(service: &str, account: &str) -> Result<Option<String>, ()> {
    if !cfg!(target_os = "macos") {
        return Ok(None);
    }
    let mut child = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-a", account, "-w", "-s", service])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let status = wait(&mut child, Duration::from_secs(5)).ok_or(())?;
    match status.code() {
        Some(0) => {
            use std::io::Read;
            let mut text = String::new();
            child
                .stdout
                .take()
                .ok_or(())?
                .read_to_string(&mut text)
                .map_err(|_| ())?;
            let text = text.trim_end_matches('\n').to_string();
            Ok((!text.is_empty()).then_some(text))
        }
        Some(44) => Ok(None),
        _ => Err(()),
    }
}

fn wait(child: &mut std::process::Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if started.elapsed() < limit => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// `cswap add` with the profile's login: the credential read from the profile's own store
/// (`CLAUDE_SECURESTORAGE_CONFIG_DIR`), the identity from a private copy of its `.claude.json`.
/// cswap refreshes the slot in place (it never refreshes the token while saving it), clears its
/// quarantine and re-seeds the profile on the next `cswap run`.
fn save_profile_login(home: &Path, slot: &ClaudeSlot) -> bool {
    let Some(cswap) = super::helpers::executable(home, "cswap") else {
        return false;
    };
    let session = slot.session_dir();
    let Ok(identity) = std::fs::read(Path::new(&session).join(".claude.json")) else {
        return false;
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix("ghostex-account-restore-");
    #[cfg(target_os = "macos")]
    let directory = builder.tempdir_in("/private/tmp");
    #[cfg(not(target_os = "macos"))]
    let directory = builder.tempdir();
    let Ok(directory) = directory else {
        return false;
    };
    let config = directory.path().join(".claude.json");
    if std::fs::write(&config, identity).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600));
    }
    let mut command = crate::platform::process::background_command(cswap);
    for key in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
        "CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR",
    ] {
        command.env_remove(key);
    }
    command
        .arg("add")
        .current_dir(home)
        .env("HOME", home)
        .env("CLAUDE_CONFIG_DIR", directory.path())
        .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", &session)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    wait(&mut child, Duration::from_secs(60)).is_some_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generation(lineage: &str, expires_at: i64) -> Generation {
        Generation {
            fingerprint: lineage.to_string(),
            expires_at,
        }
    }

    fn state(backup: Option<Generation>, profile: Option<Generation>) -> SlotState {
        SlotState {
            backup,
            profile,
            profile_is_account: true,
            ..SlotState::default()
        }
    }

    #[test]
    fn a_newer_profile_login_is_saved_over_a_dead_saved_copy() {
        // Account 4 on 2026-11-01: the saved copy expired 02:58Z and was quarantined, while the
        // profile held the generation Claude refreshed at 02:54Z.
        let s = state(
            Some(generation("spent", 1_793_501_884_000)),
            Some(generation("head", 1_793_530_476_000)),
        );
        assert_eq!(plan(&s), Plan::SaveProfileLogin);
    }

    #[test]
    fn a_profile_login_restores_an_account_with_no_saved_copy() {
        let s = state(None, Some(generation("head", 2)));
        assert_eq!(plan(&s), Plan::SaveProfileLogin);
    }

    #[test]
    fn an_older_or_same_profile_login_is_never_saved() {
        let older = state(Some(generation("saved", 5)), Some(generation("old", 4)));
        assert_eq!(plan(&older), Plan::SavedLoginSignedOut);
        let same = state(Some(generation("saved", 5)), Some(generation("saved", 9)));
        assert_eq!(plan(&same), Plan::SavedLoginSignedOut);
    }

    #[test]
    fn nothing_to_restore_says_whether_a_copy_exists() {
        // Accounts 1 and 3: no saved copy and no profile login.
        assert_eq!(plan(&state(None, None)), Plan::NoSavedLogin);
        assert_eq!(
            plan(&state(Some(generation("saved", 5)), None)),
            Plan::SavedLoginSignedOut
        );
        assert!(notice(Plan::NoSavedLogin).unwrap().contains("no copy"));
        assert!(notice(Plan::SavedLoginSignedOut)
            .unwrap()
            .contains("only the saved copy"));
        assert_eq!(notice(Plan::Restored), None);
    }

    #[test]
    fn a_login_of_another_account_is_never_saved() {
        let mut s = state(None, Some(generation("other", 9)));
        s.profile_is_account = false;
        assert_eq!(plan(&s), Plan::NoSavedLogin);
    }

    #[test]
    fn a_running_profile_waits_and_a_tried_login_is_not_tried_again() {
        let mut s = state(Some(generation("saved", 1)), Some(generation("head", 2)));
        s.profile_live = true;
        assert_eq!(plan(&s), Plan::ProfileBusy);
        s.profile_live = false;
        s.tried = true;
        assert_eq!(plan(&s), Plan::AlreadyTried);
        // After the save cswap empties the profile, and the restored copy is the saved one.
        let mut after = state(Some(generation("head", 2)), None);
        after.restored_backup = true;
        assert_eq!(plan(&after), Plan::AlreadyTried);
    }

    #[test]
    fn an_unreadable_keychain_decides_nothing() {
        let mut s = state(None, Some(generation("head", 2)));
        s.unreadable = true;
        assert_eq!(plan(&s), Plan::Unknown);
    }

    #[test]
    fn a_dead_default_login_routes_only_to_a_newer_saved_copy() {
        let saved = generation("saved", 10);
        assert!(saved_copy_ahead_of_default(Some(&saved), None));
        assert!(saved_copy_ahead_of_default(
            Some(&saved),
            Some(&generation("default", 5))
        ));
        assert!(!saved_copy_ahead_of_default(
            Some(&saved),
            Some(&generation("default", 20))
        ));
        assert!(!saved_copy_ahead_of_default(
            Some(&saved),
            Some(&generation("saved", 5))
        ));
        assert!(!saved_copy_ahead_of_default(
            None,
            Some(&generation("default", 5))
        ));
    }

    #[test]
    fn a_generation_is_its_refresh_lineage_and_expiry() {
        let text = r#"{"claudeAiOauth":{"accessToken":"a","refreshToken":"r","expiresAt":42}}"#;
        let read = Generation::read(text).unwrap();
        assert_eq!(read.expires_at, 42);
        let rotated = r#"{"claudeAiOauth":{"accessToken":"b","refreshToken":"r","expiresAt":99}}"#;
        assert_eq!(
            Generation::read(rotated).unwrap().fingerprint,
            read.fingerprint
        );
        // Account 4's profile login on 2026-11-01 expired at a fractional millisecond.
        let fractional = r#"{"claudeAiOauth":{"accessToken":"c","refreshToken":"n","expiresAt":1793530476155.781}}"#;
        assert_eq!(
            Generation::read(fractional).unwrap().expires_at,
            1_793_530_476_155
        );
        assert!(Generation::read(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_none());
        assert!(Generation::read("not json").is_none());
    }
}
