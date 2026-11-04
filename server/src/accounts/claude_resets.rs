//! Claude usage-limit reset grants: Anthropic's `cedar_ember` program, read and claimed over the same OAuth API Claude Code uses.
use super::model::ResetCredit;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

const API: &str = "https://api.anthropic.com";
/// CDXC:AgentProviders 2026-09-24 WHY:
/// Anthropic decides reset eligibility by client surface: a request that does not identify as Claude Code answers `eligible: false, ineligible_reason: "surface"` with no grants. The version is the Claude Code release whose protocol this mirrors.
const USER_AGENT: &str = "claude-cli/2.1.281 (external, cli)";
const PROGRAM: &str = "cedar_ember";

/// One cswap slot as its own `sequence.json` records it.
pub(crate) struct ClaudeSlot {
    root: PathBuf,
    slot: String,
    email: String,
    organization: String,
    active: bool,
}

/// cswap's data root: `$XDG_DATA_HOME/claude-swap` on Linux and WSL, `~/.claude-swap-backup` elsewhere.
pub(crate) fn swap_root(home: &Path) -> PathBuf {
    if cfg!(target_os = "linux") {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"))
            .join("claude-swap")
    } else {
        home.join(".claude-swap-backup")
    }
}

impl ClaudeSlot {
    pub(crate) fn read(home: &Path, slot: &str) -> Result<Self, String> {
        let root = swap_root(home);
        let raw = std::fs::read(root.join("sequence.json"))
            .map_err(|_| "Claude Swap's account list could not be read.")?;
        let data: Value = serde_json::from_slice(&raw)
            .map_err(|_| "Claude Swap's account list is unreadable.")?;
        let row = &data["accounts"][slot];
        let email = row["email"].as_str().unwrap_or("");
        let organization = row["organizationUuid"].as_str().unwrap_or("");
        let safe = |value: &str| {
            !value.is_empty() && !value.contains(['/', '\\', '\0']) && !value.contains("..")
        };
        if !safe(email) || !safe(organization) || !slot.bytes().all(|b| b.is_ascii_digit()) {
            return Err("This Claude account is missing from Claude Swap. Reconnect it.".into());
        }
        Ok(Self {
            root,
            slot: slot.to_string(),
            email: email.to_string(),
            organization: organization.to_string(),
            active: data["activeAccountNumber"]
                .as_u64()
                .map(|n| n.to_string())
                .as_deref()
                == Some(slot),
        })
    }

    pub(crate) fn identity(&self) -> String {
        format!("{}:{}", self.email.to_lowercase(), self.organization)
    }

    pub(crate) fn organization(&self) -> &str {
        &self.organization
    }

    pub(crate) fn selector(&self) -> &str {
        &self.slot
    }

    pub(crate) fn email(&self) -> &str {
        &self.email
    }

    /// Whether the default login's `~/.claude.json` names this slot's account, which is when `cswap run` starts plain `claude` on it.
    pub(crate) fn names_default_login(&self, home: &Path) -> bool {
        default_login_matches(home, &self.email, &self.organization)
    }

    pub(crate) fn session_dir(&self) -> String {
        let slug: String = self
            .email
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!(
            "{}/sessions/{}-{slug}",
            self.root.to_string_lossy(),
            self.slot
        )
    }

    /// CDXC:AgentProviders 2026-09-24 WHY:
    /// cswap owns Claude logins, so Ghostex only reads access tokens and never refreshes them: refreshing rotates the refresh token and would strand the copy cswap and Claude Code keep. The newest generation lives where Claude Code last ran as the account (the default login for cswap's active slot, else the slot's session profile), and cswap's vault copy is the last resort. Expired or unusable tokens are skipped; cswap's own usage polling keeps at least one fresh.
    pub(crate) fn tokens(&self, home: &Path) -> Vec<String> {
        let mut sources = Vec::new();
        if self.active && default_login_matches(home, &self.email, &self.organization) {
            sources.push(keychain("Claude Code-credentials"));
            sources.push(file(&home.join(".claude/.credentials.json")));
        }
        let session = self.session_dir();
        sources.push(keychain(&hashed_service(&session)));
        sources.push(file(&PathBuf::from(&session).join(".credentials.json")));
        let vault = self.root.join(format!(
            "credentials/.creds-{}-{}.enc",
            self.slot, self.email
        ));
        sources.push(
            std::fs::read_to_string(vault)
                .ok()
                .and_then(|text| base64_decode(text.trim()))
                .and_then(|bytes| String::from_utf8(bytes).ok()),
        );
        sources.push(keychain_entry(
            "claude-swap",
            &format!("account-{}-{}", self.slot, self.email),
        ));
        usable_tokens(sources)
    }
}

