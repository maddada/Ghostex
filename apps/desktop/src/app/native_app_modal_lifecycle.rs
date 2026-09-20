//! One open, fit, deliver, and close path for every native GPUI app modal.
//!
//! CDXC:AppModal 2026-09-15 WHY:
//! The React app modals share one reusable CEF child window (`open_gpui_app_modal_window_inner`).
//! The native GPUI modals share this path instead: one borderless child window centered on the
//! main window, opened at the modal's first-frame estimate and then sized by the modal's own
//! layout, with the app holding a single `NativeAppModal` so the "one app modal at a time" rule
//! spans both hosts. Every native modal's window root is a gpui-component `Root` because its text
//! inputs read the window root as one while painting.
//! SEE-ALSO: apps/desktop/src/app/window/native_modal_kit.rs (chrome and controls), apps/desktop/src/app/modals.rs (the React host launcher that closes a native modal when it opens).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;
use gpui::{AnyEntity, WindowHandle};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) struct NativeAppModal {
    pub(crate) kind: GpuiAppModalKind,
    pub(crate) window: WindowHandle<Root>,
    pub(crate) view: AnyEntity,
}

impl GhostexGpuiApp {
    /// The modal palette for the current appearance and sidebar theme.
    pub(crate) fn gpui_native_modal_palette(&self) -> ModalPalette {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        ModalPalette::resolve(
            CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed),
            settings
                .object()
                .get("sidebarTheme")
                .and_then(serde_json::Value::as_str),
        )
    }

    /// Opens `kind` as a native window whose content is built by `build`.
    /// Replaces any open app modal, React or native.
    pub(crate) fn open_native_app_modal<V: Render>(
        &mut self,
        kind: GpuiAppModalKind,
        width: f32,
        initial_height: f32,
        build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.remove_gpui_app_modal_window_without_focus_restore(cx);
        self.remove_native_app_modal_window(cx);
        let window_size = size(px(width), px(initial_height));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered_at(
                self.main_window_bounds.center(),
                window_size,
            ))),
            app_id: gpui_platform_window_app_id(),
            focus: true,
            icon: gpui_platform_window_icon(),
            show: true,
            is_resizable: false,
            is_minimizable: false,
            display_id: self.main_window_display_id,
            titlebar: None,
            ..Default::default()
        };
        let view_slot: Rc<RefCell<Option<AnyEntity>>> = Rc::new(RefCell::new(None));
        let view_out = view_slot.clone();
        let window = cx
            .open_window(options, move |window, cx| {
                window.set_window_title("");
                window.activate_window();
                let view = build(window, cx);
                *view_out.borrow_mut() = Some(view.clone().into_any());
                cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
            })
            .ok();
        let view = view_slot.borrow_mut().take();
        self.native_app_modal = match (window, view) {
            (Some(window), Some(view)) => Some(NativeAppModal { kind, window, view }),
            _ => None,
        };
    }

    pub(crate) fn native_app_modal_kind(&self) -> Option<GpuiAppModalKind> {
        self.native_app_modal.as_ref().map(|modal| modal.kind)
    }

    /// The one switch from a modal kind to its native opener. `open_message` is
    /// the same payload the React host would have received (verbatim sidebar
    /// message or the allowlisted rebuild, plus `latestSidebarStateMessage`
    /// where the kind requires it). Returns false for kinds still hosted in
    /// React so the launcher falls through to the CEF window.
    pub(crate) fn try_open_native_app_modal(
        &mut self,
        kind: GpuiAppModalKind,
        open_message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let return_focus_target = gpui_app_modal_command_return_focus_target(
            kind,
            open_message,
            self.shell_focus,
            &self.command_pane,
        );
        match kind {
            GpuiAppModalKind::ExportTranscriptResult => {
                self.open_gpui_export_transcript_modal(open_message, cx);
            }
            GpuiAppModalKind::RenameSession => {
                self.open_gpui_rename_session_modal(open_message, cx);
            }
            GpuiAppModalKind::SessionNote => {
                self.open_gpui_session_note_modal(open_message, cx);
            }
            GpuiAppModalKind::AgentHooksRequired => {
                self.open_gpui_agent_hooks_required_modal(open_message, cx);
            }
            GpuiAppModalKind::MissingProjectFolder => {
                self.open_gpui_missing_project_folder_modal(open_message, cx);
            }
            GpuiAppModalKind::PortlessSetup => {
                self.open_gpui_portless_setup_modal(open_message, cx);
            }
            GpuiAppModalKind::RemoteGxserverInstall => {
                self.open_gpui_remote_gxserver_install_native_modal(open_message, cx);
            }
            GpuiAppModalKind::DeleteWorktree => {
                self.open_gpui_delete_worktree_modal(open_message, cx);
            }
            GpuiAppModalKind::RenameWorktree => {
                self.open_gpui_rename_worktree_modal(open_message, cx);
            }
            GpuiAppModalKind::DelayedSend => {
                self.open_gpui_delayed_send_modal(open_message, cx);
            }
            GpuiAppModalKind::SidebarSpaceEditor => {
                self.open_gpui_space_editor_modal(open_message, cx);
            }
            GpuiAppModalKind::UpdateAvailable => {
                self.open_gpui_update_available_modal(open_message, cx);
            }
            GpuiAppModalKind::RemoteSetup => {
                self.open_gpui_remote_setup_modal(open_message, cx);
            }
            GpuiAppModalKind::Worktree => {
                self.open_gpui_create_worktree_modal(open_message, cx);
            }
            GpuiAppModalKind::CommandPalette
            | GpuiAppModalKind::RecentProjects
            | GpuiAppModalKind::PreviousSessions
            | GpuiAppModalKind::StashedPrompts => {
                self.open_gpui_quick_access_modal(kind, open_message, cx);
            }
            // NATIVE-MODAL-OPEN-ARMS: one arm per converted modal kind.
            _ => return false,
        }
        if self.native_app_modal_kind() == Some(kind) {
            self.app_modal_command_return_focus_target =
                gpui_app_modal_command_return_focus_target_for_active_modal(
                    self.app_modal_command_return_focus_target,
                    return_focus_target,
                );
        }
        true
    }

    /// Routes a message meant for the open app-modal window (`toast`,
    /// `projectWorktreesResult`, `delayedSendAgents`, ...) to the open native
    /// modal. Returns false when no native modal is open or it does not consume
    /// that message type, so the React host path can have it.
    pub(crate) fn receive_native_app_modal_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(kind) = self.native_app_modal_kind() else {
            return false;
        };
        match kind {
            GpuiAppModalKind::MissingProjectFolder => {
                self.receive_gpui_missing_project_folder_modal_message(message, cx)
            }
            GpuiAppModalKind::DelayedSend => {
                self.receive_gpui_delayed_send_modal_message(message, cx)
            }
            GpuiAppModalKind::Worktree => {
                self.receive_gpui_create_worktree_modal_message(message, cx)
            }
            // NATIVE-MODAL-MESSAGE-ARMS: one arm per modal kind that receives host messages.
            _ => {
                let _ = cx;
                false
            }
        }
    }

    /// A host callback for a native modal: commands are handed to `handler`
    /// on the app through `defer`, because the modal sends them from inside
    /// its own window update and results reach it from inside an app update.
    pub(crate) fn native_app_modal_host<C: 'static>(
        &self,
        cx: &mut gpui::Context<Self>,
        handler: impl Fn(&mut Self, C, &mut gpui::Context<Self>) + 'static,
    ) -> Rc<dyn Fn(C, &mut App)> {
        let main_app = cx.weak_entity();
        let handler = Rc::new(handler);
        Rc::new(move |command, cx: &mut App| {
            let main_app = main_app.clone();
            let handler = handler.clone();
            cx.defer(move |cx| {
                let _ = main_app.update(cx, |app, cx| handler(app, command, cx));
            });
        })
    }

    /// Runs `update` against the open native modal of `kind`. Returns `None`
    /// when no such modal is open; a dead window handle is dropped on the way.
    pub(crate) fn update_native_app_modal<V: 'static, R>(
        &mut self,
        kind: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
        update: impl FnOnce(&mut V, &mut Window, &mut gpui::Context<V>) -> R,
    ) -> Option<R> {
        let modal = self.native_app_modal.as_ref()?;
        if modal.kind != kind {
            return None;
        }
        let view = modal.view.clone().downcast::<V>().ok()?;
        let result = modal.window.update(cx, |_root, window, cx| {
            view.update(cx, |view, cx| update(view, window, cx))
        });
        match result {
            Ok(result) => Some(result),
            Err(_) => {
                self.native_app_modal = None;
                None
            }
        }
    }

    /// Drops the handle after a native modal removed its own window and gives
    /// the command pane its focus back, the way the React host's close does.
    pub(crate) fn release_native_app_modal_window(
        &mut self,
        kind: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.native_app_modal_kind() != Some(kind) {
            return;
        }
        self.native_app_modal = None;
        self.restore_gpui_app_modal_command_return_focus_if_needed(cx);
        self.resume_deferred_gpui_portless_setup_prompt(cx);
    }

    /// The sidebar runtime's `close` bridge message for a modal that is native
    /// now (a relocated project folder, a finished flow): remove the window and
    /// give the command pane its focus back like the React host's close did.
    pub(crate) fn close_native_app_modal_from_bridge(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.native_app_modal.is_none() {
            return false;
        }
        let return_focus_target = self.app_modal_command_return_focus_target;
        self.remove_native_app_modal_window(cx);
        self.app_modal_command_return_focus_target = return_focus_target;
        self.restore_gpui_app_modal_command_return_focus_if_needed(cx);
        self.resume_deferred_gpui_portless_setup_prompt(cx);
        true
    }

    /// Removes the open native modal window (another modal is opening, or the
    /// same modal is reopened with a new request).
    pub(crate) fn remove_native_app_modal_window(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(modal) = self.native_app_modal.take() else {
            return false;
        };
        self.app_modal_command_return_focus_target = None;
        if modal.kind == GpuiAppModalKind::ExportTranscriptResult {
            self.pending_export_transcript_reveal_path = None;
        }
        // Quick Access keeps a live controller in the sidebar runtime; tell it to
        // stop publishing when its window is replaced or dismissed.
        if crate::app::window::quick_access::QuickAccessTabId::from_modal_kind(modal.kind).is_some()
        {
            self.dispatch_gpui_quick_access_command(serde_json::json!({ "type": "closed" }), cx);
        }
        modal
            .window
            .update(cx, |_root, window, _cx| {
                window.remove_window();
            })
            .is_ok()
    }
}
