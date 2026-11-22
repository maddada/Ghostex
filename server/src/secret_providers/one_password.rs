//! Reading one secret from 1Password through its `op` CLI. Values only ever come back to the
//! caller; nothing here logs a value, and no error message carries one.
//!
//! CDXC:Secrets 2026-11-02 WHY:
//! `--cache=false` keeps `op` from starting its background daemon, which waits on prompts a background gxserver can never show. A read gets 45 seconds: long enough to reach for the fingerprint reader when 1Password asks to unlock, short enough that a locked vault cannot wedge whatever asked.

use std::{path::PathBuf, process::Command, time::Duration};

use crate::platform::{
    live_path,
    process::{run_bounded, Keep, RunLimits, Unfinished},
};

const OP_BINARY: &str = "op";
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// A secret larger than this is not a key or a token.
const MAX_VALUE_BYTES: usize = 64 * 1024;
const MAX_REFERENCE_CHARS: usize = 1024;
const MAX_ACCOUNT_CHARS: usize = 253;

/// Where Homebrew and the 1Password installer put `op` when gxserver's own PATH lacks them.
fn extra_directories() -> Vec<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn op_path() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(fake) = FAKE_OP.with(|fake| fake.borrow().clone()) {
        return Some(fake);
    }
    live_path::find(OP_BINARY, &extra_directories())
}

#[cfg(test)]
thread_local! {
    /// A stand-in `op` script for tests, so no test ever reaches the user's real 1Password.
    pub(crate) static FAKE_OP: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Any value `op --account` takes: a shorthand, sign-in address, account ID or user ID. It goes to
/// the CLI as an argument, so it can never start with `-`.
pub(crate) fn validate_account(account: &str) -> Result<(), String> {
    let valid = !account.is_empty()
        && account.chars().count() <= MAX_ACCOUNT_CHARS
        && account
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && account
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '@'));
    if valid {
        Ok(())
    } else {
        Err("Enter the 1Password account as its sign-in address (my.1password.com), email or ID, with no spaces.".to_string())
    }
}

/// A part of `op://vault/item[/section]/field`: names with other characters must be written as
/// their 1Password ID. It also keeps line breaks and option-looking text out of the argument.
fn valid_reference_part(part: &str) -> bool {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let allowed = |ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-');
    allowed(first)
        && part.chars().last().is_some_and(allowed)
        && part.chars().all(|ch| allowed(ch) || ch == ' ')
}

/// Whether `reference` is a 1Password secret reference: `op://vault/item/field` or
/// `op://vault/item/section/field`, optionally with one `?attribute=value`.
pub(crate) fn is_reference(reference: &str) -> bool {
    if reference.chars().count() > MAX_REFERENCE_CHARS {
        return false;
    }
    let Some(rest) = reference.strip_prefix("op://") else {
        return false;
    };
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    if let Some(query) = query {
        let Some((key, value)) = query.split_once('=') else {
            return false;
        };
        let key_ok = !key.is_empty() && key.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-');
        let value_ok = !value.is_empty()
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'));
        if !key_ok || !value_ok {
            return false;
        }
    }
    let parts: Vec<&str> = path.split('/').collect();
    (3..=4).contains(&parts.len()) && parts.iter().all(|part| valid_reference_part(part))
}

pub(crate) const INVALID_REFERENCE_MESSAGE: &str =
    "Use a 1Password reference like op://vault/item/field.";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadError {
    NotInstalled,
    InvalidReference,
    InvalidAccount,
    /// `op` exited non-zero: locked and not unlocked, signed out, or no such item.
    Failed,
    TimedOut,
    Empty,
}

impl ReadError {
    /// What to tell the user; never carries the value or `op`'s own output.
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::NotInstalled => "The 1Password CLI (op) is not installed on this computer. Install it from 1Password, then turn on \"Integrate with 1Password CLI\" in 1Password's Developer settings.",
            Self::InvalidReference => INVALID_REFERENCE_MESSAGE,
            Self::InvalidAccount => "This provider's 1Password account is not valid.",
            Self::Failed => "1Password could not read that reference. Check that the account is signed in on this computer, 1Password is unlocked, and the vault, item and field exist.",
            Self::TimedOut => "1Password did not answer in time. Unlock 1Password and try again.",
            Self::Empty => "That 1Password field is empty.",
        }
    }
}

