//! The agent a launch names, read off the sidebar HUD, and the three facts a launch derives from
//! it: the hook family its hooks are checked for, the default session title, and the first-prompt
//! title settings gxserver generates a title from.
//!
//! SEE-ALSO: packages/shared/sidebar-agents.ts (`getDefaultSidebarAgentByIcon`),
//! packages/shared/session-grid-contract-session.ts (`createAgentSessionDefaultTitle`).
//! `createFirstPromptTitleRuntimeSettings` and `resolveSessionTitleGenerationCommandForGxserver`
//! were ported from the deleted `gxserver-runtime/session-create.ts` (see git history).

use serde_json::{json, Map, Value};

use crate::sidebar_view::text::js_trim;

/// One launcher agent, as the HUD's `agents` list carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarAgent {
    pub agent_id: String,
    pub name: String,
    pub icon: Option<String>,
    pub command: Option<String>,
}

impl SidebarAgent {
    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            agent_id: value.get("agentId")?.as_str()?.to_string(),
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            icon: value
                .get("icon")
                .and_then(Value::as_str)
                .map(str::to_string),
            command: value
                .get("command")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    /// `agent.command` when it is a non-blank string, which is what `if (!agent.command) return`
    /// and `agent?.command?.trim() || undefined` test.
    pub fn launch_command(&self) -> Option<&str> {
        self.command
            .as_deref()
            .filter(|command| !command.is_empty())
    }
}

/// `resolveSidebarAgent(agentId)`: the HUD agent with this id, the id trimmed first.
pub fn resolve_sidebar_agent(hud: Option<&Value>, agent_id: &str) -> Option<SidebarAgent> {
    let agent_id = js_trim(agent_id);
    if agent_id.is_empty() {
        return None;
    }
    hud?.get("agents")?
        .as_array()?
        .iter()
        .filter_map(SidebarAgent::from_json)
        .find(|agent| agent.agent_id == agent_id)
}

/// `getDefaultSidebarAgentByIcon(icon)?.agentId`: the built-in agent whose icon this is, which is
/// the family its hooks are installed for. `None` for no icon and for the browser.
pub fn default_agent_id_for_icon(icon: Option<&str>) -> Option<&'static str> {
    Some(match icon? {
        "codex" => "codex",
        "claude" => "claude",
        "cursor-cli" => "cursor",
        "pi" => "pi",
        "opencode" => "opencode",
        "gemini" => "gemini",
        "copilot" => "copilot",
        "factory-droid" => "droid",
        "grok-build" => "grok",
        "antigravity-cli" => "antigravity",
        "amp-cli" => "amp",
        "hermes-agent" => "hermes-agent",
        "rovo-dev" => "rovodev",
        "codebuddy" => "codebuddy",
        "qoder" => "qoder",
        "kiro" => "kiro",
        "omp" => "omp",
        "kimi" => "kimi",
        "openclaude" => "openclaude",
        "command-code" => "command-code",
        "devin" => "devin",
        "mastra" => "mastra",
        "zcode" => "zcode",
        _ => return None,
    })
}

/// `DEFAULT_SESSION_AGENT_TITLE_NAMES`.
fn default_session_agent_title_name(lowercased: &str) -> Option<&'static str> {
    Some(match lowercased {
        "agy" | "antigravity" | "antigravity-cli" => "Antigravity CLI",
        "amp" | "amp-cli" => "Amp CLI",
        "claude" | "claude-code" => "Claude",
        "codebuddy" | "code-buddy" => "CodeBuddy",
        "codex" | "codex-cli" => "Codex",
        "command-code" | "commandcode" => "Command Code",
        "copilot" => "Copilot",
        "cursor" | "cursor-cli" => "Cursor CLI",
        "mastra" | "mastracode" => "Mastra Code",
        "devin" => "Devin",
        "droid" | "factory-droid" => "Factory Droid",
        "gemini" => "Gemini",
        "grok" | "grok-build" => "Grok Build",
        "hermes" | "hermes-agent" => "Hermes Agent",
        "kimi" | "kimi-code" => "Kimi Code",
        "kiro" | "kiro-cli" => "Kiro CLI",
        "omp" => "OMP",
        "openclaude" | "open-claude" => "OpenClaude",
        "opencode" | "open-code" => "OpenCode",
        "pi" | "π" => "Pi",
        "qoder" | "qodercli" => "Qoder",
        "rovo" | "rovo-dev" | "rovodev" => "Rovo Dev",
        _ => return None,
    })
}

