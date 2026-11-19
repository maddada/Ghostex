use crate::GhostexGpuiApp;
use gpui::{AppContext, Entity, Focusable, Subscription, Window};
use gpui_component::input::{InputEvent, InputState};
use serde_json::json;

pub(crate) struct SidebarNameEditor {
    pub(crate) kind: &'static str,
    pub(crate) id: String,
    pub(crate) input: Entity<InputState>,
    _subscription: Subscription,
}

impl SidebarNameEditor {
    /// An editor opened outside this file (a sidebar extension's checklist item,
    /// extension_panel/actions.rs).
    pub(crate) fn new(
        kind: &'static str,
        id: String,
        input: Entity<InputState>,
        subscription: Subscription,
    ) -> Self {
        Self {
            kind,
            id,
            input,
            _subscription: subscription,
        }
    }
}

impl GhostexGpuiApp {
    pub(crate) fn begin_native_collection_rename(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_native_sidebar_rename("collection", id, window, cx);
    }

    pub(crate) fn begin_native_sidebar_rename(
        &mut self,
        kind: &'static str,
        id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let title = self.native_sidebar.snapshot.as_ref().and_then(|snapshot| {
            if kind == "collection" {
                snapshot
                    .collections
                    .iter()
                    .find(|item| item.collection_id == id)
                    .map(|item| item.title.clone())
            } else {
                snapshot
                    .groups
                    .iter()
                    .find(|item| item.group_id == id)
                    .map(|item| item.title.clone())
            }
        });
        let Some(title) = title else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title.clone()));
        let subscription = cx.subscribe_in(&input, window, |app, _, event, _, cx| {
            match event {
                InputEvent::Focus => {
                    // CDXC:FocusRouting 2026-09-19 WHY:
                    // Selecting the group name only moves GPUI focus; Linux can still send keys to the previously focused Chromium input when the pointer is over it.
                    app.drop_pending_browser_keyboard_handoff();
                    app.reclaim_gpui_root_for_chrome_input_focus();
                }
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    app.finish_native_sidebar_rename(true, cx);
                }
                _ => {}
            }
        });
        self.native_sidebar.name_editor = Some(SidebarNameEditor {
            kind,
            id: id.to_owned(),
            input: input.clone(),
            _subscription: subscription,
        });
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(0..title.len(), cx);
        });
        cx.notify();
    }

    /// Opens the inline rename on the sessions group a session's Move to New Group just made, once
    /// the snapshot holds it, then scrolls its header into view once it has been drawn. A group
    /// that never shows (a hidden project, a refused write) stops being waited for after a few
    /// seconds, so a later snapshot cannot open a rename the user no longer expects.
    pub(crate) fn begin_pending_group_rename(
        &mut self,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some((id, asked)) = self.native_sidebar.pending_group_rename.clone() {
            let drawn = self
                .native_sidebar
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.groups.iter().any(|group| group.group_id == id));
            if drawn {
                self.native_sidebar.pending_group_rename = None;
                self.native_sidebar.pending_group_reveal = Some(id.clone());
                cx.defer_in(window, move |app, window, cx| {
                    app.begin_native_sidebar_rename("group", &id, window, cx);
                });
            } else if asked.elapsed() > std::time::Duration::from_secs(5) {
                self.native_sidebar.pending_group_rename = None;
            }
        }
        let Some(id) = self.native_sidebar.pending_group_reveal.clone() else {
            return;
        };
        let Some(block) = self.native_sidebar.group_bounds.get(&id).copied() else {
            return;
        };
        self.native_sidebar.pending_group_reveal = None;
        // The header, not the whole block: a group of many sessions still shows its name field.
        let header = gpui::Bounds {
            origin: block.origin,
            size: gpui::size(
                block.size.width,
                block.size.height.min(gpui::px(30.0 * scale)),
            ),
        };
        let viewport = self.native_sidebar.scroll.bounds();
        let delta = super::scroll::reveal_delta(viewport, header, scale);
        if delta.abs() >= gpui::px(1.0) {
            let from = self.native_sidebar.scroll.offset();
            self.native_sidebar.scroll.set_offset(gpui::Point::new(
                from.x,
                (from.y - delta).min(gpui::px(0.0)),
            ));
            cx.notify();
        }
    }

    pub(crate) fn finish_native_sidebar_rename(
        &mut self,
        save: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(editor) = self.native_sidebar.name_editor.take() else {
            return;
        };
        let title = editor.input.read(cx).value().trim().to_owned();
        if save && !title.is_empty() {
            if editor.kind.starts_with("extension") {
                self.finish_native_panel_item_editor(editor.kind, &editor.id, &title, cx);
            } else if editor.kind == "collection" {
                self.dispatch_native_sidebar_ui(json!({"type": "collectionAction", "action": "rename", "collectionId": editor.id, "value": title}), cx);
            } else if editor.kind == "project" {
                self.rename_native_sidebar_project(&editor.id, title, cx);
            } else {
                self.dispatch_native_sidebar_command(
                    json!({"type": "renameGroup", "groupId": editor.id, "title": title}),
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Saves a project's new name through gxserver (`/api/renameProject`, on the computer the
    /// project is on), which publishes the renamed project to every client; only the name changes.
    fn rename_native_sidebar_project(
        &mut self,
        group_id: &str,
        title: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(scope) = self.git_scope_for_group(group_id) else {
            return self.gx_store_create_toast(
                "error",
                "Could not rename project",
                Some("Its computer isn't connected right now. Try again once it is."),
                cx,
            );
        };
        if scope.title == title {
            return;
        }
        let params = json!({ "projectId": scope.project_id(), "newName": title });
        let remote = scope.remote.clone();
        cx.spawn(async move |this, cx| {
            let result = crate::app::gx_store::git::calls::call(
                remote,
                "/api/renameProject",
                params,
                None,
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = result {
                    // An older gxserver there answers "No gxserver endpoint for POST /api/renameProject".
                    let description = if error.message.contains("/api/renameProject") {
                        "That computer's Ghostex is too old to rename projects. Update it, then try again.".to_string()
                    } else {
                        error.message
                    };
                    this.gx_store_create_toast(
                        "error",
                        "Could not rename project",
                        Some(&description),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn native_sidebar_input_owns_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.native_sidebar
            .menu
            .as_ref()
            .is_some_and(|menu| menu.focus.is_focused(window))
            || self
                .native_sidebar
                .name_editor
                .as_ref()
                .is_some_and(|editor| editor.input.read(cx).focus_handle(cx).is_focused(window))
    }
}
