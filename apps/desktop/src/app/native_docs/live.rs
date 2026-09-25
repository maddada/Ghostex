//! The live Markdown editor (zorite-editor): creating it for a document, reacting to its edits,
//! following its links, switching Live and Source, and keeping the caret on screen.

use gpui::{AppContext as _, Context, Entity, Window, px};
use serde_json::json;
use zorite_editor::{EditorEvent, EditorState};

use super::state::{DocsFileKind, DocsMarkdownMode};
use crate::GhostexGpuiApp;

/// The text of an open document, from whichever editor it has.
pub(crate) fn document_text(
    document: &super::state::DocsDocument,
    cx: &gpui::App,
) -> Option<String> {
    if let Some(live) = document.live.as_ref() {
        return Some(live.read(cx).text().to_string());
    }
    Some(document.editor.as_ref()?.read(cx).value().to_string())
}

/// Backspace, Delete, the arrows, Enter, Tab and the editing shortcuts are key bindings in the
/// editor's own key context; typed text arrives without them, so an editor with none of them bound
/// takes letters but not a single Backspace or arrow key.
fn bind_editor_keys(cx: &mut gpui::App) {
    static BOUND: std::sync::Once = std::sync::Once::new();
    BOUND.call_once(|| zorite_editor::bind_keys(cx));
}

