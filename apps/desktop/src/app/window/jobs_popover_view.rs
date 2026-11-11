//! The work area header's jobs button and the popover it opens, drawn from a plain model so the
//! app (window/jobs_popover.rs and render/workarea_header/jobs_button.rs, which own the jobs) and
//! the native modal demo (bin/native_modal_demo/jobs_popover.rs, which draws every state
//! offscreen) share one file.
//!
//! CDXC:BackgroundJobs 2026-11-09 DECISION:
//! User: "when i tried to clone a project this stayed on screen ... instead of this lets show a status button here (only shows when there's something in progress, should show on the active chat only)". A long job (a repository clone today) runs in the background: the header shows one button just left of the voice button while a job runs or a failed one waits, with a spinner and a short label ("Cloning 14%") and the whole line in its tooltip; it opens this popover with each job, its progress bar, git's last progress line and Cancel, and a failed job's plain reason with Retry and Dismiss. A finished job says so once in a plain toast and leaves the list. Built for any long job, wired for clones only.
//! SEE-ALSO: apps/desktop/src/app/background_jobs/ (the jobs and their gxserver calls).

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Transformation, Window, div, px, radians, svg,
};
use gpui_component::{h_flex, v_flex};

use super::share_popover_view::PopupSurface;

/// The popover's width; its height follows its rows.
pub(crate) const JOBS_POPOVER_WIDTH: f32 = 360.0;

const ICON_LOADER: &str = "titlebar/loader2.svg";
const ICON_ALERT: &str = "titlebar/alert-circle.svg";
const ICON_CLONE: &str = "titlebar/download.svg";
const ICON_X: &str = "titlebar/x.svg";

/// Where one job is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JobStatus {
    /// Asked for and not answered yet, or running.
    Running,
    /// Cancel was pressed; the machine has not confirmed it yet.
    Canceling,
    /// It stopped with this plain reason.
    Failed(String),
}

