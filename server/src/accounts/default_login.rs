//! CDXC:AgentProviders 2026-10-22 WHY:
//! `cswap run <N>` (claude-swap 0.27.2) starts plain `claude` on the default login, skipping the slot's session profile, whenever `~/.claude.json` names slot N's account. It never checks that the default login still holds a credential. On 2026-10-22 `~/.claude.json` named yahia@shortpoint.com while the default login was signed out, so every session on account 2 (S90:P5vkl:G2zg8 among them) started with "Not logged in · Please run /login". Reconnecting, reloading and re-seeding the slot's profile changed nothing, because cswap never used that profile. Claude Swap's own escape from that shortcut is a `CLAUDE_CONFIG_DIR` already set when it starts: it then validates the slot's profile, re-seeds it from its stored login when needed and runs Claude there. So a start whose slot names a signed-out default login gets that variable. A default login that still holds a credential keeps cswap's shortcut, which exists so the default login and a session profile never hold two copies of one token family.
//! CDXC:AgentProviders 2026-10-26 WHY:
//! A signed-out default login does not always lose its Keychain item: on 2026-10-26 Claude rewrote it at 12:24:38Z, the second account 4's sessions on the default login failed with "Login expired", and `claude auth status` has said `loggedIn: false` since, while `~/.claude.json` still names account 4 (MS). The missing-item check alone kept cswap's shortcut, so every start on MS, a Claude cloud session among them ("Cloud sessions need a claude.ai sign-in"), ran on the dead default login. When the item is there, Claude's own `claude auth status` decides; anything it cannot answer keeps cswap's choice.
//! SEE-ALSO: server/src/zmx/provider.rs (session starts and wakes), server/src/accounts/restart.rs (in-place account switch), server/src/agents/drafts.rs (draft agent switch), server/src/accounts/launch.rs (one-shot jobs, and `claude_account_argv` for Claude cloud sessions).
//! CDXC:AgentProviders 2026-11-07 WHY:
//! claude-swap 0.30.0 (realiti4/claude-swap#516, the fix for #515) reads the default login's identity from `~/.claude.json` even with `CLAUDE_CONFIG_DIR` already set, takes its shortcut onto that login and drops the variable, so the route below no longer gets past it: on 1.0.0 a signed-out default login that `~/.claude.json` still names put every new session on that account back at "Login expired". Ghostex's build of Claude Swap (claude_swap_build.rs) carries realiti4/claude-swap#563, where cswap itself skips the shortcut when the default login holds no usable token. The route stays for the cases only Ghostex can tell (Claude reports the login signed out while its tokens are still there, or the slot's saved copy is newer) and for a cswap older than 0.30.0.
use super::claude_resets::ClaudeSlot;
use std::{
    collections::HashMap,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

/// `text` with `CLAUDE_CONFIG_DIR=<slot profile>` in front of each `cswap run <slot>` command for a slot whose account is the signed-out default login, so cswap starts that slot's own profile instead. Commands are found wherever the shell runs one: at a line start, after `;`, `&&`, `||`, `|`, `(`, `$(`, `{`, `then`, `else` or `do` (a wake runs `"$(cswap run 2 -- agents --json)"; if …; then cswap run 2 -- attach …; else cswap run 2 -- --resume …; fi`, accounts/claude_background.rs). The quoted copy a restore banner prints is never in such a position.
pub(crate) fn route_around_signed_out_default(text: &str) -> String {
    if !text.contains("cswap") {
        return text.to_string();
    }
    if cfg!(windows) {
        return route_around_signed_out_default_powershell(text);
    }
    let Ok(home) = super::launch::home() else {
        return text.to_string();
    };
    let mut profiles: HashMap<String, Option<String>> = HashMap::new();
    let mut inserts = Vec::new();
    for (found, _) in text.match_indices("cswap") {
        let start = text[..found]
            .rfind(|ch: char| ch.is_whitespace() || "(;|&{`".contains(ch))
            .map_or(0, |at| {
                at + text[at..].chars().next().map_or(1, char::len_utf8)
            });
        if !in_command_position(&text[..start]) {
            continue;
        }
        let Some(slot) = cswap_run_slot(text, start) else {
            continue;
        };
        if let Some(profile) = profiles
            .entry(slot.clone())
            .or_insert_with(|| signed_out_default_profile(&home, &slot))
        {
            inserts.push((
                start,
                format!(
                    "CLAUDE_CONFIG_DIR={} ",
                    crate::agents::quote_shell_arg(profile)
                ),
            ));
        }
    }
    inserts.dedup_by(|a, b| a.0 == b.0);
    let mut routed = text.to_string();
    for (at, prefix) in inserts.into_iter().rev() {
        routed.insert_str(at, &prefix);
    }
    routed
}

/// CDXC:AgentProviders 2026-11-04 WHY:
/// The route around a signed-out default login used to stop at Windows, where every Claude account start is the PowerShell line `& '<…>\cswap.exe' run '<N>' --share-history -- …` and a `NAME=value cmd` prefix does not parse. On 2026-11-02 the default login's `.credentials.json` went dead (no refresh token, `claude auth status` reads `loggedIn: false`) while `~/.claude.json` still named account 2, so every thread started on account 2, Ghostex's default for new sessions, ran on that dead login and stopped at "Login expired · Please run /login" on its first turn (G5tnt, G2ds5, G0bum, G155q, G6ozy, G7vmp, G1bu8 on 2026-11-03 and 2026-11-04), while `ghostex accounts` rightly said account 2 was ready: its own saved login was fine. On Windows the start now runs inside a block that sets `CLAUDE_CONFIG_DIR` to the slot's profile and removes it again in `finally`, because a PowerShell `$env:` assignment outlives the command: left behind, it would make a later `cswap run` of the active default account in the same pane start a second copy of that login, which cswap's shortcut exists to prevent. PowerShell already reports `$?` true after any `& { … }` block, so the restore script's fallback check sees the same thing it did before.
fn route_around_signed_out_default_powershell(text: &str) -> String {
    let Ok(home) = super::launch::home() else {
        return text.to_string();
    };
    let mut profiles: HashMap<String, Option<String>> = HashMap::new();
    let mut wraps = Vec::new();
    for (found, _) in text.match_indices("cswap.exe'") {
        let Some(start) = text[..found].rfind("& '") else {
            continue;
        };
        // The copy a restore banner prints sits inside a quoted string, where every quote is doubled.
        if text[start + 3..found].contains('\'') || !powershell_command_position(&text[..start]) {
            continue;
        }
        let after = found + "cswap.exe'".len();
        let Some(slot) = powershell_run_slot(&text[after..]) else {
            continue;
        };
        if let Some(profile) = profiles
            .entry(slot.clone())
            .or_insert_with(|| signed_out_default_profile(&home, &slot))
        {
            wraps.push((start, powershell_command_end(text, after), profile.clone()));
        }
    }
    let mut routed = text.to_string();
    for (start, end, profile) in wraps.into_iter().rev() {
        let wrapped = format!(
            "& {{ $env:CLAUDE_CONFIG_DIR = {}; try {{ {} }} finally {{ Remove-Item Env:CLAUDE_CONFIG_DIR -ErrorAction Ignore }} }}",
            crate::agents::quote_shell_arg(&profile),
            &text[start..end]
        );
        routed.replace_range(start..end, &wrapped);
    }
    routed
}

/// Whether PowerShell expects a command right after `before`.
fn powershell_command_position(before: &str) -> bool {
    let before = before.trim_end();
    before.is_empty() || before.ends_with([';', '{', '(', '|', '\n'])
}

/// The slot of `run '<N>'` (or `run <N>`) at the start of `rest`.
fn powershell_run_slot(rest: &str) -> Option<String> {
    let rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
    let rest = rest.strip_prefix("run")?;
    let rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
    let slot: String = rest
        .trim_start_matches('\'')
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!slot.is_empty()).then_some(slot)
}

