use crate::app::helpers::*;
use crate::*;
use gpui::{Action, Window};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

#[derive(Default)]
pub(crate) struct ProjectViews {
    entries: HashMap<String, Entry>,
    pub(crate) website_editor: Option<super::project_websites::WebsiteHomeEditor>,
}
struct Entry {
    fingerprint: String,
    params: Value,
    status: Value,
    operation: Option<String>,
    project_id: String,
    show_output: bool,
    was_active: bool,
    started: bool,
    parked: Option<ProjectWorkareaRuntimeCefSurface>,
}
#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace=ghostex_gpui,no_json)]
pub(crate) struct ProjectViewCommand {
    pub id: String,
    pub operation: String,
}
/// The HUD fields the app-modal Settings window needs to render the view scope editors.
pub(crate) const PROJECT_VIEW_SCOPE_OPTION_HUD_KEYS: [&str; 2] =
    ["projectViewSpaces", "projectViewProjects"];

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

impl GhostexGpuiApp {
    /// CDXC:Spaces 2026-09-18 WHY:
    /// Settings builds its own hydrate, which omitted the sidebar's spaces and showed an empty picker despite existing spaces.
    /// Reuse the store HUD's complete local/remote options, including their computer-scoped identities.
    /// The project list rides along for the same reason: the scope editor lists the sidebar's own
    /// rows, which only the native sidebar's snapshot enumerates.
    pub(crate) fn with_project_view_scope_options(&self, mut message: Value) -> Value {
        let Some(hud) = self
            .native_sidebar
            .snapshot
            .as_ref()
            .map(|snapshot| &snapshot.hud)
        else {
            return message;
        };
        for key in PROJECT_VIEW_SCOPE_OPTION_HUD_KEYS {
            if let Some(value) = hud.get(key) {
                message["hud"][key] = value.clone();
            }
        }
        message
    }

