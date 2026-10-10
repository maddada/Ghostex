//! The Integrations page (packages/core-ui/settings-modal/tabs/integrations.tsx (deleted 2026-10-01) and
//! integration-skills.tsx (deleted 2026-10-01)): the Ghostex CLI, Desktop control (Trycua and its system permissions),
//! the bundled agent skills and App Shots. Every action posts the message the React page posted
//! (`installGhostexCli`, `install*Skill`, `*CuaDriver`, `uninstallBundledAgentSkill(s)`, ...) and
//! the app answers each with a fresh `ghostexCliStatus`, which ends the page's checking state.
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, SettingsPage, settings_button, settings_icon,
    settings_section, switch_control, tooltip_text,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{
    SettingsStore, SettingsStoreEvent, post_store_message, store_copy_to_clipboard,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, ClickEvent, Context, ElementId, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

mod spaceo;
mod tools;

const SKILLS_MODULE: &str = super::super::catalog::module::AGENT_SKILLS;

const ICON_TERMINAL: &str = "modals/settings/terminal-2.svg";
const ICON_DOWNLOAD: &str = "modals/settings/download.svg";
const ICON_REFRESH: &str = "modals/settings/refresh.svg";
const ICON_DEVICE_DESKTOP: &str = "modals/settings/device-desktop.svg";
const ICON_DEVICE_LAPTOP: &str = "modals/settings/device-laptop.svg";
const ICON_SETTINGS: &str = "modals/settings/settings.svg";
const ICON_TRASH: &str = "modals/settings/trash.svg";
const ICON_COPY: &str = "modals/settings/copy.svg";
const ICON_CIRCLE_CHECK: &str = "modals/settings/circle-check.svg";
const ICON_CIRCLE_CHECK_FILLED: &str = "modals/settings/circle-check-filled.svg";
const ICON_CIRCLE_ARROW_UP: &str = "modals/settings/circle-arrow-up.svg";
const ICON_CLOUD_SEARCH: &str = "modals/settings/cloud-search.svg";
const ICON_INFO_CIRCLE: &str = "modals/settings/info-circle.svg";
const ICON_LOADER: &str = "modals/settings/loader-2.svg";

/// `copied` stays on for 1.2s after a copy, as the React copy buttons did.
const COPIED_FEEDBACK: Duration = Duration::from_millis(1200);

/// `BUNDLED_AGENT_SKILL_ICONS`.
fn skill_icon(skill_id: &str) -> &'static str {
    match skill_id {
        "browserUse" | "embeddedBrowserUse" => "modals/settings/browser.svg",
        "cli" => ICON_TERMINAL,
        "computerUse" => ICON_DEVICE_DESKTOP,
        "spaceo" => ICON_DEVICE_LAPTOP,
        "agentsOrchestration" => "modals/settings/sitemap.svg",
        "generateTitle" => "modals/settings/pencil.svg",
        "help" => "modals/settings/help-circle.svg",
        "manageBeads" => "modals/settings/layout-kanban.svg",
        "moveCodexSession" => "modals/settings/git-pull-request.svg",
        "visuals" => "modals/settings/chart-bar.svg",
        _ => ICON_TERMINAL,
    }
}

/// The install message of each bundled skill (`onInstallSkill` in integrations.tsx (deleted 2026-10-01)).
fn skill_install_message(skill_id: &str) -> Option<&'static str> {
    Some(match skill_id {
        "cli" => "installCliSkill",
        "browserUse" => "installBrowserUseSkill",
        "computerUse" => "installComputerUseSkill",
        "spaceo" => "installSpaceoSkill",
        "embeddedBrowserUse" => "installBrowserControl",
        "agentsOrchestration" => "installAgentsOrchestrationSkill",
        "manageBeads" => "installManageBeadsSkill",
        "generateTitle" => "installGenerateTitleSkill",
        "moveCodexSession" => "installMoveCodexSessionSkill",
        "help" => "installHelpSkill",
        "visuals" => "installVisualsSkill",
        _ => return None,
    })
}

/// `isBundledGhostexAgentSkillInstalled`.
fn skill_installed(skill_id: &str, status: Option<&Value>) -> bool {
    let key = match skill_id {
        "browserUse" => "browserSkillInstalled",
        "embeddedBrowserUse" => "embeddedBrowserSkillInstalled",
        "computerUse" => "computerUseSkillInstalled",
        "spaceo" => "spaceoSkillInstalled",
        "cli" => "cliSkillInstalled",
        "agentsOrchestration" => "agentsOrchestrationSkillInstalled",
        "manageBeads" => "manageBeadsSkillInstalled",
        "generateTitle" => "generateTitleSkillInstalled",
        "moveCodexSession" => "moveCodexSessionSkillInstalled",
        "help" => "helpSkillInstalled",
        "visuals" => "visualsSkillInstalled",
        _ => return false,
    };
    flag(status, key) == Some(true)
}

fn flag(status: Option<&Value>, key: &str) -> Option<bool> {
    status
        .and_then(|status| status.get(key))
        .and_then(Value::as_bool)
}

