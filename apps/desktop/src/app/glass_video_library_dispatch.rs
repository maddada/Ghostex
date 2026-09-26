//! Settings -> Theme -> Transparency -> Video: the settings page's requests about the glass video
//! library (list, download, cancel, remove), answered over the app-modal bridge. The work itself is
//! in helpers/glass_video_library.rs.

use std::time::{Duration, Instant};

use futures::StreamExt as _;

use crate::GhostexGpuiApp;
use crate::app::helpers::glass_video_library as library;

/// How often a running download reports its progress to the settings page.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

impl GhostexGpuiApp {
    /// Reads the catalogue (network first, then the saved copy) off the main thread and answers
    /// with the whole library.
    pub(crate) fn handle_gpui_list_glass_video_library_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let listing = background
                .spawn(async move {
                    let (catalogue, online) = library::fetch_catalogue();
                    library::library_listing(&catalogue, online)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(listing, cx);
            });
        })
        .detach();
    }

    /// Starts downloading one catalogue video. Progress goes to the settings page a few times a
    /// second; the end reports success or the reason, then the refreshed library.
    pub(crate) fn handle_gpui_download_glass_video_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(id) = message["id"].as_str().map(str::to_string) else {
            return;
        };
        let Some(video) = library::last_catalogue()
            .into_iter()
            .find(|video| video.id == id)
        else {
            self.dispatch_open_gpui_app_modal_message(
                serde_json::json!({
                    "type": "glassVideoDownloadFinished",
                    "id": id,
                    "ok": false,
                    "error": "This video is no longer in the library.",
                }),
                cx,
            );
            return;
        };
        let Some(cancel) = library::begin_download(&id) else {
            return;
        };
        let (progress_tx, mut progress_rx) =
            futures::channel::mpsc::unbounded::<(u64, Option<u64>)>();
        let progress_id = id.clone();
        cx.spawn(async move |this, cx| {
            while let Some((received, total)) = progress_rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_open_gpui_app_modal_message(
                        serde_json::json!({
                            "type": "glassVideoDownloadProgress",
                            "id": progress_id,
                            "received": received,
                            "total": total,
                        }),
                        cx,
                    );
                });
            }
        })
        .detach();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let download_id = id.clone();
            let result = background
                .spawn(async move {
                    let mut last_report = Instant::now() - PROGRESS_INTERVAL;
                    let result = library::download_video(&video, &cancel, |received, total| {
                        let finished = total.is_some_and(|total| received >= total);
                        if finished || last_report.elapsed() >= PROGRESS_INTERVAL {
                            last_report = Instant::now();
                            let _ = progress_tx.unbounded_send((received, total));
                        }
                    });
                    library::end_download(&download_id);
                    result
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let (ok, error) = match result {
                    Ok(()) => (true, None),
                    Err(error) => (false, Some(error)),
                };
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "type": "glassVideoDownloadFinished",
                        "id": id,
                        "ok": ok,
                        "error": error,
                    }),
                    cx,
                );
                let listing =
                    library::library_listing(&library::last_catalogue(), library::last_online());
                this.dispatch_open_gpui_app_modal_message(listing, cx);
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_cancel_glass_video_download_message(
        &mut self,
        message: &serde_json::Value,
    ) {
        if let Some(id) = message["id"].as_str() {
            library::cancel_download(id);
        }
    }

    /// Deletes a downloaded video. A glass that was playing it re-reads the settings, finds the
    /// video gone and shows the live blur until another one is chosen.
    pub(crate) fn handle_gpui_remove_glass_video_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(id) = message["id"].as_str() else {
            return;
        };
        let result = library::remove_video(id);
        if let Err(error) = &result {
            self.dispatch_open_gpui_app_modal_message(
                serde_json::json!({ "type": "glassVideoRemoveFailed", "id": id, "error": error }),
                cx,
            );
        }
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        crate::app::helpers::refresh_window_glass(snapshot.object());
        let listing = library::library_listing(&library::last_catalogue(), library::last_online());
        self.dispatch_open_gpui_app_modal_message(listing, cx);
        cx.notify();
    }
}
