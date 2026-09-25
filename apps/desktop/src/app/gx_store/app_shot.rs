//! Stages a captured App Shot: types its prompt into the target agent's terminal, or starts the
//! default prompt agent in the active project with the prompt, and says which in a toast.
//!
//! CDXC:AppShots 2026-09-25 WHY:
//! The capture used to go to the app runtime (`onNativeAppShotCaptured`), which chose the target,
//! focused it, posted the prompt back (`postNativeAppShotPromptToSession`) and waited up to 2 s for
//! Rust's answer (`onNativeAppShotPromptResult`) before falling back to a new session. The app
//! runtime port (family F6) keeps that order in process: the target comes from gx-core
//! (`app_shot_target`), the insert is the same exact-surface write
//! (`insert_native_app_shot_prompt_into_*_agents_session`), retried for the same 2 s while a target
//! that had to be focused first mounts, and the fallback is the Git workflows' prompt agent start
//! (`git_start_prompt_agent`), which is what the runtime's `createAgentSessionForProject` was. The
//! prompt, paths and titles are never logged or stored.
//!
//! SEE-ALSO: packages/gx-core/src/app_shot.rs, apps/desktop/src/app/workspace_reconcile.rs (the
//! inserts), apps/desktop/src/app/sidebar_dispatch.rs (the capture's entry).

use ghostex_gx_core::app_shot::AppShotRecentTarget;
#[cfg(target_os = "macos")]
use ghostex_gx_core::app_shot::{
    APP_SHOT_PROMPT_INSERT_TIMEOUT_MS, AppShotCaptureInput, app_shot_target,
    format_app_shot_prompt, normalize_app_shot_capture,
};
#[cfg(target_os = "macos")]
use ghostex_gx_core::{MachineId, ProjectKey, SessionKey};
#[cfg(target_os = "macos")]
use serde_json::Value;

#[cfg(target_os = "macos")]
use crate::GhostexGpuiApp;
#[cfg(target_os = "macos")]
use crate::app::ffi::GpuiAppShotCapture;
#[cfg(target_os = "macos")]
use crate::app::model::GpuiLocalWorkspaceSessionKey;

/// How often a target that is still mounting is tried again.
#[cfg(target_os = "macos")]
const INSERT_RETRY_MS: u64 = 50;

/// The last session an App Shot went to.
#[derive(Default)]
pub(crate) struct AppShotHost {
    recent: Option<AppShotRecentTarget>,
}

#[cfg(target_os = "macos")]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// One App Shot on its way to a terminal.
#[cfg(target_os = "macos")]
struct Staging {
    target: SessionKey,
    prompt: String,
    app_name: String,
    deadline_ms: u64,
    focus_requested: bool,
}

#[cfg(target_os = "macos")]
impl GhostexGpuiApp {
    /// A capture from the App Shots monitor.
    pub(crate) fn gx_store_stage_app_shot(
        &mut self,
        capture: GpuiAppShotCapture,
        cx: &mut gpui::Context<Self>,
    ) {
        let input = AppShotCaptureInput {
            app_name: &capture.app_name,
            image_path: &capture.image_path,
            bundle_identifier: capture.bundle_identifier.as_deref(),
            window_title: capture.window_title.as_deref(),
            window_width: capture.window_width.map(i64::from),
            window_height: capture.window_height.map(i64::from),
            trigger: capture.trigger.as_deref(),
        };
        let Some(app_shot) = normalize_app_shot_capture(&input) else {
            self.app_shot_failed("Could not read the native App Shot.", cx);
            return;
        };
        let include_metadata = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("appShotsMetadataEnabled")
            .and_then(Value::as_bool)
            == Some(true);
        let prompt = format_app_shot_prompt(&app_shot, include_metadata);
        let now = now_ms();
        let target = app_shot_target(
            &self.gx_store.core,
            self.gx_store.app_shot.recent.as_ref(),
            now,
            &|session| self.app_shot_session_mounted(session),
        );
        match target {
            Some(target) => self.app_shot_insert(
                Staging {
                    target,
                    prompt,
                    app_name: app_shot.app_name,
                    deadline_ms: now + APP_SHOT_PROMPT_INSERT_TIMEOUT_MS,
                    focus_requested: false,
                },
                cx,
            ),
            None => self.app_shot_start_agent(prompt, app_shot.app_name, cx),
        }
    }

