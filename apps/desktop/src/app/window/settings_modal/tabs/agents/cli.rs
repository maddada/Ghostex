//! The agent CLI install and update controls of Settings > Agents: the job lifecycle of
//! packages/core-ui/agent-cli/use-agent-cli-job.ts (deleted 2026-10-01) (read, poll every 1.5s while a job is queued or
//! running, `onInstalled` once per succeeded job id, a start whose reply failed still re-reads),
//! the one `list` read of use-agent-cli-list.ts (deleted 2026-10-01), the expanded row's `AgentCliControls`
//! (controls.tsx) and the collapsed row's `AgentCliRowAction` (row-action.tsx (deleted 2026-10-01)), all over gxserver's
//! `/api/agentCliMaintenance` (server/src/agent_cli/endpoint.rs).
//!
//! CDXC:AgentProviders 2026-09-28 SEE-ALSO:
//! The row action and the expanded controls keep separate job states, as their two React hooks do. Every request goes to the machine the page's machine picker shows (tabs/machine_picker.rs): this computer's gxserver, or a connected remote machine's through its tunnel.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, settings_button_sized, settings_icon, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_machine_gxserver_rpc;
use super::AgentsTab;
use super::icons;
use super::logos::muted_fill;
use super::select::DropdownOption;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Task,
    Transformation, Window, div, px, radians,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

/// `AGENT_CLI_JOB_POLL_MS`.
const POLL: Duration = Duration::from_millis(1500);
/// The transport's `AbortSignal.timeout(30_000)`.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const ENDPOINT: &str = "/api/agentCliMaintenance";

/// One row of packages/shared/agent-cli-catalog.json, the fields the page reads.
pub(super) struct CliCatalogEntry {
    pub(super) agent_id: String,
    pub(super) binary: String,
    pub(super) docs_url: String,
}

/// `AGENT_CLI_CATALOG`.
/// CDXC:AgentProviders 2026-09-14 DECISION:
/// User: install and update agent CLIs from the Agents page, using each CLI's own commands, and link to each agent's installation docs. ZCode uses npm install -g zcode-app-cli@latest, launches with zcode, and links to https://github.com/kingsword09/zcode-cli (packages/shared/agent-cli-catalog.json, which server/src/agent_cli/catalog.rs embeds).
pub(super) fn cli_catalog() -> &'static [CliCatalogEntry] {
    static CATALOG: OnceLock<Vec<CliCatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let entries: Vec<Value> = serde_json::from_str(include_str!(
            "../../../../../../../../packages/shared/agent-cli-catalog.json"
        ))
        .unwrap_or_default();
        entries
            .iter()
            .filter_map(|entry| {
                let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
                Some(CliCatalogEntry {
                    agent_id: text("agentId")?,
                    binary: text("binary").unwrap_or_default(),
                    docs_url: text("docsUrl").unwrap_or_default(),
                })
            })
            .collect()
    })
}

pub(super) fn cli_definition(agent_id: &str) -> Option<&'static CliCatalogEntry> {
    cli_catalog()
        .iter()
        .find(|entry| entry.agent_id == agent_id)
}

#[derive(Clone, Debug)]
pub(super) struct CliMethod {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) command: String,
    pub(super) unavailable_reason: Option<String>,
    /// What one click does, including anything Ghostex installs first (absent from older gxservers).
    pub(super) plan: Option<String>,
    /// "node", "homebrew", "mise" or "systemTools": a tool Ghostex installs before `command`.
    pub(super) prerequisite: Option<String>,
    pub(super) system_tools: Vec<String>,
}

