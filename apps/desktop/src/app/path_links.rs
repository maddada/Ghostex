//! What a click on a path shown in Ghostex's own UI does: open it in the Files view.
//!
//! CDXC:PathLinks 2026-11-08 DECISION:
//! User: "pls whenever we have a file path like this make clicking on it open it in ghostex files".
//! A file opens in Files (inside the project as a tree path, outside it through the one-file grant
//! chat links use); a folder inside the project is revealed in Files' tree. Files never lists
//! folders outside the project (CDXC:Docs 2026-09-27 in native_docs/open_file.rs), so a folder
//! outside it opens in the system file manager instead, as does anything on a remote project.
//! `~` is the home folder. A path that is not there says "Not found" in a plain toast (no buttons).
//! The look and the click are `native_modal_kit/path_link.rs`; this is the host it calls.

use std::path::{Path, PathBuf};

use gpui::{App, Pixels, Point, Window};

use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::model::TitlebarMode;
use crate::app::window::native_modal_kit::{PathLinkHost, install_path_link_host};
use crate::app::workspace_windows::active_workspace_window;
use crate::*;

/// The label of the Reveal row, as the platform's file manager is called.
const REVEAL_LABEL: &str = if cfg!(target_os = "macos") {
    "Reveal in Finder"
} else if cfg!(target_os = "windows") {
    "Reveal in Explorer"
} else {
    "Show in file manager"
};

/// `path` with `~` and `~/` expanded to the home folder; `None` for a path that is not absolute
/// once expanded.
pub(crate) fn expand_path_link(path: &str) -> Option<PathBuf> {
    let path = path.trim();
    let expanded = if path == "~" || path.starts_with("~/") || path.starts_with("~\\") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        let rest = path[1..].trim_start_matches(['/', '\\']);
        if rest.is_empty() {
            PathBuf::from(home)
        } else {
            PathBuf::from(home).join(rest)
        }
    } else {
        PathBuf::from(path)
    };
    expanded.is_absolute().then_some(expanded)
}

impl GhostexGpuiApp {
    /// Opens a clicked path (`~/…` or absolute) in the Files view; see the module comment.
    pub(crate) fn open_path_link_in_files(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(expanded) = expand_path_link(path) else {
            return;
        };
        let Ok(resolved) = canonicalize_simplified(&expanded) else {
            self.dispatch_gpui_workspace_action_toast("error", "Not found", path, cx);
            return;
        };
        let is_directory = resolved.is_dir();
        let project = self.native_docs_project();
        let remote_project = project.as_ref().is_some_and(|project| {
            gpui_remote_project_reference_from_project_id(&project.project_id).is_some()
        });
        let files_available = !gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage)
            && self.titlebar_mode_available(TitlebarMode::Manage);
        let project_root = project.as_ref().map(|project| {
            canonicalize_simplified(&project.project_path)
                .unwrap_or_else(|_| project.project_path.clone())
        });
        let inside_project = project_root
            .as_ref()
            .is_some_and(|root| resolved.starts_with(root));
        let is_project_root = project_root.as_ref() == Some(&resolved);
        // Files lists only the project's own folders, and a remote project cannot read this
        // computer's files: those go to the system file manager.
        if !files_available || remote_project || (is_directory && !inside_project) {
            self.open_path_link_in_file_manager(&resolved, is_directory, cx);
            return;
        }
        window.activate_window();
        self.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
        self.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
        // The project's own folder is what Files shows already.
        if is_project_root {
            return;
        }
        // Files may not have drawn for this project yet: ask again until it has.
        cx.spawn(async move |this, cx| {
            for attempt in 0..PENDING_DOCS_FILE_OPEN_MAX_ATTEMPTS {
                if attempt > 0 {
                    cx.background_executor()
                        .timer(PENDING_DOCS_FILE_OPEN_RETRY_INTERVAL)
                        .await;
                }
                let resolved = resolved.clone();
                let opened = this.update(cx, |this, cx| {
                    let ready = this.native_docs_project().is_some()
                        && this.native_docs.project == this.native_docs_project();
                    if ready {
                        this.native_docs_open_absolute_path(&resolved, cx);
                    }
                    ready
                });
                match opened {
                    Ok(false) => {}
                    // Opened, or the app is gone.
                    _ => return,
                }
            }
        })
        .detach();
    }

    fn open_path_link_in_file_manager(
        &mut self,
        path: &Path,
        is_directory: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let result = if is_directory {
            gpui_open_path(path)
        } else {
            gpui_reveal_path_in_finder(path)
        };
        if let Err(message) = result {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't open that path",
                &message,
                cx,
            );
        }
    }

    /// Reveal in Finder/Explorer from the path menu.
    fn reveal_path_link(&mut self, path: &str, cx: &mut gpui::Context<Self>) {
        let Some(expanded) = expand_path_link(path) else {
            return;
        };
        let Ok(resolved) = canonicalize_simplified(&expanded) else {
            self.dispatch_gpui_workspace_action_toast("error", "Not found", path, cx);
            return;
        };
        let result = if resolved.is_dir() {
            gpui_open_path(&resolved)
        } else {
            gpui_reveal_path_in_finder(&resolved)
        };
        if let Err(message) = result {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't reveal that path",
                &message,
                cx,
            );
        }
    }
}

/// Runs `f` on the workspace window the click came from, deferred past the caller's update.
fn on_active_app(
    cx: &mut App,
    f: impl FnOnce(&mut GhostexGpuiApp, &mut Window, &mut gpui::Context<GhostexGpuiApp>) + 'static,
) {
    let Some((handle, app)) = active_workspace_window(cx) else {
        return;
    };
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, cx| {
            let _ = app.update(cx, |app, cx| f(app, window, cx));
        });
    });
}

/// Installs the host the modal kit's path links call, and the two actions of their menu.
pub(crate) fn install_path_links(cx: &mut App) {
    install_path_link_host(PathLinkHost {
        open: std::rc::Rc::new(|path, _window, cx| {
            on_active_app(cx, move |app, window, cx| {
                app.open_path_link_in_files(&path, window, cx)
            });
        }),
        menu: std::rc::Rc::new(
            |path: String, position: Point<Pixels>, window: &mut Window, cx: &mut App| {
                let Some((_, app)) = active_workspace_window(cx) else {
                    return;
                };
                let Some(app) = app.upgrade() else {
                    return;
                };
                GpuiContextMenu::new()
                    .menu(
                        "Copy path",
                        Box::new(CopyGpuiPathLink { path: path.clone() }),
                    )
                    .menu(REVEAL_LABEL, Box::new(RevealGpuiPathLink { path }))
                    .show_for_app(app, position, window, cx);
            },
        ),
    });
    cx.on_action(|action: &CopyGpuiPathLink, cx| {
        let text = expand_path_link(&action.path)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| action.path.clone());
        gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(text), cx);
    });
    cx.on_action(|action: &RevealGpuiPathLink, cx| {
        let path = action.path.clone();
        on_active_app(cx, move |app, _window, cx| app.reveal_path_link(&path, cx));
    });
}
