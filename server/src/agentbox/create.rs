//! The box half of `/api/createAgentSession`: which agent runs, the box it runs in, and the launch.

use serde_json::{json, Map, Value};

use super::command::{box_agent_args, box_launch_command, box_launch_plan};
use super::first_prompt::LAUNCH_PROMPT_KEY;
use super::location::{new_box_name, AGENTBOX_AGENTS};
use super::session::SessionAgentbox;
use crate::domain::DomainStateError;

/// Runtime keys an account launch writes. A box session has its own sign-in inside the box, so a
/// create that names an account for it must not leave the account half-applied.
const ACCOUNT_RUNTIME_KEYS: &[&str] = &[
    "accountBaseCommand",
    "accountCommand",
    "accountId",
    "accountName",
    "accountProvider",
    "accountSlot",
];

/// The agent family a box can run, or the refusal a create gets for any other agent.
pub(crate) fn box_agent_family(family: Option<&str>) -> Result<String, DomainStateError> {
    family
        .filter(|family| AGENTBOX_AGENTS.contains(family))
        .map(str::to_string)
        .ok_or_else(|| {
            DomainStateError::bad_request("AgentBox can run Claude, Codex, OpenCode and Pi.")
        })
}

/// The longest first prompt a box launch carries itself.
const INLINE_PROMPT_MAX_BYTES: usize = 4 * 1024;

/// `agentbox claude login && <command>`: sign in for boxes first, in the session's own terminal.
pub(crate) fn with_claude_sign_in(command: &str) -> String {
    format!("agentbox claude login && {command}")
}

/// What a box create stores and launches.
pub(crate) struct BoxLaunch {
    pub(crate) launch_plan: Value,
}

/// Fills `runtimeSettings.agentbox` and returns the launch plan that runs the agent in a new box.
///
/// CDXC:AgentBox 2026-11-02 DECISION:
/// User (D3 "into cloud box menu", Q996 "gxserver on box", Q997 "advanced"): Instabox and Boat boxes are Cloud Boxes that Ghostex drives itself (server/src/cloud_boxes/, gxserver on the box); the `agentbox` CLI stays only for the older places (Docker, your SSH server, Hetzner, Vercel, Daytona, E2B, DigitalOcean), set up under Settings > Cloud Boxes > Advanced and picked under More providers in a new chat's Cloud Box picker (and on New Thread's Run on row). This supersedes the 2026-10-01 "let's try just use agentbox cli for now", which still describes how those places run: Ghostex runs the `agentbox` CLI in the session's terminal, not agentbox's hub REST API, account wrapping is skipped (the box has its own sign-in), and agentbox, not Ghostex, carries the agent's settings, skills and Codex sign-in into the box.
/// SEE-ALSO: packages/gx-chat-core/src/menus/cloud_box.rs (More providers), packages/gx-chat-core/src/menus/run_location.rs.
pub(crate) fn prepare_box_launch(
    provider: &str,
    family: &str,
    project_path: &str,
    configured_command: Option<&str>,
    claude_sign_in_first: bool,
    runtime_settings: &mut Map<String, Value>,
) -> BoxLaunch {
    for key in ACCOUNT_RUNTIME_KEYS {
        runtime_settings.remove(*key);
    }
    // CDXC:AgentBox 2026-10-01 WHY: agentbox hands the agent's arguments to tmux as one command line inside the box, and tmux rejects commands over about 16KB, so only a prompt well under that rides the launch. A longer one stays an ordinary startup send, which the queue types once the agent's input box is up.
    let first_prompt = runtime_settings
        .get("firstUserMessage")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty() && prompt.len() <= INLINE_PROMPT_MAX_BYTES)
        .map(str::to_string);
    let box_name = new_box_name(project_path);
    let agent_args = box_agent_args(configured_command, family);
    let launch_command = box_launch_command(
        provider,
        family,
        &box_name,
        &agent_args,
        first_prompt.as_deref(),
    );
    let agentbox = SessionAgentbox {
        provider: provider.to_string(),
        box_name,
        agent: family.to_string(),
        created: false,
        launch_command: Some(launch_command.clone()),
    };
    let mut command = launch_command;
    /*
    CDXC:AgentBox 2026-10-01 WHY:
    With `-y` agentbox skips its "Sign in with your Claude subscription?" question, so a Claude box created before Claude was ever signed in for boxes started at "Not logged in" and the user only found out after typing a prompt (observed live 2026-10-01). The first launch therefore signs in first, in the session's own terminal (agentbox prints the auth URL, opens the browser and asks for the code), and `&&` starts the box only after a successful sign-in. Restores rerun this step only while the box does not exist yet (restore.rs). Codex needs no step: Docker boxes reuse `~/.codex/auth.json` and cloud boxes receive it.
    */
    if family == "claude" && claude_sign_in_first {
        command = with_claude_sign_in(&command);
        super::status::invalidate_status_cache();
    }
    let mut record = agentbox.to_value();
    if first_prompt.is_some() {
        record[LAUNCH_PROMPT_KEY] = json!("pending");
    }
    runtime_settings.insert("agentbox".to_string(), record);
    let base_command = configured_command
        .map(str::to_string)
        .unwrap_or_else(|| family.to_string());
    BoxLaunch {
        launch_plan: box_launch_plan(&base_command, &command, first_prompt.as_deref()),
    }
}