impl CliMethod {
    pub(super) fn parse(method: &Value) -> Option<Self> {
        Some(Self {
            id: nonempty(method, "id")?,
            label: nonempty(method, "label").unwrap_or_default(),
            command: nonempty(method, "command").unwrap_or_default(),
            unavailable_reason: nonempty(method, "unavailableReason"),
            plan: nonempty(method, "plan"),
            prerequisite: nonempty(method, "prerequisite"),
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
    }

    /// `agentCliMethodTooltip` (packages/shared/agent-cli-maintenance.ts (deleted 2026-10-01)).
    pub(super) fn tooltip(&self) -> String {
        self.unavailable_reason
            .clone()
            .or_else(|| self.plan.clone())
            .unwrap_or_else(|| self.command.clone())
    }

    /// `agentCliPrerequisiteSuffix`.
    pub(super) fn prerequisite_suffix(&self) -> Option<String> {
        match self.prerequisite.as_deref()? {
            "node" => Some("installs Node.js first".into()),
            "homebrew" => Some("installs Homebrew first".into()),
            "mise" => Some("installs mise first".into()),
            "systemTools" => {
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
        }
    }

    /// `agentCliMethodLabel`.
    pub(super) fn display_label(&self) -> String {
        match self.prerequisite_suffix() {
            Some(suffix) => format!("{}, {suffix}", self.label),
            None => self.label.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CliJobInfo {
    pub(super) id: String,
    pub(super) operation: String,
    pub(super) status: String,
    pub(super) output: String,
    pub(super) error: Option<String>,
    /// A finished update that still leaves a newer release (mise held it back), in plain words.
    pub(super) notice: Option<String>,
}

impl CliJobInfo {
    /// `isAgentCliJobActive`.
    fn active(&self) -> bool {
        self.status == "queued" || self.status == "running"
    }
}

/// `AgentCliState`.
#[derive(Clone, Debug, Default)]
pub(super) struct CliState {
    pub(super) platform: String,
    pub(super) executable_path: Option<String>,
    pub(super) version: Option<String>,
    pub(super) version_error: Option<String>,
    pub(super) detected_method_id: Option<String>,
    pub(super) path_directory: Option<String>,
    pub(super) latest_version: Option<String>,
    pub(super) update_available: bool,
    /// The CLI's own update check, in its words ("Update available: 14220 commits behind origin/main.").
    pub(super) update_summary: Option<String>,
    /// The CLI's own update check is still running in gxserver.
    pub(super) update_checking: bool,
    /// Why mise does not install the newer release yet (the user's `minimum_release_age`).
    pub(super) update_held_back: Option<String>,
    /// mise installs an older eligible release now and holds the newest back.
    pub(super) update_note: Option<String>,
    pub(super) methods: Vec<CliMethod>,
    pub(super) job: Option<CliJobInfo>,
}

fn nonempty(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

impl CliState {
    pub(super) fn parse(value: &Value) -> Option<Self> {
        if !value.is_object() {
            return None;
        }
        Some(Self {
            platform: nonempty(value, "platform").unwrap_or_default(),
            executable_path: nonempty(value, "executablePath"),
            version: nonempty(value, "version"),
            version_error: nonempty(value, "versionError"),
            detected_method_id: nonempty(value, "detectedMethodId"),
            path_directory: nonempty(value, "pathDirectory"),
            latest_version: nonempty(value, "latestVersion"),
            update_available: value.get("updateAvailable").and_then(Value::as_bool) == Some(true),
            update_summary: nonempty(value, "updateSummary"),
            update_checking: value.get("updateChecking").and_then(Value::as_bool) == Some(true),
            update_held_back: nonempty(value, "updateHeldBack"),
            update_note: nonempty(value, "updateNote"),
            methods: value
                .get("methods")
                .and_then(Value::as_array)
                .map(|methods| methods.iter().filter_map(CliMethod::parse).collect())
                .unwrap_or_default(),
            job: value.get("job").and_then(|job| {
                Some(CliJobInfo {
                    id: nonempty(job, "id")?,
                    operation: nonempty(job, "operation").unwrap_or_default(),
                    status: nonempty(job, "status").unwrap_or_default(),
                    output: job
                        .get("output")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    error: nonempty(job, "error"),
                    notice: nonempty(job, "notice"),
                })
            }),
        })
    }

    /// `defaultAgentCliInstallMethod`.
    fn default_install_method(&self) -> Option<&CliMethod> {
        if self.executable_path.is_some() {
            return None;
        }
        self.methods
            .iter()
            .find(|method| method.unavailable_reason.is_none())
            .or_else(|| self.methods.first())
    }

    fn job_active(&self) -> bool {
        self.job.as_ref().is_some_and(CliJobInfo::active)
    }

    /// An update Ghostex can run now: not one mise holds back.
    fn update_offered(&self) -> bool {
        self.update_available && self.update_held_back.is_none()
    }

    /// What the newer release is called: its version, else the CLI's own words.
    fn newer_release(&self) -> Option<String> {
        self.latest_version
            .clone()
            .or_else(|| self.update_summary.clone())
    }
}

/// Which of an agent's two job hooks: the collapsed row's action or the expanded controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum CliSlot {
    Row,
    Panel,
}

/// One `useAgentCliJob` instance.
#[derive(Default)]
pub(super) struct CliJob {
    eager: bool,
    state: Option<CliState>,
    loading: bool,
    /// The read whose answer is current (`signal.active`).
    read_seq: u64,
    error: Option<String>,
    action_error: Option<String>,
    starting: bool,
    refreshed: bool,
    completed_job: Option<String>,
    poll: Option<Task<()>>,
    /// The Installation method the user picked (`methodId`).
    method_id: Option<String>,
    /// `<details>` of the command output is open.
    output_open: bool,
}

impl CliJob {
    fn running(&self) -> bool {
        self.starting || self.state.as_ref().is_some_and(CliState::job_active)
    }
}

/// Every CLI answer the page holds.
#[derive(Default)]
pub(super) struct CliModel {
    /// `useAgentCliList`: every catalog agent's state from one `list` read.
    pub(super) list: HashMap<String, CliState>,
    /// Why the last `list` read failed (a remote machine not connected, or too old to answer).
    pub(super) list_error: Option<String>,
    list_seq: u64,
    jobs: HashMap<(CliSlot, String), CliJob>,
    seq: u64,
}

impl CliModel {
    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

/// `x1b[...` escape sequences removed from job output (`replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '')`).
fn strip_ansi(output: &str) -> String {
    let mut plain = String::with_capacity(output.len());
    let mut chars = output.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
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

/// A 16px icon that turns (`animate-spin`).
pub(super) fn spinning_icon(
    path: &'static str,
    size: f32,
    color: gpui::Rgba,
    id: &str,
) -> AnyElement {
    settings_icon(path, size, color)
        .flex_shrink_0()
        .with_animation(
            SharedString::from(format!("{id}-spin")),
            Animation::new(Duration::from_millis(1000)).repeat(),
            |icon, delta| {
                icon.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

type CliReply = Box<dyn FnOnce(&mut AgentsTab, Result<Value, String>, &mut Context<AgentsTab>)>;

impl AgentsTab {
    /// The connection exists: the host reaches gxserver.
    pub(super) fn cli_connected(&self, cx: &gpui::App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    /// Sends to the machine the page shows; an answer that comes back after another machine was
    /// picked is dropped.
    fn cli_request(&self, params: Value, reply: CliReply, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        let store = self.store.clone();
        let machine = self.shown_machine.clone();
        store_machine_gxserver_rpc(
            &store,
            machine.clone().as_deref(),
            ENDPOINT,
            params,
            REQUEST_TIMEOUT,
            move |result, cx| {
                let _ = weak.update(cx, |page, cx| {
                    if page.shown_machine == machine {
                        reply(page, result, cx);
                    }
                });
            },
            cx,
        );
    }

    /// The name of the machine the CLI answers describe ("This computer" or the remote's name).
    pub(super) fn cli_machine_label(&self) -> String {
        self.shown_machine_name
            .clone()
            .unwrap_or_else(|| "This computer".to_string())
    }

    fn cli_job(&mut self, slot: CliSlot, agent_id: &str) -> &mut CliJob {
        self.cli
            .jobs
            .entry((slot, agent_id.to_string()))
            .or_insert_with(|| CliJob {
                eager: slot == CliSlot::Panel,
                ..CliJob::default()
            })
    }

    /// Mounts the expanded controls' job (`eager`: read as soon as it exists).
    pub(super) fn cli_mount_panel(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        if self
            .cli
            .jobs
            .contains_key(&(CliSlot::Panel, agent_id.to_string()))
        {
            return;
        }
        self.cli_job(CliSlot::Panel, agent_id);
        self.cli_read(CliSlot::Panel, agent_id, cx);
    }

    /// Unmounts a job (a collapsed row, a row whose action went away, the page left).
    pub(super) fn cli_unmount(&mut self, slot: CliSlot, agent_id: &str) {
        self.cli.jobs.remove(&(slot, agent_id.to_string()));
    }

    pub(super) fn cli_reset(&mut self) {
        self.cli = CliModel::default();
    }

    fn cli_read(&mut self, slot: CliSlot, agent_id: &str, cx: &mut Context<Self>) {
        if !self.cli_connected(cx) {
            return;
        }
        let seq = self.cli.next_seq();
        let job = self.cli_job(slot, agent_id);
        job.loading = true;
        job.read_seq = seq;
        let agent = agent_id.to_string();
        self.cli_request(
            json!({ "action": "read", "agentId": agent_id }),
            Box::new(move |page, result, cx| page.cli_read_done(slot, &agent, seq, result, cx)),
            cx,
        );
        cx.notify();
    }

    fn cli_read_done(
        &mut self,
        slot: CliSlot,
        agent_id: &str,
        seq: u64,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = self.cli.jobs.get_mut(&(slot, agent_id.to_string())) else {
            return;
        };
        if job.read_seq != seq {
            return;
        }
        let mut installed = false;
        let installed_agent = agent_id.to_string();
        match result.and_then(|value| {
            CliState::parse(&value).ok_or_else(|| "The CLI request failed.".to_string())
        }) {
            Ok(state) => {
                if let Some(done) = state.job.as_ref().filter(|job| job.status == "succeeded")
                    && job.completed_job.as_deref() != Some(done.id.as_str())
                {
                    job.completed_job = Some(done.id.clone());
                    installed = true;
                }
                job.state = Some(state);
                job.error = None;
            }
            Err(error) => job.error = Some(error),
        }
        job.loading = false;
        // CDXC:AgentProviders 2026-09-28 WHY (use-agent-cli-job.ts (deleted 2026-10-01)): polling is keyed on finished reads, so a queued job or a quiet installer keeps being re-read until the job ends.
        job.poll = None;
        if job
            .state
            .as_ref()
            .is_some_and(|state| state.job_active() || state.update_checking)
        {
            let agent = agent_id.to_string();
            job.poll = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(POLL).await;
                let _ = this.update(cx, |page, cx| page.cli_read(slot, &agent, cx));
            }));
        }
        cx.notify();
        if installed {
            // The turn-on steps and resume hooks belong to this computer's agents.
            if self.shown_machine.is_none() {
                self.on_cli_installed(&installed_agent, cx);
            }
            self.cli_changed(cx);
        }
    }

    /// `refresh()`: re-read now (the first refresh of a lazy row is its first read).
    pub(super) fn cli_refresh(&mut self, slot: CliSlot, agent_id: &str, cx: &mut Context<Self>) {
        let job = self.cli_job(slot, agent_id);
        job.refreshed = true;
        if job.eager || job.refreshed {
            self.cli_read(slot, agent_id, cx);
        }
    }

    /// `start(operation, methodId)`.
    pub(super) fn cli_start(
        &mut self,
        slot: CliSlot,
        agent_id: &str,
        operation: &str,
        method_id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.cli_connected(cx) {
            return;
        }
        let job = self.cli_job(slot, agent_id);
        job.starting = true;
        job.action_error = None;
        let agent = agent_id.to_string();
        self.cli_request(
            json!({ "action": "start", "agentId": agent_id, "operation": operation, "methodId": method_id }),
            Box::new(move |page, result, cx| {
                let job = page.cli_job(slot, &agent);
                job.starting = false;
                match result.and_then(|value| {
                    CliState::parse(&value).ok_or_else(|| "The CLI request failed.".to_string())
                }) {
                    Ok(state) => job.state = Some(state),
                    // A timed-out start response can still have started the server-owned job.
                    Err(error) => job.action_error = Some(error),
                }
                page.cli_refresh(slot, &agent, cx);
            }),
            cx,
        );
        cx.notify();
    }

    /// `install()`: read the state when it is not loaded, then install through the default method.
    pub(super) fn cli_install(&mut self, slot: CliSlot, agent_id: &str, cx: &mut Context<Self>) {
        if !self.cli_connected(cx) {
            return;
        }
        let job = self.cli_job(slot, agent_id);
        job.starting = true;
        job.action_error = None;
        match job.state.clone() {
            Some(state) => self.cli_install_from(slot, agent_id, state, cx),
            None => {
                let agent = agent_id.to_string();
                self.cli_request(
                    json!({ "action": "read", "agentId": agent_id }),
                    Box::new(move |page, result, cx| {
                        match result.and_then(|value| {
                            CliState::parse(&value)
                                .ok_or_else(|| "The CLI request failed.".to_string())
                        }) {
                            Ok(state) => {
                                let job = page.cli_job(slot, &agent);
                                job.state = Some(state.clone());
                                job.error = None;
                                page.cli_install_from(slot, &agent, state, cx);
                            }
                            Err(error) => page.cli_install_finished(slot, &agent, Some(error), cx),
                        }
                    }),
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn cli_install_from(
        &mut self,
        slot: CliSlot,
        agent_id: &str,
        state: CliState,
        cx: &mut Context<Self>,
    ) {
        if state.executable_path.is_some() {
            self.cli_install_finished(slot, agent_id, None, cx);
            return;
        }
        let Some(method) = state.default_install_method().cloned() else {
            self.cli_install_finished(
                slot,
                agent_id,
                Some(format!(
                    "No install method is available on {}. Follow the install docs.",
                    if self.shown_machine.is_some() {
                        self.cli_machine_label()
                    } else {
                        "this computer".to_string()
                    }
                )),
                cx,
            );
            return;
        };
        if let Some(reason) = method.unavailable_reason {
            self.cli_install_finished(slot, agent_id, Some(reason), cx);
            return;
        }
        let agent = agent_id.to_string();
        self.cli_request(
            json!({ "action": "start", "agentId": agent_id, "operation": "install", "methodId": method.id }),
            Box::new(move |page, result, cx| match result.and_then(|value| {
                CliState::parse(&value).ok_or_else(|| "The CLI request failed.".to_string())
            }) {
                Ok(state) => {
                    page.cli_job(slot, &agent).state = Some(state);
                    page.cli_install_finished(slot, &agent, None, cx);
                }
                Err(error) => page.cli_install_finished(slot, &agent, Some(error), cx),
            }),
            cx,
        );
    }

    /// The `finally` of `install()`: a timed-out reply can still have started the job, so re-read.
    fn cli_install_finished(
        &mut self,
        slot: CliSlot,
        agent_id: &str,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let job = self.cli_job(slot, agent_id);
        if error.is_some() {
            job.action_error = error;
        }
        job.starting = false;
        self.cli_refresh(slot, agent_id, cx);
        cx.notify();
    }

    /// `addToPath()`; the row action refreshes hook status and the list afterwards.
    pub(super) fn cli_add_to_path(
        &mut self,
        slot: CliSlot,
        agent_id: &str,
        then_changed: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.cli_connected(cx) {
            return;
        }
        let job = self.cli_job(slot, agent_id);
        job.starting = true;
        job.action_error = None;
        let agent = agent_id.to_string();
        self.cli_request(
            json!({ "action": "addToPath", "agentId": agent_id }),
            Box::new(move |page, result, cx| {
                let job = page.cli_job(slot, &agent);
                match result.and_then(|value| {
                    CliState::parse(&value).ok_or_else(|| "The CLI request failed.".to_string())
                }) {
                    Ok(state) => job.state = Some(state),
                    Err(error) => job.action_error = Some(error),
                }
                job.starting = false;
                cx.notify();
                if then_changed {
                    page.cli_changed(cx);
                }
            }),
            cx,
        );
        cx.notify();
    }

    /// `cliList.refresh()` / the list read when the page becomes active.
    pub(super) fn cli_list_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.active || !self.cli_connected(cx) {
            return;
        }
        self.cli.list_seq += 1;
        let seq = self.cli.list_seq;
        self.cli_request(
            json!({ "action": "list" }),
            Box::new(move |page, result, cx| {
                if page.cli.list_seq != seq {
                    return;
                }
                // Rows fall back to hook status on a failed list; the expanded controls show the error.
                let value = match result {
                    Ok(value) => value,
                    Err(error) => {
                        page.cli.list_error = Some(error);
                        cx.notify();
                        return;
                    }
                };
                page.cli.list_error = None;
                page.cli.list = value
                    .get("agents")
                    .and_then(Value::as_array)
                    .map(|agents| {
                        agents
                            .iter()
                            .filter_map(|agent| {
                                let id = nonempty(agent, "agentId")?;
                                Some((id, CliState::parse(agent)?))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                cx.notify();
            }),
            cx,
        );
    }

    /// `onCliChanged`: a job finished, so refresh hook status and the list.
    pub(super) fn cli_changed(&mut self, cx: &mut Context<Self>) {
        // Hook status is this computer's.
        if self.shown_machine.is_none() {
            self.request_hook_status(cx);
        }
        self.cli_list_refresh(cx);
    }

    /// The state a row reads: its own job's answer, else the list's.
    fn cli_row_state(&self, agent_id: &str) -> Option<CliState> {
        self.cli
            .jobs
            .get(&(CliSlot::Row, agent_id.to_string()))
            .and_then(|job| job.state.clone())
            .or_else(|| self.cli.list.get(agent_id).cloned())
    }

    /// `AgentCliRowAction`: Install CLI when it is missing, Add to PATH when new terminals cannot
    /// find it, Update CLI when the vendor has a newer release, and the running job's label.
    pub(super) fn render_cli_row_action(
        &mut self,
        p: &SettingsPalette,
        agent_id: &str,
        cli_missing_from_hooks: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let definition = cli_definition(agent_id)?;
        if !self.cli_connected(cx) {
            return None;
        }
        let state = self.cli_row_state(agent_id);
        let (running, action_error) = self
            .cli
            .jobs
            .get(&(CliSlot::Row, agent_id.to_string()))
            .map(|job| (job.running(), job.action_error.clone()))
            .unwrap_or((false, None));
        let missing = state.as_ref().map_or(cli_missing_from_hooks, |state| {
            state.executable_path.is_none()
        });
        let failed = state
            .as_ref()
            .and_then(|state| state.job.as_ref())
            .filter(|job| job.status == "failed")
            .and_then(|job| job.error.clone());
        let error = action_error.or(failed);
        let id = |suffix: &str| SharedString::from(format!("agent-cli-row-{agent_id}-{suffix}"));
        if running {
            let job = state.as_ref().and_then(|state| state.job.as_ref());
            let label = match job {
                Some(job) if job.status == "queued" => "Waiting…",
                Some(job) if job.operation == "update" => "Updating…",
                _ => "Installing…",
            };
            return Some(cli_button_busy(p, id("busy"), label, cx));
        }
        let agent = agent_id.to_string();
        if missing {
            let title = error.clone().unwrap_or_else(|| {
                state
                    .as_ref()
                    .and_then(CliState::default_install_method)
                    .map(CliMethod::tooltip)
                    .unwrap_or_else(|| {
                        format!("Install {} with its official installer", definition.binary)
                    })
            });
            return Some(cli_titled_button(
                p,
                id("install"),
                if error.is_some() {
                    "Retry install"
                } else {
                    "Install CLI"
                },
                icons::DOWNLOAD,
                title,
                move |page: &mut Self, _window, cx| page.cli_install(CliSlot::Row, &agent, cx),
                cx,
            ));
        }
        if let Some(directory) = state
            .as_ref()
            .and_then(|state| state.path_directory.clone())
        {
            let title = error.clone().unwrap_or_else(|| {
                format!(
                    "Add {directory} to your PATH so new terminals find {}",
                    definition.binary
                )
            });
            return Some(cli_titled_button(
                p,
                id("path"),
                "Add to PATH",
                icons::DOWNLOAD,
                title,
                move |page: &mut Self, _window, cx| {
                    page.cli_add_to_path(CliSlot::Row, &agent, true, cx)
                },
                cx,
            ));
        }
        let state = state?;
        let detected = state.detected_method_id.clone()?;
        if !state.update_offered() {
            return None;
        }
        let title = error.clone().unwrap_or_else(|| {
            let update = match &state.latest_version {
                Some(latest) => format!("Update {} to {latest}.", definition.binary),
                None => format!(
                    "Update {}. {}",
                    definition.binary,
                    state.update_summary.clone().unwrap_or_default()
                ),
            };
            let update = match &state.update_note {
                Some(note) => format!("{update} {note}"),
                None => update,
            };
            match state
                .methods
                .iter()
                .find(|method| method.id == detected)
                .map(CliMethod::tooltip)
            {
                Some(plan) => format!("{update} {plan}"),
                None => update,
            }
        });
        // CDXC:AgentLauncher 2026-10-06 DECISION: User: "ok implement the plan": an available update is not a problem, so the row offers it as a muted link instead of a button.
        Some(cli_quiet_link(
            p,
            id("update"),
            if error.is_some() {
                "Retry update"
            } else {
                "Update available"
            },
            title,
            move |page: &mut Self, _window, cx| {
                page.cli_start(CliSlot::Row, &agent, "update", &detected, cx)
            },
            cx,
        ))
    }

    /// Whether a row shows its CLI action (`showCliAction` of `SettingsAgentRow`).
    pub(super) fn cli_row_action_shown(
        &self,
        agent_id: &str,
        cli_missing: bool,
        cx: &gpui::App,
    ) -> bool {
        if !self.cli_connected(cx) || cli_definition(agent_id).is_none() {
            return false;
        }
        let list = self.cli.list.get(agent_id);
        cli_missing
            || list.is_some_and(|state| state.path_directory.is_some())
            || list.is_some_and(CliState::update_offered)
    }

    /// `cliMissing` of a row: the list's answer, else the hook status.
    pub(super) fn cli_missing(&self, agent_id: &str, hook_status: Option<&str>) -> bool {
        match self.cli.list.get(agent_id) {
            Some(state) => state.executable_path.is_none(),
            None => hook_status == Some("cliMissing"),
        }
    }

    /// `AgentCliControls` of an expanded row.
    pub(super) fn render_cli_controls(
        &mut self,
        p: &SettingsPalette,
        agent_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let definition = cli_definition(agent_id)?;
        let connected = self.cli_connected(cx);
        if connected {
            self.cli_mount_panel(agent_id, cx);
        }
        let key = (CliSlot::Panel, agent_id.to_string());
        let (state, loading, error, action_error, running, method_id, output_open) = self
            .cli
            .jobs
            .get(&key)
            .map(|job| {
                (
                    job.state.clone(),
                    job.loading,
                    job.error.clone(),
                    job.action_error.clone(),
                    job.running(),
                    job.method_id.clone(),
                    job.output_open,
                )
            })
            .unwrap_or((None, false, None, None, false, None, false));
        let installed = state
            .as_ref()
            .is_some_and(|state| state.executable_path.is_some());
        let operation = if installed { "update" } else { "install" };
        let methods: Vec<CliMethod> = state
            .as_ref()
            .map(|state| {
                state
                    .methods
                    .iter()
                    .filter(|method| {
                        !installed
                            || state.detected_method_id.is_none()
                            || state.detected_method_id.as_deref() == Some(method.id.as_str())
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let selected = (if installed {
            state
                .as_ref()
                .and_then(|state| state.detected_method_id.clone())
        } else {
            None
        })
        .or(method_id)
        .or_else(|| {
            state
                .as_ref()
                .and_then(|state| state.default_install_method())
                .map(|method| method.id.clone())
        });
        let method = methods
            .iter()
            .find(|method| Some(&method.id) == selected.as_ref())
            .cloned();
        let status = match &state {
            Some(state) if state.executable_path.is_some() => {
                let mut text = state
                    .version
                    .clone()
                    .unwrap_or_else(|| "Installed".to_string());
                if state.update_available
                    && let Some(newer) = state.newer_release()
                {
                    if state.update_held_back.is_some() {
                        text.push_str(&format!(" · {newer} held back by mise"));
                    } else if state.latest_version.is_some() {
                        text.push_str(&format!(" · {newer} available"));
                    } else {
                        text.push_str(&format!(" · {newer}"));
                    }
                } else if state.update_checking {
                    text.push_str(" · checking for updates…");
                }
                text
            }
            Some(_) => "Not installed".to_string(),
            None if loading => "Checking CLI…".to_string(),
            None => "Not checked".to_string(),
        };
        let mut status_line = status;
        if connected {
            status_line.push_str(&format!(" · {}", self.cli_machine_label()));
        }
        if let Some(state) = &state {
            status_line.push_str(&format!(" · {}", state.platform));
        }
        let muted = p.muted;
        let small = |text: String, color: gpui::Rgba| {
            div()
                .min_w_0()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(hsla(color))
                .child(text)
                .into_any_element()
        };
        let docs_url = definition.docs_url.clone();
        let docs_link = (!docs_url.is_empty()).then(|| {
            let url = docs_url.clone();
            let link_color = p.primary;
            h_flex()
                .id(SharedString::from(format!("agent-cli-docs-{agent_id}")))
                .items_center()
                .gap(px(4.0))
                .cursor_pointer()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(hsla(link_color))
                .hover(|this| this.underline())
                .role(gpui::Role::Link)
                .aria_label("Install docs")
                .on_press(cx, move |page, _window, cx| {
                    let url = url.clone();
                    page.store.update(cx, |store, cx| {
                        store.post_message(json!({ "type": "openExternalUrl", "url": url }), cx)
                    });
                })
                .child("Install docs")
                .child(settings_icon(icons::EXTERNAL_LINK, 14.0, link_color))
                .into_any_element()
        });
        let refresh_disabled = !connected || loading || running;
        let refresh_agent = agent_id.to_string();
        let refresh_icon = if loading {
            spinning_icon(
                icons::REFRESH,
                16.0,
                p.foreground,
                &format!("agent-cli-refresh-{agent_id}"),
            )
        } else {
            settings_icon(icons::REFRESH, 16.0, p.foreground).into_any_element()
        };
        let refresh = ghost_icon_button(
            p,
            SharedString::from(format!("agent-cli-refresh-{agent_id}")),
            "Check the CLI again",
            refresh_icon,
            refresh_disabled,
            false,
            move |page: &mut Self, _window, cx| {
                page.cli_refresh(CliSlot::Panel, &refresh_agent, cx)
            },
            cx,
        );
        let mut body = v_flex()
            .w_full()
            .min_w_0()
            .gap(px(12.0))
            .py(px(16.0))
            .border_b_1()
            .border_color(hsla(css_fade(p.hairline, 0.7)))
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .text_color(hsla(p.foreground))
                                    .child("Agent CLI"),
                            )
                            .child(small(status_line, muted)),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .children(docs_link)
                            .child(refresh),
                    ),
            );
        if let Some(path) = state
            .as_ref()
            .and_then(|state| state.executable_path.clone())
        {
            body = body.child(
                div()
                    .w_full()
                    .min_w_0()
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(hsla(muted))
                    .child(path),
            );
        }
        if let Some(directory) = state
            .as_ref()
            .and_then(|state| state.path_directory.clone())
        {
            let path_agent = agent_id.to_string();
            body = body.child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(hsla(muted))
                            .child(format!(
                                "New terminals will not find {}: {directory} is not on your PATH.",
                                definition.binary
                            )),
                    )
                    .child(settings_button_sized(
                        p,
                        SharedString::from(format!("agent-cli-path-{agent_id}")),
                        "Add to PATH",
                        None,
                        ButtonVariant::Outline,
                        ButtonSize::Sm,
                        !connected || running,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.cli_add_to_path(CliSlot::Panel, &path_agent, false, cx)
                        },
                        cx,
                    )),
            );
        }
        if !connected {
            body = body.child(small(
                "Connect to a computer to manage its agent CLIs.".to_string(),
                muted,
            ));
        }
        if state.is_some() && methods.is_empty() {
            body = body.child(small(
                "Follow the installation docs for this platform.".to_string(),
                muted,
            ));
        }
        if !methods.is_empty() {
            let options: Vec<DropdownOption> = methods
                .iter()
                .map(|method| DropdownOption::plain(method.id.clone(), method.display_label()))
                .collect();
            let select_agent = agent_id.to_string();
            let select = self.dropdown(
                p,
                SharedString::from(format!("agent-cli-method-{agent_id}")),
                &options,
                selected.as_deref(),
                if installed {
                    "Choose how this CLI was installed"
                } else {
                    "Installation method"
                },
                false,
                None,
                running,
                None,
                move |page, value, _window, cx| {
                    page.cli_job(CliSlot::Panel, &select_agent).method_id = Some(value);
                    cx.notify();
                },
                window,
                cx,
            );
            let job_status = state.as_ref().and_then(|state| state.job.clone());
            let button_label = if running {
                match &job_status {
                    Some(job) if job.status == "queued" => "Waiting…",
                    Some(job) if job.operation == "update" => "Updating…",
                    _ => "Installing…",
                }
            } else if installed {
                "Update CLI"
            } else {
                "Install CLI"
            };
            let start_disabled = running
                || loading
                || !connected
                || method.is_none()
                || method
                    .as_ref()
                    .is_some_and(|method| method.unavailable_reason.is_some());
            let start_agent = agent_id.to_string();
            let start_method = method.as_ref().map(|method| method.id.clone());
            let start_title = method.as_ref().map(CliMethod::tooltip);
            let button_icon: AnyElement = if running {
                spinning_icon(
                    icons::REFRESH,
                    16.0,
                    p.foreground,
                    &format!("agent-cli-start-{agent_id}"),
                )
            } else {
                settings_icon(
                    if installed {
                        icons::REFRESH
                    } else {
                        icons::DOWNLOAD
                    },
                    16.0,
                    p.foreground,
                )
                .into_any_element()
            };
            body = body.child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_1().min_w_0().flex().child(select))
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "agent-cli-start-{agent_id}-title"
                            )))
                            .flex_shrink_0()
                            .when_some(start_title, |this, title| this.tooltip(tooltip_text(title)))
                            .child(cli_icon_button(
                                p,
                                SharedString::from(format!("agent-cli-start-{agent_id}")),
                                button_label,
                                button_icon,
                                start_disabled,
                                move |page: &mut Self, _window, cx| {
                                    let running = page
                                        .cli
                                        .jobs
                                        .get(&(CliSlot::Panel, start_agent.clone()))
                                        .is_some_and(CliJob::running);
                                    if let Some(method_id) = start_method.clone()
                                        && !running
                                    {
                                        page.cli_start(
                                            CliSlot::Panel,
                                            &start_agent,
                                            operation,
                                            &method_id,
                                            cx,
                                        );
                                    }
                                },
                                cx,
                            )),
                    ),
            );
            if let Some(method) = &method {
                body = body.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(hsla(muted_fill(p)))
                        .font_family(MODAL_MONO_FONT)
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.foreground))
                        .child(method.command.clone()),
                );
                if let Some(reason) = &method.unavailable_reason {
                    body = body.child(small(reason.clone(), muted));
                } else if let Some(plan) = &method.plan {
                    body = body.child(small(plan.clone(), muted));
                }
            }
        }
        if let Some(held_back) = state.as_ref().and_then(|state| {
            state
                .update_held_back
                .clone()
                .or_else(|| state.update_note.clone())
        }) {
            body = body.child(small(held_back, muted));
        }
        if let Some(version_error) = state.as_ref().and_then(|state| state.version_error.clone()) {
            body = body.child(small(
                format!("Version check failed: {version_error}"),
                muted,
            ));
        }
        if let Some(message) = action_error.or(error) {
            body = body.child(small(message, p.destructive));
        }
        if let Some(job) = state.as_ref().and_then(|state| state.job.clone()) {
            let failed = job.status == "failed";
            let text = match job.status.as_str() {
                "queued" => "Waiting for another CLI install or update to finish.".to_string(),
                "running" => {
                    "Running. You can close Settings and return to check progress.".to_string()
                }
                "failed" => job
                    .error
                    .clone()
                    .unwrap_or_else(|| "CLI operation failed.".to_string()),
                _ => job.notice.clone().unwrap_or_else(|| {
                    "CLI command completed. Start a new session to use the installed version."
                        .to_string()
                }),
            };
            let mut job_block = v_flex()
                .w_full()
                .min_w_0()
                .child(small(text, if failed { p.destructive } else { muted }));
            if !job.output.is_empty() {
                let toggle_agent = agent_id.to_string();
                job_block = job_block.child(
                    h_flex()
                        .id(SharedString::from(format!("agent-cli-output-{agent_id}")))
                        .mt(px(8.0))
                        .gap(px(4.0))
                        .items_center()
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(muted))
                        .role(gpui::Role::Button)
                        .aria_label("Command output")
                        .aria_expanded(output_open)
                        .on_press(cx, move |page, _window, cx| {
                            let job = page.cli_job(CliSlot::Panel, &toggle_agent);
                            job.output_open = !job.output_open;
                            cx.notify();
                        })
                        .child(disclosure_triangle(output_open, muted))
                        .child("Command output"),
                );
                if output_open {
                    job_block = job_block.child(
                        div()
                            .id(SharedString::from(format!(
                                "agent-cli-output-text-{agent_id}"
                            )))
                            .mt(px(8.0))
                            .w_full()
                            .max_h(px(192.0))
                            .overflow_y_scroll()
                            .p(px(8.0))
                            .rounded(px(4.0))
                            .bg(hsla(muted_fill(p)))
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(hsla(p.foreground))
                            .child(strip_ansi(&job.output)),
                    );
                }
            }
            body = body.child(job_block);
        }
        Some(body.into_any_element())
    }
}

/// The `<details>` marker: a small filled triangle, pointing down while open.
fn disclosure_triangle(open: bool, color: gpui::Rgba) -> AnyElement {
    div()
        .text_size(px(9.0))
        .text_color(hsla(color))
        .child(if open { "▼" } else { "▶" })
        .into_any_element()
}

/// A 28px ghost icon button (`size='icon-sm' variant='ghost'`); `pressed` is the
/// `aria-expanded` fill of an open disclosure.
pub(super) fn ghost_icon_button(
    p: &SettingsPalette,
    id: SharedString,
    label: impl Into<SharedString>,
    icon: AnyElement,
    disabled: bool,
    pressed: bool,
    on_click: impl Fn(&mut AgentsTab, &mut Window, &mut Context<AgentsTab>) + 'static,
    cx: &mut Context<AgentsTab>,
) -> AnyElement {
    let hover = if p.light {
        gpui::rgb(0xf1f1f1)
    } else {
        css_fade(gpui::rgb(0x262626), 0.5)
    };
    let pressed_fill = muted_fill(p);
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.into())
        .flex_shrink_0()
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .when(pressed, |this| this.bg(hsla(pressed_fill)))
        .when(disabled, |this| this.opacity(0.5))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(if pressed { pressed_fill } else { hover })))
                .on_press(cx, move |page, window, cx| {
                    on_click(page, window, cx);
                })
        })
        .child(icon)
        .into_any_element()
}

