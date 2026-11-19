//! How long one Settings open took, from the click to its first frame on screen: one
//! `gpui.settings.openTiming` line in the app-modal log (`gpui-app-modal-debug.jsonl`) while the
//! Modals and Settings diagnostic area is on, with each step's time since the open began and since
//! the step before it. Nothing is written while the steps happen, so the log adds no time to the
//! open.
//!
//! CDXC:Settings 2026-11-19 WHY:
//! User, on Windows: Settings showed its blurred background for a few seconds before its content. Built on its own (the `native-modal-demo` preview, with the user's saved settings) Settings put its first frame on screen in 110 to 560 ms on every page, so the time goes somewhere only the running app has; this line says which step it is: the gxserver read before the window (`hydrateRead`), building the window (`windowBuilt`), or waiting for its first frame (`firstFrame`, `firstFramePresented`).
//! SEE-ALSO: apps/desktop/src/app/window/visual_page_modal/open_timing.rs (the page windows' line).

use std::cell::RefCell;
use std::time::Instant;

struct OpenTiming {
    started: Instant,
    steps: Vec<(&'static str, f64)>,
}

thread_local! {
    static OPEN: RefCell<Option<OpenTiming>> = const { RefCell::new(None) };
}

/// A Settings open began (any of its entry points). An open still being timed is written first.
pub(crate) fn start() {
    finish("replaced", None);
    OPEN.with_borrow_mut(|open| {
        *open = Some(OpenTiming {
            started: Instant::now(),
            steps: Vec::new(),
        });
    });
}

/// `start`, unless an open is already being timed (its entry point started it earlier).
pub(crate) fn start_if_idle() {
    if OPEN.with_borrow(Option::is_none) {
        start();
    }
}

/// Records `step` the first time it happens during the open being timed; nothing otherwise.
pub(crate) fn mark(step: &'static str) {
    OPEN.with_borrow_mut(|open| {
        if let Some(open) = open
            && !open.steps.iter().any(|(seen, _)| *seen == step)
        {
            let ms = open.started.elapsed().as_secs_f64() * 1000.0;
            open.steps.push((step, (ms * 10.0).round() / 10.0));
        }
    });
}

/// Writes the open's line; `outcome` says how it ended (`shown`, `replaced`), `page` the page it
/// opened on.
pub(crate) fn finish(outcome: &'static str, page: Option<&'static str>) {
    let Some(open) = OPEN.with_borrow_mut(Option::take) else {
        return;
    };
    let mut previous = 0.0;
    let steps: Vec<_> = open
        .steps
        .iter()
        .map(|(step, ms)| {
            let since_step = ((ms - previous) * 10.0).round() / 10.0;
            previous = *ms;
            serde_json::json!({ "step": step, "ms": ms, "sinceStepMs": since_step })
        })
        .collect();
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::AppModal,
        "gpui.settings.openTiming",
        serde_json::json!({
            "outcome": outcome,
            "page": page,
            "glass": crate::app::helpers::window_glass_active(),
            "totalMs": (open.started.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
            "steps": steps,
        }),
    );
}

/// Marks the open window's first frame (the first frame request after it opened) and the one
/// after it (by then the first frame is on screen), then writes the line.
pub(crate) fn follow_first_frames(window: &mut gpui::Window, page: Option<&'static str>) {
    window.on_next_frame(move |window, _| {
        mark("firstFrame");
        window.on_next_frame(move |_, _| {
            mark("firstFramePresented");
            finish("shown", page);
        });
    });
}
