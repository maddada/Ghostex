//! Native GPUI Onboarding: the five-panel first-run flow (Welcome, Agents, Workspace, Mobile, Get
//! started) and its "You're set" screen, the desktop twin of the React `OnboardingModal` in
//! packages/core-ui/onboarding/.
//!
//! CDXC:AppModal 2026-09-28 DECISION:
//! User: migrate every React modal to GPUI with GPUI-Kit components, looking and working exactly as
//! now, so the app runs without CEF. The onboarding keeps its 1672x941 stage scaled to the window,
//! its WebGL-look backgrounds (backdrop.rs), animated demos, arrow-key paging, own toast and popups,
//! and talks to the app through `OnboardingCommand` instead of the modal host's bridge messages.
//! SEE-ALSO: packages/core-ui/onboarding/ (the React twin and its CDXC:Onboarding decisions), apps/desktop/src/app/onboarding_modal_lifecycle.rs (open, first-run gating, host commands), apps/desktop/src/bin/native_modal_demo/onboarding.rs (standalone preview), apps/desktop/src/assets/onboarding.rs (icons and pictures).
mod agent_cli;
mod agents;
mod backdrop;
mod extension;
mod finished;
mod folder_chooser;
pub(crate) mod fonts;
mod get_started;
mod install_guide;
mod interact;
mod intro_video;
mod intro_web_view;
mod mobile;
pub(crate) mod model;
mod primitives;
mod scan_log;
mod stage;
mod text;
mod welcome;
mod workspace;
mod workspace_views;

pub(crate) use agent_cli::AgentCliRequest;
pub(crate) use model::{CatalogAgent, OnboardingSettings, ThemeSwatchPreset, ThemeTable};

use agent_cli::CliRows;
use agents::AgentsPanelState;
use finished::FinishedState;
use get_started::GetStartedState;
use gpui::StyledImage as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    KeyDownEvent, ObjectFit, ParentElement as _, Render, RenderImage, Styled as _, Window, div,
    img, px,
};
use mobile::MobileState;
use model::{ComputerUseState, DetectedAgent, FlowState};
use primitives::*;
use serde_json::{Map, Value};
use stage::*;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use welcome::WelcomeState;
use workspace::WorkspaceState;

/// CDXC:Onboarding 2026-09-11 WHY:
/// The onboarding renders a 1672x941 stage scaled to fit its window, so the frame keeps that aspect ratio at a size that still fits a 1440x900 screen with the menu bar and Dock.
pub(crate) const ONBOARDING_MODAL_WIDTH: f32 = 1400.0;
pub(crate) const ONBOARDING_MODAL_HEIGHT: f32 = 788.0;

/// Panel shown when the window opens (1-5), or the "You're set" screen (the preview binary's states).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum InitialPanel {
    Panel(usize),
    Finished,
}

/// What the app opens the onboarding with.
pub(crate) struct OnboardingConfig {
    /// The automatic first-run open: turns Browser + Files on once (CDXC:Onboarding 2026-09-11 DECISION in contract.ts).
    pub(crate) first_run: bool,
    /// The sidebar already lists a project: the last panel ends with Finish and the window may close.
    pub(crate) has_projects: bool,
    pub(crate) settings: OnboardingSettings,
    pub(crate) catalog: Vec<CatalogAgent>,
    pub(crate) theme: ThemeTable,
    /// The system appearance, for which colours the look card shows first under System.
    pub(crate) system_light: bool,
    /// gxserver's agent CLI jobs are reachable; without it missing agents offer the Install guide only.
    pub(crate) cli_available: bool,
    pub(crate) initial_panel: InitialPanel,
    pub(crate) picked_folder: Option<String>,
    /// Open on the intro video page before Welcome (the automatic first run, once per install).
    pub(crate) intro_video: bool,
    /// Why this computer cannot show agent alerts, if it is known (Linux without notify-send,
    /// Windows with Ghostex's notifications off).
    pub(crate) notification_problem: Option<String>,
}

/// Where the flow goes once it closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FinishTarget {
    /// Close only.
    None,
    /// Settings, optionally on one of its pages (`theme`).
    Settings(Option<&'static str>),
    /// Settings -> Remote -> Easy Connect ("Configure my phone after this flow").
    RemoteSettings,
}

/// Commands the onboarding sends to the app. Each maps to the bridge message the React page posted.
#[derive(Clone, Debug)]
pub(crate) enum OnboardingCommand {
    /// `updateSettingsPatch` with `source: firstLaunch:preferences`.
    UpdateSettings(Map<String, Value>),
    /// `requestAgentHookStatus`.
    RescanAgents,
    /// `requestGhostexCliStatus`.
    RequestCliStatus,
    /// `installAgentHooks`.
    InstallAgentHooks(Vec<String>),
    /// `installCuaDriver` when the driver is missing, then `installComputerUseSkill`.
    InstallComputerUse {
        install_driver: bool,
    },
    OpenAccessibilityPreferences,
    OpenScreenRecordingPreferences,
    /// `installBrowserUseSkill`.
    InstallBrowserSkill,
    /// `uninstallBundledAgentSkill` for `browserUse`.
    UninstallBrowserSkill,
    OpenExternalUrl(String),
    /// `pickFirstLaunchProjectFolder`.
    PickProjectFolder,
    /// `firstLaunchCreateProjectSession`.
    FinishFirstLaunch {
        request_id: String,
        agent_id: String,
        path: String,
    },
    /// `completeFirstLaunchSetup`, then open `FinishTarget`.
    Finish(FinishTarget),
    /// Close without a project: setup is not marked finished, so it comes back on the next launch
    /// while there is still no project, and Add Project opens so the app is never an empty shell.
    Leave,
    /// The user left the intro video page: it is not shown again.
    IntroVideoSeen,
    /// `/api/agentCliMaintenance` for one agent.
    AgentCli {
        request_id: u64,
        agent_id: String,
        request: AgentCliRequest,
    },
}