/// A filled `size='sm'` button whose leading icon is any element (a spinner while busy).
fn cli_icon_button(
    p: &SettingsPalette,
    id: SharedString,
    label: &'static str,
    icon: AnyElement,
    disabled: bool,
    on_click: impl Fn(&mut AgentsTab, &mut Window, &mut Context<AgentsTab>) + 'static,
    cx: &mut Context<AgentsTab>,
) -> AnyElement {
    let fills = neutral_button_fills(p.light);
    h_flex()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .h(px(28.0))
        .pl(px(8.0))
        .pr(px(12.0))
        .gap(px(4.0))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(transparent())
        .bg(hsla(fills.rest))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(fills.hover)))
                .active(move |this| this.bg(hsla(fills.pressed)))
                .on_press(cx, move |page, window, cx| {
                    on_click(page, window, cx);
                })
        })
        .child(icon)
        .child(label)
        .into_any_element()
}

/// The row action's disabled busy button.
fn cli_button_busy(
    p: &SettingsPalette,
    id: SharedString,
    label: &'static str,
    cx: &mut Context<AgentsTab>,
) -> AnyElement {
    let icon = spinning_icon(icons::REFRESH, 16.0, p.foreground, &id);
    cli_icon_button(p, id, label, icon, true, |_, _, _| {}, cx)
}

