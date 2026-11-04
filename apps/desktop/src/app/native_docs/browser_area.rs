//! The browser area: HTML files, Excalidraw drawings, video, audio, PDFs, AVIF pictures and fonts
//! need a browser engine, so the Docs page runs in embed mode
//! (`apps/desktop/views/manage/embed.tsx`) as a normal child of the document area, showing that
//! one file.

use gpui::{AnyElement, Context, IntoElement as _};

use super::state::DocsFileKind;
use crate::GhostexGpuiApp;
use crate::app::model::*;

impl GhostexGpuiApp {
    /// The embed page's URL for the open HTML file or drawing, or `None` when another kind of
    /// file (or nothing) is open, which leaves the slot without a page.
    ///
    /// CDXC:Docs 2026-10-26 WHY:
    /// A project switch applies the new project before the next render brings back its Files state (`native_docs_sync_project`), so meanwhile the open file is still the old project's. An address built from both made a throwaway page for a file this project never had open, and with Keep views loaded on that page replaced the one held for the file it did have open.
    pub(crate) fn native_docs_browser_area_url(
        &self,
        snapshot: &crate::GpuiProjectSnapshot,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        let project = self.native_docs.project.as_ref()?;
        if snapshot.active_project_id.as_ref().map(|id| id.0.as_str())
            != Some(project.project_id.as_str())
            || snapshot.in_memory_project_path.as_ref() != Some(&project.project_path)
        {
            return None;
        }
        let document = self.native_docs.active_document()?;
        if !document.kind.uses_browser_area()
            || !crate::app::helpers::web_runtime::web_runtime_available()
        {
            return None;
        }
        // A published page loads from its gxserver, not through the embed page
        // (`published_page.rs`).
        if document.kind == DocsFileKind::Page {
            return ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(
                self.native_docs_page_browser_area_url()?,
            );
        }
        let media = match document.kind {
            DocsFileKind::Video => "video",
            DocsFileKind::Audio => "audio",
            DocsFileKind::Pdf => "pdf",
            DocsFileKind::BrowserImage => "image",
            DocsFileKind::Font => "font",
            _ => "",
        };
        let base =
            crate::app::helpers::manage_workarea_runtime_url_from_project_snapshot(snapshot)?;
        ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(
            crate::app::helpers::append_url_query_params(
                base.value,
                &[
                    ("embed", "1".to_string()),
                    ("path", document.path.to_string()),
                    // Where its sibling stylesheets, scripts and media load from.
                    (
                        "resourcePath",
                        self.native_docs_resource_path(&document.path),
                    ),
                    (
                        "annotate",
                        if document.html_annotate { "1" } else { "0" }.to_string(),
                    ),
                    ("revision", document.embed_revision.to_string()),
                    ("media", media.to_string()),
                ],
            ),
        )
    }

    /// Something native is drawn over the document inside the main window (the notes list or
    /// the composer). A browser page cannot sit under GPUI content, so it hides meanwhile. The
    /// floating files list has a window of its own and leaves the page showing.
    pub(crate) fn native_docs_browser_area_covered(&self) -> bool {
        self.native_docs.composer.is_some()
            || self.native_docs.notes_list_open
            || self.native_docs.rename_dialog.is_some()
    }

    /// Creates, replaces or hides the page after the open file or the covering state changed.
    pub(crate) fn native_docs_sync_browser_area(&mut self, cx: &mut Context<Self>) {
        let key = (
            self.native_docs
                .active_document()
                .filter(|document| document.kind.uses_browser_area())
                .map(|document| {
                    (
                        document.path.clone(),
                        document.html_annotate,
                        document.embed_revision,
                    )
                }),
            self.native_docs_browser_area_covered(),
        );
        if self.native_docs.browser_area_key.as_ref() == Some(&key) {
            return;
        }
        self.native_docs.browser_area_key = Some(key);
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = app.update(cx, |this, cx| {
                this.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                this.prune_project_workarea_runtime_cef_surfaces_for_current_gates(cx);
                this.update_project_workarea_runtime_cef_surface_visibility(cx);
            });
        });
    }

    /// The page, placed in the document area once it exists.
    pub(crate) fn render_native_docs_browser_area(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let slot = ProjectWorkareaCefSurfaceSlotKey::Manage;
        let surface = self.project_workarea_runtime_cef_surface_for_render(slot)?;
        Some(
            self.render_project_workarea_runtime_cef_surface(slot, surface, cx)
                .into_any_element(),
        )
    }
}
