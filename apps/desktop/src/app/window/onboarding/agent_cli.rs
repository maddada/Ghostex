//! The agent CLI install rows: one gxserver `/api/agentCliMaintenance` job lifecycle per row
//! (read, poll every 1.5s while a job runs, install through the first runnable method, add to
//! PATH), and the events a row narrates to the scan log. Port of packages/core-ui/agent-cli/use-agent-cli-job.ts (deleted 2026-10-01)
//! and packages/core-ui/onboarding/agent-install.ts (deleted 2026-10-01).
//!
//! CDXC:Onboarding 2026-09-15 WHY:
//! The scan log must only ever print real job state, so every event is derived from the polled
//! CLI state and only jobs a row saw running (started here, or found running on the server) are
//! narrated: a job that already finished before the panel opened still shows its failure inline
//! under the row, but it is not narrated as if it just happened.
use super::{GpuiOnboardingWindow, OnboardingCommand};
use gpui::Context;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How often a running install job is re-read from gxserver.
const POLL: Duration = Duration::from_millis(1500);
const LOG_LINE_MAX: usize = 72;

#[derive(Clone, Debug)]
pub(crate) enum AgentCliRequest {
    Read,
    Start { method_id: String },
    AddToPath,
}

#[derive(Clone, Debug)]
pub(crate) struct CliMethod {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) command: String,
    pub(crate) unavailable_reason: Option<String>,
    /// What one click does, including anything Ghostex installs first (absent from older gxservers).
    pub(crate) plan: Option<String>,
    /// "node", "homebrew", "mise" or "systemTools": a tool Ghostex installs before `command`.
    pub(crate) prerequisite: Option<String>,
    pub(crate) system_tools: Vec<String>,
}

impl CliMethod {
    /// `agentCliMethodTooltip` (packages/shared/agent-cli-maintenance.ts (deleted 2026-10-01)).
    pub(crate) fn tooltip(&self) -> String {
        self.unavailable_reason
            .clone()
            .or_else(|| self.plan.clone())
            .unwrap_or_else(|| self.command.clone())
    }