/// The filled neutral `size='sm'` action with a leading refresh icon and a `title`, for offers
/// that are not problems (an available CLI update).
///
/// CDXC:AppModal 2026-10-23 DECISION: User: "the color of the bg of the button is the same as the bg behind it for this one. please fix." The 2026-10-06 muted link (no fill until hovered) read as plain text beside the agent's switch; every text button in Settings is filled (2026-10-14), so this one takes `neutral_button_fills` like its Install and Add to PATH neighbours. It stays a quieter size (12.5px) than they are, since an update is an offer, not a problem.
fn cli_quiet_link(
    p: &SettingsPalette,
    id: SharedString,
    label: &'static str,
    title: String,
    on_click: impl Fn(&mut AgentsTab, &mut Window, &mut Context<AgentsTab>) + 'static,
    cx: &mut Context<AgentsTab>,
) -> AnyElement {
    let fills = neutral_button_fills(p.light);
    h_flex()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .h(px(28.0))
        .px(px(8.0))
        .gap(px(5.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(transparent())
        .bg(hsla(fills.rest))
        .cursor_pointer()
        .text_size(px(12.5))
        .line_height(px(16.0))
        .text_color(hsla(p.foreground))
        .whitespace_nowrap()
        .hover(move |this| this.bg(hsla(fills.hover)))
        .active(move |this| this.bg(hsla(fills.pressed)))
        .tooltip(tooltip_text(title))
        .on_press(cx, move |page, window, cx| {
            on_click(page, window, cx);
        })
        .child(settings_icon(icons::REFRESH, 14.0, p.foreground))
        .child(label)
        .into_any_element()
}

/// A filled `size='sm'` action with a `title` (the Kit tooltip stands in for the native one).
fn cli_titled_button(
    p: &SettingsPalette,
    id: SharedString,
    label: &'static str,
    icon: &'static str,
    title: String,
    on_click: impl Fn(&mut AgentsTab, &mut Window, &mut Context<AgentsTab>) + 'static,
    cx: &mut Context<AgentsTab>,
) -> AnyElement {
    let icon = settings_icon(icon, 16.0, p.foreground).into_any_element();
    div()
        .id(SharedString::from(format!("{id}-title")))
        .flex_shrink_0()
        .tooltip(tooltip_text(title))
        .child(cli_icon_button(p, id, label, icon, false, on_click, cx))
        .into_any_element()
}
