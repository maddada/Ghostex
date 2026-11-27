//! Extension-provided views: the process-wide runtime state of each extension's server, the
//! placeholder it shows while that server starts, and the bridge events its page sends.
//! Moved verbatim out of `app/workarea.rs` on 2026-09-20.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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
    pub(crate) fn installed_extension_view(
        &self,
        id: ExtensionId,
    ) -> Option<&GpuiInstalledExtension> {
        self.extensions_snapshot
            .installed
            .get(id.as_str())
            .filter(|extension| {
                extension.enabled
                    && extension.placements.contains(&GpuiExtensionPlacement::View)
                    && extension.placement == Some(GpuiExtensionPlacement::View)
            })
    }

    pub(crate) fn extension_view_runtime_url(
        &self,
        id: ExtensionId,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        // The built-in Canvas shows the Files embed page on the open drawing (app/canvas/).
        if TitlebarMode::Extension(id).is_canvas() {
            return self.canvas_page_url();
        }
        if let Some(custom_view) = gpui_custom_view(id) {
            if !custom_view.enabled {
                return None;
            }
            if custom_view.definition.get("source").is_some() {
                return self.custom_project_view_url(id);
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
        if gpui_custom_view(id).is_some_and(|v| v.definition.get("source").is_some()) {
            return self.custom_project_view_placeholder(id);
        }
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
        if gpui_custom_view(id).is_some() {
            self.ensure_custom_project_views(cx);
            return false;
        }
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

    pub(crate) fn extension_view_bridge_event_handler(
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
}
