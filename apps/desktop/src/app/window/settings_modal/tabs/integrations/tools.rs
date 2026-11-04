//! `ManagedToolsSection` (packages/core-ui/settings-modal/tabs/managed-tools-section.tsx (deleted 2026-10-01)) and
//! `useManagedTools` (packages/core-ui/managed-tools/use-managed-tools.ts (deleted 2026-10-01)): the Tools section of
//! the Integrations page, listing what Ghostex installs for the user from gxserver's
//! `/api/managedTools`, re-read every 1.5 s while a job runs, each finished job toasted once.
//!
//! CDXC:ManagedTools 2026-09-29 SEE-ALSO: the React twin above carries the user's decision (one
//! click installs, the Install tooltip says how); server/src/managed_tools/endpoint.rs answers.
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, SizedButtonSize, SizedButtonVariant, settings_button,
    settings_icon, settings_list_item, settings_section, settings_sized_button,
    settings_square_button, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{post_store_message, store_machine_gxserver_rpc};
use super::IntegrationsTab;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, div, px, rgb,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

/// `MANAGED_TOOLS_POLL_MS`.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// A list reads versions and, once every six hours, the latest releases.
const TIMEOUT: Duration = Duration::from_secs(60);
/// `MANAGED_TOOL_ORDER`.
const ORDER: [&str; 7] = [
    "node",
    "uv",
    "homebrew",
    "systemTools",
    "beads",
    "gh",
    "glab",
];
const SPINNER: &str = "modals/settings/loader-2.svg";
const SKY_400: u32 = 0x38bdf8;

#[derive(Default)]
pub(super) struct ManagedToolsState {
    tools: Option<Vec<Value>>,
    error: Option<String>,
    /// The tool whose "check again" read is in flight.
    checking: Option<String>,
    /// An operation whose start request has not answered yet: (tool, operation).
    starting: Option<(String, String)>,
    /// The last `finishedAt` seen per tool.
    seen_finish: HashMap<String, String>,
    /// The first read only records jobs that finished before the page opened.
    primed: bool,
    poll: Option<Task<()>>,
    confirm_uninstall: Option<String>,
}

impl ManagedToolsState {
    pub(super) fn confirming_uninstall(&self) -> bool {
        self.confirm_uninstall.is_some()
    }

    /// A list has come back.
    pub(super) fn has_tools(&self) -> bool {
        self.tools.is_some()
    }
}

fn icon(tool: &str) -> &'static str {
    match tool {
        "node" => "modals/settings/brand-nodejs.svg",
        "uv" => "modals/settings/brand-python.svg",
        "homebrew" => "modals/settings/beer.svg",
        "beads" => "modals/settings/layout-kanban.svg",
        "gh" => "modals/settings/brand-github.svg",
        "glab" => "modals/settings/brand-gitlab.svg",
        "trycua" => "modals/settings/device-desktop.svg",
        "spaceo" => "modals/settings/device-laptop.svg",
        _ => "modals/settings/tools.svg",
    }
}

/// `MANAGED_TOOL_ACTION_WORDS`: (running, done, failed).
fn words(operation: &str) -> (&'static str, &'static str, &'static str) {
    match operation {
        "update" => ("Updating", "updated", "update failed"),
        "reinstall" => ("Reinstalling", "reinstalled", "reinstall failed"),
        "uninstall" => ("Uninstalling", "uninstalled", "uninstall failed"),
        _ => ("Installing", "installed", "install failed"),
    }
}

fn text<'a>(tool: &'a Value, key: &str) -> Option<&'a str> {
    tool.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn id(tool: &Value) -> String {
    text(tool, "id").unwrap_or_default().to_string()
}

fn label(tool: &Value) -> String {
    text(tool, "label").unwrap_or("Tool").to_string()
}

fn active_job(tool: &Value) -> Option<&Value> {
    tool.get("job")
        .filter(|job| matches!(job["status"].as_str(), Some("queued" | "running")))
}

