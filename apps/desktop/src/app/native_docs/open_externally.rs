//! Open Externally and Open With for a file in the Files view: gxserver hands out a private link
//! to the file (HTML as it is, Markdown rendered, an Excalidraw drawing as an editor that saves
//! back) and the browser opens it; Open With lists the browsers for that link and the apps for the
//! file itself.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{Bounds, Context, Pixels, Window};
use serde_json::{Value, json};

use super::entry::is_review_path;
use super::state::DocsFileKind;
use crate::GhostexGpuiApp;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

thread_local! {
    /// Where the header's Open With chevron is, so its menu opens below it.
    pub(crate) static OPEN_WITH_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

/// One app Open With offers.
pub(crate) struct OpenWithApp {
    pub(crate) name: String,
    path: PathBuf,
}

/// The apps Launch Services offers for a link (`is_url`) or a file extension, the default first.
#[cfg(target_os = "macos")]
pub(crate) fn open_with_applications(target: &str, is_url: bool) -> Vec<OpenWithApp> {
    use std::ffi::{CStr, CString, c_char};
    unsafe extern "C" {
        fn ghostex_open_with_applications(target: *const c_char, is_url: bool) -> *mut c_char;
        fn ghostex_open_with_free(lines: *mut c_char);
    }
    let Ok(target) = CString::new(target) else {
        return Vec::new();
    };
    // SAFETY: the shim returns a malloc'd NUL-terminated string (or null) that we free once.
    let lines = unsafe {
        let raw = ghostex_open_with_applications(target.as_ptr(), is_url);
        if raw.is_null() {
            return Vec::new();
        }
        let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
        ghostex_open_with_free(raw);
        text
    };
    lines
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, path)| OpenWithApp {
            name: name.to_string(),
            path: PathBuf::from(path),
        })
        .collect()
}

