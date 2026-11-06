//! The bots' Claude Code and Codex run on the accounts saved in Ghostex: the runtime's `claude`
//! and `codex` engines are pointed at two small wrappers that ask gxserver for the account of the
//! turn (accounts/launch.rs `bot_account_argv`) and run the CLI through Claude Swap or Codex Swap.
//!
//! CDXC:GBots 2026-11-07 WHY:
//! The runtime spawns its engine by one executable path (`PATCH /api/instances/<id> {cli}`), with no way to add arguments, so the wrapper is a file; it runs `ghostex gbots agent <cli>`, which picks the account each time the runtime starts the CLI. The wrapper names the CLI by its absolute path, so it never finds itself on PATH.

use std::path::PathBuf;

use serde_json::json;

/// The engines that run on saved accounts: the runtime's instance id and Ghostex's provider.
const ENGINES: [(&str, crate::accounts::model::Provider); 2] = [
    ("claude", crate::accounts::model::Provider::Claude),
    ("codex", crate::accounts::model::Provider::Codex),
];

fn wrapper_dir() -> PathBuf {
    super::sidecar::state_dir().join("bin")
}

/// Writes the wrappers (unix shell scripts) beside the sidecar's data. `None` when the `ghostex`
/// CLI shipped with this gxserver is missing.
pub(crate) fn write_wrappers(cli: &std::path::Path) -> Option<Vec<(&'static str, PathBuf)>> {
    let dir = wrapper_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let quoted = cli.to_string_lossy().replace('\'', "'\\''");
    let mut written = Vec::new();
    for (engine, _) in ENGINES {
        let path = dir.join(engine);
        let script = format!("#!/bin/sh\nexec '{quoted}' gbots agent {engine} -- \"$@\"\n");
        if std::fs::read_to_string(&path).ok().as_deref() != Some(script.as_str()) {
            std::fs::write(&path, script).ok()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
        written.push((engine, path));
    }
    Some(written)
}

/// Points the runtime's engines at the wrappers (idempotent; the runtime keeps it in its config).
pub(crate) fn point_engines_at_wrappers(wrappers: &[(&'static str, PathBuf)]) {
    for (engine, path) in wrappers {
        let _ = super::client::send(
            "PATCH",
            &format!("/api/instances/{engine}"),
            &json!({ "cli": path }),
        );
    }
}

/// `agentArgv {agent}`: what the wrapper runs in front of the CLI's own arguments; empty when no
/// saved account is usable (the CLI's own login).
pub(crate) fn agent_argv(
    state: &crate::server::AppState,
    agent: &str,
) -> Result<Vec<String>, String> {
    let (_, provider) = ENGINES
        .iter()
        .find(|(engine, _)| *engine == agent)
        .ok_or("agent must be claude or codex.")?;
    Ok(crate::accounts::launch::bot_account_argv(state, *provider).unwrap_or_default())
}
