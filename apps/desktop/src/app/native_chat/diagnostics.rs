use crate::{shared_settings, support_logs};
use std::cell::RefCell;
use web_time::Instant;

struct FocusRequest {
    session_id: String,
    started: Instant,
    observed_paint: bool,
}

thread_local! {
    static REQUEST: RefCell<Option<FocusRequest>> = const { RefCell::new(None) };
}

fn enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot().debugging_mode()
        && support_logs::scenario_enabled(support_logs::GpuiDiagnosticScenario::SidebarRefresh)
}

pub(crate) fn focus_requested(session_id: &str) {
    REQUEST.with_borrow_mut(|pending| {
        *pending = enabled().then(|| FocusRequest {
            session_id: session_id.to_owned(),
            started: Instant::now(),
            observed_paint: false,
        });
    });
}

/// CDXC:SessionChat 2026-09-17 WHY: Sidebar focus state can update before transcript content arrives. Measure the matching chat's completed CPU paint separately; this marker does not measure GPU presentation or display scanout.
pub(super) fn content_frame_painted(
    session_id: &str,
    rows: usize,
    content_ready: bool,
    composer_ready: bool,
    pane_focused: bool,
    selected: impl FnOnce() -> bool,
) {
    REQUEST.with_borrow_mut(|pending| {
        let Some(request) = pending.as_mut().filter(|request| request.session_id == session_id) else {
            return;
        };
        if !enabled() { *pending = None; return; }
        let selected = selected();
        if !request.observed_paint {
            request.observed_paint = true;
            support_logs::append(support_logs::GpuiSupportLog::SidebarRefresh, "gpui.chat.focusFramePainted", serde_json::json!({
                "sessionId": session_id,
                "elapsedMs": request.started.elapsed().as_secs_f64() * 1000.0,
                "selected": selected, "paneFocused": pane_focused, "contentReady": content_ready, "rows": rows,
            }));
        }
        if !selected || !content_ready { return; }
        let request = pending.take().unwrap();
        if enabled() {
            support_logs::append(
                support_logs::GpuiSupportLog::SidebarRefresh,
                "gpui.chat.contentFrameReady",
                serde_json::json!({
                    "sessionId": session_id,
                    "elapsedMs": request.started.elapsed().as_secs_f64() * 1000.0,
                    "rows": rows,
                    "composerReady": composer_ready,
                    "phase": "cpuPaint",
                }),
            );
        }
    });
}