fn can(tool: &Value, operation: &str) -> bool {
    tool["actions"]
        .as_array()
        .is_some_and(|actions| actions.iter().any(|action| action == operation))
}

fn sort(mut tools: Vec<Value>) -> Vec<Value> {
    let rank = |tool: &Value| {
        let id = id(tool);
        ORDER
            .iter()
            .position(|known| *known == id)
            .unwrap_or(ORDER.len())
    };
    tools.sort_by_key(rank);
    tools
}

/// `managedToolInstallTooltip`.
fn install_tooltip(tool: &Value) -> String {
    let plan = text(tool, "installPlan").unwrap_or_default().to_string();
    if tool["needsPassword"] == true && !plan.to_lowercase().contains("password") {
        format!("{plan} Asks for your password.")
    } else {
        plan
    }
}

/// `uninstallDetail`.
fn uninstall_detail(tool: &Value) -> String {
    match id(tool).as_str() {
        "node" => {
            "Agent CLIs installed with Ghostex's Node.js stop working until it's installed again."
                .into()
        }
        "uv" => "Tools uv installed, such as Claude Swap, keep working.".into(),
        _ => format!("Removes Ghostex's copy of {}.", label(tool)),
    }
}

/// `rowDetail`.
fn row_detail(tool: &Value) -> String {
    let name = label(tool);
    let description = text(tool, "description").unwrap_or_default();
    if let Some(job) = active_job(tool) {
        if job["status"] == "queued" {
            return "Waiting for another install to finish…".into();
        }
        let (running, _, _) = words(job["operation"].as_str().unwrap_or_default());
        let progress = job["output"].as_str().and_then(|output| {
            output
                .lines()
                .map(str::trim)
                .filter(|line| {
                    !line.is_empty()
                        && !(line.ends_with('%')
                            && line[..line.len() - 1].chars().all(|c| c.is_ascii_digit()))
                })
                .next_back()
        });
        return match progress {
            Some(progress) => format!("{running} {name}… {progress}"),
            None => format!("{running} {name}…"),
        };
    }
    if let Some(reason) = text(tool, "notAnswering") {
        return format!("{name} isn't answering. {reason}");
    }
    if tool["installed"] != true {
        return match text(tool, "detail") {
            Some(detail) => format!("{description} {detail}"),
            None => description.to_string(),
        };
    }
    if id(tool) == "systemTools"
        && let Some(detail) = text(tool, "detail")
    {
        return detail.to_string();
    }
    // An installed Fast Computer & Browser Use or SpaceO still waiting for its macOS grants.
    if let Some(note) = text(tool, "permissionNote") {
        return match text(tool, "version") {
            Some(version) => format!("Version {version}. {note}"),
            None => note.to_string(),
        };
    }
    let owner = if text(tool, "source") == Some("ghostex") {
        "installed by Ghostex"
    } else {
        "installed by you"
    };
    match text(tool, "version") {
        Some(version) => format!("{description} Version {version} · {owner}."),
        None => {
            let mut owner = owner.to_string();
            owner[..1].make_ascii_uppercase();
            format!("{description} {owner}.")
        }
    }
}

