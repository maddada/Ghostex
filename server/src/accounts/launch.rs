use super::{helpers, model::*, store};
use crate::{
    agents::{command_word, quote_shell_arg, reusable_account_command},
    domain::{DomainRepository, DomainStateError},
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// CDXC:AgentProviders 2026-09-07 DECISION:
/// Picking an account for a custom agent keeps its model, instructions and other command arguments. Replace only the provider invocation, retaining the original command for later account switches and resumes.
pub(crate) fn with_account_command(
    base: &str,
    provider: Provider,
    wrapper: &str,
) -> Result<String, DomainStateError> {
    let invalid = || {
        DomainStateError::bad_request(format!(
        "Account selection needs a {} command (or {} run). Update this custom agent's command while keeping its arguments.",
        provider.id(), provider.helper()
    ))
    };
    let mut offset = 0;
    while let Some((start, end, word)) = command_word(base, offset) {
        offset = end;
        // Environment assignments and the standard invocation prefixes keep their original shell spelling.
        if word.contains('=') || matches!(word.as_str(), "env" | "exec" | "command") {
            continue;
        }
        let executable = Path::new(&word)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if executable == provider.id() {
            return Ok(format!("{}{wrapper}{}", &base[..start], &base[end..]));
        }
        if executable == provider.helper() {
            let (_, run_end, run) = command_word(base, offset).ok_or_else(invalid)?;
            if run != "run" {
                return Err(invalid());
            }
            let (_, slot_end, _) = command_word(base, run_end).ok_or_else(invalid)?;
            offset = slot_end;
            while let Some((_, end, word)) = command_word(base, offset) {
                if word == "--" {
                    return Ok(format!("{}{wrapper}{}", &base[..start], &base[end..]));
                }
                if !matches!(
                    word.as_str(),
                    "--share-history" | "--no-share" | "--require-session"
                ) {
                    return Ok(format!("{}{wrapper}{}", &base[..start], &base[offset..]));
                }
                offset = end;
            }
            return Ok(format!("{}{wrapper}", &base[..start]));
        }
        return Err(invalid());
    }
    Err(invalid())
}

/// Whether a custom agent's command chose its own login, so an automatic account would override or reject it: it assigns the provider's profile directory (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`; also PowerShell's `$env:` form) before its executable, or it never runs the provider CLI or its account helper (a wrapper such as `claude-personal`).
/// Any command that names the provider CLI or helper anywhere (an absolute or quoted path, a Windows `.exe`/`.cmd` shim, `& '…'`, `npx @anthropic-ai/claude-code`, `mise exec -- claude`) keeps the account flow it had before, so only those two shapes change behaviour. Clients can ask this for an agent's command to show that it uses its own login.
pub(crate) fn uses_own_login(base: &str, provider: Provider) -> bool {
    let profile_var = match provider {
        Provider::Claude => "CLAUDE_CONFIG_DIR",
        Provider::Codex => "CODEX_HOME",
    };
    // An empty or unfinished command keeps the error account assignment gave it before.
    if base.trim().is_empty() || reusable_account_command(base, provider.id()).is_err() {
        return false;
    }
    let mut offset = 0;
    let mut skip_value = false;
    let mut in_powershell_assignment = false;
    while let Some((_, end, word)) = command_word(base, offset) {
        offset = end;
        if std::mem::take(&mut skip_value) {
            continue;
        }
        // `$env:NAME = 'value';` spans words up to its `;`, and PowerShell names ignore case.
        if let Some(name) = word
            .get(..5)
            .filter(|prefix| prefix.eq_ignore_ascii_case("$env:"))
            .map(|_| word[5..].split('=').next().unwrap_or_default())
        {
            if name.eq_ignore_ascii_case(profile_var) {
                return true;
            }
            in_powershell_assignment = !word.ends_with(';');
            continue;
        }
        if in_powershell_assignment {
            in_powershell_assignment = !word.ends_with(';');
            continue;
        }
        if let Some((name, _)) = word.split_once('=') {
            if name == profile_var {
                return true;
            }
            continue;
        }
        // PowerShell's call operator and the standard invocation prefixes come before the executable.
        if matches!(word.as_str(), "&" | "env" | "exec" | "command" | "export") {
            continue;
        }
        // Options of the prefixes above, such as `env -u NAME claude`; `env -u`/`-C` and `exec -a` take the next word as their value.
        if word.starts_with('-') {
            skip_value = matches!(word.as_str(), "-u" | "--unset" | "-C" | "--chdir" | "-a");
            continue;
        }
        break;
    }
    !mentions_provider(base, provider)
}

/// Whether any word of a command runs the provider CLI or its account helper, read from the raw text so unquoted Windows paths (whose backslashes a POSIX word reader drops) still count.
fn mentions_provider(command: &str, provider: Provider) -> bool {
    let package = match provider {
        Provider::Claude => "@anthropic-ai/claude-code",
        Provider::Codex => "@openai/codex",
    };
    command
        .split(|c: char| c.is_whitespace() || "'\"`;&|()=,".contains(c))
        .any(|token| {
            let token = token.to_ascii_lowercase();
            let name = token.rsplit(['/', '\\']).next().unwrap_or(&token);
            let stem = [".exe", ".cmd", ".bat", ".ps1"]
                .iter()
                .find_map(|suffix| name.strip_suffix(suffix))
                .unwrap_or(name);
            stem == provider.id() || stem == provider.helper() || token.contains(package)
        })
}

pub(crate) fn provider(project: &Value, session: &Value) -> Option<Provider> {
    match crate::agents::session_agent_family_id(project, session).as_deref() {
        Some("claude") => Some(Provider::Claude),
        Some("codex") => Some(Provider::Codex),
        _ => None,
    }
}
/// The saved account a new session of this agent starts on when its creator names one (`ghostex coordinator start-thread --account`): its number, email or id, resolved and refused the way `switch-account` does.
pub(crate) fn requested_account(
    db: &Connection,
    agent_id: &str,
    icon: Option<&str>,
    command: Option<&str>,
    wanted: &str,
) -> Result<String, DomainStateError> {
    let provider = agent_provider(agent_id, icon).ok_or_else(|| {
        DomainStateError::bad_request(
            "This agent is not Claude or Codex, so it has no account to choose. Leave out --account.",
        )
    })?;
    if command.is_some_and(|command| uses_own_login(command, provider)) {
        return Err(DomainStateError::bad_request(
            "This agent's command chooses its own login, so it takes no account. Leave out --account.",
        ));
    }
    let registry = store::read(db)?;
    let snapshot = super::runtime::current_snapshot();
    let (account, _) = super::cli::resolve_ready_account(&registry, &snapshot, provider, wanted)?;
    Ok(account.id.clone())
}
/// The account provider of a launcher agent, read the way `apply_new_session` reads it: the icon family, else the agent id.
pub(crate) fn agent_provider(agent_id: &str, icon: Option<&str>) -> Option<Provider> {
    match icon.unwrap_or(agent_id) {
        "claude" => Some(Provider::Claude),
        "codex" => Some(Provider::Codex),
        _ => None,
    }
}
/// Whether a launcher agent (the sidebar HUD's `agentId`, `icon` and `command`) starts sessions on its own login, so the launcher offers it no account.
pub(crate) fn agent_uses_own_login(agent_id: &str, icon: Option<&str>, command: &str) -> bool {
    agent_provider(agent_id, icon).is_some_and(|provider| uses_own_login(command, provider))
}
/// CDXC:AgentProviders 2026-10-09 DECISION:
/// User (#212): a session that runs on its own login (see `uses_own_login`) offers no account choice anywhere: the chat's Switch Account panel, the sidebar row and the terminal bar flyouts, and the launcher and New Thread picker show none, and gxserver refuses a manual or automatic switch, because wrapping the command in an account helper would replace the login the command chose. A session that already has an account keeps the account flow.
/// SEE-ALSO: `usesOwnLogin` on the chat read (session_chat_read.rs, gx-chat-core `account_providers`), on the accounts answer's `session` (endpoint.rs, gx-core `session_account_page`) and on the HUD agent buttons (sidebar_hud/buttons.rs, gx-core `LauncherAgent`, the desktop New Thread picker).
pub(crate) fn session_uses_own_login(provider: Provider, session: &Value) -> bool {
    let runtime = &session["runtimeSettings"];
    runtime["accountId"].as_str().is_none()
        && runtime["accountBaseCommand"]
            .as_str()
            .or_else(|| runtime["agentCommand"].as_str())
            .is_some_and(|base| uses_own_login(base, provider))
}
pub(crate) fn command(home: &Path, account: &SavedAccount) -> Result<String, DomainStateError> {
    validate_identity(home, account)?;
    let executable = helpers::executable(home, account.provider.helper()).ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Install {} in Settings > Accounts first.",
            helpers::tool_label(account.provider.helper())
        ))
    })?;
    // CDXC:AgentProviders 2026-09-18 WHY:
    // Windows sessions run in PowerShell, where a quoted executable path in command position is a string literal that prints instead of running. The call operator is required, as in agent_cli/endpoint.rs and zmx/scripts_windows.rs.
    let invoke = if cfg!(windows) { "& " } else { "" };
    Ok(format!(
        "{invoke}{} run {} --share-history --",
        quote_shell_arg(&executable.to_string_lossy()),
        quote_shell_arg(&account.selector)
    ))
}
/// The program and arguments that run `claude` on a saved Claude account (`cswap run <slot>
/// --share-history --`, then Claude's own arguments), for a caller that spawns the CLI itself
/// rather than typing a command into a shell: the cloud runner. Refused like [`launch_command`].
///
/// CDXC:ClaudeCloud 2026-10-23 WHY: a Claude cloud session runs on the claude.ai login of the account that started it, so its start, follow-ups and attach run `claude` through the same account wrapper a local Claude session's command uses, `--share-history` included.
pub(crate) fn claude_account_argv(
    db: &Connection,
    account_id: &str,
) -> Result<Vec<String>, DomainStateError> {
    let registry = store::read(db)?;
    let account = registry
        .accounts
        .iter()
        .find(|account| account.id == account_id && account.provider == Provider::Claude)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "The Claude account this cloud session started on is no longer registered.",
            )
        })?;
    let home = home()?;
    launch_command(&home, account)?;
    let executable = helpers::executable(&home, account.provider.helper()).ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Install {} in Settings > Accounts first.",
            helpers::tool_label(account.provider.helper())
        ))
    })?;
    // A slot whose account is the signed-out default login starts in its own profile, as a local
    // start's command does (default_login.rs); `env` carries the variable for an argv spawn.
    let mut argv = super::default_login::signed_out_default_config_dir(&account.selector)
        .map(|profile| {
            vec![
                "/usr/bin/env".to_string(),
                format!("CLAUDE_CONFIG_DIR={profile}"),
            ]
        })
        .unwrap_or_default();
    argv.extend([
        executable.to_string_lossy().into_owned(),
        "run".to_string(),
        account.selector.clone(),
        "--share-history".to_string(),
        "--".to_string(),
    ]);
    Ok(argv)
}