fn text<'a>(status: Option<&'a Value>, key: &str) -> Option<&'a str> {
    status
        .and_then(|status| status.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

/// One `VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS` entry.
struct Skill {
    id: String,
    name: String,
    /// The folder name agents invoke it by, as `$<skill_name>`.
    skill_name: String,
    description: String,
    command: String,
    tier: String,
    requires_cua_driver: bool,
    requires_spaceo: bool,
}

fn visible_skills() -> Vec<Skill> {
    settings_catalog()
        .module_value(SKILLS_MODULE, "VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS")
        .and_then(Value::as_array)
        .map(|skills| {
            skills
                .iter()
                .filter(|skill| {
                    cfg!(target_os = "macos")
                        || skill.get("macOSOnly").and_then(Value::as_bool) != Some(true)
                })
                .filter_map(|skill| {
                    let text = |key: &str| skill.get(key)?.as_str().map(str::to_string);
                    Some(Skill {
                        id: text("id")?,
                        name: text("name")?,
                        skill_name: text("skillName").unwrap_or_default(),
                        description: text("description").unwrap_or_default(),
                        command: text("command").unwrap_or_default(),
                        tier: text("tier").unwrap_or_default(),
                        requires_cua_driver: skill
                            .get("requiresCuaDriver")
                            .and_then(Value::as_bool)
                            == Some(true),
                        requires_spaceo: skill.get("requiresSpaceo").and_then(Value::as_bool)
                            == Some(true),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `trycuaJobView` (packages/core-ui/trycua-job.ts (deleted 2026-10-01)): the background job the desktop app runs for
/// a tool's Install, Update, Reinstall and Uninstall (Trycua's, SpaceO's), as the rows show it.
struct InstallJob {
    running: bool,
    /// The running job's operation.
    operation: Option<String>,
    /// Progress while running, or why the last one failed.
    detail: Option<String>,
    /// Its recent output, for the progress row's hover.
    output: String,
    running_reason: Option<String>,
    plan: Option<String>,
    blocked_reason: Option<String>,
}

fn trycua_job(status: Option<&Value>) -> InstallJob {
    install_job(
        status,
        &trycua_name(),
        "cuaDriverJob",
        "cuaDriverInstallPlan",
        Some("cuaDriverApplicationsBlockedReason"),
    )
}

/// The job, plan and blocked reason a tool's status fields carry.
fn install_job(
    status: Option<&Value>,
    name: &str,
    job_key: &str,
    plan_key: &str,
    blocked_key: Option<&str>,
) -> InstallJob {
    let job = status
        .and_then(|status| status.get(job_key))
        .filter(|job| job.is_object());
    let field = |key: &str| job.and_then(|job| job.get(key)).and_then(Value::as_str);
    let running = field("status") == Some("running");
    let operation = field("operation").unwrap_or("install").to_string();
    let verb = match operation.as_str() {
        "update" => "Updating",
        "reinstall" => "Reinstalling",
        "uninstall" => "Uninstalling",
        _ => "Installing",
    };
    let output = field("output").unwrap_or_default().to_string();
    let progress = output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string);
    let detail = if running {
        Some(format!(
            "{verb} {name}…{}",
            progress
                .as_ref()
                .map(|line| format!(" {line}"))
                .unwrap_or_default()
        ))
    } else if field("status") == Some("failed") {
        Some(format!(
            "The last {operation} did not finish: {}",
            field("error")
                .filter(|error| !error.is_empty())
                .map(str::to_string)
                .or(progress)
                .unwrap_or_else(|| "no output".to_string())
        ))
    } else {
        None
    };
    InstallJob {
        running,
        operation: running.then_some(operation),
        detail,
        output,
        running_reason: running.then(|| format!("{verb} {name}…")),
        plan: text(status, plan_key).map(str::to_string),
        blocked_reason: blocked_key
            .and_then(|key| text(status, key))
            .map(str::to_string),
    }
}

/// `GHOSTEX_TRYCUA_REPOSITORY_LABEL` and `GHOSTEX_TRYCUA_REPOSITORY_URL`: the link after the name.
fn trycua_repository() -> (String, String) {
    let catalog = settings_catalog();
    (
        catalog.text(SKILLS_MODULE, "GHOSTEX_TRYCUA_REPOSITORY_LABEL"),
        catalog.text(SKILLS_MODULE, "GHOSTEX_TRYCUA_REPOSITORY_URL"),
    )
}

/// `GHOSTEX_TRYCUA_PRODUCT_NAME`.
fn trycua_name() -> String {
    let name = settings_catalog().text(SKILLS_MODULE, "GHOSTEX_TRYCUA_PRODUCT_NAME");
    if name.is_empty() {
        "Fast Computer & Browser Use".to_string()
    } else {
        name
    }
}

/// `getCuaPermissionStatus`.
fn cua_permission_status(status: Option<&Value>, loading: bool) -> (&'static str, ListItemStatus) {
    if loading || status.is_none() {
        return ("Checking", ListItemStatus::Neutral);
    }
    if flag(status, "cuaDriverInstalled") != Some(true) {
        return (
            "Fast Computer & Browser Use Not Installed",
            ListItemStatus::Warning,
        );
    }
    let accessibility = flag(status, "cuaDriverAccessibilityPermissionGranted");
    let screen = flag(status, "cuaDriverScreenRecordingPermissionGranted");
    match (accessibility, screen) {
        (Some(true), Some(true)) => ("Permissions Allowed", ListItemStatus::Success),
        (Some(false), Some(false)) => ("Permissions Off - Open Settings", ListItemStatus::Warning),
        (Some(false), _) => ("Accessibility Off - Open Settings", ListItemStatus::Warning),
        (_, Some(false)) => (
            "Screen Recording Off - Open Settings",
            ListItemStatus::Warning,
        ),
        (Some(true), _) => ("Screen Recording Unknown", ListItemStatus::Warning),
        (_, Some(true)) => ("Accessibility Unknown", ListItemStatus::Warning),
        _ => ("Permission Status Unknown", ListItemStatus::Warning),
    }
}

/// The `IntegrationStatusPill` / badge colours: `border-<hue>-500/40 bg-<hue>-500/10 text-<hue>-200`,
/// with the light theme's darker text (styles/settings-light.css).
fn tinted_label(
    p: &SettingsPalette,
    hue: u32,
    text_dark: u32,
    text_light: u32,
    label: &str,
) -> AnyElement {
    div()
        .flex_shrink_0()
        .px(px(8.0))
        .py(px(2.0))
        .border_1()
        .border_color(hsla(css_fade(rgb(hue), 0.4)))
        .bg(hsla(css_fade(rgb(hue), 0.1)))
        .text_size(px(11.0))
        .line_height(px(16.0))
        .text_color(hsla(rgb(if p.light { text_light } else { text_dark })))
        .child(label.to_string())
        .into_any_element()
}

/// CDXC:Settings 2026-10-06 DECISION:
/// User: every skill row shows the command that runs it ("`$ghostex-computer-use` for example"), but "only show the command to run it if it's installed".
fn skill_invocation(p: &SettingsPalette, skill_name: &str) -> AnyElement {
    div()
        .flex_shrink_0()
        .font_family(MODAL_MONO_FONT)
        .text_size(px(13.0))
        .line_height(px(18.9))
        .text_color(hsla(p.muted))
        .child(format!("${skill_name}"))
        .into_any_element()
}

/// `amber-500` (a pill that needs attention) and `sky-500` (Beta).
const AMBER_500: u32 = 0xf59e0b;
const AMBER_200: u32 = 0xfde68a;
const AMBER_LIGHT_TEXT: u32 = 0x92400e;
const SKY_500: u32 = 0x0ea5e9;
const SKY_200: u32 = 0xbae6fd;
const SKY_LIGHT_TEXT: u32 = 0x0369a1;
const SKY_400: u32 = 0x38bdf8;

pub(crate) fn integrations_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| IntegrationsTab::new(store.clone(), cx)).into()
}

pub(crate) struct IntegrationsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// `ghostexCliStatusLoading`: set when a request or action is posted, cleared by the answer.
    loading: bool,
    /// Which copy button shows its `copied` state, and the task that turns it off.
    copied: Option<&'static str>,
    copied_task: Option<Task<()>>,
    /// The Tools section (tools.rs).
    managed: tools::ManagedToolsState,
}

impl IntegrationsTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.subscribe(
            &store,
            |page: &mut Self, _, event: &SettingsStoreEvent, cx| {
                let SettingsStoreEvent::HostPayload(kind) = event;
                if kind == "ghostexCliStatus" {
                    page.loading = false;
                    cx.notify();
                }
            },
        )
        .detach();
        // Integrations probes the CLI, skills and Trycua only while it is open, and only when no
        // status has arrived yet (after the window being built reaches its host).
        let missing = store.read(cx).host_payload("ghostexCliStatus").is_none();
        if missing {
            cx.spawn(async move |page, cx| {
                let _ = page.update(cx, |page, cx| page.post("requestGhostexCliStatus", cx));
            })
            .detach();
        }
        cx.spawn(async move |page, cx| {
            let _ = page.update(cx, |page, cx| page.load_managed_tools(cx));
        })
        .detach();
        Self {
            store,
            fields: FieldStates::default(),
            loading: missing,
            copied: None,
            copied_task: None,
            managed: tools::ManagedToolsState::default(),
        }
    }

    /// Posts `{ type }` with the checking state on, as every React handler did.
    fn post(&mut self, kind: &str, cx: &mut Context<Self>) {
        self.post_message(json!({ "type": kind }), cx);
    }

    fn post_message(&mut self, message: Value, cx: &mut Context<Self>) {
        self.loading = true;
        post_store_message(&self.store, message, cx);
        cx.notify();
    }

    /// `CopyCommandButton`: the command to the clipboard with the copy feedback.
    fn copy_command(&mut self, button: &'static str, command: String, cx: &mut Context<Self>) {
        store_copy_to_clipboard(&self.store, command, cx);
        self.copied = Some(button);
        self.copied_task = Some(cx.spawn(async move |page, cx| {
            cx.background_executor().timer(COPIED_FEEDBACK).await;
            let _ = page.update(cx, |page, cx| {
                page.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl super::HoldsUnsavedInput for IntegrationsTab {
    /// An Uninstall confirmation is open.
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        self.managed.confirming_uninstall()
    }
}

impl SettingsPage for IntegrationsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// What an Integrations row shows in its title line.
struct RowTitle {
    label: String,
    /// The hover info icon's tooltip (`SettingDescriptionTooltip`).
    description: String,
    /// A product-state badge such as Beta.
    badge: Option<&'static str>,
    /// A dependency note such as Needs Trycua.
    pill: Option<String>,
    /// An element after the label: the tool's repository link, or an installed skill's `$name`.
    link: Option<AnyElement>,
}

/// CDXC:Settings 2026-09-09 DECISION:
/// User: rows do not spell out Installed or Permissions Allowed in a pill. State is a small dot before the icon, the way the Extensions page marks enabled views.
/// `SettingsListItem` with `IntegrationRowTitle`: the status dot, the 32px icon, the label with its
/// badge, pill and hover info icon, and the controls; the info icon shows while the row is hovered.
fn integration_row(
    p: &SettingsPalette,
    id: &str,
    status: Option<ListItemStatus>,
    icon: Option<&'static str>,
    title: RowTitle,
    controls: Vec<AnyElement>,
) -> AnyElement {
    let group: SharedString = format!("integration-row-{id}").into();
    let label_line = h_flex()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .min_w_0()
                .text_size(px(14.0))
                .line_height(px(18.9))
                .text_color(hsla(p.foreground))
                .child(title.label.clone()),
        )
        .children(title.link)
        .children(title.badge.map(|badge| {
            div()
                .ml(px(6.0))
                .child(tinted_label(p, SKY_500, SKY_200, SKY_LIGHT_TEXT, badge))
        }))
        .children(title.pill.map(|pill| {
            div().ml(px(6.0)).child(tinted_label(
                p,
                AMBER_500,
                AMBER_200,
                AMBER_LIGHT_TEXT,
                &pill,
            ))
        }))
        .child(
            div()
                .id(ElementId::Name(format!("integration-row-{id}-info").into()))
                .flex_shrink_0()
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .opacity(0.0)
                .group_hover(group.clone(), |this| this.opacity(1.0))
                .tooltip(tooltip_text(title.description))
                .child(settings_icon(ICON_INFO_CIRCLE, 15.0, p.muted)),
        );
    h_flex()
        .id(ElementId::Name(group.clone()))
        .group(group)
        .w_full()
        .min_h(px(52.0))
        .px(px(20.0))
        .py(px(10.0))
        .gap(px(24.0))
        .items_center()
        .justify_between()
        .children(status.map(|status| {
            let color = match status {
                ListItemStatus::Success => modal_rgba(0x34d399, 0.8),
                ListItemStatus::Warning => modal_rgba(0xfbbf24, 0.85),
                ListItemStatus::Neutral => modal_rgba(0xffffff, 0.2),
            };
            div()
                .flex_shrink_0()
                .size(px(6.0))
                .rounded_full()
                .bg(hsla(color))
        }))
        .children(icon.map(|icon| {
            div()
                .flex_shrink_0()
                .size(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .child(settings_icon(icon, 18.0, p.muted))
        }))
        .child(div().flex_1().min_w_0().child(label_line))
        .when(!controls.is_empty(), |this| {
            this.child(
                h_flex()
                    .flex_shrink_0()
                    .max_w(gpui::relative(0.6))
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .children(controls),
            )
        })
        .into_any_element()
}

/// A 32px ghost icon button with a tooltip (`SettingButton size='icon' variant='ghost'` in an
/// `AppTooltip`), in `color` (the update button is sky-400).
#[allow(clippy::too_many_arguments)]
fn ghost_icon_button(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    icon: &'static str,
    color: Rgba,
    tooltip: String,
    disabled: bool,
    disabled_reason: String,
    on_click: impl Fn(&mut IntegrationsTab, &mut Window, &mut Context<IntegrationsTab>) + 'static,
    cx: &mut Context<IntegrationsTab>,
) -> AnyElement {
    let hover = if p.light {
        rgb(0xf1f1f1)
    } else {
        css_fade(rgb(0x262626), 0.5)
    };
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .when(disabled, |this| {
            this.opacity(0.5).tooltip(tooltip_text(disabled_reason))
        })
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .tooltip(tooltip_text(tooltip))
                .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                    on_click(page, window, cx);
                }))
        })
        .child(settings_icon(icon, 16.0, color))
        .into_any_element()
}

impl IntegrationsTab {
    fn cli_section(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let cli_ready = flag(status, "installed") == Some(true);
        let (label, tone) = if checking {
            ("Checking", ListItemStatus::Neutral)
        } else if cli_ready {
            ("Installed", ListItemStatus::Success)
        } else {
            ("Not installed", ListItemStatus::Warning)
        };
        let reason: SharedString = "CLI status is being checked.".into();
        let controls = vec![
            settings_button(
                p,
                "integrations-cli-repair",
                "Repair",
                Some(ICON_DOWNLOAD),
                ButtonVariant::Outline,
                checking,
                Some(reason.clone()),
                |page: &mut Self, _window, cx| page.post("installGhostexCli", cx),
                cx,
            ),
            settings_button(
                p,
                "integrations-cli-refresh",
                "Refresh",
                Some(ICON_REFRESH),
                ButtonVariant::Ghost,
                checking,
                Some(reason),
                |page: &mut Self, _window, cx| page.post("requestGhostexCliStatus", cx),
                cx,
            ),
        ];
        let row = integration_row(
            p,
            "ghostex-cli",
            Some(tone),
            Some(ICON_TERMINAL),
            RowTitle {
                link: None,
                label: "Command line tool".to_string(),
                description: format!(
                    "{label}. Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup. gx is linked when that alias is available and not taken by another command."
                ),
                badge: None,
                pill: None,
            },
            controls,
        );
        settings_section(p, "Ghostex CLI", None, None, vec![row]).map(IntoElement::into_any_element)
    }

    /// `TrycuaInstalledActions`.
    ///
    /// CDXC:Settings 2026-09-26 DECISION:
    /// An installed Trycua row gets one-click Update, an "up to date" state, Reinstall and Uninstall (which keeps the system permissions), as icon-only buttons so the Desktop control area stays quiet; versions live in the tooltips, not in row text.
    fn trycua_installed_actions(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let name = trycua_name();
        let current = text(status, "cuaDriverVersion");
        let latest = text(status, "cuaDriverLatestVersion");
        let installed_suffix = current
            .map(|current| format!(" (installed v{current})"))
            .unwrap_or_default();
        let job = trycua_job(status);
        let checking_reason = job
            .running_reason
            .clone()
            .unwrap_or_else(|| format!("{name} status is being checked."));
        let busy = checking || job.running;
        let running = |operations: &[&str]| {
            job.operation
                .as_deref()
                .is_some_and(|operation| operations.contains(&operation))
        };
        let mut actions = Vec::new();
        if flag(status, "cuaDriverManagedUpdatesSupported") == Some(true) {
            let (icon, color, tooltip, message) = match flag(status, "cuaDriverUpdateAvailable") {
                Some(true) => (
                    ICON_CIRCLE_ARROW_UP,
                    rgb(SKY_400),
                    match latest {
                        Some(latest) => format!("Update {name} to v{latest}{installed_suffix}"),
                        None => format!("Update {name}{installed_suffix}"),
                    },
                    "installCuaDriver",
                ),
                Some(false) => (
                    ICON_CIRCLE_CHECK,
                    p.muted,
                    format!(
                        "{name}{} is up to date. Click to check again.",
                        current
                            .or(latest)
                            .map(|version| format!(" v{version}"))
                            .unwrap_or_default()
                    ),
                    "checkCuaDriverUpdate",
                ),
                None => (
                    ICON_CLOUD_SEARCH,
                    p.foreground,
                    format!("Check for {name} updates{installed_suffix}"),
                    "checkCuaDriverUpdate",
                ),
            };
            actions.push(ghost_icon_button(
                p,
                "integrations-trycua-update",
                if running(&["update"]) {
                    ICON_LOADER
                } else {
                    icon
                },
                color,
                tooltip,
                busy,
                checking_reason.clone(),
                move |page, _window, cx| page.post(message, cx),
                cx,
            ));
        }
        actions.push(ghost_icon_button(
            p,
            "integrations-trycua-reinstall",
            if running(&["reinstall", "install"]) {
                ICON_LOADER
            } else {
                ICON_REFRESH
            },
            p.foreground,
            format!(
                "Reinstall the latest {name}{installed_suffix}. {}",
                job.plan.clone().unwrap_or_default()
            )
            .trim()
            .to_string(),
            busy || job.blocked_reason.is_some(),
            if busy {
                checking_reason.clone()
            } else {
                job.blocked_reason.clone().unwrap_or_default()
            },
            |page, _window, cx| page.post("reinstallCuaDriver", cx),
            cx,
        ));
        actions.push(ghost_icon_button(
            p,
            "integrations-trycua-uninstall",
            if running(&["uninstall"]) {
                ICON_LOADER
            } else {
                ICON_TRASH
            },
            p.foreground,
            format!("Uninstall {name} (keeps Accessibility and Screen Recording permissions)"),
            busy,
            checking_reason,
            |page, _window, cx| page.post("uninstallCuaDriver", cx),
            cx,
        ));
        actions
    }

    /// The `trycua/cua ↗` link after the product name, opening the repository in the browser.
    fn trycua_repository_link(&self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let (label, url) = trycua_repository();
        div()
            .id("integrations-trycua-repository")
            .flex_shrink_0()
            .cursor_pointer()
            .text_size(px(14.0))
            .line_height(px(18.9))
            .text_color(hsla(p.primary))
            .hover(|this| this.underline())
            .tooltip(tooltip_text(url.clone()))
            .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                post_store_message(
                    &page.store,
                    json!({ "type": "openExternalUrl", "url": url }),
                    cx,
                );
            }))
            .child(label)
            .into_any_element()
    }

    /// The Install skill button an installed driver shows until its Cua Driver skill is installed;
    /// the Agent skills list installs the same skill.
    fn cua_driver_skill_install_button(
        &mut self,
        p: &SettingsPalette,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = settings_button(
            p,
            "integrations-trycua-install-skill",
            "Install skill",
            Some(ICON_DOWNLOAD),
            ButtonVariant::Outline,
            disabled,
            Some("Fast Computer & Browser Use is busy.".into()),
            |page: &mut Self, _window, cx| page.post("installCuaDriverSkill", cx),
            cx,
        );
        if disabled {
            return button;
        }
        div()
            .id("integrations-trycua-install-skill-tooltip")
            .tooltip(tooltip_text(
                "Install the Cua Driver skill (cua-driver skills install) so your agents know the driver's commands.",
            ))
            .child(button)
            .into_any_element()
    }

    /// `DesktopControlSection`.
    fn desktop_control_section(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        show_trycua: bool,
        show_permissions: bool,
        show_spaceo: bool,
        show_spaceo_permissions: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !show_trycua && !show_permissions && !show_spaceo && !show_spaceo_permissions {
            return None;
        }
        let name = trycua_name();
        let installed = flag(status, "cuaDriverInstalled") == Some(true);
        let version = text(status, "cuaDriverVersion");
        let install_command = text(status, "cuaDriverInstallCommand").map(str::to_string);
        let job = trycua_job(status);
        let mut rows = Vec::new();
        if show_trycua {
            let controls = if installed {
                let mut controls = Vec::new();
                if flag(status, "cuaDriverSkillInstalled") == Some(false) {
                    controls.push(self.cua_driver_skill_install_button(
                        p,
                        checking || job.running,
                        cx,
                    ));
                }
                controls.extend(self.trycua_installed_actions(p, status, checking, cx));
                controls
            } else {
                let disabled = checking || job.running || job.blocked_reason.is_some();
                let reason = if checking {
                    format!("{name} status is being checked.")
                } else {
                    job.running_reason
                        .clone()
                        .or(job.blocked_reason.clone())
                        .unwrap_or_default()
                };
                let button = settings_button(
                    p,
                    "integrations-trycua-install",
                    if job.running {
                        format!("Installing {name}…")
                    } else {
                        format!("Install {name}")
                    },
                    Some(if job.running {
                        ICON_LOADER
                    } else {
                        ICON_DOWNLOAD
                    }),
                    ButtonVariant::Outline,
                    disabled,
                    Some(reason.into()),
                    |page: &mut Self, _window, cx| page.post("installCuaDriver", cx),
                    cx,
                );
                // The plan is the enabled button's tooltip; a disabled one shows its reason.
                vec![match job.plan.clone().filter(|_| !disabled) {
                    Some(plan) => div()
                        .id("integrations-trycua-install-plan")
                        .tooltip(tooltip_text(plan))
                        .child(button)
                        .into_any_element(),
                    None => button,
                }]
            };
            let installed_prefix = if installed {
                match version {
                    Some(version) => format!("Version {version} installed. "),
                    None => "Installed. ".to_string(),
                }
            } else {
                String::new()
            };
            rows.push(integration_row(
                p,
                "trycua",
                Some(if checking {
                    ListItemStatus::Neutral
                } else if installed {
                    ListItemStatus::Success
                } else {
                    ListItemStatus::Warning
                }),
                Some(ICON_DEVICE_DESKTOP),
                RowTitle {
                    link: Some(self.trycua_repository_link(p, cx)),
                    label: name.clone(),
                    description: format!(
                        "{installed_prefix}{name} is Trycua's open-source driver that lets any agent control your machine and browsers: clicking, typing, and seeing what is on screen. Its Cua Driver skill teaches agents the driver's commands, and Ghostex Computer Use and Ghostex Browser Use add Ghostex's own guidance on top."
                    ),
                    badge: None,
                    pill: None,
                },
                controls,
            ));
            if let Some(detail) = job.detail.clone() {
                let output = job.output.trim();
                rows.push(integration_row(
                    p,
                    "trycua-job",
                    None,
                    None,
                    RowTitle {
                        link: None,
                        label: detail,
                        description: if output.is_empty() {
                            job.plan.clone().unwrap_or_default()
                        } else {
                            output
                                .lines()
                                .rev()
                                .take(12)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect::<Vec<_>>()
                                .join("\n")
                        },
                        badge: None,
                        pill: None,
                    },
                    Vec::new(),
                ));
            }
        }
        if show_trycua
            && !installed
            && let Some(command) = install_command
        {
            let copied = self.copied == Some("integrations-trycua-copy-command");
            let command_for_copy = command.clone();
            let code = div()
                .flex_shrink_1()
                .min_w_0()
                .max_w(px(416.0))
                .px(px(10.0))
                .py(px(4.8))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.surface))
                .font_family(MODAL_MONO_FONT)
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(p.muted))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(command)
                .into_any_element();
            let copy = ghost_icon_button(
                p,
                "integrations-trycua-copy-command",
                if copied {
                    ICON_CIRCLE_CHECK_FILLED
                } else {
                    ICON_COPY
                },
                p.foreground,
                if copied { "Copied" } else { "Copy command" }.to_string(),
                false,
                String::new(),
                move |page, _window, cx| {
                    page.copy_command(
                        "integrations-trycua-copy-command",
                        command_for_copy.clone(),
                        cx,
                    )
                },
                cx,
            );
            rows.push(integration_row(
                p,
                "trycua-install-command",
                None,
                None,
                RowTitle {
                    link: None,
                    label: "Install command".to_string(),
                    description: format!(
                        "Install {name} runs this command in the background and shows its progress here. You can also run it yourself."
                    ),
                    badge: None,
                    pill: None,
                },
                vec![code, copy],
            ));
        }
        if show_permissions {
            let (permission, tone) = cua_permission_status(status, checking);
            let controls = vec![
                settings_button(
                    p,
                    "integrations-accessibility",
                    "Accessibility",
                    None,
                    ButtonVariant::Ghost,
                    false,
                    None,
                    |page: &mut Self, _window, cx| {
                        post_store_message(
                            &page.store,
                            json!({ "type": "openAccessibilityPreferences" }),
                            cx,
                        );
                    },
                    cx,
                ),
                settings_button(
                    p,
                    "integrations-screen-recording",
                    "Screen Recording",
                    None,
                    ButtonVariant::Ghost,
                    false,
                    None,
                    |page: &mut Self, _window, cx| {
                        post_store_message(
                            &page.store,
                            json!({ "type": "openScreenRecordingPreferences" }),
                            cx,
                        );
                    },
                    cx,
                ),
            ];
            rows.push(integration_row(
                p,
                "system-permissions",
                Some(tone),
                Some(ICON_SETTINGS),
                RowTitle {
                    link: None,
                    label: "System permissions".to_string(),
                    description: format!(
                        "{permission}. {name} needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop."
                    ),
                    badge: None,
                    pill: None,
                },
                controls,
            ));
        }
        rows.extend(self.spaceo_rows(
            p,
            status,
            checking,
            show_spaceo,
            show_spaceo_permissions,
            cx,
        ));
        settings_section(p, "Desktop control", None, None, rows).map(IntoElement::into_any_element)
    }

    /// `AgentSkillsSection`.
    ///
    /// CDXC:Settings 2026-09-09 DECISION:
    /// User picked the "quiet rows" mockup (docs/2026-09-09/integrations-settings) for the Integrations page: Trycua is its own Desktop control section of plain rows, skills are one row each with the description and install command behind the hover info icon, Recommended and Optional are caption rows inside one card, and Refresh plus Uninstall all live in the section header.
    fn skills_section(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        checking: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let cli_ready = flag(status, "installed") == Some(true);
        let driver_installed = flag(status, "cuaDriverInstalled") == Some(true);
        let skills = visible_skills();
        let any_installed = skills
            .iter()
            .any(|skill| skill_installed(&skill.id, status));
        let actions = h_flex()
            .items_center()
            .gap(px(8.0))
            .child(settings_button(
                p,
                "integrations-skills-refresh",
                "Refresh",
                Some(ICON_REFRESH),
                ButtonVariant::Ghost,
                checking,
                Some("Skill status is being checked.".into()),
                |page: &mut Self, _window, cx| page.post("requestGhostexCliStatus", cx),
                cx,
            ))
            .child(settings_button(
                p,
                "integrations-skills-uninstall-all",
                "Uninstall all",
                Some(ICON_TRASH),
                ButtonVariant::Ghost,
                checking || !any_installed,
                Some(
                    if checking {
                        "Skill status is being checked."
                    } else {
                        "No bundled Ghostex skills are installed."
                    }
                    .into(),
                ),
                |page: &mut Self, _window, cx| page.post("uninstallBundledAgentSkills", cx),
                cx,
            ))
            .into_any_element();
        let mut groups: Vec<AnyElement> = Vec::new();
        for (tier, tier_label) in [("recommended", "Recommended"), ("optional", "Optional")] {
            let tier_skills: Vec<&Skill> =
                skills.iter().filter(|skill| skill.tier == tier).collect();
            if tier_skills.is_empty() {
                continue;
            }
            let hairline = hsla(p.hairline);
            let rows = tier_skills.into_iter().enumerate().map(|(index, skill)| {
                let row =
                    self.skill_row(p, skill, status, checking, cli_ready, driver_installed, cx);
                div()
                    .w_full()
                    .when(index > 0, |this| this.border_t_1().border_color(hairline))
                    .child(row)
            });
            groups.push(
                v_flex()
                    .w_full()
                    .child(
                        div()
                            .px(px(20.0))
                            .pt(px(12.0))
                            .pb(px(6.0))
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.muted))
                            .child(tier_label),
                    )
                    .children(rows)
                    .into_any_element(),
            );
        }
        settings_section(p, "Agent skills", None, Some(actions), groups)
            .map(IntoElement::into_any_element)
    }

    /// `AgentSkillRow`.
    #[allow(clippy::too_many_arguments)]
    fn skill_row(
        &mut self,
        p: &SettingsPalette,
        skill: &Skill,
        status: Option<&Value>,
        checking: bool,
        cli_ready: bool,
        driver_installed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let installed = skill_installed(&skill.id, status);
        let needs_trycua = skill.requires_cua_driver && !driver_installed && !checking;
        let needs_spaceo =
            skill.requires_spaceo && flag(status, "spaceoInstalled") != Some(true) && !checking;
        let install_message = skill_install_message(&skill.id);
        let install_disabled = checking || !cli_ready || install_message.is_none();
        let install_reason = if checking {
            "Skill status is being checked."
        } else if !cli_ready {
            "Install or repair the Ghostex CLI first."
        } else {
            "Skill installation isn’t available here."
        };
        let mut install = settings_button(
            p,
            SharedString::from(format!("integrations-skill-install-{}", skill.id)),
            if installed { "Reinstall" } else { "Install" },
            Some(if installed {
                ICON_REFRESH
            } else {
                ICON_DOWNLOAD
            }),
            if installed {
                ButtonVariant::Ghost
            } else {
                ButtonVariant::Outline
            },
            install_disabled,
            Some(install_reason.into()),
            move |page: &mut Self, _window, cx| {
                if let Some(message) = install_message {
                    page.post(message, cx);
                }
            },
            cx,
        );
        if (needs_trycua || needs_spaceo) && !installed && !install_disabled {
            install = div().opacity(0.6).child(install).into_any_element();
        }
        let mut controls = vec![install];
        if installed {
            let skill_id = skill.id.clone();
            controls.push(ghost_icon_button(
                p,
                SharedString::from(format!("integrations-skill-uninstall-{}", skill.id)),
                ICON_TRASH,
                p.foreground,
                format!("Uninstall {}", skill.name),
                checking,
                "Skill status is being checked.".to_string(),
                move |page, _window, cx| {
                    page.post_message(
                        json!({ "skillId": skill_id, "type": "uninstallBundledAgentSkill" }),
                        cx,
                    );
                },
                cx,
            ));
        }
        integration_row(
            p,
            &format!("skill-{}", skill.id),
            Some(if installed {
                ListItemStatus::Success
            } else {
                ListItemStatus::Neutral
            }),
            Some(skill_icon(&skill.id)),
            RowTitle {
                link: (installed && !skill.skill_name.is_empty())
                    .then(|| skill_invocation(p, &skill.skill_name)),
                label: skill.name.clone(),
                description: format!("{}\n\n{}", skill.description, skill.command),
                badge: None,
                pill: if needs_trycua {
                    Some(format!("Needs {}", trycua_name()))
                } else if needs_spaceo {
                    Some(format!("Needs {}", spaceo::spaceo_name()))
                } else {
                    None
                },
            },
            controls,
        )
    }
}

