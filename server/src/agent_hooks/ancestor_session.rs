//! The Ghostex session a hook belongs to, read from the hook's process tree.

use std::env;

/// CDXC:AgentHooks 2026-10-06 WHY:
/// Empryo 3.9.0-beta runs every hook with a filtered environment (its `*_PROJECT_DIR` variables plus a fixed safe list), so no GHOSTEX_* routing variable reaches a hook in `~/.empryo/hooks.json`. Instead, an Empryo hook finds its session by walking up its process tree to the `zmx run S60-<project>-<session>` daemon that owns the pane, and reaches gxserver at the machine-wide local address and token file the `ghostex` CLI uses. Routing variables already in the environment win, so this retires on its own once Empryo passes host variables through (the upstream ask). Native Windows (wmx) has no lookup yet: an Empryo hook there reports nothing.
/// SEE-ALSO: server/src/agent_hooks/plugin_sources.rs (the notify script lets an `empryo` hook through without routing variables), server/src/accounts/codex_blockers.rs `zmx_owners` (the same daemon walk for live Codex processes), server/src/ghostex_cli/agents/identity.rs `caller` (a `ghostex` command run from Empryo's shell tool, which filters the same variables).
pub(crate) fn adopt_ancestor_session_routing() {
    if env::var_os("GHOSTEX_GLOBAL_SESSION_REF").is_some() {
        return;
    }
    let Some(zmx_name) = ancestor_zmx_session_name() else {
        return;
    };
    let mut parts = zmx_name.split('-');
    let (Some(server), Some(project), Some(session), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return;
    };
    if !(server.starts_with('S') && project.starts_with('P') && session.starts_with('G')) {
        return;
    }
    env::set_var(
        "GHOSTEX_GLOBAL_SESSION_REF",
        crate::ids::create_global_session_ref(server, project, session),
    );
    adopt_local_gxserver_address();
}

/// Fills in the machine-wide local gxserver address and token file the `ghostex` CLI uses, where
/// the hook's environment carries none.
pub(crate) fn adopt_local_gxserver_address() {
    if env::var_os("GHOSTEX_GXSERVER_BASE_URL").is_none() {
        env::set_var(
            "GHOSTEX_GXSERVER_BASE_URL",
            format!(
                "http://{}:{}",
                crate::constants::GXSERVER_LOCAL_API_HOST,
                crate::ghostex_cli::rpc::local_gxserver_api_port()
            ),
        );
    }
    if env::var_os("GHOSTEX_GXSERVER_AUTH_TOKEN_FILE").is_none() {
        env::set_var(
            "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
            crate::ghostex_cli::rpc::gxserver_auth_token_path(),
        );
    }
}

/// The session name of the closest `zmx run <name>` above this process, when that pane runs
/// Empryo itself: above the `empryo` process only shells may stand before the daemon. An Empryo
/// another agent started from its tool (Claude's Bash tool, say) belongs to that agent's pane, not
/// to an Empryo session, so its hooks report nothing.
#[cfg(unix)]
fn ancestor_zmx_session_name() -> Option<String> {
    const SHELLS: &[&str] = &[
        "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "nu", "env", "login",
    ];
    let table = super::nested_agent::ProcessTable::read();
    let mut current = std::process::id();
    let mut above_empryo = false;
    for _ in 0..64 {
        let parent = table.parent(current)?;
        let stem = table
            .executable_stem(parent)?
            .trim_start_matches('-')
            .to_ascii_lowercase();
        if stem == "zmx" {
            return above_empryo.then(|| zmx_run_session_name(parent)).flatten();
        }
        if stem == "empryo" || stem == "em" {
            above_empryo = true;
        } else if above_empryo && !SHELLS.contains(&stem.as_str()) {
            return None;
        }
        current = parent;
    }
    None
}

#[cfg(not(unix))]
fn ancestor_zmx_session_name() -> Option<String> {
    None
}

/// Points this hook at the Ghostex session the zmx pane `zmx_name` belongs to, replacing whatever
/// routing the environment carried. False when the name is not a Ghostex session name.
pub(crate) fn adopt_zmx_session_routing(zmx_name: &str) -> bool {
    let mut parts = zmx_name.split('-');
    let (Some(server), Some(project), Some(session), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if !(server.starts_with('S') && project.starts_with('P') && session.starts_with('G')) {
        return false;
    }
    env::set_var(
        "GHOSTEX_GLOBAL_SESSION_REF",
        crate::ids::create_global_session_ref(server, project, session),
    );
    env::set_var("GHOSTEX_SESSION_ID", zmx_name);
    // A state file in this environment belongs to the session that started Claude's daemon.
    env::remove_var("GHOSTEX_SESSION_STATE_FILE");
    true
}

/// The session name of the `zmx run <name>` pane whose own agent is `agent`: above it only shells
/// may stand before the daemon, the same rule `nested_agent` applies to a hook's own agent.
#[cfg(unix)]
pub(crate) fn zmx_session_owning_agent(agent: u32) -> Option<String> {
    const SHELLS: &[&str] = &[
        "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "nu", "env", "login",
    ];
    let table = super::nested_agent::ProcessTable::read();
    let mut current = agent;
    for _ in 0..64 {
        let parent = table.parent(current)?;
        let stem = table
            .executable_stem(parent)?
            .trim_start_matches('-')
            .to_ascii_lowercase();
        if stem == "zmx" {
            return zmx_run_session_name(parent);
        }
        if !SHELLS.contains(&stem.as_str()) {
            return None;
        }
        current = parent;
    }
    None
}

/// Native Windows: the wmx pane host above `agent` (wmx_pane.rs).
#[cfg(windows)]
pub(crate) fn zmx_session_owning_agent(agent: u32) -> Option<String> {
    super::wmx_pane::wmx_session_owning_agent(agent)
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn zmx_session_owning_agent(_agent: u32) -> Option<String> {
    None
}

/// The `<name>` of a `zmx run <name> …` command line; a `zmx attach` client yields nothing.
#[cfg(unix)]
fn zmx_run_session_name(pid: u32) -> Option<String> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    let command = String::from_utf8_lossy(&output.stdout);
    let mut words = command.split_whitespace();
    words.next()?;
    (words.next()? == "run").then(|| words.next().map(str::to_string))?
}
