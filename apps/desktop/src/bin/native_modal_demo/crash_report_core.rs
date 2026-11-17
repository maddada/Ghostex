//! The crash reporter's own report code (crash_report/record.rs, report.rs, system.rs) with the
//! two things it takes from the app: the feedback scrubber and the logs folder, here
//! `GHOSTEX_CRASH_REPORT_DEMO_LOGS` (a folder holding `crash-reports/pending-*.json`).
#[path = "../../app/window/feedback_modal/offline_issue.rs"]
mod offline_issue;
#[path = "../../crash_report/record.rs"]
pub(crate) mod record;
#[path = "../../crash_report/report.rs"]
pub(crate) mod report;
#[path = "../../crash_report/system.rs"]
pub(crate) mod system;

pub(crate) use offline_issue::{FeedbackScrubContext, scrub_personal_data};

fn logs_directory() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("GHOSTEX_CRASH_REPORT_DEMO_LOGS").unwrap_or_default())
}
