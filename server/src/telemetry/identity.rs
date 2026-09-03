/*
CDXC:Telemetry 2026-08-27 (addendum v2, §1):
`distinct_id` resolution. This used to be "SHA-256 of the install's serverId",
which counted one human with a laptop AND a desktop as two people. The chain
below prefers an id that is stable across a person's machines, and falls back to
the per-install serverId only when there is nothing better:

  1. `~/.codex/auth.json` -> `tokens.account_id`
  2. `~/.claude.json`     -> `userID`
  3. the gxserver `serverId` (random, per install)

THE SALT PREFIX IS NOT OPTIONAL. Other products hash these same account ids with
a bare SHA-256; without a product-specific prefix our `distinct_id` for a person
would be byte-identical to theirs, and anyone holding both datasets could join
them and de-anonymise our users. `ghostex-analytics-v1:` makes the ids we emit
meaningless outside Ghostex.

The raw account id's lifetime is one `if let` binding inside `resolve`: it comes
out of `read_json_string_field`, goes straight into `hash_with_salt`, and is
dropped at the end of that arm. Nothing else ever holds it — `ResolvedIdentity`
carries only the hash — so it is never stored, never logged, and never named in an
error — note that every read below discards its error with `.ok()` rather than
formatting one, because a formatted error is exactly how a file's contents leak
into a log. A missing file is the NORMAL case here, not a failure, so each step
falls through to the next silently.

Resolution happens once per server start and is cached for the process lifetime
(it lives in the `Telemetry` struct). It is deliberately NOT persisted: a user
who installs Codex next month should start deduplicating from that point, which
a persisted id would prevent forever.
*/

use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::paths::GxserverPaths;

/// The salt that makes a Ghostex `distinct_id` un-joinable with any other
/// product that hashes the same account ids.
const DISTINCT_ID_SALT_PREFIX: &str = "ghostex-analytics-v1:";

const CODEX_AUTH_RELATIVE_PATH: &str = ".codex/auth.json";
const CLAUDE_CONFIG_RELATIVE_PATH: &str = ".claude.json";

/// Which link of the chain produced the id. Reported as the `identity_source`
/// property so we can tell how much of the user count is actually deduplicated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentitySource {
    Codex,
    Claude,
    Install,
}

impl IdentitySource {
    /// The taxonomy member for this source. `&'static str` by construction, so
    /// nothing runtime-owned reaches the wire through this path.
    pub fn as_enum(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Install => "install",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedIdentity {
    /// Lowercase hex SHA-256 of the salted raw id. The only thing that leaves.
    pub distinct_id: String,
    pub source: IdentitySource,
}

/// Walk the chain and hash whatever it finds. Always succeeds: the last link is
/// the serverId, which the caller already holds.
pub fn resolve(paths: &GxserverPaths, server_id: &str) -> ResolvedIdentity {
    let home = agent_home_dir(paths);
    if let Some(account_id) = read_codex_account_id(&home) {
        return ResolvedIdentity {
            distinct_id: hash_with_salt(&account_id),
            source: IdentitySource::Codex,
        };
    }
    if let Some(user_id) = read_claude_user_id(&home) {
        return ResolvedIdentity {
            distinct_id: hash_with_salt(&user_id),
            source: IdentitySource::Claude,
        };
    }
    ResolvedIdentity {
        distinct_id: hash_with_salt(server_id),
        source: IdentitySource::Install,
    }
}

/*
The same home-dir rule `agent_prompt_search::resolve_search_paths` uses: an
isolated profile (`GHOSTEX_HOME`, or an explicit daemon home) must read the
agent CLI data inside that profile rather than following the process environment
back out into the real user's `~/.codex`. Reading `$HOME` directly here would
make a throwaway test profile report the developer's real identity.
*/
fn agent_home_dir(paths: &GxserverPaths) -> PathBuf {
    paths
        .isolated_agent_home_dir
        .clone()
        .unwrap_or_else(|| paths.home_dir.clone())
}

/// SHA-256 of `salt || raw`, lowercase hex. One-way: the value that lands in
/// PostHog cannot be turned back into an account id, and cannot be matched
/// against another product's hash of the same account id.
fn hash_with_salt(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DISTINCT_ID_SALT_PREFIX.as_bytes());
    hasher.update(raw.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_codex_account_id(home: &Path) -> Option<String> {
    read_json_string_field(
        &home.join(CODEX_AUTH_RELATIVE_PATH),
        &["tokens", "account_id"],
    )
}

fn read_claude_user_id(home: &Path) -> Option<String> {
    read_json_string_field(&home.join(CLAUDE_CONFIG_RELATIVE_PATH), &["userID"])
}

/*
Read one string out of one JSON file, or nothing. Every failure mode — file
absent, unreadable, not JSON, key missing, key not a string, key empty — folds
into `None` WITHOUT producing a message, because the only messages worth
printing here would have to describe the very value that must never be
described. The caller simply tries the next link.
*/
fn read_json_string_field(path: &Path, key_path: &[&str]) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<Value>(&text).ok()?;
    let mut cursor = &parsed;
    for key in key_path {
        cursor = cursor.get(key)?;
    }
    let value = cursor.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}
