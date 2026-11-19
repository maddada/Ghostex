//! Starting a Claude or Codex session On the hub: the session runs in a profile folder of
//! Ghostex's own for that hub (`~/.claude-profiles/ghostex-<hub>`, `~/.codex-profiles/ghostex-<hub>`),
//! which points the CLI at the hub with its client key, and the command only names that folder
//! (`CLAUDE_CONFIG_DIR=… claude`, `CODEX_HOME=… codex`).
//!
//! CDXC:AgentProviders 2026-11-18 WHY:
//! Naming the profile folder in the command is what makes a hub session "uses its own login" (launch.rs `uses_own_login`): it is never wrapped in Claude Swap or Codex Swap, never moved to another account at a limit, and resume and fork keep the folder. The key stays out of the command, the terminal and the session's saved settings: Claude reads it through `apiKeyHelper` from a 0600 file in the folder, Codex from the folder's 0600 `config.toml` (`experimental_bearer_token`). The folders sit where Ghostex already looks for profiles (hooks, transcripts, folder trust, skills), so hub sessions get all of them like any profile.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::accounts::model::{NewSessionAccount, Provider, Registry};
use crate::agents::quote_shell_arg;
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// The runtime setting a hub session carries: its hub's id (the limit wait reads the hub's pool).
pub(crate) const RUNTIME_KEY: &str = "cliproxyHubId";
/// What a launcher sends as the account to start a session on a hub: `cliproxy:<hub id>`.
pub(crate) const ACCOUNT_PREFIX: &str = "cliproxy:";
const KEY_FILE: &str = ".ghostex-hub-key";
const CODEX_PROVIDER: &str = "ghostex-hub";

pub(crate) fn profile_dir(home: &Path, provider: Provider, hub_id: &str) -> PathBuf {
    let parent = match provider {
        Provider::Claude => ".claude-profiles",
        Provider::Codex => ".codex-profiles",
    };
    home.join(parent).join(format!("ghostex-{hub_id}"))
}

fn private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes `body` to `path` readable by this user only, through a rename. Each write has a temp
/// name of its own, so two launches preparing the same profile folder at once never write into or
/// rename away each other's file.
fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(
        "{name}.{}-{}.ghostex-tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

fn read_object(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn io(error: std::io::Error) -> DomainStateError {
    DomainStateError::bad_request(format!("Couldn't set up the hub's profile folder: {error}"))
}

/// The `apiKeyHelper` command that prints the key file.
///
/// CDXC:AgentProviders 2026-11-19 WHY:
/// On native Windows Claude Code runs `apiKeyHelper` through `cmd.exe` (its `ComSpec`), not PowerShell or bash: a probe on Windows with Claude Code 2.1.328 saw `%OS%` expanded and `$0` left as text, and the `cat '…'` helper failed with "'cat' is not recognized", so a session On the hub had no key. cmd's own `type "…"` prints the file there; everywhere else the helper runs in a POSIX shell.
fn key_helper(key_file: &Path) -> String {
    let path = key_file.to_string_lossy();
    if cfg!(windows) {
        format!("type \"{path}\"")
    } else {
        format!("cat {}", quote_shell_arg(&path))
    }
}

/// Claude's folder: the key file, `settings.json` with the hub's address and the key helper (the
/// hooks Ghostex adds there are kept), and `.claude.json` marked as past Claude's first-run steps.
fn prepare_claude(
    dir: &Path,
    url: &str,
    key: &str,
    statusline: Option<&Path>,
) -> std::io::Result<()> {
    private_dir(dir)?;
    let key_file = dir.join(KEY_FILE);
    write_private(&key_file, key.as_bytes())?;
    let settings_path = dir.join("settings.json");
    let mut settings = read_object(&settings_path);
    let env = settings
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(env) = env {
        env.insert("ANTHROPIC_BASE_URL".into(), json!(url));
        env.remove("ANTHROPIC_AUTH_TOKEN");
        env.remove("ANTHROPIC_API_KEY");
    } else {
        settings.insert("env".into(), json!({ "ANTHROPIC_BASE_URL": url }));
    }
    settings.insert("apiKeyHelper".into(), json!(key_helper(&key_file)));
    // What Ghostex gives every Claude profile with or without hooks: its statusLine (the model,
    // effort and limits the chat shows) while Read live status from Claude Code and Cursor is on
    // (agent_hooks/statusline_setting.rs), and transcripts kept for good.
    let mut settings = Value::Object(settings);
    match statusline {
        Some(script) => {
            crate::agent_hooks::statusline::register_statusline(&mut settings, script);
        }
        None => {
            crate::agent_hooks::statusline::unregister_statusline(&mut settings);
        }
    }
    crate::agent_hooks::claude_retention::ensure_claude_transcript_retention(&mut settings);
    write_private(
        &settings_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&settings).unwrap_or_default()
        )
        .as_bytes(),
    )?;
    let state_path = dir.join(".claude.json");
    let mut state = read_object(&state_path);
    if state.get("hasCompletedOnboarding") != Some(&Value::Bool(true)) {
        state.insert("hasCompletedOnboarding".into(), json!(true));
        write_private(
            &state_path,
            serde_json::to_string_pretty(&Value::Object(state))
                .unwrap_or_default()
                .as_bytes(),
        )?;
    }
    Ok(())
}

