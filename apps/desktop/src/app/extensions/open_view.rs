//! Opening an extension's view in a project's side panel, the one path for `ghostex extensions
//! open`, an extension's own `openView`, the switch toast's Open and a rail pin's click.
//!
//! CDXC:Extensions 2026-11-26 WHY:
//! gxserver decides who may open what and whether the user is asked first (server/src/extensions/open_view.rs); only the app knows which view is in front of which project, so it answers `alreadyShowing` for a view the user is already looking at (no toast for it) and does the open. A background open adds the tab to the project, on screen or parked, without showing it, as an agent's background Browser page does.
//! SEE-ALSO: packages/gx-core/src/renderer_commands/verbs.rs (`OpenExtensionView`), app/sidebar_rail/ (`ext:<id>` pins).

use ghostex_gx_core::ExtensionViewOpenMode;
use serde_json::{Value, json};

use crate::app::model::*;
use crate::app::window::{GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastLevel};
use crate::*;

impl GhostexGpuiApp {
    /// Whether this app can show `id` as a side panel view at all: an enabled `view` extension,
    /// or a sidebar extension's side panel view.
    pub(crate) fn extension_view_installed(&self, id: ExtensionId) -> bool {
        (self.installed_extension_view(id).is_some()
            && gpui_extension_view_presentation(id).is_some())
            || self.extension_side_panel_offered(id)
    }

    /// The title an extension's view goes by (its tab's), or its id.
    pub(crate) fn extension_view_title(&self, id: ExtensionId) -> String {
        gpui_extension_view_presentation(id)
            .map(|presentation| presentation.title.to_string())
            .or_else(|| {
                self.extensions_snapshot
                    .installed
                    .get(id.as_str())
                    .map(|extension| extension.title.clone())
            })
            .unwrap_or_else(|| id.as_str().to_string())
    }

    /// Opens `extension_id`'s view in `project_id`'s side panel (the project on screen when
    /// `None`) as `mode` says. `switch_project` brings another project on screen first.
    pub(crate) fn open_extension_view_in_side_panel(
        &mut self,
        extension_id: &str,
        project_id: Option<&str>,
        mode: ExtensionViewOpenMode,
        switch_project: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Result<Value, String> {
        let Some(id) = ExtensionId::new(extension_id) else {
            return Err(format!("{extension_id} is not an extension id."));
        };
        let view = TitlebarMode::Extension(id);
        let title = self.extension_view_title(id);
        if !self.extension_view_installed(id) {
            return Err(format!(
                "{title} is off, or has no side panel view, in the Ghostex app."
            ));
        }
        let active_project = self.agents_workspace_project_id.clone();
        let project_id = project_id
            .map(str::to_string)
            .or_else(|| active_project.clone())
            .ok_or_else(|| "No project is open in Ghostex.".to_string())?;
        let on_screen = active_project.as_deref() == Some(project_id.as_str());
        let scope_key = extension_view_scope_key(id.as_str());
        let allowed_here = if on_screen {
            self.titlebar_mode_available(view)
        } else {
            self.view_scope_allows_in_project(&scope_key, &project_id)
        };
        if !allowed_here {
            return Err(format!(
                "{title} is hidden in this project. Show it here from its tab's menu in the side panel, or on its card in Settings > Extensions."
            ));
        }
        let answer = |shown: bool, extra: Value| {
            let mut value = json!({
                "accepted": true,
                "action": "openExtensionView",
                "extensionId": id.as_str(),
                "ok": true,
                "projectId": project_id,
                "shown": shown,
            });
            if let (Some(object), Value::Object(extra)) = (value.as_object_mut(), extra) {
                object.extend(extra);
            }
            value
        };
        if on_screen && self.active_mode == view {
            return Ok(answer(true, json!({ "alreadyShowing": true })));
        }
        match mode {
            ExtensionViewOpenMode::Ask => Ok(answer(false, json!({ "needsAsk": true }))),
            ExtensionViewOpenMode::Background => {
                self.add_extension_view_tab_unshown(view, &project_id, false);
                self.persist_shell_layout_state();
                cx.notify();
                Ok(answer(false, json!({ "background": true })))
            }
            ExtensionViewOpenMode::Show if on_screen => {
                if !self.activate_mode(view, cx) {
                    return Err(format!("{title} could not be opened."));
                }
                cx.notify();
                Ok(answer(true, json!({})))
            }
            ExtensionViewOpenMode::Show => {
                // The project's saved view becomes this one, so it is in front when the project
                // comes on screen (now, when asked to switch; else whenever the user goes there).
                self.add_extension_view_tab_unshown(view, &project_id, true);
                self.persist_shell_layout_state();
                if switch_project {
                    if !self.dispatch_gpui_menu_bar_project_activation(&project_id, cx) {
                        return Err("The project could not be opened.".to_string());
                    }
                    cx.notify();
                    return Ok(answer(true, json!({ "switchedProject": true })));
                }
                let project_title = self
                    .parked_project_title(&project_id)
                    .unwrap_or_else(|| "another project".to_string());
                self.upsert_gpui_app_toast(
                    GpuiAppToast {
                        copy_text: None,
                        id: format!("gpui-extension-view-opened-in-project-{project_id}"),
                        level: GpuiAppToastLevel::from_raw(Some("info")),
                        title: format!("{title} opened in {project_title}"),
                        description: None,
                        loading: false,
                        persistent: false,
                        duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                        action: None,
                        epoch: 0,
                    },
                    cx,
                );
                cx.notify();
                Ok(answer(false, json!({ "openedInProject": true })))
            }
        }
    }

    /// Adds `view`'s tab to `project_id`'s side panel without showing it: the project on screen
    /// gets the tab in its strip, a parked one in its saved view. `front` makes it the parked
    /// project's view to show when it comes back on screen.
    fn add_extension_view_tab_unshown(
        &mut self,
        view: TitlebarMode,
        project_id: &str,
        front: bool,
    ) {
        if self.agents_workspace_project_id.as_deref() == Some(project_id) {
            if !self.open_views.contains(&view) {
                // Shown later by a click on its tab, so it starts asleep like a restored tab.
                self.project_editor_shell.mark_mode_sleeping(view);
                self.record_open_view_tab(view);
            }
            return;
        }
        let ratio = self.project_editor_shell.workarea_split_ratio;
        let state = self
            .project_view_states_by_project
            .entry(project_id.to_string())
            .or_insert_with(|| GpuiProjectViewState::closed(ratio));
        if !state.open_views.contains(&view) {
            state.open_views.push(view);
        }
        if front {
            state.active_mode = view;
            state.last_view_mode = Some(view);
            state.active_view_awake = false;
        }
    }
}