    /// Whether the app holds a terminal for the session (`nativePaneState` mounted).
    fn app_shot_session_mounted(&self, session: &SessionKey) -> bool {
        match session.machine.remote_id() {
            None => {
                self.local_workspace_session_mappings
                    .contains_key(&GpuiLocalWorkspaceSessionKey {
                        project_id: session.project_id.clone(),
                        session_id: session.session_id.clone(),
                    })
            }
            Some(machine_id) => self.remote_attach_sessions.contains_key(
                &crate::app::helpers::GpuiRemoteAttachSessionKey {
                    remote_machine_id: machine_id.to_string(),
                    project_id: session.project_id.clone(),
                    session_id: session.session_id.clone(),
                },
            ),
        }
    }

    /// Types the prompt into the target's terminal; a target that is not on screen is focused
    /// first and tried until the deadline, then the default prompt agent starts instead.
    fn app_shot_insert(&mut self, mut staging: Staging, cx: &mut gpui::Context<Self>) {
        if self.app_shot_try_insert(&staging.target, &staging.prompt, cx) {
            self.app_shot_remember(staging.target);
            self.dispatch_gpui_app_modal_toast("success", "App Shot Added", &staging.app_name, cx);
            return;
        }
        if now_ms() >= staging.deadline_ms {
            self.app_shot_start_agent(staging.prompt, staging.app_name, cx);
            return;
        }
        if !staging.focus_requested && staging.target.machine.is_local() {
            // The runtime's `stageNativeAppShotInExistingAgentSession` focused a local row before
            // it posted the prompt; a row click's focus does the same here. A remote row is never
            // focused for an App Shot: that could open its attach tab (CDXC:AppShots 2026-06-26-04:27).
            staging.focus_requested = true;
            let row_id = staging.target.to_sidebar_session_id();
            self.gx_store_focus_activated_session(&row_id, cx);
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(INSERT_RETRY_MS))
                .await;
            let _ = this.update(cx, |app, cx| app.app_shot_insert(staging, cx));
        })
        .detach();
    }

    fn app_shot_try_insert(
        &mut self,
        target: &SessionKey,
        prompt: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let scoped_session_id = target.to_focus_state_session_id();
        if target.machine.remote_id().is_some() {
            let Some(reference) =
                crate::app::helpers::gpui_remote_attach_session_reference_from_project_id(
                    &scoped_session_id,
                )
            else {
                return false;
            };
            return self
                .insert_native_app_shot_prompt_into_remote_agents_session(&reference, prompt, cx);
        }
        // The insert finds the tab through the App Shot map, which the runtime's focus posts fed;
        // the workspace's own map of attached sessions names the same tab.
        let key = GpuiLocalWorkspaceSessionKey {
            project_id: target.project_id.clone(),
            session_id: target.session_id.clone(),
        };
        if let Some(shell_session_id) = self.local_workspace_session_mappings.get(&key).copied() {
            self.local_app_shot_session_mappings
                .entry(scoped_session_id.clone())
                .or_insert(shell_session_id);
        }
        if !self.insert_native_app_shot_prompt_into_local_agents_session(
            &scoped_session_id,
            prompt,
            cx,
        ) {
            return false;
        }
        // The insert selected the tab; the store's focus follows as it does for a tab click.
        self.dispatch_gpui_workspace_tab_session_selected(
            &target.project_id,
            &target.session_id,
            false,
            false,
            cx,
        );
        true
    }

    /// `stageNativeAppShotInAgentSession`'s fallback: the default prompt agent in the active
    /// project, started with the prompt.
    fn app_shot_start_agent(
        &mut self,
        prompt: String,
        app_name: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let presentation = self.gx_store.core.presentation();
        let Some(loaded) = presentation.loaded(&MachineId::Local) else {
            self.app_shot_failed("The local agent service is not ready.", cx);
            return;
        };
        // `activeDomainProject`: the active local project, else the first saved one.
        let machine = presentation.machine(&MachineId::Local);
        let is_saved = |project_id: &str| {
            machine
                .and_then(|machine| machine.domain_project(project_id))
                .is_some_and(|row| {
                    row.get("isRecentProject").and_then(Value::as_bool) != Some(true)
                        && row.get("isQuick").and_then(Value::as_bool) != Some(true)
                        && row
                            .pointer("/launchSettings/isQuick")
                            .and_then(Value::as_bool)
                            != Some(true)
                })
        };
        let project_id = match self.gx_store.core.focus().active_project.as_ref() {
            Some(project) if project.machine.is_local() => machine
                .and_then(|machine| machine.domain_project(&project.project_id))
                .map(|_| project.project_id.clone()),
            _ => loaded
                .projects()
                .iter()
                .map(|project| project.project_id.clone())
                .find(|project_id| is_saved(project_id)),
        };
        let Some(project_id) = project_id else {
            self.app_shot_failed("Open a project before using App Shots.", cx);
            return;
        };
        let agent_id = self.git_default_prompt_agent_id(None);
        let Some(agent) = self
            .git_hud_agent(&agent_id)
            .filter(|agent| agent.has_command())
        else {
            self.app_shot_failed(
                "Choose a configured default prompt agent before using App Shots.",
                cx,
            );
            return;
        };
        let Ok(scope) = self.git_scope_for_key(ProjectKey::local(project_id)) else {
            self.app_shot_failed("Open a project before using App Shots.", cx);
            return;
        };
        let task = self.git_start_prompt_agent(
            &scope,
            agent,
            super::git::PromptAgentLaunch {
                prompt,
                ..Default::default()
            },
            cx,
        );
        cx.spawn(async move |this, cx| {
            let created = task.await;
            let _ = this.update(cx, |app, cx| match created {
                Ok(session) => {
                    app.app_shot_remember(session);
                    app.dispatch_gpui_app_modal_toast("success", "App Shot Added", &app_name, cx);
                }
                Err(_) => {
                    app.app_shot_failed("Could not stage the App Shot in an agent session.", cx)
                }
            });
        })
        .detach();
    }

    /// Writes the prompt into the tab's live input: the chat composer when the session shows its
    /// Chat view, the terminal otherwise (a paste, so the prompt's line breaks do not submit it).
    /// `false` when the tab has neither yet, which the caller retries until its deadline.
    ///
    /// CDXC:AppShots 2026-09-25 WHY:
    /// The insert wrote only to the AppKit Ghostty surface map, which nothing has filled since the Agents terminals moved to the GPUI engine and the chat view, so every App Shot declined the focused agent and started a new session (before and after the app runtime port alike). It now writes where Add to Session Context and a restored stash write: the chat composer or the engine terminal.
    pub(crate) fn app_shot_write_into_agents_tab(
        &mut self,
        shell_session_id: crate::app::model::TerminalSessionId,
        prompt: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.agents_chat_mode_sessions.contains(&shell_session_id) {
            return self.insert_prompt_into_session_chat(shell_session_id, prompt, cx);
        }
        self.ensure_agents_gpui_engine_terminal_view(shell_session_id, cx);
        let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .map(|record| record.view.clone())
        else {
            return false;
        };
        view.update(cx, |view, cx| view.paste_text(prompt, cx));
        true
    }

    fn app_shot_remember(&mut self, session: SessionKey) {
        self.gx_store.app_shot.recent = Some(AppShotRecentTarget {
            session,
            at_ms: now_ms(),
        });
    }

    fn app_shot_failed(&mut self, description: &str, cx: &mut gpui::Context<Self>) {
        self.dispatch_gpui_app_modal_toast("warning", "App Shot Failed", description, cx);
    }
}