/// `command` for a session that is about to start on `account`: refused when the helper would only print an error.
pub(crate) fn launch_command(
    home: &Path,
    account: &SavedAccount,
) -> Result<String, DomainStateError> {
    let command = command(home, account)?;
    if account.provider != Provider::Claude {
        return Ok(command);
    }
    let support = helpers::executable(home, account.provider.helper())
        .and_then(|executable| share_history_support(&executable));
    let refusal = match support {
        None | Some(ShareHistory::Supported) => return Ok(command),
        // CDXC:AgentProviders 2026-10-16 DECISION in claude_swap_windows.rs: Windows gets Ghostex's own build, which shares history.
        Some(_) if super::claude_swap_windows::applies() => {
            if super::claude_swap_switch::pending().is_some() {
                return Err(DomainStateError::bad_request("Claude Swap on this computer can't share conversation history on Windows. Ghostex's Windows build of Claude Swap is ready and replaces it as soon as no program uses the installed one; Settings > Accounts shows when. Start the session again then."));
            }
            match super::helper_tools::install_windows_build_in_background(home) {
                Ok(()) => "Claude Swap on this computer can't share conversation history on Windows, so Ghostex is installing its own Windows build of Claude Swap now (Settings > Accounts shows the progress). Start the session again in a minute.".to_string(),
                Err(reason) => format!("Claude Swap on this computer can't share conversation history on Windows, so this account's conversations would never reach the chat. {reason}"),
            }
        }
        Some(_) => "Claude Swap on this computer is too old to share conversation history between accounts. Update it in Settings > Accounts, then try again.".to_string(),
    };
    Err(DomainStateError::bad_request(refusal))
}

