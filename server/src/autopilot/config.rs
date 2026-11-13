//! Autopilot's saved choices: the switch, which sessions it covers, what it may do, and the
//! projects and sessions taken out of it (or, in the narrower scopes, put into it). gxserver owns
//! them in one file so Settings, the chat pill, the sidebar menu, the CLI and the phone all read
//! and change the same thing.
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User, on the design page (Q1530 to Q1542): the mode is called "Autopilot" (Settings > Accounts keeps its own "Keep going at a limit" row), it stays on "until turned off", and "we need a setting to make it enabled on all sessions or have toggle per session. or enable it per project." It covers every thread and session, not only coordinators. Its default tier is Everything. The hard limits it may cross are each their own toggle, all off by default ("add all those as toggles in the page in settings for autopilot please"): push a branch, finish a release, push main, merge, delete outside the project, post publicly; sign-ins, 2FA, identity checks and payments are never done. Answering questions is "1 toggle under the autopilot, shouldn't be enabled by default", after a wait of 5, 10 or 15 minutes; answering approval cards is "yes if it's enabled in the page", off by default. The supervisor model is Opus 5.5 at medium or "sol 6.1 high" (Codex's GPT 6.1 Sol at high), and the sweep has a sensible default (2 minutes) with a slider.
//! SEE-ALSO: server/src/autopilot/runtime.rs (the watcher), apps/desktop/src/app/window/settings_modal/tabs/autopilot/ (the page), packages/settings-catalog/src/pages/ (its search rows).

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::GxserverPaths;

const CONFIG_FILE: &str = "autopilot.json";

/// The sweep's default and bounds, in seconds.
pub const SWEEP_DEFAULT_SECS: u64 = 120;
pub const SWEEP_MIN_SECS: u64 = 60;
pub const SWEEP_MAX_SECS: u64 = 600;
/// The waits a question may get before Autopilot picks its recommended choice, in minutes.
pub const ANSWER_WAITS: [u64; 3] = [5, 10, 15];

/// Which sessions the switch covers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// Every session, except projects and sessions turned off.
    #[default]
    All,
    /// Only projects turned on (and their worktrees), except sessions turned off.
    Projects,
    /// Only sessions turned on one by one.
    Sessions,
}

/// What Autopilot may do on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    /// Only what any human would do without thinking: carry on, interrupt, reload, switch account.
    Heal,
    /// Heal, plus answer what the agent asks with its recommended choice.
    Decide,
    /// Decide, plus approve any prompt that is not a hard limit.
    #[default]
    Everything,
}

/// The hard limits Autopilot may cross, each off until the user turns it on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HardLimits {
    pub push_branch: bool,
    pub finish_release: bool,
    pub push_main: bool,
    pub merge: bool,
    pub delete_outside_project: bool,
    pub post_publicly: bool,
}

/// The agent and model the judgment agent runs on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SupervisorModel {
    /// Claude Code, Opus 5.5 (`opus[1m]`) at medium.
    #[default]
    OpusMedium,
    /// Codex, GPT 6.1 Sol (`gpt-6.1-sol`) at high: the user's "sol 6.1 high".
    SolHigh,
}

impl SupervisorModel {
    /// The agent, model and effort the supervisor starts with.
    pub fn launch(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::OpusMedium => ("claude", "opus[1m]", "medium"),
            Self::SolHigh => ("codex", "gpt-6.1-sol", "high"),
        }
    }

    /// The other choice, tried when this one's agent has no account with room.
    pub fn other(self) -> Self {
        match self {
            Self::OpusMedium => Self::SolHigh,
            Self::SolHigh => Self::OpusMedium,
        }
    }
}

/// A project's or a session's own setting, against the scope's default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Membership {
    On,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutopilotConfig {
    pub enabled: bool,
    /// When the switch last went on (RFC 3339), for the summary.
    pub enabled_at: Option<String>,
    pub scope: Scope,
    pub tier: Tier,
    pub limits: HardLimits,
    pub answer_questions: bool,
    pub answer_after_minutes: u64,
    /// On unless the user turned it off ([`AutopilotConfig::answer_approvals_set`]).
    pub answer_approvals: bool,
    /// The user set "Answer approval cards" themselves; until then it is on.
    ///
    /// CDXC:Autopilot 2026-11-13 DECISION:
    /// User: "Make this enabled by default when Autopilot is enabled" for "Answer approval cards", which the 2026-11-07 decision had off by default; anyone who set the switch keeps their choice. A file written before this marker existed always held `answerApprovals: false`, whether or not the user touched it, so its `false` reads as never set (only `true` there was surely a choice). The hard limits keep their own switches, all off, and a read-only thread's cards are never approved (server/src/server/autopilot_answers.rs).
    pub answer_approvals_set: bool,
    pub supervisor: SupervisorModel,
    pub sweep_seconds: u64,
    /// By project id; a worktree follows its main project.
    pub projects: BTreeMap<String, Membership>,
    /// By `<projectId>:<sessionId>`.
    pub sessions: BTreeMap<String, Membership>,
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_at: None,
            scope: Scope::All,
            tier: Tier::Everything,
            limits: HardLimits::default(),
            answer_questions: false,
            answer_after_minutes: 10,
            answer_approvals: true,
            answer_approvals_set: false,
            supervisor: SupervisorModel::OpusMedium,
            sweep_seconds: SWEEP_DEFAULT_SECS,
            projects: BTreeMap::new(),
            sessions: BTreeMap::new(),
        }
    }
}

pub fn session_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
}

