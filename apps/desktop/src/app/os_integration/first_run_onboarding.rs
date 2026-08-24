// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds the first-run onboarding marker persistence, the onboarding kickoff flow, and the Windows first-run distribution setup UI.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

#[cfg(target_os = "windows")]
use gpui::AnyElement;
#[cfg(target_os = "windows")]
use gpui::FontWeight;
#[cfg(target_os = "windows")]
use gpui::MouseButton;
#[cfg(target_os = "windows")]
use gpui::MouseUpEvent;
#[cfg(target_os = "windows")]
use gpui::div;
#[cfg(target_os = "windows")]
use gpui::relative;
#[cfg(target_os = "windows")]
use gpui::rgba;
#[cfg(target_os = "windows")]
use gpui_component::h_flex;
#[cfg(target_os = "windows")]
use gpui_component::v_flex;

impl GhostexGpuiApp {
    /// GPUI port of the macOS sidebar-mount first-run block
    /// (`openTipsAndTricksOnFirstLaunch` + `showOSIntegrationOnboardingOnFirstLaunch`,
    /// native-sidebar.tsx): current shipped macOS consumes the legacy tips and
    /// Highlighted Features markers WITHOUT opening those surfaces (the
    /// Discover→firstLaunchSetup auto chain was superseded 2026-06-18), opens
    /// the tutorial video modal once per first-launch-setup revision, and shows
    /// a once-forever OS Integration toast.
    /// CDXC:GPUIFirstRunOnboardingOnce 2026-08-18: writes one first-run marker
    /// off the foreground thread, re-reading the file first so a concurrent
    /// writer of a different field is not clobbered by this pass's snapshot.
    pub(crate) fn persist_gpui_first_run_onboarding_marker(
        &self,
        marker: GpuiFirstRunOnboardingMarker,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.background_executor()
            .spawn(async move {
                let mut state = load_gpui_first_run_onboarding_state();
                match marker {
                    GpuiFirstRunOnboardingMarker::FirstLaunchSetupSeen => {
                        if state.first_launch_setup_seen_revision.as_deref()
                            == Some(GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION)
                        {
                            return;
                        }
                        state.first_launch_setup_seen_revision =
                            Some(GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION.to_string());
                    }
                    GpuiFirstRunOnboardingMarker::OsIntegrationOnboardingSeen => {
                        if state.os_integration_onboarding_seen {
                            return;
                        }
                        state.os_integration_onboarding_seen = true;
                    }
                }
                persist_gpui_first_run_onboarding_state(&state);
            })
            .detach();
    }

