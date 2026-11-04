//! The agent a launch names, read off the sidebar HUD, and the three facts a launch derives from
//! it: the hook family its hooks are checked for, the default session title, and the first-prompt
//! title settings gxserver generates a title from.
//!
//! SEE-ALSO: packages/shared/sidebar-agents.ts (deleted 2026-10-01) (`getDefaultSidebarAgentByIcon`),
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
        "claude-cloud" => "claude-cloud",
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
        "freebuff" => "freebuff",
        "empryo" => "empryo",
        "dsh" => "dsh",
        _ => return None,
    })
}

/// The agent id a launch on ANOTHER computer sends: that computer's gxserver resolves the command
/// from its own agents, so a custom agent of this computer travels as the built-in agent its icon
/// names or, with no built-in icon ("Nothing (plain command)"), the built-in agent its command
/// starts. Any other id, and a custom agent that names no built-in either way, is sent as it is.
///
/// CDXC:RemoteMachines 2026-10-01 WHY: Custom agent ids (`custom-claude-70-…`) are generated per computer and never exist on a remote machine, so sending one made the remote gxserver refuse the create ("no launch command for agent"): the Mac's "Claude" button, a custom agent running `claude`, could not start Claude on the Windows machine. The remote still picks its own command for that agent, as `CDXC:RemoteMachines 2026-06-24-17:19` requires.
///
/// CDXC:RemoteMachines 2026-10-25 WHY: A custom agent added with Works like "Nothing (plain command)" and the command `omp` has no icon, so it still travelled as its `custom-omp-…` id and the remote refused it (Discord report, 2026-10-24: "Add custom agent -> omp" did nothing on a remote machine while More agents' OMP worked). The command's executable names the built-in the same way gxserver's `infer_agent_id_from_command_executable` reads it for a local launch.
pub fn remote_launch_agent_id(hud: Option<&Value>, agent_id: &str) -> String {
    let agent_id = js_trim(agent_id);
    resolve_sidebar_agent(hud, agent_id)
        .filter(|agent| agent.agent_id.starts_with("custom-"))
        .and_then(|agent| {
            default_agent_id_for_icon(agent.icon.as_deref()).or_else(|| {
                agent
                    .command
                    .as_deref()
                    .and_then(built_in_agent_id_for_command)
            })
        })
        .map_or_else(|| agent_id.to_string(), str::to_string)
}

/// The built-in agent a command line starts, read from its executable word (environment
/// assignments and `env`/`exec`/`command` prefixes skipped; path, quotes and Windows suffixes
/// dropped), so `FOO=1 /opt/bin/omp --flag` is OMP and `git log | grep claude` is nothing.
///
/// SEE-ALSO: server/src/agents/identity/agent_ids.rs (`infer_agent_id_from_command`), whose
/// executable table this matches.
pub fn built_in_agent_id_for_command(command: &str) -> Option<&'static str> {
    let word = command
        .split_whitespace()
        .map(|word| word.trim_matches(['\'', '"']))
        .find(|word| !word.contains('=') && !matches!(*word, "env" | "exec" | "command"))?
        .to_ascii_lowercase();
    let basename = word.rsplit(['/', '\\']).next().unwrap_or(&word);
    let executable = [".exe", ".cmd", ".bat", ".ps1"]
        .iter()
        .find_map(|suffix| basename.strip_suffix(suffix))
        .unwrap_or(basename);
    Some(match executable {
        "cursor-agent" => "cursor",
        "hermes" => "hermes-agent",
        "codebuddy" => "codebuddy",
        "agy" => "antigravity",
        "opencode" => "opencode",
        "omp" => "omp",
        "rovodev" => "rovodev",
        "qodercli" => "qoder",
        "empryo" | "em" => "empryo",
        "commandcode" => "command-code",
        "openclaude" => "openclaude",
        "mastracode" => "mastra",
        "devin" => "devin",
        "kimi" => "kimi",
        "kiro-cli" => "kiro",
        "claude" | "cswap" => "claude",
        "copilot" => "copilot",
        "gemini" => "gemini",
        "codex" | "xswap" => "codex",
        "zcode" | "zcode-cli" => "zcode",
        "droid" => "droid",
        "freebuff" => "freebuff",
        "dsh-tui" | "dst" => "dsh",
        "grok" => "grok",
        "amp" => "amp",
        "pi" => "pi",
        _ => return None,
    })
}

/// The orchestrator family a launcher runs as (`COORDINATOR_AGENT_FAMILIES`): by its icon, its
/// agent id, or the executable its command runs, the first of those an orchestrator supports.
///
/// CDXC:Coordinators 2026-11-02 WHY: A custom launcher added with Works like "Nothing (plain command)" and the command `codex` has no icon and a `custom-codex-…` id, so reading only the icon and the id handed an orchestrator off as a plain session while gxserver (which also reads the command) would have run it as an orchestrator.
/// SEE-ALSO: server/src/coordinators/create.rs (`coordinator_launcher_family`, gxserver's copy of the same order).
pub fn coordinator_launcher_family(
    agent_id: &str,
    icon: Option<&str>,
    command: Option<&str>,
) -> Option<&'static str> {
    let agent_id = js_trim(agent_id);
    let by_icon = default_agent_id_for_icon(icon);
    let by_id = default_agent_id_for_icon(Some(agent_id)).or(Some(agent_id));
    let by_command = command.and_then(built_in_agent_id_for_command);
    [by_icon, by_id, by_command]
        .into_iter()
        .flatten()
        .find_map(|family| {
            ghostex_gx_protocol::coordinator::COORDINATOR_AGENT_FAMILIES
                .into_iter()
                .find(|supported| *supported == family)
        })
}