/// One job as the button and the popover show it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JobRow {
    pub(crate) id: u64,
    /// What it is: "Cloning thecodeorg/maestro_ed".
    pub(crate) title: String,
    /// What a failed one says instead: "Couldn't clone thecodeorg/maestro_ed".
    pub(crate) failed_title: String,
    /// Where it goes: "Into ~/dev/maestro_ed on Bigbox".
    pub(crate) detail: String,
    /// The tool's last progress line (git's `Receiving objects: 14% (…)`).
    pub(crate) progress: Option<String>,
    /// 0 to 100, read from that line.
    pub(crate) percent: Option<f32>,
    pub(crate) status: JobStatus,
    /// The verb the button's label starts with ("Cloning").
    pub(crate) verb: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JobsModel {
    pub(crate) rows: Vec<JobRow>,
    pub(crate) light: bool,
    /// Window glass is on: the frost shows through the panel.
    pub(crate) glass: bool,
    /// The panel's frame when the host gives it one: the app's dropdown menu surface, as the
    /// Share popover takes it (CDXC:Collab 2026-11-11 in share_popover_view.rs).
    pub(crate) surface: Option<PopupSurface>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JobsAction {
    Cancel(u64),
    Retry(u64),
    Dismiss(u64),
}

pub(crate) type JobsHandler<V> = Rc<dyn Fn(&mut V, JobsAction, &mut Window, &mut Context<V>)>;

/// The percentage in git's progress line (`Receiving objects:  14% (14522/102831), …`).
pub(crate) fn progress_percent(line: &str) -> Option<f32> {
    let percent_at = line.find('%')?;
    let digits: String = line[..percent_at]
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits
        .parse::<f32>()
        .ok()
        .filter(|percent| *percent <= 100.0)
}

/// What the header button says, its tooltip, and whether it reports a failure rather than work.
pub(crate) fn jobs_button_text(rows: &[JobRow]) -> Option<(String, String, bool)> {
    let running: Vec<&JobRow> = rows
        .iter()
        .filter(|row| !matches!(row.status, JobStatus::Failed(_)))
        .collect();
    let failed = rows.len() - running.len();
    let tooltip = rows
        .iter()
        .map(|row| {
            let mut lines = vec![row.title.clone(), row.detail.clone()];
            match &row.status {
                JobStatus::Failed(reason) => {
                    lines[0] = row.failed_title.clone();
                    lines.push(reason.clone());
                }
                JobStatus::Canceling => lines.push("Canceling…".to_string()),
                JobStatus::Running => lines.extend(row.progress.clone()),
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let label = match (running.as_slice(), failed) {
        ([], 0) => return None,
        ([], 1) => format!("{} failed", failed_noun(rows)),
        ([], count) => format!("{count} failed"),
        ([row], _) => match row.percent {
            Some(percent) => format!("{} {}%", row.verb, percent.round() as u32),
            None => row.verb.clone(),
        },
        (many, _) => format!("{} running", many.len()),
    };
    Some((label, tooltip, running.is_empty()))
}

/// "Clone" for a failed clone: the verb's noun, for the one-failure label.
fn failed_noun(rows: &[JobRow]) -> String {
    rows.iter()
        .find(|row| matches!(row.status, JobStatus::Failed(_)))
        .map(|row| match row.verb.as_str() {
            "Cloning" => "Clone".to_string(),
            verb => verb.to_string(),
        })
        .unwrap_or_else(|| "Job".to_string())
}

// ---- colours ------------------------------------------------------------------------------

/// Frosted warm grey in dark mode, frosted light grey in light mode, no near-black fills, as the
/// Share popover beside it (share_popover_view.rs).
#[derive(Clone, Copy)]
struct Palette {
    light: bool,
    glass: bool,
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    gpui::Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a,
    }
    .into()
}

impl Palette {
    fn ink(self, alpha: f32) -> Hsla {
        if self.light {
            rgba(22, 22, 24, alpha)
        } else {
            rgba(255, 255, 255, alpha)
        }
    }

    fn surface(self) -> Hsla {
        match (self.light, self.glass) {
            (false, true) => rgba(64, 60, 58, 0.56),
            (false, false) => rgba(52, 49, 47, 1.0),
            (true, true) => rgba(246, 244, 242, 0.72),
            (true, false) => rgba(244, 242, 240, 1.0),
        }
    }

    fn danger(self) -> Hsla {
        if self.light {
            rgba(184, 50, 44, 1.0)
        } else {
            rgba(240, 146, 142, 1.0)
        }
    }

    fn bar(self) -> Hsla {
        if self.light {
            rgba(37, 99, 235, 1.0)
        } else {
            rgba(125, 170, 255, 1.0)
        }
    }
}

// ---- pieces -------------------------------------------------------------------------------

fn icon(path: &'static str, size: f32, color: Hsla) -> gpui::Svg {
    svg()
        .path(path)
        .flex_shrink_0()
        .size(px(size))
        .text_color(color)
}

/// The turning loader both the button and a running row draw.
pub(crate) fn jobs_spinner(id: &'static str, size: f32, color: Hsla) -> AnyElement {
    icon(ICON_LOADER, size, color)
        .with_animation(
            id,
            Animation::new(std::time::Duration::from_millis(900))
                .repeat()
                .with_max_fps(crate::app::helpers::INDICATOR_MAX_FPS),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

/// The header button's body: the spinner (or the alert for a failure) and the label. The header
/// adds the press, the tooltip and its own hover (render/workarea_header/jobs_button.rs).
pub(crate) fn jobs_button(
    label: String,
    failed: bool,
    ink: Hsla,
    danger: Hsla,
    height: f32,
    radius: f32,
) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id("ghostex-gpui-workarea-header-jobs")
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .h(px(height))
        .px(px(9.0))
        .rounded(px(radius))
        .text_size(px(12.5))
        .text_color(if failed { danger } else { ink })
        .whitespace_nowrap()
        .child(if failed {
            icon(ICON_ALERT, 14.0, danger).into_any_element()
        } else {
            jobs_spinner("jobs-button-spinner", 14.0, ink)
        })
        .child(label)
}

fn small_button<V: 'static>(
    p: Palette,
    id: SharedString,
    label: &'static str,
    on: &JobsHandler<V>,
    action: JobsAction,
    cx: &mut Context<V>,
) -> AnyElement {
    let on = on.clone();
    let hover = p.ink(0.14);
    div()
        .id(id)
        .flex_shrink_0()
        .h(px(24.0))
        .px(px(9.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(p.ink(0.08))
        .border_1()
        .border_color(p.ink(0.1))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.ink(0.92))
        .cursor_pointer()
        .hover(move |this| this.bg(hover))
        .on_click(cx.listener(move |view, _, window, cx| on(view, action.clone(), window, cx)))
        .child(label)
        .into_any_element()
}

fn dismiss_button<V: 'static>(
    p: Palette,
    id: u64,
    on: &JobsHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let on = on.clone();
    let hover = p.ink(0.1);
    div()
        .id(SharedString::from(format!("jobs-dismiss-{id}")))
        .flex_shrink_0()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(move |this| this.bg(hover))
        .on_click(
            cx.listener(move |view, _, window, cx| on(view, JobsAction::Dismiss(id), window, cx)),
        )
        .child(icon(ICON_X, 13.0, p.ink(0.7)))
        .into_any_element()
}

fn progress_bar(p: Palette, percent: Option<f32>) -> AnyElement {
    let track = div()
        .w_full()
        .h(px(4.0))
        .rounded(px(2.0))
        .overflow_hidden()
        .bg(p.ink(0.12));
    match percent {
        Some(percent) => track
            .child(
                div()
                    .h_full()
                    .w(gpui::relative((percent / 100.0).clamp(0.02, 1.0)))
                    .rounded(px(2.0))
                    .bg(p.bar()),
            )
            .into_any_element(),
        // No number yet: a short bar that slides, so the row still reads as working.
        None => track
            .relative()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .h_full()
                    .w(gpui::relative(0.3))
                    .rounded(px(2.0))
                    .bg(p.bar())
                    .with_animation(
                        "jobs-progress-indeterminate",
                        Animation::new(std::time::Duration::from_millis(1400))
                            .repeat()
                            .with_max_fps(crate::app::helpers::INDICATOR_MAX_FPS),
                        |bar, delta| bar.left(gpui::relative(delta * 1.0 - 0.3)),
                    ),
            )
            .into_any_element(),
    }
}

fn job_row<V: 'static>(
    p: Palette,
    row: &JobRow,
    first: bool,
    on: &JobsHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let failed = matches!(row.status, JobStatus::Failed(_));
    let lead = if failed {
        icon(ICON_ALERT, 15.0, p.danger()).into_any_element()
    } else {
        icon(ICON_CLONE, 15.0, p.ink(0.8)).into_any_element()
    };
    let buttons = match &row.status {
        JobStatus::Running => h_flex()
            .child(small_button(
                p,
                SharedString::from(format!("jobs-cancel-{}", row.id)),
                "Cancel",
                on,
                JobsAction::Cancel(row.id),
                cx,
            ))
            .into_any_element(),
        JobStatus::Canceling => div()
            .text_size(px(12.0))
            .text_color(p.ink(0.6))
            .child("Canceling…")
            .into_any_element(),
        JobStatus::Failed(_) => h_flex()
            .gap(px(4.0))
            .child(small_button(
                p,
                SharedString::from(format!("jobs-retry-{}", row.id)),
                "Retry",
                on,
                JobsAction::Retry(row.id),
                cx,
            ))
            .child(dismiss_button(p, row.id, on, cx))
            .into_any_element(),
    };
    let line = |text: String, color: Hsla| {
        div()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(px(12.0))
            .line_height(px(17.0))
            .text_color(color)
            .child(text)
    };
    let mut body = v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(3.0))
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink(0.95))
                        .child(if failed {
                            row.failed_title.clone()
                        } else {
                            row.title.clone()
                        }),
                )
                .child(buttons),
        )
        .child(line(row.detail.clone(), p.ink(0.62)));
    body = match &row.status {
        JobStatus::Failed(reason) => body.child(
            div()
                .w_full()
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(p.danger())
                .child(reason.clone()),
        ),
        _ => body
            .child(
                h_flex()
                    .w_full()
                    .mt(px(3.0))
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_1().child(progress_bar(p, row.percent)))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(32.0))
                            .text_size(px(11.5))
                            .text_color(p.ink(0.7))
                            .child(
                                row.percent
                                    .map(|percent| format!("{}%", percent.round() as u32))
                                    .unwrap_or_default(),
                            ),
                    ),
            )
            .children(
                row.progress
                    .clone()
                    .map(|progress| line(progress, p.ink(0.55))),
            ),
    };
    h_flex()
        .w_full()
        .items_start()
        .gap(px(10.0))
        .px(px(13.0))
        .py(px(11.0))
        .when(!first, |this| this.border_t_1().border_color(p.ink(0.08)))
        .child(div().mt(px(2.0)).child(lead))
        .child(body)
        .into_any_element()
}

/// The popover: a heading, then one row per job.
pub(crate) fn render_jobs_popover<V: 'static>(
    model: &JobsModel,
    on: &JobsHandler<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = Palette {
        light: model.light,
        glass: model.glass,
    };
    let running = model
        .rows
        .iter()
        .filter(|row| !matches!(row.status, JobStatus::Failed(_)))
        .count();
    let heading = if running == 0 {
        "Stopped"
    } else {
        "In progress"
    };
    let surface = model.surface.unwrap_or(PopupSurface {
        fill: p.surface(),
        border: p.ink(0.12),
        radius: 8.0,
    });
    v_flex()
        .id("jobs-popover")
        .w(px(JOBS_POPOVER_WIDTH))
        .rounded(px(surface.radius))
        .overflow_hidden()
        .bg(surface.fill)
        .border_1()
        .border_color(surface.border)
        .text_color(p.ink(0.92))
        .child(
            h_flex()
                .w_full()
                .h(px(36.0))
                .items_center()
                .px(px(13.0))
                .border_b_1()
                .border_color(p.ink(0.08))
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink(0.75))
                .child(heading),
        )
        .children(
            model
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| job_row(p, row, index == 0, on, cx)),
        )
        .into_any_element()
}
