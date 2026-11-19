//! TEMPORARY (2026-11-19): remove with `gpui::redraw_census` once the Windows redraw investigation
//! is done. While Show debug UI controls and the "App and server lifecycle" diagnostic area are on,
//! counts what made each window draw again and writes one `gpui.redrawCensus` line to
//! `gpui-performance.jsonl` every few seconds: frames drawn, and for each cause (the view and source
//! line that called `notify`, `refresh`, `request_animation_frame` or `refresh_windows`) how many
//! of those frames it was pending for. Nothing is counted while the area is off.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::json;

use crate::GhostexGpuiApp;
use crate::support_logs::{self, GpuiSupportLog};

/// How often a line is written while the census runs.
const EVERY: Duration = Duration::from_secs(5);
/// The existing diagnostic area that turns the census on.
const SCENARIO: &str = "native.host.lifecycle";
/// The causes a line lists, most frames first.
const TOP_CAUSES: usize = 30;

/// One loop for the whole app, whichever workspace window starts it first.
static STARTED: AtomicBool = AtomicBool::new(false);

impl GhostexGpuiApp {
    pub(crate) fn start_redraw_census(&mut self, cx: &mut gpui::Context<Self>) {
        if STARTED.swap(true, Ordering::Relaxed) {
            return;
        }
        cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor().timer(EVERY).await;
                if !support_logs::scenario_writes(SCENARIO) {
                    gpui::redraw_census::set_enabled(false);
                    continue;
                }
                if !gpui::redraw_census::enabled() {
                    // Counting starts now; the first line covers a whole interval.
                    gpui::redraw_census::set_enabled(true);
                    continue;
                }
                let Some(report) = gpui::redraw_census::take_report() else {
                    continue;
                };
                let seconds = EVERY.as_secs_f64();
                let list = |causes: &[(String, u64)], key: &str| {
                    causes
                        .iter()
                        .take(TOP_CAUSES)
                        .map(|(cause, count)| json!({ "cause": cause, key: count }))
                        .collect::<Vec<_>>()
                };
                support_logs::append_for_scenario(
                    GpuiSupportLog::Performance,
                    SCENARIO,
                    "gpui.redrawCensus",
                    json!({
                        "seconds": seconds,
                        "frames": report.frames,
                        "framesPerSecond": report.frames as f64 / seconds,
                        "framesByWindow": report
                            .frames_by_window
                            .iter()
                            .map(|(_, frames)| *frames)
                            .collect::<Vec<_>>(),
                        "frameCauses": list(&report.frames_by_cause, "frames"),
                        "calls": list(&report.calls_by_cause, "calls"),
                    }),
                );
            }
        })
        .detach();
    }
}
