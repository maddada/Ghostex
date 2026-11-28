//! Claude in Chrome (`--chrome`) support for Native and CLI Claude sessions.
//!
//! CDXC:AgentProviders 2026-11-28 WHY:
//! GitHub #790: Claude Code 2.1.x ignores `claudeInChromeDefaultEnabled` in its global config
//! whenever it starts in non-interactive (`--output-format stream-json`) mode, so Native Claude
//! sessions never loaded the Chrome browser tools that Terminal-based sessions got by default.
//! Passing `--chrome` explicitly overrides that non-interactive guard in Claude Code; Ghostex
//! reads only the session's own `.claude.json` (default home, `CLAUDE_CONFIG_DIR`, cswap slot,
//! or CLIProxyAPI hub profile) and checks `claude --help` first so older Claude CLIs without
//! `--chrome` skip it instead of failing startup.
//! A chat's own choice (`CLAUDE_CHROME_KEY`, from `--chrome` or the Mode menu's "Claude in Chrome"
//! switch, server/src/session_chat_chrome_switch.rs) wins over the account's default: on passes
//! `--chrome`, off passes `--no-chrome` so a Terminal-mode Claude does not load it by default.

use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Map, Value};

use crate::accounts::claude_resets::ClaudeSlot;
use crate::domain::DomainStateError;

use super::spec::AcpLaunchProfile;

/// The create param `ghostex agents create --chrome` and `ghostex coordinator start-thread --chrome` send.
pub const CLAUDE_CHROME_PARAM: &str = "chrome";
/// The runtime setting that holds this chat's own Chrome choice across resumes, forks and
/// Native/Terminal switches: `true` (created with `--chrome`, or the Mode menu's "Claude in
/// Chrome" switch turned on), `false` (that switch turned off), absent (the account's default).
pub const CLAUDE_CHROME_KEY: &str = "claudeChrome";

const HELP_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long one reading of an account's Chrome default stands, so the queue frames gxserver
/// publishes for a chat don't each read its `.claude.json`.
const ACCOUNT_DEFAULT_TTL: Duration = Duration::from_secs(10);

/// What the installed `claude --help` lists.
#[derive(Clone, Copy, Default)]
struct ChromeFlags {
    chrome: bool,
    no_chrome: bool,
}

type HelpCacheKey = PathBuf;
type HelpCacheEntry = (Option<SystemTime>, u64, ChromeFlags);

static CLAUDE_CHROME_HELP_CACHE: OnceLock<Mutex<HashMap<HelpCacheKey, HelpCacheEntry>>> =
    OnceLock::new();

/// Whether a create request explicitly asked to connect Chrome.
pub fn claude_chrome_requested(params: &Map<String, Value>) -> bool {
    params.get(CLAUDE_CHROME_PARAM).and_then(Value::as_bool) == Some(true)
}

/// This chat's own Chrome choice: `Some(true)` on, `Some(false)` off, `None` the account's default.
pub fn claude_chrome_choice(runtime_settings: Option<&Map<String, Value>>) -> Option<bool> {
    runtime_settings?.get(CLAUDE_CHROME_KEY)?.as_bool()
}

/// Whether this chat starts with Chrome connected whatever its account's default.
pub fn is_claude_chrome_session(runtime_settings: Option<&Map<String, Value>>) -> bool {
    claude_chrome_choice(runtime_settings) == Some(true)
}

/// Whether this chat starts without Chrome whatever its account's default.
pub fn is_claude_chrome_off(runtime_settings: Option<&Map<String, Value>>) -> bool {
    claude_chrome_choice(runtime_settings) == Some(false)
}

/// Whether this Claude chat starts with Chrome: its own choice, else its account's "Enabled by
/// default" (the switch's starting state, server/src/session_chat_chrome_switch.rs). Reads only
/// that one key of the `.claude.json` the chat's account runs on.
pub(crate) fn chat_starts_with_chrome(runtime_settings: Option<&Map<String, Value>>) -> bool {
    if let Some(choice) = claude_chrome_choice(runtime_settings) {
        return choice;
    }
    let text = |key: &str| {
        runtime_settings
            .and_then(|runtime| runtime.get(key))
            .and_then(Value::as_str)
    };
    let hub_id = runtime_settings.and_then(crate::accounts::hub::launch::picked_hub);
    // The account fields a launch reads, as stored: what the memo is keyed on.
    let key = format!(
        "{}|{}|{}",
        text("accountCommand").unwrap_or(""),
        text("accountId").unwrap_or(""),
        hub_id.as_deref().unwrap_or("")
    );
    static MEMO: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> = OnceLock::new();
    let memo = MEMO.get_or_init(Default::default);
    if let Some((at, on)) = memo.lock().ok().and_then(|memo| memo.get(&key).copied()) {
        if at.elapsed() < ACCOUNT_DEFAULT_TTL {
            return on;
        }
    }
    let profile = AcpLaunchProfile {
        account_wrapper: super::spec::account_wrapper_from_ghostex_account(
            "claude",
            runtime_settings,
            text("accountCommand"),
        ),
        hub_id,
        ..AcpLaunchProfile::default()
    };
    let on = account_default_enabled(&profile, &hub_envs(&profile));
    if let Ok(mut memo) = memo.lock() {
        memo.insert(key, (Instant::now(), on));
    }
    on
}