    /// `agentCliMethodLabel`: the method's name with its prerequisite note.
    pub(crate) fn display_label(&self) -> String {
        let suffix = match self.prerequisite.as_deref() {
            Some("node") => Some("installs Node.js first".to_string()),
            Some("homebrew") => Some("installs Homebrew first".to_string()),
            Some("mise") => Some("installs mise first".to_string()),
            Some("systemTools") => {
                let tools: Vec<&str> = if self.system_tools.is_empty() {
                    vec!["curl"]
                } else {
                    self.system_tools
                        .iter()
                        .map(|tool| {
                            if tool == "ca-certificates" {
                                "certificates"
                            } else {
                                tool.as_str()
                            }
                        })
                        .collect()
                };
                Some(format!("installs {} first", tools.join(", ")))
            }
            _ => None,
        };
        match suffix {
            Some(suffix) => format!("{}, {suffix}", self.label),
            None => self.label.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CliJob {
    pub(crate) id: String,
    pub(crate) command: String,
    pub(crate) status: String,
    pub(crate) output: String,
    pub(crate) error: Option<String>,
}

impl CliJob {
    fn active(&self) -> bool {
        self.status == "queued" || self.status == "running"
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CliState {
    pub(crate) executable_path: Option<String>,
    pub(crate) path_directory: Option<String>,
    pub(crate) methods: Vec<CliMethod>,
    pub(crate) job: Option<CliJob>,
}

impl CliState {
    fn parse(value: &Value) -> Self {
        let text = |value: &Value, key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        Self {
            executable_path: text(value, "executablePath"),
            path_directory: text(value, "pathDirectory"),
            methods: value
                .get("methods")
                .and_then(Value::as_array)
                .map(|methods| {
                    methods
                        .iter()
                        .filter_map(|method| {
                            Some(CliMethod {
                                id: text(method, "id")?,
                                label: text(method, "label").unwrap_or_default(),
                                command: text(method, "command").unwrap_or_default(),
                                unavailable_reason: text(method, "unavailableReason"),
                                plan: text(method, "plan"),
                                prerequisite: text(method, "prerequisite"),
                                system_tools: method
                                    .get("systemTools")
                                    .and_then(Value::as_array)
                                    .map(|tools| {
                                        tools
                                            .iter()
                                            .filter_map(|tool| tool.as_str().map(str::to_string))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            job: value.get("job").and_then(|job| {
                Some(CliJob {
                    id: text(job, "id")?,
                    command: text(job, "command").unwrap_or_default(),
                    status: text(job, "status").unwrap_or_default(),
                    output: job
                        .get("output")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    error: text(job, "error"),
                })
            }),
        }
    }

    /// `defaultAgentCliInstallMethod`: the first method the computer can run, else the first listed
    /// (so its reason can show); none once the CLI is installed.
    pub(crate) fn default_method(&self) -> Option<&CliMethod> {
        if self.executable_path.is_some() {
            return None;
        }
        self.methods
            .iter()
            .find(|method| method.unavailable_reason.is_none())
            .or_else(|| self.methods.first())
    }
}

/// `output` without ANSI CSI sequences (colours, cursor moves).
pub(crate) fn strip_ansi(output: &str) -> String {
    let mut plain = String::with_capacity(output.len());
    let mut chars = output.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            // Parameter and intermediate bytes, then one final byte.
            while let Some(&next) = chars.peek() {
                chars.next();
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }
        plain.push(ch);
    }
    plain
}

/// `lastOutputLine`: the last non-empty line of a job's output without ANSI codes, short enough for one scan-log row.
pub(crate) fn last_output_line(output: &str) -> Option<String> {
    strip_ansi(output)
        .split(['\n', '\r'])
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
        .map(|line| {
            if line.chars().count() > LOG_LINE_MAX {
                let mut short: String = line.chars().take(LOG_LINE_MAX - 1).collect();
                short.push('…');
                short
            } else {
                line.to_string()
            }
        })
}

/// What an install row tells the Agents panel so the scan log can print real job events.
#[derive(Clone, Debug)]
pub(crate) enum InstallEvent {
    Started {
        agent_id: String,
        name: String,
        method_label: String,
    },
    Output {
        agent_id: String,
        line: String,
    },
    Succeeded {
        agent_id: String,
        name: String,
    },
    Failed {
        agent_id: String,
        name: String,
        error: String,
    },
}

pub(crate) struct CliRow {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    /// Guide rows read lazily (on Install); panel rows read as soon as they mount.
    lazy: bool,
    pub(crate) state: Option<CliState>,
    loading: bool,
    pub(crate) action_error: Option<String>,
    starting: bool,
    poll_at: Option<Instant>,
    tracked: HashSet<String>,
    reported_job: Option<String>,
    reported_line: Option<String>,
    reported_ended: Option<String>,
    reported_action_error: Option<String>,
    /// The Install guide shows this row's failed install output under it.
    pub(crate) show_output: bool,
}

/// What the Install guide's button shows for one agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuideRowStatus {
    Install,
    /// Waiting behind another install (gxserver runs them one at a time).
    Queued,
    Installing,
    Installed,
    /// The last install failed: Retry, with its output under Details.
    Failed,
}

/// CDXC:Onboarding 2026-11-03 WHY:
/// A guide row said Installed only from its own CLI state, and the list dropped every agent the scan
/// had found, so a finished install either vanished from the list when the rescan landed or, when the
/// rescan missed it, read "Install" again once the popup was reopened. A row is Installed when either
/// its own fresh state or the scan says so, and running or installed rows outlive the popup.
pub(crate) fn guide_row_status(row: Option<&CliRow>, scan_installed: bool) -> GuideRowStatus {
    match row {
        Some(row) if row.queued() => GuideRowStatus::Queued,
        Some(row) if row.running() => GuideRowStatus::Installing,
        Some(row) if row.installed() => GuideRowStatus::Installed,
        _ if scan_installed => GuideRowStatus::Installed,
        Some(row) if row.error().is_some() => GuideRowStatus::Failed,
        _ => GuideRowStatus::Install,
    }
}

/// Whether the Install guide lists an agent: not one the scan found before the row was shown,
/// but a row already on screen stays (as Installed) when the scan finds it.
pub(crate) fn guide_lists_agent(scan_installed: bool, has_row: bool) -> bool {
    !scan_installed || has_row
}

impl CliRow {
    fn job(&self) -> Option<&CliJob> {
        self.state.as_ref().and_then(|state| state.job.as_ref())
    }

    pub(crate) fn method(&self) -> Option<&CliMethod> {
        self.state.as_ref().and_then(CliState::default_method)
    }

    pub(crate) fn checking(&self) -> bool {
        self.loading && self.state.is_none()
    }

    pub(crate) fn running(&self) -> bool {
        self.starting || self.job().is_some_and(CliJob::active)
    }

    pub(crate) fn queued(&self) -> bool {
        self.job().is_some_and(|job| job.status == "queued")
    }

    pub(crate) fn installed(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| state.executable_path.is_some())
    }

    /// The job's full output (ANSI codes and all), when it printed anything.
    pub(crate) fn job_output(&self) -> Option<&str> {
        self.job()
            .map(|job| job.output.as_str())
            .filter(|output| !output.trim().is_empty())
    }

    /// The running job's latest output line, for the row while it installs.
    pub(crate) fn progress_line(&self) -> Option<String> {
        self.job()
            .filter(|job| job.active())
            .and_then(|job| last_output_line(&job.output))
    }

    pub(crate) fn path_directory(&self) -> Option<&str> {
        self.state
            .as_ref()
            .and_then(|state| state.path_directory.as_deref())
    }

    /// The failure shown under the row: a start that was refused, or the job's own error / last line.
    pub(crate) fn error(&self) -> Option<String> {
        if let Some(error) = &self.action_error {
            return Some(error.clone());
        }
        let job = self.job()?;
        (job.status == "failed").then(|| {
            job.error
                .clone()
                .or_else(|| last_output_line(&job.output))
                .unwrap_or_else(|| "The install failed.".to_string())
        })
    }

    /// The events this state change narrates (`useAgentInstallRow`'s effects).
    fn narrate(&mut self) -> Vec<InstallEvent> {
        let mut events = Vec::new();
        let Some(state) = self.state.clone() else {
            return self.narrate_action_error(events);
        };
        if let Some(job) = &state.job {
            if job.active() {
                self.tracked.insert(job.id.clone());
            }
            if self.tracked.contains(&job.id) {
                if self.reported_job.as_deref() != Some(job.id.as_str()) {
                    self.reported_job = Some(job.id.clone());
                    self.reported_line = None;
                    self.reported_ended = None;
                    let method_label = state
                        .methods
                        .iter()
                        .find(|method| method.command == job.command)
                        .map(|method| method.label.clone())
                        .unwrap_or_else(|| "its installer".to_string());
                    events.push(InstallEvent::Started {
                        agent_id: self.agent_id.clone(),
                        name: self.name.clone(),
                        method_label,
                    });
                }
                let line = last_output_line(&job.output);
                if let Some(line) = &line
                    && self.reported_line.as_deref() != Some(line.as_str())
                {
                    self.reported_line = Some(line.clone());
                    events.push(InstallEvent::Output {
                        agent_id: self.agent_id.clone(),
                        line: line.clone(),
                    });
                }
                if job.status == "failed" && self.reported_ended.as_deref() != Some(job.id.as_str())
                {
                    self.reported_ended = Some(job.id.clone());
                    events.push(InstallEvent::Failed {
                        agent_id: self.agent_id.clone(),
                        name: self.name.clone(),
                        error: job
                            .error
                            .clone()
                            .or(line)
                            .unwrap_or_else(|| "The install failed.".to_string()),
                    });
                }
                if job.status == "succeeded"
                    && self.reported_ended.as_deref() != Some(job.id.as_str())
                {
                    self.reported_ended = Some(job.id.clone());
                    events.push(InstallEvent::Succeeded {
                        agent_id: self.agent_id.clone(),
                        name: self.name.clone(),
                    });
                }
            }
        }
        self.narrate_action_error(events)
    }

    fn narrate_action_error(&mut self, mut events: Vec<InstallEvent>) -> Vec<InstallEvent> {
        if let Some(error) = &self.action_error
            && self.reported_action_error.as_ref() != Some(error)
        {
            self.reported_action_error = Some(error.clone());
            events.push(InstallEvent::Failed {
                agent_id: self.agent_id.clone(),
                name: self.name.clone(),
                error: error.clone(),
            });
        }
        events
    }
}

#[derive(Clone, Copy, Debug)]
enum Pending {
    Read,
    InstallRead,
    InstallStart,
    AddToPath,
}

#[derive(Default)]
pub(crate) struct CliRows {
    rows: HashMap<String, CliRow>,
    pending: HashMap<u64, (String, Pending)>,
}

impl CliRows {
    /// Drops every row; answers still in flight are ignored when they land.
    pub(crate) fn clear(&mut self) {
        self.rows.clear();
        self.pending.clear();
    }

    /// Drops the Install guide's rows (the popup closed), except installs still running or done, so
    /// reopening it shows them as Installing / Installed instead of "Install".
    pub(crate) fn clear_lazy(&mut self) {
        self.rows
            .retain(|_, row| !row.lazy || row.running() || row.installed());
        let rows = &self.rows;
        self.pending.retain(|_, (key, _)| rows.contains_key(key));
    }

    pub(crate) fn get(&self, key: &str) -> Option<&CliRow> {
        self.rows.get(key)
    }

    pub(crate) fn toggle_output(&mut self, key: &str) {
        if let Some(row) = self.rows.get_mut(key) {
            row.show_output = !row.show_output;
        }
    }
}

impl GpuiOnboardingWindow {
    /// Mounts an install row (`useAgentInstallRow`); an eager row reads its state right away.
    pub(super) fn ensure_cli_row(
        &mut self,
        key: &str,
        agent_id: &str,
        name: &str,
        lazy: bool,
        cx: &mut Context<Self>,
    ) {
        if self.cli_rows.rows.contains_key(key) {
            return;
        }
        self.cli_rows.rows.insert(
            key.to_string(),
            CliRow {
                agent_id: agent_id.to_string(),
                name: name.to_string(),
                lazy,
                state: None,
                loading: false,
                action_error: None,
                starting: false,
                poll_at: None,
                tracked: HashSet::new(),
                reported_job: None,
                reported_line: None,
                reported_ended: None,
                reported_action_error: None,
                show_output: false,
            },
        );
        if !lazy && self.cli_available {
            self.cli_read(key, cx);
        }
    }

    fn cli_send(
        &mut self,
        key: &str,
        pending: Pending,
        request: AgentCliRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.cli_rows.rows.get(key) else {
            return;
        };
        let agent_id = row.agent_id.clone();
        let request_id = self.next_request_id();
        self.cli_rows
            .pending
            .insert(request_id, (key.to_string(), pending));
        self.send(
            OnboardingCommand::AgentCli {
                request_id,
                agent_id,
                request,
            },
            cx,
        );
    }

    fn cli_read(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(row) = self.cli_rows.rows.get_mut(key) {
            row.loading = true;
            row.poll_at = None;
        }
        self.cli_send(key, Pending::Read, AgentCliRequest::Read, cx);
    }

    /// `install()`: read the state when it is not loaded, then start the default method.
    pub(super) fn cli_install(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.cli_available {
            return;
        }
        let Some(row) = self.cli_rows.rows.get_mut(key) else {
            return;
        };
        if row.running() {
            return;
        }
        row.starting = true;
        row.action_error = None;
        row.show_output = false;
        match row.state.clone() {
            None => self.cli_send(key, Pending::InstallRead, AgentCliRequest::Read, cx),
            Some(state) => self.cli_start_with(key, &state, cx),
        }
    }

    fn cli_start_with(&mut self, key: &str, state: &CliState, cx: &mut Context<Self>) {
        if state.executable_path.is_some() {
            if let Some(row) = self.cli_rows.rows.get_mut(key) {
                row.starting = false;
            }
            self.cli_read(key, cx);
            return;
        }
        let method = match state.default_method() {
            None => Err(
                "No install method is available on this computer. Follow the install docs."
                    .to_string(),
            ),
            Some(method) => match &method.unavailable_reason {
                Some(reason) => Err(reason.clone()),
                None => Ok(method.id.clone()),
            },
        };
        match method {
            Ok(method_id) => {
                self.cli_send(
                    key,
                    Pending::InstallStart,
                    AgentCliRequest::Start { method_id },
                    cx,
                );
            }
            Err(error) => {
                if let Some(row) = self.cli_rows.rows.get_mut(key) {
                    row.action_error = Some(error);
                    row.starting = false;
                }
                self.cli_after_change(key, cx);
                self.cli_read(key, cx);
            }
        }
    }

    /// `addToPath()`, then a rescan, as the row's button does.
    pub(super) fn cli_add_to_path(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.cli_available {
            return;
        }
        let Some(row) = self.cli_rows.rows.get_mut(key) else {
            return;
        };
        row.starting = true;
        row.action_error = None;
        self.cli_send(key, Pending::AddToPath, AgentCliRequest::AddToPath, cx);
    }

    pub(super) fn handle_cli_result(
        &mut self,
        request_id: u64,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let Some((key, pending)) = self.cli_rows.pending.remove(&request_id) else {
            return;
        };
        if !self.cli_rows.rows.contains_key(&key) {
            return;
        }
        let now = Instant::now();
        match pending {
            Pending::Read => {
                if let Some(row) = self.cli_rows.rows.get_mut(&key) {
                    row.loading = false;
                    if let Ok(value) = &result {
                        row.state = Some(CliState::parse(value));
                    }
                    if row.job().is_some_and(CliJob::active) {
                        row.poll_at = Some(now + POLL);
                    }
                }
                self.cli_after_change(&key, cx);
            }
            Pending::InstallRead => match result {
                Ok(value) => {
                    let state = CliState::parse(&value);
                    if let Some(row) = self.cli_rows.rows.get_mut(&key) {
                        row.state = Some(state.clone());
                    }
                    self.cli_after_change(&key, cx);
                    self.cli_start_with(&key, &state, cx);
                }
                Err(error) => {
                    if let Some(row) = self.cli_rows.rows.get_mut(&key) {
                        row.action_error = Some(error);
                        row.starting = false;
                    }
                    self.cli_after_change(&key, cx);
                    self.cli_read(&key, cx);
                }
            },
            Pending::InstallStart => {
                if let Some(row) = self.cli_rows.rows.get_mut(&key) {
                    row.starting = false;
                    match &result {
                        Ok(value) => row.state = Some(CliState::parse(value)),
                        // A timed-out start can still have started the server-owned job; the read below finds it.
                        Err(error) => row.action_error = Some(error.clone()),
                    }
                }
                self.cli_after_change(&key, cx);
                self.cli_read(&key, cx);
            }
            Pending::AddToPath => {
                if let Some(row) = self.cli_rows.rows.get_mut(&key) {
                    row.starting = false;
                    match &result {
                        Ok(value) => row.state = Some(CliState::parse(value)),
                        Err(error) => row.action_error = Some(error.clone()),
                    }
                }
                self.cli_after_change(&key, cx);
                self.rescan_agents(cx);
            }
        }
    }

    fn cli_after_change(&mut self, key: &str, cx: &mut Context<Self>) {
        let events = self
            .cli_rows
            .rows
            .get_mut(key)
            .map(CliRow::narrate)
            .unwrap_or_default();
        for event in events {
            self.on_install_event(event, cx);
        }
    }

    pub(super) fn tick_cli_rows(&mut self, now: Instant, cx: &mut Context<Self>) {
        let due: Vec<String> = self
            .cli_rows
            .rows
            .iter()
            .filter(|(_, row)| row.poll_at.is_some_and(|at| at <= now))
            .map(|(key, _)| key.clone())
            .collect();
        for key in due {
            self.cli_read(&key, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(lazy: bool, state: Option<Value>) -> CliRow {
        CliRow {
            agent_id: "qoder".into(),
            name: "Qoder".into(),
            lazy,
            state: state.as_ref().map(CliState::parse),
            loading: false,
            action_error: None,
            starting: false,
            poll_at: None,
            tracked: HashSet::new(),
            reported_job: None,
            reported_line: None,
            reported_ended: None,
            reported_action_error: None,
            show_output: false,
        }
    }

    fn job(status: &str, output: &str) -> Value {
        json!({"agentId": "qoder", "methods": [{"id": "npm", "label": "npm", "command": "npm i -g x"}],
            "job": {"id": "j1", "command": "npm i -g x", "status": status, "output": output}})
    }

    #[test]
    fn a_guide_row_goes_from_install_through_installing_to_installed() {
        assert_eq!(guide_row_status(None, false), GuideRowStatus::Install);
        let mut cli = row(true, None);
        assert_eq!(guide_row_status(Some(&cli), false), GuideRowStatus::Install);
        cli.starting = true;
        assert_eq!(
            guide_row_status(Some(&cli), false),
            GuideRowStatus::Installing
        );
        cli.starting = false;
        cli.state = Some(CliState::parse(&job("queued", "")));
        assert_eq!(guide_row_status(Some(&cli), false), GuideRowStatus::Queued);
        cli.state = Some(CliState::parse(&job("running", "added 12 packages\n")));
        assert_eq!(
            guide_row_status(Some(&cli), false),
            GuideRowStatus::Installing
        );
        assert_eq!(cli.progress_line().as_deref(), Some("added 12 packages"));
        let mut done = job("succeeded", "ok");
        done["executablePath"] = json!("/usr/local/bin/qodercli");
        cli.state = Some(CliState::parse(&done));
        assert_eq!(
            guide_row_status(Some(&cli), false),
            GuideRowStatus::Installed
        );
        assert_eq!(cli.progress_line(), None);
    }

    #[test]
    fn the_scan_alone_marks_a_row_installed() {
        // A row never read (lazy), or one whose own state predates the install, reads Installed once
        // the rescan finds the CLI.
        let cli = row(true, None);
        assert_eq!(
            guide_row_status(Some(&cli), true),
            GuideRowStatus::Installed
        );
        assert_eq!(guide_row_status(None, true), GuideRowStatus::Installed);
        let running = row(true, Some(job("running", "")));
        assert_eq!(
            guide_row_status(Some(&running), true),
            GuideRowStatus::Installing
        );
    }

    #[test]
    fn a_failed_install_offers_retry_with_its_output() {
        let cli = row(
            true,
            Some(job("failed", "\u{1b}[31mnpm ERR! code EACCES\u{1b}[0m\n")),
        );
        assert_eq!(guide_row_status(Some(&cli), false), GuideRowStatus::Failed);
        assert_eq!(cli.error().as_deref(), Some("npm ERR! code EACCES"));
        assert_eq!(
            strip_ansi(cli.job_output().unwrap()).trim(),
            "npm ERR! code EACCES"
        );
        let mut refused = row(true, None);
        refused.action_error = Some("No install method".into());
        assert_eq!(
            guide_row_status(Some(&refused), false),
            GuideRowStatus::Failed
        );
    }

    #[test]
    fn closing_the_guide_keeps_running_and_installed_rows() {
        let mut rows = CliRows::default();
        let mut installed = job("succeeded", "");
        installed["executablePath"] = json!("/bin/qodercli");
        rows.rows.insert("guide:a".into(), row(true, None));
        rows.rows
            .insert("guide:b".into(), row(true, Some(job("running", ""))));
        rows.rows
            .insert("guide:c".into(), row(true, Some(installed)));
        rows.rows
            .insert("guide:d".into(), row(true, Some(job("failed", "x"))));
        rows.rows.insert("row:claude".into(), row(false, None));
        rows.pending.insert(1, ("guide:a".into(), Pending::Read));
        rows.pending.insert(2, ("guide:b".into(), Pending::Read));
        rows.clear_lazy();
        let mut kept: Vec<_> = rows.rows.keys().cloned().collect();
        kept.sort();
        assert_eq!(kept, ["guide:b", "guide:c", "row:claude"]);
        assert!(rows.pending.contains_key(&2) && !rows.pending.contains_key(&1));
    }

    #[test]
    fn the_guide_keeps_listing_an_agent_its_row_installed() {
        assert!(guide_lists_agent(false, false));
        assert!(!guide_lists_agent(true, false));
        assert!(guide_lists_agent(true, true));
    }
}