/// Whether a cswap can start an account with `--share-history` here, from its `run --help`.
#[derive(Clone, Copy)]
enum ShareHistory {
    Supported,
    /// The flag does not exist (before 0.16.0).
    Missing,
    /// Every released cswap refuses the flag on Windows.
    RefusedOnWindows,
}

/// What `cswap run --help` said, per executable and modification time, so an update is read again.
type ShareHistoryReadings =
    std::collections::HashMap<PathBuf, (std::time::SystemTime, ShareHistory)>;
static SHARE_HISTORY_HELP: std::sync::LazyLock<std::sync::Mutex<ShareHistoryReadings>> =
    std::sync::LazyLock::new(Default::default);

/// CDXC:AgentProviders 2026-10-16 WHY:
/// Ghostex starts every Claude account as `cswap run <N> --share-history`, and without the flag the account's conversations fork into its own profile where the chat never looks, so the flag is never dropped. No released Claude Swap (0.16.0, which added the flag, through 0.28.0) runs it on Windows: `cswap run` printed "--share-history is not supported on Windows yet", PowerShell stayed open, and a customer's chat sat at "starting" with "hello" queued for good (2026-10-16). Ghostex's Windows build (claude_swap_windows.rs) and any cswap built from realiti4/claude-swap#429 can, and the version number cannot tell them apart, but cswap's own `run --help` can: every refusing release ends the flag's help with "Not supported on Windows.", and the PR removes that sentence. `None` when the help could not be read, which refuses nothing.
fn share_history_support(executable: &Path) -> Option<ShareHistory> {
    let modified = std::fs::metadata(executable)
        .and_then(|metadata| metadata.modified())
        .ok()?;
    let mut cache = SHARE_HISTORY_HELP
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some((at, support)) = cache.get(executable) {
        if *at == modified {
            return Some(*support);
        }
    }
    let output = crate::platform::process::background_command(executable)
        .args(["run", "--help"])
        .env("COLUMNS", "200")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    // argparse wraps help text at the terminal width, so a sentence can span lines.
    let help = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let support = if !help.contains("--share-history") {
        ShareHistory::Missing
    } else if cfg!(windows) && help.contains("Not supported on Windows") {
        ShareHistory::RefusedOnWindows
    } else {
        ShareHistory::Supported
    };
    cache.insert(executable.to_path_buf(), (modified, support));
    Some(support)
}