/// Where the PowerShell command whose arguments start at `from` ends: the first `;`, `|`, line
/// break or unmatched `}` outside quotes, with trailing spaces left outside.
fn powershell_command_end(text: &str, from: usize) -> usize {
    let (mut single, mut double, mut escaped, mut depth) = (false, false, false, 0usize);
    let mut end = text.len();
    for (offset, ch) in text[from..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '`' if !single => escaped = true,
            _ if single || double => {}
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            ';' | '|' | '\n' | '\r' | '}' => {
                end = from + offset;
                break;
            }
            _ => {}
        }
    }
    from + text[from..end].trim_end().len()
}

/// Whether a word that follows `before` is where the shell expects a command name.
fn in_command_position(before: &str) -> bool {
    let before = before.trim_end_matches([' ', '\t']);
    if before.is_empty() || before.ends_with(['\n', ';', '|', '&', '(', '{', '`']) {
        return true;
    }
    let word_start = before
        .rfind(|ch: char| ch.is_whitespace() || ";|&({`".contains(ch))
        .map_or(0, |at| at + 1);
    let word = &before[word_start..];
    // A custom agent's own variables (`FOO=1 claude …`) stay in front of the cswap wrapper (launch.rs `with_account_command`).
    let assignment = word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.bytes().all(|b| b == b'_' || b.is_ascii_alphanumeric())
    });
    (assignment || matches!(word, "then" | "else" | "do"))
        && in_command_position(&before[..word_start])
}

/// The slot of a command that starts at `from` and runs cswap as `cswap run <slot>`.
fn cswap_run_slot(text: &str, from: usize) -> Option<String> {
    let (_, end, word) = crate::agents::command_word(text, from)?;
    if Path::new(&word).file_name()?.to_str()? != "cswap" {
        return None;
    }
    let (_, end, run) = crate::agents::command_word(text, end)?;
    if run != "run" {
        return None;
    }
    let (_, _, slot) = crate::agents::command_word(text, end)?;
    (!slot.is_empty() && slot.bytes().all(|byte| byte.is_ascii_digit())).then_some(slot)
}