/// The apps the shell offers for a link scheme (`is_url`) or a file extension.
#[cfg(target_os = "windows")]
pub(crate) fn open_with_applications(target: &str, is_url: bool) -> Vec<OpenWithApp> {
    crate::app::helpers::windows_open_with_applications(target, is_url)
        .into_iter()
        .map(|(name, path)| OpenWithApp { name, path })
        .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn open_with_applications(_target: &str, _is_url: bool) -> Vec<OpenWithApp> {
    Vec::new()
}

impl DocsFileKind {
    /// HTML pages, drawings and Markdown have a browser page to open.
    pub(crate) fn opens_externally(self) -> bool {
        matches!(self, Self::Html | Self::Excalidraw | Self::Markdown)
    }
}

/// CDXC:Docs 2026-10-13 DECISION:
/// User chose (options 1A, 2A, 3A): HTML, Excalidraw and Markdown files pop out to the default browser through a gxserver link, so they work in a real browser and from remote projects; an Open With menu beside the header's pop-out button and in a file's right-click menu offers the other browsers for the link and the apps for the file; the view strip's Open Externally pops out the open file instead of staying disabled in Files. On 2026-10-13 the user added: "add file explorer here as top option please", so a local file's Open With menu starts with a row that shows the file selected in the system file manager, through the same reveal action as the entry menu; the user named it "Show in Folder" on every platform.
pub(crate) fn file_opens_externally(path: &str) -> bool {
    !is_review_path(path)
        && !super::published_page::is_page_path(path)
        && DocsFileKind::for_path(path).opens_externally()
}

impl GhostexGpuiApp {
    /// The open file, when Open Externally has a page for it.
    pub(crate) fn native_docs_external_file(&self) -> Option<String> {
        let document = self.native_docs.active_document()?;
        (document.kind.opens_externally() && !is_review_path(&document.path))
            .then(|| document.path.to_string())
    }

    fn native_docs_remote_target(
        &self,
    ) -> Option<(
        GpuiRemoteProjectReference,
        Option<GpuiRemoteGxserverRequestTarget>,
    )> {
        let reference = gpui_remote_project_reference_from_project_id(
            self.latest_sidebar_project_snapshot
                .as_ref()?
                .active_project_id
                .as_ref()?
                .0
                .as_str(),
        )?;
        let target = self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str());
        Some((reference, target))
    }

    /// Opens `path`'s link in the default browser, or in `browser`. Unsaved edits are saved first
    /// so the browser shows what the editor shows.
    pub(crate) fn native_docs_open_externally(
        &mut self,
        path: &str,
        browser: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let dirty = self
            .native_docs
            .document(path)
            .is_some_and(|document| document.dirty);
        if dirty {
            let path_owned = path.to_string();
            self.native_docs_save(path, cx, move |this, cx| {
                this.native_docs_open_file_link(path_owned, browser, cx)
            });
        } else {
            self.native_docs_open_file_link(path.to_string(), browser, cx);
        }
    }

    fn native_docs_open_file_link(
        &mut self,
        path: String,
        browser: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let request = self.native_docs_request("openLink", json!({ "path": path }));
        let additional_docs_folders =
            gpui_manage_additional_docs_folders_text(&self.sidebar_runtime_settings_snapshot);
        let remote = self.native_docs_remote_target();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let opened = background
                .spawn(async move {
                    let mut params = json!({
                        "action": "openLink",
                        "additionalDocsFolders": additional_docs_folders,
                        "path": request["path"],
                        "projectId": request["projectId"],
                        "requestId": request["requestId"],
                        "scope": "project",
                    });
                    let timeout = Duration::from_secs(15);
                    let (base, response) = match remote {
                        Some((reference, target)) => {
                            let target = target.ok_or_else(|| {
                                "Reconnect the remote machine to open its files.".to_string()
                            })?;
                            params["projectId"] = Value::String(reference.project_id.clone());
                            let response = gpui_remote_gxserver_rpc_result(
                                &target,
                                "/api/runProjectDocsAction",
                                &params,
                                timeout,
                            )?;
                            (format!("http://127.0.0.1:{}", target.local_port), response)
                        }
                        None => {
                            let response = gpui_gxserver_rpc_result(
                                "/api/runProjectDocsAction",
                                &params,
                                timeout,
                            )?;
                            (
                                format!(
                                    "http://{GPUI_GXSERVER_LOCAL_API_HOST}:{}",
                                    gpui_local_gxserver_api_port()
                                ),
                                response,
                            )
                        }
                    };
                    if let Some(error) = response["error"].as_str() {
                        return Err(error.to_string());
                    }
                    let link = response["linkPath"]
                        .as_str()
                        .filter(|link| link.starts_with('/'))
                        .ok_or_else(|| {
                            "This version of gxserver can't open files in a browser. Update Ghostex on that computer.".to_string()
                        })?;
                    let url = format!("{base}{link}");
                    match browser {
                        Some(browser) => {
                            gpui_open_with_application(&browser, std::ffi::OsStr::new(&url))
                        }
                        None => gpui_open_external_http_url(&url),
                    }
                })
                .await;
            if let Err(error) = opened {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't open the file",
                        &error,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// Opens the file itself with `app` (local projects only).
    pub(crate) fn native_docs_open_with_app(
        &mut self,
        path: &str,
        app: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let request = self.native_docs_request(
            "openWithSystemApp",
            json!({ "path": path, "appPath": app.to_string_lossy() }),
        );
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if let Some(error) = response["error"].as_str() {
                this.dispatch_gpui_workspace_action_toast(
                    "error",
                    "Couldn't open the file",
                    error,
                    cx,
                );
            }
        });
    }

    /// The Open With rows for `path`: the browsers for its link, then (for a file on this
    /// computer) the other apps registered for its extension.
    pub(crate) fn native_docs_open_with_menu(&self, path: &str) -> GpuiContextMenu {
        let command = |kind: &str, app: &PathBuf| {
            super::actions::action(json!({
                "type": kind,
                "path": path,
                "app": app.to_string_lossy(),
            }))
        };
        let mut menu = GpuiContextMenu::new();
        // The system file manager is the top row, for a file on this computer only.
        if self.native_docs_remote_target().is_none() {
            menu = menu.menu(
                "Show in Folder",
                super::actions::action(json!({ "type": "revealInFinder", "path": path })),
            );
            menu = menu.separator();
        }
        // Apps that take web links but cannot show a page (launchers, chat apps) are left out:
        // a browser is an app that also opens HTML files.
        let browsers = if file_opens_externally(path) {
            let html_apps = open_with_applications("html", false);
            open_with_applications("http://127.0.0.1/", true)
                .into_iter()
                .filter(|app| html_apps.iter().any(|html| html.path == app.path))
                .collect()
        } else {
            Vec::new()
        };
        for browser in &browsers {
            menu = menu.menu(
                browser.name.clone(),
                command("openExternally", &browser.path),
            );
        }
        let extension = path
            .rsplit('/')
            .next()
            .and_then(|name| name.rsplit_once('.'))
            .map(|(_, extension)| extension)
            .unwrap_or_default();
        if self.native_docs_remote_target().is_none() && !extension.is_empty() {
            menu = menu.separator();
            for app in open_with_applications(extension, false)
                .into_iter()
                .filter(|app| browsers.iter().all(|browser| browser.path != app.path))
            {
                menu = menu.menu(app.name, command("openWithApp", &app.path));
            }
            // Windows' own Open With dialog, for an app the list does not have.
            #[cfg(target_os = "windows")]
            {
                let chooser = PathBuf::from(crate::app::helpers::GPUI_OPEN_WITH_CHOOSER);
                menu = menu.menu("Choose another app…", command("openWithApp", &chooser));
            }
        }
        menu
    }

    /// The header chevron's menu.
    pub(crate) fn show_native_docs_open_with_menu(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu = self.native_docs_open_with_menu(path);
        let trigger = OPEN_WITH_MENU_ANCHOR.with(|cell| cell.get());
        self.native_docs_show_menu(menu, trigger, true, window, cx);
    }
}