impl IntegrationsTab {
    /// Floating Capture: the floating button, its hotkeys and its screenshot tools.
    ///
    /// CDXC:GhostexCapture 2026-09-30 DECISION:
    /// User: call it "Floating Capture", put it at the very top of Integrations with text under it that explains what it does, and do not repeat the same name as both the section title and the toggle.
    fn ghostex_capture_section(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let enabled = self.store.read(cx).values().bool("ghostexCaptureEnabled");
        let switch_to_session = self
            .store
            .read(cx)
            .values()
            .bool("ghostexCaptureSwitchToSession");
        let hotkey = if cfg!(target_os = "macos") {
            "Cmd+Ctrl+Shift+S"
        } else {
            "Alt+Ctrl+Shift+S"
        };
        let mut rows = vec![integration_row(
            p,
            "ghostex-capture",
            Some(if enabled {
                ListItemStatus::Success
            } else {
                ListItemStatus::Neutral
            }),
            Some(ICON_DEVICE_DESKTOP),
            RowTitle {
                link: None,
                label: "Show the floating button".to_string(),
                description: format!(
                    "{hotkey} opens the button's panel. With the same keys, A captures an area, Space the current app, F the full screen, and T writes a prompt, straight away."
                ),
                badge: Some("Beta"),
                pill: None,
            },
            vec![switch_control(
                p,
                "ghostex-capture-enabled",
                "Show the floating button",
                enabled,
                false,
                None,
                |page: &mut Self, checked, _window, cx| {
                    save(page, "ghostexCaptureEnabled", json!(checked), cx)
                },
                cx,
            )],
        )];
        rows.push(integration_row(
            p,
            "ghostex-capture-switch-to-session",
            Some(if switch_to_session {
                ListItemStatus::Success
            } else {
                ListItemStatus::Neutral
            }),
            Some(ICON_DEVICE_DESKTOP),
            RowTitle {
                link: None,
                label: "Switch to the session after sending".to_string(),
                description: "After a prompt is sent, the Ghostex window shows the session it went to, without coming in front of the app you are in.".to_string(),
                badge: None,
                pill: None,
            },
            vec![switch_control(
                p,
                "ghostex-capture-switch-to-session",
                "Switch to the session after sending",
                switch_to_session,
                false,
                None,
                |page: &mut Self, checked, _window, cx| {
                    save(page, "ghostexCaptureSwitchToSession", json!(checked), cx)
                },
                cx,
            )],
        ));
        settings_section(
            p,
            "Floating Capture",
            Some(
                "A small button that floats over every app and shows how many agents are working, waiting for you, or asking a question. Use it to screenshot an area, an app or the whole screen, mark it up, and send a prompt to any project or session without switching to Ghostex."
                    .into(),
            ),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }
}

fn save(
    page: &mut IntegrationsTab,
    key: &'static str,
    value: Value,
    cx: &mut Context<IntegrationsTab>,
) {
    let store = page.store.clone();
    store.update(cx, |store, cx| store.update_setting(key, value, cx));
}

impl Render for IntegrationsTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching, status) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Integrations),
                matching,
                store.host_payload("ghostexCliStatus").cloned(),
            )
        };
        let status = status.as_ref();
        let checking = self.loading || status.is_none();
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Integrations,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        let section = "integrations";
        if search.section(section).has_visible() {
            if search.row_visible(section, "ghostexCapture") {
                blocks.extend(
                    self.ghostex_capture_section(&p, cx)
                        .map(|element| PageBlock::section("ghostexCapture", element)),
                );
            }
            if search.row_visible(section, "ghostexCli") {
                blocks.extend(
                    self.cli_section(&p, status, checking, cx)
                        .map(|element| PageBlock::section("ghostexCli", element)),
                );
            }
            let show_trycua = search.row_visible(section, "bundledAgentSkills");
            // Accessibility and Screen Recording are macOS grants; Fast Computer & Browser Use needs none on Windows or Linux.
            let show_permissions =
                cfg!(target_os = "macos") && search.row_visible(section, "cuaPermissions");
            // SpaceO runs only on Apple Silicon Macs with macOS 14 or later; the status says when this one can't.
            let spaceo_possible =
                cfg!(target_os = "macos") && flag(status, "spaceoSupported") != Some(false);
            let show_spaceo = spaceo_possible && search.row_visible(section, "spaceo");
            let show_spaceo_permissions =
                spaceo_possible && search.row_visible(section, "spaceoPermissions");
            blocks.extend(
                self.desktop_control_section(
                    &p,
                    status,
                    checking,
                    show_trycua,
                    show_permissions,
                    show_spaceo,
                    show_spaceo_permissions,
                    cx,
                )
                .map(|element| PageBlock::section("desktopControl", element)),
            );
            if show_trycua {
                blocks.extend(
                    self.skills_section(&p, status, checking, cx)
                        .map(|element| PageBlock::section("agentSkills", element)),
                );
            }
            // CDXC:Settings 2026-09-30 DECISION: User: "please move the tools list to the bottom of the integrations".
            if search.row_visible(section, "managedTools") {
                blocks.extend(
                    self.managed_tools_section(&p, cx)
                        .map(|element| PageBlock::section("managedTools", element)),
                );
            }
        }
        settings_page(&self.store, SettingsTabId::Integrations, &p, blocks, cx)
    }
}
