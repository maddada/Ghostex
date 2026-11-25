//! The agents gxserver lets an orchestrator run on: `ghostex orchestrator create` and promote
//! refuse the others, and every client offers New Orchestrator and Make Orchestrator only for these.

/// The agent families a new orchestrator can run on, in the order `ghostex orchestrator create`
/// prefers them when no agent is named.
///
/// CDXC:Coordinators 2026-10-20 WHY:
/// An orchestrator must read a skill, run the `ghostex` CLI and take Ghostex's chat sends (the user's messages and its threads' reports). These are the chat agents that load skills from a folder Ghostex can install into; OpenCode 2 joined once its chat landed (sends go through its server API, it reads `~/.agents/skills` itself). Left out: Empryo (no skill folder Ghostex knows of; its older orchestrators keep the `/agent` profile), Hermes (its replies and queued sends never reach the chat, agent flow tests 2026-10-17), Freebuff (no skill folder), and agents without a chat (Command Code and the rest: no transcript reader and no input-box reading, so the first message and every report would be typed blind and the Threads panel has no chat to live in). Supersedes the 2026-10-17 WHY that left OpenCode out for having no chat.
///
/// CDXC:Coordinators 2026-11-07 WHY:
/// Issue #480: DeepSeek Harness (`dsh`) does all three, so it joined last in the order. It reads `~/.agents/skills` itself (dsh-skill-filesystem's `agentsHome/skills` root), so no per-agent skill link is needed, and chat sends reach it through dsh-tui's inject socket (server/src/session_chat_queue_runtime/dsh_channel_send.rs). Its launch takes no model or effort (gxserver refuses both for a dsh session), so its New Coordinator form shows neither and it starts on its configured model.
/// SEE-ALSO: server/src/coordinators/role.rs (gxserver's checks), packages/gx-core/src/sidebar_menu/session.rs (Make Orchestrator), apps/desktop/src/app/gx_store/create/coordinator.rs (the New Orchestrator dialog), apps/mobile/app/src/sessions/sessionCommands.ts (the phone's copy, which cannot link this crate).
pub const COORDINATOR_AGENT_FAMILIES: [&str; 11] = [
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
    "dsh",
];

/// True for the agent families a new orchestrator can run on: every agent Ghostex can name.
///
/// CDXC:Coordinators 2026-11-13 DECISION:
/// User (Q2980 "Build it: auto-send + remind on drift"): "Any harness, account and model is allowed: remove every limit on which model or harness a coordinator can use." So gxserver no longer refuses a coordinator, a promotion or Make Coordinator for an agent outside [`COORDINATOR_AGENT_FAMILIES`]; that list now only orders `ghostex coordinator create`'s default, names the agents whose skill folder Ghostex installs the playbook into (the others get the guide line, gx-chat-core `coordinator_skill_line`) and the ones whose launch takes a model. A low-end model gets a note in the model picker instead ([`coordinator_model_may_degrade`]). Supersedes the 2026-10-20 WHY above as a limit.
pub fn coordinator_agent_family_supported(family: &str) -> bool {
    !family.trim().is_empty()
}

/// True for the orchestrator families whose new coordinator takes a chosen model, so a New
/// Coordinator form offers a Model field: every one of them but DeepSeek Harness, whose launch takes
/// no model (issue #480).
///
/// CDXC:Coordinators 2026-11-02 WHY:
/// Issue #395: the forms kept their own list of model-taking agents (Claude, Codex, Pi, ZCode, Antigravity) after gxserver started taking a launch model from Cursor, Grok, Copilot and OMP on 2026-11-01, so those coordinators showed no Model or Effort field. The forms read this rule and gxserver's launch refusals stay the one that decides. OpenCode joined the same day (the user chose to build it): its terminal app has no model flag, so gxserver switches a new OpenCode coordinator to the chosen model and variant through OpenCode's own server before its first message, the way ZCode's model goes as a `/model` line.
/// SEE-ALSO: server/src/agents/launch_plan.rs (`apply_requested_agent_model`, what a launch accepts), server/src/server/coordinator_runtime.rs (`start_orchestrator_role`, the ZCode and OpenCode `/model` line), server/src/ghostex_cli/coordinator/command.rs (`ghostex coordinator options`, the phone's form), apps/desktop/src/app/gx_store/create/coordinator.rs (the desktop dialog).
pub fn coordinator_launch_takes_model(family: &str) -> bool {
    COORDINATOR_AGENT_FAMILIES.contains(&family) && family != "dsh"
}

