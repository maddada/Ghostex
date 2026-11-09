//! Preview of the work area header's jobs button and its popover, drawn from the app's own view
//! (app/window/jobs_popover_view.rs) under a stand-in of the header's other buttons.
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `running` (the default: one clone at 14%, popover
//! open), `button` (popover closed), `starting` (no percent yet), `two` (a clone running and one
//! failed), `failed`, `canceling`.
//! `live`: a real clone through a gxserver (`GHOSTEX_ADD_PROJECT_DEMO_GXSERVER` and `_TOKEN`, as
//! the Add Project preview), of `GHOSTEX_JOBS_DEMO_CLONE_URL` into `GHOSTEX_JOBS_DEMO_DEST`, read
//! every 900ms the way app/background_jobs/clone.rs reads it; `GHOSTEX_JOBS_DEMO_CANCEL_MS` presses
//! Cancel after that long, `GHOSTEX_JOBS_DEMO_POPOVER=0` keeps the popover closed. Each step is
//! printed, and the finished clone's toast is drawn as a stand-in at the bottom.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div, px, rgb, svg,
};
use gpui_component::h_flex;
use serde_json::{Value, json};

use super::add_project_demo::LiveGxserver;
use super::jobs_popover_view::{
    self as view, JOBS_POPOVER_WIDTH, JobRow, JobStatus, JobsHandler, JobsModel,
};

const WINDOW_WIDTH: f32 = 760.0;
const WINDOW_HEIGHT: f32 = 400.0;

fn clone_row(id: u64, progress: Option<&str>, status: JobStatus) -> JobRow {
    JobRow {
        id,
        title: "Cloning thecodeorg/maestro_ed".into(),
        failed_title: "Couldn't clone thecodeorg/maestro_ed".into(),
        detail: "Into ~/dev/maestro_ed".into(),
        percent: progress.and_then(view::progress_percent),
        progress: progress.map(str::to_string),
        status,
        verb: "Cloning".into(),
    }
}

fn rows(state: &str) -> Vec<JobRow> {
    let receiving = Some("Receiving objects:  14% (14522/102831), 377.15 MiB | 15.39 MiB/s");
    match state {
        "starting" => vec![clone_row(1, None, JobStatus::Running)],
        "canceling" => vec![clone_row(1, receiving, JobStatus::Canceling)],
        "failed" => vec![JobRow {
            title: "Cloning acme/private-api".into(),
            failed_title: "Couldn't clone acme/private-api".into(),
            detail: "Into ~/dev/private-api on Bigbox".into(),
            ..clone_row(
                2,
                None,
                JobStatus::Failed(
                    "Repository not found. Check the URL and that this computer can reach it."
                        .into(),
                ),
            )
        }],
        "two" => [rows("running"), rows("failed")].concat(),
        _ => vec![clone_row(1, receiving, JobStatus::Running)],
    }
}

struct JobsDemo {
    model: JobsModel,
    popover: bool,
    toast: Option<String>,
}

impl Render for JobsDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on: JobsHandler<Self> = Rc::new(|_, action, _, _| eprintln!("action {action:?}"));
        let light = self.model.light;
        let ink: gpui::Hsla = if light { rgb(0x1a1a1c) } else { rgb(0xe8e8ea) }.into();
        let danger: gpui::Hsla = if light { rgb(0xb8322c) } else { rgb(0xf0928e) }.into();
        let border: gpui::Hsla = if light { rgb(0xd4d4d8) } else { rgb(0x3a3a40) }.into();
        let stand_in = |icon: &'static str, label: &'static str| {
            h_flex()
                .h(px(26.0))
                .px(px(9.0))
                .ml(px(4.0))
                .gap(px(6.0))
                .items_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(border)
                .text_size(px(12.5))
                .text_color(ink)
                .child(svg().path(icon).size(px(14.0)).text_color(ink))
                .when(!label.is_empty(), |this| this.child(label))
        };
        let button = view::jobs_button_text(&self.model.rows)
            .map(|(label, _, failed)| view::jobs_button(label, failed, ink, danger, 26.0, 6.0));
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(if light { rgb(0xf4f4f5) } else { rgb(0x17171b) })
            .child(
                h_flex()
                    .absolute()
                    .top(px(10.0))
                    .right(px(16.0))
                    .items_center()
                    .children(button)
                    .child(stand_in("titlebar/headset.svg", ""))
                    .child(stand_in("titlebar/player-play.svg", "Start"))
                    .child(stand_in("titlebar/git-commit.svg", "Commit")),
            )
            .child(
                div()
                    .absolute()
                    .top(px(44.0))
                    .left(px(16.0))
                    .text_size(px(13.0))
                    .text_color(ink)
                    .opacity(0.4)
                    .child("Work area header (stand-ins right of the jobs button)"),
            )
            .when(self.popover && !self.model.rows.is_empty(), |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(44.0))
                        .right(px(196.0))
                        .w(px(JOBS_POPOVER_WIDTH))
                        .child(view::render_jobs_popover(&self.model, &on, cx)),
                )
            })
            .when_some(self.toast.clone(), |this, toast| {
                this.child(
                    h_flex()
                        .absolute()
                        .bottom(px(18.0))
                        .left(px(WINDOW_WIDTH / 2.0 - 190.0))
                        .w(px(380.0))
                        .h(px(40.0))
                        .px(px(14.0))
                        .items_center()
                        .rounded(px(8.0))
                        .bg(if light { rgb(0xe7f6ec) } else { rgb(0x1f3a2a) })
                        .text_size(px(13.0))
                        .text_color(ink)
                        .child(toast),
                )
            })
    }
}

