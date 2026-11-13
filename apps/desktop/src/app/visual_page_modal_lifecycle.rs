//! Where a chat page card opens its page (Settings > Chat > Open Ghostex pages in), and the open and
//! close plumbing for the floating window, the default.
//! SEE-ALSO: apps/desktop/src/app/window/visual_page_modal.rs (the window entity), apps/desktop/src/app/native_docs/published_page.rs (the page in the Files view), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/cef_deferred_startup.rs (an open that waits for the web runtime).
use crate::app::helpers::web_runtime::web_runtime_available;
use crate::app::helpers::*;
use crate::app::visual_page_hidden::same_page;
use crate::app::window::*;
use crate::web_bridge_types::AppModalHostBridgeEvent;
use crate::*;

impl GhostexGpuiApp {
    /// A chat page card's Open for the session `session_id`, where Settings > Chat > Open chat
    /// pages in says (`ChatPageOpenTarget`). A page a remote machine's session published lives on
    /// that machine's gxserver, so its address is resolved for the machine
    /// (`gpui_remote_machine_url`) first; the open message then carries the machine
    /// (`remoteMachineId`, `remoteProjectId`) for the window's picture and its links.
    pub(crate) fn open_session_chat_visual_page(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = ChatPageOpenTarget::from_settings();
        if target == ChatPageOpenTarget::Popup {
            visual_page_modal::open_timing::start(
                cef::context_initialized(),
                message["url"].as_str().unwrap_or_default(),
                message["title"].as_str().unwrap_or_default(),
            );
        }
        let Some(remote_project) = self
            .remote_project_for_terminal_target(GpuiEngineTerminalEventTarget::Agents(session_id))
        else {
            self.open_resolved_visual_page(target, session_id, message, window, cx);
            return;
        };
        let url = message["url"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        let Some(request_target) =
            self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
        else {
            let machine = gpui_remote_machine_name_from_settings(&remote_project.remote_machine_id)
                .unwrap_or_else(|| "the remote computer".to_string());
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "This page is on another computer",
                &format!("Connect {machine} to open it."),
                cx,
            );
            return;
        };
        match gpui_remote_machine_url(&url, &request_target) {
            GpuiRemoteMachineUrl::Gxserver(url) => {
                let mut message = message.clone();
                message["url"] = url.into();
                message["remoteMachineId"] = remote_project.remote_machine_id.into();
                message["remoteProjectId"] = remote_project.project_id.into();
                self.open_resolved_visual_page(target, session_id, &message, window, cx);
            }
            GpuiRemoteMachineUrl::MachineLoopback => {
                self.open_remote_machine_loopback_link(&url, remote_project, cx);
            }
            GpuiRemoteMachineUrl::AsWritten => {
                self.open_resolved_visual_page(target, session_id, message, window, cx)
            }
        }
    }