impl GhostexGpuiApp {
    /// Builds the live editor for a Markdown document whose text has arrived.
    pub(crate) fn native_docs_create_live_editor(
        &mut self,
        path: &str,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        super::fonts::register(cx);
        bind_editor_keys(cx);
        let p = self
            .native_docs
            .palette
            .clone()
            .unwrap_or_else(|| super::palette::DocsPalette::current(window));
        let live_mode = self
            .native_docs
            .document(path)
            .is_none_or(|document| document.mode == DocsMarkdownMode::Live);
        let style = super::editor_style::syntax_style(&p);
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx).with_text(initial);
            editor.set_tab_indent(2);
            if live_mode {
                editor.set_markdown_style(style, cx);
            }
            editor
        });
        super::blocks::install(
            &editor,
            &self.native_docs.blocks,
            path.to_string(),
            p.light,
            cx,
        );
        let subscription = cx.subscribe_in(&editor, window, Self::native_docs_on_live_event);
        if let Some(document) = self.native_docs.document_mut(path) {
            document.live = Some(editor.clone());
            document._live_subscription = Some(subscription);
        }
        let scope = self.manage_docs_resource_scope();
        super::blocks::prerender(&editor, &self.native_docs.blocks, path, p.light, scope, cx);
        self.native_docs_load_git_base(path, cx);
    }

    fn native_docs_on_live_event(
        &mut self,
        editor: &Entity<EditorState>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .native_docs
            .documents
            .iter()
            .find(|document| document.live.as_ref() == Some(editor))
            .map(|document| document.path.clone())
        else {
            return;
        };
        match event {
            EditorEvent::Changed => {
                let text = editor.read(cx).text().to_string();
                if let Some(document) = self.native_docs.document_mut(&path) {
                    if document.review_session_title.is_none() {
                        document.dirty = document.saved_text != text;
                    }
                    document.changes = document
                        .git_base
                        .as_deref()
                        .map(|base| super::gutter::diff(base, &text));
                }
                self.native_docs.highlights_stale = true;
                self.native_docs_schedule_draft_write(&path, cx);
                let light = self.native_docs.palette.as_ref().is_some_and(|p| p.light);
                let scope = self.manage_docs_resource_scope();
                super::blocks::prerender(editor, &self.native_docs.blocks, &path, light, scope, cx);
                self.native_docs_refresh_find(cx);
                self.native_docs_keep_caret_visible(window, cx);
                self.native_docs_notify(cx);
            }
            EditorEvent::SelectionChanged => {
                self.native_docs_keep_caret_visible(window, cx);
                self.native_docs_notify(cx);
            }
            EditorEvent::OpenLink(target) => self.native_docs_open_link(&path, target, cx),
            EditorEvent::OpenWikiLink(target) => {
                let wanted = target
                    .split(['#', '|'])
                    .next()
                    .unwrap_or(target)
                    .trim()
                    .to_lowercase();
                let found = self
                    .native_docs
                    .entries
                    .iter()
                    .find(|entry| {
                        let name = entry.name.to_lowercase();
                        name == format!("{wanted}.md") || name == wanted
                    })
                    .map(|entry| entry.path.clone());
                if let Some(found) = found {
                    self.native_docs_open_external(found, cx);
                }
            }
            _ => {}
        }
    }

    /// A Cmd-clicked link: web addresses open in the browser, relative paths inside the Docs
    /// roots open in Docs.
    fn native_docs_open_link(&mut self, doc_path: &str, target: &str, cx: &mut Context<Self>) {
        if target.contains("://") || target.starts_with("mailto:") {
            cx.open_url(target);
            return;
        }
        let target = target.split('#').next().unwrap_or(target);
        if target.is_empty() {
            return;
        }
        if let Some(resolved) = super::blocks::resolve_image_path(doc_path, target) {
            self.native_docs_open_external(resolved, cx);
        }
    }

    /// Scrolls the document so the caret stays on screen after an edit or a caret move.
    pub(crate) fn native_docs_keep_caret_visible(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.on_next_frame(window, |this, _, cx| {
            let Some(document) = this.native_docs.active_document() else {
                return;
            };
            let Some(live) = document.live.as_ref() else {
                return;
            };
            let Some(caret) = live.read(cx).caret_screen_bounds() else {
                return;
            };
            let scroll = document.scroll.clone();
            let viewport = scroll.bounds();
            let mut offset = scroll.offset();
            let (top_margin, bottom_margin) = (px(24.0), px(96.0));
            if caret.bottom() + bottom_margin > viewport.bottom() {
                offset.y -= caret.bottom() + bottom_margin - viewport.bottom();
            } else if caret.top() - top_margin < viewport.top() {
                offset.y = (offset.y + (viewport.top() - (caret.top() - top_margin))).min(px(0.0));
            } else {
                return;
            }
            scroll.set_offset(offset);
            cx.notify();
        });
    }

    /// Live or Source for a Markdown document: the same editor, with or without its rendering.
    pub(crate) fn native_docs_toggle_live(&mut self, path: &str, cx: &mut Context<Self>) {
        let p = self.native_docs.palette.clone();
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        document.mode = match document.mode {
            DocsMarkdownMode::Live => DocsMarkdownMode::Source,
            DocsMarkdownMode::Source => DocsMarkdownMode::Live,
        };
        let live_mode = document.mode == DocsMarkdownMode::Live;
        if let (Some(editor), Some(p)) = (document.live.clone(), p) {
            editor.update(cx, |editor, cx| {
                if live_mode {
                    editor.set_markdown_style(super::editor_style::syntax_style(&p), cx);
                } else {
                    editor.clear_markdown_style(cx);
                }
            });
        }
        self.native_docs.highlights_stale = true;
        self.native_docs_notify(cx);
    }

    /// Re-applies the palette to every live editor after the appearance changed.
    pub(crate) fn native_docs_restyle_live_editors(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.native_docs.palette.clone() else {
            return;
        };
        let style = super::editor_style::syntax_style(&p);
        for document in &self.native_docs.documents {
            if document.kind != DocsFileKind::Markdown || document.mode != DocsMarkdownMode::Live {
                continue;
            }
            if let Some(editor) = document.live.clone() {
                let style = style.clone();
                editor.update(cx, |editor, cx| editor.set_markdown_style(style, cx));
            }
        }
    }

    /// The file's text at HEAD, for the git change stripe.
    fn native_docs_load_git_base(&mut self, path: &str, cx: &mut Context<Self>) {
        if super::entry::is_review_path(path) {
            return;
        }
        let generation = self.native_docs.generation;
        let request = self.native_docs_request("gitBaseline", json!({ "path": path }));
        let path = path.to_string();
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            let baseline = &response["gitBaseline"];
            let base = (baseline["available"].as_bool() == Some(true))
                .then(|| baseline["baseText"].as_str().map(str::to_string))
                .flatten();
            let text = this
                .native_docs
                .document(&path)
                .and_then(|document| document_text(document, cx));
            if let Some(document) = this.native_docs.document_mut(&path) {
                document.changes = base
                    .as_deref()
                    .zip(text.as_deref())
                    .map(|(base, text)| super::gutter::diff(base, text));
                document.git_base = base;
            }
            this.native_docs_notify(cx);
        });
    }
}
