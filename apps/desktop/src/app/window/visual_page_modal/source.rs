//! The page window's Refresh: while the window is open it asks the page's gxserver every few
//! seconds whether the HTML file the page was published from changed on disk, shows a refresh
//! button at the top right when it did, and on a click has gxserver publish the file again under
//! the same address and reloads the page, whose script returns to the scroll offset it was at.
//! The same answer says whether a question on the page is still unanswered (`questionsLeft`) and
//! how many of its questions are answered (`questionsAnswered` of `questionsTotal`), for the page's
//! pill in the chat's pills bar while it is hidden (app/visual_page_hidden.rs).
//!
//! CDXC:SessionChat 2026-10-22 DECISION:
//! User: "when the page's file changes on disk while the window is open, show a refresh button at the top right; clicking it reloads the page (and keeps the scroll position where it can). Don't reload on its own." gxserver owns the file and the check (server/src/visual_pages/source.rs), so a remote machine's page is checked on that machine.

use super::{GpuiVisualPageModalWindow, VisualPageModalCommand};
use crate::app::gx_store::gx_rpc;
use gpui::Context;
use serde_json::json;
use std::time::Duration;

/// How often the open window asks whether the page's file changed.
const SOURCE_CHECK_EVERY: Duration = Duration::from_secs(2);

/// The page saves its draft synchronously (the page API's `__flush`), then reloads.
const RELOAD_AFTER_FLUSH: &str =
    "try{window.ghostex&&ghostex.__flush&&ghostex.__flush();}finally{location.reload();}";

/// `(answered, total)` of the page's questions, when it has any (a gxserver from 2026-11-11 on
/// says; an older one leaves both out).
fn question_counts(status: &serde_json::Value) -> Option<(u64, u64)> {
    let total = status["questionsTotal"]
        .as_u64()
        .filter(|total| *total > 0)?;
    Some((
        status["questionsAnswered"].as_u64().unwrap_or(0).min(total),
        total,
    ))
}

impl GpuiVisualPageModalWindow {
    /// Starts asking; stops for a page published without its file's path (an older `ghostex show`)
    /// or one gxserver no longer has, unless the answer also says whether questions are left
    /// (a gxserver from 2026-11-11 on), which is asked for as long as the window lives.
    pub(super) fn watch_page_source(&mut self, cx: &mut Context<Self>) {
        let Some(page_id) = self.page_id.clone() else {
            return;
        };
        let remote = self.remote.clone();
        self._source_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SOURCE_CHECK_EVERY).await;
                let status = gx_rpc(
                    remote.clone(),
                    "/api/readVisualPageSource",
                    json!({ "pageId": page_id }),
                )
                .await;
                let Ok(status) = status else {
                    // gxserver restarting or the tunnel down: ask again on the next turn.
                    continue;
                };
                let questions_left = status["questionsLeft"].as_bool();
                let counts = question_counts(&status);
                if status["refreshable"] != true && questions_left.is_none() {
                    return;
                }
                let changed = status["refreshable"] == true && status["changed"] == true;
                let updated = this.update(cx, |this, cx| {
                    if this.source_changed != changed && !this.refreshing {
                        this.source_changed = changed;
                        cx.notify();
                    }
                    if let Some(left) = questions_left {
                        this.note_questions_left(left, counts, cx);
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
        }));
    }

    fn note_questions_left(
        &mut self,
        left: bool,
        counts: Option<(u64, u64)>,
        cx: &mut Context<Self>,
    ) {
        if self.questions_left != left || self.question_counts != counts {
            self.questions_left = left;
            self.question_counts = counts;
            (self.host)(VisualPageModalCommand::TabChanged, cx);
        }
    }

    /// Asks once, a moment after the page was told to save its draft (`set_hidden`), so the
    /// answer counts the reader's last pick.
    pub(super) fn check_questions_left_soon(&mut self, cx: &mut Context<Self>) {
        let Some(page_id) = self.page_id.clone() else {
            return;
        };
        let remote = self.remote.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let Ok(status) = gx_rpc(
                remote,
                "/api/readVisualPageSource",
                json!({ "pageId": page_id }),
            )
            .await
            else {
                return;
            };
            if let Some(left) = status["questionsLeft"].as_bool() {
                let counts = question_counts(&status);
                let _ = this.update(cx, |this, cx| this.note_questions_left(left, counts, cx));
            }
        })
        .detach();
    }

    /// The refresh button: publish the file again, then reload where the reader was.
    pub(super) fn refresh_page(&mut self, cx: &mut Context<Self>) {
        let Some(page_id) = self.page_id.clone() else {
            return;
        };
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        cx.notify();
        let remote = self.remote.clone();
        cx.spawn(async move |this, cx| {
            let refreshed = gx_rpc(
                remote,
                "/api/refreshVisualPage",
                json!({ "pageId": page_id }),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                this.refreshing = false;
                match refreshed {
                    Ok(_) => {
                        this.source_changed = false;
                        // The page's script saves its scroll as it goes and puts it back once the
                        // page has loaded again (page.rs). The page reloads itself after saving its
                        // draft, so the reloaded page is served what the reader typed last.
                        if let Some(surface) = &this.surface {
                            surface.update(cx, |surface, _| {
                                surface.execute_app_owned_script(RELOAD_AFTER_FLUSH)
                            });
                        }
                    }
                    Err(error) => (this.host)(
                        VisualPageModalCommand::RefreshFailed {
                            message: error.message,
                        },
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