/// The `CLAUDE_CONFIG_DIR` a CLIProxyAPI hub profile starts Claude with (`build_acp_server_argv`).
fn hub_envs(profile: &AcpLaunchProfile) -> Vec<(String, String)> {
    if profile.account_wrapper.is_some() {
        return Vec::new();
    }
    profile
        .hub_id
        .as_deref()
        .and_then(|hub| super::spec::resolve_hub_profile_dir("claude", hub))
        .map(|dir| {
            vec![(
                "CLAUDE_CONFIG_DIR".to_string(),
                dir.to_string_lossy().into_owned(),
            )]
        })
        .unwrap_or_default()
}

/// Whether Claude Code's own "Enabled by default" Claude in Chrome setting is on for the account
/// this launch runs on.
fn account_default_enabled(profile: &AcpLaunchProfile, acp_envs: &[(String, String)]) -> bool {
    !cfc_env_explicitly_disabled()
        && resolve_claude_config_json_path(profile, acp_envs)
            .is_some_and(|path| read_claude_in_chrome_default_enabled(&path))
}

/// Refuses `--chrome` on non-Claude agents or sandboxed boxes that cannot reach host Chrome.
pub fn check_claude_chrome_family(
    family: Option<&str>,
    in_box: bool,
) -> Result<(), DomainStateError> {
    if in_box {
        return Err(DomainStateError::bad_request(
            "A session that runs in a box cannot connect Chrome on this computer. Leave out --chrome or the box.",
        ));
    }
    if family == Some("claude") {
        return Ok(());
    }
    let name = family
        .and_then(crate::agents::default_agent_session_title_name)
        .unwrap_or("This agent");
    Err(DomainStateError::bad_request(format!(
        "{name} cannot connect Chrome. --chrome works only with Claude Code sessions."
    )))
}

/// Appends this chat's own Chrome choice to a Claude command (`--chrome` or `--no-chrome`, none
/// when it follows the account's default): to a Terminal-mode command only when the installed
/// `claude` CLI accepts the flag, to an `acp-host` command always (its Native driver checks the CLI
/// itself, `should_pass_chrome_flag`).
pub(crate) fn with_claude_chrome_flag(
    family: Option<&str>,
    runtime_settings: Option<&Map<String, Value>>,
    command: &str,
) -> String {
    let trimmed = command.trim_end();
    let Some(on) = claude_chrome_choice(runtime_settings) else {
        return command.to_string();
    };
    if family != Some("claude")
        || trimmed.is_empty()
        || trimmed
            .split_whitespace()
            .any(|token| token == "--chrome" || token == "--no-chrome")
    {
        return command.to_string();
    }
    if super::acp_family_from_launch_command(trimmed).is_none() {
        let flags = claude_cli_chrome_flags(None);
        if !(if on { flags.chrome } else { flags.no_chrome }) {
            return command.to_string();
        }
    }
    format!("{trimmed} {}", if on { "--chrome" } else { "--no-chrome" })
}

/// The flag a Native-to-Terminal switch adds for this chat's own Chrome choice, when the installed
/// `claude` accepts it (spec.rs `build_cli_switch_argv_with_options`).
pub(crate) fn cli_switch_flag(profile: &AcpLaunchProfile) -> Option<&'static str> {
    if profile.claude_chrome && claude_cli_supports_chrome(None) {
        return Some("--chrome");
    }
    (profile.claude_chrome_off && claude_cli_chrome_flags(None).no_chrome).then_some("--no-chrome")
}

/// Whether `driver_claude` should pass `--chrome` on this Native Claude launch or resume.
pub(crate) fn should_pass_chrome_flag(
    profile: &AcpLaunchProfile,
    acp_envs: &[(String, String)],
    prog: &str,
) -> bool {
    if profile.claude_chrome_off {
        return false;
    }
    let wants_chrome = profile.claude_chrome || account_default_enabled(profile, acp_envs);
    if !wants_chrome {
        return false;
    }
    let supported = claude_cli_supports_chrome(Some(prog));
    if !supported {
        eprintln!(
            "[Native] Skipping --chrome: installed Claude Code CLI does not advertise --chrome."
        );
    }
    supported
}