pub(crate) fn assign(
    runtime: &mut Map<String, Value>,
    account: &SavedAccount,
    command: String,
) -> Result<String, DomainStateError> {
    let base = runtime
        .get("accountBaseCommand")
        .or_else(|| runtime.get("agentCommand"))
        .and_then(Value::as_str)
        .unwrap_or(account.provider.id());
    let base = reusable_account_command(base, account.provider.id())?;
    let command = with_account_command(&base, account.provider, &command)?;
    runtime.insert("accountBaseCommand".into(), json!(base));
    for (k, v) in [
        ("accountId", json!(account.id)),
        ("accountProvider", json!(account.provider)),
        ("accountName", json!(account.name)),
        ("accountSlot", json!(account.selector)),
        ("accountCommand", json!(command)),
        ("agentCommand", json!(command)),
    ] {
        runtime.insert(k.into(), v);
    }
    Ok(command)
}
pub(crate) fn apply_new_session(
    db: &Connection,
    agent_id: &str,
    icon: Option<&str>,
    workspace_claude_account_id: Option<&str>,
    runtime: &mut Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    let Some(provider) = agent_provider(agent_id, icon) else {
        return Ok(None);
    };
    // CDXC:AgentProviders 2026-10-08 WHY:
    // Upstream #208: a custom agent whose command sets its own profile (CLAUDE_CONFIG_DIR or CODEX_HOME) or runs a wrapper such as `claude-personal` had that profile replaced by cswap's session profile, or failed to launch, as soon as any account was registered. Such a command already chose its login, so it runs as-is. The sidebar launcher sends the rule's account as `accountId`, so an automatic choice cannot be told apart from a picked one here.
    if runtime
        .get("accountBaseCommand")
        .or_else(|| runtime.get("agentCommand"))
        .and_then(Value::as_str)
        .is_some_and(|base| uses_own_login(base, provider))
    {
        return Ok(None);
    }
    let registry = store::read(db)?;
    // CDXC:AgentProviders 2026-09-11 DECISION: User: use the current CLI login until an account is added to Ghostex for that provider (2026-09-09); once accounts exist, a launch without an explicit account uses the provider's Account for new sessions rule from Settings (Most limit remaining by default, see default_account.rs), which supersedes the lowest-slot choice. When that rule yields no account the launch keeps the current CLI login, so a normal CLI launch needs no account switcher.
    let snapshot = super::runtime::current_snapshot();
    // CDXC:Workspaces 2026-10-09 DECISION:
    // User: each workspace can pick which of the user's Claude accounts its agents use; it applies when a session starts in one of the workspace's projects, unless the user picked an account for that session. A picked account (`accountId`) wins, then the workspace's account (while it is still registered), then the provider's rule.
    let workspace_account = workspace_claude_account_id
        .filter(|_| provider == Provider::Claude)
        .filter(|id| {
            registry.accounts.iter().any(|account| {
                account.id == *id && account.provider == provider && !account.user_disabled
            })
        })
        .map(str::to_string);
    let id = runtime
        .get("accountId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or(workspace_account)
        .or_else(|| {
            super::default_account::quick_launch_account(&registry, &snapshot, provider)
                .map(|a| a.id.clone())
        });
    let Some(id) = id else {
        return Ok(None);
    };
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == id && a.provider == provider)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "The selected account is no longer registered. Choose another account.",
            )
        })?;
    let home = home()?;
    let cmd = launch_command(&home, account)?;
    let assigned = assign(runtime, account, cmd)?;
    super::default_account::record_last_used(db, &registry, provider, &account.id)?;
    Ok(Some(assigned))
}
/// CDXC:AgentProviders 2026-10-17 WHY:
/// The one-shot jobs (Generate Commit Message, Catch me up, Generate Name and the automatic title) run the bare `claude` or `codex` through the user's interactive shell, which lets the shell's own `claude` function or alias choose the login. On one Mac that function ran `cswap run` for the personal account last remembered in a cache file, a login the user had turned off, and every job failed with "Account-3 has no stored credentials" (2026-10-17). The job now names its account itself, like a session start does: the session's own account while that login works, else the provider's new-session account (default_account.rs), never one that is turned off, disabled or signed out. With no usable account the command stays as it was, on the CLI's own login.
/// SEE-ALSO: server/src/server/title_generation/title_command.rs, server/src/server/commit_message_generation.rs, server/src/session_chat_catch_up.rs (the three one-shot launchers).
pub(crate) fn one_shot_command(
    state: &crate::server::AppState,
    agent_id: &str,
    command: &str,
    session: Option<&Value>,
) -> String {
    one_shot_account_command(state, agent_id, command, session)
        .unwrap_or_else(|| command.to_string())
}