    pub(crate) fn start_gpui_first_run_onboarding(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sidebar.is_none() {
            return;
        }
        #[cfg(target_os = "windows")]
        if self.windows_first_run_setup_state != GpuiWindowsFirstRunSetupState::Ready {
            return;
        }
        /*
        CDXC:GPUIFirstRunOnboardingOnce 2026-08-18:
        The first-launch-setup and OS-integration markers are persisted only
        after their surface is really shown (below), so the persisted flags no
        longer dedupe the callers that start onboarding. Latch the pass in
        memory instead, after every early-return guard, so the gxserver
        bootstrap, CEF init, and daemon-respawn entry points can each try to
        start onboarding while only the first one that gets past the guards
        runs it.
        */
        if self.first_run_onboarding_started {
            return;
        }
        self.first_run_onboarding_started = true;
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (show_os_integration_toast, first_launch_setup_sidebar_state) = background
                .spawn(async move {
                    let mut state = load_gpui_first_run_onboarding_state();
                    let mut changed = false;
                    if !state.tips_and_tricks_seen {
                        state.tips_and_tricks_seen = true;
                        changed = true;
                    }
                    if state.highlighted_features_seen_revision.as_deref()
                        != Some(GPUI_HIGHLIGHTED_FEATURES_SEEN_REVISION)
                    {
                        state.highlighted_features_seen_revision =
                            Some(GPUI_HIGHLIGHTED_FEATURES_SEEN_REVISION.to_string());
                        changed = true;
                    }
                    /*
                    CDXC:GPUIFirstRunOnboardingOnce 2026-08-18:
                    The first-launch-setup and OS-integration markers describe
                    surfaces the user is supposed to see, so they are persisted
                    from the foreground block below once the toast is really
                    posted and the modal window really exists. Marking them here
                    burned first-run onboarding whenever the pass reached this
                    point and the UI never appeared (no sidebar-owned window,
                    a failed modal open, or an app exit in between). The two
                    legacy markers above show nothing at all, so they stay
                    consumed on this pass.

                    CDXC:GPUIFirstLaunchTutorialVideo 2026-08-19:
                    First run raises exactly ONE modal. The tutorial video used
                    to be its own window opened before (Windows) or instead of
                    (everywhere else) the setup modal; it is the setup modal's
                    first page now. Two markers still gate it, and they mean
                    different things: the revision marker says "this revision's
                    setup was presented", written when the window exists, so a
                    future revision bump shows it again; `complete` says "the
                    user closed it", written on close, so quitting mid-setup
                    brings it back on the next launch.
                    */
                    let show_os_integration_toast = !state.os_integration_onboarding_seen;
                    if changed {
                        persist_gpui_first_run_onboarding_state(&state);
                    }
                    let needs_first_launch_setup = state.first_launch_setup_seen_revision.as_deref()
                        != Some(GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION)
                        || !state.first_launch_setup_complete;
                    let first_launch_setup_sidebar_state = needs_first_launch_setup.then(|| {
                        gpui_app_modal_sidebar_state_message_for_active_project_id(
                            active_project_id.as_deref(),
                        )
                    });
                    (show_os_integration_toast, first_launch_setup_sidebar_state)
                })
                .await;
            if !show_os_integration_toast && first_launch_setup_sidebar_state.is_none() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if show_os_integration_toast {
                    this.upsert_gpui_app_toast(
                        GpuiAppToast {
                            id: "gpui-os-integration-onboarding".to_string(),
                            level: GpuiAppToastLevel::from_raw(Some("info")),
                            title: "OS Integration available".to_string(),
                            description: Some(
                                "Open Settings > OS Integration to set Ghostex as your editor or terminal target."
                                    .to_string(),
                            ),
                            loading: false,
                            persistent: false,
                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                            epoch: 0,
                        },
                        cx,
                    );
                    this.persist_gpui_first_run_onboarding_marker(
                        GpuiFirstRunOnboardingMarker::OsIntegrationOnboardingSeen,
                        cx,
                    );
                }
                if let Some(base_sidebar_state) = first_launch_setup_sidebar_state {
                    this.open_gpui_first_launch_setup_with_sidebar_state(base_sidebar_state, cx);
                    // Only a window that actually exists counts as "this
                    // revision's setup was presented".
                    if this.app_modal_window.is_some() {
                        this.persist_gpui_first_run_onboarding_marker(
                            GpuiFirstRunOnboardingMarker::FirstLaunchSetupSeen,
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn retry_windows_first_run_setup(&mut self, cx: &mut gpui::Context<Self>) {
        windows_terminal_backend::reset();
        self.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::Checking;
        self.start_gpui_local_gxserver_bootstrap(cx);
        cx.notify();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn save_windows_first_run_distribution(
        &mut self,
        distribution: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.insert(
            "windowsWslDistribution".to_string(),
            serde_json::Value::String(distribution),
        );
        match shared_settings::write_shared_sidebar_settings_object(settings) {
            Ok(_) => self.retry_windows_first_run_setup(cx),
            Err(_) => {
                self.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::Failed(
                    "Ghostex could not save the selected Linux environment. Check that its settings folder is writable, then try again."
                        .to_string(),
                );
                cx.notify();
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn handle_windows_first_run_setup_action(
        &mut self,
        action: GpuiWindowsFirstRunSetupAction,
        cx: &mut gpui::Context<Self>,
    ) {
        match action {
            GpuiWindowsFirstRunSetupAction::Retry => self.retry_windows_first_run_setup(cx),
            GpuiWindowsFirstRunSetupAction::OpenWslGuide => {
                let _ =
                    gpui_open_external_http_url("https://learn.microsoft.com/windows/wsl/install");
            }
            GpuiWindowsFirstRunSetupAction::ChooseDistribution(distribution) => {
                self.save_windows_first_run_distribution(distribution, cx);
            }
            GpuiWindowsFirstRunSetupAction::ClearDistribution => {
                self.save_windows_first_run_distribution(String::new(), cx);
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn render_windows_first_run_setup_button(
        &self,
        label: impl Into<gpui::SharedString>,
        primary: bool,
        action: GpuiWindowsFirstRunSetupAction,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .h(px(36.0))
            .px(px(16.0))
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .border_1()
            .border_color(if primary {
                rgba(0x5a9bf0ff)
            } else {
                rgba(0xffffff2b)
            })
            .bg(if primary {
                rgba(0x377bd8ff)
            } else {
                rgba(0xffffff0d)
            })
            .hover(|this| {
                this.bg(if primary {
                    rgba(0x4389e7ff)
                } else {
                    rgba(0xffffff17)
                })
            })
            .cursor_pointer()
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(0xffffffff))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.handle_windows_first_run_setup_action(action.clone(), cx);
                }),
            )
            .child(label.into())
            .into_any_element()
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn render_windows_first_run_setup(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let state = self.windows_first_run_setup_state.clone();
        let (title, description, detail, progress) = match &state {
            GpuiWindowsFirstRunSetupState::Checking => (
                "Checking Windows",
                "Ghostex uses WSL2 for fast terminals, agents, and persistent sessions.",
                "Looking for an initialized Linux environment…".to_string(),
                0.12,
            ),
            GpuiWindowsFirstRunSetupState::MissingWsl => (
                "Ghostex requires WSL2",
                "Install Windows Subsystem for Linux, then return here. Ghostex will never install it or request administrator access without you choosing to do so.",
                "WSL is not available on this PC.".to_string(),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::MissingDistribution => (
                "Finish setting up Linux",
                "Install a Linux distribution such as Ubuntu and open it once to create your Linux account. Then Ghostex can finish terminal setup.",
                "No initialized WSL2 Linux environment was found.".to_string(),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::ChooseDistribution(_) => (
                "Choose your Linux environment",
                "Ghostex found more than one initialized WSL2 distribution. Choose where terminals, agents, and sessions should run.",
                "You can change this later in Settings → Terminal.".to_string(),
                0.18,
            ),
            GpuiWindowsFirstRunSetupState::ConfiguredDistributionUnavailable(distribution) => (
                "Your Linux environment is unavailable",
                "The WSL distribution saved in Ghostex was removed, renamed, or has not finished initialization.",
                format!("Saved environment: {distribution}"),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::SettingUp(phase) => {
                let (detail, progress) = match phase {
                    windows_terminal_backend::WindowsWslSetupPhase::Checking => {
                        ("Checking the bundled terminal engine…", 0.28)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Installing => {
                        ("Installing the Ghostex terminal engine in WSL2…", 0.52)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Starting => {
                        ("Starting the Ghostex terminal engine…", 0.74)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Connecting => {
                        ("Connecting Ghostex to WSL2…", 0.9)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Ready => {
                        ("Ghostex terminals are ready.", 1.0)
                    }
                };
                (
                    "Setting up Ghostex terminals",
                    "This is a one-time setup for the selected Linux environment.",
                    detail.to_string(),
                    progress,
                )
            }
            GpuiWindowsFirstRunSetupState::Failed(message) => (
                "Terminal setup didn’t finish",
                "Ghostex could not finish its WSL2 terminal setup. Nothing was removed or replaced outside Ghostex’s managed files.",
                message.clone(),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::Ready => unreachable!(),
        };

        let mut content = v_flex()
            .w(px(620.0))
            .p(px(32.0))
            .gap(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgba(0xffffff1f))
            .bg(rgba(0x17191dff))
            .child(
                div()
                    .text_size(px(24.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(0xfffffff2))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(rgba(0xffffffad))
                    .child(description),
            )
            .child(
                div()
                    .w_full()
                    .h(px(7.0))
                    .rounded(px(4.0))
                    .bg(rgba(0xffffff12))
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress))
                            .rounded(px(4.0))
                            .bg(rgba(0x4f8fe8ff)),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(
                        if matches!(state, GpuiWindowsFirstRunSetupState::Failed(_)) {
                            rgba(0xffa2aaff)
                        } else {
                            rgba(0xffffff85)
                        },
                    )
                    .child(detail),
            );

        if let GpuiWindowsFirstRunSetupState::ChooseDistribution(distributions) = &state {
            content = content.child(v_flex().gap(px(8.0)).children(
                distributions.iter().cloned().map(|distribution| {
                    self.render_windows_first_run_setup_button(
                        distribution.clone(),
                        true,
                        GpuiWindowsFirstRunSetupAction::ChooseDistribution(distribution),
                        cx,
                    )
                }),
            ));
        }

        let actions = match state {
            GpuiWindowsFirstRunSetupState::MissingWsl
            | GpuiWindowsFirstRunSetupState::MissingDistribution => Some(
                h_flex()
                    .gap(px(10.0))
                    .child(self.render_windows_first_run_setup_button(
                        "View WSL setup guide",
                        false,
                        GpuiWindowsFirstRunSetupAction::OpenWslGuide,
                        cx,
                    ))
                    .child(self.render_windows_first_run_setup_button(
                        "Check again",
                        true,
                        GpuiWindowsFirstRunSetupAction::Retry,
                        cx,
                    )),
            ),
            GpuiWindowsFirstRunSetupState::ConfiguredDistributionUnavailable(_) => Some(
                h_flex()
                    .gap(px(10.0))
                    .child(self.render_windows_first_run_setup_button(
                        "Choose automatically",
                        false,
                        GpuiWindowsFirstRunSetupAction::ClearDistribution,
                        cx,
                    ))
                    .child(self.render_windows_first_run_setup_button(
                        "Check again",
                        true,
                        GpuiWindowsFirstRunSetupAction::Retry,
                        cx,
                    )),
            ),
            GpuiWindowsFirstRunSetupState::Failed(_) => {
                Some(h_flex().child(self.render_windows_first_run_setup_button(
                    "Try again",
                    true,
                    GpuiWindowsFirstRunSetupAction::Retry,
                    cx,
                )))
            }
            _ => None,
        };
        if let Some(actions) = actions {
            content = content.child(actions);
        }

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x0d0f12ff))
            .child(content)
            .into_any_element()
    }
}
