//! When the crash window opens and what its buttons do: once per launch, from the lead window, a
//! moment after it shows, when the last run crashed (crash_report/, CDXC:CrashReports
//! 2026-11-17 DECISION there). Send report posts through gxserver (`crash_report::send_report`).
use crate::app::window::*;
use crate::crash_report::{CrashReport, PendingCrash, SystemFacts};
use crate::*;
use std::rc::Rc;
use std::time::Duration;

/// Lets the main window draw first, so the crash window opens over it and not before it.
const OPEN_AFTER: Duration = Duration::from_millis(1500);
/// `GHOSTEX_CRASH_REPORT_TEST=panic` panics on the main thread this long after opening.
const TEST_CRASH_AFTER: Duration = Duration::from_secs(4);

impl GhostexGpuiApp {
    /// Offers the last run's crash, if there was one. Reading the crash files and the system
    /// details spawns processes and reads the disk, so it runs in the background.
    pub(crate) fn offer_pending_crash_report(&mut self, cx: &mut gpui::Context<Self>) {
        // CDXC:CrashReports 2026-11-17 WHY: a developer switch to prove the crash window end to
        // end (a real panic on the main thread, then the window at the next open) without
        // breaking anything on purpose in the code.
        if std::env::var("GHOSTEX_CRASH_REPORT_TEST").as_deref() == Ok("panic") {
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(TEST_CRASH_AFTER).await;
                panic!("Test crash requested with GHOSTEX_CRASH_REPORT_TEST=panic");
            })
            .detach();
        }
        let found = cx.background_executor().spawn(async move {
            let crash = crate::crash_report::collect_pending()?;
            Some((crash, crate::crash_report::read_system_facts()))
        });
        cx.spawn(async move |this, cx| {
            let Some((crash, facts)) = found.await else {
                return;
            };
            cx.background_executor().timer(OPEN_AFTER).await;
            let _ = this.update(cx, |app, cx| {
                app.open_gpui_crash_report_modal(crash, facts, cx)
            });
        })
        .detach();
    }

    fn open_gpui_crash_report_modal(
        &mut self,
        crash: PendingCrash,
        facts: SystemFacts,
        cx: &mut gpui::Context<Self>,
    ) {
        // Another dialog is up (first-run setup, the go back message): the crash stays saved and
        // is offered at the next open instead of replacing it.
        if self.native_app_modal_kind().is_some() {
            return;
        }
        let scrub = crate::app::feedback_modal_lifecycle::feedback_scrub_context();
        let summary = crate::crash_report::summary_line(&crash.record, &scrub);
        let report_for_note = {
            let crash = crash.clone();
            Rc::new(move |note: &str| {
                let report = crate::crash_report::build_report(
                    &crash.record,
                    crash.times,
                    &facts,
                    note,
                    &scrub,
                );
                CrashReportPreview {
                    title: report.title,
                    body: report.body,
                    removed: report.removed,
                }
            })
        };
        let config = CrashReportModalConfig {
            palette: self.gpui_native_dialog_palette(),
            summary,
            report_for_note: report_for_note.clone(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| match command {
            CrashReportModalCommand::Send { note } => {
                let preview = report_for_note(&note);
                app.send_gpui_crash_report(crash.clone(), preview, cx);
            }
            CrashReportModalCommand::DontSend => {
                crate::crash_report::dismiss_pending();
                app.release_native_app_modal_window(GpuiAppModalKind::CrashReport, cx);
            }
            CrashReportModalCommand::OpenIssue(url) => {
                crate::app::helpers::gpui_open_external_url(&url, cx);
            }
            CrashReportModalCommand::Closed | CrashReportModalCommand::Done => {
                app.release_native_app_modal_window(GpuiAppModalKind::CrashReport, cx);
            }
        });
        self.open_native_app_modal(
            GpuiAppModalKind::CrashReport,
            CRASH_REPORT_MODAL_WIDTH,
            CRASH_REPORT_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiCrashReportModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Posts the report the window showed and tells the window how it went.
    fn send_gpui_crash_report(
        &mut self,
        crash: PendingCrash,
        preview: CrashReportPreview,
        cx: &mut gpui::Context<Self>,
    ) {
        let report = CrashReport {
            title: preview.title,
            body: preview.body,
            fingerprint: crash.record.fingerprint(),
            removed: preview.removed,
        };
        let sending = cx
            .background_executor()
            .spawn(async move { crate::crash_report::send_report(&report, &crash.record).await });
        cx.spawn(async move |this, cx| {
            let result = sending.await;
            let _ = this.update(cx, |app, cx| {
                app.update_native_app_modal(
                    GpuiAppModalKind::CrashReport,
                    cx,
                    |modal: &mut GpuiCrashReportModalWindow, window, cx| match result {
                        Ok(sent) => modal.set_sent(sent.issue_number, sent.issue_url, window, cx),
                        Err(reason) => modal.set_failed(reason, cx),
                    },
                );
            });
        })
        .detach();
    }
}
