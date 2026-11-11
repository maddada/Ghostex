//! How long one page window open took, from the chat card's click to the page's first load: one
//! `gpui.visualPage.openTiming` line in the app-modal log (`gpui-app-modal-debug.jsonl`), with
//! each step's time since the click and since the step before it. Nothing is written while the
//! steps happen, so the log adds no time to the open.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! User, on an overloaded Mac: opening a Ghostex page took more than ten seconds while the rest of
//! the app stayed fast, and nothing on his machine said which step was slow, because this line was
//! written only with Settings > Debugging on. It is now always written: one line per open, with
//! the page's id and its title's length, never the page, its answers or any token. The web
//! runtime's own start is the `gpui.cefRuntime.ready` line (cef/shell/lifecycle.rs).

use std::cell::RefCell;
use std::time::Instant;

struct OpenTiming {
    started: Instant,
    web_runtime_running: bool,
    page: Option<String>,
    title_chars: usize,
    steps: Vec<(&'static str, f64)>,
}

thread_local! {
    static OPEN: RefCell<Option<OpenTiming>> = const { RefCell::new(None) };
}

/// The card was clicked. `web_runtime_running`: the web runtime was already up, so no start
/// stands between the click and the page. `url` gives the page's id; `title` only its length.
pub(crate) fn start(web_runtime_running: bool, url: &str, title: &str) {
    finish("replaced");
    let page = url
        .split("/visual/")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_hexdigit)
                .collect::<String>()
        })
        .filter(|id| !id.is_empty());
    OPEN.with_borrow_mut(|open| {
        *open = Some(OpenTiming {
            started: Instant::now(),
            web_runtime_running,
            page,
            title_chars: title.chars().count(),
            steps: Vec::new(),
        });
    });
}

/// Records `step` the first time it happens during the open being timed.
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

/// Writes the open's line; `outcome` says how it ended (`loaded`, `closed`, ...).
pub(crate) fn finish(outcome: &'static str) {
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
    crate::support_logs::append_always(
        crate::support_logs::GpuiSupportLog::AppModal,
        "gpui.visualPage.openTiming",
        serde_json::json!({
            "outcome": outcome,
            "page": open.page,
            "titleChars": open.title_chars,
            "webRuntimeRunning": open.web_runtime_running,
            "totalMs": (open.started.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
            "steps": steps,
        }),
    );
}
