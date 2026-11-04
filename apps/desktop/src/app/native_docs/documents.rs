//! Open documents: reading a file into its editor, tracking unsaved changes, saving and closing.

use gpui::{AppContext as _, Context, SharedString, Window};
use gpui_component::input::{EditorState, InputEvent};
use serde_json::json;

use super::state::{DocsDocument, DocsDocumentLoad, DocsFileKind};
use crate::GhostexGpuiApp;

/// What a file opened from a link or a search says when it is not there any more.
const MISSING_FILE_MESSAGE: &str = "This file no longer exists.";

impl GhostexGpuiApp {
    /// Opens `path` (or selects it when it is already open) and closes a narrow view's drawer.
    pub(crate) fn native_docs_open(
        &mut self,
        path: &str,
        display_path: &str,
        cx: &mut Context<Self>,
    ) {
        if DocsFileKind::for_path(path) == DocsFileKind::SystemApp {
            self.native_docs_open_with_system_app(path, cx);
            return;
        }
        if !super::entry::is_review_path(path) {
            self.native_docs_drop_reviews(cx);
        }
        if self.native_docs.document(path).is_none() {
            self.native_docs.documents.push(DocsDocument::new(
                path.to_string(),
                display_path.to_string(),
            ));
            self.native_docs_read(path, cx);
        }
        self.native_docs.active = Some(path.to_string());
        self.native_docs_close_drawer(cx);
        self.native_docs_persist_open_files(cx);
        self.native_docs_notify(cx);
    }