/// The access tokens and organization of the default Claude login (`~/.claude`), for a Claude
/// cloud session that runs on the current CLI login rather than a saved account.
pub(crate) fn default_login_tokens(home: &Path) -> Option<(Vec<String>, String)> {
    let config: Value =
        serde_json::from_slice(&std::fs::read(home.join(".claude.json")).ok()?).ok()?;
    let organization = config["oauthAccount"]["organizationUuid"]
        .as_str()
        .filter(|value| !value.is_empty())?
        .to_string();
    let tokens = usable_tokens(vec![
        keychain("Claude Code-credentials"),
        file(&home.join(".claude/.credentials.json")),
    ]);
    Some((tokens, organization))
}

/// The unexpired access tokens of credential JSON texts, without duplicates.
fn usable_tokens(sources: Vec<Option<String>>) -> Vec<String> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tokens: Vec<String> = Vec::new();
    for text in sources.into_iter().flatten() {
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let oauth = &value["claudeAiOauth"];
        let Some(token) = oauth["accessToken"]
            .as_str()
            .filter(|t| !t.trim().is_empty())
        else {
            continue;
        };
        if oauth["expiresAt"]
            .as_i64()
            .is_some_and(|at| at <= now + 60_000)
        {
            continue;
        }
        if oauth["scopes"]
            .as_array()
            .is_some_and(|scopes| !scopes.iter().any(|s| s == "user:profile"))
        {
            continue;
        }
        if !tokens.iter().any(|t| t == token) {
            tokens.push(token.trim().to_string());
        }
    }
    tokens
}

fn default_login_matches(home: &Path, email: &str, organization: &str) -> bool {
    std::fs::read(home.join(".claude.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .is_some_and(|config| {
            let account = &config["oauthAccount"];
            account["emailAddress"]
                .as_str()
                .is_some_and(|e| e.eq_ignore_ascii_case(email))
                && account["organizationUuid"].as_str() == Some(organization)
        })
}

/// The Keychain service Claude Code derives for a `CLAUDE_CONFIG_DIR`: the first eight hex digits of its SHA-256.
pub(super) fn hashed_service(config_dir: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(config_dir.as_bytes()));
    format!("Claude Code-credentials-{}", &digest[..8])
}

fn file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn keychain(service: &str) -> Option<String> {
    let user = std::env::var("USER").ok().filter(|u| !u.is_empty())?;
    keychain_entry(service, &user)
}

/// CDXC:AgentProviders 2026-11-04 WHY:
/// The cloud poller asks for a login every 10 to 30 seconds per cloud session, and each ask read up to three Keychain items through `/usr/bin/security`, one process each (149 launches in five minutes, measured 2026-11-04, issues-1027/macos-perf). A read is kept until the five-minute limit or a minute before its access token expires, whichever is first, and a missing item for a minute; a login that claude.ai refuses drops every kept read (`forget_keychain_reads`), so a rotated token is read again on the next try.
/// The items stay read through `security`: Claude Code and cswap create them with that tool, so it is the application their access lists trust, and reading them from gxserver itself would ask the user for Keychain access.
#[cfg(target_os = "macos")]
const KEYCHAIN_READ_TTL: Duration = Duration::from_secs(5 * 60);
#[cfg(target_os = "macos")]
const KEYCHAIN_MISSING_TTL: Duration = Duration::from_secs(60);

/// What one Keychain read answered: the item's secret, or no such item. A Keychain that did not
/// answer is not an answer and is never kept.
#[cfg(target_os = "macos")]
static KEYCHAIN_READS: Mutex<Option<HashMap<(String, String), (Instant, Option<String>)>>> =
    Mutex::new(None);

