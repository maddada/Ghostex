//! The agents gxserver lets an orchestrator run on: `ghostex orchestrator create` and promote
//! refuse the others, and every client offers New Orchestrator and Make Orchestrator only for these.

/// The agent families a new orchestrator can run on, in the order `ghostex orchestrator create`
/// prefers them when no agent is named.
///
/// CDXC:Coordinators 2026-10-20 WHY:
/// An orchestrator must read a skill, run the `ghostex` CLI and take Ghostex's chat sends (the user's messages and its threads' reports). These are the chat agents that load skills from a folder Ghostex can install into; OpenCode 2 joined once its chat landed (sends go through its server API, it reads `~/.agents/skills` itself). Left out: Empryo (no skill folder Ghostex knows of; its older orchestrators keep the `/agent` profile), Hermes (its replies and queued sends never reach the chat, agent flow tests 2026-10-17), Freebuff (no skill folder), and agents without a chat (Command Code, Gemini and the rest: no transcript reader and no input-box reading, so the first message and every report would be typed blind and the Threads panel has no chat to live in). Supersedes the 2026-10-17 WHY that left OpenCode out for having no chat.
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
/// User (Q2980 "Build it: auto-send + remind on drift"): "Any harness, account and model is allowed: remove every limit on which model or harness a coordinator can use." So gxserver no longer refuses a coordinator, a promotion or Make Coordinator for an agent outside [`COORDINATOR_AGENT_FAMILIES`]; that list now only orders `ghostex coordinator create`'s default, names the agents whose skill folder Ghostex installs the playbook into (the others get the guide line, gx-chat-core `coordinator_skill_line`) and the ones whose launch takes a model. A weak or unknown model gets a warning instead ([`coordinator_quality_concern`]). Supersedes the 2026-10-20 WHY above as a limit.
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

/// The label of the first Model row of a family whose new coordinator can also keep the agent's
/// own configured model (no value, no effort): OpenCode, whose lineup marks no default and runs
/// hundreds of models across providers.
pub fn coordinator_default_model_label(family: &str) -> Option<&'static str> {
    (family == "opencode").then_some("OpenCode's default")
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

/// Why a coordinator's model may make it work worse: what its chat's warning pill, the agent
/// picker's step and its guide line say.
///
/// CDXC:Coordinators 2026-11-13 DECISION:
/// User (Q2980 "Build it: auto-send + remind on drift"): any agent, account and model may run a coordinator, but when its model is not Opus, Fable, GPT Sol or GPT Astra (any version), its effort is below medium, or Ghostex does not know its model, Ghostex warns: its guide gets a line telling it to tell the user once that a coordinator needs Opus 5.5 at medium or high, or GPT 6.1 Sol at high, its chat shows "Coordinator quality may be degraded on <model>" (dismissible per session, live as the model or effort changes), and the agent picker step shows the same warning for such a model. An effort Ghostex does not know is no reason on its own.
/// SEE-ALSO: server/src/coordinators/playbook.rs (the guide line), packages/gx-chat-core (the pill), apps/desktop/src/app/window/new_thread_picker.rs (the picker step), apps/mobile/app/src/sessions/coordinatorQuality.ts (the phone's copy, which cannot link this crate).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorQualityConcern {
    /// The model as the user knows it (its label, else its value); `None` when it is unknown.
    pub model: Option<String>,
    /// The effort, when it is known.
    pub effort: Option<String>,
    /// The effort is below medium.
    pub low_effort: bool,
}

/// The words of a model value or label, lowercased: `claude-opus-5-5` reads `claude opus 5 5`.
fn model_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// True for a model a coordinator works well on: Opus, Fable, GPT Sol or GPT Astra, any version
/// (`opus[1m]`, `claude-opus-5-5`, `Fable 5`, `gpt-6.1-sol`, `GPT-6.1 Astra`).
pub fn coordinator_model_is_strong(model: &str) -> bool {
    let words = model_words(model);
    let has = |word: &str| words.iter().any(|candidate| candidate == word);
    has("opus") || has("fable") || (has("gpt") && (has("sol") || has("astra")))
}

/// True for an effort below medium (`low`, `minimal`, `none`).
pub fn coordinator_effort_is_low(effort: &str) -> bool {
    matches!(
        effort.trim().to_ascii_lowercase().as_str(),
        "low" | "minimal" | "none" | "min"
    )
}

/// The concern about a coordinator on `model` (its value and label, either may be empty) at
/// `effort`, or `None` when it runs on a model it works well on at medium or above.
pub fn coordinator_quality_concern(
    model_value: Option<&str>,
    model_label: Option<&str>,
    effort: Option<&str>,
) -> Option<CoordinatorQualityConcern> {
    let present = |text: Option<&str>| {
        text.map(str::trim)
            .filter(|text| !text.is_empty() && !text.eq_ignore_ascii_case("default"))
            .map(str::to_string)
    };
    let value = present(model_value);
    let label = present(model_label);
    let effort = present(effort);
    let strong = [value.as_deref(), label.as_deref()]
        .into_iter()
        .flatten()
        .any(coordinator_model_is_strong);
    let low_effort = effort.as_deref().is_some_and(coordinator_effort_is_low);
    if strong && !low_effort {
        return None;
    }
    Some(CoordinatorQualityConcern {
        model: label.or(value),
        effort,
        low_effort,
    })
}

impl CoordinatorQualityConcern {
    /// What it runs on, for the pill: "Sonnet 5.5", "Opus 5.5 at low effort", "an unknown model".
    pub fn runs_on(&self) -> String {
        let model = self
            .model
            .clone()
            .unwrap_or_else(|| "an unknown model".to_string());
        match (&self.effort, self.low_effort) {
            (Some(effort), true) => format!("{model} at {effort} effort"),
            _ => model,
        }
    }

    /// The warning pill's words: "Coordinator quality may be degraded on Sonnet 5.5".
    pub fn pill_text(&self) -> String {
        format!("Coordinator quality may be degraded on {}", self.runs_on())
    }

    /// The line a coordinator's guide gets.
    pub fn guide_line(&self) -> String {
        let model = self
            .model
            .clone()
            .unwrap_or_else(|| "a model Ghostex can't identify".to_string());
        let runs_on = match &self.effort {
            Some(effort) => format!("{model} at {effort} effort"),
            None => model,
        };
        format!(
            "You run on {runs_on}. Tell the user once that a coordinator needs Opus 5.5 at medium or high, or GPT 6.1 Sol at high, to work well."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deepseek_harness_session_can_be_a_coordinator() {
        assert!(coordinator_agent_family_supported("dsh"));
        assert_eq!(coordinator_agent_family_name("dsh"), "DeepSeek Harness");
        assert!(coordinator_agent_families_text().ends_with("OpenCode or DeepSeek Harness"));
        assert!(coordinator_agent_family_supported("gemini"));
    }

    #[test]
    fn a_deepseek_harness_coordinator_starts_on_its_configured_model() {
        assert!(!coordinator_launch_takes_model("dsh"));
        assert!(!coordinator_launch_takes_effort("dsh"));
        assert!(coordinator_launch_takes_model("opencode"));
        assert!(coordinator_launch_takes_effort("claude"));
    }
}