/// Reads one secret. 1Password may ask the user to unlock first (Touch ID or the app password).
pub(crate) fn read(account: &str, reference: &str) -> Result<String, ReadError> {
    validate_account(account).map_err(|_| ReadError::InvalidAccount)?;
    if !is_reference(reference) {
        return Err(ReadError::InvalidReference);
    }
    let op = op_path().ok_or(ReadError::NotInstalled)?;
    let mut command = Command::new(op);
    command
        .arg("--cache=false")
        .arg("read")
        .arg(format!("--account={account}"))
        .arg("--no-newline")
        .arg(reference)
        .env("PATH", live_path::value());
    let output = run_bounded(
        command,
        RunLimits {
            timeout: READ_TIMEOUT,
            stdin: None,
            stdout: Keep::AtMost(MAX_VALUE_BYTES),
            stderr: Keep::Tail(2048),
        },
    )
    .map_err(|error| match error {
        Unfinished::Start(_) => ReadError::NotInstalled,
        Unfinished::TimedOut(_) => ReadError::TimedOut,
        Unfinished::TooLarge(_) => ReadError::Failed,
    })?;
    if !output.status.success() {
        return Err(ReadError::Failed);
    }
    let value = String::from_utf8(output.stdout).map_err(|_| ReadError::Failed)?;
    // A secret is passed on as stored; surrounding whitespace can be part of it.
    if value.trim().is_empty() {
        return Err(ReadError::Empty);
    }
    Ok(value)
}

/// One account `op` knows on this computer (`op account list`), which needs no unlock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KnownAccount {
    pub(crate) url: String,
    pub(crate) email: String,
    pub(crate) account_uuid: String,
    pub(crate) user_uuid: String,
}

impl KnownAccount {
    /// Whether `account` (any spelling `op --account` takes) names this account.
    pub(crate) fn matches(&self, account: &str) -> bool {
        let wanted = account.trim().to_ascii_lowercase();
        let shorthand = self
            .url
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        [
            self.url.to_ascii_lowercase(),
            self.email.to_ascii_lowercase(),
            self.account_uuid.to_ascii_lowercase(),
            self.user_uuid.to_ascii_lowercase(),
            shorthand,
        ]
        .iter()
        .any(|known| !known.is_empty() && *known == wanted)
    }
}

pub(crate) fn parse_account_list(json: &str) -> Option<Vec<KnownAccount>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let text = |entry: &serde_json::Value, key: &str| {
        entry
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some(
        value
            .as_array()?
            .iter()
            .map(|entry| KnownAccount {
                url: text(entry, "url"),
                email: text(entry, "email"),
                account_uuid: text(entry, "account_uuid"),
                user_uuid: text(entry, "user_uuid"),
            })
            .collect(),
    )
}

/// The accounts `op` is signed in to on this computer; `None` when `op` is missing or failed.
pub(crate) fn known_accounts() -> Option<Vec<KnownAccount>> {
    let op = op_path()?;
    let mut command = Command::new(op);
    command
        .arg("--cache=false")
        .args(["account", "list", "--format=json"])
        .env("PATH", live_path::value());
    let output = run_bounded(
        command,
        RunLimits {
            timeout: LIST_TIMEOUT,
            stdin: None,
            stdout: Keep::AtMost(256 * 1024),
            stderr: Keep::Tail(2048),
        },
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_account_list(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_follow_one_password_syntax() {
        assert!(is_reference("op://Private/GitHub/token"));
        assert!(is_reference("op://Private/GitHub token/section/field"));
        assert!(is_reference("op://dev/db/password?attribute=otp"));
        assert!(!is_reference("op://Private/GitHub"));
        assert!(!is_reference("op://Private/a/b/c/d"));
        assert!(!is_reference("op://Private/ GitHub/token"));
        assert!(!is_reference("op://Private/Git{{Hub/token"));
        assert!(!is_reference("op://Private/GitHub/token\n"));
        assert!(!is_reference("https://Private/GitHub/token"));
        assert!(!is_reference("op://Private/GitHub/token?attribute=x y"));
    }

    #[test]
    fn accounts_never_look_like_options() {
        assert!(validate_account("my.1password.com").is_ok());
        assert!(validate_account("me@example.com").is_ok());
        assert!(validate_account("ABCDEF123").is_ok());
        assert!(validate_account("-x").is_err());
        assert!(validate_account("my account").is_err());
        assert!(validate_account("").is_err());
    }

    #[test]
    fn account_list_matches_every_spelling() {
        let accounts = parse_account_list(
            r#"[{"url":"team.1password.com","email":"a@b.co","user_uuid":"U1","account_uuid":"A1"}]"#,
        )
        .unwrap();
        assert!(accounts[0].matches("team.1password.com"));
        assert!(accounts[0].matches("team"));
        assert!(accounts[0].matches("A@B.co"));
        assert!(accounts[0].matches("A1"));
        assert!(!accounts[0].matches("other.1password.com"));
    }
}
