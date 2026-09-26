//! The on-demand code-server runtime behind the Code view: starting it for the current
//! project, finishing and retrying its load, stopping it, and installing or uninstalling its
//! component. Moved verbatim out of `app/workarea.rs` on 2026-09-20.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
use anyhow::Result;
use futures::StreamExt as _;
use futures::channel::mpsc;
use std::time::Instant;

impl GhostexGpuiApp {
    fn trace_source_runtime_state(&self, phase: &str) {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref();
        let remote = snapshot
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .and_then(|id| gpui_remote_project_reference_from_project_id(&id.0));
        let connection = remote.as_ref().and_then(|remote| {
            self.remote_gxserver_connections
                .get(&remote.remote_machine_id)
        });
        let surface = self
            .project_workarea_runtime_cef_surfaces
            .get(&ProjectWorkareaCefSurfaceSlotKey::Source);
        support_logs::append_for_scenario(
            support_logs::GpuiSupportLog::SidebarRefresh,
            "native.sidebar.refresh",
            "gpui.codeRuntime.state",
            serde_json::json!({
                "phase": phase,
                "activeMode": format!("{:?}", self.active_mode),
                "awake": self.project_editor_shell.is_mode_awake(TitlebarMode::Source),
                "mayBeVisible": self.project_workarea_runtime_cef_surface_may_be_visible(ProjectWorkareaCefSurfaceSlotKey::Source),
                "hasSnapshot": snapshot.is_some(),
                "hasProject": snapshot.is_some_and(|snapshot| snapshot.active_project_id.is_some()),
                "hasSourceIdentity": snapshot.is_some_and(|snapshot| snapshot.surface_ids.source_workarea_id.is_some()),
                "hasProjectPath": snapshot.is_some_and(|snapshot| snapshot.in_memory_project_path.is_some()),
                "sourceAvailable": snapshot.is_some_and(|snapshot| snapshot.feature_availability.source),
                "quickProjectless": snapshot.is_some_and(|snapshot| snapshot.is_quick_projectless),
                "remote": remote.is_some(),
                "connected": connection.is_some(),
                "hasComponentPlatform": connection.is_some_and(|connection| connection.code_server_component_platform.is_some()),
                "hasTarget": snapshot.and_then(|snapshot| self.source_code_server_runtime_target(snapshot)).is_some(),
                "runtimeState": format!("{:?}", self.source_code_server_runtime.state),
                "generation": self.source_code_server_runtime.generation,
                "hasChild": self.source_code_server_runtime.child.is_some(),
                "hasSurface": surface.is_some(),
                "pageReady": surface.is_some_and(|surface| surface.page_ready()),
            }),
        );
    }