    /// Opens a page card's page, its address already resolved for this computer, in `target`.
    ///
    /// CDXC:SessionChat 2026-10-26 DECISION:
    /// User: "Add a setting with 3 options for opening inline htmls (popup, embedded files, system browser)" (2026-10-24), then "My setting wins for every page", after "i have pop up selected on windows but inline html pages keep opening in the browser in the side panel". Settings > Chat > Open Ghostex pages in (`chatPageOpenTarget`) sends every page card's click on the desktop, and a plain link to a published page, to the floating page window (Popup, the default), a page document in the Files view (Files, native_docs/published_page.rs) or the computer's default browser (System browser). This supersedes the 2026-10-24 rule that a card the agent marked for the browser (`ghostex show --browser`, no `"open": "popup"`) kept opening there: the block's `"open"` no longer decides anything on the desktop. Every mode reacts to the click at once, and a page's answers come back to the chat that published it in all three, because the page itself posts them to gxserver. The phone keeps its own page viewer.
    fn open_resolved_visual_page(
        &mut self,
        target: ChatPageOpenTarget,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let remote_project =
            text("remoteMachineId").map(|remote_machine_id| GpuiRemoteProjectReference {
                remote_machine_id,
                project_id: text("remoteProjectId").unwrap_or_default(),
            });
        match target {
            ChatPageOpenTarget::Popup => {
                // The chat whose header shows the page's tab while it is hidden.
                let mut message = message.clone();
                message["sourceSessionId"] = session_id.0.to_string().into();
                self.receive_app_modal_host_bridge_event(
                    AppModalHostBridgeEvent::Message(message.to_string()),
                    window,
                    cx,
                )
            }
            ChatPageOpenTarget::SystemBrowser => {
                let light = self.gpui_native_modal_palette().light;
                let opened = text("url")
                    .and_then(|url| visual_page_reader_url(&url, light))
                    .ok_or_else(|| "The page's address is not valid.".to_string())
                    .and_then(|url| gpui_open_external_http_url(&url));
                if let Err(error) = opened {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "The page could not be opened in your browser",
                        &error,
                        cx,
                    );
                }
            }
            ChatPageOpenTarget::Files => {
                let Some(url) = text("url")
                    .filter(|url| crate::app::window::visual_page_modal::page_id(url).is_some())
                else {
                    // Only a page gxserver serves has an id to open in Files.
                    return self.open_resolved_visual_page(
                        ChatPageOpenTarget::Popup,
                        session_id,
                        message,
                        window,
                        cx,
                    );
                };
                if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage)
                    || !self.titlebar_mode_available(TitlebarMode::Manage)
                {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "The Files view is turned off",
                        "Turn the Files view on, or pick another place under Settings > Chat > Open Ghostex pages in.",
                        cx,
                    );
                    return;
                }
                self.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
                self.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
                self.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
                self.native_docs_open_page(
                    crate::app::native_docs::published_page::DocsPendingPage {
                        source: crate::app::native_docs::published_page::DocsPageSource {
                            url,
                            remote_project,
                        },
                        title: text("title").unwrap_or_else(|| "Ghostex page".to_string()),
                        origin: Some(session_id),
                    },
                    cx,
                );
                if !cef::context_initialized() && web_runtime_available() {
                    self.request_cef_runtime(cx);
                }
            }
        }
    }

    /// A link a published page opened, shown outside the floating window (the Files view): it
    /// opens where a chat link opens (`open_visual_page_web_link`), unless it names the page's
    /// machine's own localhost, which only the Browser on that machine's network reaches.
    pub(crate) fn open_visual_page_link(
        &mut self,
        url: String,
        remote_project: Option<GpuiRemoteProjectReference>,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = remote_project.as_ref().and_then(|remote_project| {
            self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
        });
        let url = match (remote_project, target) {
            (Some(remote_project), Some(target)) => match gpui_remote_machine_url(&url, &target) {
                GpuiRemoteMachineUrl::Gxserver(url) => url,
                GpuiRemoteMachineUrl::MachineLoopback => {
                    self.open_remote_machine_loopback_link(&url, remote_project, cx);
                    return;
                }
                GpuiRemoteMachineUrl::AsWritten => url,
            },
            (Some(_), None) if gpui_remote_link_is_loopback(&url) => return,
            _ => url,
        };
        self.open_visual_page_web_link(url, cx);
    }

    /// CDXC:SessionChat 2026-11-10 DECISION: User: "links inside the inline html need to open it inside the app's browser if i pick that in settings for opening links please". A web link clicked in a Ghostex page (the floating window or the Files view) takes the chat link's path (`open_session_chat_link`), so "Open links in embedded browser" decides between the app's Browser and the system browser, as it does for a link in the transcript. The page's own anchors and other schemes never get here: the page bootstrap (server/src/visual_pages/theme.rs `PAGE_BOOTSTRAP`) only sends other http(s) documents out as new windows.
    ///
    /// CDXC:SessionChat 2026-11-11 DECISION: a link that opens in the app's browser puts the shown page aside (app/visual_page_hidden.rs), so the link's page is not under it; one that leaves for the system browser leaves the page as it is.
    fn open_visual_page_web_link(&mut self, url: String, cx: &mut gpui::Context<Self>) {
        if shared_settings::shared_sidebar_settings_snapshot().web_links_open_in_app() {
            self.hide_visual_page_modal(cx);
        }
        let app = cx.weak_entity();
        // The link path needs the main window, which this command does not carry.
        cx.defer(move |cx| {
            let _ = app.update_in(cx, |app, window, cx| {
                app.open_session_chat_link(&url, None, false, false, window, cx);
            });
        });
    }

    /// Opens the floating window for the `visualPage` open message the chat's page card sends
    /// (`url`, `title`, and the published file's name as `file`). The window opens at once; its
    /// page needs the web runtime, so when the runtime is installed but not running yet it is
    /// started and the page created once it is up (`sync_visual_page_modal_with_web_runtime`).
    /// When it is not installed the open is held behind the install prompt instead.
    pub(crate) fn open_gpui_visual_page_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(url) = text("url").filter(|url| {
            gpui::http_client::Url::parse(url)
                .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
        }) else {
            return;
        };
        let web_runtime_ready = cef::context_initialized();
        if !web_runtime_ready && !web_runtime_available() {
            visual_page_modal::open_timing::finish("webRuntimeNotInstalled");
            self.defer_gpui_app_modal_open_for_cef(
                GpuiAppModalKind::VisualPage,
                message.clone(),
                cx,
            );
            return;
        }
        // The same page put aside comes back as it was instead of opening a second time.
        if let Some(key) = self.hidden_visual_page_showing(&url, cx) {
            visual_page_modal::open_timing::finish("shownAgain");
            self.show_hidden_visual_page(key, cx);
            return;
        }
        // The page shown now is opened again in a new window, as before, not put aside twice.
        if self
            .shown_visual_page_url(cx)
            .is_some_and(|shown| same_page(&shown, &url))
        {
            self.remove_native_app_modal_window(cx);
        }
        // A plain link to a page's address brings no title; the window takes the page's own.
        let title = text("title");
        let source_session = text("sourceSessionId")
            .and_then(|id| id.parse::<u64>().ok())
            .map(TerminalSessionId);
        let key = next_visual_page_key();
        let remote_project =
            text("remoteMachineId").map(|remote_machine_id| GpuiRemoteProjectReference {
                remote_machine_id,
                project_id: text("remoteProjectId").unwrap_or_default(),
            });
        let remote = remote_project.as_ref().and_then(|remote_project| {
            self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
        });
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_visual_page_modal_command(command, key, remote_project.clone(), cx);
        });
        let kind = GpuiAppModalKind::VisualPage;
        let (width, height) = self.chat_preview_modal_size(cx);
        let open_message = message.clone();
        self.open_native_app_modal(
            kind,
            width,
            height,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiVisualPageModalWindow::new(
                        url,
                        title,
                        open_message,
                        palette,
                        host,
                        key,
                        source_session,
                        remote,
                        web_runtime_ready,
                        window,
                        cx,
                    )
                })
            },
            cx,
        );
        visual_page_modal::open_timing::mark("windowOpened");
        if !web_runtime_ready {
            self.request_cef_runtime(cx);
        }
    }

    /// The web runtime changed (`refresh_web_runtime_views`): a page window opened before it was
    /// up gets its page now, or says why it failed. A runtime that turned out not to be installed
    /// closes the window for the install prompt, which opens the page once it is installed.
    pub(crate) fn sync_visual_page_modal_with_web_runtime(&mut self, cx: &mut gpui::Context<Self>) {
        let reopen = self
            .update_native_app_modal(
                GpuiAppModalKind::VisualPage,
                cx,
                |modal: &mut GpuiVisualPageModalWindow, window, cx| {
                    modal.web_runtime_changed(window, cx)
                },
            )
            .flatten();
        if let Some(message) = reopen {
            self.close_native_app_modal_from_bridge(cx);
            self.defer_gpui_app_modal_open_for_cef(GpuiAppModalKind::VisualPage, message, cx);
        }
    }

    /// `remote_project` is the machine whose session published the open page, `None` for this
    /// computer's.
    /// `key` is the page window that sent it (`GpuiVisualPageModalWindow::key`): a hidden page's
    /// commands only repaint its pill.
    fn handle_gpui_visual_page_modal_command(
        &mut self,
        command: VisualPageModalCommand,
        key: u64,
        remote_project: Option<GpuiRemoteProjectReference>,
        cx: &mut gpui::Context<Self>,
    ) {
        if matches!(command, VisualPageModalCommand::TabChanged) {
            self.notify_minimized_page_chats(cx);
            return;
        }
        if self.shown_visual_page_key(cx) != Some(key) {
            return;
        }
        let url = match command {
            VisualPageModalCommand::TabChanged => return,
            VisualPageModalCommand::Hide => {
                self.hide_visual_page_modal(cx);
                return;
            }
            VisualPageModalCommand::Close => {
                self.close_native_app_modal_from_bridge(cx);
                return;
            }
            VisualPageModalCommand::OpenInBrowser { url } => {
                self.close_native_app_modal_from_bridge(cx);
                let _ = gpui_open_external_http_url(&url);
                return;
            }
            // The browser downloads the exported page; the page stays open.
            VisualPageModalCommand::ExportPage { url } => {
                self.export_visual_page_to_file(url, cx);
                return;
            }
            VisualPageModalCommand::Notice {
                level,
                title,
                message,
            } => {
                self.dispatch_gpui_app_modal_toast(level, &title, &message, cx);
                return;
            }
            VisualPageModalCommand::RefreshFailed { message } => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "The page could not be refreshed",
                    &message,
                    cx,
                );
                return;
            }
            // A link the page opened opens where a chat link opens (`open_visual_page_web_link`)
            // and leaves the page open, unless it names the page's machine's own localhost, which
            // only the Browser on that machine's network reaches.
            VisualPageModalCommand::OpenLink { url } => {
                let target = remote_project.as_ref().and_then(|remote_project| {
                    self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
                });
                match (remote_project, target) {
                    (Some(remote_project), Some(target)) => {
                        match gpui_remote_machine_url(&url, &target) {
                            GpuiRemoteMachineUrl::Gxserver(url) => url,
                            GpuiRemoteMachineUrl::MachineLoopback => {
                                // It opens in the app's Browser, so the page steps aside.
                                self.hide_visual_page_modal(cx);
                                self.open_remote_machine_loopback_link(&url, remote_project, cx);
                                return;
                            }
                            GpuiRemoteMachineUrl::AsWritten => url,
                        }
                    }
                    (Some(_), None) if gpui_remote_link_is_loopback(&url) => return,
                    _ => url,
                }
            }
        };
        self.open_visual_page_web_link(url, cx);
    }
}

