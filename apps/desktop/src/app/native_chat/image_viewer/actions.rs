//! What the image preview can do to its picture, shared by the toolbar buttons and the picture's
//! right-click menu.

use super::super::images::ChatImageSource;
use super::super::{state::NativeChatView, transcript::text};
use super::window::ImageViewerWindow;
use base64::Engine as _;
use gpui::{Context, Pixels, Point, Window};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

/// The app-bridge image transfer moves already-base64 bytes in ordered 256 KiB messages.
const SAVE_CHUNK_CHARS: usize = 256 * 1024;
/// Toggle key in `menu_toggle.rs`: a second right press on the picture only closes its menu.
const PICTURE_MENU_TRIGGER: &str = "chat-image-viewer-menu";

static SAVE_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl NativeChatView {
    fn image_viewer_current(&self) -> Option<Value> {
        self.image_viewer
            .request
            .as_ref()
            .map(|request| request.current().clone())
    }

    /// Runs one of the preview's actions by its menu command name.
    pub(in crate::app::native_chat) fn image_viewer_action(
        &mut self,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        match action {
            "copyImage" => self.copy_viewer_image(cx),
            "copyPath" => self.copy_viewer_path(cx),
            "locateFile" => self.open_viewer_file(true, cx),
            "saveImage" => self.save_viewer_image(cx),
            "resetZoom" => {
                if let Some(request) = &mut self.image_viewer.request {
                    request.zoom = 0;
                    cx.notify();
                }
            }
            "dismiss" => self.close_image_viewer(cx),
            _ => {}
        }
    }

    pub(super) fn copy_viewer_image(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.image_viewer_current() else {
            return;
        };
        // A picture without its bytes here (still loading, a web address GPUI fetches itself, or
        // one that could not be read) used to make Copy image do nothing at all.
        match self.chat_image(&image, cx) {
            ChatImageSource::Bytes(bytes) => {
                if crate::app::helpers::gpui_copy_picture_to_clipboard(&bytes, cx) {
                    self.note_image_viewer_action("Image copied", cx);
                }
            }
            ChatImageSource::Loading => {
                crate::app::helpers::gpui_copy_failed_feedback("The image is still loading", cx)
            }
            ChatImageSource::Uri(_) | ChatImageSource::Unavailable(_) => {
                crate::app::helpers::gpui_copy_failed_feedback("Couldn't copy this image", cx)
            }
        }
    }

    pub(super) fn copy_viewer_path(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .image_viewer_current()
            .map(|image| text(&image, "copyPath"))
            .filter(|path| !path.is_empty())
        else {
            return;
        };
        crate::app::helpers::gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(path), cx);
        self.note_image_viewer_action("Path copied", cx);
    }

    /// Opens the file behind a picture the preview could not show, or its folder, through the
    /// host's own file routes, which know the session's machine, then closes the preview.
    pub(super) fn open_viewer_file(&mut self, locate: bool, cx: &mut Context<Self>) {
        let Some(path) = self
            .image_viewer_current()
            .map(|image| text(&image, "path"))
            .filter(|path| !path.is_empty())
        else {
            return;
        };
        // A picture an agent linked at its own localhost has an address, not a file: it opens
        // like a link, which the host routes to the session's machine.
        if path.starts_with("http://") || path.starts_with("https://") {
            self.invoke(
                json!({"type": "openMarkdownLink", "href": path, "external": false}),
                cx,
            );
        } else {
            self.host(
                if locate { "locateFile" } else { "openFile" },
                json!({ "path": path }),
                cx,
            );
        }
        self.close_image_viewer(cx);
    }

    /// Hands the bytes to the host's Downloads writer, the route React's Save image also takes.
    pub(super) fn save_viewer_image(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.image_viewer_current() else {
            return;
        };
        let ChatImageSource::Bytes(bytes) = self.chat_image(&image, cx) else {
            return;
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes.bytes);
        let request_id = format!(
            "native-image-{}",
            SAVE_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let name = text(&image, "fileName");
        self.host(
            "saveImageStart",
            json!({
                "requestId": request_id,
                "suggestedName": if name.is_empty() { "image.png".to_string() } else { name },
            }),
            cx,
        );
        for (index, chunk) in encoded.as_bytes().chunks(SAVE_CHUNK_CHARS).enumerate() {
            self.host(
                "saveImageChunk",
                json!({
                    "requestId": request_id,
                    "chunkIndex": index,
                    "base64Chunk": String::from_utf8_lossy(chunk),
                }),
                cx,
            );
        }
        self.host("saveImageFinish", json!({ "requestId": request_id }), cx);
        self.note_image_viewer_action("Saving image", cx);
    }
}

impl ImageViewerWindow {
    /**
     * The picture's right-click menu.
     *
     * CDXC:SessionChat 2026-09-28 DECISION:
     * User: right-clicking the previewed image no longer closes it; it opens a menu with the
     * toolbar's actions, Reset Zoom while the picture is zoomed in, and Dismiss, grouped with
     * separators. Copying comes first, then saving, then the view, and Dismiss last. A right-click
     * on the surround still closes the preview. This supersedes the 2026-09-19 decision that a
     * right-click anywhere in the preview closed it.
     */
    pub(super) fn show_picture_menu(
        &mut self,
        at: Point<Pixels>,
        copyable: bool,
        zoomed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = |action: &str| json!({"type":"imageViewer","action":action});
        let mut rows = vec![
            json!({"label":"Copy Image","iconPath":"titlebar/copy.svg","command":command("copyImage")}),
        ];
        if copyable {
            rows.push(json!({"label":"Copy Path","iconPath":"titlebar/link.svg","command":command("copyPath")}));
        }
        // The folder row of every file the chat shows (CDXC:SessionChat 2026-10-17 in file_actions.rs).
        let locatable = self.chat.read(cx).file_locate_available()
            && self
                .chat
                .read(cx)
                .image_viewer_current()
                .and_then(|image| super::super::file_actions::image_file_path(&image))
                .is_some();
        if locatable {
            rows.push(json!({"label":"Open Image Location","iconPath":"titlebar/folder-open.svg","command":command("locateFile")}));
        }
        rows.push(json!({"separator":true}));
        rows.push(json!({"label":"Save Image","iconPath":"titlebar/download.svg","command":command("saveImage")}));
        if zoomed {
            rows.push(json!({"separator":true}));
            rows.push(json!({"label":"Reset Zoom","iconPath":"titlebar/zoom-reset.svg","command":command("resetZoom")}));
        }
        rows.push(json!({"separator":true}));
        rows.push(json!({"label":"Dismiss","iconPath":"titlebar/x.svg","detail":"Esc","command":command("dismiss")}));
        self.chat.update(cx, |chat, cx| {
            if !chat.chat_menu_toggled_shut(PICTURE_MENU_TRIGGER, cx) {
                chat.show_chat_menu_in_window_at(rows, at, 220.0, window, cx);
            }
        });
    }
}
