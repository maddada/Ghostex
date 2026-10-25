//! Opening links and files from a chat, copy-path fallbacks for unavailable work areas, and pending Docs file opens.

use std::path::Path;

use gpui::ClipboardItem;
use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::session_chat_context_menu::{
    GpuiResolvedSessionChatFile, GpuiSessionChatFileResolutionError, resolve_session_chat_file,
};
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /*
    CDXC:SessionChat 2026-08-03:
    A web link in the conversation opens where the reader already is: the
    integrated Browser workarea, through the same renderer-open path as
    `ghostex browser open` (same-origin reuse, so re-clicking a dev-server URL
    does not multiply tabs). Shift+click is the explicit escape hatch to the OS
    browser and takes the http/https-only external opener.

    CDXC:SessionChat 2026-08-18:
    Chat web links answer to the same "Open links in embedded browser" setting
    as Command-clicked terminal links, so a single switch decides where every
    agent-sent web link lands. With that setting off, an ordinary click leaves
    for the system default browser exactly like Shift+click already does.

    The transcript context menu is explicit: its embedded row bypasses that
    ordinary-click preference, while its external row uses the OS browser.
    Remote loopback links retain their machine through the embedded tunnel;
    the OS browser cannot interpret that machine's localhost address.
    */
    pub(crate) fn open_session_chat_link(
        &mut self,
        url: &str,
        source_session_id: Option<TerminalSessionId>,
        external: bool,
        force_embedded: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if url.chars().count() > GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS {
            return;
        }
        if let Some(session_id) = source_session_id
            && self.try_open_remote_loopback_link(
                url,
                GpuiEngineTerminalEventTarget::Agents(session_id),
                cx,
            )
        {
            return;
        }
        let open_in_app =
            shared_settings::shared_sidebar_settings_snapshot().web_links_open_in_app();
        if external || (!force_embedded && !open_in_app) {
            if let Some(url) = normalize_address(url) {
                let _ = gpui_open_external_http_url(&url);
            }
            return;
        }
        self.open_browser_url_from_renderer_command(
            GpuiSidebarOpenBrowserUrlMessage {
                url: url.to_string(),
                reuse: GpuiBrowserRendererOpenReuse::Similar,
                from_quick_header: false,
                project_id: None,
            },
            window,
            cx,
        );
    }

    /*
    CDXC:SessionChat 2026-08-03:
    Absolute paths and home-relative paths stay machine paths, while ordinary
    relative paths resolve against the clicked session's project and then its
    tracked working directory. Where the file then opens is Settings > File
    opening (`file_open_target_for_path`), falling back to the other available
    workarea.
    */
    pub(crate) fn open_session_chat_file(
        &mut self,
        path: &str,
        line: Option<u32>,
        column: Option<u32>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            self.report_session_chat_file_open_failure(
                "No terminal session is focused in this window.",
                cx,
            );
            return;
        };
        self.open_session_chat_file_for_session(session_id, path, line, column, None, window, cx);
    }

    pub(crate) fn open_session_chat_file_for_session(
        &mut self,
        session_id: TerminalSessionId,
        path: &str,
        line: Option<u32>,
        column: Option<u32>,
        requested_view: Option<shared_settings::SharedChatFileOpenView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let trimmed = path.trim();
        if trimmed.is_empty() || trimmed.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS {
            return;
        }
        if self
            .agents_chat_remote_key_for_session(session_id)
            .is_some()
        {
            self.open_remote_session_chat_file(
                session_id,
                trimmed,
                line,
                column,
                requested_view,
                window,
                cx,
            );
            return;
        }
        let context = match self.session_chat_file_resolution_context(session_id) {
            Ok(context) => context,
            Err(_) => {
                self.report_session_chat_file_open_failure(
                    "That session's project folder is unavailable.",
                    cx,
                );
                return;
            }
        };
        let trimmed = trimmed.to_string();
        let path = trimmed.clone();
        let window_handle = Window::window_handle(window);
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let resolved = background
                .spawn(async move { resolve_session_chat_file(context, &path) })
                .await;
            let _ = cx.update_window(window_handle, |view, window, app_cx| {
                if let Ok(view) = view.downcast::<GhostexGpuiApp>() {
                    let _ = view.update(app_cx, |this, cx| {
                        this.finish_session_chat_file_open(
                            resolved,
                            trimmed,
                            line,
                            column,
                            requested_view,
                            session_id,
                            window,
                            cx,
                        );
                    });
                }
            });
        })
        .detach();
    }

    fn finish_session_chat_file_open(
        &mut self,
        resolution: Result<GpuiResolvedSessionChatFile, GpuiSessionChatFileResolutionError>,
        path: String,
        line: Option<u32>,
        column: Option<u32>,
        requested_view: Option<shared_settings::SharedChatFileOpenView>,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let resolved = match resolution {
            Ok(resolved) => resolved,
            Err(GpuiSessionChatFileResolutionError::InvalidPath) => return,
            Err(GpuiSessionChatFileResolutionError::ProjectUnavailable) => {
                self.report_session_chat_file_open_failure(
                    "That session's project folder is unavailable.",
                    cx,
                );
                return;
            }
            Err(GpuiSessionChatFileResolutionError::NotFile) => {
                self.report_session_chat_file_open_failure("That path is not a file.", cx);
                return;
            }
            Err(GpuiSessionChatFileResolutionError::NotFound) => {
                self.copy_unresolved_session_chat_file_path(&path, cx);
                return;
            }
        };
        // CDXC:SessionChat 2026-09-23 DECISION:
        // User: local folder links open in the system file explorer; remote folder links are browsed in this computer's Code view. This replaces the earlier explorer-only rule for remote folders.
        if resolved.is_directory {
            if gpui_open_path(&resolved.file_path).is_err() {
                self.report_session_chat_file_open_failure(
                    &format!("Could not open that folder in {GPUI_FILE_MANAGER_NAME}."),
                    cx,
                );
            }
            return;
        }
        self.open_resolved_project_file(
            resolved,
            line,
            column,
            requested_view,
            Some(session_id),
            window,
            cx,
        );
    }

    /// Opens a file of a local project where Settings > File opening says (or in the view a
    /// transcript menu row asked for): the Files view, the Code view, an editor app or the system
    /// app. `origin` is the chat it was clicked in, which Files remembers as the document's agent.
    ///
    /// CDXC:SessionChat 2026-09-27 DECISION:
    /// User: images, videos and audio clicked in chat open in the Files view by default, or in the system app when their setting says so or Files cannot play the format. PDFs keep opening in the system app.
    /// SEE-ALSO: `file_open_target_for_path` in `apps/desktop/src/app/native_docs/entry.rs` holds every kind's rule; `media_kind` in `packages/gx-chat-core/src/composer/reference_pills.rs` names the same files in labels and menu rows.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_resolved_project_file(
        &mut self,
        resolved: GpuiResolvedSessionChatFile,
        line: Option<u32>,
        column: Option<u32>,
        requested_view: Option<shared_settings::SharedChatFileOpenView>,
        origin: Option<TerminalSessionId>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let file_path = resolved.file_path;
        let root = resolved.project_root;
        let session_project_id = Some(resolved.project_id);
        let project_relative_path = file_path
            .strip_prefix(&root)
            .ok()
            .map(|relative| relative.to_string_lossy().replace('\\', "/"));
        let resolved_path = file_path.to_string_lossy().into_owned();
        let target = match requested_view {
            Some(shared_settings::SharedChatFileOpenView::Code) => {
                shared_settings::SharedFileOpenTarget::Code
            }
            Some(shared_settings::SharedChatFileOpenView::Docs) => {
                shared_settings::SharedFileOpenTarget::Files
            }
            None => crate::app::native_docs::entry::local_file_open_target(&file_path),
        };
        match &target {
            shared_settings::SharedFileOpenTarget::SystemApp => {
                if gpui_open_path(&file_path).is_err() {
                    self.report_session_chat_file_open_failure(
                        "The operating system could not open that file.",
                        cx,
                    );
                }
                return;
            }
            // An editor that is no longer installed falls back to the Files view.
            shared_settings::SharedFileOpenTarget::Editor(id) => {
                if let Some(editor) = gpui_installed_editor_open_target(id) {
                    match gpui_launch_open_target(&editor, &file_path) {
                        Ok(()) => {
                            self.report_session_chat_file_opening(&editor.label, &file_path, cx)
                        }
                        Err(message) => self.report_session_chat_file_open_failure(&message, cx),
                    }
                    return;
                }
            }
            _ => {}
        }
        let preferred = match target {
            shared_settings::SharedFileOpenTarget::Code => {
                shared_settings::SharedChatFileOpenView::Code
            }
            _ => shared_settings::SharedChatFileOpenView::Docs,
        };
        let docs_available = !gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage)
            && self.titlebar_mode_available(TitlebarMode::Manage);
        let code_unavailable_reason =
            if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
                || !self.titlebar_mode_available(TitlebarMode::Source)
            {
                Some("Code view is not available for this project.")
            } else {
                self.embedded_code_editor_unavailable_reason()
            };
        let code_available = code_unavailable_reason.is_none();
        let destination = match requested_view {
            Some(shared_settings::SharedChatFileOpenView::Code) if code_available => requested_view,
            Some(shared_settings::SharedChatFileOpenView::Docs) if docs_available => requested_view,
            Some(_) => {
                self.report_session_chat_file_open_failure(
                    "That view is not available for this file.",
                    cx,
                );
                return;
            }
            None => match preferred {
                shared_settings::SharedChatFileOpenView::Docs if docs_available => {
                    Some(shared_settings::SharedChatFileOpenView::Docs)
                }
                shared_settings::SharedChatFileOpenView::Code if code_available => {
                    Some(shared_settings::SharedChatFileOpenView::Code)
                }
                _ if docs_available => Some(shared_settings::SharedChatFileOpenView::Docs),
                _ if code_available => Some(shared_settings::SharedChatFileOpenView::Code),
                _ => None,
            },
        };

        if destination.is_none() {
            self.copy_path_for_unavailable_project_workarea(
                &resolved_path,
                "Files and Code",
                "Files and Code views are not available for this project.",
                cx,
            );
            return;
        }

        if destination == Some(shared_settings::SharedChatFileOpenView::Docs) {
            // The Files view lists the whole project, so any project file is a tree path.
            let relative_path = if let Some(relative_path) = project_relative_path.clone() {
                relative_path
            } else {
                let Some(project_id) = session_project_id else {
                    self.copy_path_for_unavailable_project_workarea(
                        &resolved_path,
                        "Files",
                        "No active project can authorize this file for Files.",
                        cx,
                    );
                    return;
                };
                match authorize_manage_chat_file(&project_id, &file_path) {
                    Ok(path) => path,
                    Err(error) => {
                        self.report_session_chat_file_open_failure(&error, cx);
                        return;
                    }
                }
            };
            self.report_session_chat_file_opening("Files view", &file_path, cx);
            self.pending_docs_file_open = Some(relative_path);
            self.native_docs.pending_origin = origin;
            self.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
            self.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
            self.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
            if !self.deliver_pending_docs_file_open(cx) {
                self.schedule_pending_docs_file_open_delivery(cx);
            }
            return;
        }
        self.report_session_chat_file_opening("Code view", &file_path, cx);
        self.pending_source_file_open = Some(PendingSourceFileOpen {
            column,
            file_path,
            line,
            origin: PendingSourceFileOpenOrigin::SessionChat,
            project_path: resolved.project_path,
            remote_target: None,
            remote_working_directory: None,
        });
        self.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Source, cx);
        self.focus_project_editor_surface(TitlebarMode::Source, window, cx);
    }

    pub(crate) fn copy_path_for_disabled_project_workarea(
        &mut self,
        path: &str,
        plugin_name: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        gpui_copy_to_clipboard(ClipboardItem::new_string(path.to_string()), cx);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: format!(
                    "gpui-disabled-{}-file-path-copied",
                    plugin_name.to_ascii_lowercase()
                ),
                level: GpuiAppToastLevel::from_raw(Some("success")),
                title: "Copied to Clipboard!".to_string(),
                description: Some(format!("({plugin_name} plugin is disabled)")),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    pub(crate) fn copy_path_for_unavailable_project_workarea(
        &mut self,
        path: &str,
        workarea_name: &str,
        reason: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        gpui_copy_to_clipboard(ClipboardItem::new_string(path.to_string()), cx);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: format!(
                    "gpui-unavailable-{}-file-path-copied",
                    workarea_name.to_ascii_lowercase()
                ),
                level: GpuiAppToastLevel::from_raw(Some("success")),
                title: "Copied path to clipboard".to_string(),
                description: Some(reason.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    /**
    CDXC:SessionChat 2026-08-23:
    A path that does not resolve here is still the answer to "which file was
    that?": an agent quotes partial paths, paths relative to a subdirectory it
    was working in, and paths on a remote checkout, any of which can name a
    file that is sitting right there on disk. The toast names that (missing
    file or incomplete path), copies the path, and points at Code view's file
    search, which is the tool that turns a fragment back into the real file.
    When Code is not reachable at all this defers to the disabled-workarea
    copy, so the toast never names a place the reader cannot go.
    */
    pub(crate) fn copy_unresolved_session_chat_file_path(
        &mut self,
        path: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
            || !self.titlebar_mode_available(TitlebarMode::Source)
        {
            self.copy_path_for_disabled_project_workarea(path, "Code", cx);
            return;
        }
        // Naming Code view is only useful advice when Code view can actually
        // search: with the component uninstalled the tab is a prompt to install
        // it, so the toast stops at the copy.
        let description = if self.embedded_code_editor_unavailable_reason().is_some() {
            "Couldn't find this file in the session's project.".to_string()
        } else {
            "Couldn't find this file. Try searching in Code.".to_string()
        };
        gpui_copy_to_clipboard(ClipboardItem::new_string(path.to_string()), cx);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: "gpui-session-chat-unresolved-file-path-copied".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("success")),
                title: "File path copied".to_string(),
                description: Some(description),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    /** Main-window toast for a chat file link that cannot be opened. */
    pub(crate) fn report_session_chat_file_open_failure(
        &mut self,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: "gpui-session-chat-file-open-failed".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("warning")),
                title: "Could not open the file".to_string(),
                description: Some(description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    /**
    Main-window toast naming where a clicked chat file link is going. The
    workarea switch takes the reader off the chat pane and Code/Docs can take a
    moment to show the file, so the toast is what tells them the click landed.
    */
    pub(crate) fn report_session_chat_file_opening(
        &mut self,
        destination: &str,
        file_path: &Path,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID.to_string(),
                level: GpuiAppToastLevel::from_raw(None),
                title: format!("Opening file in {destination}"),
                description: Some(file_path.to_string_lossy().into_owned()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: None,
                epoch: 0,
            },
            cx,
        );
    }

    /**
    `None` when the bundled code-server component can actually open a file right
    now, otherwise the reason to show the reader. This is the same availability
    read the Source workarea launches on, so a `None` here means the workarea
    switch ends on the file rather than on the component installer prompt.
    */
    pub(crate) fn embedded_code_editor_unavailable_reason(&self) -> Option<&'static str> {
        let Some(target) = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot))
        else {
            return Some("Code view is not available for this project.");
        };
        match source_code_server_runtime_availability(&target) {
            SourceCodeServerRuntimeAvailability::Available => None,
            SourceCodeServerRuntimeAvailability::InstallRequired => {
                Some("Code view is not installed on this machine.")
            }
            SourceCodeServerRuntimeAvailability::Failed(_) => {
                Some("Code view is unavailable on this machine.")
            }
        }
    }

    /// Hands the pending path to the native Files view.
    pub(crate) fn deliver_pending_docs_file_open(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(relative_path) = self.pending_docs_file_open.clone() else {
            return false;
        };
        self.pending_docs_file_open = None;
        self.native_docs_open_external(relative_path, cx);
        true
    }

    /**
    Docs may need a moment before it can take the request: the mode switch
    creates the surface, and CEF has no main frame to run the script in until
    the page commits. Surface reconciliation is event-driven, so poll briefly
    instead of waiting for the next unrelated event, and drop the request if
    Docs never comes up.
    */
    pub(crate) fn schedule_pending_docs_file_open_delivery(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            for _ in 0..PENDING_DOCS_FILE_OPEN_MAX_ATTEMPTS {
                cx.background_executor()
                    .timer(PENDING_DOCS_FILE_OPEN_RETRY_INTERVAL)
                    .await;
                match this.update(cx, |this, cx| this.deliver_pending_docs_file_open(cx)) {
                    Ok(false) => {}
                    // Delivered, or the app is gone.
                    _ => return,
                }
            }
            let _ = this.update(cx, |this, _| {
                this.pending_docs_file_open = None;
            });
        })
        .detach();
    }
}
