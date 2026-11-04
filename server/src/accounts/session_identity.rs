use super::model::{Provider, Registry};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// CDXC:AgentProviders 2026-09-09 WHY:
/// Imported and older CLI sessions can use a saved login without a Ghostex accountId binding.
/// Resolve display stats by the configured home's login identity, never by the default account or transcript folder's slot number; shared history can cross account homes.
/// This read-only association does not change the session's launch command or continuation policy.
pub(crate) fn display_account_id<'a>(
    registry: &'a Registry,
    provider: Provider,
    session: &'a Value,
    home: &Path,
) -> Option<&'a str> {
    if let Some(id) = session
        .pointer("/runtimeSettings/accountId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return Some(id);
    }
    let root = configured_home(session, provider)?;
    let identity = login_identity(provider, &root, home)?;
    let accounts = || {
        registry
            .accounts
            .iter()
            .filter(|account| account.provider == provider)
    };
    // An exact identity first, so a legacy workspace-only Codex identity never takes a session that a member's own account matches.
    accounts()
        .find(|account| !identity.is_empty() && account.identity == identity)
        .or_else(|| {
            accounts().find(|account| {
                super::codex_identity::identity_matches(provider, &account.identity, &identity)
            })
        })
        .map(|account| account.id.as_str())
}

pub(crate) fn login_identity(provider: Provider, root: &Path, home: &Path) -> Option<String> {
    let path = match provider {
        Provider::Codex => root.join("auth.json"),
        Provider::Claude if root == home.join(".claude") => home.join(".claude.json"),
        Provider::Claude => root.join(".claude.json"),
    };
    let data: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    match provider {
        Provider::Codex => super::codex_identity::codex_auth_identity(&data),
        Provider::Claude => {
            let account = data.get("oauthAccount")?;
            let email = account.get("emailAddress")?.as_str()?;
            let organization = account.get("organizationUuid")?.as_str()?;
            if email.is_empty() || organization.is_empty() {
                return None;
            }
            Some(format!("{}:{organization}", email.to_lowercase()))
        }
    }
}

/// The login of the CLI home the session's current transcript was written in, which is the login the running CLI uses whatever account the session is assigned to.
pub(crate) fn transcript_login_identity(
    provider: Provider,
    session: &Value,
    home: &Path,
) -> Option<String> {
    login_identity(provider, &transcript_home(session, provider)?, home)
}

fn configured_home(session: &Value, provider: Provider) -> Option<PathBuf> {
    if let Some(root) = session
        .pointer("/runtimeSettings/externalAgentHome")
        .and_then(Value::as_str)
    {
        return Path::new(root).is_absolute().then(|| PathBuf::from(root));
    }
    transcript_home(session, provider)
}

fn transcript_home(session: &Value, provider: Provider) -> Option<PathBuf> {
    let transcript = Path::new(
        session
            .pointer("/runtimeSettings/agentSessionPath")?
            .as_str()?,
    );
    if !transcript.is_absolute() {
        return None;
    }
    let directory = match provider {
        Provider::Codex => "sessions",
        Provider::Claude => "projects",
    };
    transcript
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == directory))?
        .parent()
        .map(Path::to_path_buf)
}