/// True for the orchestrator families whose new coordinator takes a chosen effort, so a New
/// Coordinator form offers an Effort field: not Cursor (its effort lives in the model id) or ZCode
/// (no effort choice). OpenCode's effort is the chosen model's variant.
pub fn coordinator_launch_takes_effort(family: &str) -> bool {
    coordinator_launch_takes_model(family) && !matches!(family, "cursor" | "zcode")
}

/// Kept for API compatibility; returns `None` because coordinator model pickers always show
/// concrete model names rather than a generic "default" row.
pub fn coordinator_default_model_label(_family: &str) -> Option<&'static str> {
    None
}

/// A Model row's label in a New Coordinator form: OpenCode's lineup lists the same model under
/// several providers ("GPT-6.1 Terra" from OpenAI and from OpenRouter), so its rows name the provider.
pub fn coordinator_model_label(family: &str, value: &str, label: &str) -> String {
    match value.split_once('/') {
        Some((provider, _)) if family == "opencode" && !provider.is_empty() => {
            format!("{label} ({provider})")
        }
        _ => label.to_string(),
    }
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
        "dsh" => "DeepSeek Harness",
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

/// The words a coordinator's model picker, chat pill and agent picker step show when its model or
/// effort may give the user a degraded experience ([`coordinator_model_may_degrade`]).
///
/// CDXC:Coordinators 2026-11-18 DECISION:
/// GitHub #665 (an Antigravity coordinator on Orchestrator (Argon) was told it runs on a model Ghostex can't identify), user: "i wanna let people pick any model, no need to mention that model isn't best to the AI itself. let's just show a warning in the model picker if it's a low end model like deepseek flash haiku sonnet or opus medium/low. just say you might get degraded experience that's all. opus fable astra sol high and xhigh don't show this (any version of those)". So the coordinator's guide never mentions its model, and the user sees "You might get a degraded experience." only on a low-end model (Haiku, Sonnet, any Flash, Mini, Lite or Nano tier, DeepSeek Flash included), or on Opus, Fable, Astra or Sol (any version) at medium effort or below. Every other model (Argon, one Ghostex does not know) and an unknown effort says nothing. Supersedes the 2026-11-13 Q2980 decision that warned on every model but Opus, Fable, GPT Sol and GPT Astra and put a line in the guide.
/// SEE-ALSO: packages/gx-chat-core/src/menus/picker/projection.rs (the model picker's note), packages/gx-chat-core/src/extras/coordinator_quality.rs (the chat's pill), apps/desktop/src/app/gx_store/create/coordinator.rs (the agent picker's step), apps/mobile/app/src/sessions/coordinatorQuality.ts (the phone's copy of this rule).
pub const COORDINATOR_DEGRADED_TEXT: &str = "You might get a degraded experience.";

/// The words of a model value or label, lowercased: `claude-opus-5-5` reads `claude opus 5 5`.
fn model_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// True for a small model tier: Haiku, Sonnet, or any Flash, Mini, Lite or Nano (`deepseek-v4-flash`,
/// `gemini-3-flash-lite`, `gpt-6-mini`).
pub fn coordinator_model_is_low_end(model: &str) -> bool {
    model_words(model).iter().any(|word| {
        matches!(
            word.as_str(),
            "haiku" | "sonnet" | "flash" | "mini" | "lite" | "nano"
        )
    })
}

/// True for Opus, Fable, Astra or Sol, any version (`opus[1m]`, `claude-opus-5-5`, `Fable 5`,
/// `gpt-6.1-sol`, `GPT-6.1 Astra`), which warn only at medium effort or below.
pub fn coordinator_model_is_top(model: &str) -> bool {
    model_words(model)
        .iter()
        .any(|word| matches!(word.as_str(), "opus" | "fable" | "astra" | "sol"))
}

/// True for medium effort or below (`medium`, `low`, `minimal`, `none`).
pub fn coordinator_effort_is_medium_or_lower(effort: &str) -> bool {
    matches!(
        effort.trim().to_ascii_lowercase().as_str(),
        "medium" | "med" | "low" | "minimal" | "min" | "none"
    )
}

/// True when a coordinator on `model` (its value and label, either may be empty) at `effort` may
/// give a degraded experience.
pub fn coordinator_model_may_degrade(
    model_value: Option<&str>,
    model_label: Option<&str>,
    effort: Option<&str>,
) -> bool {
    let names: Vec<&str> = [model_value, model_label]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect();
    if names.iter().any(|name| coordinator_model_is_low_end(name)) {
        return true;
    }
    names.iter().any(|name| coordinator_model_is_top(name))
        && effort.is_some_and(coordinator_effort_is_medium_or_lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deepseek_harness_session_can_be_a_coordinator() {
        assert!(coordinator_agent_family_supported("dsh"));
        assert_eq!(coordinator_agent_family_name("dsh"), "DeepSeek Harness");
        assert!(coordinator_agent_families_text().ends_with("OpenCode or DeepSeek Harness"));
        assert!(coordinator_agent_family_supported("kiro"));
    }

    #[test]
    fn a_deepseek_harness_coordinator_starts_on_its_configured_model() {
        assert!(!coordinator_launch_takes_model("dsh"));
        assert!(!coordinator_launch_takes_effort("dsh"));
        assert!(coordinator_launch_takes_model("opencode"));
        assert!(coordinator_launch_takes_effort("claude"));
    }

    #[test]
    fn only_a_low_end_pick_may_degrade_a_coordinator() {
        let degrades = |value: &str, label: &str, effort: Option<&str>| {
            coordinator_model_may_degrade(Some(value), Some(label), effort)
        };
        // Low-end tiers warn at any effort.
        assert!(degrades("claude-haiku-5-5", "Haiku 5.5", None));
        assert!(degrades("sonnet", "Sonnet 5.5", Some("high")));
        assert!(degrades("deepseek-v4-flash", "DeepSeek Flash", Some("max")));
        assert!(degrades("gemini-3-flash-lite", "", None));
        assert!(degrades("gpt-6-mini", "GPT-6 Mini", Some("xhigh")));
        // The top four warn at medium or below only.
        assert!(degrades("opus[1m]", "Opus 5.5", Some("medium")));
        assert!(degrades("claude-fable-5-2", "Fable 5.2", Some("low")));
        assert!(degrades("gpt-6.1-sol", "GPT-6.1 Sol", Some("minimal")));
        assert!(!degrades("gpt-6.1-astra", "GPT-6.1 Astra", Some("high")));
        assert!(!degrades("opus", "Opus 5.5", Some("xhigh")));
        assert!(!degrades("claude-opus-5-5", "", Some("max")));
        assert!(!degrades("opus", "Opus 5.5", None));
        // Every other model says nothing (#665: Argon), as does an unknown one.
        assert!(!degrades(
            "gemini-pro-orchestrator-g1-pro",
            "Orchestrator (Argon)",
            Some("low")
        ));
        assert!(!coordinator_model_may_degrade(None, None, Some("low")));
        // "gemini" holds "mini" but is not the word.
        assert!(!degrades("gemini-3-pro", "Gemini 3 Pro", None));
    }
}