pub(crate) type OnboardingHost = Rc<dyn Fn(OnboardingCommand, &mut App)>;

const KEY_CONTEXT: &str = "GxOnboarding";
gpui::actions!(
    gx_onboarding,
    [FocusNext, FocusPrevious, Activate, ActivateSpace]
);

/// A cross-faded pair of backdrop frames.
struct FrameSet {
    previous: Option<Arc<RenderImage>>,
    current: Option<Arc<RenderImage>>,
    swapped_at: Instant,
    fade: Duration,
}

impl FrameSet {
    fn new(fade: Duration) -> Self {
        Self {
            previous: None,
            current: None,
            swapped_at: Instant::now(),
            fade,
        }
    }

    /// Takes a new frame and hands back the one that can leave the sprite atlas.
    fn push(&mut self, image: Arc<RenderImage>, now: Instant) -> Option<Arc<RenderImage>> {
        let retired = self.previous.take();
        self.previous = self.current.take();
        self.current = Some(image);
        self.swapped_at = now;
        retired
    }

    fn layers(&self, now: Instant) -> Vec<(Arc<RenderImage>, f32)> {
        let mut layers = Vec::new();
        let t = (now.saturating_duration_since(self.swapped_at).as_secs_f32()
            / self.fade.as_secs_f32().max(1e-3))
        .clamp(0.0, 1.0);
        if let Some(previous) = &self.previous
            && t < 1.0
        {
            layers.push((previous.clone(), 1.0));
        }
        if let Some(current) = &self.current {
            layers.push((
                current.clone(),
                if self.previous.is_some() { t } else { 1.0 },
            ));
        }
        layers
    }

    fn drain(&mut self) -> Vec<Arc<RenderImage>> {
        [self.previous.take(), self.current.take()]
            .into_iter()
            .flatten()
            .collect()
    }
}

struct Toast {
    message: String,
    shown_at: Instant,
    /// A failure or warning: drawn with a warning mark instead of the check.
    warning: bool,
}

/// The browser skill switch while its install or removal runs.
#[derive(Clone, Copy)]
struct BrowserSkillPending {
    /// Installed (true) or removed (false) once the run ends.
    target: bool,
    deadline: Instant,
}

pub(crate) struct GpuiOnboardingWindow {
    host: OnboardingHost,
    focus_handle: FocusHandle,
    first_run: bool,
    has_projects: bool,
    catalog: Vec<CatalogAgent>,
    theme: model::ThemeTable,
    system_light: bool,
    cli_available: bool,
    notification_problem: Option<String>,
    settings: OnboardingSettings,
    panel: usize,
    flow: FlowState,
    /// When the current panel (or the finished screen) mounted: the scene's fade-in and every
    /// demo clock count from here.
    scene_started: Instant,
    opened_at: Instant,
    transitions: Transitions,
    toast: Option<Toast>,
    agents: Vec<DetectedAgent>,
    /// Bumped with every new detection payload, the way React sees a new `agents` array.
    agents_revision: u64,
    agents_loading: bool,
    hook_status: Option<Value>,
    cli_status: Option<Value>,
    computer_use_install_requested: bool,
    computer_use_install_deadline: Option<Instant>,
    browser_skill_pending: Option<BrowserSkillPending>,
    picked_folder: Option<String>,
    cli_rows: CliRows,
    next_request_id: u64,
    welcome: WelcomeState,
    agents_panel: AgentsPanelState,
    workspace: WorkspaceState,
    mobile: MobileState,
    get_started: GetStartedState,
    finished: FinishedState,
    /// `MoreMenu` open on the Workspace preview.
    menu_open: bool,
    workspace_menu_opened: Instant,
    /// When a press outside the demo workspace's ⋯ menu closed it: that press's click on the ⋯
    /// button is the closing half of a toggle and must not reopen it.
    workspace_menu_dismissed: Option<Instant>,
    veil: FrameSet,
    nebula: FrameSet,
    vignette: Option<Arc<RenderImage>>,
    grain: Option<Arc<RenderImage>>,
    closed: bool,
    /// Keyboard focus: which control holds it, the tab order and each control's action.
    focus: interact::FocusState,
    /// The Install guide list's scroll position, for its scrollbar.
    guide_scroll: gpui::ScrollHandle,
    /// The intro video page, while it is the page shown (intro_video.rs).
    intro: Option<intro_video::IntroVideo>,
    /// Get started's own folder chooser, open when the system folder dialog could not open.
    folder_chooser: Option<folder_chooser::FolderChooser>,
}

