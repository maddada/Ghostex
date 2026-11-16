//! Keeps SpaceO healthy on this Mac (packages/spaceo-heal, CDXC:ManagedTools 2026-11-16): a
//! check soon after launch and whenever SpaceO's daemon log shows a failure, the safe fixes done
//! on their own, the "SpaceO is stuck" notice with Repair, and the Repair job itself, which runs
//! only from the user's click (the notice's button or Settings > Integrations > SpaceO).

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use ghostex_spaceo_heal::{Health, LogCursor, Outcome, Spaceo, State};

use crate::app::helpers::*;
use crate::app::window::{
    GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel,
};
use crate::*;

const NOTICE_ID: &str = "spaceo-health-notice";
/// Lets the window draw before the first check.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(15);
/// How often SpaceO's log is read for new failures.
const LOG_POLL: Duration = Duration::from_secs(30);
/// A failure in the log checks SpaceO at most this often.
const FAILURE_CHECK_GAP: Duration = Duration::from_secs(60);
/// Without failures SpaceO is still checked this often (a daemon that died quietly).
const QUIET_CHECK_GAP: Duration = Duration::from_secs(15 * 60);
/// A closed notice comes back only after this long, and only when agents are still refused.
const RENOTIFY_GAP: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
struct Watch {
    cursor: LogCursor,
    last_check: Option<Instant>,
    last_state: Option<State>,
    notified_at: Option<Instant>,
}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);

/// What one check found and did.
struct Checked {
    health: Health,
    actions: Vec<String>,
    previous: Option<State>,
    /// Whether the notice should be (re)shown now.
    notify: bool,
}

/// This Mac's SpaceO, when it is installed and supported.
pub(crate) fn gpui_spaceo_heal_target() -> Option<Spaceo> {
    Some(Spaceo {
        exe: gpui_spaceo_executable_path()?,
        home: gpui_home_dir(),
        state_dir: ghostex_state_root(),
    })
}

/// SpaceO's health for Settings' `spaceoHealth` field (read-only, about 0.2 s).
pub(crate) fn gpui_spaceo_health_json() -> Option<serde_json::Value> {
    let spaceo = gpui_spaceo_heal_target()?;
    let mut health = spaceo.check().json();
    health["repairRunning"] = serde_json::json!(spaceo.repair_running() || SPACEO_JOB.running());
    Some(health)
}

/// One watch tick, blocking: reads the new log lines and, when a check is due, checks SpaceO and
/// does the safe fixes. `None` when no check was due.
fn watch_tick(first: bool) -> Option<Checked> {
    let spaceo = gpui_spaceo_heal_target()?;
    let mut guard = WATCH.lock().ok()?;
    let watch = guard.get_or_insert_with(Watch::default);
    let new_lines = watch.cursor.read_new(&spaceo.daemon_log());
    let failure = ghostex_spaceo_heal::log_shows_failure(&new_lines);
    let since_check = watch.last_check.map(|at| at.elapsed());
    let due = first
        || since_check.is_none_or(|elapsed| {
            elapsed >= QUIET_CHECK_GAP || (failure && elapsed >= FAILURE_CHECK_GAP)
        });
    if !due || SPACEO_JOB.running() || spaceo.repair_running() {
        return None;
    }
    watch.last_check = Some(Instant::now());
    let mut health = spaceo.check();
    let actions = ghostex_spaceo_heal::auto_heal(&spaceo, &health, "app");
    if !actions.is_empty() {
        if health.state == State::DaemonDown {
            let _ = gpui_run_command_with_captured_output_timeout(
                &spaceo.exe,
                &["daemon", "wait", "--timeout", "10"],
                Duration::from_secs(15),
                4 * 1024,
            );
        }
        health = spaceo.check();
    }
    let previous = watch.last_state.replace(health.state);
    let needs_user = matches!(health.state, State::Stuck | State::DaemonDown);
    if needs_user && previous != Some(health.state) {
        spaceo.log(
            "app",
            &format!("Noticed: {} ({:?})", health.summary(), health.spaceo_reason),
        );
    }
    let notify = needs_user
        && (previous != Some(health.state)
            || (failure
                && watch
                    .notified_at
                    .is_none_or(|at| at.elapsed() >= RENOTIFY_GAP)));
    if notify {
        watch.notified_at = Some(Instant::now());
    }
    Some(Checked {
        health,
        actions,
        previous,
        notify,
    })
}

