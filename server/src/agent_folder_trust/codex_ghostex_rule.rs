//! The Codex rule that lets Codex run Ghostex's own `ghostex` command outside its sandbox.
//!
//! CDXC:AgentSandbox 2026-11-18 WHY:
//! GitHub #655: in Codex's `workspace-write` sandbox (its default in a trusted folder) every
//! `ghostex` command that needs gxserver failed with "Could not connect to local gxserver at
//! http://127.0.0.1:<port>": the sandbox cuts all network access, the loopback port included, and
//! Codex reported the failure instead of asking to run it outside. So a Codex session could not
//! run `ghostex agents send`, `needs-you add` or `secret request`, which Ghostex's own instructions
//! tell it to. Measured with Codex 0.169.0 on macOS: turning the sandbox's network on, or Codex's
//! experimental network proxy with local binding allowed, also gets through, but both reach every
//! command Codex runs (all of the internet, or every local port), and a gxserver Unix socket would
//! need a socket gxserver does not have plus Codex's proxy feature. A Codex rule
//! `prefix_rule(pattern=["ghostex"], decision="allow")`, the same line Codex itself saves when the
//! user answers "always allow" for a command, runs only `ghostex` outside the sandbox and without
//! asking, while every other command stays sandboxed with its network off (checked: `ghostex agents
//! whoami` answered, `curl https://example.com` still could not resolve). Codex reads rules only
//! from the `rules` folder of its config folders, never from a `-c` launch flag, so the rule is a
//! file of its own in each Codex home, written before a Codex session starts, as the folder-trust
//! entries are. A rule of the user's own for `ghostex` with "prompt" or "forbidden" still wins,
//! since Codex keeps the strictest match.
//! SEE-ALSO: server/src/agent_hooks/ancestor_session.rs (a `ghostex` command finds its session when
//! Codex passes no GHOSTEX_* variable), server/src/agent_folder_trust/stores.rs (`codex_config_files`,
//! the same Codex homes).

use std::path::{Path, PathBuf};

use super::files::{env_folder_or, write_atomically};

const RULE_FILE_NAME: &str = "ghostex.rules";

const RULE_TEXT: &str = r#"# Written by Ghostex. Lets Codex run Ghostex's own `ghostex` command outside its sandbox,
# so it can reach the Ghostex app on this computer; every other command stays sandboxed.
# Ghostex writes it again whenever it starts Codex. To be asked first instead, add
# prefix_rule(pattern = ["ghostex"], decision = "prompt") to another file in this folder.
prefix_rule(
    pattern = ["ghostex"],
    decision = "allow",
    justification = "Ghostex's own command, which talks to the Ghostex app on this computer",
)
"#;

/// Every Codex home: `CODEX_HOME` (else `~/.codex`), each xswap account and each
/// `~/.codex-profiles` profile that has a `config.toml`.
fn codex_homes(home: &Path) -> Vec<PathBuf> {
    let mut homes = vec![env_folder_or(
        "CODEX_HOME",
        ghostex_paths::agent_homes::codex_default_home(home),
    )];
    for root in [
        crate::accounts::launch::xswap_root(home).join("accounts"),
        home.join(".codex-profiles"),
    ] {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        homes.extend(
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.join("config.toml").is_file()),
        );
    }
    let mut seen = Vec::new();
    homes.retain(|path| {
        if !path.is_dir() {
            return false;
        }
        let identity = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if seen.contains(&identity) {
            return false;
        }
        seen.push(identity);
        true
    });
    homes
}

/// Writes the rule into `codex_home`'s `rules` folder unless it is there already.
fn write_rule(codex_home: &Path) -> std::io::Result<()> {
    let path = codex_home.join("rules").join(RULE_FILE_NAME);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(RULE_TEXT) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_atomically(&path, RULE_TEXT)
}

/// Run right before a Codex session's command reaches its terminal. A box session's Codex keeps
/// its config inside the box, so it is left alone.
pub(crate) fn allow_ghostex_in_codex_before_launch(session: &serde_json::Value) {
    if cfg!(test) || session.pointer("/runtimeSettings/agentbox").is_some() {
        return;
    }
    if crate::session_chat_composer::session_chat_composer_agent_id(session).as_deref()
        != Some("codex")
    {
        return;
    }
    for codex_home in codex_homes(&crate::resume_lookup::home_dir()) {
        if let Err(error) = write_rule(&codex_home) {
            eprintln!(
                "Could not write the Ghostex rule for Codex in {}: {error}",
                codex_home.display()
            );
        }
    }
}
