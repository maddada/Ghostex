//! The active project's Git state: read it when the project changes, when the menu opens and after
//! a write, keep the per-project leases, and draw the titlebar Git menu and the work area's Commit
//! button from it. Ported from the old runtime's `git/state-and-github.ts`.

use std::time::Duration;

use ghostex_gx_core::git_menu::{
    GIT_HUB_DEFERRED_PROBE_DELAY_MS, GitPreferences, GitState, GitToastLevel, ProjectGitRead,
    RemoteGitReadFailure, RemoteGitReadStep, git_state_from_read, plan_remote_git_read_failure,
    remote_git_disconnected_toast, titlebar_menu,
};
use serde_json::json;

use super::calls;
use super::scope::{GitScope, ScopeMiss, remote_machine_saved_name};
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;
use crate::app::consts::{
    GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE,
    GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION,
};
use crate::app::gx_store::GxRpcError;
use crate::app::gx_store::host::now_ms;
use crate::app::model::gpui_titlebar_git_menu_state_from_payload;

/// How a read behaves (`refreshGitState`'s options).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GitReadOptions {
    /// Leave `gh --version` / `gh pr view` out and publish the leased GitHub answer instead,
    /// probing a moment later when the lease has run out (`CDXC:Git 2026-07-29`). Only background
    /// and switch-driven reads set this.
    pub(crate) defer_git_hub: bool,
    /// The read is the one the user asked for: it becomes the project the switch-driven refresh
    /// last ran for.
    pub(crate) force: bool,
    /// Draw the menu busy while the read runs.
    pub(crate) publish_busy: bool,
    pub(crate) toast_on_failure: bool,
    /// An automatic read (startup, a project switch, a file change, the poll): a failure never
    /// shows a notice, the Git state just stays in its quiet "not connected" look and the read is
    /// tried again when the computer connects (`CDXC:Git 2026-11-09`).
    pub(crate) background: bool,
}

impl GhostexGpuiApp {
    /// The active project changed (`refreshGitStateForActiveProjectIfNeeded`).
    ///
    /// CDXC:Git 2026-07-29:
    /// Project switching is on the critical path of terminal attach: every call this fires competes
    /// with the attach calls on the same daemon. A project the user switched away from seconds ago
    /// has not changed on disk, so its leased state is drawn and nothing is called; only a cold or
    /// stale project pays for a read, and that read leaves the GitHub CLI probe out of the burst.
    pub(crate) fn gx_store_git_active_project_changed(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_git_start_poll(cx);
        let active = self.git_active_scoped_project_id();
        self.gx_store.git.active_scope = active.clone();
        let Some(active) = active else {
            return;
        };
        if self.gx_store.git.last_refresh_scope.as_deref() == Some(active.as_str()) {
            return;
        }
        let key = ghostex_gx_core::ProjectKey::parse_workspace_project_id(&active);
        if key.as_ref().is_some_and(|key| !key.machine.is_local()) {
            // CDXC:Git 2026-09-23 WHY:
            // A remote selection is refreshed by the machine-scoped project, or the first Commit
            // menu displays the previous computer's Git state until reopened.
            self.gx_store.git.last_refresh_scope = Some(active.clone());
            self.git_refresh_scoped(&active, true, cx);
            return;
        }
        let scope = match self.git_scope_for_project(&active) {
            Ok(scope) => scope,
            Err(_) => return,
        };
        self.gx_store.git.last_refresh_scope = Some(active);
        let project_id = scope.project_id();
        if let Some(memoized) = self.gx_store.git.state_memo.get(&project_id, now_ms()) {
            self.gx_store.git.counters.memo_hits += 1;
            let git_hub = self.gx_store.git.git_hub_memo.peek(&project_id).cloned();
            self.gx_store.git.state = match (memoized.is_repo, git_hub) {
                (true, Some(git_hub)) => memoized.with_git_hub(&git_hub),
                (_, _) => memoized,
            };
            self.git_publish_menu_state(cx);
            return;
        }
        self.git_refresh_state(
            scope,
            GitReadOptions {
                defer_git_hub: true,
                ..GitReadOptions::default()
            },
            cx,
        )
        .detach();
    }