/// Codex's folder: `config.toml` with the hub as its model provider (other lines, such as the
/// folder trust and the hooks Ghostex adds, are kept).
fn prepare_codex(dir: &Path, url: &str, key: &str) -> std::io::Result<()> {
    private_dir(dir)?;
    let path = dir.join("config.toml");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut document = text.parse::<toml_edit::DocumentMut>().unwrap_or_default();
    document["model_provider"] = toml_edit::value(CODEX_PROVIDER);
    let providers = document
        .entry("model_providers")
        .or_insert(toml_edit::table());
    if let Some(providers) = providers.as_table_mut() {
        providers.set_implicit(true);
        let mut provider = toml_edit::Table::new();
        provider["name"] = toml_edit::value("CLIProxyAPI hub");
        provider["base_url"] = toml_edit::value(format!("{url}/v1"));
        provider["wire_api"] = toml_edit::value("responses");
        provider["experimental_bearer_token"] = toml_edit::value(key);
        provider["requires_openai_auth"] = toml_edit::value(false);
        providers.insert(CODEX_PROVIDER, toml_edit::Item::Table(provider));
    }
    write_private(&path, document.to_string().as_bytes())
}

/// Makes the files Ghostex writes in a hub profile readable by this user only.
fn tighten(dir: &Path) {
    #[cfg(unix)]
    for name in ["settings.json", ".claude.json", "config.toml", KEY_FILE] {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        if path.is_file() {
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Takes the key out of a hub's profile folders when the hub is removed. The folders stay: they
/// hold the conversations of the sessions that ran there.
pub(crate) fn forget_key(home: &Path, hub_id: &str) {
    let _ = fs::remove_file(profile_dir(home, Provider::Claude, hub_id).join(KEY_FILE));
    let path = profile_dir(home, Provider::Codex, hub_id).join("config.toml");
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(mut document) = text.parse::<toml_edit::DocumentMut>() {
            if let Some(provider) = document
                .get_mut("model_providers")
                .and_then(|providers| providers.get_mut(CODEX_PROVIDER))
                .and_then(toml_edit::Item::as_table_mut)
            {
                provider.remove("experimental_bearer_token");
                let _ = write_private(&path, document.to_string().as_bytes());
            }
        }
    }
}

/// `base` started in `dir`: the profile variable before the command, as a custom agent's command
/// would set it.
fn in_profile(provider: Provider, dir: &Path, base: &str) -> String {
    let variable = match provider {
        Provider::Claude => "CLAUDE_CONFIG_DIR",
        Provider::Codex => "CODEX_HOME",
    };
    let dir = dir.to_string_lossy();
    if cfg!(windows) {
        format!("$env:{variable} = '{}'; {base}", dir.replace('\'', "''"))
    } else {
        format!("{variable}={} {base}", quote_shell_arg(&dir))
    }
}

/// Prepares a new session to start on hub `hub_id`: its profile folder is written with the hub's
/// address and client key, and the command it starts with names that folder. Returns the command;
/// `runtime` loses the picked account and gains [`RUNTIME_KEY`].
pub(crate) fn apply(
    paths: &GxserverPaths,
    provider: Provider,
    hub_id: &str,
    configured_command: Option<&str>,
    runtime: &mut Map<String, Value>,
) -> Result<String, DomainStateError> {
    if !super::enabled(paths) {
        return Err(DomainStateError::bad_request(
            ghostex_settings_catalog::built_in_extensions::turned_off_message(
                ghostex_settings_catalog::built_in_extensions::CLI_PROXY_API,
            ),
        ));
    }
    let hub = super::store::find(paths, hub_id).ok_or_else(|| {
        DomainStateError::bad_request(
            "That hub was removed. Pick another place to start the session.",
        )
    })?;
    let base = configured_command
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .unwrap_or(provider.id());
    if crate::accounts::launch::uses_own_login(base, provider) {
        return Err(DomainStateError::bad_request(
            "This agent's command chooses its own login, so it can't start on the hub. Start it from the built-in agent instead.",
        ));
    }
    let key =
        super::store::open_key(paths, &hub.client_key).map_err(DomainStateError::bad_request)?;
    // The home gxserver gives agents (hooks, skills and folder trust use it too).
    let home = paths.agent_config_home_dir().to_path_buf();
    let dir = profile_dir(&home, provider, &hub.id);
    match provider {
        Provider::Claude => prepare_claude(
            &dir,
            &hub.url,
            &key,
            crate::agent_hooks::statusline_setting::read_agent_live_status(paths)
                .then(|| {
                    crate::agent_hooks::config::HookPaths::from_paths(paths).statusline_hook_path
                })
                .as_deref(),
        ),
        Provider::Codex => prepare_codex(&dir, &hub.url, &key),
    }
    .map_err(io)?;
    // Ghostex's hooks in the new folder only while Use agent hooks is on: Claude and Codex are
    // followed without them otherwise (CDXC:AgentHooks 2026-11-12 in agent_hooks/hook_free.rs).
    // Best effort: the session still runs without them, and gxserver's start repairs them.
    if !crate::agent_hooks::hook_free::hook_free(paths, provider.id()) {
        let _ = crate::agent_hooks::install_agent_hooks_for_running_agent(paths, provider.id());
    }
    // The hook installer rewrites the folder's settings with the usual permissions; the folder is
    // this user's only (0700), and its files go back to 0600 too.
    tighten(&dir);
    for key in [
        "accountId",
        "accountProvider",
        "accountName",
        "accountSlot",
        "accountCommand",
    ] {
        runtime.remove(key);
    }
    runtime.insert(RUNTIME_KEY.into(), json!(hub.id));
    let command = in_profile(provider, &dir, base);
    runtime.insert("accountBaseCommand".into(), json!(command));
    Ok(command)
}

/// The hub `id` names (`cliproxy:<hub id>`) while the extension is on and the hub is still saved.
fn saved_hub(paths: &GxserverPaths, id: &str) -> Option<String> {
    let hub_id = id.strip_prefix(ACCOUNT_PREFIX)?;
    (super::enabled(paths) && super::store::find(paths, hub_id).is_some())
        .then(|| hub_id.to_string())
}

/// The hub `provider`'s New sessions start on is pinned to, while it can be used. A pin to a hub
/// that was removed, or while the extension is off, reads as no hub (the rule falls back to Auto).
///
/// CDXC:AgentProviders 2026-11-19 DECISION:
/// User: "make my macbook and windows machine call claude and codex thro that hub which should balance my different accounts". A hub is a choice in New sessions start on, saved as a pinned account `cliproxy:<hub id>`, so every launch that names no account (quick launch, the launchers, `start-thread`, `agents create`, the phone) starts On the hub, and the hub balances its own accounts. A picked account and a workspace's Claude account still win, as they do over every rule.
/// SEE-ALSO: default_account.rs (`pick`, `quick_launch_accounts`), agents/launch_plan.rs, apps/desktop/src/app/window/settings_modal/tabs/accounts/rules.rs.
pub(crate) fn default_hub(registry: &Registry, provider: Provider) -> Option<String> {
    match registry.new_session_accounts.get(&provider)? {
        NewSessionAccount::Pinned { id } if id.starts_with(ACCOUNT_PREFIX) => {
            saved_hub(&crate::paths::get_gxserver_paths(None), id)
        }
        _ => None,
    }
}

/// Whether `id` is a saved hub (`cliproxy:<hub id>`) that New sessions start on can be pinned to.
pub(crate) fn pinnable(paths: &GxserverPaths, id: &str) -> bool {
    saved_hub(paths, id).is_some()
}

/// The hub `wanted` names (`ghostex coordinator start-thread --account`): its id, `cliproxy:<id>`,
/// its name or host (any case), or `hub` while exactly one is saved. `None` when the extension is
/// off or nothing matches, so the name is read as a Claude Swap or Codex Swap account.
pub(crate) fn hub_named(paths: &GxserverPaths, wanted: &str) -> Option<String> {
    if !super::enabled(paths) {
        return None;
    }
    let hubs = super::store::read(paths).ok()?;
    let wanted = wanted.trim();
    let id = wanted.strip_prefix(ACCOUNT_PREFIX).unwrap_or(wanted);
    if let Some(hub) = hubs.iter().find(|hub| {
        hub.id == id
            || (!hub.label.trim().is_empty() && hub.label.trim().eq_ignore_ascii_case(wanted))
            || super::host(&hub.url).eq_ignore_ascii_case(wanted)
    }) {
        return Some(hub.id.clone());
    }
    match hubs.as_slice() {
        [only] if wanted.eq_ignore_ascii_case("hub") => Some(only.id.clone()),
        _ => None,
    }
}

/// Whether hub `hub_id` answers with its client key now, before a session starts on it: a hub that
/// is down or refuses the key would only fail the session's first message.
pub(crate) fn check_reachable(paths: &GxserverPaths, hub_id: &str) -> Result<(), DomainStateError> {
    let Some(hub) = super::store::find(paths, hub_id) else {
        return Ok(());
    };
    let key =
        super::store::open_key(paths, &hub.client_key).map_err(DomainStateError::bad_request)?;
    super::client::check_client_key(&hub.url, &key).map_err(|error| {
        DomainStateError::bad_request(format!(
            "The session didn't start on the hub. {error} Or start it on another account."
        ))
    })
}

/// The name a New sessions start on pin to a hub (`cliproxy:<hub id>`) shows, whether or not the
/// extension is on: the hub's name, else its host.
pub(crate) fn pinned_label(paths: &GxserverPaths, id: &str) -> Option<String> {
    let hub = super::store::find(paths, id.strip_prefix(ACCOUNT_PREFIX)?)?;
    Some(if hub.label.trim().is_empty() {
        super::host(&hub.url)
    } else {
        hub.label
    })
}

/// The address of hub `hub_id`, while it is saved.
pub(crate) fn hub_url(paths: &GxserverPaths, hub_id: &str) -> Option<String> {
    super::store::find(paths, hub_id).map(|hub| hub.url)
}

/// The hub id a launcher's picked account names (`cliproxy:<hub id>`), if it names one.
pub(crate) fn picked_hub(runtime: &Map<String, Value>) -> Option<String> {
    runtime
        .get("accountId")
        .and_then(Value::as_str)
        .and_then(|id| id.strip_prefix(ACCOUNT_PREFIX))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hub_command_uses_its_own_login() {
        let command = in_profile(
            Provider::Claude,
            Path::new("/Users/sam/.claude-profiles/ghostex-hub-1"),
            "claude --model opus",
        );
        assert!(crate::accounts::launch::uses_own_login(
            &command,
            Provider::Claude
        ));
        let command = in_profile(
            Provider::Codex,
            Path::new("/Users/sam/.codex-profiles/ghostex-hub-1"),
            "codex",
        );
        assert!(crate::accounts::launch::uses_own_login(
            &command,
            Provider::Codex
        ));
    }
}