    /// CDXC:Extensions 2026-09-09 WHY:
    /// Two projects can resolve to the same website. Keep their page navigation separate and park the actual CEF child when switching projects or opening command output.
    pub(crate) fn park_custom_project_view(&mut self, owned: ProjectWorkareaRuntimeCefSurface) {
        let Some(key) = owned.runtime_url.project_view_key.as_ref() else {
            return;
        };
        let Some(entry) = self.project_views.entries.get_mut(key) else {
            return;
        };
        if text(&entry.status, "state") == "ready"
            && text(&entry.status, "url") == owned.runtime_url.value
        {
            entry.parked = Some(owned);
        }
    }
    pub(crate) fn take_custom_project_view(
        &mut self,
        url: &ProjectWorkareaRealRuntimeUrl,
    ) -> Option<ProjectWorkareaRuntimeCefSurface> {
        let owned = self
            .project_views
            .entries
            .get_mut(url.project_view_key.as_ref()?)?
            .parked
            .take()?;
        owned.matches_runtime_url(url).then_some(owned)
    }
    fn project_view_key(&self, id: ExtensionId) -> Option<String> {
        let project = self
            .latest_sidebar_project_snapshot
            .as_ref()?
            .active_project_id
            .as_ref()?;
        Some(format!("{}\n{}", project.0, id.as_str()))
    }
    pub(crate) fn custom_project_view_status(&self, id: ExtensionId) -> Option<&Value> {
        self.project_views
            .entries
            .get(&self.project_view_key(id)?)
            .map(|e| &e.status)
    }
    pub(crate) fn custom_project_view_visible(&self, view: &GpuiCustomView) -> bool {
        if !view.enabled {
            return false;
        }
        if view.definition.get("source").is_none() {
            return true;
        }
        let Some(snapshot) = self.latest_sidebar_project_snapshot.as_ref() else {
            return false;
        };
        let Some(project) = snapshot.active_project_id.as_ref() else {
            return false;
        };
        if text(&view.definition, "availability") == "selected"
            && !view.definition["projectIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(&project.0)))
        {
            return false;
        }
        if text(&view.definition, "availability") == "spaces" {
            return self
                .custom_project_view_status(view.id)
                .is_some_and(|s| s["scopeMatches"].as_bool() == Some(true));
        }
        if text(&view.definition, "availability") == "matching" {
            return self
                .custom_project_view_status(view.id)
                .is_some_and(|s| s["available"].as_bool() == Some(true));
        }
        true
    }
    pub(crate) fn custom_project_view_url(
        &self,
        id: ExtensionId,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        if self.website_home_editor_is_open(id) {
            return None;
        }
        let entry = self
            .project_views
            .entries
            .get(&self.project_view_key(id)?)?;
        if entry.show_output || text(&entry.status, "state") != "ready" {
            return None;
        }
        let mut url = ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(
            text(&entry.status, "url").to_string(),
        )?;
        url.project_view_key = self.project_view_key(id);
        Some(url)
    }
    pub(crate) fn custom_project_view_placeholder(
        &self,
        id: ExtensionId,
    ) -> ProjectEditorPlaceholderSignature {
        use ProjectEditorPlaceholderAction::*;
        let view = gpui_custom_view(id);
        let name = view.as_ref().map(|v| v.title.as_str()).unwrap_or("View");
        let entry = self
            .project_view_key(id)
            .and_then(|key| self.project_views.entries.get(&key));
        let status = entry.map(|e| e.status.clone()).unwrap_or(Value::Null);
        let show_output = entry.is_some_and(|e| e.show_output);
        let state = text(&status, "state");
        let message = if show_output {
            format!(
                "{}\n\n{}",
                text(&status["plan"], "command"),
                text(&status, "output")
            )
        } else {
            text(&status, "error").to_string()
        };
        let title = if show_output {
            format!("{name} command output")
        } else {
            match state {
                "failed" => format!("{name} could not start"),
                "unconfigured" => format!("Configure {name}"),
                "stopped" => format!("{name} is stopped"),
                "ready" => format!("Opening {name}…"),
                _ => format!("Preparing {name}…"),
            }
        };
        ProjectEditorPlaceholderSignature {
            mode: TitlebarMode::Extension(id),
            title: Some(title),
            message,
            actions: if show_output {
                vec![ProjectViewOpen, ProjectViewRetry, ProjectViewStop]
            } else if ["failed", "stopped", "unconfigured"].contains(&state) {
                if id.as_str() == "storybook" {
                    vec![ProjectViewRetry, ProjectViewOutput]
                } else {
                    vec![ProjectViewRetry, ProjectViewConfigure, ProjectViewOutput]
                }
            } else {
                vec![ProjectViewStop, ProjectViewOutput]
            },
        }
    }
    pub(crate) fn project_view_command(
        &mut self,
        action: &ProjectViewCommand,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(id) = ExtensionId::new(&action.id) else {
            return;
        };
        if action.operation == "home" {
            self.open_website_home_editor(id, window, cx);
            return;
        }
        if action.operation == "configure" {
            if id.as_str() == "storybook" {
                self.open_view_scope_settings(
                    TitlebarMode::Extension(id).switcher_index(),
                    window,
                    cx,
                );
                return;
            }
            let modal = GpuiAppModalKind::Settings;
            let sidebar_state_message =
                self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
            self.open_gpui_app_modal_window(
                modal,
                json!({
                    "initialTab": "extensions",
                    "initialCustomViewId": id.as_str(),
                    "modal": modal.modal_id(),
                    "type": "open",
                    "latestSidebarStateMessage": sidebar_state_message.clone(),
                }),
                sidebar_state_message,
                Some(window),
                cx,
            );
            return;
        }
        if action.operation == "output" {
            self.set_active_mode(TitlebarMode::Extension(id), window, cx);
        }
        if let Some(entry) = self
            .project_view_key(id)
            .and_then(|key| self.project_views.entries.get_mut(&key))
        {
            match action.operation.as_str() {
                "output" => entry.show_output = true,
                "open" => entry.show_output = false,
                "stop" | "restart" => {
                    entry.operation = Some(action.operation.clone());
                    entry.show_output = false;
                    entry.started = true;
                }
                _ => return,
            }
        }
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        cx.notify();
    }
    pub(crate) fn ensure_custom_project_views(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self.latest_sidebar_project_snapshot.as_ref() else {
            return;
        };
        let Some(project_id) = snapshot.active_project_id.as_ref().map(|id| id.0.clone()) else {
            return;
        };
        for view in super::project_websites::website_views()
            .into_iter()
            .filter(|view| view.enabled)
        {
            let provider =
                super::project_websites::website_provider(view.id).expect("website provider");
            let home = self.website_home(provider, &project_id);
            let key = format!("{project_id}\n{}", view.id.as_str());
            let fingerprint = home.clone().unwrap_or_default();
            if self
                .project_views
                .entries
                .get(&key)
                .is_some_and(|entry| entry.fingerprint == fingerprint)
            {
                continue;
            }
            self.project_views.entries.insert(key, Entry {
                fingerprint, params: Value::Null,
                status: json!({"state":if home.is_some() {"ready"} else {"unconfigured"}, "url":home, "available":!provider.automatic() || home.is_some()}),
                operation:None, project_id:project_id.clone(), show_output:false, was_active:false, started:false, parked:None,
            });
        }
        let project = self.extension_projects.get(&project_id);
        let repository_origin_url = project.and_then(|p| p.git_remote_origin_url.clone());
        let path = project
            .and_then(|p| p.path.clone())
            .or_else(|| {
                snapshot
                    .in_memory_project_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_default();
        if path.is_empty() {
            return;
        }
        let remote = gpui_remote_project_reference_from_project_id(&project_id);
        let target = remote
            .as_ref()
            .and_then(|r| self.gpui_remote_gxserver_request_target(&r.remote_machine_id));
        if remote.is_some() && target.is_none() {
            return;
        }
        let parent_id = project.and_then(|p| p.parent_project_id.clone()).map(|id| {
            remote
                .as_ref()
                .map(|r| gpui_remote_scoped_project_id(&r.remote_machine_id, &id))
                .unwrap_or(id)
        });
        for view in gpui_custom_views_from_settings().into_iter().filter(|v| {
            v.enabled
                && v.definition.get("source").is_some()
                && super::project_websites::website_provider(v.id).is_none()
        }) {
            let key = format!("{project_id}\n{}", view.id.as_str());
            if text(&view.definition, "availability") == "selected"
                && !view.definition["projectIds"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(&project_id)))
            {
                continue;
            }
            let bindings = &view.definition["projectBindings"];
            let mut binding = parent_id
                .as_ref()
                .and_then(|id| bindings.get(id))
                .filter(|v| v["inherit"].as_bool() != Some(false))
                .cloned()
                .unwrap_or(json!({}));
            if let Some(own) = bindings.get(&project_id).and_then(Value::as_object) {
                for (k, v) in own {
                    if !v.is_string() || !v.as_str().unwrap_or_default().is_empty() {
                        binding[k] = v.clone();
                    }
                }
            }
            let mut definition = view.definition.clone();
            if let Some(object) = definition.as_object_mut() {
                object.remove("projectBindings");
                object.remove("projectIds");
            }
            let fingerprint = format!("{definition}{binding}{path}{repository_origin_url:?}");
            if self
                .project_views
                .entries
                .get(&key)
                .is_some_and(|e| e.fingerprint == fingerprint)
            {
                continue;
            }
            let owner = format!(
                "{}-{:?}-{}",
                std::process::id(),
                cx.entity_id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let params = json!({"view":definition,"binding":binding,"projectPath":path,"owner":owner,"repositoryOriginUrl":repository_origin_url, "projectId":remote.as_ref().map(|r| r.project_id.as_str()).unwrap_or(&project_id), "spaceSectionKey":remote.as_ref().map(|r| format!("remote:{}", r.remote_machine_id)).unwrap_or_else(|| "local".into())});
            self.project_views.entries.insert(
                key.clone(),
                Entry {
                    fingerprint: fingerprint.clone(),
                    params: params.clone(),
                    status: json!({"state":"starting","available":false}),
                    operation: Some("describe".into()),
                    project_id: project_id.clone(),
                    show_output: false,
                    was_active: false,
                    started: false,
                    parked: None,
                },
            );
            let target = target.clone();
            let project_id = project_id.clone();
            let id = view.id;
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                loop {
                    let request = this
                        .update(cx, |app, _cx| {
                            let active = app.active_mode == TitlebarMode::Extension(id)
                                && app.project_view_key(id).as_deref() == Some(key.as_str());
                            let enabled = gpui_enabled_custom_view(id).is_some();
                            let request_target =
                                match gpui_remote_project_reference_from_project_id(&project_id) {
                                    Some(remote) => Some(app.gpui_remote_gxserver_request_target(
                                        &remote.remote_machine_id,
                                    )?),
                                    None => None,
                                };
                            let entry = app.project_views.entries.get_mut(&key)?;
                            if entry.fingerprint != fingerprint || !enabled {
                                return None;
                            }
                            if !app.extension_projects.contains_key(&entry.project_id)
                                && app
                                    .latest_sidebar_project_snapshot
                                    .as_ref()
                                    .and_then(|s| s.active_project_id.as_ref())
                                    .map(|p| p.0.as_str())
                                    != Some(entry.project_id.as_str())
                            {
                                return None;
                            }
                            let mut operation =
                                entry.operation.take().unwrap_or_else(|| "status".into());
                            let auto = entry.params["binding"]["startOnProjectOpen"].as_bool()
                                == Some(true);
                            if text(&entry.status, "state") == "stopped"
                                && ((!entry.was_active && active) || (auto && !entry.started))
                            {
                                operation = "start".into();
                                entry.started = true;
                            }
                            if text(&entry.status, "state") != "starting" {
                                entry.was_active = active;
                            }
                            let mut request = entry.params.clone();
                            request["operation"] = json!(operation);
                            Some((request, request_target))
                        })
                        .ok()
                        .flatten();
                    let Some((request, request_target)) = request else {
                        break;
                    };
                    let result = background
                        .spawn(async move { project_view_rpc(request_target.as_ref(), &request) })
                        .await;
                    let keep = this
                        .update(cx, |app, cx| {
                            let Some(entry) = app.project_views.entries.get_mut(&key) else {
                                return false;
                            };
                            if entry.fingerprint != fingerprint {
                                return false;
                            }
                            entry.status = match result {
                                Ok(value) => value["status"].clone(),
                                Err(error) => {
                                    json!({"state":"failed","available":id.as_str() != "storybook" || entry.status["available"].as_bool().unwrap_or(false),"error":error})
                                }
                            };
                            app.ensure_project_workarea_runtime_cef_surfaces_for_current_context(
                                cx,
                            );
                            app.update_project_workarea_runtime_cef_surface_visibility(cx);
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    if !keep {
                        break;
                    }
                    let delay = this
                        .update(cx, |app, _| {
                            let undetected = app.project_views.entries.get(&key).is_some_and(|entry| {
                                entry.status["available"].as_bool() != Some(true)
                            });
                            if id.as_str() == "storybook"
                                && app.active_mode != TitlebarMode::Extension(id)
                                && undetected
                            {
                                15
                            } else {
                                2
                            }
                        })
                        .unwrap_or(2);
                    background.timer(Duration::from_secs(delay)).await;
                }
                let _ = this.update(cx, |app, _| {
                    if app
                        .project_views
                        .entries
                        .get(&key)
                        .is_some_and(|e| e.fingerprint == fingerprint)
                    {
                        app.project_views.entries.remove(&key);
                    }
                });
                let mut release = params;
                release["operation"] = json!("release");
                let _ = background
                    .spawn(async move { project_view_rpc(target.as_ref(), &release) })
                    .await;
            })
            .detach();
        }
    }
}
fn project_view_rpc(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    params: &Value,
) -> Result<Value, String> {
    match target {
        Some(target) => gpui_remote_gxserver_rpc_result(
            target,
            "/api/projectView",
            params,
            Duration::from_secs(20),
        ),
        None => gpui_gxserver_rpc_result("/api/projectView", params, Duration::from_secs(20)),
    }
}
