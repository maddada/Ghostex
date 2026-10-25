//! How the rest of the app opens something in Docs: a file from a chat or terminal link, and an
//! agent reply to annotate (Reply by Annotating).

use gpui::Context;

use super::state::{DocsDocument, DocsDocumentLoad, DocsFileKind, DocsMediaKind};
use crate::GhostexGpuiApp;
use crate::shared_settings::{
    self, SharedDocumentFileKind, SharedFileOpenTarget, SharedMediaFileOpenTarget,
};

/// Where a file opens when clicked in chat or a terminal or picked in Quick Access, by its kind
/// and Settings > File opening: HTML, Markdown and pictures follow `documentFileOpenTarget`, video
/// and audio their own settings, Excalidraw drawings always Files, pictures and media Files cannot
/// show the system app, and everything else (PDFs, fonts and SQLite databases included)
/// `otherFileOpenTarget`.
///
/// CDXC:Navigation 2026-10-17 DECISION:
/// User: a picked file "should always open that file in the files app not in code (unless the user picks open files in "Code" (setting)". This supersedes the rule that sent every file other than Markdown, HTML, Excalidraw and media to Code; the dropdowns themselves are in packages/settings-catalog/src/file_opening.rs.
/// CDXC:Docs 2026-09-27 SEE-ALSO: video and audio keep `videoFileOpenTarget` / `audioFileOpenTarget`; terminal links (`gpui_terminal_file_opens_with_os_default`), chat file links (`open_session_chat_file_for_session`) and Quick Access (`open_quick_access_project_file`) all ask here.
pub(crate) fn file_open_target_for_path(path: &std::path::Path) -> SharedFileOpenTarget {
    let path = path.to_string_lossy();
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let files_cannot_show = DocsFileKind::for_path(&path) == DocsFileKind::SystemApp;
    match DocsMediaKind::for_path(&path) {
        Some(DocsMediaKind::Image) => {
            let target = settings.document_file_open_target(SharedDocumentFileKind::Image);
            if target == SharedFileOpenTarget::Files && files_cannot_show {
                SharedFileOpenTarget::SystemApp
            } else {
                target
            }
        }
        Some(media @ (DocsMediaKind::Video | DocsMediaKind::Audio)) => {
            let setting = if media == DocsMediaKind::Video {
                settings.video_file_open_target()
            } else {
                settings.audio_file_open_target()
            };
            if files_cannot_show || setting == SharedMediaFileOpenTarget::SystemApp {
                SharedFileOpenTarget::SystemApp
            } else {
                SharedFileOpenTarget::Files
            }
        }
        None => match super::state::file_extension(&path).as_str() {
            "md" | "markdown" | "mdown" | "mkdn" | "mdx" => {
                settings.document_file_open_target(SharedDocumentFileKind::Markdown)
            }
            "html" | "htm" => settings.document_file_open_target(SharedDocumentFileKind::Html),
            "excalidraw" => SharedFileOpenTarget::Files,
            // CDXC:Docs 2026-10-17 DECISION:
            // User (Q224): notebooks get the "code view": an `.ipynb` opens in the Code view, which shows it as cells, wherever other files would open in Files; an editor app picked under Settings > File opening keeps it. A remote notebook takes the remote Code route the same way, and when the Code view is missing the open falls back to Files.
            "ipynb" => match settings.other_file_open_target() {
                SharedFileOpenTarget::Files => SharedFileOpenTarget::Code,
                target => target,
            },
            _ if files_cannot_show => SharedFileOpenTarget::SystemApp,
            _ => settings.other_file_open_target(),
        },
    }
}

/// [`file_open_target_for_path`] for a file on this computer, which can also look inside it: a
/// file that would open in the Files view but is not text Files can show (it has NUL bytes, or is
/// not UTF-8) opens in the system app instead of Files' "Binary files are not previewed" notice.
///
/// CDXC:Navigation 2026-10-17 DECISION:
/// User (Q218): "Yes, binary files open in the system app": a binary file picked in Quick Access's Files tab, or opened from a chat or terminal link, that the Files view can't show opens in the system app. A remote project's file keeps the Files notice, because this computer's apps cannot reach it, so the remote route never asks here. Since 2026-10-25 a chat or terminal link to a runnable file asks first (session_chat/runnable_links.rs).
pub(crate) fn local_file_open_target(path: &std::path::Path) -> SharedFileOpenTarget {
    let target = file_open_target_for_path(path);
    let kind = DocsFileKind::for_path(&path.to_string_lossy());
    if target == SharedFileOpenTarget::Files
        && ((kind == DocsFileKind::Text && !looks_like_text(path))
            // A `.db` that is not SQLite (a Berkeley DB, `Thumbs.db`) is another app's file.
            || (kind == DocsFileKind::Sqlite && !looks_like_sqlite(path)))
    {
        return SharedFileOpenTarget::SystemApp;
    }
    target
}

