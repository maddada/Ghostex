//! A path shown in a dialog, card or notice of the page: what a click and a right-click do.
//!
//! CDXC:PathLinks 2026-11-08 DECISION:
//! User: "pls whenever we have a file path like this make clicking on it open it in ghostex files".
//! The page has no Files view and cannot reach the computer's folders, so a click answers with the
//! same toast every file open here gives; a right-click copies the path (the page's Copy path).
//! The desktop's host is `app/path_links.rs`; the look is `native_modal_kit/path_link.rs`.

use std::rc::Rc;

use gpui::WeakEntity;

use crate::GhostexGpuiApp;
use crate::app::window::native_modal_kit::{PathLinkHost, install_path_link_host};

/// Installs the host the modal kit's path links call, answering through `shell`'s toasts.
pub(crate) fn install_path_links(shell: WeakEntity<GhostexGpuiApp>) {
    let open_shell = shell.clone();
    install_path_link_host(PathLinkHost {
        open: Rc::new(move |path, _window, cx| {
            let _ = open_shell.update(cx, |app, cx| {
                app.dispatch_gpui_app_modal_toast(
                    "info",
                    "Open the desktop app to open files",
                    &path,
                    cx,
                );
            });
        }),
        menu: Rc::new(move |path, _position, _window, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
            let _ = shell.update(cx, |app, cx| {
                app.dispatch_gpui_app_modal_toast("success", "Path copied", &path, cx);
            });
        }),
    });
}
