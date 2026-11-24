//! Per-pane navigation history (path + scroll position) for the Files view.
//!
//! CDXC:Docs 2026-11-24 DECISION:
//! GitHub #733: Back and Forward in the Files view walk a per-pane trail of visited files and their
//! scroll positions. Opening a file from the files list, tabs, search or a link inside an HTML or
//! Markdown document pushes a stop; Back and Forward restore the earlier/later file at its saved
//! scroll position (reopening it from disk when its tab was closed).

use gpui::{Context, point, px};
use serde_json::json;

use super::state::DocsFileKind;
use crate::GhostexGpuiApp;

/// The custom DOM event dispatched to the Files embed page to restore an HTML page's scroll
/// position when an already-loaded (parked) CEF surface returns to screen.
pub(crate) const SCROLL_COMMAND_EVENT: &str = "ghostex-manage-scroll-command";

const MAX_HISTORY_ENTRIES: usize = 100;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DocsScrollPosition {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl DocsScrollPosition {
    pub(crate) fn from_js(x: f64, y: f64) -> Option<Self> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        Some(Self {
            x: (x as f32).max(0.0),
            y: (y as f32).max(0.0),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocsHistoryEntry {
    pub(crate) path: String,
    pub(crate) scroll: DocsScrollPosition,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DocsHistory {
    entries: Vec<DocsHistoryEntry>,
    /// Index of the current stop in `entries`, when `entries` is non-empty.
    cursor: Option<usize>,
}

impl DocsHistory {
    pub(crate) fn can_go_back(&self) -> bool {
        self.cursor.is_some_and(|index| index > 0)
    }

    pub(crate) fn can_go_forward(&self) -> bool {
        self.cursor
            .is_some_and(|index| index + 1 < self.entries.len())
    }

    /// Seeds the initial stop when a project restores its previously active file.
    pub(crate) fn seed_initial(&mut self, path: &str) {
        if self.entries.is_empty() {
            self.entries.push(DocsHistoryEntry {
                path: path.to_string(),
                scroll: DocsScrollPosition::default(),
            });
            self.cursor = Some(0);
        }
    }

    /// Updates the current stop's recorded scroll position when it still names `path`.
    pub(crate) fn update_current_scroll(&mut self, path: &str, scroll: DocsScrollPosition) {
        if let Some(index) = self.cursor
            && let Some(entry) = self.entries.get_mut(index)
            && entry.path == path
        {
            entry.scroll = scroll;
        }
    }

    /// Records navigating from `previous` (with its scroll position) to `next_path`.
    /// Truncates any forward stops after the cursor and avoids consecutive duplicate paths.
    pub(crate) fn record_visit(
        &mut self,
        previous: Option<(&str, DocsScrollPosition)>,
        next_path: &str,
    ) {
        if self.entries.is_empty() {
            if let Some((prev_path, prev_scroll)) = previous
                && prev_path != next_path
            {
                self.entries.push(DocsHistoryEntry {
                    path: prev_path.to_string(),
                    scroll: prev_scroll,
                });
                self.entries.push(DocsHistoryEntry {
                    path: next_path.to_string(),
                    scroll: DocsScrollPosition::default(),
                });
                self.cursor = Some(1);
                return;
            }
            self.entries.push(DocsHistoryEntry {
                path: next_path.to_string(),
                scroll: DocsScrollPosition::default(),
            });
            self.cursor = Some(0);
            return;
        }
        let index = self.cursor.unwrap_or(self.entries.len() - 1);
        if let Some((prev_path, prev_scroll)) = previous
            && self.entries[index].path == prev_path
        {
            self.entries[index].scroll = prev_scroll;
        }
        if self.entries[index].path == next_path {
            return;
        }
        self.entries.truncate(index + 1);
        self.entries.push(DocsHistoryEntry {
            path: next_path.to_string(),
            scroll: DocsScrollPosition::default(),
        });
        if self.entries.len() > MAX_HISTORY_ENTRIES {
            let drop = self.entries.len() - MAX_HISTORY_ENTRIES;
            self.entries.drain(..drop);
        }
        self.cursor = Some(self.entries.len() - 1);
    }

    /// Moves one stop back or forward, saving `current`'s scroll position first.
    pub(crate) fn step(
        &mut self,
        back: bool,
        current: Option<(&str, DocsScrollPosition)>,
    ) -> Option<DocsHistoryEntry> {
        let index = self.cursor?;
        if let Some((current_path, current_scroll)) = current
            && let Some(entry) = self.entries.get_mut(index)
            && entry.path == current_path
        {
            entry.scroll = current_scroll;
        }
        let next_index = if back {
            index.checked_sub(1)?
        } else {
            (index + 1 < self.entries.len()).then_some(index + 1)?
        };
        self.cursor = Some(next_index);
        self.entries.get(next_index).cloned()
    }

    /// Keeps the cursor coherent when a document tab is closed. Virtual documents (reviews and
    /// published pages) cannot be reopened from disk, so they leave history; regular files remain
    /// reopenable via Back/Forward.
    pub(crate) fn on_close(
        &mut self,
        closed_path: &str,
        closed_scroll: DocsScrollPosition,
        next_active: Option<&str>,
    ) {
        self.update_current_scroll(closed_path, closed_scroll);
        if super::entry::is_review_path(closed_path)
            || super::published_page::is_page_path(closed_path)
        {
            self.remove_matching(|path| path == closed_path);
            return;
        }
        if let Some(next) = next_active
            && let Some(index) = self.cursor
            && self
                .entries
                .get(index)
                .is_some_and(|e| e.path == closed_path)
            && index > 0
            && self.entries[index - 1].path == next
        {
            self.cursor = Some(index - 1);
        }
    }

    /// Rewrites history paths after a file or folder rename/move.
    pub(crate) fn remap_paths(&mut self, old: &str, new: &str) {
        let prefix = format!("{old}/");
        for entry in &mut self.entries {
            if entry.path == old {
                entry.path = new.to_string();
            } else if let Some(rest) = entry.path.strip_prefix(&prefix) {
                entry.path = format!("{new}/{rest}");
            }
        }
    }

    /// Drops entries matching `predicate` (deleted files or closed virtual documents), collapses
    /// any adjacent duplicates that result, and keeps the cursor valid.
    pub(crate) fn remove_matching(&mut self, mut predicate: impl FnMut(&str) -> bool) {
        let old_cursor = self.cursor;
        let mut next: Vec<DocsHistoryEntry> = Vec::with_capacity(self.entries.len());
        let mut next_cursor: Option<usize> = None;
        for (index, entry) in self.entries.drain(..).enumerate() {
            let is_cursor = old_cursor == Some(index);
            if predicate(&entry.path) {
                if is_cursor && !next.is_empty() {
                    next_cursor = Some(next.len() - 1);
                }
                continue;
            }
            if next.last().is_some_and(|last| last.path == entry.path) {
                if is_cursor {
                    next_cursor = Some(next.len() - 1);
                }
                continue;
            }
            next.push(entry);
            if is_cursor {
                next_cursor = Some(next.len() - 1);
            }
        }
        self.entries = next;
        self.cursor = if self.entries.is_empty() {
            None
        } else {
            next_cursor.or(Some(self.entries.len() - 1))
        };
    }
}

impl GhostexGpuiApp {
    /// Reads the current scroll position of `path` from its live viewer or editor.
    pub(crate) fn native_docs_capture_scroll(
        &self,
        path: &str,
        cx: &Context<Self>,
    ) -> DocsScrollPosition {
        let Some(document) = self.native_docs.document(path) else {
            return DocsScrollPosition::default();
        };
        match document.kind {
            DocsFileKind::Html => document.html_scroll,
            DocsFileKind::Markdown => {
                let offset = document.scroll.offset();
                DocsScrollPosition {
                    x: f32::from(offset.x),
                    y: f32::from(offset.y),
                }
            }
            DocsFileKind::Image if !document.svg_source => {
                let offset = document.picture.scroll.offset();
                DocsScrollPosition {
                    x: f32::from(offset.x),
                    y: f32::from(offset.y),
                }
            }
            _ => document
                .editor
                .as_ref()
                .map(|editor| {
                    let offset = editor.read(cx).scroll_offset();
                    DocsScrollPosition {
                        x: f32::from(offset.x),
                        y: f32::from(offset.y),
                    }
                })
                .unwrap_or_default(),
        }
    }

    /// Records a visit to `next_path`, saving the outgoing active file's scroll position first.
    pub(crate) fn native_docs_record_history_visit(&mut self, next_path: &str, cx: &Context<Self>) {
        let previous = self
            .native_docs
            .active
            .as_deref()
            .map(|path| (path.to_string(), self.native_docs_capture_scroll(path, cx)));
        self.native_docs.history.record_visit(
            previous
                .as_ref()
                .map(|(path, scroll)| (path.as_str(), *scroll)),
            next_path,
        );
    }

    /// Updates the last-known HTML scroll position reported by the embed page for `path`.
    pub(crate) fn native_docs_update_html_scroll(
        &mut self,
        path: &str,
        scroll: DocsScrollPosition,
    ) {
        if let Some(document) = self.native_docs.document_mut(path) {
            document.html_scroll = scroll;
        }
        self.native_docs.history.update_current_scroll(path, scroll);
    }

    /// Walks one stop back or forward in the Files view's history, reopening the file when needed
    /// and restoring its saved scroll position. Returns `true` when a stop was navigated to.
    pub(crate) fn native_docs_navigate_history(
        &mut self,
        back: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let current = self
            .native_docs
            .active
            .as_deref()
            .map(|path| (path.to_string(), self.native_docs_capture_scroll(path, cx)));
        let Some(entry) = self.native_docs.history.step(
            back,
            current
                .as_ref()
                .map(|(path, scroll)| (path.as_str(), *scroll)),
        ) else {
            return false;
        };
        let display_path = self.native_docs_display_path(&entry.path);
        self.native_docs_reveal_in_tree(&entry.path, cx);
        self.native_docs_open_without_history(&entry.path, &display_path, cx);
        if let Some(document) = self.native_docs.document_mut(&entry.path) {
            document.pending_scroll = Some(entry.scroll);
            if document.kind == DocsFileKind::Html {
                document.html_scroll = entry.scroll;
            }
        }
        self.native_docs_restore_document_scroll(&entry.path, cx);
        self.native_docs_apply_pending_html_scroll(cx);
        self.native_docs_notify(cx);
        true
    }

    /// Restores a non-HTML document's `pending_scroll` immediately when its viewer or editor is
    /// already alive; newly reopened files keep `pending_scroll` until their editor materializes.
    pub(crate) fn native_docs_restore_document_scroll(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        let Some(scroll) = document.pending_scroll else {
            return;
        };
        match document.kind {
            DocsFileKind::Html => {}
            DocsFileKind::Markdown => {
                if document.live.is_some() {
                    document
                        .scroll
                        .set_offset(point(px(scroll.x), px(scroll.y)));
                    if document.scroll.bounds().size.height > px(0.0) {
                        document.pending_scroll = None;
                    }
                }
            }
            DocsFileKind::Image if !document.svg_source => {
                if document.image.is_some() {
                    document
                        .picture
                        .scroll
                        .set_offset(point(px(scroll.x), px(scroll.y)));
                    document.pending_scroll = None;
                }
            }
            _ => {
                if let Some(editor) = document.editor.clone() {
                    document.pending_scroll = None;
                    editor.update(cx, |state, cx| {
                        state.set_scroll_offset(point(px(scroll.x), px(scroll.y)), cx);
                    });
                }
            }
        }
    }

    /// Sends a pending HTML scroll target to the active Manage CEF surface once it is already
    /// loaded (for example, unparked from keep-alive). Freshly created pages leave `pending_scroll`
    /// in place so `annotateLoad` delivers it before the iframe renders.
    pub(crate) fn native_docs_apply_pending_html_scroll(&mut self, cx: &mut Context<Self>) {
        let Some((path, scroll)) = self
            .native_docs
            .active_document()
            .filter(|doc| doc.kind == DocsFileKind::Html)
            .and_then(|doc| Some((doc.path.clone(), doc.pending_scroll?)))
        else {
            return;
        };
        let loaded_for_path = self
            .project_workarea_runtime_cef_surfaces
            .get(&crate::ProjectWorkareaCefSurfaceSlotKey::Manage)
            .is_some_and(|owned| {
                owned.page_loaded.load(std::sync::atomic::Ordering::Relaxed)
                    && crate::app::helpers::manage_embed_page_rules::manage_embed_url_param(
                        &owned.runtime_url.value,
                        "path",
                    )
                    .as_deref()
                        == Some(path.as_ref())
            });
        if !loaded_for_path {
            return;
        }
        if let Some(document) = self.native_docs.document_mut(&path) {
            document.pending_scroll = None;
        }
        let detail = json!({ "x": scroll.x, "y": scroll.y });
        self.dispatch_project_workarea_json_event(
            crate::ProjectWorkareaCefSurfaceSlotKey::Manage,
            SCROLL_COMMAND_EVENT,
            &detail.to_string(),
            cx,
        );
    }
}