    pub(crate) fn ensure_source_code_server_runtime_for_current_context(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CodeEditor 2026-06-24-23:17:
        Source startup is lazy and visible-workarea scoped. Selecting or focusing awake Source may launch the shared code-server process in the background, but CEF creation still waits for the runtime readiness result and an authorized folder URL; no hidden Source CEF prewarm, fallback localhost adoption, persistent URL storage, or renderer-provided path is allowed.
        */
        if self.active_mode == TitlebarMode::Source {
            self.trace_source_runtime_state("ensure");
        }
        if self.source_code_server_runtime.state == SourceCodeServerRuntimeLaunchState::Installing {
            return false;
        }
        if !self.project_workarea_runtime_cef_surface_may_be_visible(
            ProjectWorkareaCefSurfaceSlotKey::Source,
        ) {
            return false;
        }
        let Some(target) = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot))
        else {
            return false;
        };
        let settings = SourceCodeServerRuntimeSettings::from_sidebar_runtime_settings(
            &self.sidebar_runtime_settings_snapshot,
        );
        let mut changed = self.refresh_source_code_server_runtime_child(cx);
        if matches!(
            self.source_code_server_runtime.state,
            SourceCodeServerRuntimeLaunchState::InstallRequired
                | SourceCodeServerRuntimeLaunchState::Failed
        ) && self.source_code_server_runtime.target.as_ref() == Some(&target)
            && self.source_code_server_runtime.settings.as_ref() == Some(&settings)
        {
            return changed;
        }
        if self
            .source_code_server_runtime
            .can_reuse_ready_process(&target, &settings)
        {
            let previous_target = self.source_code_server_runtime.target.clone();
            if previous_target.as_ref() != Some(&target) {
                self.source_code_server_runtime
                    .pending_remote_prompt_editor_request = None;
            }
            self.source_code_server_runtime
                .set_ready_target(target, settings);
            changed |= previous_target != self.source_code_server_runtime.target;
            return changed;
        }
        if self
            .source_code_server_runtime
            .launching_matches(&target, &settings)
        {
            return changed;
        }
        if self
            .source_code_server_runtime
            .launching_can_share(&target, &settings)
        {
            if self.source_code_server_runtime.target.as_ref() != Some(&target) {
                self.source_code_server_runtime
                    .pending_remote_prompt_editor_request = None;
            }
            self.source_code_server_runtime.target = Some(target);
            return true;
        }
        if self
            .source_code_server_runtime
            .child_is_within_startup_grace()
            && self.source_code_server_runtime.settings.as_ref() == Some(&settings)
        {
            return changed;
        }

        if self.source_code_server_runtime.child.is_some() {
            self.stop_source_code_server_runtime(cx);
        }

        let generation = self.source_code_server_runtime.next_generation();
        let started_at = Instant::now();
        self.source_code_server_runtime
            .set_launching(target.clone(), settings.clone(), started_at);
        self.trace_source_runtime_state("launchStarted");
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SOURCE_CODE_SERVER_LOADING_PLACEHOLDER_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.source_code_server_runtime.generation == generation
                    && this.source_code_server_runtime.state
                        == SourceCodeServerRuntimeLaunchState::Launching
                {
                    cx.notify();
                }
            });
        })
        .detach();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // Availability verifies the installed component and walks its files; it belongs
            // to the same background launch as SSH, not Code focus or the render loop.
            // Start the readiness budget afterward: verifying a large archive can outlast it.
            let result = background
                .spawn(async move {
                    match source_code_server_runtime_availability(&target) {
                        SourceCodeServerRuntimeAvailability::Available => {
                            let startup_deadline =
                                Instant::now() + SOURCE_CODE_SERVER_STARTUP_TIMEOUT;
                            Ok(source_code_server_start_runtime_for_target(
                                target,
                                settings,
                                startup_deadline,
                            ))
                        }
                        unavailable => Err((target, settings, unavailable)),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => this.finish_source_code_server_runtime_start(generation, result, cx),
                Err((target, settings, availability)) => {
                    if this.source_code_server_runtime.generation != generation {
                        return;
                    }
                    let target = this
                        .source_code_server_runtime
                        .target
                        .clone()
                        .filter(|current| current.can_share_runtime_with(&target))
                        .unwrap_or(target);
                    match availability {
                        SourceCodeServerRuntimeAvailability::InstallRequired => {
                            this.source_code_server_runtime
                                .set_install_required(target, settings);
                        }
                        SourceCodeServerRuntimeAvailability::Failed(failure) => {
                            this.source_code_server_runtime
                                .set_failed(target, settings, None, None, failure);
                        }
                        SourceCodeServerRuntimeAvailability::Available => unreachable!(),
                    }
                    this.trace_source_runtime_state("availabilityFinished");
                    cx.notify();
                }
            });
        })
        .detach();
        true
    }

    pub(crate) fn source_code_server_runtime_target(
        &self,
        snapshot: &GpuiProjectSnapshot,
    ) -> Option<SourceCodeServerRuntimeTarget> {
        let active_project_id = snapshot.active_project_id.as_ref()?;
        let endpoint = if let Some(reference) =
            gpui_remote_project_reference_from_project_id(active_project_id.0.as_str())
        {
            let connection_generation = self
                .remote_gxserver_connect_generations
                .get(reference.remote_machine_id.as_str())
                .copied()?;
            let connection = self
                .remote_gxserver_connections
                .get(reference.remote_machine_id.as_str())?;
            let component_platform = connection.code_server_component_platform.clone()?;
            let settings = shared_settings::shared_sidebar_settings_snapshot();
            let machine_config = gpui_remote_machine_config_from_settings(
                settings.object(),
                reference.remote_machine_id.as_str(),
            )?;
            SourceCodeServerRuntimeEndpoint::Remote {
                component_platform,
                connection_generation,
                execution_target: connection.execution_target.clone(),
                machine_config,
                remote_machine_id: reference.remote_machine_id,
            }
        } else {
            SourceCodeServerRuntimeEndpoint::Local
        };
        source_code_server_runtime_target_from_project_snapshot(snapshot, endpoint)
    }

    pub(crate) fn finish_source_code_server_runtime_start(
        &mut self,
        generation: u64,
        result: Result<
            (
                SourceCodeServerRuntimeTarget,
                SourceCodeServerRuntimeSettings,
                SourceCodeServerRuntimeStartOutput,
            ),
            (
                SourceCodeServerRuntimeTarget,
                SourceCodeServerRuntimeSettings,
                String,
            ),
        >,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.source_code_server_runtime.generation != generation {
            self.trace_source_runtime_state("staleLaunchFinished");
            if let Ok((_, _, mut output)) = result {
                let _ = output.child.kill();
                let _ = output.child.wait();
            }
            return;
        }

        match result {
            Ok((launched_target, settings, output))
                if output.http_runtime_ready && output.prompt_editor_ipc_ready =>
            {
                let target = self
                    .source_code_server_runtime
                    .target
                    .clone()
                    .filter(|target| target.can_share_runtime_with(&launched_target))
                    .unwrap_or(launched_target);
                self.source_code_server_runtime.set_ready(
                    target,
                    settings,
                    output.child,
                    output.started_at,
                    output.runtime_origin,
                    output.prompt_editor_ipc_ready,
                );
                self.watch_source_code_server_runtime_child(generation, cx);
                self.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
                self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
            }
            Ok((target, settings, output)) => {
                self.source_code_server_runtime.set_failed(
                    target,
                    settings,
                    Some(output.child),
                    Some(output.started_at),
                    SourceCodeServerRuntimeFailure::Launch,
                );
                self.remove_project_workarea_runtime_cef_surface(
                    ProjectWorkareaCefSurfaceSlotKey::Source,
                    cx,
                );
                self.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
            }
            Err((target, settings, _message)) => {
                self.source_code_server_runtime.set_failed(
                    target,
                    settings,
                    None,
                    None,
                    SourceCodeServerRuntimeFailure::Launch,
                );
                self.remove_project_workarea_runtime_cef_surface(
                    ProjectWorkareaCefSurfaceSlotKey::Source,
                    cx,
                );
                self.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
            }
        }
        self.deliver_pending_remote_prompt_editor_request_if_ready(cx);
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        self.trace_source_runtime_state("launchFinished");
        cx.notify();
    }

    /// CDXC:CodeEditor 2026-09-23 WHY:
    /// A matching CEF URL does not prove its owned Code process is alive. Reconnect can lose the new SSH child after readiness; retire that dead page and show Retry instead of leaving a permanently blank view.
    pub(crate) fn refresh_source_code_server_runtime_child(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.source_code_server_runtime.refresh_child_exit() {
            return false;
        }
        self.remove_project_workarea_runtime_cef_surface(
            ProjectWorkareaCefSurfaceSlotKey::Source,
            cx,
        );
        self.trace_source_runtime_state("childExited");
        cx.notify();
        true
    }

    fn watch_source_code_server_runtime_child(
        &self,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_watching = this
                    .update(cx, |this, cx| {
                        if this.source_code_server_runtime.generation != generation
                            || this.source_code_server_runtime.state
                                != SourceCodeServerRuntimeLaunchState::Ready
                        {
                            return false;
                        }
                        !this.refresh_source_code_server_runtime_child(cx)
                    })
                    .unwrap_or(false);
                if !keep_watching {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn stop_source_code_server_runtime(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        self.trace_source_runtime_state("stop");
        let changed = self.source_code_server_runtime.stop();
        let removed = self.remove_project_workarea_runtime_cef_surface(
            ProjectWorkareaCefSurfaceSlotKey::Source,
            cx,
        );
        if changed || removed {
            self.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
        }
        changed || removed
    }

    pub(crate) fn restart_source_code_server_runtime_after_settings_change(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .project_editor_shell
            .is_mode_awake(TitlebarMode::Source)
        {
            return false;
        }
        let stopped = self.stop_source_code_server_runtime(cx);
        self.ensure_source_code_server_runtime_for_current_context(cx) || stopped
    }

    pub(crate) fn retry_source_code_server_load(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(
            self.source_code_server_runtime.failure,
            Some(
                SourceCodeServerRuntimeFailure::InstallDownload
                    | SourceCodeServerRuntimeFailure::InstallIntegrity
                    | SourceCodeServerRuntimeFailure::InstallOther
            )
        ) {
            self.install_source_code_server_component(cx);
            return;
        }
        self.stop_source_code_server_runtime(cx);
        self.ensure_source_code_server_runtime_for_current_context(cx);
        cx.notify();
    }

    pub(crate) fn install_source_code_server_component(&mut self, cx: &mut gpui::Context<Self>) {
        if self.source_code_server_runtime.state == SourceCodeServerRuntimeLaunchState::Installing {
            return;
        }
        let target = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot));
        let settings = target.as_ref().map(|_| {
            SourceCodeServerRuntimeSettings::from_sidebar_runtime_settings(
                &self.sidebar_runtime_settings_snapshot,
            )
        });
        self.stop_source_code_server_runtime(cx);
        let generation = self.source_code_server_runtime.next_generation();
        self.source_code_server_runtime
            .set_installing(target.clone(), settings.clone());

        let (progress_tx, mut progress_rx) = mpsc::unbounded();
        cx.spawn(async move |this, cx| {
            while let Some(progress) = progress_rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    if this.source_code_server_runtime.generation != generation
                        || this.source_code_server_runtime.state
                            != SourceCodeServerRuntimeLaunchState::Installing
                    {
                        return;
                    }
                    this.source_code_server_runtime.install_progress = Some(progress);
                    this.refresh_gpui_plugins_modal(cx);
                    cx.notify();
                });
            }
        })
        .detach();

        let background = cx.background_executor().clone();
        let install_target = target.clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    source_code_server_install_component(install_target.as_ref(), progress_tx)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.source_code_server_runtime.generation != generation {
                    return;
                }
                match result {
                    Ok(_) => {
                        this.source_code_server_runtime.reset_after_install();
                        this.ensure_source_code_server_runtime_for_current_context(cx);
                    }
                    Err(message) => {
                        let failure = source_code_server_install_failure(&message);
                        this.source_code_server_runtime
                            .set_install_failed(target, settings, failure);
                    }
                }
                this.refresh_gpui_plugins_modal(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn uninstall_component(
        &mut self,
        name: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Result<bool, String> {
        if name == SOURCE_CODE_SERVER_COMPONENT_NAME {
            self.stop_source_code_server_runtime(cx);
        }
        let Some(store) = on_demand_component_store()? else {
            return Err("The sealed component manifest is unavailable.".to_string());
        };
        let installed = store.query_current(name)?;
        let removed = store.uninstall(name, &installed.version)?;
        if removed && name == SOURCE_CODE_SERVER_COMPONENT_NAME {
            self.ensure_source_code_server_runtime_for_current_context(cx);
        }
        self.refresh_gpui_plugins_modal(cx);
        cx.notify();
        Ok(removed)
    }
}
