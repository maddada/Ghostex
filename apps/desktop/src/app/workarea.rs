// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: project-workarea runtime CEF surfaces, source code server, tab scroll handles

use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::ClipboardItem;
use gpui::Entity;
use gpui::ScrollHandle;

use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

#[derive(Clone)]
enum ExtensionViewRuntimeState {
    Starting,
    Ready(ProjectWorkareaRealRuntimeUrl),
    Failed(String),
}

fn extension_view_runtime_states() -> &'static Mutex<HashMap<ExtensionId, ExtensionViewRuntimeState>>
{
    static STATES: OnceLock<Mutex<HashMap<ExtensionId, ExtensionViewRuntimeState>>> =
        OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn extension_view_runtime_state(id: ExtensionId) -> Option<ExtensionViewRuntimeState> {
    extension_view_runtime_states()
        .lock()
        .ok()?
        .get(&id)
        .cloned()
}

fn set_extension_view_runtime_state(id: ExtensionId, state: ExtensionViewRuntimeState) {
    if let Ok(mut states) = extension_view_runtime_states().lock() {
        states.insert(id, state);
    }
}

impl GhostexGpuiApp {
    pub(crate) fn persist_shell_layout_state(&self) {
        persist_gpui_workspace_shell_state(self);
    }

    pub(crate) fn project_scoped_workarea_availability(&self) -> ProjectScopedWorkareaAvailability {
        project_scoped_workarea_availability_from_latest_sidebar_snapshot(
            self.latest_sidebar_project_snapshot.as_ref(),
            ProjectScopedWorkareaAvailability::from_env_bridge(),
        )
    }

    pub(crate) fn refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-24-10:12:
        The real CEF surface map is active app-level ownership, not proof-only evidence. Refresh may prune already-owned Source/Kanban/Automate/Manage CefSurface entities when their safe gate disappears, but it still cannot create a surface, issue or store a URL, synthesize fallback navigation, mount hidden/offscreen views, use WKWebView/WebKit, or persist/log private runtime data.

        CDXC:Workarea 2026-06-28-17:09:
        The old slot, URL-issuance, startup-readiness, and owner-gate proof maps are removed from runtime state. Refresh now only prunes already-owned project workarea CefSurface entities whose current explicit project context can no longer provide a direct runtime URL.

        CDXC:Workarea 2026-06-29-00:15:
        Refresh must also prune already-owned surfaces whose stored runtime URL identity differs from the current direct runtime URL so a valid new project cannot inherit the previous project's slot-owned CEF view.
        */
        self.prune_project_workarea_runtime_cef_surfaces_for_current_gates(cx)
    }

    pub(crate) fn ensure_source_code_server_runtime_for_current_context(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CodeEditor 2026-06-24-23:17:
        Source startup is lazy and visible-workarea scoped. Selecting or focusing awake Source may launch the shared code-server process in the background, but CEF creation still waits for the runtime readiness result and an authorized folder URL; no hidden Source CEF prewarm, fallback localhost adoption, persistent URL storage, or renderer-provided path is allowed.
        */
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
        let mut changed = self.source_code_server_runtime.refresh_child_exit();
        match source_code_server_runtime_availability(&target) {
            SourceCodeServerRuntimeAvailability::Available => {}
            SourceCodeServerRuntimeAvailability::InstallRequired => {
                let was_current = self.source_code_server_runtime.state
                    == SourceCodeServerRuntimeLaunchState::InstallRequired
                    && self.source_code_server_runtime.target.as_ref() == Some(&target)
                    && self.source_code_server_runtime.settings.as_ref() == Some(&settings);
                if !was_current {
                    self.source_code_server_runtime
                        .set_install_required(target, settings);
                    changed = true;
                }
                return changed;
            }
            SourceCodeServerRuntimeAvailability::Failed(failure) => {
                self.source_code_server_runtime
                    .set_failed(target, settings, None, None, failure);
                return true;
            }
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
        let startup_deadline = started_at + SOURCE_CODE_SERVER_STARTUP_TIMEOUT;
        self.source_code_server_runtime
            .set_launching(target.clone(), settings.clone(), started_at);
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
            let result = background
                .spawn(async move {
                    source_code_server_start_runtime_for_target(target, settings, startup_deadline)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_source_code_server_runtime_start(generation, result, cx);
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
        cx.notify();
    }

    pub(crate) fn stop_source_code_server_runtime(&mut self, cx: &mut gpui::Context<Self>) -> bool {
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

    pub(crate) fn hide_code_view_tab(&mut self, cx: &mut gpui::Context<Self>) {
        let _ = self.set_project_workarea_titlebar_mode_hidden(TitlebarMode::Source, true, cx);
    }

    pub(crate) fn set_project_workarea_titlebar_mode_hidden(
        &mut self,
        mode: TitlebarMode,
        hidden: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        let Some(settings_key) = titlebar_mode_view_tab_hidden_settings_key(mode) else {
            return Err("That built-in feature does not have a visibility switch.".to_string());
        };
        let mut settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.insert(settings_key.to_string(), serde_json::Value::Bool(hidden));
        shared_settings::write_shared_sidebar_settings_object(settings)
            .map_err(|error| format!("Could not update the built-in feature setting: {error:?}"))?;
        self.refresh_gpui_plugins_modal(cx);
        cx.notify();
        Ok(())
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

    pub(crate) fn remove_project_workarea_runtime_cef_surface(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(owned_surface) = self.project_workarea_runtime_cef_surfaces.remove(&slot_key)
        else {
            return false;
        };
        owned_surface
            .surface
            .update(cx, |surface, _| surface.set_visible(false));
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        true
    }

    fn installed_extension_view(&self, id: ExtensionId) -> Option<&GpuiInstalledExtension> {
        self.extensions_snapshot
            .installed
            .get(id.as_str())
            .filter(|extension| {
                extension.enabled
                    && extension.placements.contains(&GpuiExtensionPlacement::View)
                    && extension.placement == Some(GpuiExtensionPlacement::View)
            })
    }

    fn extension_view_runtime_url(&self, id: ExtensionId) -> Option<ProjectWorkareaRealRuntimeUrl> {
        if let Some(custom_view) = gpui_custom_view(id) {
            if !custom_view.enabled {
                return None;
            }
            return ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(custom_view.url);
        }
        let extension = self.installed_extension_view(id)?;
        let presentation = gpui_extension_view_presentation(id)?;
        if presentation.server_is_static {
            return extension
                .runtime_url
                .clone()
                .and_then(ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url);
        }
        match extension_view_runtime_state(id) {
            Some(ExtensionViewRuntimeState::Ready(url)) => Some(url),
            Some(ExtensionViewRuntimeState::Starting | ExtensionViewRuntimeState::Failed(_))
            | None => None,
        }
    }

    pub(crate) fn extension_view_placeholder_signature(
        &self,
        id: ExtensionId,
    ) -> ProjectEditorPlaceholderSignature {
        let mode = TitlebarMode::Extension(id);
        let title = gpui_extension_view_presentation(id)
            .map(|presentation| presentation.title)
            .unwrap_or_else(|| id.as_str().to_string());
        if gpui_custom_view(id).is_some() {
            return ProjectEditorPlaceholderSignature {
                mode,
                title: Some(format!("Opening {title}…")),
                message: String::new(),
                actions: Vec::new(),
            };
        }
        let (title, message) = match extension_view_runtime_state(id) {
            Some(ExtensionViewRuntimeState::Failed(error)) => {
                (Some(format!("{title} could not start")), error)
            }
            Some(ExtensionViewRuntimeState::Starting) => {
                (Some(format!("Starting {title}…")), String::new())
            }
            Some(ExtensionViewRuntimeState::Ready(_)) => {
                (Some(format!("Opening {title}…")), String::new())
            }
            None if self.installed_extension_view(id).is_none() => (
                Some("Extension unavailable".to_string()),
                "This extension is no longer installed, enabled, or assigned to View.".to_string(),
            ),
            None => (Some(format!("Preparing {title}…")), String::new()),
        };
        ProjectEditorPlaceholderSignature {
            mode,
            title,
            message,
            actions: Vec::new(),
        }
    }

    pub(crate) fn ensure_extension_view_runtime_for_current_context(
        &mut self,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.installed_extension_view(id).is_none()
            || gpui_extension_view_presentation(id).is_some_and(|value| value.server_is_static)
            || matches!(
                extension_view_runtime_state(id),
                Some(ExtensionViewRuntimeState::Starting | ExtensionViewRuntimeState::Ready(_))
            )
        {
            return false;
        }
        let Some(snapshot) = self.latest_sidebar_project_snapshot.as_ref() else {
            return false;
        };
        let session_id = self
            .active_extension_session_details()
            .and_then(|details| details.get("sessionId").cloned())
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        let project_id = snapshot.active_project_id.as_ref().map(|id| id.0.as_str());
        let project = project_id.and_then(|id| self.extension_projects.get(id));
        let params = serde_json::json!({
            "id": id.as_str(),
            "context": {
                "sessionId": session_id,
                "projectPath": project
                    .and_then(|project| project.path.as_deref())
                    .or_else(|| snapshot.in_memory_project_path.as_deref().and_then(|path| path.to_str()))
                    .unwrap_or(""),
                "projectName": project
                    .map(|project| project.name.as_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or(snapshot.display_name.as_str()),
                "worktree": project.is_some_and(|project| project.is_worktree),
                "worktreeBranch": project
                    .and_then(|project| project.worktree_branch.as_deref())
                    .unwrap_or(""),
            },
        });
        set_extension_view_runtime_state(id, ExtensionViewRuntimeState::Starting);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/startExtension",
                        &params,
                        Duration::from_secs(65),
                    )
                })
                .await;
            let next_state = match result {
                Ok(result) => {
                    let status = result.get("status").and_then(serde_json::Value::as_object);
                    match status
                        .and_then(|status| status.get("state"))
                        .and_then(serde_json::Value::as_str)
                    {
                        Some("ready") => status
                            .and_then(|status| status.get("url"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                            .and_then(ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url)
                            .map(ExtensionViewRuntimeState::Ready)
                            .unwrap_or_else(|| {
                                ExtensionViewRuntimeState::Failed(
                                    "The extension did not provide a runtime URL.".to_string(),
                                )
                            }),
                        Some("failed") => ExtensionViewRuntimeState::Failed(
                            status
                                .and_then(|status| status.get("error"))
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("The extension failed to launch.")
                                .to_string(),
                        ),
                        _ => ExtensionViewRuntimeState::Failed(
                            "The extension did not reach its ready state.".to_string(),
                        ),
                    }
                }
                Err(error) => ExtensionViewRuntimeState::Failed(error),
            };
            set_extension_view_runtime_state(id, next_state);
            let _ = this.update(cx, |this, cx| {
                this.refresh_extensions_in_background(cx);
                this.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                cx.notify();
            });
        })
        .detach();
        true
    }

    fn extension_view_bridge_event_handler(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        cx: &mut gpui::Context<Self>,
    ) -> cef::ExtensionBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        Rc::new(move |event: cef::ExtensionBridgeEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            let foreground = foreground.clone();
            foreground
                .clone()
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, _window, cx| {
                        let response_app = cx.entity().downgrade();
                        let response_async_cx = cx.to_async();
                        let response_foreground = cx.foreground_executor().clone();
                        let responder: GpuiExtensionBridgeResponder = Rc::new(move |payload| {
                            let response_app = response_app.clone();
                            let mut response_async_cx = response_async_cx.clone();
                            response_foreground
                                .spawn(async move {
                                    let _ = response_app.update_in(
                                        &mut response_async_cx,
                                        |this, _window, cx| {
                                            let Some(surface) = this
                                                .project_workarea_runtime_cef_surfaces
                                                .get(&slot_key)
                                                .map(|owned| owned.surface.clone())
                                            else {
                                                return;
                                            };
                                            surface.update(cx, |surface, _| {
                                                surface.dispatch_extension_bridge_message(&payload);
                                            });
                                        },
                                    );
                                })
                                .detach();
                        });
                        let close_app = cx.entity().downgrade();
                        let close_async_cx = cx.to_async();
                        let close_foreground = cx.foreground_executor().clone();
                        let close_handler: GpuiExtensionCloseHandler = Rc::new(move || {
                            let close_app = close_app.clone();
                            let mut close_async_cx = close_async_cx.clone();
                            close_foreground
                                .spawn(async move {
                                    let _ = close_app.update_in(
                                        &mut close_async_cx,
                                        |this, window, cx| {
                                            if this.set_active_mode(
                                                TitlebarMode::Agents,
                                                window,
                                                cx,
                                            ) {
                                                cx.notify();
                                            }
                                        },
                                    );
                                })
                                .detach();
                        });
                        this.handle_extension_bridge_event(
                            event,
                            this.extension_surface_context(GpuiExtensionPlacement::View),
                            responder,
                            Some(close_handler),
                            cx,
                        );
                    });
                })
                .detach();
        })
    }

    pub(crate) fn project_workarea_runtime_cef_surface_replacement_permitted(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-24-11:03:
        Runtime placeholder replacement now follows real navigable CEF URL authority, not the retired proof-only owner gates. Kanban, Automate, and Manage can replace placeholders only when the current explicit project snapshot can issue a first-party bundled CEF URL.

        CDXC:CodeEditor 2026-06-24-23:17:
        Source joins this same replacement edge only after the app-owned code-server runtime has reached the ready state for the current explicit sidebar project target. The URL may be used immediately for CefSurface creation but is not retained in shell state, logs, or proof JSON.
        */
        self.project_workarea_runtime_url_for_slot(slot_key)
            .is_some()
    }

    pub(crate) fn project_workarea_runtime_url_for_slot(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        if let ProjectWorkareaCefSurfaceSlotKey::Extension(id) = slot_key {
            if gpui_custom_view(id).is_none() {
                self.latest_sidebar_project_snapshot.as_ref()?;
            }
            return self.extension_view_runtime_url(id);
        }
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        match slot_key {
            ProjectWorkareaCefSurfaceSlotKey::Source => {
                let target = self.source_code_server_runtime_target(snapshot)?;
                self.source_code_server_runtime
                    .runtime_url_for_target(&target)
            }
            ProjectWorkareaCefSurfaceSlotKey::Kanban => {
                kanban_workarea_runtime_url_from_project_snapshot(snapshot)
            }
            ProjectWorkareaCefSurfaceSlotKey::Automate => {
                automate_workarea_runtime_url_from_project_snapshot(
                    snapshot,
                    &self.sidebar_runtime_settings_snapshot,
                )
            }
            ProjectWorkareaCefSurfaceSlotKey::Manage => {
                manage_workarea_runtime_url_from_project_snapshot(snapshot)
            }
            ProjectWorkareaCefSurfaceSlotKey::Extension(_) => None,
        }
    }

    pub(crate) fn project_workarea_runtime_cef_surface_may_be_visible(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        let mode = slot_key.titlebar_mode();
        self.active_mode == mode
            && self.project_editor_shell.is_mode_awake(mode)
            && !self.browser_tab_drag_active
            && !self.command_tab_drag_active
            && !self.workspace_tab_drag_active
    }

    pub(crate) fn project_workarea_runtime_cef_surface_should_be_visible(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        self.project_workarea_runtime_cef_surface_replacement_permitted(slot_key)
            && self.project_workarea_runtime_cef_surface_may_be_visible(slot_key)
    }

    pub(crate) fn project_workarea_runtime_cef_surface_is_current(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> bool {
        let Some(runtime_url) = self.project_workarea_runtime_url_for_slot(slot_key) else {
            return false;
        };
        self.project_workarea_runtime_cef_surfaces
            .get(&slot_key)
            .is_some_and(|owned_surface| owned_surface.matches_runtime_url(&runtime_url))
    }

    pub(crate) fn project_workarea_runtime_cef_surface_for_render(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
    ) -> Option<Entity<CefSurface>> {
        if !self.project_workarea_runtime_cef_surface_may_be_visible(slot_key) {
            return None;
        }

        let runtime_url = self.project_workarea_runtime_url_for_slot(slot_key)?;
        let owned_surface = self.project_workarea_runtime_cef_surfaces.get(&slot_key)?;
        owned_surface
            .matches_runtime_url(&runtime_url)
            .then(|| owned_surface.surface.clone())
    }

    #[allow(dead_code)]
    pub(crate) fn ensure_project_workarea_runtime_cef_surface(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        runtime_url: ProjectWorkareaRealRuntimeUrl,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        /*
        CDXC:Workarea 2026-06-24-10:12:
        Project-workarea CEF creation happens only at the visible replacement edge after the slot gate already permits placeholder replacement and a real runtime URL value has been supplied. This avoids hidden/offscreen preparatory mounts and keeps URL values out of app shell state while still using the existing CefSurface child-view wrapper for Source, Kanban, Automate, and Manage.
        */
        if self.sidebar.is_none()
            || !self.project_workarea_runtime_cef_surface_should_be_visible(slot_key)
        {
            return None;
        }
        if let Some(owned_surface) = self.project_workarea_runtime_cef_surfaces.get(&slot_key) {
            if owned_surface.matches_runtime_url(&runtime_url) {
                return Some(owned_surface.surface.clone());
            }
        }
        if let Some(stale_surface) = self.project_workarea_runtime_cef_surfaces.remove(&slot_key) {
            stale_surface
                .surface
                .update(cx, |surface, _| surface.set_visible(false));
        }

        let parent_ns_view = self.parent_ns_view;
        let surface_id = slot_key.cef_surface_id();
        let profile = slot_key.cef_profile_id();
        let url = runtime_url.clone().into_cef_url();
        // The Source slot hosts the app-owned code-server runtime; its origin
        // is the one trusted clipboard origin (macOS trustedClipboardOrigin).
        let trusted_clipboard_origin =
            (slot_key == ProjectWorkareaCefSurfaceSlotKey::Source).then(|| url.clone());
        let project_workarea_bridge_event_handler =
            (!matches!(slot_key, ProjectWorkareaCefSurfaceSlotKey::Extension(_)))
                .then(|| self.project_workarea_bridge_event_handler(slot_key, cx));
        let surface_background = if slot_key == ProjectWorkareaCefSurfaceSlotKey::Source {
            source_view_background_color()
        } else {
            workspace_background_color()
        };
        let manage_docs_resource_scope = if slot_key == ProjectWorkareaCefSurfaceSlotKey::Manage {
            let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
            let active_project_id = snapshot.active_project_id.as_ref()?.0.as_str();
            if let Some(reference) =
                gpui_remote_project_reference_from_project_id(active_project_id)
            {
                /*
                CDXC:Docs 2026-08-06:
                A remote project path belongs to its gxserver and must never be
                installed as a local CEF resource scope. Keep the synthetic
                Docs origin, but back it with a fixed project-id resource loader
                through the authenticated tunnel so authored remote HTML can
                load sibling CSS, JavaScript, images, and module imports too.
                */
                let target =
                    self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str());
                let project_id = reference.project_id;
                let docs_folders = gpui_manage_additional_docs_folders_text(
                    &self.sidebar_runtime_settings_snapshot,
                );
                Some(cef::ManageDocsResourceScope::new_remote(Arc::new(
                    move |relative_path| {
                        read_remote_manage_docs_resource(
                            target.as_ref(),
                            project_id.as_str(),
                            relative_path,
                            docs_folders.as_str(),
                        )
                    },
                )))
            } else {
                /*
                CDXC:Docs 2026-08-09:
                Docs resources resolve against the same configurable roots the
                Docs bridge uses, through the one shared resolver. The lookup is
                deferred into the scope's resolver because reading the project's
                Docs directory talks to the daemon, which must never run on the
                main thread while a CEF surface is being created.

                CDXC:Docs 2026-08-09: both mounts come out of that
                same deferred lookup — the project root serving docs/ and the
                configured Docs folders, and the mounted Docs directory serving
                its whole tree behind its reserved path segment.
                */
                let project_root = snapshot.in_memory_project_path.clone()?;
                let project_id = active_project_id.to_string();
                let dynamic_project_id = project_id.clone();
                let chat_file_authorization = self.session_chat_docs_file_authorization.clone();
                let docs_folders = gpui_manage_additional_docs_folders_text(
                    &self.sidebar_runtime_settings_snapshot,
                );
                let global_docs_directory =
                    gpui_global_docs_directory_text(&self.sidebar_runtime_settings_snapshot);
                Some(cef::ManageDocsResourceScope::new(
                    Arc::new(move || {
                        let roots = manage_docs_root(
                            Some(project_id.as_str()),
                            Some(project_root.as_path()),
                            global_docs_directory.as_str(),
                            None,
                            None,
                        )
                        .ok()?;
                        let mut mounts = vec![cef::ManageDocsResourceRoot {
                            allowed_relative_roots: manage_docs_project_scan_root_relative_paths(
                                roots.project.as_path(),
                                docs_folders.as_str(),
                            ),
                            mount_segment: String::new(),
                            path: roots.project,
                        }];
                        if let Some(path) = roots.extra.and_then(|mount| mount.location.ok()) {
                            mounts.push(cef::ManageDocsResourceRoot {
                                // The whole tree, matching what the mount lists.
                                allowed_relative_roots: vec![String::new()],
                                mount_segment: MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT.to_string(),
                                path,
                            });
                        }
                        Some(mounts)
                    }),
                    Arc::new(move || {
                        let authorization = chat_file_authorization.lock().ok()?;
                        let authorization = authorization.as_ref().filter(|authorization| {
                            authorization.project_id == dynamic_project_id
                        })?;
                        Some(cef::ManageDocsResourceRoot {
                            allowed_relative_roots: vec![String::new()],
                            mount_segment: MANAGE_DOCS_CHAT_FILE_MOUNT_SEGMENT.to_string(),
                            path: authorization.root.clone(),
                        })
                    }),
                ))
            }
        } else {
            None
        };
        let creation_result = match slot_key {
            ProjectWorkareaCefSurfaceSlotKey::Extension(id) => {
                if gpui_custom_view(id).is_some() {
                    CefSurface::try_new(
                        surface_id,
                        parent_ns_view,
                        url,
                        profile,
                        CEF_DARK_PREPAINT_BACKGROUND_COLOR,
                        true,
                        surface_background,
                        None,
                        true,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        cx,
                    )
                } else {
                    let extension = self.installed_extension_view(id)?;
                    let bridge_surface = extension
                        .bridge_surface_spec()
                        .filter(|surface| surface.matches_url(&url))
                        .or_else(|| extension.bridge_surface_spec_for_url(&url));
                    let Some(bridge_surface) = bridge_surface else {
                        return None;
                    };
                    CefSurface::try_new_extension(
                        surface_id,
                        parent_ns_view,
                        url,
                        profile,
                        CEF_DARK_PREPAINT_BACKGROUND_COLOR,
                        false,
                        surface_background,
                        true,
                        bridge_surface,
                        self.extension_view_bridge_event_handler(slot_key, cx),
                        cx,
                    )
                }
            }
            _ => CefSurface::try_new(
                surface_id,
                parent_ns_view,
                url,
                profile,
                CEF_DARK_PREPAINT_BACKGROUND_COLOR,
                false,
                surface_background,
                trusted_clipboard_origin,
                true,
                None,
                None,
                None,
                None,
                None,
                None,
                project_workarea_bridge_event_handler,
                manage_docs_resource_scope,
                None,
                None,
                None,
                cx,
            ),
        };
        let surface = match creation_result {
            Ok(surface) => surface,
            Err(error) => {
                // Ensure-style reconcile: skip this pass, retried on the next
                // workarea sync (CDXC:CefRuntime 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({ "surface": "projectWorkarea", "error": error }),
                );
                return None;
            }
        };
        self.project_workarea_runtime_cef_surfaces.insert(
            slot_key,
            ProjectWorkareaRuntimeCefSurface {
                runtime_url,
                surface: surface.clone(),
            },
        );
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        Some(surface)
    }

    pub(crate) fn ensure_project_workarea_runtime_cef_surfaces_for_current_context(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-24-11:03:
        Workarea CEF surface materialization is active-workarea-only. The app creates Kanban/Automate/Manage CefSurface entities only when CEF is initialized, the workarea is selected and awake, and a real bundled runtime URL can be issued from the current explicit sidebar snapshot; it does not prewarm hidden surfaces or synthesize Source/code-server URLs.

        CDXC:CodeEditor 2026-06-24-23:17:
        Source uses the same active-workarea-only materialization edge, with one extra predecessor: ensure the shared code-server runtime is launching or ready before asking the URL gate for a Source CefSurface. Until the app-owned runtime reaches ready, this method leaves Source on its loading/error placeholder instead of creating an about:blank or dead localhost surface.
        */
        let mut changed = false;
        if !self.project_workarea_runtime_cef_surface_is_current(
            ProjectWorkareaCefSurfaceSlotKey::Source,
        ) {
            changed |= self.ensure_source_code_server_runtime_for_current_context(cx);
        }
        let mut slot_keys = ProjectWorkareaCefSurfaceSlotKey::project_placeholder_slots().to_vec();
        if let TitlebarMode::Extension(id) = self.active_mode {
            slot_keys.push(ProjectWorkareaCefSurfaceSlotKey::Extension(id));
        }
        for slot_key in slot_keys {
            if !self.project_workarea_runtime_cef_surface_may_be_visible(slot_key) {
                continue;
            }
            let Some(runtime_url) = self.project_workarea_runtime_url_for_slot(slot_key) else {
                continue;
            };
            if self
                .project_workarea_runtime_cef_surfaces
                .get(&slot_key)
                .is_some_and(|owned_surface| owned_surface.matches_runtime_url(&runtime_url))
            {
                continue;
            }
            if self
                .ensure_project_workarea_runtime_cef_surface(slot_key, runtime_url, cx)
                .is_some()
            {
                changed = true;
            }
        }
        changed |= self.open_pending_source_file_if_ready(cx);
        changed |= self.deliver_pending_docs_file_open(cx);
        changed
    }

    pub(crate) fn open_pending_source_file_if_ready(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(pending) = self.pending_source_file_open.as_ref() else {
            return false;
        };
        let Some(target) = self.source_code_server_runtime.target.as_ref() else {
            return false;
        };
        if self.source_code_server_runtime.state != SourceCodeServerRuntimeLaunchState::Ready
            || target.project_path != pending.project_path
            || !self.project_workarea_runtime_cef_surface_is_current(
                ProjectWorkareaCefSurfaceSlotKey::Source,
            )
        {
            return false;
        }

        let Some(pending) = self.pending_source_file_open.take() else {
            return false;
        };
        let origin = pending.origin;
        let failed_file_path = pending.file_path.to_string_lossy().into_owned();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    source_code_server_open_file_in_existing_instance(
                        &pending.file_path,
                        pending.line,
                        pending.column,
                    )
                })
                .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(failed_file_path));
                    let (id, title) = match origin {
                        PendingSourceFileOpenOrigin::AgentsHub => (
                            "gpui-agents-hub-source-open-failed",
                            "Could not open Agents Hub file",
                        ),
                        PendingSourceFileOpenOrigin::SessionChat => (
                            GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID,
                            "Could not open file in Code view",
                        ),
                    };
                    this.upsert_gpui_app_toast(
                        GpuiAppToast {
                            id: id.to_string(),
                            level: GpuiAppToastLevel::from_raw(Some("warning")),
                            title: title.to_string(),
                            description: Some(format!(
                                "{message} The file path was copied to your clipboard."
                            )),
                            loading: false,
                            persistent: false,
                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                            epoch: 0,
                        },
                        cx,
                    );
                });
            }
        })
        .detach();
        true
    }

    pub(crate) fn prune_project_workarea_runtime_cef_surfaces_for_current_gates(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let surface_slot_keys = self
            .project_workarea_runtime_cef_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut pruned = false;
        for slot_key in surface_slot_keys {
            let current_runtime_url = self.project_workarea_runtime_url_for_slot(slot_key);
            let surface_is_current = current_runtime_url.as_ref().is_some_and(|runtime_url| {
                self.project_workarea_runtime_cef_surfaces
                    .get(&slot_key)
                    .is_some_and(|owned_surface| owned_surface.matches_runtime_url(runtime_url))
            });
            if surface_is_current {
                continue;
            }
            if let Some(owned_surface) =
                self.project_workarea_runtime_cef_surfaces.remove(&slot_key)
            {
                owned_surface
                    .surface
                    .update(cx, |surface, _| surface.set_visible(false));
                pruned = true;
            }
        }
        if pruned {
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
        }
        pruned
    }

    pub(crate) fn update_project_workarea_runtime_cef_surface_visibility(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-06-24-10:12:
        Source, Kanban, Automate, and Manage CEF visibility is derived from the active awake workarea plus the same replacement gate used by rendering. The loop only toggles already-owned CEF child views; it does not create missing surfaces, issue URLs, synthesize fallback pages, overlap project panes, or persist/log private runtime details.
        */
        let surface_slot_keys = self
            .project_workarea_runtime_cef_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for slot_key in surface_slot_keys {
            let current_runtime_url = self.project_workarea_runtime_url_for_slot(slot_key);
            let surface_may_be_visible =
                self.project_workarea_runtime_cef_surface_may_be_visible(slot_key);
            if let Some(owned_surface) = self.project_workarea_runtime_cef_surfaces.get(&slot_key) {
                let surface_visible = surface_may_be_visible
                    && current_runtime_url
                        .as_ref()
                        .is_some_and(|runtime_url| owned_surface.matches_runtime_url(runtime_url));
                owned_surface.surface.update(cx, |surface, _| {
                    surface.set_visible(surface_visible);
                });
            }
        }
    }

    pub(crate) fn titlebar_mode_available(&self, mode: TitlebarMode) -> bool {
        /*
        CDXC:Extensions 2026-08-23:
        A workarea turned off in Settings → Customize is not just missing its
        titlebar tab: it is a place the shell must never route to. Folding the
        Customize gate into the single availability predicate closes that off
        for every caller at once — hotkeys, command palette, chat/terminal link
        and file opens, saved Browser Actions, `ghostex browser open`, OS
        `ghostex://` opens, restored and persisted active modes — instead of
        leaving each entry point to remember its own check. Entry points that
        answer a click still handle the refusal visibly (they copy the target
        and say why); this predicate is the backstop that keeps the rest from
        silently parking the user on a view they turned off.
        */
        let available = match mode {
            TitlebarMode::Extension(id) => {
                if gpui_custom_view(id).is_some() {
                    gpui_enabled_custom_view(id).is_some()
                } else {
                    self.project_scoped_workarea_availability()
                        .titlebar_mode_available(mode)
                        && self.installed_extension_view(id).is_some()
                        && gpui_extension_view_presentation(id).is_some()
                }
            }
            _ => self
                .project_scoped_workarea_availability()
                .titlebar_mode_available(mode),
        };
        available && !gpui_titlebar_mode_hidden_from_settings(mode)
    }

    pub(crate) fn available_titlebar_mode_or_agents(&self, mode: TitlebarMode) -> TitlebarMode {
        if self.titlebar_mode_available(mode) {
            mode
        } else {
            TitlebarMode::Agents
        }
    }

    pub(crate) fn titlebar_mode_switcher_items(&self) -> Vec<TitlebarModeSwitcherItem> {
        let mut items = titlebar_mode_switcher_items(self.project_scoped_workarea_availability())
            .into_iter()
            .filter(|item| !gpui_titlebar_mode_hidden_from_settings(item.mode))
            .collect::<Vec<_>>();
        let installed_extension_available = self
            .project_scoped_workarea_availability()
            .project_context
            .has_project_scoped_workareas();
        let mut extension_modes = self
            .extensions_snapshot
            .installed
            .values()
            .filter(|extension| {
                extension.enabled
                    && extension.placements.contains(&GpuiExtensionPlacement::View)
                    && extension.placement == Some(GpuiExtensionPlacement::View)
            })
            .filter_map(|extension| {
                let id = ExtensionId::new(&extension.id)?;
                if gpui_custom_view(id).is_some() {
                    return None;
                }
                let title = gpui_extension_view_presentation(id)?.title;
                Some((title, id, installed_extension_available))
            })
            .collect::<Vec<_>>();
        extension_modes.sort_by(|left, right| left.0.cmp(&right.0));
        items.extend(extension_modes.into_iter().map(|(_, id, is_available)| {
            TitlebarModeSwitcherItem {
                mode: TitlebarMode::Extension(id),
                is_available,
                disabled_reason: (!is_available)
                    .then_some(TITLEBAR_PROJECT_CONTEXT_DISABLED_REASON),
            }
        }));
        items.extend(
            gpui_custom_views_from_settings()
                .into_iter()
                .filter(|view| view.enabled)
                .map(|view| TitlebarModeSwitcherItem {
                    mode: TitlebarMode::Extension(view.id),
                    is_available: true,
                    disabled_reason: None,
                }),
        );
        if items.len() == 1 && items[0].mode == TitlebarMode::Agents {
            items.clear();
        }
        items
    }

    pub(crate) fn coerce_active_mode_to_available_project_context(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-22-19:44:
        Runtime GPUI workarea availability now prefers the latest valid in-memory sidebar project snapshot and uses the strict env bridge only before the sidebar reports. When a project-context update disables the active project-scoped mode, fall back through the existing Agents route, hide Browser CEF through the normal visibility gate, and persist only shell mode/focus state without writing project names, paths, ids, URLs, raw JSON, tokens, cookies, or user content.
        */
        let next_active_mode = self.available_titlebar_mode_or_agents(self.active_mode);
        if next_active_mode == self.active_mode {
            return false;
        }

        self.active_mode = next_active_mode;
        self.focus_default_surface_for_active_mode();
        self.update_active_mode_cef_child_visibility(cx);
        self.scroll_all_active_tab_strips();
        self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        self.persist_shell_layout_state();
        true
    }

    pub(crate) fn ensure_tab_scroll_handles_for_current_layout(&mut self) {
        /*
        CDXC:CommandPane 2026-06-22-12:30:
        Native tab overflow parity needs runtime-only ScrollHandles for Agents pane tab strips, Browser pane tab strips, expanded command group tab strips, and the collapsed command strip. Selection, keyboard cycling, close-neighbor selection, new-tab creation, and drag/drop moves should reveal the active item with scroll_to_item(active index) without persisting scroll offsets or adding overlays, hidden hit regions, hit-test routing, synthetic coordinate routing, or broad layout overlap.
        */
        let workspace_pane_ids = self.agents_workspace.leaf_order();
        self.workspace_tab_scroll_handles
            .retain(|pane_id, _| workspace_pane_ids.contains(pane_id));
        for pane_id in workspace_pane_ids {
            self.workspace_tab_scroll_handles
                .entry(pane_id)
                .or_insert_with(ScrollHandle::new);
        }

        let browser_pane_ids = self.browser_tabs.rendered_leaf_order();
        self.browser_tab_scroll_handles
            .retain(|pane_id, _| browser_pane_ids.contains(pane_id));
        for pane_id in browser_pane_ids {
            self.browser_tab_scroll_handles
                .entry(pane_id)
                .or_insert_with(ScrollHandle::new);
        }

        let command_group_ids = self.command_pane.group_order();
        self.command_tab_scroll_handles
            .retain(|group_id, _| command_group_ids.contains(group_id));
        for group_id in command_group_ids {
            self.command_tab_scroll_handles
                .entry(group_id)
                .or_insert_with(ScrollHandle::new);
        }
    }

    pub(crate) fn workspace_tab_scroll_handle(&self, pane_id: WorkspacePaneId) -> ScrollHandle {
        self.workspace_tab_scroll_handles
            .get(&pane_id)
            .cloned()
            .unwrap_or_else(ScrollHandle::new)
    }

    pub(crate) fn browser_tab_scroll_handle(&self, pane_id: BrowserPaneId) -> ScrollHandle {
        self.browser_tab_scroll_handles
            .get(&pane_id)
            .cloned()
            .unwrap_or_else(ScrollHandle::new)
    }

    pub(crate) fn command_tab_scroll_handle(&self, group_id: CommandPaneGroupId) -> ScrollHandle {
        self.command_tab_scroll_handles
            .get(&group_id)
            .cloned()
            .unwrap_or_else(ScrollHandle::new)
    }

    pub(crate) fn scroll_all_active_tab_strips(&mut self) {
        self.ensure_tab_scroll_handles_for_current_layout();
        for pane_id in self.agents_workspace.leaf_order() {
            self.scroll_workspace_pane_active_tab_without_ensure(pane_id);
        }
        for pane_id in self.browser_tabs.rendered_leaf_order() {
            self.scroll_browser_pane_active_tab_without_ensure(pane_id);
        }
        for group_id in self.command_pane.group_order() {
            self.scroll_command_group_active_tab_without_ensure(group_id);
        }
        self.scroll_command_collapsed_active_tab_without_ensure();
    }

    pub(crate) fn scroll_workspace_pane_active_tab(&mut self, pane_id: WorkspacePaneId) {
        self.ensure_tab_scroll_handles_for_current_layout();
        self.scroll_workspace_pane_active_tab_without_ensure(pane_id);
    }

    pub(crate) fn scroll_browser_pane_active_tab(&mut self, pane_id: BrowserPaneId) {
        self.ensure_tab_scroll_handles_for_current_layout();
        self.scroll_browser_pane_active_tab_without_ensure(pane_id);
    }

    pub(crate) fn scroll_focused_browser_pane_active_tab(&mut self) {
        self.scroll_browser_pane_active_tab(self.browser_tabs.focused_pane);
    }

    pub(crate) fn scroll_command_group_active_tab(&mut self, group_id: CommandPaneGroupId) {
        self.ensure_tab_scroll_handles_for_current_layout();
        self.scroll_command_group_active_tab_without_ensure(group_id);
        self.scroll_command_collapsed_active_tab_without_ensure();
    }

    pub(crate) fn focused_command_active_tab_reveal_target(
        command_pane: &CommandPaneModel,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:CommandPane 2026-06-26-00:39:
        Focused command active-tab reveal is responder-like: resolve only the live `focused_group` active session and no-op when `focused_group` is stale, so expanded and collapsed reveal never fall back to the first command group.
        */
        command_pane.focused_group_active_session_id()
    }

    pub(crate) fn scroll_focused_command_active_tab(&mut self) {
        self.ensure_tab_scroll_handles_for_current_layout();
        let Some((group_id, _session_id)) =
            Self::focused_command_active_tab_reveal_target(&self.command_pane)
        else {
            return;
        };
        self.scroll_command_group_active_tab_without_ensure(group_id);
        self.scroll_command_collapsed_active_tab_without_ensure();
    }

    pub(crate) fn scroll_workspace_pane_active_tab_without_ensure(&self, pane_id: WorkspacePaneId) {
        let Some(active_index) = self
            .agents_workspace
            .find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_session_index())
        else {
            return;
        };
        if let Some(handle) = self.workspace_tab_scroll_handles.get(&pane_id) {
            handle.scroll_to_item(active_index);
        }
    }

    pub(crate) fn scroll_browser_pane_active_tab_without_ensure(&self, pane_id: BrowserPaneId) {
        let Some(active_index) = self
            .browser_tabs
            .find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_tab_index())
        else {
            return;
        };
        if let Some(handle) = self.browser_tab_scroll_handles.get(&pane_id) {
            handle.scroll_to_item(active_index);
        }
    }

    pub(crate) fn scroll_command_group_active_tab_without_ensure(
        &self,
        group_id: CommandPaneGroupId,
    ) {
        let Some(active_index) = self
            .command_pane
            .find_leaf(group_id)
            .and_then(|leaf| leaf.tab_group.active_session_index())
        else {
            return;
        };
        if let Some(handle) = self.command_tab_scroll_handles.get(&group_id) {
            command_pane_reveal_active_tab_with_native_margin(handle, active_index);
        }
    }

    pub(crate) fn scroll_command_collapsed_active_tab_without_ensure(&self) {
        let Some((active_group_id, active_session_id)) =
            self.command_pane.active_group_and_session_id()
        else {
            return;
        };
        let Some(active_index) =
            self.command_pane
                .flat_tab_ids()
                .into_iter()
                .position(|(group_id, session_id)| {
                    group_id == active_group_id && session_id == active_session_id
                })
        else {
            return;
        };
        command_pane_reveal_active_tab_with_native_margin(
            &self.command_collapsed_tab_scroll_handle,
            active_index,
        );
    }

    pub(crate) fn show_command_pane_active_tab_from_sticky_proxy(
        &mut self,
        group_id: CommandPaneGroupId,
        scroll_handle: ScrollHandle,
        active_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-06-25-13:34:
        Clicking Show Active Tab should reveal the already-selected command tab in the current scroll strip. This is navigation state and must not mutate tab order, session identity, action metadata, command text, or logs.

        CDXC:CommandPane 2026-06-25-18:56:
        Native `performStickyActiveTabButton` scrolls existing tab geometry directly; it does not route through tab action dispatch or select a different session.

        CDXC:CommandPane 2026-06-25-21:50:
        The GPUI command-pane overflow proxy must focus the command pane/group that owns the clipped active tab before revealing it. This remains a real button click path, not a hidden overlay or hit-test route, and it must not select another tab, change drag/drop state, or mutate command session identity.

        CDXC:CommandPane 2026-06-25-21:56:
        Native `performStickyActiveTabButton` calls `centerActiveTabInTabStrip`, so Show Active Tab should center the clipped active tab when scroll bounds allow. Keep the softer native-margin reveal helper for ordinary focus/selection scrolling, not this explicit proxy action.
        */
        if self.command_pane.focus_group(group_id) {
            self.focus_command_pane();
            self.request_command_group_terminal_text_focus_handoff(group_id);
        }
        command_pane_center_active_tab_in_scroll_handle(&scroll_handle, active_index);
        cx.notify();
    }
}
