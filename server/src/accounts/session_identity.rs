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
        Provider::Claude if root == ghostex_paths::agent_homes::claude_default_home(&home) => {
            ghostex_paths::agent_homes::claude_json_path(&home)
        }
        Provider::Claude => root.join(".claude.json"),
    };
    let stamp = std::fs::metadata(&path)
        .ok()
        .map(|meta| (meta.modified().ok(), meta.len()));
    let memo = || LOGIN_MEMO.get_or_init(Default::default);
    if let (Some(stamp), Ok(memo)) = (stamp, memo().lock()) {
        if let Some((_, identity)) = memo.get(&path).filter(|(known, _)| *known == stamp) {
            return identity.clone();
        }
    }
    let identity = read_login_identity(provider, &path);
    if let (Some(stamp), Ok(mut memo)) = (stamp, memo().lock()) {
        memo.insert(path, (stamp, identity.clone()));
    }
    identity
}

/// The login each account file named when it had this modification time and size.
///
/// CDXC:AgentProviders 2026-11-10 WHY: the status probe of every working chat asked for its login
/// each second, and each ask parsed a whole `.claude.json` (136 to 420 KB here) for two fields:
/// 179 of 3,605 busy samples of gxserver. The file changes only on a login or a settings write, so
/// its login is remembered until its modification time or size changes.
type LoginStamp = (Option<std::time::SystemTime>, u64);
static LOGIN_MEMO: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, (LoginStamp, Option<String>)>>,
> = std::sync::OnceLock::new();

fn read_login_identity(provider: Provider, path: &Path) -> Option<String> {
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
