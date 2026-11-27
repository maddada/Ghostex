//! The one-time note after the Community Canvas extension's boards became drawings
//! (server/src/extensions/canvas_migration.rs).
//!
//! CDXC:Canvas 2026-11-27 DECISION:
//! User (canvas page Q3 A): after the move "a one-time note says where they went", drawn on the page
//! as "Canvas is built in now. Your 3 boards are drawings in Canvas. The extension was removed." with
//! Open Canvas. Background pictures were not picked for the built-in, so the note says they were not
//! kept only when a board had one. Each project's Canvas opens first on the board that was open last
//! in the extension.

use std::time::Duration;

use gpui::Context;
use serde_json::json;

use super::{read_last_drawing, write_last_drawing};
use crate::app::window::toast::{
    GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel,
};
use crate::*;

pub(crate) const CANVAS_MOVED_NOTE_ID: &str = "canvas-moved";

/// The note's words for `boards` moved boards, and whether a picture was left behind.
pub(crate) fn canvas_moved_note_text(boards: u64, dropped_pictures: bool) -> String {
    let mut text = match boards {
        0 => "The extension was removed.".to_string(),
        1 => "Your board is a drawing in Canvas. The extension was removed.".to_string(),
        n => format!("Your {n} boards are drawings in Canvas. The extension was removed."),
    };
    if dropped_pictures {
        text.push_str(" Background pictures were not kept.");
    }
    text
}

impl GhostexGpuiApp {
    pub(crate) fn canvas_show_moved_note(
        &mut self,
        note: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if self.canvas.note_shown {
            return;
        }
        self.canvas.note_shown = true;
        if let Some(last) = note.get("lastDrawings").and_then(|value| value.as_object()) {
            for (project_id, file) in last {
                if let Some(file) = file.as_str()
                    && read_last_drawing(project_id).is_none()
                {
                    write_last_drawing(project_id, file);
                }
            }
        }
        let boards = note
            .get("boards")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let pictures = note
            .get("droppedPictures")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                copy_text: None,
                id: CANVAS_MOVED_NOTE_ID.to_string(),
                level: GpuiAppToastLevel::from_raw(Some("info")),
                title: "Canvas is built in now".to_string(),
                description: Some(canvas_moved_note_text(boards, pictures)),
                loading: false,
                persistent: true,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                action: Some(GpuiAppToastAction::OpenCanvas),
                epoch: 0,
            },
            cx,
        );
        // Shown: gxserver keeps it from coming back.
        cx.background_executor()
            .spawn(async move {
                let _ = gpui_gxserver_rpc_result(
                    "/api/listExtensions",
                    &json!({ "canvasMovedNoteShown": true }),
                    Duration::from_secs(5),
                );
            })
            .detach();
    }

    /// The note's Open Canvas.
    pub(crate) fn open_canvas_from_note(&mut self, cx: &mut Context<Self>) {
        let Some(id) = ExtensionId::new(super::CANVAS_VIEW_ID) else {
            return;
        };
        self.defer_in_main_window(cx, move |app, window, cx| {
            app.open_view_tab(TitlebarMode::Extension(id), window, cx);
        });
    }
}