impl GpuiOnboardingWindow {
    pub(crate) fn new(
        config: OnboardingConfig,
        host: OnboardingHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        fonts::register(cx);
        // Tab / Enter / Space are bound in the onboarding's own key context so they outrank any
        // app-wide binding for the same keys (the window root's focus cycling).
        static KEYS: std::sync::Once = std::sync::Once::new();
        KEYS.call_once(|| {
            cx.bind_keys([
                gpui::KeyBinding::new("tab", FocusNext, Some(KEY_CONTEXT)),
                gpui::KeyBinding::new("shift-tab", FocusPrevious, Some(KEY_CONTEXT)),
                gpui::KeyBinding::new("enter", Activate, Some(KEY_CONTEXT)),
                gpui::KeyBinding::new("space", ActivateSpace, Some(KEY_CONTEXT)),
            ]);
        });
        interact::reset();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let now = Instant::now();
        let (panel, finished) = match config.initial_panel {
            InitialPanel::Panel(panel) => (panel.clamp(1, PANEL_COUNT), false),
            InitialPanel::Finished => (PANEL_COUNT, true),
        };
        let vignette = backdrop::render_vignette();
        let intro_video = config.intro_video;
        let mut this = Self {
            host,
            focus_handle,
            first_run: config.first_run,
            has_projects: config.has_projects,
            catalog: config.catalog,
            theme: config.theme,
            system_light: config.system_light,
            cli_available: config.cli_available,
            notification_problem: config.notification_problem,
            settings: config.settings,
            panel,
            flow: FlowState {
                finished,
                ..FlowState::default()
            },
            scene_started: now,
            opened_at: now,
            transitions: Transitions::default(),
            toast: None,
            agents: Vec::new(),
            agents_revision: 0,
            agents_loading: false,
            hook_status: None,
            cli_status: None,
            computer_use_install_requested: false,
            computer_use_install_deadline: None,
            browser_skill_pending: None,
            picked_folder: config.picked_folder,
            cli_rows: CliRows::default(),
            next_request_id: 1,
            welcome: WelcomeState::new(now),
            agents_panel: AgentsPanelState::new(now),
            workspace: WorkspaceState::new(now),
            mobile: MobileState::new(now),
            get_started: GetStartedState::default(),
            finished: FinishedState::new(now),
            menu_open: false,
            workspace_menu_opened: now,
            workspace_menu_dismissed: None,
            veil: FrameSet::new(Duration::from_millis(1000 / 12)),
            nebula: FrameSet::new(Duration::from_millis(500)),
            vignette: pixels_to_image(vignette),
            grain: None,
            closed: false,
            focus: interact::FocusState::default(),
            guide_scroll: gpui::ScrollHandle::new(),
            intro: None,
            folder_chooser: None,
        };
        this.get_started.reset(&this.settings, this.system_light);
        // CDXC:Onboarding 2026-09-11 DECISION:
        // User: "if it's first run ever browser/docs is actually applied as default enabled; for regular current users if they revisit the onboarding by clicking on the setup button in the tips dropdown this shouldn't happen."
        if this.first_run {
            let patch = model::views_patch(&[
                (model::ViewKey::Browser, true),
                (model::ViewKey::Docs, true),
                (model::ViewKey::Code, false),
                (model::ViewKey::Kanban, false),
                (model::ViewKey::Automate, false),
            ]);
            this.update_settings(patch, cx);
        }
        // The page only exposes a rescan, so detection and the CLI status are asked for on open.
        this.agents_loading = true;
        this.send(OnboardingCommand::RescanAgents, cx);
        this.send(OnboardingCommand::RequestCliStatus, cx);
        this.mount_panel(now, cx);
        this.start_backdrop(cx);
        if intro_video {
            this.start_intro_video();
        }
        this
    }

    fn send(&self, command: OnboardingCommand, cx: &mut App) {
        (self.host)(command, cx);
    }

    /// Writes a settings patch and shows it at once (the host echoes the saved settings back).
    fn update_settings(&mut self, patch: Map<String, Value>, cx: &mut Context<Self>) {
        self.settings.apply_patch(&patch);
        self.send(OnboardingCommand::UpdateSettings(patch), cx);
        cx.notify();
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }

    /// `go(next)`: switch panel, leave the finished screen.
    fn go(&mut self, next: usize, cx: &mut Context<Self>) {
        let next = next.clamp(1, PANEL_COUNT);
        let changed = next != self.panel || self.flow.finished;
        self.panel = next;
        self.flow.finished = false;
        if changed {
            self.scene_started = Instant::now();
            self.menu_open = false;
            self.mount_panel(self.scene_started, cx);
            self.focus.anchor.set(interact::TabAnchor::PanelStart);
        }
        cx.notify();
    }

    /// Resets the local state of the panel that is now shown, the way React remounts it.
    fn mount_panel(&mut self, now: Instant, cx: &mut Context<Self>) {
        self.cli_rows.clear();
        if self.flow.finished && self.panel == PANEL_COUNT {
            self.finished = FinishedState::new(now);
            return;
        }
        match self.panel {
            1 => self.welcome = WelcomeState::new(now),
            2 => self.mount_agents_panel(now, cx),
            3 => self.workspace = WorkspaceState::new(now),
            4 => self.mobile = MobileState::new(now),
            _ => self.get_started.reset(&self.settings, self.system_light),
        }
    }

