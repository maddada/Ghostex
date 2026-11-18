//! The automation state the Automate view shows, parsed from the `automationState` object the
//! gxserver automation endpoints return (the `ProjectAutomationsBridgeState` shape of the deleted
//! packages/shared/automations.ts, in git history). Entries that do not parse are dropped one by
//! one, the way those TypeScript normalizers dropped them, instead of failing the whole state.

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationAgentOption {
    pub(crate) agent_id: String,
    #[serde(default)]
    pub(crate) icon: Option<String>,
    #[serde(default)]
    pub(crate) label: String,
}

impl AutomationAgentOption {
    pub(crate) fn display_label(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.agent_id
        } else {
            &self.label
        }
    }

    /// `resolveAutomationAgentIcon`: the option's own icon, else the sidebar icon for its id.
    pub(crate) fn icon_id(&self) -> Option<&'static str> {
        crate::app::helpers::gpui_sidebar_agent_icon(self.icon.as_deref())
            .or_else(|| crate::app::helpers::gpui_sidebar_agent_icon(Some(&self.agent_id)))
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationTargetProject {
    #[serde(default)]
    pub(crate) can_use_worktrees: bool,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) path: String,
    pub(crate) project_id: String,
    #[serde(default)]
    pub(crate) worktree_unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum AutomationSchedule {
    Once {
        #[serde(rename = "runAt")]
        run_at: String,
    },
    Interval {
        #[serde(rename = "everyMs")]
        every_ms: f64,
    },
    Daily {
        time: String,
        #[serde(default)]
        timezone: String,
    },
    Weekly {
        days: Vec<u32>,
        time: String,
        #[serde(default)]
        timezone: String,
    },
    Cron {
        expression: String,
        #[serde(default)]
        timezone: String,
    },
}

