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
    let path = match provider {
        Provider::Codex => root.join("auth.json"),
        Provider::Claude if root == home.join(".claude") => home.join(".claude.json"),
        Provider::Claude => root.join(".claude.json"),
    };
    let data: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let identity = match provider {
        Provider::Codex => data.pointer("/tokens/account_id")?.as_str()?.to_string(),
        Provider::Claude => {
            let account = data.get("oauthAccount")?;
            let email = account.get("emailAddress")?.as_str()?;
            let organization = account.get("organizationUuid")?.as_str()?;
            if email.is_empty() || organization.is_empty() {
                return None;
            }
            format!("{}:{organization}", email.to_lowercase())
        }
    };
    registry
        .accounts
        .iter()
        .find(|account| {
            account.provider == provider && !identity.is_empty() && account.identity == identity
        })
        .map(|account| account.id.as_str())
}

fn configured_home(session: &Value, provider: Provider) -> Option<PathBuf> {
    if let Some(root) = session
        .pointer("/runtimeSettings/externalAgentHome")
        .and_then(Value::as_str)
    {
        return Path::new(root).is_absolute().then(|| PathBuf::from(root));
    }
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