impl GhostexGpuiApp {
    /// A page card's Export: the page's address as the session's computer wrote it, made one this
    /// computer can reach (a remote computer's goes through its tunnel, as Open does), then
    /// exported (`export_visual_page_to_file`).
    pub(crate) fn export_session_chat_visual_page(
        &mut self,
        session_id: TerminalSessionId,
        url: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let url = url.trim().to_string();
        let remote_project = self
            .remote_project_for_terminal_target(GpuiEngineTerminalEventTarget::Agents(session_id));
        let url = match remote_project.and_then(|remote_project| {
            self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
        }) {
            Some(target) => match gpui_remote_machine_url(&url, &target) {
                GpuiRemoteMachineUrl::Gxserver(url) => url,
                _ => url,
            },
            None => url,
        };
        self.export_visual_page_to_file(url, cx);
    }

    /// Export page: this computer's gxserver builds the exported file (`/api/exportVisualPage`
    /// `{url}`, server/src/visual_pages/exported.rs), from its own copy of the page or from the page
    /// as its computer serves it, so a page on a computer with an older Ghostex exports too; the
    /// file is saved where the reader picks (Downloads first), and a toast says where.
    ///
    /// CDXC:SessionChat 2026-11-13 WHY:
    /// Export used to hand the page's `/export` address to the system browser. For a page on another computer that is a tunnel port on this machine (`http://127.0.0.1:50423/…`), which the other computer's 11.5.0 gxserver answered with a 404, and a tunnel address has no business in a browser anyway.
    pub(crate) fn export_visual_page_to_file(&mut self, url: String, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let exported = crate::app::gx_store::gx_rpc(
                None,
                "/api/exportVisualPage",
                serde_json::json!({ "url": url }),
            )
            .await;
            let exported = match exported {
                Ok(exported) => exported,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "The page could not be exported",
                            &error.message,
                            cx,
                        )
                    });
                    return;
                }
            };
            let html = exported["html"].as_str().unwrap_or_default().to_string();
            let name = exported["fileName"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or("ghostex-page.html")
                .to_string();
            let folder = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(std::path::PathBuf::from)
                .map(|home| {
                    let downloads = home.join("Downloads");
                    if downloads.is_dir() { downloads } else { home }
                })
                .unwrap_or_else(std::env::temp_dir);
            let picked = cx.update(|cx| cx.prompt_for_new_path(&folder, Some(name.as_str())));
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let written = std::fs::write(&path, html);
            let _ = this.update(cx, |this, cx| match written {
                Ok(()) => this.dispatch_gpui_app_modal_toast(
                    "success",
                    "Page exported",
                    &format!("Saved to {}", path.display()),
                    cx,
                ),
                Err(error) => this.dispatch_gpui_app_modal_toast(
                    "warning",
                    "The page could not be saved",
                    &error.to_string(),
                    cx,
                ),
            });
        })
        .detach();
    }
}