    /// Reads a file from disk into its document. Images, HTML and drawings are not read as text,
    /// and a SQLite database is read a page at a time.
    pub(crate) fn native_docs_read(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(kind) = self
            .native_docs
            .document(path)
            .map(|document| document.kind)
        else {
            return;
        };
        if kind == DocsFileKind::Image {
            self.native_docs_read_image(path, cx);
            if !self
                .native_docs
                .document(path)
                .is_some_and(|document| document.svg_source)
            {
                return;
            }
        } else if kind.uses_browser_area() {
            if let Some(document) = self.native_docs.document_mut(path) {
                document.load = DocsDocumentLoad::Ready;
            }
            return;
        } else if kind == DocsFileKind::Sqlite {
            self.native_docs_sqlite_load(path, cx);
            return;
        }
        let generation = self.native_docs.generation;
        let request = self.native_docs_request("read", json!({ "path": path }));
        let path = path.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            this.native_docs_apply_read(&path, &response, cx);
        });
    }

    /// Opens a file Files does not show (a PDF, an `.mp4` the embedded browser cannot play, a
    /// binary) in the system's own app.
    pub(crate) fn native_docs_open_with_system_app(&mut self, path: &str, cx: &mut Context<Self>) {
        let request = self.native_docs_request("openWithSystemApp", json!({ "path": path }));
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

    /// Opens a Markdown file too large for the Markdown editor in the Code view.
    pub(crate) fn native_docs_open_in_code_view(&mut self, path: &str, cx: &mut Context<Self>) {
        let request = self.native_docs_request("openInCodeView", json!({ "path": path }));
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if let Some(error) = response["error"].as_str() {
                this.dispatch_gpui_workspace_action_toast(
                    "error",
                    "Couldn't open the file in VS Code view",
                    error,
                    cx,
                );
            }
        });
    }

    /// SVG images: switches between the picture and its source in the code editor.
    pub(crate) fn native_docs_toggle_svg_source(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        document.svg_source = !document.svg_source;
        let needs_text = document.svg_source && document.editor.is_none();
        if needs_text {
            let generation = self.native_docs.generation;
            let request = self.native_docs_request("read", json!({ "path": path }));
            let path = path.to_string();
            self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
                if this.native_docs.generation != generation {
                    return;
                }
                this.native_docs_apply_read(&path, &response, cx);
            });
        }
        self.native_docs_notify(cx);
    }

    /// Reads an image through the Docs resource scope, the same roots the page's images used.
    pub(crate) fn native_docs_read_image(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(scope) = self.manage_docs_resource_scope() else {
            return;
        };
        let format = match path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("png") => gpui::ImageFormat::Png,
            Some("jpg" | "jpeg" | "jfif" | "jpe") => gpui::ImageFormat::Jpeg,
            Some("gif") => gpui::ImageFormat::Gif,
            Some("webp") => gpui::ImageFormat::Webp,
            Some("svg") => gpui::ImageFormat::Svg,
            Some("bmp") => gpui::ImageFormat::Bmp,
            Some("ico") => gpui::ImageFormat::Ico,
            Some("tif" | "tiff") => gpui::ImageFormat::Tiff,
            _ => gpui::ImageFormat::Png,
        };
        let generation = self.native_docs.generation;
        let path = path.to_string();
        let relative = self.native_docs_resource_path(&path);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let bytes = background
                .spawn(async move {
                    crate::app::helpers::manage_docs_resources::read_manage_docs_resource(
                        &scope, &relative,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.native_docs.generation != generation {
                    return;
                }
                let Some(document) = this.native_docs.document_mut(&path) else {
                    return;
                };
                match bytes {
                    Some(bytes) => {
                        document.image =
                            Some(std::sync::Arc::new(gpui::Image::from_bytes(format, bytes)));
                        document.load = DocsDocumentLoad::Ready;
                    }
                    None => {
                        document.load =
                            DocsDocumentLoad::Error("This image couldn't be read.".to_string());
                    }
                }
                this.native_docs_notify(cx);
            });
        })
        .detach();
    }

    fn native_docs_apply_read(
        &mut self,
        path: &str,
        response: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let file = &response["file"];
        if file["kind"].as_str() == Some("unsupported") {
            let reason = file["error"]
                .as_str()
                .unwrap_or("This file can't be shown in Files.")
                .to_string();
            if let Some(document) = self.native_docs.document_mut(path) {
                document.size = file["size"].as_u64();
                document.load = DocsDocumentLoad::Unsupported(reason);
            }
            self.native_docs_notify(cx);
            return;
        }
        if file["missing"] == true {
            /*
            CDXC:Docs 2026-10-28 WHY: the Files view reopens the files that were open last time,
            and one deleted since then failed with "Manage paths must stay inside the project"
            (`manage_existing_url` reads every unresolvable path as outside the project), leaving
            the view on an error instead of the tree (found testing on Linux; every platform).
            A file that is not there any more answers `missing`: a reopened one without a draft
            closes quietly, so the view falls back to the next file or the tree, and anything else
            says plainly that the file is gone.
            */
            let quiet = self.native_docs.document(path).is_some_and(|document| {
                document.restored && !self.native_docs.drafts.contains_key(path)
            });
            if quiet {
                self.native_docs_close(path, cx);
            } else {
                if let Some(document) = self.native_docs.document_mut(path) {
                    document.load = DocsDocumentLoad::Error(MISSING_FILE_MESSAGE.to_string());
                }
                self.native_docs_notify(cx);
            }
            return;
        }
        let error = response["error"]
            .as_str()
            .or_else(|| file["error"].as_str())
            .map(str::to_string);
        if let Some(error) = error {
            if let Some(document) = self.native_docs.document_mut(path) {
                document.load = DocsDocumentLoad::Error(error);
            }
            self.native_docs_notify(cx);
            return;
        }
        let text = file["content"].as_str().unwrap_or_default().to_string();
        if let Some(document) = self.native_docs.document_mut(path) {
            document.disk_signature = super::watch::disk_signature(file);
            document.disk = super::state::DocsDiskState::InSync;
            document.disk_reload = None;
            document.size = file["size"].as_u64();
            if let Some(display_path) = file["displayPath"].as_str() {
                document.display_path = display_path.into();
            }
            document.saved_text = text.clone();
            document.pending_text = Some(text);
            document.load = DocsDocumentLoad::Ready;
        }
        self.native_docs_notify(cx);
    }

    /// Gives every document whose text has arrived its editor. Runs from the render, which is
    /// where a window is at hand; the unsaved draft, when there is one, goes in over the disk text.
    pub(crate) fn native_docs_materialize_editors(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A clean code file that changed on disk takes the new text in place, keeping its caret
        // and scroll position (`native_docs_apply_disk_text`).
        for document in &mut self.native_docs.documents {
            let (Some(editor), Some(text)) = (document.editor.clone(), document.disk_reload.take())
            else {
                continue;
            };
            // A table or tree shows the new text too.
            document.data_view = None;
            editor.update(cx, |state, cx| {
                let selection = state.selected_range();
                let scroll = state.scroll_offset();
                let len = text.len();
                state.set_value(text, window, cx);
                state.set_selected_range(selection.start.min(len)..selection.end.min(len), cx);
                state.set_scroll_offset(scroll, cx);
            });
        }
        let waiting: Vec<(SharedString, String)> = self
            .native_docs
            .documents
            .iter_mut()
            .filter_map(|document| {
                let text = document.pending_text.take()?;
                Some((document.path.clone(), text))
            })
            .collect();
        for (path, text) in waiting {
            let draft = self.native_docs_stored_draft(&path, &text);
            let language = DocsFileKind::editor_language(&path);
            let initial = draft.clone().unwrap_or_else(|| text.clone());
            if self
                .native_docs
                .document(&path)
                .is_some_and(|document| document.kind == DocsFileKind::Markdown)
            {
                self.native_docs_create_live_editor(&path, initial, window, cx);
                self.native_docs.highlights_stale = true;
                if let Some(document) = self.native_docs.document_mut(&path) {
                    document.dirty = draft.as_ref().is_some_and(|draft| *draft != text);
                }
                continue;
            }
            let line_numbers = self.native_docs.line_numbers;
            let word_wrap = self.native_docs.word_wrap;
            let editor = cx.new(|cx| {
                EditorState::new(window, cx)
                    .language(language)
                    .line_number(line_numbers)
                    // GPUI Kit's editor closes brackets and indents structurally by default; the
                    // Docs text view keeps typing literal, with Enter copying the line's indent.
                    .auto_close(false)
                    .smart_indent(false)
                    .soft_wrap(word_wrap)
                    // The formatting bar floats over the bottom lines; let them scroll out from under it.
                    .scroll_beyond_last_line(Some(4))
                    // Files' own find panel answers Cmd+F (find.rs), not the editor's.
                    .searchable(false)
                    .default_value(initial)
            });
            let watched_path = path.clone();
            let subscription = cx.subscribe_in(
                &editor,
                window,
                move |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value();
                        if let Some(document) = this.native_docs.document_mut(&watched_path)
                            && document.review_session_title.is_none()
                        {
                            let dirty = value.as_ref() != document.saved_text;
                            if dirty != document.dirty {
                                document.dirty = dirty;
                                this.native_docs_notify(cx);
                            }
                        }
                        this.native_docs.highlights_stale = true;
                        if this.native_docs_find_visible() {
                            this.native_docs_refresh_find(cx);
                        }
                        this.native_docs_schedule_draft_write(&watched_path, cx);
                    }
                },
            );
            self.native_docs.highlights_stale = true;
            if let Some(document) = self.native_docs.document_mut(&path) {
                document.dirty = draft.as_ref().is_some_and(|draft| *draft != text);
                document.editor = Some(editor);
                document._editor_subscription = Some(subscription);
            }
        }
    }

    /// The header's wrap toggle: every open text and code editor follows it, and it is remembered.
    pub(crate) fn native_docs_toggle_word_wrap(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wrap = !self.native_docs.word_wrap;
        self.native_docs.word_wrap = wrap;
        for editor in self
            .native_docs
            .documents
            .iter()
            .filter_map(|document| document.editor.clone())
        {
            editor.update(cx, |state, cx| state.set_soft_wrap(wrap, window, cx));
        }
        self.native_docs_persist_word_wrap(cx);
        self.native_docs_notify(cx);
    }

    /// Reload: reads the file again and drops its draft.
    pub(crate) fn native_docs_reload(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        document.editor = None;
        document._editor_subscription = None;
        document.data_view = None;
        document.live = None;
        document._live_subscription = None;
        document.image = None;
        document.embed_revision += 1;
        document.dirty = false;
        document.load = DocsDocumentLoad::Loading;
        self.native_docs.drafts.remove(path);
        self.native_docs_write_drafts(cx);
        self.native_docs_read(path, cx);
        self.native_docs_notify(cx);
    }

    /// Cmd+S: writes the active document to disk.
    ///
    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: Markdown and text files no longer save on their own; edits stay unsaved until Cmd+S, with the unsaved state shown on the header icon and the open-files row. Excalidraw drawings keep their one-second autosave (the browser area's embed page) because drawing gestures have no natural save moment.
    pub(crate) fn native_docs_save_active(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.native_docs.active.clone() else {
            return;
        };
        self.native_docs_save(&path, cx, |_, _| {});
    }

    /// Writes one document, then runs `then` once it is saved.
    pub(crate) fn native_docs_save(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        if document.editor.is_none() && document.live.is_none() {
            return;
        }
        if document.saving || document.review_session_title.is_some() {
            return;
        }
        // VS Code refuses to save over a file that changed on disk under unsaved edits; the
        // conflict notice under the header offers Compare, Keep mine and Take theirs.
        if matches!(document.disk, super::state::DocsDiskState::Conflict(_)) {
            let (title, message) = (
                format!("Couldn't save {}", document.name),
                "The file changed on disk. Compare it with your changes, keep yours, or take the disk version.".to_string(),
            );
            self.dispatch_gpui_workspace_action_toast("error", &title, &message, cx);
            return;
        }
        let Some(content) = super::live::document_text(document, cx) else {
            return;
        };
        document.saving = true;
        let generation = self.native_docs.generation;
        let request = self.native_docs_request("save", json!({ "path": path, "content": content }));
        let path = path.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            let Some(document) = this.native_docs.document_mut(&path) else {
                return;
            };
            document.saving = false;
            if let Some(error) = response["error"].as_str() {
                let (title, error) = (
                    format!("Couldn't save {}", document.name),
                    error.to_string(),
                );
                this.dispatch_gpui_workspace_action_toast("error", &title, &error, cx);
                this.native_docs_notify(cx);
                return;
            }
            document.saved_text = content.clone();
            document.disk_signature =
                super::watch::disk_signature(&response["file"]).or(document.disk_signature.take());
            document.disk = super::state::DocsDiskState::InSync;
            document.size = response["file"]["size"].as_u64().or(document.size);
            document.saved_flash_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(1600));
            // An SVG's picture is drawn from the file, so it follows its saved source.
            let redraw_picture = document.kind == DocsFileKind::Image;
            this.native_docs_notify_after(std::time::Duration::from_millis(1600), cx);
            if redraw_picture {
                this.native_docs_read_image(&path, cx);
            }
            let Some(document) = this.native_docs.document_mut(&path) else {
                return;
            };
            let current = super::live::document_text(document, cx);
            document.dirty = current.is_some_and(|current| current != content);
            let still_dirty = document.dirty;
            if !still_dirty {
                this.native_docs_clear_draft(&path, cx);
            }
            this.native_docs_notify(cx);
            then(this, cx);
        });
    }

    /// Closes a document, asking first when it has unsaved changes.
    pub(crate) fn native_docs_request_close(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dirty = self
            .native_docs
            .document(path)
            .is_some_and(|document| document.dirty);
        if dirty {
            self.native_docs_confirm_close(path, window, cx);
        } else {
            self.native_docs_close(path, cx);
        }
    }

    /// Closes without asking and drops the document's draft.
    pub(crate) fn native_docs_close(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(index) = self
            .native_docs
            .documents
            .iter()
            .position(|document| document.path == path)
        else {
            return;
        };
        self.native_docs.documents.remove(index);
        self.native_docs.sqlite.remove(path);
        self.native_docs_clear_draft(path, cx);
        if self.native_docs.active.as_deref() == Some(path) {
            let next = index.min(self.native_docs.documents.len().saturating_sub(1));
            self.native_docs.active = self
                .native_docs
                .documents
                .get(next)
                .map(|document| document.path.to_string());
        }
        self.native_docs_persist_open_files(cx);
        self.native_docs_notify(cx);
    }
}
