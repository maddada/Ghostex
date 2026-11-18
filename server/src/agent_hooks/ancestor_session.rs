//! The Ghostex session a hook belongs to, read from the hook's process tree.

use std::env;

/// CDXC:AgentHooks 2026-10-06 WHY:
/// Empryo 3.9.0-beta runs every hook with a filtered environment (its `*_PROJECT_DIR` variables plus a fixed safe list), so no GHOSTEX_* routing variable reaches a hook in `~/.empryo/hooks.json`. Instead, an Empryo hook finds its session by walking up its process tree to the `zmx run S60-<project>-<session>` daemon that owns the pane, and reaches gxserver at the machine-wide local address and token file the `ghostex` CLI uses. Routing variables already in the environment win, so this retires on its own once Empryo passes host variables through (the upstream ask). Native Windows (wmx) has no lookup yet: an Empryo hook there reports nothing.
/// SEE-ALSO: server/src/agent_hooks/plugin_sources.rs (the notify script lets an `empryo` hook through without routing variables), server/src/accounts/codex_blockers.rs `zmx_owners` (the same daemon walk for live Codex processes), server/src/ghostex_cli/mod.rs `adopt_caller_session_from_process_tree` (every `ghostex` command run from Empryo's or Codex's shell tool, which filter the same variables).
pub(crate) fn adopt_ancestor_session_routing() {
    if env::var_os("GHOSTEX_GLOBAL_SESSION_REF").is_some() {
        return;
    }
    if let Some((zmx_name, _)) = ancestor_zmx_session() {
        adopt_ancestor_routing(&zmx_name);
    }
}

/// The `ghostex` CLI's lookup (ghostex_cli/mod.rs): the session's routing, as for a hook, and the
/// session's own `gxs_` token, read from the agent process that owns this process tree.
pub(crate) fn adopt_ancestor_session_for_cli() {
    if env::var_os("GHOSTEX_GLOBAL_SESSION_REF").is_some() {
        return;
    }
    let Some((zmx_name, agent)) = ancestor_zmx_session() else {
        return;
    };
    if adopt_ancestor_routing(&zmx_name) {
        adopt_agent_session_token(agent, &zmx_name);
    }
}

/*
CDXC:Secrets 2026-11-18 DECISION:
User (GitHub #669, 18 Nov): `ghostex secret request` must work from a Codex whose sandbox hides
Ghostex's variables (`shell_environment_policy.inherit = "core"`), so the CLI there reads its
session's own `gxs_` token from the pane's environment. The 8 Nov rule stands: only the asking
session makes and waits for its request (secret_requests/api.rs `require_own_session`). The token is
taken only from the `codex` or `empryo` process this command runs under, the agent of the pane the
walk above found, and only when that process's own `ZMX_SESSION` names that same pane; never from
the token file, another pane or any process outside this tree. WHY it adds no reach: Codex itself
was started with the token in its environment and only drops it from its tool's shells, and any
process of the same user could already read it there (`ps eww`); the lookup only ever finds the
token of the session whose agent ran the command.
*/
fn adopt_agent_session_token(agent: u32, zmx_name: &str) {
    if env::var_os(crate::security::SESSION_TOKEN_ENV).is_some() {
        return;
    }
    let Some(launch) = crate::accounts::process_launch(i64::from(agent)) else {
        return;
    };
    let value = |key: &str| {
        launch
            .environment
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.trim())
    };
    if value("ZMX_SESSION") != Some(zmx_name) {
        return;
    }
    if let Some(token) = value(crate::security::SESSION_TOKEN_ENV)
        .filter(|token| token.starts_with(crate::security::SESSION_TOKEN_PREFIX))
    {
        env::set_var(crate::security::SESSION_TOKEN_ENV, token);
    }
}

/// Points this process at the Ghostex session `zmx_name` names, with the local gxserver address.
/// False when the name is not a Ghostex session name.
fn adopt_ancestor_routing(zmx_name: &str) -> bool {
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
    adopt_local_gxserver_address();
    true
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

/// CDXC:AgentSandbox 2026-11-18 WHY:
/// GitHub #655: a Codex whose own config says `[shell_environment_policy] inherit = "core"` runs
/// its shell commands without any GHOSTEX_* variable, so `ghostex agents whoami` and `ghostex agents
/// send` answered "Cannot identify the caller". A `ghostex` command run from Codex's shell tool
/// finds its pane the way one from Empryo's does, from the `zmx run` daemon above the `codex`
/// process. Codex installed from npm starts its native `codex` from `node`, so a `node` right
/// above `codex` is let through too; anywhere else a `node` is another agent (an npm Claude, say)
/// and the walk gives up, as for any other program.
const ANCESTOR_AGENTS: &[&str] = &["empryo", "em", "codex"];

/// The session name of the closest `zmx run <name>` above this process, when that pane runs
/// Empryo or Codex itself: above the agent's process only shells may stand before the daemon. An
/// agent another agent started from its tool (Claude's Bash tool, say) belongs to that agent's
/// pane, not to a session of its own, so its hooks report nothing. With it, the pid of the agent
/// process closest to this one (the native `codex`, below any `node` that started it).
#[cfg(unix)]
fn ancestor_zmx_session() -> Option<(String, u32)> {
    const SHELLS: &[&str] = &[
        "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "nu", "env", "login",
    ];
    let table = super::nested_agent::ProcessTable::read();
    let mut current = std::process::id();
    let mut agent = None;
    let mut right_above_codex = false;
    for _ in 0..64 {
        let parent = table.parent(current)?;
        let stem = table
            .executable_stem(parent)?
            .trim_start_matches('-')
            .to_ascii_lowercase();
        if stem == "zmx" {
            return zmx_run_session_name(parent).zip(agent);
        }
        let launcher_of_codex = right_above_codex && stem == "node";
        right_above_codex = stem == "codex";
        if ANCESTOR_AGENTS.contains(&stem.as_str()) {
            agent = agent.or(Some(parent));
        } else if agent.is_some() && !launcher_of_codex && !SHELLS.contains(&stem.as_str()) {
            return None;
        }
        current = parent;
    }
    None
}

#[cfg(not(unix))]
fn ancestor_zmx_session() -> Option<(String, u32)> {
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