/// The Files view's own test for text (`manage_docs/file_operations.rs` refuses NUL bytes and
/// invalid UTF-8), on the first 64 KB. A file that cannot be read counts as text, so Files shows
/// its own error.
fn looks_like_text(path: &std::path::Path) -> bool {
    use std::io::Read as _;
    let Ok(file) = std::fs::File::open(path) else {
        return true;
    };
    let mut sample = Vec::with_capacity(64 * 1024);
    if file.take(64 * 1024).read_to_end(&mut sample).is_err() {
        return true;
    }
    if sample.contains(&0) {
        return false;
    }
    match std::str::from_utf8(&sample) {
        Ok(_) => true,
        // A character cut off by the 64 KB sample is still text.
        Err(error) => error.error_len().is_none(),
    }
}

/// The file starts with SQLite's header. A file that cannot be read counts as SQLite, so Files
/// shows its own error.
fn looks_like_sqlite(path: &std::path::Path) -> bool {
    use std::io::Read as _;
    let mut header = [0u8; 16];
    match std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut header)) {
        Ok(()) => &header == b"SQLite format 3\0",
        Err(error) => error.kind() != std::io::ErrorKind::UnexpectedEof,
    }
}

/// Where review documents live; never read from or written to disk.
pub(crate) const REVIEW_DOCUMENT_ROOT: &str = ".ghostex-review";

pub(crate) fn is_review_path(path: &str) -> bool {
    path.starts_with(&format!("{REVIEW_DOCUMENT_ROOT}/"))
}

impl GhostexGpuiApp {
    /// Opens `path` (a Docs routing path) and opens every folder above it in the tree. Waits for
    /// the project's files when Docs has not synced to the current project yet.
    pub(crate) fn native_docs_open_external(&mut self, path: String, cx: &mut Context<Self>) {
        let current = self.native_docs_project();
        if current.is_none() || self.native_docs.project != current {
            self.native_docs.pending_open = Some(path);
            self.native_docs_notify(cx);
            return;
        }
        let origin = self.native_docs.pending_origin.take();
        self.native_docs_reveal_in_tree(&path, cx);
        let display_path = self.native_docs_display_path(&path);
        self.native_docs_open(&path, &display_path, cx);
        // Opened again from another agent's chat: that agent becomes the target.
        if let Some(origin) = origin
            && let Some(document) = self.native_docs.document_mut(&path)
        {
            document.origin_session = Some(origin);
        }
    }

    /// Opens an agent reply as a review document: Markdown with no file behind it, never saved
    /// and never unsaved, whose notes live only in memory.
    ///
    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: an agent reply can be annotated like a document. The chat's Annotate action hands the reply's markdown to Docs, which opens it as a review document with no file behind it; its notes live only in memory and feedback goes back to the same session.
    pub(crate) fn native_docs_open_review(&mut self, payload: &str, cx: &mut Context<Self>) {
        let Ok(payload) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        let content = payload["content"].as_str().unwrap_or_default().to_string();
        if content.trim().is_empty() {
            return;
        }
        let id = payload["id"]
            .as_str()
            .filter(|id| {
                !id.is_empty()
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            })
            .map(str::to_string)
            .unwrap_or_else(|| format!("reply-{}", super::annotations::now_ms()));
        let title = payload["title"]
            .as_str()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Agent reply")
            .to_string();
        let path = format!("{REVIEW_DOCUMENT_ROOT}/{id}.md");
        self.native_docs_drop_reviews(cx);
        let mut document = DocsDocument::new(path.clone(), title);
        document.kind = DocsFileKind::Markdown;
        document.origin_session = self.native_docs.pending_origin.take();
        document.review_session_title = Some(
            payload["sessionTitle"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        document.saved_text = content.clone();
        document.pending_text = Some(content);
        document.load = DocsDocumentLoad::Ready;
        self.native_docs.documents.push(document);
        self.native_docs.active = Some(path);
        self.native_docs.highlights_stale = true;
        self.native_docs_notify(cx);
    }

    /// Closes any open review document and forgets its notes (they are never written anywhere).
    pub(crate) fn native_docs_drop_reviews(&mut self, cx: &mut Context<Self>) {
        let reviews: Vec<String> = self
            .native_docs
            .documents
            .iter()
            .map(|document| document.path.clone())
            .filter(|path| is_review_path(path))
            .collect();
        for path in reviews {
            self.native_docs.notes.remove(&path);
            self.native_docs
                .documents
                .retain(|document| document.path != path);
            if self.native_docs.active.as_deref() == Some(path.as_str()) {
                self.native_docs.active = None;
            }
        }
        self.native_docs_notify(cx);
    }
}