/// The `CLAUDE_CONFIG_DIR` an argv-launched `cswap run <slot>` needs (a Claude cloud session's
/// runner), for the same reason `route_around_signed_out_default` adds it to a shell command.
pub(crate) fn signed_out_default_config_dir(slot: &str) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    signed_out_default_profile(&super::launch::home().ok()?, slot)
}

/// The slot's session profile when `cswap run <slot>` would start Claude on a default login that names the slot's account but holds no credential, or one cswap reads signed out while the slot's saved copy is newer (restore.rs). Every start, wake, resume and account switch on a slot passes here first, so a slot that reads Needs sign-in gets its login restored from this computer's newer copy before cswap runs (`restore::before_start`).
fn signed_out_default_profile(home: &Path, slot: &str) -> Option<String> {
    super::restore::before_start(home, slot);
    let slot = ClaudeSlot::read(home, slot).ok()?;
    (slot.names_default_login(home)
        && (default_login_signed_out(home)
            || super::restore::default_login_behind_saved_copy(home, &slot)))
    .then(|| slot.session_dir())
}

/// Whether the default login certainly holds no working credential: no credential material at all, read the way Claude Swap reads a profile's (no non-empty `~/.claude/.credentials.json` and, on macOS, a Keychain miss), or material Claude itself reports signed out. A Keychain or a Claude that cannot answer counts as signed in, which leaves cswap's own choice alone.
fn default_login_signed_out(home: &Path) -> bool {
    let file = std::fs::read_to_string(
        ghostex_paths::agent_homes::claude_default_home(home).join(".credentials.json"),
    )
    .is_ok_and(|text| !text.trim().is_empty());
    #[cfg(target_os = "macos")]
    if !file && keychain_item_missing("Claude Code-credentials") {
        return true;
    }
    #[cfg(not(target_os = "macos"))]
    if !file {
        return true;
    }
    claude_reports_signed_out(home)
}

/// How long one `claude auth status` answer stands, so the slots of one start and a burst of wakes ask once.
const AUTH_STATUS_TTL: Duration = Duration::from_secs(30);

static AUTH_STATUS: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// `claude auth status --json` on the default login says it is not logged in. It reads the stored
/// login without refreshing or rewriting it.
fn claude_reports_signed_out(home: &Path) -> bool {
    if let Some((at, signed_out)) = *AUTH_STATUS.lock().unwrap_or_else(|e| e.into_inner()) {
        if at.elapsed() < AUTH_STATUS_TTL {
            return signed_out;
        }
    }
    let Some(claude) = super::helpers::probe_executable(home, "claude") else {
        return false;
    };
    let mut command = crate::platform::process::background_command(claude);
    // The default login as cswap's shortcut starts it: the same environment, no profile directory.
    command
        .args(["auth", "status", "--json"])
        .current_dir(home)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR");
    let signed_out = output_within(command, Duration::from_secs(10))
        .is_some_and(|out| auth_status_signed_out(&out));
    *AUTH_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), signed_out));
    signed_out
}

/// What `command` printed, if it finished within `limit`.
fn output_within(mut command: std::process::Command, limit: Duration) -> Option<String> {
    use std::{io::Read, process::Stdio};
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < limit => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Whether `claude auth status --json` output says the login is signed out (`"loggedIn": false`).
/// Output it cannot read says nothing.
fn auth_status_signed_out(stdout: &str) -> bool {
    stdout
        .find('{')
        .and_then(|start| serde_json::from_str::<serde_json::Value>(stdout[start..].trim()).ok())
        .is_some_and(|status| status["loggedIn"] == false)
}

/// `security find-generic-password` without `-w` reads only the item's attributes; exit status 44 is "no such item".
#[cfg(target_os = "macos")]
fn keychain_item_missing(service: &str) -> bool {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let Some(user) = std::env::var("USER").ok().filter(|user| !user.is_empty()) else {
        return false;
    };
    let Ok(mut child) = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-a", &user, "-s", service])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code() == Some(44),
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_auth_status() {
        // What `claude auth status --json` (Claude Code 2.1.309) printed for this Mac's default
        // login on 2026-10-26, its Keychain item still present.
        let signed_out = r#"{
  "loggedIn": false,
  "authMethod": "none",
  "apiProvider": "firstParty",
  "analyticsDisabled": false,
  "projectsDirectory": "/Users/madda/.claude/projects",
  "configDirectory": "/Users/madda/.claude"
}"#;
        assert!(auth_status_signed_out(signed_out));
        assert!(!auth_status_signed_out(
            r#"{"loggedIn": true, "authMethod": "claude.ai"}"#
        ));
        // A warning line before the JSON is skipped; output without an answer says nothing.
        assert!(auth_status_signed_out(
            "Update available\n{\"loggedIn\":false}"
        ));
        assert!(!auth_status_signed_out("error: unknown command 'auth'"));
        assert!(!auth_status_signed_out(r#"{"authMethod": "none"}"#));
        assert!(!auth_status_signed_out(""));
    }
}