    /// The menu opened, or a scoped refresh was asked for (`refreshTitlebarGitMenuState`,
    /// `refreshGitStateForMessage`). The read is forced and drawn busy. `background` marks an
    /// automatic refresh (startup, a project switch, a file change): it never shows a notice, and
    /// a computer that is not connected yet leaves the quiet "not connected" state until the poll
    /// finds it live. A refresh the user asked for (the menu opening) says so plainly instead.
    ///
    /// CDXC:Git 2026-06-24-21:26:
    /// A refresh names its owner (a local project or a remote one). Scoped remote rows must never
    /// refresh the active local project by accident.
    ///
    /// CDXC:Git 2026-11-09 WHY:
    /// "Git unavailable on Windows: Windows is not connected" appeared on every start because the
    /// restored remote project's automatic refresh ran before the saved computer had connected.
    pub(crate) fn git_refresh_scoped(
        &mut self,
        scoped_id: &str,
        background: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store.git.retry_remote_scope.as_deref() == Some(scoped_id) {
            self.gx_store.git.retry_remote_scope = None;
        }
        match self.git_scope_for_project(scoped_id) {
            Ok(scope) if scope.is_remote() => {
                if self.git_scope_is_active(&scope) {
                    self.gx_store.git.state = GitState {
                        is_busy: true,
                        ..GitState::with_preferences(scope.preferences)
                    };
                    self.git_publish_menu_state(cx);
                }
                self.git_refresh_state(
                    scope,
                    GitReadOptions {
                        background,
                        ..GitReadOptions::default()
                    },
                    cx,
                )
                .detach();
            }
            Ok(scope) => {
                self.git_refresh_state(
                    scope,
                    GitReadOptions {
                        force: true,
                        publish_busy: true,
                        toast_on_failure: !background,
                        background,
                        ..GitReadOptions::default()
                    },
                    cx,
                )
                .detach();
            }
            Err(ScopeMiss::RemoteUnavailable) => {
                let machine_id = ghostex_gx_core::ProjectKey::parse_workspace_project_id(scoped_id)
                    .and_then(|key| key.machine.remote_id().map(str::to_string));
                if background {
                    // The Git button and badge fall back to their quiet "not connected" state; the
                    // poll reads again once the computer's stream is live.
                    self.gx_store.git.retry_remote_scope = Some(scoped_id.to_string());
                    if self.git_active_scoped_project_id().as_deref() == Some(scoped_id) {
                        self.gx_store.git.state = GitState::default();
                        self.git_publish_menu_state(cx);
                    }
                    return;
                }
                self.git_remote_disconnected_notice(machine_id.as_deref(), cx);
            }
            Err(ScopeMiss::Unknown) if background => {}
            Err(ScopeMiss::Unknown) => self.git_toast(
                GitToastLevel::Warning,
                "Git unavailable",
                ToastOptions::described("No active gxserver project is available."),
                cx,
            ),
        }
    }

    /// What the user sees when they ask for a remote project's Git while its computer is not live:
    /// one plain notice in the standard look, saying "connecting" while the connection is being
    /// made and "isn't connected right now" otherwise.
    pub(crate) fn git_remote_disconnected_notice(
        &mut self,
        machine_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let name = machine_id.and_then(remote_machine_saved_name);
        let connecting = machine_id.is_some_and(|machine_id| {
            let machine = ghostex_gx_core::MachineId::Remote(machine_id.to_string());
            self.gx_store
                .core
                .presentation()
                .machine(&machine)
                .is_some_and(|entry| {
                    entry.connection().phase == ghostex_gx_core::ConnectionPhase::Connecting
                })
        });
        let (title, description) = remote_git_disconnected_toast(name.as_deref(), connecting);
        self.git_toast(
            GitToastLevel::Info,
            &title,
            remote_git_toast_options(machine_id, description),
            cx,
        );
    }

    /// The titlebar's `refresh` selector: the menu is opening.
    pub(crate) fn git_refresh_titlebar_menu(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(active) = self.git_active_scoped_project_id() else {
            return;
        };
        match self.git_scope_for_project(&active) {
            Ok(scope) if scope.is_remote() => self.git_refresh_scoped(&active, false, cx),
            Ok(scope) => self
                .git_refresh_state(
                    scope,
                    GitReadOptions {
                        force: true,
                        ..GitReadOptions::default()
                    },
                    cx,
                )
                .detach(),
            Err(ScopeMiss::RemoteUnavailable) => self.git_refresh_scoped(&active, false, cx),
            Err(ScopeMiss::Unknown) => {}
        }
    }

    pub(crate) fn git_scope_is_active(&self, scope: &GitScope) -> bool {
        self.git_active_scoped_project_id().as_deref() == Some(scope.scoped_id().as_str())
    }