/// A Keychain item's secret through `security`: `Ok(None)` when there is no such item, `Err` when
/// the Keychain did not answer. Answers are kept (`keychain_read_lifetime`); restore.rs reads the
/// same items through here, so cswap's saved copies are read once for every caller.
pub(crate) fn keychain_read(service: &str, account: &str) -> Result<Option<String>, ()> {
    #[cfg(target_os = "macos")]
    {
        let key = (service.to_string(), account.to_string());
        if let Some((until, text)) = KEYCHAIN_READS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(HashMap::new)
            .get(&key)
        {
            if Instant::now() < *until {
                return Ok(text.clone());
            }
        }
        let text = security_find_generic_password(service, account)?;
        let keep = match &text {
            Some(text) => keychain_read_lifetime(text, chrono::Utc::now().timestamp_millis()),
            None => KEYCHAIN_MISSING_TTL,
        };
        if !keep.is_zero() {
            KEYCHAIN_READS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(HashMap::new)
                .insert(key, (Instant::now() + keep, text.clone()));
        }
        Ok(text)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (service, account);
        Ok(None)
    }
}

/// `security find-generic-password -w`: exit 0 is the secret, 44 is "no such item", anything else
/// (or no answer within five seconds) is a Keychain that did not answer.
#[cfg(target_os = "macos")]
fn security_find_generic_password(service: &str, account: &str) -> Result<Option<String>, ()> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-a", account, "-w", "-s", service])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    // The secret is small; reading it before waiting keeps a full pipe from stalling the tool.
    let mut text = String::new();
    let reader = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout.read_to_string(&mut text);
            text
        })
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(());
            }
        }
    };
    if let Some(reader) = reader {
        text = reader.join().map_err(|_| ())?;
    }
    match status.code() {
        Some(0) => {
            let text = text.trim_end_matches('\n').to_string();
            Ok((!text.is_empty()).then_some(text))
        }
        Some(44) => Ok(None),
        _ => Err(()),
    }
}

fn keychain_entry(service: &str, account: &str) -> Option<String> {
    keychain_read(service, account).ok().flatten()
}

/// How long a Keychain read may be reused: the read limit, cut short a minute before the access
/// token it holds expires (when `usable_tokens` would stop using it). A token that expired more
/// than five minutes ago belongs to a login nothing is refreshing (a signed-out account, which
/// restore.rs checks on every account-list read), so it is kept like a missing item.
#[cfg(target_os = "macos")]
fn keychain_read_lifetime(text: &str, now_ms: i64) -> Duration {
    let expires_at = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value["claudeAiOauth"]["expiresAt"].as_i64());
    match expires_at {
        Some(at) if at < now_ms - 5 * 60_000 => KEYCHAIN_MISSING_TTL,
        Some(at) => {
            let left = u64::try_from(at - 60_000 - now_ms).unwrap_or(0);
            KEYCHAIN_READ_TTL.min(Duration::from_millis(left))
        }
        None => KEYCHAIN_READ_TTL,
    }
}

