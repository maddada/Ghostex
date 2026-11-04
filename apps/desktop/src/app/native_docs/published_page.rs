//! A page an agent published in chat (`ghostex show`) opened in the Files view, which Settings >
//! Chat > Open chat pages in can choose instead of the floating page window: a document with no
//! file behind it whose browser area loads the page from the gxserver that serves it.
//!
//! CDXC:SessionChat 2026-10-24 WHY:
//! The page is the agent's own HTML, so it never runs inside the Files view's embed page and never gets that page's file bridge (`postManageFilesRequest`, which the bridged load handler installs into whatever the browser loads). Its browser is made the way the floating page window makes its own: no bridge, links sent to the reader's browser. Its answers go back as they do from the floating window: the page posts them to its own `/visual/<id>/answers` route and gxserver queues them for the session that published it.
//! SEE-ALSO: apps/desktop/src/app/visual_page_modal_lifecycle.rs (the setting), apps/desktop/src/app/window/visual_page_modal.rs (the floating window's browser), server/src/visual_pages/serve.rs (`?gx-page-script=1`).

use std::rc::Rc;

use gpui::{Context, Entity};

use super::state::{DocsDocument, DocsDocumentLoad, DocsFileKind};
use crate::app::helpers::*;
use crate::*;

/// Where page documents live; never read from or written to disk.
pub(crate) const PAGE_DOCUMENT_ROOT: &str = ".ghostex-page";

pub(crate) fn is_page_path(path: &str) -> bool {
    path.starts_with(&format!("{PAGE_DOCUMENT_ROOT}/"))
}

/// Where a page document's page is served from.
#[derive(Clone, Debug)]
pub(crate) struct DocsPageSource {
    /// The page's address on its gxserver (a remote machine's already resolved for this computer).
    pub(crate) url: String,
    /// The remote machine whose session published it, `None` for this computer's.
    pub(crate) remote_project: Option<GpuiRemoteProjectReference>,
}

/// A page waiting for the Files view to sync to the current project.
#[derive(Clone, Debug)]
pub(crate) struct DocsPendingPage {
    pub(crate) source: DocsPageSource,
    pub(crate) title: String,
    /// The chat whose card opened it.
    pub(crate) origin: Option<TerminalSessionId>,
}

impl GhostexGpuiApp {
    /// Opens the page as a document (or selects it when it is already open). Waits for the
    /// project's files when Docs has not synced to the current project yet.
    pub(crate) fn native_docs_open_page(&mut self, page: DocsPendingPage, cx: &mut Context<Self>) {
        let current = self.native_docs_project();
        if current.is_none() || self.native_docs.project != current {
            self.native_docs.pending_page = Some(page);
            self.native_docs_notify(cx);
            return;
        }
        let Some(id) = crate::app::window::visual_page_modal::page_id(&page.source.url) else {
            return;
        };
        let path = format!("{PAGE_DOCUMENT_ROOT}/{id}.html");
        match self.native_docs.document_mut(&path) {
            Some(document) => {
                document.page = Some(page.source);
                document.origin_session = page.origin;
            }
            None => {
                let mut document = DocsDocument::new(path.clone(), page.title);
                document.kind = DocsFileKind::Page;
                document.page = Some(page.source);
                document.origin_session = page.origin;
                document.load = DocsDocumentLoad::Ready;
                self.native_docs.documents.push(document);
            }
        }
        self.native_docs.active = Some(path);
        self.native_docs_close_drawer(cx);
        self.native_docs_notify(cx);
    }

    /// The open document's page, when the open document is a published page.
    pub(crate) fn native_docs_active_page(&self) -> Option<&DocsPageSource> {
        let document = self.native_docs.active_document()?;
        (document.kind == DocsFileKind::Page)
            .then_some(document.page.as_ref())
            .flatten()
    }

    /// The address the browser area loads for the open page: with the page script and the app's
    /// theme (`visual_page_reader_url`).
    pub(crate) fn native_docs_page_browser_area_url(&self) -> Option<String> {
        let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
        crate::app::visual_page_modal_lifecycle::visual_page_reader_url(
            &self.native_docs_active_page()?.url,
            light,
        )
    }

    /// The browser that shows the open page in the Files view's browser area: the page and
    /// nothing of the app, in the floating page window's own browser profile. A link the page
    /// opens goes to the reader's browser, routed by the machine the page came from.
    pub(crate) fn native_docs_create_page_surface(
        &mut self,
        surface_id: String,
        parent_ns_view: *mut std::ffi::c_void,
        url: String,
        background: gpui::Hsla,
        page_load_end_handler: cef::PageLoadEndHandler,
        cx: &mut Context<Self>,
    ) -> Result<Entity<CefSurface>, String> {
        let remote_project = self
            .native_docs_active_page()
            .and_then(|page| page.remote_project.clone());
        let popup_open_handler: cef::BrowserPopupOpenHandler = {
            let app = cx.weak_entity();
            let async_cx = cx.to_async();
            let foreground = cx.foreground_executor().clone();
            Rc::new(move |requested_url, _| {
                let app = app.clone();
                let remote_project = remote_project.clone();
                let mut async_cx = async_cx.clone();
                // Run on the next turn: this is called from inside CEF's message pump.
                foreground
                    .spawn(async move {
                        let _ = app.update(&mut async_cx, |this, cx| {
                            this.open_visual_page_link(requested_url, remote_project, cx);
                        });
                    })
                    .detach();
            })
        };
        CefSurface::try_new(
            surface_id,
            parent_ns_view,
            url,
            crate::app::window::visual_page_modal::VISUAL_PAGE_CEF_PROFILE_ID.to_string(),
            pane_prepaint_background_color(),
            false,
            background,
            None,
            true,
            Some(popup_open_handler),
            None,
            None,
            None,
            None,
            None,
            Some(page_load_end_handler),
            None,
            cx,
        )
    }

    /// Opens the open page in the reader's browser (the header's Open in browser).
    pub(crate) fn native_docs_open_page_in_browser(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.native_docs_page_browser_area_url() else {
            return;
        };
        if let Err(error) = gpui_open_external_http_url(&url) {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "The page could not be opened in your browser",
                &error,
                cx,
            );
        }
    }
}