    /// Reads `scope`'s state and, while it is still the active project, draws it
    /// (`refreshGitState`, `readSidebarGitState`, `readRemoteSidebarGitState`).
    pub(crate) fn git_refresh_state(
        &mut self,
        scope: GitScope,
        options: GitReadOptions,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<GitState> {
        if options.force && !scope.is_remote() {
            self.gx_store.git.last_refresh_scope = Some(scope.scoped_id());
        }
        let base = GitState::with_preferences(scope.preferences);
        if scope.projectless {
            let state = GitState::not_a_repository(scope.preferences);
            if self.git_scope_is_active(&scope) {
                self.gx_store.git.state = state.clone();
                self.git_publish_menu_state(cx);
            }
            return gpui::Task::ready(state);
        }
        if options.publish_busy && !scope.is_remote() && self.git_scope_is_active(&scope) {
            self.gx_store.git.state = GitState {
                is_busy: true,
                ..base.clone()
            };
            self.git_publish_menu_state(cx);
        }
        self.gx_store.git.counters.state_reads += 1;
        // A remote machine's state has no lease and always carries its GitHub answer, as the old
        // runtime's remote read did.
        let git_hub = scope.is_remote() || !options.defer_git_hub;
        let remote = scope.remote.clone();
        let project_id = scope.project_id();
        let is_remote = scope.is_remote();
        cx.spawn(async move |this, cx| {
            // A remote read that fails is read again quietly first (`CDXC:Git 2026-11-04`); the
            // attempt it ended on decides whether the failure is shown.
            let mut attempt = 0;
            let result = loop {
                let result =
                    calls::read_git_state(remote.clone(), project_id.clone(), git_hub).await;
                let delay_ms = match &result {
                    Err(error) if is_remote => match plan_remote_git_read_failure(
                        attempt,
                        None,
                        &remote_git_read_failure(error),
                    ) {
                        RemoteGitReadStep::RetryAfter(delay_ms) => delay_ms,
                        RemoteGitReadStep::Toast { .. } => break result,
                    },
                    _ => break result,
                };
                cx.background_executor()
                    .timer(Duration::from_millis(delay_ms))
                    .await;
                attempt += 1;
            };
            this.update(cx, |this, cx| {
                let state = this.git_apply_read(&scope, options, attempt, result, cx);
                if this.git_scope_is_active(&scope) {
                    this.gx_store.git.state = state.clone();
                    this.git_publish_menu_state(cx);
                }
                state
            })
            .unwrap_or(base)
        })
    }

    fn git_apply_read(
        &mut self,
        scope: &GitScope,
        options: GitReadOptions,
        attempt: usize,
        result: Result<ProjectGitRead, GxRpcError>,
        cx: &mut gpui::Context<Self>,
    ) -> GitState {
        let project_id = scope.project_id();
        let read = match result {
            Ok(read) => read,
            Err(error) if scope.is_remote() => {
                let name = scope.machine_name();
                let failure = remote_git_read_failure(&error);
                if options.background {
                    // An automatic read that still fails after its quiet retries shows nothing; the
                    // poll tries again.
                    self.gx_store.git.retry_remote_scope = Some(scope.scoped_id());
                } else if let RemoteGitReadStep::Toast { title, description } =
                    plan_remote_git_read_failure(attempt, name.as_deref(), &failure)
                {
                    self.git_toast(
                        GitToastLevel::Warning,
                        &title,
                        remote_git_toast_options(scope.machine_id(), description),
                        cx,
                    );
                }
                return GitState::not_a_repository(scope.preferences);
            }
            Err(_) => {
                if options.toast_on_failure {
                    self.git_toast(
                        GitToastLevel::Error,
                        "Could not refresh Git state",
                        ToastOptions::described("gxserver could not inspect the selected project."),
                        cx,
                    );
                }
                // CDXC:Git 2026-07-29: a failed probe is not a cacheable answer. Drop any leased
                // entry so the next switch reads again instead of drawing a state gxserver could no
                // longer confirm.
                self.gx_store.git.state_memo.delete(&project_id);
                return GitState::with_preferences(scope.preferences);
            }
        };
        let project = scope.state_project();
        if !read.is_repo {
            let state = GitState::not_a_repository(scope.preferences);
            if !scope.is_remote() {
                self.gx_store
                    .git
                    .state_memo
                    .set(&project_id, state.clone(), now_ms());
            }
            return state;
        }
        let git_hub = match &read.git_hub {
            Some(git_hub) => {
                if !scope.is_remote() {
                    self.gx_store
                        .git
                        .git_hub_memo
                        .set(&project_id, git_hub.clone(), now_ms());
                }
                git_hub.clone()
            }
            None => self
                .gx_store
                .git
                .git_hub_memo
                .peek(&project_id)
                .cloned()
                .unwrap_or_default(),
        };
        if options.defer_git_hub && !scope.is_remote() {
            self.git_schedule_deferred_git_hub_probe(scope, cx);
        }
        let state = git_state_from_read(&read, &project, &git_hub);
        if !scope.is_remote() {
            self.gx_store
                .git
                .state_memo
                .set(&project_id, state.clone(), now_ms());
        }
        state
    }

    /// The GitHub probe a deferred read skipped, once its lease has run out
    /// (`scheduleDeferredGitHubProbeIfStale`, `runDeferredGitHubProbe`). A failed probe leaves the
    /// previous lease; the next read reschedules it.
    fn git_schedule_deferred_git_hub_probe(
        &mut self,
        scope: &GitScope,
        cx: &mut gpui::Context<Self>,
    ) {
        let project_id = scope.project_id();
        let git = &mut self.gx_store.git;
        if git.git_hub_memo.is_fresh_key(&project_id, now_ms())
            || !git.pending_git_hub_probes.insert(project_id.clone())
        {
            return;
        }
        let scope = scope.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(GIT_HUB_DEFERRED_PROBE_DELAY_MS))
                .await;
            let result = calls::read_git_hub(None, project_id.clone()).await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.git.pending_git_hub_probes.remove(&project_id);
                let Ok(git_hub) = result else {
                    return;
                };
                this.gx_store.git.counters.git_hub_probes += 1;
                this.gx_store
                    .git
                    .git_hub_memo
                    .set(&project_id, git_hub.clone(), now_ms());
                if this.git_scope_is_active(&scope) && this.gx_store.git.state.is_repo {
                    this.gx_store.git.state =
                        this.gx_store.git.state.clone().with_git_hub(&git_hub);
                    this.git_publish_menu_state(cx);
                }
            });
        })
        .detach();
    }

    /// Drops the leases a write in `project_id` made untrue (`CDXC:Git 2026-07-29`: invalidate at
    /// the write, so a switch back cannot draw the state from before it).
    pub(crate) fn git_forget_leases(&mut self, project_id: &str, git_hub_too: bool) {
        self.gx_store.git.state_memo.delete(project_id);
        if git_hub_too {
            self.gx_store.git.git_hub_memo.delete(project_id);
        }
    }

    /// Marks the drawn state busy or idle without a read (`runGitMutation`).
    pub(crate) fn git_set_busy(&mut self, busy: bool, cx: &mut gpui::Context<Self>) {
        self.gx_store.git.state.is_busy = busy;
        self.git_publish_menu_state(cx);
    }

    /// The preferences the drawn state takes: the active project's (`gitStateForHud`).
    fn git_active_preferences(&self) -> GitPreferences {
        self.git_active_scoped_project_id()
            .and_then(|active| self.git_scope_for_project(&active).ok())
            .map(|scope| scope.preferences)
            .unwrap_or_default()
    }

    /// Draws the titlebar Git menu and the Commit button from the active state
    /// (`postTitlebarGitMenuState`). The payload goes through the same reader the bridge used, so
    /// its bounds and row validation are unchanged.
    pub(crate) fn git_publish_menu_state(&mut self, cx: &mut gpui::Context<Self>) {
        let state = self
            .gx_store
            .git
            .state
            .clone()
            .with_overlaid_preferences(self.git_active_preferences());
        let mut payload = serde_json::to_value(titlebar_menu(&state)).unwrap_or_default();
        payload["type"] = json!(GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_TYPE);
        payload["version"] = json!(GPUI_SIDEBAR_TITLEBAR_GIT_MENU_STATE_MESSAGE_VERSION);
        let Some(next) = gpui_titlebar_git_menu_state_from_payload(&payload.to_string()) else {
            return;
        };
        if self.titlebar_git_menu_state.as_ref() == Some(&next) {
            return;
        }
        self.titlebar_git_menu_state = Some(next);
        self.defer_in_main_window(cx, |app, window, cx| {
            app.refresh_open_git_popup(window, cx);
        });
        cx.notify();
    }
}

/// How a failed remote read reads to the user: no answer at all, the computer's own refusal, or an
/// answer this app could not parse.
fn remote_git_read_failure(error: &GxRpcError) -> RemoteGitReadFailure {
    if error.code.is_some() {
        return RemoteGitReadFailure::Refused(error.message.clone());
    }
    let message = error.message.as_str();
    if !error.unanswered
        && (message.contains("HTTP") || message.contains("JSON") || message.contains("unexpected"))
    {
        return RemoteGitReadFailure::UnexpectedAnswer;
    }
    RemoteGitReadFailure::NotAnswering
}

/// One toast per computer: a read that fails again replaces it instead of stacking a second.
fn remote_git_toast_options(machine_id: Option<&str>, description: String) -> ToastOptions {
    let key = format!("remote-git-unavailable-{}", machine_id.unwrap_or("remote"));
    ToastOptions::keyed(&key).with_description(description)
}