/// Drops every kept Keychain read: after claude.ai refused all of a login's tokens, and before and
/// after restore.rs replaces a saved login.
pub(crate) fn forget_keychain_reads() {
    #[cfg(target_os = "macos")]
    if let Some(reads) = KEYCHAIN_READS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
    {
        reads.clear();
    }
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

/// `method` is `ureq::Agent::get` or `ureq::Agent::post`.
fn request<B>(
    method: impl FnOnce(&ureq::Agent, String) -> ureq::RequestBuilder<B>,
    path: &str,
    token: &str,
) -> ureq::RequestBuilder<B> {
    method(crate::http_agent::agent(), format!("{API}{path}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .config()
        .timeout_global(Some(Duration::from_secs(25)))
        .build()
}

/// What the reset panel shows for one Claude account: `None` when Anthropic reports no reset program for the plan.
type Status = Option<Vec<ResetCredit>>;

fn fetch(home: &Path, slot: &ClaudeSlot) -> Result<Status, String> {
    let tokens = slot.tokens(home);
    if tokens.is_empty() {
        return Err(
            "Resets need a current login. Start a Claude session with this account, then refresh."
                .into(),
        );
    }
    for token in tokens {
        match request(
            ureq::Agent::get,
            "/api/oauth/usage?cedar_ember=1&skip_spend=1",
            &token,
        )
        .call()
        {
            Ok(response) => {
                let body: Value = crate::http_agent::read_json(response)
                    .map_err(|_| "Anthropic returned an unreadable reset status.")?;
                return Ok(parse(&body["cedar_ember"], chrono::Utc::now()));
            }
            Err(ureq::Error::StatusCode(401 | 403)) => continue,
            Err(ureq::Error::StatusCode(429)) => {
                return Err(
                    "Anthropic is limiting usage requests. Resets will refresh later.".into(),
                )
            }
            Err(_) => return Err("Resets could not be read. Ghostex will try again.".into()),
        }
    }
    forget_keychain_reads();
    Err("The saved login could not read resets. Reconnect this account.".into())
}

/// Grants the account can still use, one entry per remaining reset, soonest deadline first. Grants past their deadline or with none left are dropped; paused grants stay listed because they are still owned.
fn parse(block: &Value, now: chrono::DateTime<chrono::Utc>) -> Status {
    let block = block.as_object()?;
    let mut credits = Vec::new();
    if block.get("eligible").and_then(Value::as_bool) != Some(true) {
        return Some(credits);
    }
    for grant in block
        .get("grants")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let Some(id) = grant["id"].as_str().filter(|id| valid_grant(id)) else {
            continue;
        };
        let left = grant["resets_left"].as_u64().unwrap_or(0);
        let ends = grant["ends_at"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|date| date.with_timezone(&chrono::Utc));
        if left == 0 || ends.is_some_and(|date| date <= now) {
            continue;
        }
        let paused = grant["paused"] == true;
        let requires_limit = grant["use_requires_limit"] == true;
        let note = if paused {
            Some("Paused by Anthropic for now.".to_string())
        } else if requires_limit {
            Some("Only works once you reach a usage limit.".to_string())
        } else {
            None
        };
        for _ in 0..left.min(20) {
            credits.push(ResetCredit {
                id: id.to_string(),
                expires_at: ends.map(|date| date.to_rfc3339()),
                note: note.clone(),
                paused,
                requires_limit,
            });
        }
    }
    credits.sort_by(|a, b| match (&a.expires_at, &b.expires_at) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
    Some(credits)
}

fn valid_grant(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// CDXC:AgentProviders 2026-09-24 WHY:
/// The reset status comes from Anthropic's usage endpoint, which rate-limits per account and is already polled by cswap. Ghostex reads it at most every 15 minutes per account (30 after a 429), and a claim clears the entry so the next refresh shows the new count.
static CACHE: Mutex<Option<HashMap<String, (Instant, Duration, Result<Status, String>)>>> =
    Mutex::new(None);

pub(crate) fn cached(home: &Path, slot: &str) -> Result<Status, String> {
    let slot = ClaudeSlot::read(home, slot)?;
    let key = slot.identity();
    if let Some((at, ttl, result)) = CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        if at.elapsed() < *ttl {
            return result.clone();
        }
    }
    let result = fetch(home, &slot);
    let ttl = match &result {
        Ok(_) => Duration::from_secs(15 * 60),
        Err(error) if error.contains("limiting") => Duration::from_secs(30 * 60),
        Err(_) => Duration::from_secs(5 * 60),
    };
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, (Instant::now(), ttl, result.clone()));
    result
}

fn forget(slot: &ClaudeSlot) {
    if let Some(cache) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        cache.remove(&slot.identity());
    }
}

/// Claims one reset of `grant_id`. `request_id` is the caller's idempotency key, reused on retry so a lost reply can never spend a second reset.
pub(crate) fn claim(
    home: &Path,
    slot: &ClaudeSlot,
    grant_id: &str,
    request_id: &str,
) -> super::reset_claim::Outcome {
    use super::reset_claim::Outcome;
    if !valid_grant(grant_id) {
        return Outcome::failed("This reset has an unsupported identifier.");
    }
    let tokens = slot.tokens(home);
    if tokens.is_empty() {
        return Outcome::failed(
            "Resets need a current login. Start a Claude session with this account, then try again.",
        );
    }
    let path = format!("/api/organizations/{}/reset_rate_limits", slot.organization);
    let body = json!({"program":PROGRAM,"grant_id":grant_id,"request_id":request_id});
    for token in tokens {
        let response = match request(ureq::Agent::post, &path, &token).send_json(body.clone()) {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(401 | 403)) => continue,
            Err(ureq::Error::StatusCode(429)) => {
                return Outcome::failed("Anthropic is limiting requests. Try again in a moment.")
            }
            Err(_) => {
                forget(slot);
                return Outcome::failed(
                    "The reset could not be confirmed. Check your limits before trying again.",
                );
            }
        };
        forget(slot);
        let Ok(value) = crate::http_agent::read_json::<Value>(response) else {
            return Outcome::failed(
                "The reset could not be confirmed. Check your limits before trying again.",
            );
        };
        return outcome(&value);
    }
    forget_keychain_reads();
    Outcome::failed("The saved login could not use resets. Reconnect this account.")
}

