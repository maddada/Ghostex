//! Preview of "Ghostex closed unexpectedly" (crash_report_modal.rs), built from a real crash record:
//! the newest `pending-*.json` under `GHOSTEX_CRASH_REPORT_DEMO_LOGS/crash-reports/`, this
//! computer's system details and the app's own scrubber. `GHOSTEX_CRASH_REPORT_DEMO_REQUEST=<file>`
//! writes the `/api/sendFeedback` request the app would make for it.
//! States: the default (What's sent closed), `expanded`, `sent`, `failed`.
use super::crash_report_core::{FeedbackScrubContext, record, report, system};
use super::crash_report_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

fn scrub_context() -> FeedbackScrubContext {
    let variable = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    FeedbackScrubContext {
        home_dirs: ["HOME"].into_iter().filter_map(variable).collect(),
        private_words: ["USER", "LOGNAME"]
            .into_iter()
            .filter_map(variable)
            .collect(),
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let Some((_, crash)) = record::pending().into_iter().next() else {
        eprintln!("no pending crash under GHOSTEX_CRASH_REPORT_DEMO_LOGS/crash-reports");
        cx.quit();
        return;
    };
    let times = record::pending()
        .iter()
        .filter(|(_, other)| other.fingerprint() == crash.fingerprint())
        .count();
    let facts = system::read_system_facts();
    let scrub = scrub_context();
    let built = report::build_report(&crash, times, &facts, "", &scrub);
    if let Ok(path) = std::env::var("GHOSTEX_CRASH_REPORT_DEMO_REQUEST") {
        // Exactly what crash_report::send_report posts to gxserver.
        let request = serde_json::json!({
            "app": "desktop",
            "title": built.title,
            "body": built.body,
            "crash": {
                "fingerprint": built.fingerprint,
                "version": crash.version,
                "location": report::short_location(&crash),
            },
        });
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(&request).unwrap_or_default(),
        );
        eprintln!("wrote {path}");
    }
    let summary = report::summary_line(&crash, &scrub);
    let report_for_note = Rc::new(move |note: &str| {
        let built = report::build_report(&crash, times, &facts, note, &scrub);
        CrashReportPreview {
            title: built.title,
            body: built.body,
            removed: built.removed,
        }
    });
    let config = CrashReportModalConfig {
        palette: demo.palette,
        summary,
        report_for_note,
    };
    let host: CrashReportModalHost = Rc::new(|command, cx: &mut App| {
        eprintln!("{command:?}");
        if !matches!(
            command,
            CrashReportModalCommand::Send { .. } | CrashReportModalCommand::OpenIssue(_)
        ) {
            cx.quit();
        }
    });
    let state = demo.state.clone();
    super::open_modal_window(
        CRASH_REPORT_MODAL_WIDTH,
        CRASH_REPORT_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut view = GpuiCrashReportModalWindow::new(config, host, window, cx);
                match state.as_str() {
                    "expanded" => view.set_expanded(true, cx),
                    // A made-up issue number: the demo never sends anything.
                    "sent" => view.set_sent(
                        1234,
                        "https://github.com/maddada/Ghostex/issues/1234".to_string(),
                        window,
                        cx,
                    ),
                    "failed" => view.set_failed(
                        "Could not reach the feedback service. Check your internet connection and try again.".to_string(),
                        cx,
                    ),
                    _ => {}
                }
                view
            })
        },
        cx,
    );
}
