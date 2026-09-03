// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds settings/tips-menu actions plus titlebar action selection/state and configured-action running.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn open_gpui_settings_actions_modal_from_titlebar(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Titlebar 2026-06-24-14:24:
        Empty, missing, or unconfigured titlebar Actions paths must deep-link to the shared Settings modal Actions tab with `{ modal: "settings", initialTab: "actions" }`. Do not reopen the old configureActions modal id or a GPUI placeholder surface from this titlebar path.
        */
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTab": "actions",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn open_gpui_settings_extensions_page(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Extensions 2026-08-30:
        The standalone Extensions browser modal and the Settings "Customize"
        page are one Settings tab now, so every extensions entry point (app
        menu, titlebar Settings menu, titlebar right-click, the puzzle popup's
        browse row, and the `openExtensions` hotkey) deep-links here with
        `{ modal: "settings", initialTab: "extensions" }`. There is no
        `extensionsBrowser` app-modal kind any more.
        */
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTab": "extensions",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_tips_header_action(
        &mut self,
        action_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match action_index {
            0 => self.open_gpui_browser_action_url(GHOSTEX_DOCS_URL.to_string(), window, cx),
            1 => self.open_gpui_app_modal_from_titlebar(
                GpuiAppModalKind::WatchGhostexVideo,
                window,
                cx,
            ),
            2 => self.open_gpui_app_modal_from_titlebar(
                GpuiAppModalKind::FirstLaunchSetup,
                window,
                cx,
            ),
            3 => self.open_gpui_browser_action_url(GHOSTEX_CHANGELOG_URL.to_string(), window, cx),
            4 => self.open_gpui_settings_integrations_from_titlebar(None, window, cx),
            _ => {}
        }
    }

    pub(crate) fn run_gpui_titlebar_tip(
        &mut self,
        tip_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(tip) = GPUI_NATIVE_TITLEBAR_TIPS.get(tip_index).copied() else {
            return;
        };
        gpui_mark_titlebar_tip_read(tip.id);
        self.titlebar_tips_unread_count = gpui_titlebar_tips_unread_count_from_settings();
        match tip.id {
            "use-ghostex-computer-use-skill" => self.open_gpui_settings_integrations_from_titlebar(
                Some("Ghostex Computer Use"),
                window,
                cx,
            ),
            "use-ghostex-browser-use-skill" => self.open_gpui_settings_integrations_from_titlebar(
                Some("Ghostex Browser Use"),
                window,
                cx,
            ),
            "use-ghostex-embedded-browser-use-skill" => self
                .open_gpui_settings_integrations_from_titlebar(
                    Some("Ghostex Embedded Browser Use"),
                    window,
                    cx,
                ),
            _ => cx.notify(),
        }
    }

    pub(crate) fn open_gpui_settings_integrations_from_titlebar(
        &mut self,
        search_query: Option<&str>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTab": "integrations",
            "modal": modal.modal_id(),
            "type": "open",
        });
        if let Some(search_query) = search_query {
            open_message["initialSearchQuery"] = serde_json::json!(search_query);
        }
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn open_gpui_settings_agent_hooks_page(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialSearchQuery": "Agent Hooks",
            "initialTab": "agents",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn open_gpui_titlebar_notice_settings(
        &mut self,
        target: GpuiNativeTitlebarNoticeTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (initial_tab, search_query) = match target {
            GpuiNativeTitlebarNoticeTarget::AgentHooks => ("integrations", "Agent Hooks"),
            GpuiNativeTitlebarNoticeTarget::DebuggingMode => ("settings", "Show debug UI controls"),
            GpuiNativeTitlebarNoticeTarget::GhostexCli => ("integrations", "Ghostex CLI"),
        };
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialSearchQuery": search_query,
            "initialTab": initial_tab,
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn visible_gpui_titlebar_actions(&self) -> Vec<GpuiTitlebarAction> {
        self.titlebar_actions_snapshot.clone()
    }

    pub(crate) fn refresh_titlebar_actions_in_background(&mut self, cx: &mut gpui::Context<Self>) {
        if self.titlebar_actions_refresh_in_flight {
            return;
        }
        self.titlebar_actions_refresh_in_flight = true;
        let fetched_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        let request_project_id = fetched_project_id.clone();
        /*
        CDXC:RemoteMachines 2026-08-29:
        A remote project's Actions live on the machine that owns it, so a remote
        active project reads that machine's HUD through its live tunnel. The
        local daemon does not know a `remote:` project id and would answer with
        its own unconfigured defaults, which is what used to make the Actions
        button deep-link to Settings for every remote project. A remote project
        whose machine is not connected has no Actions to show, so it stays empty
        rather than borrowing the local daemon's answer.
        */
        let remote_reference = request_project_id
            .as_deref()
            .and_then(gpui_remote_project_reference_from_project_id);
        let remote_request = remote_reference.as_ref().map(|reference| {
            (
                self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str()),
                reference.project_id.clone(),
            )
        });
        cx.spawn(async move |this, cx| {
            let actions = cx
                .background_executor()
                .spawn(async move {
                    match remote_request {
                        Some((Some(target), project_id)) => {
                            gpui_remote_titlebar_actions_for_project(&target, project_id.as_str())
                        }
                        Some((None, _)) => Vec::new(),
                        None => gpui_titlebar_actions_for_active_project_id(
                            request_project_id.as_deref(),
                        ),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.titlebar_actions_refresh_in_flight = false;
                let current_project_id = gpui_active_project_id_from_snapshot(
                    this.latest_sidebar_project_snapshot.as_ref(),
                )
                .map(str::to_string);
                if current_project_id != fetched_project_id {
                    this.refresh_titlebar_actions_in_background(cx);
                    return;
                }
                if this.titlebar_actions_snapshot != actions {
                    this.titlebar_actions_snapshot = actions;
                    cx.notify();
                }
                this.refresh_extensions_in_background(cx);
            });
        })
        .detach();
    }

    pub(crate) fn titlebar_selection_owner_project_id(&self) -> Option<&str> {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.selection_owner_project_id.as_ref())
            .map(|project_id| project_id.0.as_str())
    }

    pub(crate) fn restore_gpui_titlebar_project_selections(&mut self) {
        let Some(project_id) = self
            .titlebar_selection_owner_project_id()
            .map(str::to_string)
        else {
            self.active_open_target_id = None;
            self.active_action_command_id = None;
            return;
        };
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        self.active_open_target_id = gpui_titlebar_project_selection_from_settings(
            settings.object(),
            GPUI_TITLEBAR_OPEN_TARGET_SELECTIONS_SETTINGS_KEY,
            &project_id,
        );
        self.active_action_command_id = gpui_titlebar_project_selection_from_settings(
            settings.object(),
            GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY,
            &project_id,
        );
    }

    pub(crate) fn persist_gpui_titlebar_project_selection(&self, settings_key: &str, value: &str) {
        let Some(project_id) = self.titlebar_selection_owner_project_id() else {
            return;
        };
        let _ = gpui_persist_titlebar_project_selection(settings_key, project_id, value);
    }

    pub(crate) fn configured_gpui_titlebar_actions(&self) -> Vec<GpuiTitlebarAction> {
        self.visible_gpui_titlebar_actions()
            .into_iter()
            .filter(GpuiTitlebarAction::is_configured)
            .collect()
    }

    pub(crate) fn active_gpui_titlebar_action(&self) -> Option<GpuiTitlebarAction> {
        let actions = self.configured_gpui_titlebar_actions();
        self.active_action_command_id
            .as_deref()
            .and_then(|active_id| {
                actions
                    .iter()
                    .find(|action| action.command_id == active_id)
                    .cloned()
            })
            .or_else(|| actions.into_iter().next())
    }

    pub(crate) fn titlebar_quick_action_button_on_cooldown(&self) -> bool {
        self.titlebar_quick_action_cooldown_until
            .is_some_and(|until| std::time::Instant::now() < until)
    }

    pub(crate) fn begin_titlebar_quick_action_button_cooldown(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentLauncher 2026-08-27:
        Every command-Action terminal launch arms a 2-second cooldown on the
        titlebar Quick Actions button, because clicking it while the Action is
        still running now kills and relaunches that terminal — without the
        cooldown, spamming the button would churn kill/create cycles. Popup
        menu rows and hotkeys stay ungated; only the primary button click is.
        */
        let cooldown = std::time::Duration::from_secs(2);
        self.titlebar_quick_action_cooldown_until = Some(std::time::Instant::now() + cooldown);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(cooldown).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    pub(crate) fn run_active_gpui_titlebar_action(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.titlebar_quick_action_button_on_cooldown() {
            return;
        }
        let Some(action) = self.active_gpui_titlebar_action() else {
            self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
            return;
        };
        self.run_gpui_titlebar_action_from_titlebar(action, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_action_index(
        &mut self,
        action_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(action) = self
            .visible_gpui_titlebar_actions()
            .into_iter()
            .nth(action_index)
        else {
            return;
        };
        self.run_gpui_titlebar_action_from_titlebar(action, window, cx);
    }

    pub(crate) fn run_configured_gpui_titlebar_action_index(
        &mut self,
        action_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(action) = self
            .configured_gpui_titlebar_actions()
            .into_iter()
            .nth(action_index)
        else {
            return;
        };
        self.run_gpui_titlebar_action_from_titlebar(action, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_action_from_titlebar(
        &mut self,
        mut action: GpuiTitlebarAction,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Titlebar 2026-06-27-09:26:
        Titlebar left-clicks, right-click menu rows, and positional Action hotkeys are click sources, so GPUI derives Debug reruns from sanitized local feedback just like the React command palette. Sidebar bridge payloads call `run_gpui_titlebar_action` directly and keep their explicit `runMode` authority.
        */
        action.run_mode = gpui_titlebar_action_run_mode_for_click(
            &action,
            self.sidebar_command_run_feedback_states
                .get(&action.command_id),
        );
        self.run_gpui_titlebar_action(action, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_action(
        &mut self,
        action: GpuiTitlebarAction,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !action.is_configured() {
            self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
            return;
        }

        match action.action_type {
            GpuiTitlebarActionType::Browser => {
                let Some(url) = action
                    .url
                    .as_deref()
                    .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
                    .map(str::to_string)
                else {
                    self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                    return;
                };
                self.persist_gpui_titlebar_project_selection(
                    GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY,
                    &action.command_id,
                );
                self.active_action_command_id = Some(action.command_id);
                self.open_gpui_browser_action_url(url, window, cx);
            }
            GpuiTitlebarActionType::Terminal => {
                let Some(command) = action
                    .command
                    .as_deref()
                    .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
                    .map(str::to_string)
                else {
                    self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                    return;
                };
                let title = action.command_title();
                let command_id = action.command_id.clone();
                self.persist_gpui_titlebar_project_selection(
                    GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY,
                    &command_id,
                );
                self.active_action_command_id = Some(command_id.clone());
                /*
                CDXC:RemoteMachines 2026-08-29:
                The Action belongs to a remote project, so its command has to run
                on the machine that owns that project — the local command pane
                would run the project's command here, against a path that only
                exists there. Debug reruns collapse into the same remote launch
                because the debug workspace terminal is a local surface too.
                */
                if let Some(reference) = self.active_gpui_remote_project_reference() {
                    self.run_gpui_remote_command_action_terminal(
                        reference, command_id, title, command, window, cx,
                    );
                    self.open_gpui_titlebar_action_links(&action.links, window, cx);
                    return;
                }
                match action.run_mode {
                    GpuiTitlebarActionRunMode::Default => {
                        self.open_gpui_command_action_terminal(
                            command_id,
                            title,
                            command,
                            action.play_completion_sound,
                            action.close_terminal_on_exit,
                            window,
                            cx,
                        );
                    }
                    GpuiTitlebarActionRunMode::Debug => {
                        self.open_gpui_debug_command_action_terminal(title, command, cx);
                    }
                }
                self.open_gpui_titlebar_action_links(&action.links, window, cx);
            }
        }
    }
}