fn one_shot_account_command(
    state: &crate::server::AppState,
    agent_id: &str,
    command: &str,
    session: Option<&Value>,
) -> Option<String> {
    let provider = agent_provider(agent_id, None)?;
    // A command that chose its own login (a profile variable, a wrapper such as `claude-personal`) runs as written.
    if uses_own_login(command, provider) {
        return None;
    }
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let registry = store::read(&db).ok()?;
    let snapshot = super::runtime::current_snapshot();
    let session_account = session
        .and_then(|session| session.pointer("/runtimeSettings/accountId"))
        .and_then(Value::as_str);
    let usable = |account: &SavedAccount| {
        account.provider == provider
            && !account.user_disabled
            && account.disabled.is_none()
            && snapshot
                .accounts
                .iter()
                .find(|row| super::endpoint::account_id(row) == account.id)
                .is_none_or(|row| row.status == "ready")
    };
    let account = session_account
        .and_then(|id| registry.accounts.iter().find(|account| account.id == id))
        .filter(|account| usable(account))
        .or_else(|| {
            super::default_account::quick_launch_account(&registry, &snapshot, provider)
                .filter(|account| usable(account))
        })?;
    let wrapper = launch_command(&home().ok()?, account).ok()?;
    let base = reusable_account_command(command, provider.id()).ok()?;
    with_account_command(&base, provider, &wrapper)
        .ok()
        .map(|command| super::default_login::route_around_signed_out_default(&command))
}
/// CDXC:AgentProviders 2026-09-18 WHY:
/// Windows has no HOME, so reading it alone failed every account launch and resume with "The server's home directory is unavailable."
pub(crate) fn home() -> Result<PathBuf, DomainStateError> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|p| p.is_absolute())
        .ok_or_else(|| DomainStateError::bad_request("The server's home directory is unavailable."))
}
pub(crate) fn validate_session(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<(), DomainStateError> {
    let Some(id) = session
        .pointer("/runtimeSettings/accountId")
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    let registry = store::read(repository.db)?;
    let Some(account) = registry.accounts.iter().find(|a| a.id == id) else {
        let runtime = &session["runtimeSettings"];
        let provider = match runtime["accountProvider"].as_str() {
            Some("codex") => Some(Provider::Codex),
            Some("claude") => Some(Provider::Claude),
            _ => None,
        };
        let label = account_label(
            runtime["accountSlot"].as_str().unwrap_or(""),
            runtime["accountName"].as_str().unwrap_or(""),
        );
        return Err(account_refusal(
            session,
            provider,
            &format!("{label}, which was removed from Ghostex. Switch the session to another account before resuming it"),
        ));
    };
    validate_identity(&home()?, account).map_err(|_| {
        account_refusal(
            session,
            Some(account.provider),
            &format!(
                "{}, whose saved login changed or is unavailable. Refresh Accounts and reconnect it before resuming",
                account_label(&account.selector, &account.name)
            ),
        )
    })
}

/// `account 3 (Work)`: the number and name Settings > Accounts and the 5h/7d meters show.
pub(crate) fn account_label(selector: &str, name: &str) -> String {
    match (selector.trim(), name.trim()) {
        ("", "") => "an account".to_string(),
        (selector, "") => format!("account {selector}"),
        ("", name) => format!("account {name}"),
        (selector, name) => format!("account {selector} ({name})"),
    }
}

/// CDXC:AgentProviders 2026-10-24 DECISION:
/// User: "it's not saying which agent and which account it's talking about also should have an action button to take me to the place to fix this". A resume refused because of the session's account names the session (its title), the agent and the account (number and name, as Accounts and the 5h/7d meters show them) and carries the code `accountUnavailable`, which clients read to offer Open Accounts. `problem` continues "“<title>” (<Agent>) is on <Agent> ".
/// SEE-ALSO: wake_account.rs (`refuse_turned_off`), apps/desktop/src/app/workspace_events.rs (the toast and its button), server/src/server/http_endpoints.rs (the status).
pub(crate) fn account_refusal(
    session: &Value,
    provider: Option<Provider>,
    problem: &str,
) -> DomainStateError {
    let title = session["title"]
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty());
    let title: String = match title {
        Some(title) if title.chars().count() > 48 => {
            format!(
                "“{}…”",
                title.chars().take(47).collect::<String>().trim_end()
            )
        }
        Some(title) => format!("“{title}”"),
        None => "This session".to_string(),
    };
    let agent = match provider {
        Some(Provider::Claude) => "Claude",
        Some(Provider::Codex) => "Codex",
        None => "",
    };
    let message = if agent.is_empty() {
        format!("{title} is on {problem}.")
    } else {
        format!("{title} ({agent}) is on {agent} {problem}.")
    };
    DomainStateError {
        code: ACCOUNT_UNAVAILABLE,
        message,
    }
}