impl AutomationSchedule {
    pub(crate) fn to_json(&self) -> Value {
        match self {
            Self::Once { run_at } => serde_json::json!({ "kind": "once", "runAt": run_at }),
            Self::Interval { every_ms } => {
                serde_json::json!({ "kind": "interval", "everyMs": every_ms.round() as i64 })
            }
            Self::Daily { time, timezone } => {
                serde_json::json!({ "kind": "daily", "time": time, "timezone": timezone })
            }
            Self::Weekly {
                days,
                time,
                timezone,
            } => serde_json::json!({
                "kind": "weekly", "days": days, "time": time, "timezone": timezone,
            }),
            Self::Cron {
                expression,
                timezone,
            } => serde_json::json!({
                "kind": "cron", "expression": expression, "timezone": timezone,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum AutomationExecutionMode {
    Local,
    Worktree {
        #[serde(default, rename = "setupCommand")]
        setup_command: Option<String>,
    },
    Thread {
        #[serde(default, rename = "agentSessionId")]
        agent_session_id: Option<String>,
        #[serde(default, rename = "expiresAt")]
        expires_at: Option<String>,
        #[serde(default, rename = "sessionId")]
        session_id: Option<String>,
    },
}

impl AutomationExecutionMode {
    pub(crate) fn to_json(&self) -> Value {
        let mut object = serde_json::Map::new();
        let mut put = |key: &str, value: &Option<String>| {
            if let Some(value) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                object.insert(key.to_string(), Value::String(value.to_string()));
            }
        };
        let kind = match self {
            Self::Local => "local",
            Self::Worktree { setup_command } => {
                put("setupCommand", setup_command);
                "worktree"
            }
            Self::Thread {
                agent_session_id,
                expires_at,
                session_id,
            } => {
                put("agentSessionId", agent_session_id);
                put("expiresAt", expires_at);
                put("sessionId", session_id);
                "thread"
            }
        };
        object.insert("kind".to_string(), Value::String(kind.to_string()));
        Value::Object(object)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationDefinition {
    pub(crate) agent_id: String,
    #[serde(default)]
    pub(crate) created_at: Option<String>,
    #[serde(default)]
    pub(crate) enabled: bool,
    pub(crate) execution_mode: AutomationExecutionMode,
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) next_run_at: Option<String>,
    #[serde(default)]
    pub(crate) project_ids: Vec<String>,
    #[serde(default)]
    pub(crate) prompt: String,
    pub(crate) schedule: AutomationSchedule,
    /// The model each run's new session starts on; `None` keeps the agent's own.
    #[serde(default)]
    pub(crate) model: Option<String>,
    /// The effort each run's new session starts on; `None` keeps the agent's own.
    #[serde(default)]
    pub(crate) effort: Option<String>,
}

impl AutomationDefinition {
    pub(crate) fn project_id(&self) -> Option<&str> {
        self.project_ids
            .first()
            .map(String::as_str)
            .filter(|id| !id.trim().is_empty())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationRunWorktree {
    #[serde(default)]
    pub(crate) branch: String,
    #[serde(default)]
    pub(crate) path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationRun {
    pub(crate) automation_id: String,
    #[serde(default)]
    pub(crate) completed_at: Option<String>,
    #[serde(default)]
    pub(crate) created_at: String,
    #[serde(default)]
    pub(crate) error_message: Option<String>,
    #[serde(default)]
    pub(crate) findings_summary: Option<String>,
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) is_archived: bool,
    #[serde(default)]
    pub(crate) is_unread: bool,
    #[serde(default)]
    pub(crate) project_id: String,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) worktree: Option<AutomationRunWorktree>,
}

impl AutomationRun {
    /// `isAutomationRunActive`.
    pub(crate) fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running")
    }

    pub(crate) fn summary(&self) -> &str {
        [&self.findings_summary, &self.error_message]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .find(|text| !text.trim().is_empty())
            .unwrap_or("Run is waiting for agent output.")
    }

    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
    }

    pub(crate) fn worktree(&self) -> Option<&AutomationRunWorktree> {
        self.worktree
            .as_ref()
            .filter(|worktree| !worktree.path.trim().is_empty())
    }

    fn sort_time(&self) -> i64 {
        super::drafts::parse_iso_millis(self.completed_at.as_deref().unwrap_or(&self.created_at))
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AutomationState {
    pub(crate) agents: Vec<AutomationAgentOption>,
    pub(crate) automations: Vec<AutomationDefinition>,
    pub(crate) default_agent_id: Option<String>,
    pub(crate) project_can_use_worktrees: bool,
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) project_path: String,
    pub(crate) projects: Vec<AutomationTargetProject>,
    /// Newest first (`compareAutomationRunsNewestFirst`).
    pub(crate) runs: Vec<AutomationRun>,
    pub(crate) worktree_unavailable_reason: Option<String>,
}

fn parse_list<T: serde::de::DeserializeOwned>(value: &Value, key: &str) -> Vec<T> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
        .collect()
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

impl AutomationState {
    pub(crate) fn from_json(value: &Value) -> Self {
        let mut runs: Vec<AutomationRun> = parse_list(value, "runs");
        runs.sort_by_key(|run| std::cmp::Reverse(run.sort_time()));
        Self {
            agents: parse_list(value, "agents"),
            automations: parse_list(value, "automations"),
            default_agent_id: text(value, "defaultAgentId"),
            project_can_use_worktrees: value
                .get("projectCanUseWorktrees")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            project_id: text(value, "projectId").unwrap_or_default(),
            project_name: text(value, "projectName").unwrap_or_default(),
            project_path: text(value, "projectPath").unwrap_or_default(),
            projects: parse_list(value, "projects"),
            runs,
            worktree_unavailable_reason: text(value, "worktreeUnavailableReason"),
        }
    }

    pub(crate) fn agent(&self, agent_id: &str) -> Option<&AutomationAgentOption> {
        self.agents.iter().find(|agent| agent.agent_id == agent_id)
    }

    pub(crate) fn project(&self, project_id: &str) -> Option<&AutomationTargetProject> {
        self.projects
            .iter()
            .find(|project| project.project_id == project_id)
    }

    pub(crate) fn visible_runs(&self) -> impl Iterator<Item = &AutomationRun> {
        self.runs.iter().filter(|run| !run.is_archived)
    }

    /// `selectAutomationRunsForTriage`: every actionable run, plus the most recent completed ones,
    /// ordered unread first, then by status weight, then newest first.
    pub(crate) fn triage_runs(&self) -> Vec<&AutomationRun> {
        let visible: Vec<&AutomationRun> = self.visible_runs().collect();
        let recent_completed = visible
            .iter()
            .filter(|run| run.completed_at.is_some() && !run.is_active())
            .take(TRIAGE_RECENT_COMPLETED_LIMIT)
            .map(|run| run.id.as_str())
            .collect::<Vec<_>>();
        let mut selected: Vec<&AutomationRun> = visible
            .iter()
            .copied()
            .filter(|run| triage_actionable(run) || recent_completed.contains(&run.id.as_str()))
            .collect();
        selected.sort_by(|left, right| {
            right
                .is_unread
                .cmp(&left.is_unread)
                .then_with(|| triage_weight(right).cmp(&triage_weight(left)))
                .then_with(|| right.sort_time().cmp(&left.sort_time()))
        });
        selected
    }
}

/// `PROJECT_AUTOMATION_TRIAGE_RECENT_COMPLETED_LIMIT`.
const TRIAGE_RECENT_COMPLETED_LIMIT: usize = 5;

fn triage_actionable(run: &AutomationRun) -> bool {
    run.is_unread
        || matches!(
            run.status.as_str(),
            "findings" | "needs_attention" | "failed"
        )
}

fn triage_weight(run: &AutomationRun) -> u8 {
    match run.status.as_str() {
        "needs_attention" | "failed" => 3,
        "findings" => 2,
        _ => 1,
    }
}

/// One agent session the Thread execution mode can target (`ProjectBoardSessionOption`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationSessionOption {
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
    #[serde(default)]
    pub(crate) agent_session_id: Option<String>,
    #[serde(default)]
    pub(crate) label: String,
    pub(crate) session_id: String,
}

pub(crate) fn parse_session_options(payload: &Value) -> Vec<AutomationSessionOption> {
    parse_list(payload, "sessions")
}
