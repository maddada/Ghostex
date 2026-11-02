//! The agents gxserver lets an orchestrator run on: `ghostex orchestrator create` and promote
//! refuse the others, and every client offers New Orchestrator and Make Orchestrator only for these.

/// The agent families a new orchestrator can run on, in the order `ghostex orchestrator create`
/// prefers them when no agent is named.
///
/// CDXC:Coordinators 2026-10-20 WHY:
/// An orchestrator must read a skill, run the `ghostex` CLI and take Ghostex's chat sends (the user's messages and its threads' reports). These are the chat agents that load skills from a folder Ghostex can install into; OpenCode 2 joined once its chat landed (sends go through its server API, it reads `~/.agents/skills` itself). Left out: Empryo (no skill folder Ghostex knows of; its older orchestrators keep the `/agent` profile), Hermes (its replies and queued sends never reach the chat, agent flow tests 2026-10-17), Freebuff (no skill folder), and agents without a chat (Command Code, Gemini and the rest: no transcript reader and no input-box reading, so the first message and every report would be typed blind and the Threads panel has no chat to live in). Supersedes the 2026-10-17 WHY that left OpenCode out for having no chat.
/// SEE-ALSO: server/src/coordinators/role.rs (gxserver's checks), packages/gx-core/src/sidebar_menu/session.rs (Make Orchestrator), apps/desktop/src/app/gx_store/create/coordinator.rs (the New Orchestrator dialog), apps/mobile/app/src/sessions/sessionCommands.ts (the phone's copy, which cannot link this crate).
pub const COORDINATOR_AGENT_FAMILIES: [&str; 10] = [
    "claude",
    "codex",
    "zcode",
    "cursor",
    "grok",
    "copilot",
    "antigravity",
    "pi",
    "omp",
    "opencode",
];

/// True for the agent families a new orchestrator can run on.
pub fn coordinator_agent_family_supported(family: &str) -> bool {
    COORDINATOR_AGENT_FAMILIES.contains(&family)
}

/// True for the orchestrator families whose launch takes a chosen model, so a New Coordinator form
/// offers a Model field: every one but OpenCode, whose terminal app has no model flag.
///
/// CDXC:Coordinators 2026-11-02 WHY:
/// Issue #395: the forms kept their own list of model-taking agents (Claude, Codex, Pi, ZCode, Antigravity) after gxserver started taking a launch model from Cursor, Grok, Copilot and OMP on 2026-11-01, so those coordinators showed no Model or Effort field. The forms read this rule and gxserver's launch refusals stay the one that decides.
/// SEE-ALSO: server/src/agents/launch_plan.rs (`apply_requested_agent_model`, what a launch accepts), server/src/ghostex_cli/coordinator/command.rs (`ghostex coordinator options`, the phone's form), apps/desktop/src/app/gx_store/create/coordinator.rs (the desktop dialog).
pub fn coordinator_launch_takes_model(family: &str) -> bool {
    coordinator_agent_family_supported(family) && family != "opencode"
}

/// True for the orchestrator families whose launch takes a chosen effort, so a New Coordinator form
/// offers an Effort field: not Cursor (its effort lives in the model id), ZCode (no effort choice)
/// or OpenCode (neither).
pub fn coordinator_launch_takes_effort(family: &str) -> bool {
    coordinator_launch_takes_model(family) && !matches!(family, "cursor" | "zcode")
}

/// The name an orchestrator family is called by in messages.
pub fn coordinator_agent_family_name(family: &str) -> &str {
    match family {
        "claude" => "Claude",
        "codex" => "Codex",
        "zcode" => "ZCode",
        "cursor" => "Cursor",
        "grok" => "Grok",
        "copilot" => "Copilot",
        "antigravity" => "Antigravity",
        "pi" => "Pi",
        "omp" => "OMP",
        "opencode" => "OpenCode",
        other => other,
    }
}

/// "Claude, Codex, … or OpenCode": what gxserver answers when an orchestrator is asked to run on
/// another agent.
pub fn coordinator_agent_families_text() -> String {
    let names: Vec<&str> = COORDINATOR_AGENT_FAMILIES
        .iter()
        .map(|family| coordinator_agent_family_name(family))
        .collect();
    match names.split_last() {
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}