impl AutopilotConfig {
    fn normalized(mut self) -> Self {
        self.sweep_seconds = self.sweep_seconds.clamp(SWEEP_MIN_SECS, SWEEP_MAX_SECS);
        if !ANSWER_WAITS.contains(&self.answer_after_minutes) {
            self.answer_after_minutes = 10;
        }
        if !self.answer_approvals_set {
            self.answer_approvals = true;
        }
        self
    }

    /// Whether Autopilot covers a session, given its project and, for a worktree, the project it
    /// was made from. Off when the switch is off.
    pub fn covers(
        &self,
        project_id: &str,
        main_project_id: Option<&str>,
        session_id: &str,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        let session = self
            .sessions
            .get(&session_key(project_id, session_id))
            .copied();
        if let Some(own) = session {
            return own == Membership::On;
        }
        let project = self
            .projects
            .get(project_id)
            .or_else(|| main_project_id.and_then(|main| self.projects.get(main)))
            .copied();
        match self.scope {
            Scope::All => project != Some(Membership::Off),
            Scope::Projects => project == Some(Membership::On),
            Scope::Sessions => false,
        }
    }

    /// The project's own setting, or its main project's for a worktree: `true` when it is in.
    pub fn project_in(&self, project_id: &str, main_project_id: Option<&str>) -> bool {
        let project = self
            .projects
            .get(project_id)
            .or_else(|| main_project_id.and_then(|main| self.projects.get(main)))
            .copied();
        match self.scope {
            Scope::All => project != Some(Membership::Off),
            Scope::Projects => project == Some(Membership::On),
            Scope::Sessions => false,
        }
    }

    /// Puts a session in or takes it out. A choice equal to what its project and the scope give
    /// it anyway is forgotten, so the session follows its project again.
    pub fn set_session(
        &mut self,
        project_id: &str,
        main_project_id: Option<&str>,
        session_id: &str,
        on: bool,
    ) {
        let key = session_key(project_id, session_id);
        if self.project_in(project_id, main_project_id) == on && self.scope != Scope::Sessions {
            self.sessions.remove(&key);
        } else if !on && self.scope == Scope::Sessions {
            self.sessions.remove(&key);
        } else {
            self.sessions
                .insert(key, if on { Membership::On } else { Membership::Off });
        }
    }

    /// Puts a project (and its worktrees) in or takes it out; its sessions follow it again.
    pub fn set_project(&mut self, project_id: &str, on: bool) {
        let default_in = self.scope == Scope::All;
        if on == default_in {
            self.projects.remove(project_id);
        } else {
            self.projects.insert(
                project_id.to_string(),
                if on { Membership::On } else { Membership::Off },
            );
        }
        let prefix = format!("{project_id}:");
        self.sessions.retain(|key, _| !key.starts_with(&prefix));
    }
}

/// The choices this process last read or wrote, else the defaults; never a file on disk.
#[cfg(test)]
pub(super) fn cached_or_default() -> AutopilotConfig {
    cached()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| AutopilotConfig::default().normalized())
}

fn cached() -> &'static Mutex<Option<AutopilotConfig>> {
    static CONFIG: Mutex<Option<AutopilotConfig>> = Mutex::new(None);
    &CONFIG
}

/// The saved choices, read from disk once per process.
pub fn read_config(paths: &GxserverPaths) -> AutopilotConfig {
    let mut cache = cached().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(config) = cache.as_ref() {
        return config.clone();
    }
    let config = std::fs::read_to_string(paths.app_state_dir.join(CONFIG_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<AutopilotConfig>(&text).ok())
        .unwrap_or_default()
        .normalized();
    *cache = Some(config.clone());
    config
}

/// Changes the saved choices under one lock and writes them; returns the new choices.
pub fn update_config(
    paths: &GxserverPaths,
    change: impl FnOnce(&mut AutopilotConfig),
) -> std::io::Result<AutopilotConfig> {
    let current = read_config(paths);
    let mut cache = cached().lock().unwrap_or_else(PoisonError::into_inner);
    let mut next = cache.clone().unwrap_or(current);
    let was_enabled = next.enabled;
    change(&mut next);
    let mut next = next.normalized();
    if next.enabled && !was_enabled {
        next.enabled_at = Some(chrono::Utc::now().to_rfc3339());
    }
    std::fs::create_dir_all(&paths.app_state_dir)?;
    let text = serde_json::to_string_pretty(&next).unwrap_or_default();
    let target = paths.app_state_dir.join(CONFIG_FILE);
    let temporary = paths.app_state_dir.join(format!("{CONFIG_FILE}.tmp"));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &target)?;
    *cache = Some(next.clone());
    Ok(next)
}

/// Applies a partial JSON object of choices (the Settings page's and the CLI's edits).
pub fn apply_patch(config: &mut AutopilotConfig, patch: &Value) {
    let Some(patch) = patch.as_object() else {
        return;
    };
    let mut merged = serde_json::to_value(&*config).unwrap_or_default();
    for (key, value) in patch {
        if matches!(
            key.as_str(),
            "projects" | "sessions" | "enabledAt" | "answerApprovalsSet"
        ) {
            continue;
        }
        if key == "limits" {
            if let (Some(limits), Some(changes)) = (
                merged.get_mut("limits").and_then(Value::as_object_mut),
                value.as_object(),
            ) {
                for (limit, on) in changes {
                    if limits.contains_key(limit) {
                        limits.insert(limit.clone(), on.clone());
                    }
                }
            }
            continue;
        }
        if let Some(slot) = merged.get_mut(key) {
            *slot = value.clone();
        }
    }
    if let Ok(mut next) = serde_json::from_value::<AutopilotConfig>(merged) {
        if patch.get("answerApprovals").is_some_and(Value::is_boolean) {
            next.answer_approvals_set = true;
        }
        *config = next;
    }
}
