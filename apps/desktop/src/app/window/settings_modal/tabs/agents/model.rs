//! What the Agents page reads and the small rules it ports from TypeScript: the roster
//! (`hud.agents`), the hook status payload, the default agent registry, the approval-policy and
//! Chat View support checks, the Default interface options, the title-generation command preview
//! and the draft-order helpers of drag reorder.
//!
//! CDXC:AgentProviders 2026-09-28 SEE-ALSO:
//! Ported from packages/shared/sidebar-agents.ts (deleted 2026-10-01) (`getDefaultSidebarAgentById` / `ByIcon`), packages/shared/sidebar-agent-accept-all.ts (deleted 2026-10-01) (`resolveAgentAcceptAllSpec`), packages/shared/session-chat-agents.ts (`resolveSessionChatTranscriptAgent`), packages/shared/ghostex-settings/option-tables.ts (deleted 2026-10-01) (`getPreferredAgentInterfaceOverrideOptions`), packages/shared/ghostex-settings/session-title-generation.ts (deleted 2026-10-01) (`getSessionTitleGenerationCommandPreview`) and packages/core-ui/settings-modal/drag-data.ts (deleted 2026-10-01); the data tables come from the generated catalog, so only this logic must follow those files.
use super::super::super::catalog::{SettingOption, module, settings_catalog};
use serde_json::Value;
use std::sync::OnceLock;

/// One agent of the roster (`SidebarAgentButton`, or a gxserver `agentRoster` entry that also
/// carries agents that are off).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AgentButton {
    pub(super) agent_id: String,
    pub(super) name: String,
    pub(super) command: Option<String>,
    pub(super) icon: Option<String>,
    pub(super) accept_all_mode: Option<String>,
    /// The agent is on: it shows in the launcher, the New Thread picker and the phone.
    pub(super) enabled: bool,
    /// A built-in agent (no Delete, only off).
    pub(super) is_default: bool,
    /// The newest session with this agent, absent when it was never used.
    pub(super) last_used_at: Option<String>,
    /// A built-in agent's own name and command, for Reset to defaults.
    pub(super) default_name: Option<String>,
    pub(super) default_command: Option<String>,
}

impl AgentButton {
    pub(super) fn used_before(&self) -> bool {
        self.last_used_at.is_some()
    }

    /// The row lives in the list (on, used before, or the user's own) rather than in More agents.
    pub(super) fn listed(&self) -> bool {
        self.enabled || self.used_before() || !self.is_default
    }
}