/// The live clone: start, then read every 900ms until it ends, as clone.rs does.
fn run_live(view: Entity<JobsDemo>, live: Rc<LiveGxserver>, cx: &mut App) {
    let url = std::env::var("GHOSTEX_JOBS_DEMO_CLONE_URL").unwrap_or_default();
    let dest = std::env::var("GHOSTEX_JOBS_DEMO_DEST").unwrap_or_default();
    let cancel_after = std::env::var("GHOSTEX_JOBS_DEMO_CANCEL_MS")
        .ok()
        .and_then(|ms| ms.trim().parse::<u64>().ok());
    let repository = url
        .trim_end_matches(".git")
        .rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/");
    let folder = dest.rsplit('/').next().unwrap_or("").to_string();
    view.update(cx, |demo, cx| {
        demo.model.rows = vec![JobRow {
            id: 1,
            title: format!("Cloning {repository}"),
            failed_title: format!("Couldn't clone {repository}"),
            detail: format!("Into {dest}"),
            percent: None,
            progress: None,
            status: JobStatus::Running,
            verb: "Cloning".into(),
        }];
        cx.notify();
    });
    let background = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let started = std::time::Instant::now();
        let call = |endpoint: &'static str, params: Value| {
            let (base, token) = (live.base_url.clone(), live.token.clone());
            background.spawn(async move { LiveGxserver::call(&base, &token, endpoint, params) })
        };
        let job_id = match call(
            "/api/startRepositoryClone",
            json!({ "remoteUrl": url, "destinationPath": dest }),
        )
        .await
        {
            Ok(value) => value["job"]["jobId"].as_str().unwrap_or("").to_string(),
            Err(error) => {
                eprintln!("job start failed: {error}");
                view.update(cx, |demo, cx| {
                    demo.model.rows[0].status = JobStatus::Failed(error);
                    cx.notify();
                });
                return;
            }
        };
        eprintln!("job started {job_id}");
        let mut canceled = false;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;
            if !canceled
                && cancel_after.is_some_and(|ms| started.elapsed().as_millis() as u64 >= ms)
            {
                canceled = true;
                eprintln!("job cancel pressed");
                view.update(cx, |demo, cx| {
                    if let Some(row) = demo.model.rows.first_mut() {
                        row.status = JobStatus::Canceling;
                    }
                    cx.notify();
                });
                let answer =
                    call("/api/cancelRepositoryCloneJob", json!({ "jobId": job_id })).await;
                eprintln!(
                    "job cancel answer: {:?}",
                    answer.map(|v| v["job"]["state"].clone())
                );
            }
            let job = match call("/api/readRepositoryCloneJob", json!({ "jobId": job_id })).await {
                Ok(value) => value["job"].clone(),
                Err(error) => {
                    eprintln!("job read failed: {error}");
                    continue;
                }
            };
            let state = job["state"].as_str().unwrap_or("").to_string();
            let progress = job["progress"].as_str().map(str::to_string);
            eprintln!(
                "job {state} {}",
                progress.clone().unwrap_or_else(|| "-".into())
            );
            let done = view.update(cx, |demo, cx| {
                let done = match state.as_str() {
                    "running" => {
                        if let Some(row) = demo.model.rows.first_mut()
                            && progress.is_some()
                        {
                            row.percent = progress.as_deref().and_then(view::progress_percent);
                            row.progress = progress.clone();
                        }
                        false
                    }
                    "completed" => {
                        demo.model.rows.clear();
                        demo.toast = Some(format!(
                            "Repository cloned \u{b7} {folder} is in your projects."
                        ));
                        true
                    }
                    "canceled" => {
                        demo.model.rows.clear();
                        true
                    }
                    _ => {
                        let reason = job["error"]
                            .as_str()
                            .or(job["message"].as_str())
                            .unwrap_or("Repository clone failed.")
                            .to_string();
                        if let Some(row) = demo.model.rows.first_mut() {
                            row.status = JobStatus::Failed(reason);
                        }
                        true
                    }
                };
                cx.notify();
                done
            });
            if done {
                eprintln!("job ended: {state}");
                return;
            }
        }
    })
    .detach();
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let light = super::env("GHOSTEX_NATIVE_MODAL_DEMO_THEME") == "light";
    let state = if demo.state.is_empty() {
        "running".to_string()
    } else {
        demo.state.clone()
    };
    let popover = state != "button" && super::env("GHOSTEX_JOBS_DEMO_POPOVER") != "0";
    let model = JobsModel {
        rows: if state == "live" {
            Vec::new()
        } else {
            rows(&state)
        },
        light,
        glass: false,
        surface: None,
    };
    let slot: Rc<RefCell<Option<Entity<JobsDemo>>>> = Rc::new(RefCell::new(None));
    let out = slot.clone();
    super::open_modal_window(
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        move |_window, cx| {
            let view = cx.new(|_| JobsDemo {
                model,
                popover,
                toast: None,
            });
            *out.borrow_mut() = Some(view.clone());
            view
        },
        cx,
    );
    if state == "live" {
        let view = slot.borrow_mut().take().expect("the jobs demo view");
        match LiveGxserver::from_env() {
            Some(live) => run_live(view, Rc::new(live), cx),
            None => eprintln!("live needs GHOSTEX_ADD_PROJECT_DEMO_GXSERVER and _TOKEN"),
        }
    }
}
