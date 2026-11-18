//! One-click crash reports for the desktop app.
//!
//! A crash is saved on disk as it happens (`capture`: a panic on the main thread, an uncaught
//! Objective-C exception) or read back at the next open from the file Ghostty's crash handler
//! wrote (`native`). The next time Ghostex opens it shows the crash window
//! (app/window/crash_report_modal/), and Send report posts the issue through gxserver's
//! `/api/sendFeedback` to the feedback service, which files it on GitHub.
//!
//! CDXC:CrashReports 2026-11-17 DECISION:
//! User: "on crashes we must have a crash handler that has a button to send an issue with the
//! crash details. so people can easily click 1 button to report the crash to us. and then we can
//! fix it (should include all the relevant info to fix the crash like os / version / etc". Picks
//! (docs/2026-11-17/release-safety-simple, Q3731 and Q3732): the report goes through the existing
//! feedback service at feedback.ghostex.dev (no GitHub account needed), and the crash window
//! shows the next time Ghostex opens. The window says what happened in one line, shows exactly
//! what is sent under "What's sent", and has Send report and Don't send. Home folders, emails,
//! tokens, keys and file paths are removed before anything is shown or sent. The same crash
//! (same version and place) carries the same crash id, so the service files it on one issue.
//! SEE-ALSO: server/src/feedback/relay.rs (the `crash` field passed to the service),
//! app/crash_report_modal_lifecycle.rs (when the window opens),
//! app/window/feedback_modal/offline_issue.rs (`scrub_personal_data`).

mod capture;
mod context;
mod native;
mod record;
mod report;
mod system;

use std::time::Duration;

use serde_json::json;

// record.rs, report.rs and system.rs reach the app only through these, so the native-modal-demo
// can build the same report (bin/native_modal_demo/crash_report_core.rs).
pub(crate) use crate::app::window::{FeedbackScrubContext, scrub_personal_data};

fn logs_directory() -> std::path::PathBuf {
    crate::support_logs::logs_directory()
}

pub(crate) use capture::{PendingCrash, collect_pending, record_panic, record_uncaught_exception};
pub(crate) use context::{note_event, note_screen};
pub(crate) use record::{CrashRecord, SentCrash};
pub(crate) use report::{CrashReport, build_report, summary_line};
pub(crate) use system::{SystemFacts, read_system_facts};

/// The feedback service gets 170 s from gxserver; a crash report has no images, so this is
/// generous.
const SEND_TIMEOUT: Duration = Duration::from_secs(60);

/// Posts `report` through gxserver and remembers it, so this crash is not offered again; the
/// pending crashes are cleared. Answers the issue it went to.
pub(crate) async fn send_report(
    report: &CrashReport,
    record: &CrashRecord,
) -> Result<SentCrash, String> {
    // CDXC:CrashReports 2026-11-17 WHY: `GHOSTEX_CRASH_REPORT_DRY_RUN=1` proves the whole send
    // without filing an issue: gxserver answers the request it would post (its `dryRun`), which
    // is saved beside the crash records, and the crash stays pending.
    let dry_run = std::env::var("GHOSTEX_CRASH_REPORT_DRY_RUN").as_deref() == Ok("1");
    let params = json!({
        "app": "desktop",
        "title": report.title,
        "body": report.body,
        "crash": {
            "fingerprint": report.fingerprint,
            "version": record.version,
            "location": report::short_location(record),
        },
        "dryRun": dry_run,
    });
    let answer =
        crate::app::gx_store::gx_rpc_with_timeout(None, "/api/sendFeedback", params, SEND_TIMEOUT)
            .await
            .map_err(|error| error.message)?;
    if dry_run {
        let path = record::crash_reports_dir().join("dry-run-request.json");
        let text = serde_json::to_string_pretty(&answer).unwrap_or_default();
        return Err(match std::fs::write(&path, text) {
            Ok(()) => format!(
                "Dry run: nothing was sent. The request is in {}.",
                path.display()
            ),
            Err(error) => {
                format!("Dry run: nothing was sent, and the request could not be saved: {error}")
            }
        });
    }
    let (Some(issue_number), Some(issue_url)) =
        (answer["issueNumber"].as_u64(), answer["issueUrl"].as_str())
    else {
        return Err("The report was sent, but no link came back.".to_string());
    };
    let sent = SentCrash {
        fingerprint: report.fingerprint.clone(),
        issue_number,
        issue_url: issue_url.to_string(),
        at_ms: record.at_ms,
    };
    record::remember_sent(sent.clone());
    record::clear_pending();
    Ok(sent)
}

/// Don't send: the pending crashes are dropped and not offered again.
pub(crate) fn dismiss_pending() {
    record::clear_pending();
}

/// For the "go back to the previous version" message's "Also send the crash report" checkbox:
/// sends the newest pending crash, with nothing for the person to review again (that message
/// already said what is sent). `Ok(None)` when there is no crash to send. Run it off the main
/// thread (`cx.background_executor().spawn(send_pending_crash_report(scrub))`); the scrub
/// context is `crate::app::feedback_modal_lifecycle::feedback_scrub_context()`.
pub(crate) async fn send_pending_crash_report(
    scrub: FeedbackScrubContext,
) -> Result<Option<SentCrash>, String> {
    let Some(pending) = collect_pending() else {
        return Ok(None);
    };
    let facts = read_system_facts();
    let report = build_report(&pending.record, pending.times, &facts, "", &scrub);
    send_report(&report, &pending.record).await.map(Some)
}