fn agent_button(agent: &Value, enabled: bool) -> Option<AgentButton> {
    Some(AgentButton {
        agent_id: text(agent, "agentId")?,
        name: text(agent, "name").unwrap_or_default(),
        command: text(agent, "command"),
        icon: text(agent, "icon").filter(|icon| !icon.is_empty()),
        accept_all_mode: text(agent, "acceptAllMode"),
        enabled: agent
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(enabled),
        is_default: agent.get("isDefault").and_then(Value::as_bool) == Some(true),
        last_used_at: text(agent, "lastUsedAt").filter(|at| !at.is_empty()),
        default_name: text(agent, "defaultName"),
        default_command: text(agent, "defaultCommand"),
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// `useSidebarStore((state) => state.hud.agents)`: the agents that are on.
pub(super) fn agents_from_hud(hud: Option<&Value>) -> Vec<AgentButton> {
    hud.and_then(|hud| hud.get("agents"))
        .and_then(Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .filter_map(|agent| agent_button(agent, true))
                .collect()
        })
        .unwrap_or_default()
}

/// The `agentRoster` of a `readSidebarHud { includeAgentRoster }` answer: every agent, on and
/// off, in the shared order. `None` from a gxserver that predates it.
pub(super) fn roster_from_hud_answer(answer: &Value) -> Option<Vec<AgentButton>> {
    answer
        .get("agentRoster")
        .and_then(Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .filter_map(|agent| agent_button(agent, false))
                .collect()
        })
}

/// "3 weeks ago" for a session time (RFC 3339).
pub(super) fn last_used_label(at: &str) -> String {
    let Ok(time) = chrono::DateTime::parse_from_rfc3339(at) else {
        return "Used before".to_string();
    };
    let days = (chrono::Utc::now() - time.with_timezone(&chrono::Utc))
        .num_days()
        .max(0);
    let ago = match days {
        0 => return "Last used today".to_string(),
        1 => return "Last used yesterday".to_string(),
        2..=13 => format!("{days} days"),
        14..=59 => format!("{} weeks", days / 7),
        60..=364 => format!("{} months", days / 30),
        _ => return "Last used over a year ago".to_string(),
    };
    format!("Last used {ago} ago")
}

/// One entry of `DEFAULT_SIDEBAR_AGENTS`.
#[derive(Clone, Debug)]
pub(super) struct DefaultAgent {
    pub(super) agent_id: String,
    pub(super) name: String,
    pub(super) icon: String,
    pub(super) command: String,
}

fn parse_default_agents(value: Option<&Value>) -> Vec<DefaultAgent> {
    value
        .and_then(Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .filter_map(|agent| {
                    Some(DefaultAgent {
                        agent_id: text(agent, "agentId")?,
                        name: text(agent, "name").unwrap_or_default(),
                        icon: text(agent, "icon").unwrap_or_default(),
                        command: text(agent, "command").unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `DEFAULT_SIDEBAR_AGENTS`.
pub(super) fn default_agents() -> &'static [DefaultAgent] {
    static AGENTS: OnceLock<Vec<DefaultAgent>> = OnceLock::new();
    AGENTS.get_or_init(|| {
        parse_default_agents(
            settings_catalog().module_value(module::SIDEBAR_AGENTS, "DEFAULT_SIDEBAR_AGENTS"),
        )
    })
}

/// `AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS`.
pub(super) fn hook_supported_agents() -> &'static [DefaultAgent] {
    static AGENTS: OnceLock<Vec<DefaultAgent>> = OnceLock::new();
    AGENTS.get_or_init(|| {
        parse_default_agents(settings_catalog().module_value(
            module::SETTINGS_TYPES,
            "AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS",
        ))
    })
}

/// `getDefaultSidebarAgentById`.
pub(super) fn default_agent_by_id(agent_id: &str) -> Option<&'static DefaultAgent> {
    let normalized = agent_id.trim().to_lowercase();
    default_agents()
        .iter()
        .find(|agent| agent.agent_id == normalized)
}

/// `getDefaultSidebarAgentByIcon`.
pub(super) fn default_agent_by_icon(icon: Option<&str>) -> Option<&'static DefaultAgent> {
    let icon = icon.filter(|icon| !icon.is_empty() && *icon != "browser")?;
    default_agents().iter().find(|agent| agent.icon == icon)
}

/// The hook row of a roster agent: hooks are per CLI, so a launcher resolves to its default agent
/// through its icon, the mapping session creation uses (CDXC:AgentHooks 2026-08-28).
pub(super) fn hook_agent_id(agent: &AgentButton) -> Option<String> {
    let default_id = default_agent_by_icon(agent.icon.as_deref())?
        .agent_id
        .clone();
    hook_supported_agents()
        .iter()
        .find(|entry| entry.agent_id == default_id)
        .map(|entry| entry.agent_id.clone())
}

fn accept_all_spec_is_set(agent_id: &str) -> bool {
    settings_catalog()
        .module_value(module::SIDEBAR_AGENT_ACCEPT_ALL, "AGENT_ACCEPT_ALL_SPECS")
        .and_then(|specs| specs.get(agent_id))
        .is_some_and(|spec| !spec.is_null())
}

/// `supportsAgentAcceptAll(agentId, icon)` (`resolveAgentAcceptAllSpec(..) !== undefined`).
pub(super) fn supports_accept_all(agent_id: &str, icon: Option<&str>) -> bool {
    if let Some(default_agent) = default_agent_by_id(agent_id)
        && accept_all_spec_is_set(&default_agent.agent_id)
    {
        return true;
    }
    default_agent_by_icon(icon).is_some_and(|agent| accept_all_spec_is_set(&agent.agent_id))
}

/// `AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS`.
pub(super) fn accept_all_mode_options() -> Vec<SettingOption> {
    settings_catalog().options(
        module::SIDEBAR_AGENT_ACCEPT_ALL,
        "AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS",
    )
}

/// `resolveSessionChatTranscriptAgent(agentId, icon) !== null` (`agentSupportsChatView`).
pub(super) fn supports_chat_view(agent_id: &str, icon: Option<&str>) -> bool {
    ghostex_gx_chat_core::extras::agents::transcript_agent([Some(agent_id), icon]).is_some()
}

/// `getPreferredAgentInterfaceOverrideOptions(global)`: Inherit (the global choice's label), then
/// every interface.
pub(super) fn preferred_interface_override_options(global: &str) -> Vec<SettingOption> {
    let catalog = settings_catalog();
    let options = catalog.options(module::SETTINGS, "PREFERRED_AGENT_INTERFACE_OPTIONS");
    let inherited = options
        .iter()
        .find(|option| option.value == global)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| global.to_string());
    let mut all = vec![SettingOption {
        label: format!("Inherit ({inherited})"),
        value: inherit_value(),
    }];
    all.extend(options);
    all
}

/// `PREFERRED_AGENT_INTERFACE_INHERIT_VALUE`.
pub(super) fn inherit_value() -> String {
    let value =
        settings_catalog().text(module::SETTINGS, "PREFERRED_AGENT_INTERFACE_INHERIT_VALUE");
    if value.is_empty() {
        "inherit".to_string()
    } else {
        value
    }
}

/// `SESSION_TITLE_GENERATION_AGENT_OPTIONS`.
pub(super) fn title_generation_options() -> Vec<SettingOption> {
    settings_catalog().options(module::SETTINGS, "SESSION_TITLE_GENERATION_AGENT_OPTIONS")
}

/// `resolveSettingsTitleGenerationCommand`.
pub(super) fn resolve_title_generation_command(
    agent: &str,
    agents: &[AgentButton],
    custom_command: &str,
) -> Option<String> {
    if agent == "custom" {
        return Some(custom_command.trim().to_string());
    }
    agents
        .iter()
        .find(|candidate| candidate.agent_id == agent)
        .and_then(|candidate| candidate.command.as_deref())
        .map(|command| command.trim().to_string())
}

fn preview_command(agent: &str, command: Option<&str>) -> String {
    if let Some(configured) = command.map(str::trim).filter(|command| !command.is_empty()) {
        return configured.to_string();
    }
    match agent {
        "codex" => "codex",
        "cursor" => "cursor-agent",
        "claude" => "claude",
        "grok" => "grok",
        "pi" => "pi",
        "antigravity" => "agy",
        "empryo" => "empryo",
        _ => "<custom command>",
    }
    .to_string()
}

/// The configured command with its permission-bypass, approval and sandbox flags removed, as
/// gxserver's `one_shot_text_job_command` (server/src/agents/accept_all.rs) builds the title job.
fn one_shot_text_job_command(agent: &str, command: &str) -> String {
    let (single, paired): (&[&str], &[&str]) = match agent {
        "codex" => (
            &[
                "--yolo",
                "--approve-for-me",
                "--dangerously-bypass-approvals-and-sandbox",
                "--full-auto",
            ],
            &["-a", "--ask-for-approval", "-s", "--sandbox"],
        ),
        "claude" => (
            &[
                "--dangerously-skip-permissions",
                "--allow-dangerously-skip-permissions",
            ],
            &["--permission-mode"],
        ),
        "cursor" => (
            &["--yolo", "--force", "-f", "--auto-review", "--plan"],
            &["--mode"],
        ),
        _ => return command.to_string(),
    };
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut output: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if single
            .iter()
            .any(|flag| token == *flag || token.starts_with(&format!("{flag}=")))
        {
            index += 1;
            continue;
        }
        if paired.contains(&token) {
            index += if index + 1 < tokens.len() { 2 } else { 1 };
            continue;
        }
        if paired
            .iter()
            .any(|flag| token.starts_with(&format!("{flag}=")))
        {
            index += 1;
            continue;
        }
        output.push(token);
        index += 1;
    }
    output.join(" ")
}

fn here_doc(command: &str, prompt: &str) -> String {
    format!("{command} <<'PROMPT'\n{prompt}\nPROMPT")
}

/// `getSessionTitleGenerationCommandPreview(agent, { command })`.
pub(super) fn title_generation_preview(agent: &str, command: Option<&str>) -> String {
    let command = preview_command(agent, command);
    let read_only = one_shot_text_job_command(agent, &command);
    let prompt = settings_catalog().text(
        module::SETTINGS,
        "SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER",
    );
    let prompt = if prompt.is_empty() {
        "<title generation prompt>".to_string()
    } else {
        prompt
    };
    match agent {
        "codex" => here_doc(
            &format!(
                "{read_only} exec --sandbox read-only --ephemeral --skip-git-repo-check -m gpt-6-luna -c 'model_reasoning_effort=\"low\"'"
            ),
            &prompt,
        ),
        "cursor" => format!(
            "{read_only} --print --mode ask --trust --model cursor-grok-4.5-low --output-format text '{prompt}'"
        ),
        "claude" => here_doc(
            &format!(
                "{read_only} -p --permission-mode dontAsk --model claude-haiku-5-5 --effort low"
            ),
            &prompt,
        ),
        "grok" => format!(
            "{command} --model grok-4.5 --reasoning-effort low --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 --single '{prompt}'"
        ),
        "pi" => format!(
            "{command} -p --no-session --no-tools --no-context-files --thinking low '{prompt}'"
        ),
        "antigravity" => format!(
            "{command} -p '{prompt}' --output-format text --effort low --disable-slash-commands"
        ),
        "empryo" => here_doc(
            &format!(
                "{command} --headless --quiet --no-genome --marionette-mode none --max-steps 1"
            ),
            &prompt,
        ),
        _ => here_doc(&command, &prompt),
    }
}

/// One row of the `agentHookStatus` payload (`SidebarAgentHookStatusItem`).
#[derive(Clone, Debug)]
pub(super) struct HookStatusItem {
    pub(super) agent_id: String,
    pub(super) detail: String,
    pub(super) hook_installed: bool,
    /// `installed` | `missing` | `cliMissing` | `notRequired` | `updateRequired`.
    pub(super) status: String,
}

/// `SidebarAgentHookStatusMessage`.
#[derive(Clone, Debug, Default)]
pub(super) struct HookStatus {
    pub(super) agents: Vec<HookStatusItem>,
    pub(super) error_message: Option<String>,
    pub(super) hook_state_directory: String,
}

impl HookStatus {
    pub(super) fn parse(value: &Value) -> Self {
        Self {
            agents: value
                .get("agents")
                .and_then(Value::as_array)
                .map(|agents| {
                    agents
                        .iter()
                        .filter_map(|agent| {
                            Some(HookStatusItem {
                                agent_id: text(agent, "agentId")?,
                                detail: text(agent, "detail").unwrap_or_default(),
                                hook_installed: agent.get("hookInstalled").and_then(Value::as_bool)
                                    == Some(true),
                                status: text(agent, "status").unwrap_or_default(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            error_message: text(value, "errorMessage").filter(|message| !message.is_empty()),
            hook_state_directory: text(value, "hookStateDirectory").unwrap_or_default(),
        }
    }

    pub(super) fn item(&self, agent_id: &str) -> Option<&HookStatusItem> {
        self.agents.iter().find(|item| item.agent_id == agent_id)
    }
}

/// `hasRemovableAgentHookStatus`.
pub(super) fn hook_removable(status: Option<&HookStatusItem>) -> bool {
    status.is_some_and(|status| {
        status.hook_installed || status.status == "installed" || status.status == "updateRequired"
    })
}

/// `hasRemovableAgentHooks`.
pub(super) fn any_hook_removable(status: Option<&HookStatus>) -> bool {
    status.is_some_and(|status| {
        status.error_message.is_none()
            && status.agents.iter().any(|item| hook_removable(Some(item)))
    })
}

/// `mergeIds`: the draft order with ids that left dropped and new ones appended.
pub(super) fn merge_ids(draft: &[String], synced: &[String]) -> Vec<String> {
    let mut merged: Vec<String> = draft
        .iter()
        .filter(|id| synced.contains(id))
        .cloned()
        .collect();
    for id in synced {
        if !merged.contains(id) {
            merged.push(id.clone());
        }
    }
    merged
}

/// `reconcileDraftIds`: keep a draft order only while the synced order has not caught up.
pub(super) fn reconcile_draft_ids(
    draft: Option<&[String]>,
    synced: &[String],
) -> Option<Vec<String>> {
    let draft = draft?;
    let next = merge_ids(draft, synced);
    (next != synced).then_some(next)
}

fn base36(mut value: u128) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(std::char::from_digit((value % 36) as u32, 36).unwrap_or('0'));
        value /= 36;
        if value == 0 {
            break;
        }
    }
    digits.iter().rev().collect()
}

/// `createSettingsReorderRequestId('agents')`.
pub(super) fn reorder_request_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis = now.as_millis();
    let mut noise = (now.as_nanos() as u64) ^ 0x9e37_79b9_7f4a_7c15;
    let mut suffix = String::new();
    for _ in 0..6 {
        suffix.push(std::char::from_digit((noise % 36) as u32, 36).unwrap_or('0'));
        noise /= 36;
    }
    format!("settings-agents-{}-{suffix}", base36(millis))
}