/// Whether `CLAUDE_CODE_ENABLE_CFC` in the host environment explicitly turns Chrome off.
fn cfc_env_explicitly_disabled() -> bool {
    let Ok(raw) = std::env::var("CLAUDE_CODE_ENABLE_CFC") else {
        return false;
    };
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

/// Resolves the `.claude.json` file that Claude Code itself will read for this session's
/// account/home (read-only).
fn resolve_claude_config_json_path(
    profile: &AcpLaunchProfile,
    acp_envs: &[(String, String)],
) -> Option<PathBuf> {
    if let Some((_, dir)) = acp_envs
        .iter()
        .find(|(key, value)| key == "CLAUDE_CONFIG_DIR" && !value.trim().is_empty())
    {
        return Some(PathBuf::from(dir).join(".claude.json"));
    }

    let env_config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty());
    let home = crate::accounts::launch::home().ok()?;

    // `cswap run <slot>` starts the slot's own profile, except for the slot whose account is the
    // default login, which cswap starts on the default login unless `CLAUDE_CONFIG_DIR` already
    // names the slot's profile (default_login.rs). A slot Ghostex cannot read gives no answer
    // rather than another account's file.
    if let Some(wrapper) = profile.account_wrapper.as_ref() {
        let slot = ClaudeSlot::read(&home, &wrapper.slot).ok()?;
        let session_dir = PathBuf::from(slot.session_dir());
        let on_default_login = slot.names_default_login(&home)
            && env_config_dir.as_deref() != Some(session_dir.as_path());
        if on_default_login {
            return Some(ghostex_paths::agent_homes::claude_json_path(&home));
        }
        return Some(session_dir.join(".claude.json"));
    }

    Some(match env_config_dir {
        Some(dir) => dir.join(".claude.json"),
        None => ghostex_paths::agent_homes::claude_json_path(&home),
    })
}

fn read_claude_in_chrome_default_enabled(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(data) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    data.get("claudeInChromeDefaultEnabled")
        .and_then(Value::as_bool)
        == Some(true)
}

/// Whether the installed `claude` CLI accepts `--chrome`.
pub(crate) fn claude_cli_supports_chrome(prog_hint: Option<&str>) -> bool {
    claude_cli_chrome_flags(prog_hint).chrome
}

/// The Chrome flags `claude --help` lists (cached per binary path, mtime, and size). None when
/// Claude is not found or does not answer.
fn claude_cli_chrome_flags(prog_hint: Option<&str>) -> ChromeFlags {
    let Some(executable) = resolve_claude_executable_for_probe(prog_hint) else {
        return ChromeFlags::default();
    };
    let meta = std::fs::metadata(&executable).ok();
    let stamp = meta
        .as_ref()
        .map(|m| (m.modified().ok(), m.len()))
        .unwrap_or((None, 0));

    if let Ok(cache) = CLAUDE_CHROME_HELP_CACHE
        .get_or_init(Default::default)
        .lock()
    {
        if let Some(&(cached_mtime, cached_len, flags)) = cache.get(&executable) {
            if stamp.0.is_some() && cached_mtime == stamp.0 && cached_len == stamp.1 {
                return flags;
            }
        }
    }

    let flags = probe_help_chrome_flags(&executable);
    if stamp.0.is_some() {
        if let Ok(mut cache) = CLAUDE_CHROME_HELP_CACHE
            .get_or_init(Default::default)
            .lock()
        {
            cache.insert(executable, (stamp.0, stamp.1, flags));
        }
    }
    flags
}

fn resolve_claude_executable_for_probe(prog_hint: Option<&str>) -> Option<PathBuf> {
    let home = crate::accounts::launch::home().ok();
    if let Some(hint) = prog_hint.map(str::trim).filter(|s| !s.is_empty()) {
        let stem = Path::new(hint)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(hint);
        if !matches!(stem, "cswap" | "cswap.exe") {
            let candidate = PathBuf::from(hint);
            if candidate.components().count() > 1 {
                if candidate.is_file() {
                    return Some(candidate);
                }
            } else if let Some(home) = home.as_deref() {
                if let Some(found) = crate::accounts::helpers::probe_executable(home, hint) {
                    return Some(found);
                }
            }
        }
    }
    home.as_deref()
        .and_then(|home| crate::accounts::helpers::probe_executable(home, "claude"))
}

fn probe_help_chrome_flags(executable: &Path) -> ChromeFlags {
    let mut cmd = crate::platform::process::background_command(executable);
    cmd.arg("--help")
        .env_remove("CLAUDECODE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return ChromeFlags::default();
    };
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout {
            let _ = out.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + HELP_PROBE_TIMEOUT;
    let ok = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader.join().unwrap_or_default();
    if !ok {
        return ChromeFlags::default();
    }
    let help = String::from_utf8_lossy(&bytes);
    let lists = |flag: &str| {
        help.split_whitespace().any(|token| {
            token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-') == flag
        })
    };
    ChromeFlags {
        chrome: lists("--chrome"),
        no_chrome: lists("--no-chrome"),
    }
}