/// Claude Code's own wording for each claim result, trimmed for the panel.
fn outcome(value: &Value) -> super::reset_claim::Outcome {
    use super::reset_claim::Outcome;
    let reason = value["reason"].as_str().unwrap_or("");
    match value["result"].as_str().unwrap_or("") {
        "reset" => Outcome::success(),
        "not_limited" => Outcome::nothing_to_reset(
            "Your reset only works at a usage limit, and you're not at one now. Nothing was used.",
        ),
        "already_used" => {
            Outcome::no_credit("That reset was already used. Nothing changed just now.")
        }
        "cooldown" => Outcome::failed("Resets are cooling down. Try again in a minute."),
        "ineligible" if reason == "not_next_grant" => {
            Outcome::no_credit("A different reset is on offer now. Nothing was used.")
        }
        "ineligible" => {
            Outcome::no_credit("This reset isn't available any more. Nothing was used.")
        }
        _ => Outcome::failed(
            "The reset could not be confirmed. Check your limits before trying again.",
        ),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod keychain_read_tests {
    use super::*;

    #[test]
    fn a_keychain_read_is_kept_until_a_minute_before_its_token_expires() {
        let now = 1_000_000_000_i64;
        let soon =
            json!({"claudeAiOauth": {"accessToken": "a", "expiresAt": now + 61_000 + 30_000}});
        assert_eq!(
            keychain_read_lifetime(&soon.to_string(), now),
            Duration::from_millis(31_000)
        );
        let later = json!({"claudeAiOauth": {"accessToken": "a", "expiresAt": now + 3_600_000}});
        assert_eq!(
            keychain_read_lifetime(&later.to_string(), now),
            KEYCHAIN_READ_TTL
        );
        let expiring = json!({"claudeAiOauth": {"accessToken": "a", "expiresAt": now - 1}});
        assert_eq!(
            keychain_read_lifetime(&expiring.to_string(), now),
            Duration::ZERO
        );
        let dead = json!({"claudeAiOauth": {"accessToken": "a", "expiresAt": now - 6 * 60_000}});
        assert_eq!(
            keychain_read_lifetime(&dead.to_string(), now),
            KEYCHAIN_MISSING_TTL
        );
        assert_eq!(keychain_read_lifetime("not json", now), KEYCHAIN_READ_TTL);
    }

    #[test]
    fn a_missing_keychain_item_is_read_once_until_forgotten() {
        let service = format!("ghostex-test-missing-{}", std::process::id());
        assert_eq!(keychain_read(&service, "nobody"), Ok(None));
        let key = (service.clone(), "nobody".to_string());
        let kept = KEYCHAIN_READS
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|reads| reads.get(&key).cloned());
        assert!(kept.is_some_and(|(until, text)| text.is_none() && until > Instant::now()));
        forget_keychain_reads();
        assert!(KEYCHAIN_READS
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|reads| !reads.contains_key(&key)));
    }
}