/// The error code of a resume the session's account refuses (`account_refusal`).
pub(crate) const ACCOUNT_UNAVAILABLE: &str = "accountUnavailable";
/// xswap's data folder: `XSWAP_HOME`, else `codex-swap` in its platform data folder. Its saved Codex accounts are `accounts/<slot>` inside it.
pub(crate) fn xswap_root(home: &Path) -> PathBuf {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    // CDXC:AgentProviders 2026-09-18 WHY:
    // xswap's registry is %LOCALAPPDATA%\codex-swap on Windows and $XDG_DATA_HOME/codex-swap elsewhere (its fsutil::default_data_dir).
    // Reading the POSIX location on Windows made every saved Codex account fail identity validation as changed or unavailable.
    let xswap_data_home = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or(data_home)
    } else {
        data_home
    };
    std::env::var_os("XSWAP_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| xswap_data_home.join("codex-swap"))
}

pub(crate) fn validate_identity(
    home: &Path,
    account: &SavedAccount,
) -> Result<(), DomainStateError> {
    let path = match account.provider {
        Provider::Codex => xswap_root(home).join("accounts.json"),
        Provider::Claude => super::claude_resets::swap_root(home).join("sequence.json"),
    };
    let fail = || {
        DomainStateError::bad_request("The saved account changed or is unavailable. Refresh Accounts and reconnect it before resuming.")
    };
    let raw = std::fs::read(path).map_err(|_| fail())?;
    let data: Value = serde_json::from_slice(&raw).map_err(|_| fail())?;
    let identity = if account.provider == Provider::Claude {
        let row = &data["accounts"][&account.selector];
        format!(
            "{}:{}",
            row["email"].as_str().unwrap_or("").to_lowercase(),
            row["organizationUuid"].as_str().unwrap_or("")
        )
    } else {
        let row = data["accounts"]
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|r| {
                    r["number"].as_u64().map(|n| n.to_string()).as_deref()
                        == Some(&account.selector)
                })
            })
            .ok_or_else(fail)?;
        super::codex_identity::codex_identity(
            row.pointer("/identity/accountId")
                .and_then(Value::as_str)
                .unwrap_or(""),
            row.pointer("/identity/userId").and_then(Value::as_str),
        )
    };
    if !super::codex_identity::identity_matches(account.provider, &account.identity, &identity) {
        return Err(fail());
    }
    Ok(())
}
/// CDXC:AgentProviders 2026-09-19 DECISION:
/// User: "I literally didn't touch the default in the session or in settings" and the session still did not switch accounts at a limit. A session follows the provider's continuation defaults from Settings > Accounts as they are now, unless it has its own Customize settings. This supersedes the 2026-09-05 copy saved at launch (`accountPolicyDefault`), which forks, restored sessions, CLIs adopted from a terminal, and agent re-detection never received or lost, leaving them off while Settings said on.
pub(crate) fn effective_policy(registry: &Registry, provider: Provider, session: &Value) -> Policy {
    session
        .pointer("/runtimeSettings/accountPolicyOverride")
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .or_else(|| registry.defaults.get(&provider).cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_refusal_names_the_session_agent_and_account() {
        let session = json!({"title": "Fix login bug"});
        let error = account_refusal(
            &session,
            Some(Provider::Claude),
            &format!(
                "{}, which was removed from Ghostex. Switch the session to another account before resuming it",
                account_label("3", "Work")
            ),
        );
        assert_eq!(error.code, ACCOUNT_UNAVAILABLE);
        assert_eq!(
            error.message,
            "“Fix login bug” (Claude) is on Claude account 3 (Work), which was removed from Ghostex. Switch the session to another account before resuming it."
        );
        let untitled = account_refusal(&json!({}), None, &account_label("", ""));
        assert_eq!(untitled.message, "This session is on an account.");
    }

    /// Commands that name the provider CLI or its account helper, so they keep the account flow.
    const PROVIDER_COMMANDS: &[(Provider, &str)] = &[
        (Provider::Claude, "claude"),
        (
            Provider::Claude,
            "claude --dangerously-skip-permissions --model opus --effort high",
        ),
        (
            Provider::Claude,
            "claude --resume 0b1c2d --model 'opus[1m]'",
        ),
        (
            Provider::Claude,
            "claude --append-system-prompt-file '/Users/me/.ghostex/coordinator.md'",
        ),
        (Provider::Claude, "/opt/homebrew/bin/claude --model sonnet"),
        (Provider::Claude, "~/.local/bin/claude"),
        (
            Provider::Claude,
            "'/Users/me/My Tools/claude' --model sonnet",
        ),
        (Provider::Claude, "env FOO=bar claude"),
        (Provider::Claude, "env -u ANTHROPIC_API_KEY claude"),
        (Provider::Claude, "env -u CLAUDE_CONFIG_DIR claude"),
        (
            Provider::Claude,
            "GHOSTEX_PROMPT_EDITING_ENABLED=1 VISUAL='ghostex-editor --wait' claude",
        ),
        (Provider::Claude, "CODEX_HOME=/tmp/x claude"),
        (Provider::Claude, "command claude"),
        (Provider::Claude, "exec claude --model opus"),
        (Provider::Claude, "export FOO=1; claude"),
        (
            Provider::Claude,
            "cswap run 2 --share-history -- --model opus",
        ),
        (
            Provider::Claude,
            "'/Users/me/.local/bin/cswap' run '1' --share-history --",
        ),
        (
            Provider::Claude,
            r"& 'C:\Users\me\.local\bin\cswap.exe' run '1' --share-history -- --model opus",
        ),
        (
            Provider::Claude,
            r"& 'C:\Users\me\.local\bin\claude.exe' --model opus",
        ),
        (Provider::Claude, r"& 'C:\Program Files\nodejs\claude.cmd'"),
        (
            Provider::Claude,
            r"C:\Users\me\AppData\Roaming\npm\claude.cmd --model opus",
        ),
        (
            Provider::Claude,
            r"& $env:LOCALAPPDATA\Programs\claude\claude.exe",
        ),
        (Provider::Claude, "claude.EXE"),
        (Provider::Claude, "claude.ps1 --model opus"),
        (Provider::Claude, "Claude"),
        (
            Provider::Claude,
            "$env:GHOSTEX_PROMPT_EDITING_ENABLED='1'; claude",
        ),
        (Provider::Claude, "$env:FOO = 'a b'; claude"),
        (
            Provider::Claude,
            "npx @anthropic-ai/claude-code --model opus",
        ),
        (Provider::Claude, "bunx @anthropic-ai/claude-code@latest"),
        (Provider::Claude, "mise exec -- claude"),
        (Provider::Claude, "claude 'unfinished"),
        (Provider::Claude, ""),
        (Provider::Codex, "codex"),
        (
            Provider::Codex,
            "codex --yolo -c model_reasoning_effort=high resume 019a",
        ),
        (Provider::Codex, "CLAUDE_CONFIG_DIR=/tmp/x codex"),
        (Provider::Codex, "xswap run 1 --share-history -- --yolo"),
        (
            Provider::Codex,
            r"& 'C:\Users\me\AppData\Local\Programs\codex-swap\xswap.exe' run '1' --share-history --",
        ),
        (
            Provider::Codex,
            r"& 'C:\Users\me\AppData\Roaming\npm\codex.cmd' --yolo",
        ),
        (Provider::Codex, "npx @openai/codex"),
    ];

    /// (provider, command, sets the profile directory): commands that keep their own login.
    const OWN_LOGIN_COMMANDS: &[(Provider, &str, bool)] = &[
        (
            Provider::Claude,
            "CLAUDE_CONFIG_DIR=\"$HOME/.claude-alt\" command claude",
            true,
        ),
        (
            Provider::Claude,
            "CLAUDE_CONFIG_DIR=~/.claude-profiles/work claude --model opus",
            true,
        ),
        (Provider::Claude, "env CLAUDE_CONFIG_DIR=/x claude", true),
        (
            Provider::Claude,
            "export CLAUDE_CONFIG_DIR=/x; claude",
            true,
        ),
        (
            Provider::Claude,
            r"$env:CLAUDE_CONFIG_DIR='C:\Users\me\.claude-work'; claude",
            true,
        ),
        (
            Provider::Claude,
            r"$env:claude_config_dir = 'C:\x'; & 'C:\Users\me\.local\bin\claude.exe'",
            true,
        ),
        (Provider::Claude, "claude-personal", false),
        (Provider::Claude, "claude-personal --model opus", false),
        (Provider::Claude, "~/bin/claude-work --resume abc", false),
        (Provider::Claude, "c2", false),
        (
            Provider::Codex,
            "CODEX_HOME=~/.codex-profiles/work codex",
            true,
        ),
        (
            Provider::Codex,
            r"$env:CODEX_HOME='C:\x'; codex --yolo",
            true,
        ),
        (Provider::Codex, "codex-work", false),
    ];

    /// What account assignment did before custom agents could keep their own login.
    fn assigned_before(base: &str, provider: Provider) -> bool {
        reusable_account_command(base, provider.id())
            .and_then(|base| with_account_command(&base, provider, "WRAPPER"))
            .is_ok()
    }

    #[test]
    fn provider_commands_keep_the_account_flow() {
        for (provider, base) in PROVIDER_COMMANDS {
            assert!(!uses_own_login(base, *provider), "{base}");
        }
    }

    #[test]
    fn own_login_commands_are_only_profile_assignments_or_other_executables() {
        // Every own-login command that does not set the profile directory was refused by account assignment before.
        for (provider, base, sets_profile) in OWN_LOGIN_COMMANDS {
            assert!(uses_own_login(base, *provider), "{base}");
            if !sets_profile {
                assert!(!assigned_before(base, *provider), "{base}");
            }
        }
    }

    #[test]
    fn provider_sessions_and_launcher_agents_keep_their_accounts() {
        for (provider, base) in PROVIDER_COMMANDS {
            for key in ["agentCommand", "accountBaseCommand"] {
                let session = json!({ "runtimeSettings": { key: base } });
                assert!(!session_uses_own_login(*provider, &session), "{base}");
            }
            assert!(!agent_uses_own_login(provider.id(), None, base), "{base}");
            assert!(
                !agent_uses_own_login("custom-1", Some(provider.id()), base),
                "{base}"
            );
        }
        // A built-in session with no saved command, and agents that are not Claude or Codex.
        assert!(!session_uses_own_login(
            Provider::Claude,
            &json!({ "runtimeSettings": {} })
        ));
        assert!(!agent_uses_own_login("gemini", None, "claude-personal"));
        assert!(!agent_uses_own_login(
            "custom-1",
            Some("gemini"),
            "CLAUDE_CONFIG_DIR=/x claude"
        ));
    }

    #[test]
    fn own_login_sessions_and_launcher_agents_offer_no_account() {
        for (provider, base, _) in OWN_LOGIN_COMMANDS {
            let session = json!({ "runtimeSettings": { "agentCommand": base } });
            assert!(session_uses_own_login(*provider, &session), "{base}");
            // The saved base command decides, as it does for a launch.
            let session = json!({
                "runtimeSettings": { "accountBaseCommand": base, "agentCommand": provider.id() }
            });
            assert!(session_uses_own_login(*provider, &session), "{base}");
            // A session that already runs on an account keeps the account flow.
            let session =
                json!({ "runtimeSettings": { "agentCommand": base, "accountId": "claude:1" } });
            assert!(!session_uses_own_login(*provider, &session), "{base}");
            assert!(
                agent_uses_own_login("custom-1", Some(provider.id()), base),
                "{base}"
            );
        }
    }
}