/// A new page window's key (`GpuiVisualPageModalWindow::key`).
fn next_visual_page_key() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Where a chat page card opens its page: Settings > Chat > Open Ghostex pages in
/// (`chatPageOpenTarget`, the options in packages/settings-catalog `CHAT_PAGE_OPEN_TARGET_OPTIONS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatPageOpenTarget {
    /// The floating page window over the chat.
    Popup,
    /// A page document in the Files view.
    Files,
    /// The computer's default browser.
    SystemBrowser,
}

impl ChatPageOpenTarget {
    fn from_settings() -> Self {
        match shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("chatPageOpenTarget")
            .and_then(serde_json::Value::as_str)
        {
            Some("files") => Self::Files,
            Some("system-browser") => Self::SystemBrowser,
            _ => Self::Popup,
        }
    }
}

/// A page's address for a reader that loads it by itself (the Files view, the reader's browser):
/// asking gxserver for the floating window's page script (`?gx-page-script=1`,
/// server/src/visual_pages/serve.rs) and carrying the app's theme (`#gx-theme=`), which the page
/// reads before its first paint as it does in the floating window.
pub(crate) fn visual_page_reader_url(url: &str, light: bool) -> Option<String> {
    let mut url = gpui::http_client::Url::parse(url.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    if !url
        .query_pairs()
        .any(|(key, value)| key == "gx-page-script" && value == "1")
    {
        url.query_pairs_mut().append_pair("gx-page-script", "1");
    }
    url.set_fragment(Some(if light {
        "gx-theme=light"
    } else {
        "gx-theme=dark"
    }));
    Some(url.into())
}