/// `createAgentSessionDefaultTitle(agentName)`: the name with its whitespace collapsed, mapped to
/// the built-in title name when it is one, plus " Session"; `Terminal Session` for no name.
pub fn agent_session_default_title(agent_name: Option<&str>) -> String {
    let normalized = agent_name
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|name| !name.is_empty());
    match normalized {
        Some(name) => {
            let title_name = default_session_agent_title_name(&name.to_lowercase())
                .map(str::to_string)
                .unwrap_or(name);
            format!("{title_name} Session")
        }
        None => crate::selectors::DEFAULT_TERMINAL_SESSION_TITLE.to_string(),
    }
}

/// The two settings the first-prompt title reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TitleGenerationSettings {
    /// `sessionTitleGenerationAgent`: an agent id, or `custom`.
    pub agent: String,
    /// `customSessionTitleGenerationCommand`.
    pub custom_command: String,
}

impl TitleGenerationSettings {
    /// The two keys of the shared settings document, normalized as `normalizeSettings` does: an
    /// agent outside `SESSION_TITLE_GENERATION_AGENT_OPTIONS` reads as the default `codex`, and the
    /// custom command is trimmed and cut to 240 characters.
    pub fn from_settings(settings: &Value) -> Self {
        let agent = settings
            .get("sessionTitleGenerationAgent")
            .and_then(Value::as_str)
            .filter(|agent| {
                matches!(
                    *agent,
                    "codex" | "cursor" | "claude" | "grok" | "pi" | "antigravity" | "custom"
                )
            })
            .unwrap_or("codex")
            .to_string();
        let custom_command = settings
            .get("customSessionTitleGenerationCommand")
            .and_then(Value::as_str)
            .map(|command| js_trim(command).chars().take(240).collect())
            .unwrap_or_default();
        Self {
            agent,
            custom_command,
        }
    }
}

/// `createFirstPromptTitleRuntimeSettings(firstUserMessage, firstUserInputDraft)`.
///
/// CDXC:SessionTitles 2026-07-04-21:52:
/// GPUI agent sessions must carry the same gxserver-owned first-prompt title settings as macOS
/// before hooks claim the prompt. The daemon still owns eligibility, title generation, and command
/// submission; GPUI only supplies the user's saved title-generation agent/command and any
/// already-known first prompt.
///
/// CDXC:TranscriptExport 2026-08-20:
/// A draft is the opposite of `firstUserMessage`: gxserver types it into the new agent's composer
/// once and never submits it. It travels to the daemon byte for byte (the trailing space of
/// `@<path> ` is what closes the file mention and separates it from the prompt the user writes
/// next), so it is deliberately not trimmed here or anywhere else on the way out.
pub fn first_prompt_title_runtime_settings(
    settings: &TitleGenerationSettings,
    hud: Option<&Value>,
    first_user_message: Option<&str>,
    first_user_input_draft: Option<&str>,
) -> Map<String, Value> {
    let mut runtime_settings = Map::new();
    runtime_settings.insert(
        "firstPromptTitleGenerationAgent".to_string(),
        json!(settings.agent),
    );
    let command = if settings.agent == "custom" {
        Some(js_trim(&settings.custom_command).to_string()).filter(|command| !command.is_empty())
    } else {
        resolve_sidebar_agent(hud, &settings.agent)
            .and_then(|agent| agent.command)
            .map(|command| js_trim(&command).to_string())
            .filter(|command| !command.is_empty())
    };
    if let Some(command) = command {
        runtime_settings.insert(
            "firstPromptTitleGenerationCommand".to_string(),
            json!(command),
        );
    }
    if let Some(prompt) = first_user_message
        .map(js_trim)
        .filter(|prompt| !prompt.is_empty())
    {
        runtime_settings.insert("firstUserMessage".to_string(), json!(prompt));
    }
    if let Some(draft) = first_user_input_draft.filter(|draft| !draft.is_empty()) {
        runtime_settings.insert("firstUserInputDraft".to_string(), json!(draft));
    }
    runtime_settings
}
