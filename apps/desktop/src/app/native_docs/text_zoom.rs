//! Keyboard zoom for the text of a file open in Files: Cmd+= / Cmd+- / Cmd+0 (Ctrl on Windows and
//! Linux) step the saved `filesZoomPercent` (Settings > Interface > Default files zoom), the way
//! the chat's chords step its zoom (`native_chat/zoom.rs`).
//!
//! The zoom scales `ui_fonts::files_text_size()` and `files_line_number_size()`, so the Markdown
//! body with its headings, code and tables, a text or code file, the gutter and the Changes diff
//! (which reads the same sizes) stay in proportion.

use gpui::{Context, Task};

use crate::GhostexGpuiApp;
use crate::app::native_chat::zoom::{ZoomStep, step_zoom_percent};
use crate::ui_fonts::{
    FILES_ZOOM_SETTING, set_pending_files_zoom_percent, sync_files_zoom,
    take_pending_files_zoom_percent,
};

/// Matches the chat's zoom save: a held chord shows every rung at once and saves once, after the
/// keys go quiet.
const ZOOM_SAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(180);

/// The one debounced save; a later press replaces it.
struct FilesZoomSave(#[allow(dead_code)] Task<()>);
impl gpui::Global for FilesZoomSave {}

impl GhostexGpuiApp {
    /// Steps the Files text zoom when the open document shows as text; false otherwise (a picture,
    /// a browser area, a table view), so the chord goes on to the surface that owns it.
    ///
    /// CDXC:Docs 2026-11-02 DECISION:
    /// User (on issue #388, which asked for Ctrl+= to zoom a file's text in Files): "do this one, make it match how chat does this". The chords step one saved `filesZoomPercent` on the chat's rungs (70% to 200%), Cmd+0 (Ctrl+0) sets 100%, the save is debounced like the chat's, and every open Files document (and the Changes diff, which reads the same sizes) follows at once. Terminal, browser and CEF panes keep their own zoom.
    pub(crate) fn native_docs_step_text_zoom(
        &mut self,
        step: ZoomStep,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.native_docs_text_document_open() {
            return false;
        }
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let current = sync_files_zoom(settings.object());
        let next = step_zoom_percent(current, step);
        if next == current {
            // At the end of the range the chord is still Files', not the terminal's.
            return true;
        }
        set_pending_files_zoom_percent(Some(next));
        // Every Files document and Changes view redraws at the new size now.
        cx.refresh_windows();
        let save = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ZOOM_SAVE_DEBOUNCE).await;
            cx.update(|cx| {
                // Saved through the Settings patch, which updates the shared settings snapshot in
                // the same effect cycle, so no frame paints the old value between.
                if let Some(percent) = take_pending_files_zoom_percent() {
                    let _ = this.update(cx, |this, cx| {
                        this.handle_gpui_app_modal_update_settings_patch_message(
                            &serde_json::json!({
                                "patch": { FILES_ZOOM_SETTING: percent },
                                "source": "files:zoom",
                            }),
                            cx,
                        );
                    });
                }
                cx.refresh_windows();
            });
        });
        cx.set_global(FilesZoomSave(save));
        true
    }

    /// The active document draws as text: Markdown, a text or code file (not while a CSV or JSON
    /// file shows as its table or tree), or an SVG's source.
    fn native_docs_text_document_open(&self) -> bool {
        use super::state::DocsFileKind;
        self.native_docs
            .active_document()
            .is_some_and(|document| match document.kind {
                DocsFileKind::Markdown => true,
                DocsFileKind::Text => !document.data_view_open,
                DocsFileKind::Image => document.svg_source,
                _ => false,
            })
    }
}
