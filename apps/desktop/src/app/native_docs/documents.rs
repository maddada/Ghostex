//! Open documents: reading a file into its editor, tracking unsaved changes, saving and closing.

use gpui::{AppContext as _, Context, Window};
use gpui_component::input::{InputEvent, InputState};
use serde_json::json;

use super::state::{DocsDocument, DocsDocumentLoad, DocsFileKind};
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Opens `path` (or selects it when it is already open) and closes a narrow view's drawer.
    pub(crate) fn native_docs_open(
        &mut self,
        path: &str,
        display_path: &str,
        cx: &mut Context<Self>,
    ) {
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

    /// Reads a file from disk into its document. Images, HTML and drawings are not read as text.
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
            return;
        }
        if matches!(kind, DocsFileKind::Html | DocsFileKind::Excalidraw) {
            if let Some(document) = self.native_docs.document_mut(path) {
                document.load = DocsDocumentLoad::Ready;
            }
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

    /// Reads an image through the Docs resource scope, the same roots the page's images used.
    fn native_docs_read_image(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(scope) = self.manage_docs_resource_scope() else {
            return;
        };
        let format = match path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("png") => gpui::ImageFormat::Png,
            Some("jpg" | "jpeg") => gpui::ImageFormat::Jpeg,
            Some("gif") => gpui::ImageFormat::Gif,
            Some("webp") => gpui::ImageFormat::Webp,
            Some("svg") => gpui::ImageFormat::Svg,
            Some("bmp") => gpui::ImageFormat::Bmp,
            Some("ico") => gpui::ImageFormat::Ico,
            _ => gpui::ImageFormat::Png,
        };
        let generation = self.native_docs.generation;
        let path = path.to_string();
        let relative = path.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let bytes = background
                .spawn(async move { crate::cef::read_manage_docs_resource(&scope, &relative) })
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
        let error = response["error"]
            .as_str()
            .or_else(|| file["error"].as_str())
            .map(str::to_string)
            .or_else(|| {
                (file["kind"].as_str() == Some("unsupported"))
                    .then(|| "This file can't be shown in Docs.".to_string())
            });
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
            document.external_change = false;
            document.size = file["size"].as_u64();
            if let Some(display_path) = file["displayPath"].as_str() {
                document.display_path = display_path.to_string();
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
        let waiting: Vec<(String, String)> = self
            .native_docs
            .documents
            .iter_mut()
            .filter_map(|document| Some((document.path.clone(), document.pending_text.take()?)))
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
            let editor = cx.new(|cx| {
                InputState::new(window, cx)
                    .code_editor(language)
                    .line_number(false)
                    .soft_wrap(DocsFileKind::for_path(&path) == DocsFileKind::Markdown)
                    // The formatting bar floats over the bottom lines; let them scroll out from under it.
                    .scroll_beyond_last_line(Some(4))
                    .searchable(true)
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

    /// Reload: reads the file again and drops its draft.
    pub(crate) fn native_docs_reload(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        document.editor = None;
        document._editor_subscription = None;
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
            document.external_change = false;
            document.size = response["file"]["size"].as_u64().or(document.size);
            document.saved_flash_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(1600));
            this.native_docs_notify_after(std::time::Duration::from_millis(1600), cx);
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
        self.native_docs_clear_draft(path, cx);
        if self.native_docs.active.as_deref() == Some(path) {
            let next = index.min(self.native_docs.documents.len().saturating_sub(1));
            self.native_docs.active = self
                .native_docs
                .documents
                .get(next)
                .map(|document| document.path.clone());
        }
        self.native_docs_persist_open_files(cx);
        self.native_docs_notify(cx);
    }
}