#[cfg(test)]
mod coordinator_family_tests {
    use super::coordinator_launcher_family;

    #[test]
    fn a_plain_command_launcher_runs_as_the_orchestrator_family_its_command_starts() {
        assert_eq!(
            coordinator_launcher_family("custom-codex-70-abc", None, Some("codex --yolo")),
            Some("codex")
        );
        assert_eq!(
            coordinator_launcher_family("custom-c-1", None, Some("FOO=1 /opt/bin/claude")),
            Some("claude")
        );
    }

    #[test]
    fn icon_and_id_still_name_the_family() {
        assert_eq!(
            coordinator_launcher_family("custom-x-1", Some("claude"), Some("./run.sh")),
            Some("claude")
        );
        assert_eq!(
            coordinator_launcher_family("codex", None, None),
            Some("codex")
        );
        assert_eq!(
            coordinator_launcher_family("cursor-cli", None, None),
            Some("cursor")
        );
    }

    #[test]
    fn a_launcher_no_orchestrator_runs_on_has_no_family() {
        assert_eq!(
            coordinator_launcher_family("custom-g-1", Some("gemini"), Some("gemini")),
            None
        );
        assert_eq!(
            coordinator_launcher_family("custom-my-1", None, Some("./run.sh")),
            None
        );
    }
}

#[cfg(test)]
mod remote_launch_tests {
    use super::*;

    fn hud(agent: Value) -> Value {
        json!({ "agents": [agent] })
    }

    #[test]
    fn plain_command_custom_agent_travels_as_the_built_in_it_runs() {
        let hud = hud(json!({ "agentId": "custom-omp-70-abc", "name": "omp", "command": "omp" }));
        assert_eq!(
            remote_launch_agent_id(Some(&hud), "custom-omp-70-abc"),
            "omp"
        );
    }

    #[test]
    fn icon_wins_over_command() {
        let hud = hud(json!({
            "agentId": "custom-x-1", "name": "X", "icon": "claude", "command": "codex"
        }));
        assert_eq!(remote_launch_agent_id(Some(&hud), "custom-x-1"), "claude");
    }

    #[test]
    fn unknown_plain_command_keeps_its_id() {
        let hud = hud(json!({ "agentId": "custom-my-1", "name": "Mine", "command": "./run.sh" }));
        assert_eq!(
            remote_launch_agent_id(Some(&hud), "custom-my-1"),
            "custom-my-1"
        );
    }

    #[test]
    fn built_in_ids_pass_through() {
        let hud = hud(json!({ "agentId": "omp", "name": "OMP", "icon": "omp", "command": "omp" }));
        assert_eq!(remote_launch_agent_id(Some(&hud), "omp"), "omp");
    }

    #[test]
    fn executable_word_is_read_like_gxserver() {
        assert_eq!(
            built_in_agent_id_for_command("FOO=1 env /opt/bin/omp --x"),
            Some("omp")
        );
        assert_eq!(
            built_in_agent_id_for_command("\"C:\\bin\\Codex.exe\""),
            Some("codex")
        );
        assert_eq!(
            built_in_agent_id_for_command("cursor-agent --yolo"),
            Some("cursor")
        );
        assert_eq!(built_in_agent_id_for_command("git log | grep claude"), None);
    }
}

/// `DEFAULT_SESSION_AGENT_TITLE_NAMES`.
fn default_session_agent_title_name(lowercased: &str) -> Option<&'static str> {
    Some(match lowercased {
        "agy" | "antigravity" | "antigravity-cli" => "Antigravity CLI",
        "amp" | "amp-cli" => "Amp CLI",
        "claude" | "claude-code" => "Claude",
        "claude-cloud" => "Claude (cloud)",
        "codebuddy" | "code-buddy" => "CodeBuddy",
        "codex" | "codex-cli" => "Codex",
        "command-code" | "commandcode" => "Command Code",
        "copilot" => "Copilot",
        "cursor" | "cursor-cli" => "Cursor CLI",
        "mastra" | "mastracode" => "Mastra Code",
        "devin" => "Devin",
        "droid" | "factory-droid" => "Factory Droid",
        "empryo" | "em" => "Empryo",
        "freebuff" => "Freebuff",
        "dsh" | "dsh-tui" => "DeepSeek Harness",
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
                    "codex"
                        | "cursor"
                        | "claude"
                        | "grok"
                        | "pi"
                        | "antigravity"
                        | "empryo"
                        | "custom"
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