impl IntegrationsTab {
    fn managed_connected(&self, cx: &gpui::App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    fn managed_toast(&self, level: &str, title: &str, description: &str, cx: &mut gpui::App) {
        self.store
            .update(cx, |store, cx| store.toast(level, title, description, cx));
    }

    /// `load()`: the whole list (also the poll).
    pub(super) fn load_managed_tools(&mut self, cx: &mut Context<Self>) {
        if !self.managed_connected(cx) {
            return;
        }
        let this = cx.weak_entity();
        let machine = self.shown_machine.clone();
        store_machine_gxserver_rpc(
            &self.store.clone(),
            machine.clone().as_deref(),
            "/api/managedTools",
            // A remote machine's list adds Fast Computer & Browser Use and SpaceO (remote_machine.rs).
            json!({ "action": "list", "includeDesktopControl": machine.is_some() }),
            TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| match result {
                    _ if page.shown_machine != machine => {}
                    Ok(state) => {
                        page.managed.error = None;
                        let tools = state["tools"].as_array().cloned().unwrap_or_default();
                        page.apply_managed_tools(tools, cx);
                    }
                    Err(error) => {
                        page.managed.error = Some(if error.trim().is_empty() {
                            "Could not read the tools.".into()
                        } else {
                            error
                        });
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    /// `announce(next)`: toasts each job that finished since the last read.
    fn announce_managed_tools(&mut self, next: &[Value], cx: &mut Context<Self>) {
        for tool in next {
            let Some(job) = tool.get("job") else {
                continue;
            };
            let Some(finished_at) = job["finishedAt"].as_str().filter(|at| !at.is_empty()) else {
                continue;
            };
            let key = id(tool);
            if self.managed.seen_finish.get(&key).map(String::as_str) == Some(finished_at) {
                continue;
            }
            self.managed
                .seen_finish
                .insert(key, finished_at.to_string());
            if !self.managed.primed {
                continue;
            }
            let name = label(tool);
            let operation = job["operation"].as_str().unwrap_or_default();
            let (_, done, failed) = words(operation);
            match job["status"].as_str() {
                Some("succeeded") => {
                    let description = if operation == "uninstall" {
                        format!("Ghostex removed its copy of {name}.")
                    } else if let Some(note) = text(tool, "permissionNote") {
                        note.to_string()
                    } else if let Some(version) = text(tool, "version") {
                        format!("Version {version} is installed.")
                    } else {
                        format!("{name} is ready.")
                    };
                    self.managed_toast("success", &format!("{name} {done}"), &description, cx);
                }
                Some("failed") => self.managed_toast(
                    "error",
                    &format!("{name} {failed}"),
                    text(job, "error").unwrap_or("The install did not finish."),
                    cx,
                ),
                _ => {}
            }
        }
        self.managed.primed = true;
    }

    fn apply_managed_tools(&mut self, next: Vec<Value>, cx: &mut Context<Self>) {
        self.announce_managed_tools(&next, cx);
        self.managed.tools = Some(sort(next));
        self.sync_managed_poll(cx);
        cx.notify();
    }

    fn merge_managed_tool(&mut self, tool: Value, cx: &mut Context<Self>) {
        self.announce_managed_tools(std::slice::from_ref(&tool), cx);
        let key = id(&tool);
        let mut tools = self.managed.tools.take().unwrap_or_default();
        tools.retain(|existing| id(existing) != key);
        tools.push(tool);
        self.managed.tools = Some(sort(tools));
        self.sync_managed_poll(cx);
        cx.notify();
    }

    /// Polls while any job is queued or running.
    fn sync_managed_poll(&mut self, cx: &mut Context<Self>) {
        let busy = self
            .managed
            .tools
            .iter()
            .flatten()
            .any(|tool| active_job(tool).is_some());
        if !busy {
            self.managed.poll = None;
            return;
        }
        if self.managed.poll.is_some() {
            return;
        }
        self.managed.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                if this
                    .update(cx, |page, cx| page.load_managed_tools(cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    /// `start(tool, operation)`.
    fn start_managed_tool(
        &mut self,
        tool: String,
        operation: &'static str,
        cx: &mut Context<Self>,
    ) {
        if !self.managed_connected(cx) {
            return;
        }
        self.managed.starting = Some((tool.clone(), operation.to_string()));
        cx.notify();
        let this = cx.weak_entity();
        let machine = self.shown_machine.clone();
        store_machine_gxserver_rpc(
            &self.store.clone(),
            machine.clone().as_deref(),
            "/api/managedTools",
            json!({ "action": "start", "tool": tool, "operation": operation }),
            TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if page.shown_machine != machine {
                        return;
                    }
                    page.managed.starting = None;
                    match result {
                        Ok(state) => page.merge_managed_tool(state, cx),
                        Err(error) => {
                            let name = page
                                .managed
                                .tools
                                .iter()
                                .flatten()
                                .find(|existing| id(existing) == tool)
                                .map(label)
                                .unwrap_or_else(|| tool.clone());
                            page.managed_toast(
                                "error",
                                &format!("{name} {}", words(operation).2),
                                if error.is_empty() {
                                    "The install could not start."
                                } else {
                                    &error
                                },
                                cx,
                            );
                            cx.notify();
                        }
                    }
                });
            },
            cx,
        );
    }

    /// `checkAgain(tool)`: a fresh read and its toast.
    fn check_managed_tool(&mut self, tool: String, cx: &mut Context<Self>) {
        if !self.managed_connected(cx) {
            return;
        }
        self.managed.checking = Some(tool.clone());
        cx.notify();
        let this = cx.weak_entity();
        let machine = self.shown_machine.clone();
        store_machine_gxserver_rpc(
            &self.store.clone(),
            machine.clone().as_deref(),
            "/api/managedTools",
            json!({ "action": "read", "tool": tool, "fresh": true }),
            TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if page.shown_machine != machine {
                        return;
                    }
                    page.managed.checking = None;
                    match result {
                        Ok(state) => {
                            let name = label(&state);
                            let version = text(&state, "version").unwrap_or_default().to_string();
                            match state["updateAvailable"].as_bool() {
                                _ if text(&state, "notAnswering").is_some() => page.managed_toast(
                                    "warning",
                                    &format!("{name} isn't answering"),
                                    text(&state, "notAnswering").unwrap_or_default(),
                                    cx,
                                ),
                                Some(true) => page.managed_toast(
                                    "info",
                                    &format!("{name} update available"),
                                    &format!(
                                        "Version {} is available; {version} is installed.",
                                        text(&state, "latestVersion").unwrap_or_default()
                                    ),
                                    cx,
                                ),
                                Some(false) => page.managed_toast(
                                    "success",
                                    &format!("{name} is up to date"),
                                    &format!("Version {version} is the latest release."),
                                    cx,
                                ),
                                None => page.managed_toast(
                                    "warning",
                                    &format!("Couldn't check for {name} updates"),
                                    text(&state, "checkError").unwrap_or("Try again in a moment."),
                                    cx,
                                ),
                            }
                            page.merge_managed_tool(state, cx);
                        }
                        Err(error) => {
                            page.managed_toast(
                                "error",
                                "Update check failed",
                                if error.is_empty() {
                                    "Try again in a moment."
                                } else {
                                    &error
                                },
                                cx,
                            );
                            cx.notify();
                        }
                    }
                });
            },
            cx,
        );
    }

    /// `onRunTerminalCommand`: the desktop reads the tool's `terminalCommand` from gxserver itself.
    fn run_managed_tool_in_terminal(&mut self, tool: String, cx: &mut Context<Self>) {
        post_store_message(
            &self.store,
            json!({ "type": "runManagedToolTerminalCommand", "toolId": tool }),
            cx,
        );
    }

    /// `ManagedToolsSection`.
    /// CDXC:ManagedTools 2026-09-29 DECISION:
    /// User: "when they click on something, we help them install it on windows/macos/linux automatically (show a button with a tooltip explaining how we'll install) but 1 click installs it for them as much as possible". Settings > Integrations > Tools lists what Ghostex can install for the user: Install carries the server's plan as its tooltip, and an installed tool gets the Update (or check again), Reinstall and Uninstall icon buttons of the Trycua row, each only when gxserver offers it. Tools the user installed themselves are shown as theirs without buttons.
    pub(super) fn managed_tools_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.managed_connected(cx) {
            return None;
        }
        let refresh = settings_button(
            p,
            "integrations-tools-refresh",
            "Refresh",
            Some("modals/settings/refresh.svg"),
            ButtonVariant::Ghost,
            self.managed.tools.is_none(),
            Some("Tool status is being checked.".into()),
            |page: &mut Self, _window, cx| page.load_managed_tools(cx),
            cx,
        );
        let mut rows: Vec<AnyElement> = Vec::new();
        match self.managed.tools.clone() {
            None => rows.push(settings_list_item(
                p,
                None,
                None,
                "Tools",
                Some(
                    div()
                        .child(
                            self.managed
                                .error
                                .clone()
                                .unwrap_or_else(|| "Checking what is installed…".into()),
                        )
                        .into_any_element(),
                ),
                None,
            )),
            Some(tools) => {
                // PowerShell 7 has its own row in General > Terminal; a remote machine's Fast
                // Computer & Browser Use and SpaceO have theirs in their own sections.
                for tool in tools.iter().filter(|tool| {
                    tool["supported"] == true
                        && !matches!(id(tool).as_str(), "powershell" | "trycua" | "spaceo")
                }) {
                    rows.push(self.managed_tool_row(p, tool, cx));
                    let key = id(tool);
                    if self.managed.confirm_uninstall.as_deref() == Some(key.as_str())
                        && active_job(tool).is_none()
                    {
                        rows.push(self.managed_uninstall_confirmation(p, tool, cx));
                    }
                }
            }
        }
        settings_section(
            p,
            "Tools",
            Some("Ghostex installs these when something you set up needs them. Your own copies are used when you have them.".into()),
            Some(refresh),
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// The rows of the managed tools `ids` names, in that order, with an unsupported one saying why
    /// (a remote machine's Fast Computer & Browser Use and SpaceO, in their own sections).
    pub(super) fn managed_rows_for(
        &mut self,
        p: &SettingsPalette,
        ids: &[&str],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let tools = self.managed.tools.clone().unwrap_or_default();
        let mut rows = Vec::new();
        for wanted in ids {
            let Some(tool) = tools.iter().find(|tool| id(tool) == *wanted) else {
                continue;
            };
            if tool["supported"] != true {
                rows.push(settings_list_item(
                    p,
                    Some(ListItemStatus::Neutral),
                    Some(settings_icon(icon(wanted), 17.0, p.muted).into_any_element()),
                    label(tool),
                    Some(
                        div()
                            .child(
                                text(tool, "unsupportedReason")
                                    .unwrap_or("Not available on this machine.")
                                    .to_string(),
                            )
                            .into_any_element(),
                    ),
                    None,
                ));
                continue;
            }
            rows.push(self.managed_tool_row(p, tool, cx));
            if self.managed.confirm_uninstall.as_deref() == Some(*wanted)
                && active_job(tool).is_none()
            {
                rows.push(self.managed_uninstall_confirmation(p, tool, cx));
            }
        }
        rows
    }

    fn managed_uninstall_confirmation(
        &mut self,
        p: &SettingsPalette,
        tool: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = id(tool);
        let cancel = settings_sized_button(
            p,
            SharedString::from(format!("tool-{key}-uninstall-cancel")),
            "Cancel",
            None,
            None,
            SizedButtonVariant::Outline,
            SizedButtonSize::Sm,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.managed.confirm_uninstall = None;
                cx.notify();
            },
            cx,
        );
        let confirm_key = key.clone();
        let confirm = settings_sized_button(
            p,
            SharedString::from(format!("tool-{key}-uninstall-confirm")),
            "Uninstall",
            None,
            None,
            SizedButtonVariant::Destructive,
            SizedButtonSize::Sm,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.managed.confirm_uninstall = None;
                page.start_managed_tool(confirm_key.clone(), "uninstall", cx);
            },
            cx,
        );
        settings_list_item(
            p,
            Some(ListItemStatus::Warning),
            None,
            format!("Uninstall {}?", label(tool)),
            Some(
                div()
                    .whitespace_normal()
                    .child(uninstall_detail(tool))
                    .into_any_element(),
            ),
            Some(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(cancel)
                    .child(confirm)
                    .into_any_element(),
            ),
        )
    }

    /// `ManagedToolRow`.
    fn managed_tool_row(
        &mut self,
        p: &SettingsPalette,
        tool: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = id(tool);
        let name = label(tool);
        let installed = tool["installed"] == true;
        let running: Option<String> = active_job(tool)
            .and_then(|job| job["operation"].as_str().map(str::to_string))
            .or_else(|| {
                self.managed
                    .starting
                    .as_ref()
                    .filter(|(starting, _)| *starting == key)
                    .map(|(_, operation)| operation.clone())
            });
        let busy = running.is_some();
        let checking = self.managed.checking.as_deref() == Some(key.as_str());
        let busy_reason: SharedString = match &running {
            Some(operation) => format!("{} {name}…", words(operation).0).into(),
            None => format!("{name} isn't available here.").into(),
        };
        let version = text(tool, "version").map(str::to_string);
        let installed_suffix = version
            .as_deref()
            .map(|version| format!(" (installed v{version})"))
            .unwrap_or_default();
        // GitHub #336: a tool whose check didn't answer in time gets a Retry and nothing else.
        let not_answering = !busy && text(tool, "notAnswering").is_some();
        let status = if not_answering
            || tool["updateAvailable"] == true
            || (installed && text(tool, "permissionNote").is_some())
        {
            ListItemStatus::Warning
        } else if installed {
            ListItemStatus::Success
        } else {
            ListItemStatus::Neutral
        };

        let mut controls: Vec<AnyElement> = Vec::new();
        if not_answering {
            let click_key = key.clone();
            controls.push(settings_button(
                p,
                SharedString::from(format!("tool-{key}-retry")),
                if checking { "Checking…" } else { "Retry" },
                Some(if checking {
                    SPINNER
                } else {
                    "modals/settings/refresh.svg"
                }),
                ButtonVariant::Outline,
                checking,
                Some(format!("Checking {name} again…").into()),
                move |page: &mut Self, _window, cx| {
                    page.check_managed_tool(click_key.clone(), cx);
                },
                cx,
            ));
        } else if !installed {
            let terminal = text(tool, "terminalCommand").map(str::to_string);
            // A terminal install opens a terminal on this computer, so a remote machine's waits for one there.
            let remote_terminal = terminal.is_some() && self.shown_machine.is_some();
            let disabled = busy || remote_terminal || (terminal.is_none() && !can(tool, "install"));
            let reason: SharedString = if busy {
                busy_reason.clone()
            } else if remote_terminal {
                format!("Install {name} from a terminal on that machine.").into()
            } else {
                text(tool, "unavailableReason")
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{name} can't be installed here."))
                    .into()
            };
            let installing = running.as_deref() == Some("install");
            let click_key = key.clone();
            let via_terminal = terminal.is_some();
            let button = settings_button(
                p,
                SharedString::from(format!("tool-{key}-install")),
                if installing {
                    "Installing…"
                } else {
                    "Install"
                },
                Some(if installing {
                    SPINNER
                } else if via_terminal {
                    "modals/settings/terminal-2.svg"
                } else {
                    "modals/settings/download.svg"
                }),
                ButtonVariant::Outline,
                disabled,
                Some(reason),
                move |page: &mut Self, _window, cx| {
                    if via_terminal {
                        page.run_managed_tool_in_terminal(click_key.clone(), cx);
                    } else {
                        page.start_managed_tool(click_key.clone(), "install", cx);
                    }
                },
                cx,
            );
            // `AppTooltip content={plan}` around the enabled button; a disabled one shows its reason.
            controls.push(if disabled {
                button
            } else {
                div()
                    .id(ElementId::Name(format!("tool-{key}-install-tip").into()))
                    .tooltip(tooltip_text(install_tooltip(tool)))
                    .child(button)
                    .into_any_element()
            });
        } else {
            if can(tool, "update") {
                let (icon_path, color, tooltip, is_update) = match tool["updateAvailable"].as_bool()
                {
                    Some(true) => (
                        "modals/settings/circle-arrow-up.svg",
                        Some(rgb(SKY_400)),
                        match text(tool, "latestVersion") {
                            Some(latest) => {
                                format!("Update {name} to v{latest}{installed_suffix}")
                            }
                            None => format!("Update {name}{installed_suffix}"),
                        },
                        true,
                    ),
                    Some(false) => (
                        "modals/settings/circle-check.svg",
                        Some(p.muted),
                        format!(
                            "{name}{} is up to date. Click to check again.",
                            version
                                .as_deref()
                                .map(|version| format!(" v{version}"))
                                .unwrap_or_default()
                        ),
                        false,
                    ),
                    None => (
                        "modals/settings/cloud-search.svg",
                        None,
                        match text(tool, "checkError") {
                            Some(error) => format!(
                                "Couldn't check for {name} updates: {error} Click to try again."
                            ),
                            None => format!("Check for {name} updates{installed_suffix}"),
                        },
                        false,
                    ),
                };
                let click_key = key.clone();
                controls.push(settings_square_button(
                    p,
                    SharedString::from(format!("tool-{key}-update")),
                    if running.as_deref() == Some("update") || checking {
                        SPINNER
                    } else {
                        icon_path
                    },
                    color,
                    SizedButtonVariant::Ghost,
                    32.0,
                    Some(tooltip.into()),
                    busy || checking,
                    Some(if checking {
                        format!("Checking for {name} updates…").into()
                    } else {
                        busy_reason.clone()
                    }),
                    move |page: &mut Self, _window, cx| {
                        if is_update {
                            page.start_managed_tool(click_key.clone(), "update", cx);
                        } else {
                            page.check_managed_tool(click_key.clone(), cx);
                        }
                    },
                    cx,
                ));
            }
            if can(tool, "reinstall") {
                let click_key = key.clone();
                controls.push(settings_square_button(
                    p,
                    SharedString::from(format!("tool-{key}-reinstall")),
                    if running.as_deref() == Some("reinstall") {
                        SPINNER
                    } else {
                        "modals/settings/refresh.svg"
                    },
                    None,
                    SizedButtonVariant::Ghost,
                    32.0,
                    Some(format!("Reinstall the latest {name}{installed_suffix}").into()),
                    busy,
                    Some(busy_reason.clone()),
                    move |page: &mut Self, _window, cx| {
                        page.start_managed_tool(click_key.clone(), "reinstall", cx);
                    },
                    cx,
                ));
            }
            if can(tool, "uninstall") {
                let click_key = key.clone();
                controls.push(settings_square_button(
                    p,
                    SharedString::from(format!("tool-{key}-uninstall")),
                    if running.as_deref() == Some("uninstall") {
                        SPINNER
                    } else {
                        "modals/settings/trash.svg"
                    },
                    None,
                    SizedButtonVariant::Ghost,
                    32.0,
                    Some(format!("Uninstall {name}").into()),
                    busy,
                    Some(busy_reason),
                    move |page: &mut Self, _window, cx| {
                        page.managed.confirm_uninstall = if page
                            .managed
                            .confirm_uninstall
                            .as_deref()
                            == Some(click_key.as_str())
                        {
                            None
                        } else {
                            Some(click_key.clone())
                        };
                        cx.notify();
                    },
                    cx,
                ));
            }
        }
        settings_list_item(
            p,
            Some(status),
            Some(settings_icon(icon(&key), 17.0, p.muted).into_any_element()),
            name,
            Some(div().child(row_detail(tool)).into_any_element()),
            (!controls.is_empty()).then(|| {
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(controls)
                    .into_any_element()
            }),
        )
        .into_any_element()
    }
}