fn notice_details(health: &Health) -> String {
    let mut lines = vec![health.summary()];
    if let Some(reason) = &health.spaceo_reason {
        lines.push(format!("SpaceO's reason: {reason}"));
    }
    if let Some(since) = health.since {
        lines.push(format!(
            "Latched since: {}",
            ghostex_spaceo_heal::iso_time(since)
        ));
    }
    if let Some(spaceo) = gpui_spaceo_heal_target() {
        lines.push(format!(
            "Ghostex's SpaceO log: {}",
            spaceo.heal_log().display()
        ));
    }
    lines.push(
        "Repair runs SpaceO's operator recovery: it takes com.spaceo.daemon out of launchd, stops the daemon and SpaceO Viewer, runs `spaceo safety clear-host-health --operator`, and starts SpaceO again. Agents never run it."
            .to_string(),
    );
    lines.join("\n")
}

impl GhostexGpuiApp {
    /// Starts the SpaceO watch on the lead window: a check soon after launch, then the log
    /// followed every 30 seconds. Does nothing where SpaceO can't run.
    pub(crate) fn start_gpui_spaceo_health_watch(&mut self, cx: &mut gpui::Context<Self>) {
        if !gpui_spaceo_supported() {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            background.timer(FIRST_CHECK_DELAY).await;
            let mut first = true;
            loop {
                let checked = background.spawn(async move { watch_tick(first) }).await;
                first = false;
                if let Some(checked) = checked {
                    let alive = this.update(cx, |this, cx| {
                        this.show_gpui_spaceo_health(checked, cx);
                    });
                    if alive.is_err() {
                        break;
                    }
                }
                background.timer(LOG_POLL).await;
            }
        })
        .detach();
    }

    fn show_gpui_spaceo_health(&mut self, checked: Checked, cx: &mut gpui::Context<Self>) {
        let Checked {
            health,
            actions,
            previous,
            notify,
        } = checked;
        match health.state {
            State::Stuck | State::DaemonDown if notify => {
                self.show_gpui_spaceo_stuck_notice(&health, cx);
            }
            State::Healthy => {
                self.remove_gpui_app_toast(NOTICE_ID, cx);
                let restarted = actions
                    .iter()
                    .any(|action| action.starts_with("Started SpaceO"));
                if restarted || matches!(previous, Some(State::DaemonDown)) {
                    self.dispatch_gpui_app_modal_toast(
                        "success",
                        "SpaceO started again",
                        "Its background service had stopped. Agents can get screens again.",
                        cx,
                    );
                }
            }
            _ => {}
        }
    }

    fn show_gpui_spaceo_stuck_notice(&mut self, health: &Health, cx: &mut gpui::Context<Self>) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: NOTICE_ID.to_string(),
                level: GpuiAppToastLevel::Warning,
                title: health.title.clone(),
                description: Some(health.detail.clone()),
                copy_text: Some(notice_details(health)),
                action: Some(GpuiAppToastAction::RepairSpaceo),
                loading: false,
                persistent: true,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    /// Repair's own toast, updated in place: while it runs it spins with the current step, and
    /// its result replaces it (`spaceo_repair_result_toast`).
    fn show_gpui_spaceo_repair_step(&mut self, step: &str, cx: &mut gpui::Context<Self>) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: NOTICE_ID.to_string(),
                level: GpuiAppToastLevel::Info,
                title: "Repairing SpaceO".to_string(),
                description: Some(step.to_string()),
                copy_text: None,
                action: None,
                loading: true,
                persistent: true,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    /// CDXC:ManagedTools 2026-11-16 DECISION:
    /// User: the toast said "Settings shows each step" and he only saw a toast. Repair's toast is
    /// its progress: one toast (the stuck notice's id) shows each step as it runs and then becomes
    /// the result, so nothing sends the user to Settings. Settings keeps its job row for whoever
    /// has it open. "SpaceO is still stuck" carries Repair again, so it ends the toast and waits
    /// in the notice modal instead (toasts never carry buttons).
    /// Repair, from the user's click only (the notice's Repair, Settings' Repair button).
    pub(crate) fn repair_gpui_spaceo(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(spaceo) = gpui_spaceo_heal_target() else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "SpaceO isn't installed",
                "Install it from Settings > Integrations.",
                cx,
            );
            return;
        };
        if let Err(message) = SPACEO_JOB.begin("repair") {
            self.dispatch_gpui_app_modal_toast("warning", "SpaceO is busy", &message, cx);
            return;
        }
        self.dispatch_gpui_install_job_progress(cx);
        let first_step = "Checking SpaceO…";
        self.show_gpui_spaceo_repair_step(first_step, cx);
        let current_step = Arc::new(Mutex::new(first_step.to_string()));
        let background = cx.background_executor().clone();
        let pump = background.clone();
        let pump_step = current_step.clone();
        cx.spawn(async move |this, cx| {
            let mut shown = first_step.to_string();
            loop {
                pump.timer(Duration::from_millis(250)).await;
                if !SPACEO_JOB.running() {
                    break;
                }
                let step = pump_step
                    .lock()
                    .map(|step| step.clone())
                    .unwrap_or_default();
                let changed = step != shown;
                let updated = this.update(cx, |this, cx| {
                    this.dispatch_gpui_install_job_progress(cx);
                    if changed {
                        this.show_gpui_spaceo_repair_step(&step, cx);
                    }
                });
                if updated.is_err() {
                    break;
                }
                shown = step;
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let (outcome, health) = background
                .spawn(async move {
                    let outcome = ghostex_spaceo_heal::repair(&spaceo, "app", &mut |step| {
                        SPACEO_JOB.append_line(step);
                        if let Ok(mut current) = current_step.lock() {
                            *current = step.to_string();
                        }
                    });
                    SPACEO_JOB.append_line(outcome.message());
                    SPACEO_JOB.finish(&if outcome.ok() {
                        Ok(())
                    } else {
                        Err(outcome.message().to_string())
                    });
                    let health = spaceo.check();
                    if let Ok(mut watch) = WATCH.lock() {
                        let watch = watch.get_or_insert_with(Watch::default);
                        watch.last_state = Some(health.state);
                        watch.last_check = Some(Instant::now());
                    }
                    (outcome, health)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_gpui_install_job_progress(cx);
                this.upsert_gpui_app_toast(spaceo_repair_result_toast(&outcome, &health), cx);
                this.run_gpui_app_modal_and_titlebar_status_task(
                    || gpui_ghostex_cli_status_message(None),
                    cx,
                );
            });
        })
        .detach();
    }
}

/// What Repair's toast becomes when Repair ends.
fn spaceo_repair_result_toast(outcome: &Outcome, health: &Health) -> GpuiAppToast {
    let (level, title, still_stuck) = match outcome {
        Outcome::Fixed(_) => (GpuiAppToastLevel::Success, "SpaceO repaired", false),
        Outcome::NothingToDo(_) => (GpuiAppToastLevel::Info, "SpaceO was already working", false),
        Outcome::Refused(_) => (GpuiAppToastLevel::Warning, "SpaceO wasn't repaired", false),
        Outcome::StillStuck(_) => (GpuiAppToastLevel::Error, "SpaceO is still stuck", true),
    };
    let message = outcome.message().to_string();
    GpuiAppToast {
        id: NOTICE_ID.to_string(),
        level,
        title: title.to_string(),
        description: Some(message.clone()),
        copy_text: still_stuck
            .then(|| format!("{}\nRepair result: {message}", notice_details(health))),
        action: still_stuck.then_some(GpuiAppToastAction::RepairSpaceo),
        loading: false,
        persistent: still_stuck,
        duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
        epoch: 0,
    }
}
