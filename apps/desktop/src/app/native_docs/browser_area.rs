//! The browser area: HTML files and Excalidraw drawings still need a browser engine, so the Docs
//! page runs in embed mode (`apps/desktop/views/manage/embed.tsx`) as a normal child of the
//! document area, showing that one file.

use gpui::{AnyElement, Context, IntoElement as _};

use super::state::DocsFileKind;
use crate::GhostexGpuiApp;
use crate::app::model::*;

impl GhostexGpuiApp {
    /// The embed page's URL for the open HTML file or drawing, or `None` when another kind of
    /// file (or nothing) is open, which leaves the slot without a page.
    pub(crate) fn native_docs_browser_area_url(
        &self,
        snapshot: &crate::GpuiProjectSnapshot,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        let document = self.native_docs.active_document()?;
        if !matches!(document.kind, DocsFileKind::Html | DocsFileKind::Excalidraw) {
            return None;
        }
        let base =
            crate::app::helpers::manage_workarea_runtime_url_from_project_snapshot(snapshot)?;
        ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(
            crate::app::helpers::append_url_query_params(
                base.value,
                &[
                    ("embed", "1".to_string()),
                    ("path", document.path.clone()),
                    (
                        "annotate",
                        if document.html_annotate { "1" } else { "0" }.to_string(),
                    ),
                    ("revision", document.embed_revision.to_string()),
                ],
            ),
        )
    }

    /// Something native is drawn over the document inside the main window (the notes list or
    /// the composer). A browser page cannot sit under GPUI content, so it hides meanwhile. The
    /// floating files list has a window of its own and leaves the page showing.
    pub(crate) fn native_docs_browser_area_covered(&self) -> bool {
        if !super::render::native_docs_enabled() {
            return false;
        }
        self.native_docs.composer.is_some() || self.native_docs.notes_list_open
    }

    /// Creates, replaces or hides the page after the open file or the covering state changed.
    pub(crate) fn native_docs_sync_browser_area(&mut self, cx: &mut Context<Self>) {
        let key = (
            self.native_docs
                .active_document()
                .filter(|document| {
                    matches!(document.kind, DocsFileKind::Html | DocsFileKind::Excalidraw)
                })
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
