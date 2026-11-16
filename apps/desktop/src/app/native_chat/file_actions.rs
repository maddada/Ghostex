//! Copy Path and the folder row for every file the chat shows: file links, file edit cards,
//! pictures, attachments and the Files and links list. A right-click on any of them opens the same
//! menu a file link opens (the core's `reference_menu.rs` rows); file edit cards and the Files and
//! links rows also carry the two actions as buttons.
//!
//! CDXC:SessionChat 2026-10-17 DECISION:
//! User: "also we should have "show in folder" and copy path for all files in the chat view yea?". Every file the chat shows offers Copy Path and the folder row next to its other file actions. The folder row keeps the "Open File/Folder Location" wording of CDXC:OsIntegration 2026-09-16. It shows only where this app can reveal the file: a chat on this computer in the desktop app, not a remote session's file (it is on the other computer) and not Ghostex web. The phone gets Copy Path only, in its long-press menus (apps/mobile/app/src/chat/).

use super::state::NativeChatView;
use gpui::{Context, MouseDownEvent, Window};
use serde_json::{Value, json};

/// The path of a picture's file, when it has one on the session's computer: `copyPath`, or the
/// path it is read from. A pasted `data:` picture or a web address has none.
pub(super) fn image_file_path(image: &Value) -> Option<String> {
    let copy_path = image["copyPath"].as_str().unwrap_or_default();
    let path = if copy_path.is_empty() {
        if image["transport"] != "read" {
            return None;
        }
        image["path"].as_str().unwrap_or_default()
    } else {
        copy_path
    };
    let lowered = path.to_ascii_lowercase();
    (!path.is_empty()
        && !lowered.starts_with("http://")
        && !lowered.starts_with("https://")
        && !lowered.starts_with("data:"))
    .then(|| path.to_string())
}

impl NativeChatView {
    /// Whether this app can show a file of this chat in the system file manager.
    pub(super) fn file_locate_available(&self) -> bool {
        self.config.machine_id == "local" && !cfg!(ghostex_web)
    }

    /// Drops the folder row from a menu when [`Self::file_locate_available`] says it cannot work.
    pub(super) fn keep_available_file_rows(&self, rows: &mut Vec<Value>) {
        if !self.file_locate_available() {
            rows.retain(|row| row["command"]["action"] != "locateFile");
        }
        // CDXC:SessionChat 2026-11-09 WHY: Open in Browser loads the file's `file://` address in this app's Browser, which only finds a file of a chat on this computer; a remote session's file is on the other computer (GitHub #515).
        if self.config.machine_id != "local" {
            rows.retain(|row| row["command"]["action"] != "openInBrowser");
        }
    }

    /// A right press on something that shows a file: the file's menu, led by the transcript menu's
    /// selection rows on the main transcript, as a file link does (`markdown_links.rs`).
    pub(super) fn file_secondary_press(
        &mut self,
        path: String,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if path.is_empty() {
            return;
        }
        let shown = if self.in_main_transcript(event.position) {
            self.show_transcript_menu(Some(path), event.position, window, cx)
        } else {
            self.show_reference_menu(path, event.position, window, cx)
        };
        if shown {
            cx.stop_propagation();
        }
    }

    /// Copy Path's button: the same copy the menu row makes.
    pub(super) fn copy_file_path(&mut self, path: &str, cx: &mut Context<Self>) {
        crate::app::helpers::gpui_copy_to_clipboard(
            gpui::ClipboardItem::new_string(path.to_owned()),
            cx,
        );
    }

    /// The folder row's button: the host shows the file selected in the system file manager.
    pub(super) fn locate_file(&mut self, path: &str, cx: &mut Context<Self>) {
        self.host("locateFile", json!({ "path": path }), cx);
    }
}

/// Puts a file's right-click menu on an element that shows the file (a picture tile, an
/// attachment); nothing for an element with no file behind it.
pub(super) trait FileMenuExt: Sized {
    fn file_menu(self, path: Option<String>, cx: &Context<NativeChatView>) -> Self;
}

impl FileMenuExt for gpui::Stateful<gpui::Div> {
    fn file_menu(self, path: Option<String>, cx: &Context<NativeChatView>) -> Self {
        use gpui::InteractiveElement as _;
        match path {
            Some(path) => self.on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |chat, event: &MouseDownEvent, window, cx| {
                    chat.file_secondary_press(path.clone(), event, window, cx);
                }),
            ),
            None => self,
        }
    }
}