    fn show_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.toast = Some(Toast {
            message: message.into(),
            shown_at: Instant::now(),
            warning: false,
        });
        cx.notify();
    }

    fn show_warning_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.toast = Some(Toast {
            message: message.into(),
            shown_at: Instant::now(),
            warning: true,
        });
        cx.notify();
    }

    /// An app toast raised while the onboarding is open (an install's progress or outcome).
    ///
    /// CDXC:Onboarding 2026-10-28 WHY:
    /// App toasts sit at the bottom centre of the main window, and the onboarding is centred over
    /// that window, so on a screen the onboarding nearly fills they covered its Next and Open
    /// Ghostex buttons. While the onboarding is open they show in its own toast at the top of the
    /// stage instead.
    pub(crate) fn receive_app_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        cx: &mut Context<Self>,
    ) {
        let title = title.trim();
        let description = description.trim();
        let message = if description.is_empty() || description == title {
            title.to_string()
        } else {
            format!("{title}: {description}")
        };
        if matches!(level, "warning" | "error") {
            self.show_warning_toast(message, cx);
        } else {
            self.show_toast(message, cx);
        }
    }

    /// `finishOnboarding`: record completion, close, and open Remote settings when the phone step was queued.
    fn finish(&mut self, target: FinishTarget, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        let target = if target == FinishTarget::None && self.flow.phone_queued {
            FinishTarget::RemoteSettings
        } else {
            target
        };
        self.send(OnboardingCommand::Finish(target), cx);
    }

    /// Closing without a project (the window's close control, or skipping setup on Get started).
    ///
    /// CDXC:Onboarding 2026-10-28 WHY:
    /// Skipping setup with no project used to record setup as finished and drop the user into an
    /// empty app, and the window had no close control until a project existed. Leaving now keeps
    /// setup unfinished (it returns on the next launch while there is still no project) and opens
    /// Add Project, which works from a typed path everywhere; only finishing with a project, or
    /// closing once one exists, counts as done.
    fn leave(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.send(OnboardingCommand::Leave, cx);
    }

    /// The computer's appearance, pushed whenever it changes. On Linux the desktop portal answers
    /// after the window may already be open, so the value read at open can be the light default.
    pub(crate) fn set_system_light(&mut self, system_light: bool, cx: &mut Context<Self>) {
        if self.system_light == system_light {
            return;
        }
        self.system_light = system_light;
        self.get_started
            .system_appearance_changed(&self.settings, system_light);
        cx.notify();
    }

    pub(crate) fn has_projects(&self) -> bool {
        self.has_projects
    }

    /// The window's close control: finishes setup once a project exists, otherwise leaves it.
    pub(crate) fn close_from_chrome(&mut self, cx: &mut Context<Self>) {
        if self.has_projects {
            self.finish(FinishTarget::None, cx);
        } else {
            self.leave(cx);
        }
    }

    fn popup_open(&self) -> bool {
        self.agents_panel.guide_open
            || self.finished.follow_up.is_some()
            || self.folder_chooser.is_some()
    }

    fn close_popup(&mut self, cx: &mut Context<Self>) {
        self.folder_chooser = None;
        if self.panel == 2 && !self.flow.finished && self.agents_panel.guide_open {
            self.agents_panel.guide_open = false;
            self.cli_rows.clear_lazy();
        }
        if self.flow.finished && self.finished.follow_up.is_some() {
            self.finished_popup_later(cx);
        }
        cx.notify();
    }

    fn computer_use_state(&self) -> ComputerUseState {
        model::computer_use_state(
            self.cli_status.as_ref(),
            self.computer_use_install_requested,
        )
    }

    fn browser_skill_installed(&self) -> bool {
        self.cli_status
            .as_ref()
            .and_then(|status| status.get("browserSkillInstalled"))
            .and_then(Value::as_bool)
            == Some(true)
    }

    /// What the browser skill switch shows: where a running install or removal is headed, or
    /// what is installed.
    fn browser_skill_shown_on(&self) -> bool {
        self.browser_skill_pending
            .map_or_else(|| self.browser_skill_installed(), |pending| pending.target)
    }

    fn browser_skill_busy(&self) -> bool {
        self.browser_skill_pending.is_some()
    }

    /// The status shows the install or removal landed.
    fn settle_browser_skill(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.browser_skill_pending else {
            return;
        };
        if self.browser_skill_installed() != pending.target {
            return;
        }
        self.browser_skill_pending = None;
        self.show_toast(
            if pending.target {
                "Browser skill added: agents can open, click and read pages in the Browser view."
            } else {
                "Browser skill removed."
            },
            cx,
        );
    }

    // Host replies ---------------------------------------------------------------------------

    /// An `agentHookStatus` payload (a partial of the progressive walk, the final one, or an install reply).
    pub(crate) fn receive_agent_hook_status(&mut self, payload: Value, cx: &mut Context<Self>) {
        let complete = payload.get("complete").and_then(Value::as_bool) != Some(false);
        let agents = model::detected_agents_from_hook_status(&payload, &self.catalog);
        self.hook_status = Some(payload);
        let previous_loading = self.agents_loading;
        self.agents = agents;
        self.agents_revision += 1;
        if complete {
            self.agents_loading = false;
        }
        self.after_agents_changed(previous_loading, cx);
        cx.notify();
    }

    /// A `ghostexCliStatus` payload.
    pub(crate) fn receive_cli_status(&mut self, payload: Value, cx: &mut Context<Self>) {
        let before = self.computer_use_state();
        let installed = |key: &str| payload.get(key).and_then(Value::as_bool) == Some(true);
        if installed("cuaDriverInstalled") && installed("computerUseSkillInstalled") {
            self.computer_use_install_requested = false;
            self.computer_use_install_deadline = None;
        }
        self.cli_status = Some(payload);
        self.after_computer_use_changed(before, cx);
        self.settle_browser_skill(cx);
        cx.notify();
    }

    /// A `settingsActionStatus` payload: the Computer Use install's outcome.
    pub(crate) fn receive_settings_action_status(
        &mut self,
        payload: &Value,
        cx: &mut Context<Self>,
    ) {
        let action = payload.get("action").and_then(Value::as_str);
        let available = payload.get("available").and_then(Value::as_bool) == Some(true);
        let message = payload
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|message| !message.is_empty());
        // The browser skill and Computer Use run quietly (no app toast): a failure is said here.
        if matches!(
            action,
            Some("installBrowserUseSkill" | "uninstallBundledAgentSkill")
        ) && self.browser_skill_pending.is_some()
        {
            if !available {
                self.browser_skill_pending = None;
                self.show_warning_toast(
                    message.unwrap_or("The browser skill could not be changed."),
                    cx,
                );
            }
            cx.notify();
            return;
        }
        if !self.computer_use_install_requested {
            return;
        }
        if action == Some("installComputerUseSkill") && !available {
            self.show_warning_toast(
                message.unwrap_or("Computer Use could not be installed."),
                cx,
            );
        }
        let before = self.computer_use_state();
        match action {
            // The Trycua installer's exit; a failure ends "installing" (the app already toasts it).
            Some("installCuaDriver") if !available => {
                self.computer_use_install_requested = false;
                self.computer_use_install_deadline = None;
            }
            Some("installComputerUseSkill") => {
                self.computer_use_install_deadline = None;
                if !available {
                    self.computer_use_install_requested = false;
                }
            }
            _ => return,
        }
        self.after_computer_use_changed(before, cx);
        cx.notify();
    }

    /// `firstLaunchProjectFolderPicked`.
    pub(crate) fn receive_picked_folder(&mut self, path: String, cx: &mut Context<Self>) {
        let path = path.trim().to_string();
        if !path.is_empty() {
            self.picked_folder = Some(path);
            cx.notify();
        }
    }

    /// `firstLaunchCreateProjectSessionResult`.
    pub(crate) fn receive_finish_result(
        &mut self,
        request_id: &str,
        ok: bool,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.get_started_finish_result(request_id, ok, error, cx);
    }

    /// Saved settings, echoed back after a write or changed elsewhere.
    pub(crate) fn receive_settings(
        &mut self,
        settings: OnboardingSettings,
        cx: &mut Context<Self>,
    ) {
        if self.settings != settings {
            self.settings = settings;
            cx.notify();
        }
    }

    pub(crate) fn set_has_projects(&mut self, has_projects: bool, cx: &mut Context<Self>) {
        if self.has_projects != has_projects {
            self.has_projects = has_projects;
            cx.notify();
        }
    }

    /// An `/api/agentCliMaintenance` answer.
    pub(crate) fn receive_agent_cli_result(
        &mut self,
        request_id: u64,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        self.handle_cli_result(request_id, result, cx);
        cx.notify();
    }

    // Time ------------------------------------------------------------------------------------

    /// Runs everything that waits on the clock: the scan log reveal, the "Connected" hold, the
    /// agent CLI polls, the toast, and the finished screen's follow-ups.
    fn tick(&mut self, now: Instant, cx: &mut Context<Self>) {
        if let Some(toast) = &self.toast
            && now.saturating_duration_since(toast.shown_at)
                >= Duration::from_millis(if toast.warning {
                    TOAST_MS * 3
                } else {
                    TOAST_MS
                })
        {
            self.toast = None;
        }
        if let Some(pending) = self.browser_skill_pending
            && now >= pending.deadline
        {
            self.browser_skill_pending = None;
        }
        if let Some(deadline) = self.computer_use_install_deadline
            && now >= deadline
        {
            // CDXC:Onboarding 2026-09-28 WHY: the React host rejected the skill install after 150 s without an answer; the same deadline ends "installing" here.
            let before = self.computer_use_state();
            self.computer_use_install_deadline = None;
            self.computer_use_install_requested = false;
            self.after_computer_use_changed(before, cx);
        }
        self.tick_cli_rows(now, cx);
        self.tick_get_started(now);
        self.tick_intro_video(cx);
        if self.flow.finished {
            self.tick_finished(now, cx);
        } else if self.panel == 2 {
            self.tick_agents(now, cx);
        }
    }

    fn start_backdrop(&mut self, cx: &mut Context<Self>) {
        let background = cx.background_executor().clone();
        let opened_at = self.opened_at;
        cx.spawn(async move |this, cx| {
            let grain = background
                .spawn(async move { backdrop::render_grain().and_then(pixels_to_image) })
                .await;
            if this
                .update(cx, |this, cx| {
                    this.grain = grain;
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            let mut last_nebula: Option<Instant> = None;
            loop {
                let frame_started = Instant::now();
                let veil_time = frame_started
                    .saturating_duration_since(opened_at)
                    .as_secs_f32()
                    * 0.6;
                let veil = background
                    .spawn(async move {
                        pixels_to_image(backdrop::render_veil(veil_time, VEIL_SATURATION))
                    })
                    .await;
                let nebula = if last_nebula.is_none_or(|at| {
                    frame_started.saturating_duration_since(at) >= Duration::from_millis(500)
                }) {
                    last_nebula = Some(frame_started);
                    // The React clock starts 40 s in.
                    let t = 40.0
                        + frame_started
                            .saturating_duration_since(opened_at)
                            .as_secs_f32();
                    background
                        .spawn(async move { pixels_to_image(backdrop::render_nebula(t)) })
                        .await
                } else {
                    None
                };
                let alive = this.update(cx, |this, cx| {
                    let now = Instant::now();
                    let mut retired = Vec::new();
                    if let Some(veil) = veil {
                        retired.extend(this.veil.push(veil, now));
                    }
                    if let Some(nebula) = nebula {
                        retired.extend(this.nebula.push(nebula, now));
                    }
                    for image in retired {
                        cx.drop_image(image, None);
                    }
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
                let spent = frame_started.elapsed();
                let interval = Duration::from_millis(1000 / 12);
                if spent < interval {
                    cx.background_executor().timer(interval - spent).await;
                }
            }
        })
        .detach();
    }

    /// Releases the backdrop bitmaps from the sprite atlas; the lifecycle calls it before removing the window.
    pub(crate) fn release_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut images = self.veil.drain();
        images.extend(self.nebula.drain());
        images.extend(self.vignette.take());
        images.extend(self.grain.take());
        for image in images {
            let _ = window.drop_image(image.clone());
            cx.drop_image(image, None);
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if key == "escape" && self.popup_open() {
            self.close_popup(cx);
            cx.stop_propagation();
            return;
        }
        if self.menu_open && key == "escape" {
            self.menu_open = false;
            cx.notify();
            return;
        }
        if key != "right" && key != "left" {
            return;
        }
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.shift {
            return;
        }
        if self.intro_video_active() {
            if key == "right" {
                self.leave_intro_video(cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.popup_open() {
            return;
        }
        if self.slider_key(key == "right", cx) {
            cx.stop_propagation();
            return;
        }
        let next = if key == "right" {
            self.panel + 1
        } else {
            self.panel.saturating_sub(1).max(1)
        };
        self.go(next, cx);
        cx.stop_propagation();
    }

    // Chrome ----------------------------------------------------------------------------------

    fn divider_x(&self) -> f32 {
        PANEL_DIVIDER_X[self.panel - 1]
    }

    fn render_backdrop(&self, s: S, now: Instant, divider: f32) -> AnyElement {
        let layer = |image: Arc<RenderImage>, opacity: f32| {
            img(image)
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .object_fit(ObjectFit::Fill)
                .opacity(opacity)
        };
        let veil_offset = VEIL_LEFT - divider;
        div()
            .absolute()
            .left_0()
            .top_0()
            .w(s.px(STAGE_WIDTH))
            .h(s.px(STAGE_HEIGHT))
            .bg(hex(0x07080b))
            .children(
                self.nebula
                    .layers(now)
                    .into_iter()
                    .map(|(image, opacity)| layer(image, opacity)),
            )
            .when(divider < STAGE_WIDTH, |this| {
                this.child(
                    abs(
                        s,
                        divider,
                        0.0,
                        Some(STAGE_WIDTH - divider),
                        Some(STAGE_HEIGHT),
                    )
                    .overflow_hidden()
                    .child(
                        abs(
                            s,
                            veil_offset,
                            0.0,
                            Some(STAGE_WIDTH - VEIL_LEFT),
                            Some(STAGE_HEIGHT),
                        )
                        .children(
                            self.veil
                                .layers(now)
                                .into_iter()
                                .map(|(image, opacity)| layer(image, opacity)),
                        ),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_overlays(&self, s: S) -> AnyElement {
        div()
            .absolute()
            .left_0()
            .top_0()
            .w(s.px(STAGE_WIDTH))
            .h(s.px(STAGE_HEIGHT))
            .overflow_hidden()
            .when_some(self.vignette.clone(), |this, vignette| {
                this.child(
                    img(vignette)
                        .absolute()
                        .left_0()
                        .top_0()
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                )
            })
            // `.grain`: the 160px noise tile repeated over the stage at 55%.
            .when_some(self.grain.clone(), |this, grain| {
                this.child(
                    img(grain)
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(s.px(backdrop::GRAIN_STAGE_SIZE.0))
                        .h(s.px(backdrop::GRAIN_STAGE_SIZE.1))
                        .opacity(0.55)
                        .object_fit(ObjectFit::Fill),
                )
            })
            .into_any_element()
    }

    fn render_chrome(&self, s: S, now: Instant, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let divider = self
            .transitions
            .value("divider", self.divider_x(), STAGE_MOVE, now);
        let lockup_x =
            self.transitions
                .value("lockup", PANEL_LOCKUP_X[self.panel - 1], STAGE_MOVE, now);
        let foot_right =
            self.transitions
                .value("foot-right", foot_right_x(self.panel), STAGE_MOVE, now);
        let mut elements = Vec::new();
        if self.divider_x() < STAGE_WIDTH || divider < STAGE_WIDTH - 0.5 {
            // `.divider`: a hairline graded 0.09 -> 0.045 (55%) -> 0.07 top to bottom.
            elements.push(
                abs(s, divider, 0.0, Some(1.0), Some(STAGE_HEIGHT))
                    .flex()
                    .flex_col()
                    .child(div().h(relative(0.55)).w_full().bg(gpui::linear_gradient(
                        180.0,
                        gpui::linear_color_stop(white(0.09), 0.0),
                        gpui::linear_color_stop(white(0.045), 1.0),
                    )))
                    .child(div().flex_1().w_full().bg(gpui::linear_gradient(
                        180.0,
                        gpui::linear_color_stop(white(0.045), 0.0),
                        gpui::linear_color_stop(white(0.07), 1.0),
                    )))
                    .into_any_element(),
            );
        }
        elements.push(
            abs(s, lockup_x - 9.0, 19.0, None, None)
                .flex()
                .items_center()
                .gap(s.px(12.0))
                .child(ghostex_logo(s, 34.0, false))
                .child(
                    sans(s, 17.0, 500.0, hex(0xe5e9f1))
                        .line_height(s.px(17.0))
                        .child("Ghostex"),
                )
                .into_any_element(),
        );
        elements.push(
            abs(s, STAGE_WIDTH - 46.0 - 120.0, 28.0, Some(120.0), None)
                .flex()
                .justify_end()
                .child(tracked_text(
                    s,
                    format!("{:02}", self.panel),
                    fonts::PLEX_MONO,
                    400.0,
                    17.0,
                    17.0,
                    0.04,
                    hex(0x6a739a),
                ))
                .child(tracked_text(
                    s,
                    " ",
                    fonts::PLEX_MONO,
                    400.0,
                    17.0,
                    17.0,
                    0.04,
                    hex(0xeef1f7),
                ))
                .child(tracked_text(
                    s,
                    format!("/ 0{PANEL_COUNT}"),
                    fonts::PLEX_MONO,
                    400.0,
                    17.0,
                    17.0,
                    0.04,
                    hex(0xd3d9e4),
                ))
                .into_any_element(),
        );
        elements.push(
            abs(
                s,
                FOOT_LEFT,
                FOOT_TOP,
                Some(foot_right - FOOT_LEFT),
                Some(FOOT_HEIGHT),
            )
            .flex()
            .items_center()
            // CDXC:Onboarding 2026-09-15 DECISION: User: "remove the back button from the first page". Back only renders from panel 2 on.
            .when(self.panel > 1, |this| {
                // `.back`: no transition of its own, so the host's 120ms button transition.
                let color = interact::hover_color(
                    "onboarding-back",
                    "color",
                    hex(0xdfe4ee),
                    gpui::white(),
                    interact::BUTTON_MS,
                );
                this.child(
                    self.control(
                        s,
                        div()
                            .id("onboarding-back")
                            .relative()
                            .flex()
                            .items_center()
                            .gap(s.px(13.0))
                            .h(s.px(44.0))
                            .cursor_pointer()
                            .text_color(color)
                            .child(icon(s, "arrowL", 18.0, 1.6, color))
                            .child(sans(s, 17.0, 400.0, color).child("Back")),
                        "onboarding-back",
                        interact::Ring::new(0.0, 0.0),
                        interact::Keys::EnterSpace,
                        cx,
                        |this, _, cx| {
                            let previous = this.panel.saturating_sub(1).max(1);
                            this.go(previous, cx);
                        },
                    ),
                )
            })
            .into_any_element(),
        );
        elements
    }

    /// CDXC:Onboarding 2026-09-15 DECISION:
    /// User: "make the 5 progress dots appear on the top left of the right half of the modal". They sit just right of the divider; the full-width last panel has no right half, so there they stay centred. This moves the 2026-09-12 request to put "the stepper dots at the bottom to the top center" into the stage header.
    fn render_dots(&self, s: S, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let divider_target = self.divider_x();
        // Just right of the divider; the full-width last panel has no right half, so they centre there.
        let dots_width = (0..PANEL_COUNT)
            .map(|index| {
                self.transitions.value(
                    &format!("dot-w-{index}"),
                    if index + 1 == self.panel { 22.0 } else { 6.0 },
                    (Duration::from_millis(300), Ease::Ease),
                    now,
                )
            })
            .collect::<Vec<_>>();
        let row_width: f32 = dots_width.iter().map(|width| width + 10.0).sum::<f32>()
            + 4.0 * (PANEL_COUNT as f32 - 1.0);
        let target_left = if divider_target < STAGE_WIDTH {
            divider_target + DOTS_INSET
        } else {
            STAGE_WIDTH / 2.0 - row_width / 2.0
        };
        let left = self
            .transitions
            .value("dots-left", target_left, STAGE_MOVE, now);
        abs(s, left, 27.0, None, None)
            .flex()
            .gap(s.px(4.0))
            .children((0..PANEL_COUNT).map(|index| {
                let key = format!("onboarding-dot-{index}");
                let state = if index + 1 == self.panel {
                    hex(0x8c98c2)
                } else if index + 1 < self.panel {
                    hex(0x565e72)
                } else {
                    hex(0x363c49)
                };
                // `.dots button:hover i` outranks `.dots i.on`, so the current dot greys on hover too.
                let color = interact::hover_color(&key, "bg", state, hex(0x7c8598), 300);
                self.control(
                    s,
                    div()
                        .id(gpui::SharedString::from(key.clone()))
                        .relative()
                        .py(s.px(9.0))
                        .px(s.px(5.0))
                        .cursor_pointer()
                        .child(
                            div()
                                .w(s.px(dots_width[index]))
                                .h(s.px(6.0))
                                .rounded(s.px(3.0))
                                .bg(color),
                        ),
                    key,
                    interact::Ring::new(0.0, 0.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| this.go(index + 1, cx),
                )
            }))
            .into_any_element()
    }

    fn render_toast(&self, s: S, now: Instant) -> Option<AnyElement> {
        let toast = self.toast.as_ref()?;
        let divider = self.divider_x();
        let center = if divider >= STAGE_WIDTH {
            STAGE_WIDTH / 2.0
        } else {
            divider + (STAGE_WIDTH - divider) / 2.0
        };
        let t = progress(
            toast.shown_at,
            now,
            Duration::from_millis(250),
            Ease::EaseOut,
        );
        Some(
            abs(
                s,
                center - 700.0,
                22.0 - 8.0 * (1.0 - t),
                Some(1400.0),
                None,
            )
            .flex()
            .justify_center()
            .opacity(t)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s.px(9.0))
                    .py(s.px(10.0))
                    .px(s.px(16.0))
                    .rounded(s.px(10.0))
                    .bg(rgba(14, 18, 30, 0.96))
                    .border_1()
                    .border_color(rgba(100, 140, 255, 0.35))
                    .shadow(vec![shadow(black(0.45), 0.0, 12.0, 30.0, 0.0, s)])
                    .max_w(s.px(820.0))
                    .when(toast.warning, |this| {
                        this.border_color(rgba(255, 196, 107, 0.45))
                    })
                    .child(if toast.warning {
                        icon(s, "x", 15.0, 1.6, hex(0xffc46b))
                    } else {
                        icon(s, "checkCircle", 15.0, 1.6, hex(0x3be3a2))
                    })
                    .child(
                        sans(s, 13.5, 400.0, hex(0xeef1f7))
                            .min_w_0()
                            .child(toast.message.clone()),
                    ),
            )
            .into_any_element(),
        )
    }

    fn render_scene(
        &mut self,
        s: S,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let show_finished = self.flow.finished && self.panel == PANEL_COUNT;
        let children: Vec<AnyElement> = if self.intro_video_active() {
            self.render_intro_video(s, now, cx)
        } else if show_finished {
            self.render_finished(s, now, cx)
        } else {
            match self.panel {
                1 => self.render_welcome(s, now, cx),
                2 => self.render_agents(s, now, window, cx),
                3 => self.render_workspace(s, now, cx),
                4 => self.render_mobile(s, now, cx),
                _ => self.render_get_started(s, now, cx),
            }
        };
        // `gxob-sceneIn`: 0.4s, fading in from 6px lower.
        let t = progress(
            self.scene_started,
            now,
            Duration::from_millis(400),
            Ease::Bezier(0.2, 0.7, 0.2, 1.0),
        );
        div()
            .absolute()
            .left_0()
            .top(s.px(6.0 * (1.0 - t)))
            .w(s.px(STAGE_WIDTH))
            .h(s.px(STAGE_HEIGHT))
            .opacity(t)
            .children(children)
            .into_any_element()
    }
}

fn relative(value: f32) -> gpui::DefiniteLength {
    gpui::relative(value)
}

fn pixels_to_image(pixels: backdrop::BackdropPixels) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(pixels.width, pixels.height, pixels.bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

impl Focusable for GpuiOnboardingWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GpuiOnboardingWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.tick(now, cx);
        window.request_animation_frame();
        interact::begin_frame();
        self.begin_focus_frame(window);
        let viewport = window.viewport_size();
        let scale =
            (viewport.width.as_f32() / STAGE_WIDTH).min(viewport.height.as_f32() / STAGE_HEIGHT);
        let s = S(scale);
        let stage_left = (viewport.width.as_f32() - STAGE_WIDTH * scale) / 2.0;
        let stage_top = (viewport.height.as_f32() - STAGE_HEIGHT * scale) / 2.0;
        // The intro video page spans the whole stage; the veil slides in when Welcome replaces it.
        let divider_target = if self.intro_video_active() {
            STAGE_WIDTH
        } else {
            self.divider_x()
        };
        let divider = self
            .transitions
            .value("veil-clip", divider_target, STAGE_MOVE, now);
        let backdrop = self.render_backdrop(s, now, divider);
        self.focus.set_group(interact::TabGroup::Back);
        let chrome = if self.intro_video_active() {
            self.render_intro_video_chrome(s, now)
        } else {
            self.render_chrome(s, now, cx)
        };
        self.focus.set_group(interact::TabGroup::Panel);
        let scene = self.render_scene(s, now, window, cx);
        let folder_chooser = self.render_folder_chooser(s, now, window, cx);
        self.focus.set_group(interact::TabGroup::Dots);
        let dots = (!self.intro_video_active()).then(|| self.render_dots(s, now, cx));
        let toast = self.render_toast(s, now);
        let overlays = self.render_overlays(s);
        self.end_focus_frame(window, cx);
        div()
            .id("onboarding-root")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, _: &FocusNext, window, cx| this.move_focus(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FocusPrevious, window, cx| {
                    this.move_focus(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &Activate, window, cx| {
                if this.activate_focused(false, window, cx) {
                    return;
                }
                // Enter on the intro video page continues, as on a page whose only action is Continue.
                if this.intro_video_active() {
                    this.leave_intro_video(cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ActivateSpace, window, cx| {
                if !this.activate_focused(true, window, cx) {
                    cx.propagate();
                }
            }))
            .on_key_down(cx.listener(Self::handle_key_down))
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(hex(0x040507))
            .font_family(fonts::dm_sans())
            .text_color(hex(0xeef1f7))
            .child(
                div()
                    .absolute()
                    .left(px(stage_left))
                    .top(px(stage_top))
                    .w(s.px(STAGE_WIDTH))
                    .h(s.px(STAGE_HEIGHT))
                    .overflow_hidden()
                    .child(backdrop)
                    .children(chrome)
                    .child(scene)
                    .children(dots)
                    .child(overlays)
                    .children(folder_chooser)
                    .children(toast),
            )
    }
}

/// Entry points the preview binary uses to open a state partway through an interaction.
#[allow(dead_code)]
impl GpuiOnboardingWindow {
    pub(crate) fn preview_computer_use_installing(&mut self, cx: &mut Context<Self>) {
        let before = self.computer_use_state();
        self.computer_use_install_requested = true;
        self.after_computer_use_changed(before, cx);
        cx.notify();
    }

    pub(crate) fn preview_open_install_guide(&mut self, cx: &mut Context<Self>) {
        self.agents_panel.guide_open = true;
        self.agents_panel.guide_opened_at = Instant::now();
        cx.notify();
    }

    pub(crate) fn preview_right_tab(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.agents_panel.right_tab = match tab {
            1 => agents::RightTab::Integration,
            2 => agents::RightTab::ComputerUse,
            _ => agents::RightTab::Scan,
        };
        self.agents_panel.right_tab_since = Instant::now();
        cx.notify();
    }

    pub(crate) fn preview_welcome_tab(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.welcome.tab = match tab {
            1 => welcome::WelcomeTab::Chat,
            2 => welcome::WelcomeTab::Mobile,
            _ => welcome::WelcomeTab::Together,
        };
        self.welcome.tab_since = Instant::now();
        cx.notify();
    }

    pub(crate) fn preview_workspace_tab(&mut self, view: Option<usize>, cx: &mut Context<Self>) {
        self.preview_workspace(view.map(|index| model::VIEW_KEYS[index]));
        cx.notify();
    }

    pub(crate) fn preview_finished_guide(&mut self, cx: &mut Context<Self>) {
        self.finished.follow_up = Some(finished::FollowUp::Guide);
        self.finished.follow_up_at = Instant::now();
        cx.notify();
    }
}

impl super::native_modal_kit::ModalCornerClose for GpuiOnboardingWindow {
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Escape never closes onboarding; the corner button is the window's close.
        self.close_from_chrome(cx);
    }
}
