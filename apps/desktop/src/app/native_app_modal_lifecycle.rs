//! One open, fit, deliver, and close path for every native GPUI app modal.
//!
//! CDXC:AppModal 2026-09-15 WHY:
//! The React app modals share one reusable CEF child window (`open_gpui_app_modal_window_inner`).
//! The native GPUI modals share this path instead: one borderless child window centered on the
//! main window, opened at the modal's first-frame estimate and then sized by the modal's own
//! layout, with the app holding a single `NativeAppModal` so the "one app modal at a time" rule
//! spans both hosts. Every native modal's window root is a gpui-component `Root` because its text
//! inputs read the window root as one while painting.
//! SEE-ALSO: apps/desktop/src/app/window/native_modal_kit/ (chrome and controls), apps/desktop/src/app/modals/modal_window.rs (the React host launcher that closes a native modal when it opens).
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
    /// The modal palette for the current appearance and sidebar theme, frosted under window glass,
    /// where the modal's window blurs what is behind it (`open_native_app_modal`).
    pub(crate) fn gpui_native_modal_palette(&self) -> ModalPalette {
        let palette = self.gpui_native_modal_solid_palette();
        if !window_glass_active() {
            return palette;
        }
        palette.frosted(frosted_modal_fill(palette.surface.into()).into())
    }

    /// The palette of a form dialog (Send Bug / Feedback, Add Project, Rename and the like): under
    /// window glass the Accounts & limits popover's thinner frost (CDXC:AppModal 2026-10-29 on
    /// `frosted_dialog_fill`).
    pub(crate) fn gpui_native_dialog_palette(&self) -> ModalPalette {
        let palette = self.gpui_native_modal_solid_palette();
        if !window_glass_active() {
            return palette;
        }
        palette.frosted_dialog(frosted_dialog_fill(palette.surface.into()).into())
    }

    /// The palette of a large panel (Settings and its pages, the Agents Hub, the New Thread
    /// picker): the dialogs' frost and washes over the darker panel tint (`frosted_panel_fill`).
    pub(crate) fn gpui_native_panel_palette(&self) -> ModalPalette {
        let palette = self.gpui_native_modal_solid_palette();
        if !window_glass_active() {
            return palette;
        }
        palette.frosted_dialog(frosted_panel_fill(palette.surface.into()).into())
    }

    /// The modal palette for the current appearance and sidebar theme with glass off.
    fn gpui_native_modal_solid_palette(&self) -> ModalPalette {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        ModalPalette::resolve(
            CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed),
            settings
                .object()
                .get("sidebarTheme")
                .and_then(serde_json::Value::as_str),
        )
        .tinted(titlebar_background().into())
    }

    /// Opens `kind` as a native window whose content is built by `build`.
    /// Replaces any open app modal, React or native.
    pub(crate) fn open_native_app_modal<V: ModalCornerClose>(
        &mut self,
        kind: GpuiAppModalKind,
        width: f32,
        initial_height: f32,
        build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.remove_gpui_app_modal_window_without_focus_restore(cx);
        self.remove_native_app_modal_window(cx);
        // Settings is back: the Back to Settings button of a claude.ai sign-in has done its job.
        if kind == GpuiAppModalKind::Settings {
            crate::app::claude_cloud_sign_in_return::hide_claude_sign_in_return_button(cx);
        }
        let (width, initial_height) = if kind.is_large_panel() {
            let (width, height) = kind.large_panel_size(width, initial_height);
            self.gpui_native_modal_size_on_screen(size(px(width), px(height)), cx)
        } else {
            (width, initial_height)
        };
        let window_size = size(px(width), px(initial_height));
        // The page window is moved and resized by the reader and opens where they left the last one
        // (CDXC:SessionChat 2026-11-04 DECISION in app/window/visual_page_modal/frame.rs).
        let movable = kind == GpuiAppModalKind::VisualPage;
        let window_bounds = if movable {
            use crate::app::window::visual_page_modal::frame;
            // Placed in the space all displays share, against the displays there are now, and
            // given back measured from the main window's display, as `display_id` below reads it.
            let shift = self
                .main_window_popup_owner()
                .to_global(gpui::point(px(0.0), px(0.0)), cx);
            let main =
                crate::app::ghostex_capture::platform::window_frame(self.parent_ns_view as usize)
                    .unwrap_or(gpui::Bounds::new(
                        self.main_window_bounds.origin + shift,
                        self.main_window_bounds.size,
                    ));
            let opening = frame::opening_frame(main, window_size, &frame::Screens::now(main, cx));
            gpui::Bounds::new(opening.origin - shift, opening.size)
        } else {
            gpui::Bounds::centered_at(self.main_window_bounds.center(), window_size)
        };
        let options = WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            window_decorations: crate::app::window::popup_frame::child_window_decorations(),
            #[cfg(target_os = "linux")]
            x11_parent: self.main_window_handle,
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            app_id: gpui_platform_window_app_id(),
            focus: true,
            icon: gpui_platform_window_icon(),
            show: true,
            is_resizable: movable,
            window_min_size: movable
                .then_some(crate::app::window::visual_page_modal::frame::MIN_SIZE),
            is_minimizable: false,
            display_id: self
                .main_window_popup_owner()
                .display_for(window_bounds, cx),
            titlebar: None,
            // Under window glass the modal draws the frosted palette over what its window blurs.
            window_background: window_glass_background_appearance(),
            ..Default::default()
        };
        let view_slot: Rc<RefCell<Option<AnyEntity>>> = Rc::new(RefCell::new(None));
        let view_out = view_slot.clone();
        let palette = self.gpui_native_modal_palette();
        let window_border = palette.window_border();
        let main_window_native_view = self.parent_ns_view;
        let window = cx
            .open_window(options, move |window, cx| {
                crate::app::window::popup_frame::frame_app_modal_window(window, window_border);
                apply_frosted_menu_blur(window);
                window.set_window_title(if cfg!(any(target_os = "windows", target_os = "linux")) {
                    kind.window_title()
                } else {
                    ""
                });
                window.activate_window();
                // An AppKit child of the main window moves with it (CDXC:AppModal 2026-10-04 in
                // workspace_windows/owned_windows.rs); a no-op elsewhere.
                attach_gpui_app_modal_window_to_main_window(window, main_window_native_view);
                if movable && let Ok(native) = crate::app::helpers::cef_parent_native_view(window) {
                    crate::app::ghostex_capture::platform::make_resizable(
                        native as usize,
                        crate::app::window::visual_page_modal::frame::MIN_SIZE,
                    );
                }
                /*
                CDXC:AppModal 2026-10-06 WHY:
                GPUI owns a Windows pop-up by whichever window is active when it opens. A modal opened while another modal was active (`ghostex settings open` with Settings already open, any modal replacing another) was owned by the modal it replaced, and that modal's removal, which GPUI runs after this window exists, destroyed this one with it: every second `settings open` closed Settings. An app modal belongs to the main window, so it is owned by it.
                */
                #[cfg(target_os = "windows")]
                crate::app::window::own_gpui_popup_window(window, main_window_native_view);
                let view = build(window, cx);
                *view_out.borrow_mut() = Some(view.clone().into_any());
                let frame = cx.new(|_| ModalWindowFrame::new(view, palette));
                cx.new(|cx| {
                    Root::new(frame, window, cx)
                        .bordered(false)
                        .bg(gpui::transparent_black())
                })
            })
            .ok();
        let view = view_slot.borrow_mut().take();
        self.native_app_modal = match (window, view) {
            (Some(window), Some(view)) => Some(NativeAppModal { kind, window, view }),
            _ => None,
        };
        self.sync_app_modal_over_chat(cx);
    }

    /// Tells every chat whether an app modal is open over the main window, so their floating
    /// controls (which sit at a higher window level than the modal) step down while it is.
    pub(crate) fn sync_app_modal_over_chat(&self, cx: &mut gpui::App) {
        crate::app::native_chat::frosted_overlay_window::set_app_modal_over_chat(
            self.native_app_modal.is_some() || self.app_modal_window.is_some(),
            cx,
        );
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
            GpuiAppModalKind::NewCoordinator => {
                self.open_gpui_new_coordinator_modal(open_message, cx);
            }
            GpuiAppModalKind::MakeCoordinator => {
                self.open_gpui_make_coordinator_modal(open_message, cx);
            }
            GpuiAppModalKind::RemoteSetup => {
                self.open_gpui_connect_device_modal(open_message, cx);
            }
            GpuiAppModalKind::Worktree => {
                self.open_gpui_create_worktree_modal(open_message, cx);
            }
            GpuiAppModalKind::CommandPalette
            | GpuiAppModalKind::RecentProjects
            | GpuiAppModalKind::PreviousSessions
            | GpuiAppModalKind::StashedPrompts
            | GpuiAppModalKind::QuickAccessFiles => {
                self.open_gpui_quick_access_modal(kind, open_message, cx);
            }
            // CDXC:AppModal 2026-09-27 DECISION:
            // User: "i want the easier to move to gpui ones to actually be switched now" (Browser History, the
            // Markdown table popup and the Mermaid diagram popup; Settings and Agents Hub stayed React for later,
            // and Agents Hub moved on 2026-09-28 below),
            // then "lets migrate find by prompt modal to gpui also please but make it stays exactly same as react
            // one we have now and make sure it's performant" (Search by Prompt).
            // Their React dialogs were deleted, so these kinds must not fall back to the modal host.
            GpuiAppModalKind::BrowserHistory => {
                self.open_gpui_browser_history_modal(open_message, cx);
            }
            GpuiAppModalKind::MarkdownTable => {
                self.open_gpui_markdown_table_modal(open_message, cx);
            }
            GpuiAppModalKind::VisualPage => {
                self.open_gpui_visual_page_modal(open_message, cx);
            }
            GpuiAppModalKind::MermaidDiagram => {
                self.open_gpui_mermaid_diagram_modal(open_message, cx);
            }
            GpuiAppModalKind::FindPrompts => {
                self.open_gpui_find_prompts_modal(cx);
            }
            GpuiAppModalKind::AddProject => {
                self.open_gpui_add_project_modal(open_message, cx);
            }
            GpuiAppModalKind::AgentsHub => {
                self.open_gpui_agents_hub_modal(open_message, cx);
            }
            // The first run, Tips > Setup and Quick Access > Setup all open the native onboarding (onboarding_modal_lifecycle.rs).
            GpuiAppModalKind::Onboarding => {
                self.open_gpui_onboarding_modal(open_message, cx);
            }
            // Every Settings page is native (window/settings_modal/mod.rs).
            GpuiAppModalKind::Settings
            | GpuiAppModalKind::Hotkeys
            | GpuiAppModalKind::ConfigureAgents
            | GpuiAppModalKind::ConfigureActions
            | GpuiAppModalKind::OpenTargets => {
                self.open_gpui_settings_modal(kind, open_message, cx);
            }
            GpuiAppModalKind::Feedback => {
                self.open_gpui_feedback_modal(false, cx);
            }
            GpuiAppModalKind::CreateLinearTicket => {
                self.open_gpui_create_linear_ticket_modal(open_message, cx);
            }
            GpuiAppModalKind::WorkLinkPicker => {
                self.open_gpui_work_link_picker_modal(open_message, cx);
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
            kind if kind.is_settings_modal_entry() => {
                self.receive_native_settings_modal_payload(message, cx)
            }
            GpuiAppModalKind::Onboarding => self.receive_gpui_onboarding_modal_message(message, cx),
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
                self.sync_app_modal_over_chat(cx);
                None
            }
        }
    }

    /// Repaints the frame of the open native modal of `kind` in the current modal palette: its
    /// corner close button, and on Windows its window border. `open_native_app_modal` hands both
    /// the palette of the moment it opens.
    pub(crate) fn refresh_native_app_modal_frame_palette(
        &mut self,
        kind: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(window) = self
            .native_app_modal
            .as_ref()
            .filter(|modal| modal.kind == kind)
            .map(|modal| modal.window)
        else {
            return;
        };
        let palette = self.gpui_native_modal_palette();
        let _ = window.update(cx, |root, window, cx| {
            crate::app::window::popup_frame::frame_app_modal_window(
                window,
                palette.window_border(),
            );
            if let Ok(frame) = root.view().clone().downcast::<ModalWindowFrame>() {
                frame.update(cx, |frame, cx| frame.set_palette(palette, cx));
            }
        });
    }

    /// Drops the handle after a native modal removed its own window and gives
    /// the last focused pane its keyboard back, the way the React host's close does.
    pub(crate) fn release_native_app_modal_window(
        &mut self,
        kind: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.native_app_modal_kind() != Some(kind) {
            return;
        }
        self.native_app_modal = None;
        self.sync_app_modal_over_chat(cx);
        self.restore_keyboard_focus_after_app_modal(cx);
    }

    /// The app's `close` bridge message (`close_app_modal_from_bridge`, sent by the
    /// store; the sidebar runtime sent it until 2026-09-25) for a modal that is native
    /// now (a relocated project folder, a finished flow): remove the window and
    /// give the last focused pane its keyboard back like the React host's close did.
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
        self.restore_keyboard_focus_after_app_modal(cx);
        true
    }

    /// Removes the open native modal window (another modal is opening, or the
    /// same modal is reopened with a new request).
    pub(crate) fn remove_native_app_modal_window(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(modal) = self.native_app_modal.take() else {
            return false;
        };
        self.sync_app_modal_over_chat(cx);
        self.app_modal_command_return_focus_target = None;
        if modal.kind == GpuiAppModalKind::ExportTranscriptResult {
            self.pending_export_transcript_reveal_path = None;
        }
        // A paste confirmation replaced by another modal is a cancel.
        if modal.kind == GpuiAppModalKind::TerminalPasteConfirm {
            self.pending_terminal_paste_confirmation = None;
            self.terminal_paste_confirmation_dialog_open = false;
        }
        // Quick Access keeps a live controller (quick_access/host.rs); tell it to
        // stop publishing when its window is replaced or dismissed.
        if crate::app::window::quick_access::QuickAccessTabId::from_modal_kind(modal.kind).is_some()
        {
            self.dispatch_gpui_quick_access_command(serde_json::json!({ "type": "closed" }), cx);
        }
        // A pairing request the Connect a phone window showed gets its notice back.
        if modal.kind == GpuiAppModalKind::RemoteSetup {
            self.owner_security_show_pending_pairing_requests(cx);
        }
        modal
            .window
            .update(cx, |_root, window, _cx| {
                window.remove_window();
            })
            .is_ok()
    }
}
