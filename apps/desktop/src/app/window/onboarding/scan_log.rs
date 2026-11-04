//! The Agents panel's scan console (previews/agent-scan-log.tsx (deleted 2026-10-01), `.term` in styles/agents.css).
//!
//! CDXC:Onboarding 2026-09-11 WHY:
//! The prototype replayed a scripted scan with fake timestamps. Every line here is stamped with the
//! real clock when it is appended, the "Found" lines come from the host's detection result, and the
//! stagger only paces the reveal; nothing is shown as found before the host reported it. The host
//! probes providers one at a time and re-sends the agents after each, so "Found" lines are appended
//! as agents arrive and "Scan complete." waits for the walk's final payload.
use super::GpuiOnboardingWindow;
use super::fonts::PLEX_MONO;
use super::interact;
use super::model::installed_agents;
use super::primitives::*;
use super::stage::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, linear_color_stop, linear_gradient,
};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

const LINE_STAGGER: Duration = Duration::from_millis(360);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtraTone {
    Acc,
    Ok,
    Wait,
}

/// A line the Agents panel adds under the scan (connections, installs, errors, the default).
#[derive(Clone)]
pub(crate) struct ScanExtra {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) tone: ExtraTone,
    pub(crate) at: chrono::DateTime<chrono::Local>,
    pub(crate) appeared: Instant,
}

#[derive(Clone)]
enum Step {
    Start,
    Check,
    Found { agent_id: String, name: String },
    Done,
}

struct ScanLine {
    text: String,
    at: chrono::DateTime<chrono::Local>,
    appeared: Instant,
    ok: bool,
    done: bool,
    agent_id: Option<String>,
}

pub(crate) struct ScanLog {
    lines: Vec<ScanLine>,
    queue: VecDeque<(Instant, Step)>,
    next_at: Option<Instant>,
    reveal_end: Option<Instant>,
    found_ids: HashSet<String>,
    has_started: bool,
    /// The agents revision present when the current scan started; stale until the host replaces it.
    agents_at_start: Option<u64>,
}

impl ScanLog {
    pub(crate) fn new() -> Self {
        Self {
            lines: Vec::new(),
            queue: VecDeque::new(),
            next_at: None,
            reveal_end: None,
            found_ids: HashSet::new(),
            has_started: false,
            agents_at_start: None,
        }
    }

    fn reset(&mut self) {
        self.lines.clear();
        self.queue.clear();
        self.next_at = None;
        self.reveal_end = None;
        self.found_ids.clear();
    }

    fn enqueue(&mut self, steps: Vec<Step>, now: Instant) {
        if steps.is_empty() {
            return;
        }
        let base = self.next_at.map_or(now, |next| next.max(now));
        let count = steps.len() as u32;
        for (index, step) in steps.into_iter().enumerate() {
            self.queue
                .push_back((base + LINE_STAGGER * index as u32, step));
        }
        let end = base + LINE_STAGGER * count;
        self.next_at = Some(end);
        self.reveal_end = Some(end);
    }

    fn found_steps(&mut self, agents: &[super::model::DetectedAgent]) -> Vec<Step> {
        installed_agents(agents)
            .into_iter()
            .filter(|agent| self.found_ids.insert(agent.agent_id.clone()))
            .map(|agent| Step::Found {
                agent_id: agent.agent_id.clone(),
                name: agent.name.clone(),
            })
            .collect()
    }

    fn revealing(&self, now: Instant) -> bool {
        self.reveal_end.is_some_and(|end| now < end)
    }
}

/// Merges two lists that are each in clock order into one, keeping the first list's line ahead of
/// the second's when their clocks agree.
fn chronological<T: Ord + Copy, E>(
    first: impl Iterator<Item = (T, E)>,
    second: impl Iterator<Item = (T, E)>,
) -> Vec<E> {
    let mut merged: Vec<(T, usize, E)> = first
        .map(|(at, line)| (at, 0, line))
        .chain(second.map(|(at, line)| (at, 1, line)))
        .collect();
    merged.sort_by_key(|(at, list, _)| (*at, *list));
    merged.into_iter().map(|(_, _, line)| line).collect()
}

fn start_steps() -> Vec<Step> {
    vec![Step::Start, Step::Check]
}

impl GpuiOnboardingWindow {
    /// The `[loading]` effect, run on mount and whenever loading flips.
    pub(super) fn scan_log_on_loading(&mut self, was_loading: bool, now: Instant) {
        let loading = self.agents_loading;
        let log = &mut self.agents_panel.scan_log;
        let started = loading && !was_loading;
        let ended = !loading && was_loading;
        if started {
            log.has_started = true;
            log.agents_at_start = Some(self.agents_revision);
            log.reset();
            log.enqueue(start_steps(), now);
            return;
        }
        if loading {
            return;
        }
        if !log.has_started {
            log.has_started = true;
            log.reset();
            let mut steps = start_steps();
            steps.extend(log.found_steps(&self.agents));
            steps.push(Step::Done);
            log.enqueue(steps, now);
            return;
        }
        if ended {
            let mut steps = log.found_steps(&self.agents);
            steps.push(Step::Done);
            log.enqueue(steps, now);
        }
    }

    /// The `[agents, loading]` effect: a new partial result while scanning prints its new finds.
    pub(super) fn scan_log_on_agents(&mut self, loading: bool, now: Instant) {
        let log = &mut self.agents_panel.scan_log;
        if !loading || !log.has_started || log.agents_at_start == Some(self.agents_revision) {
            return;
        }
        let steps = log.found_steps(&self.agents);
        log.enqueue(steps, now);
    }

    pub(super) fn scan_log_mount(&mut self, now: Instant) {
        self.agents_panel.scan_log = ScanLog::new();
        self.agents_panel.scan_log_mounted = now;
        self.scan_log_on_loading(false, now);
    }

    pub(super) fn scan_log_tick(&mut self, now: Instant) {
        let log = &mut self.agents_panel.scan_log;
        while let Some((due, _)) = log.queue.front() {
            if *due > now {
                break;
            }
            let Some((due, step)) = log.queue.pop_front() else {
                break;
            };
            let (text, ok, done, agent_id) = match step {
                Step::Start => ("Starting the scan...".to_string(), false, false, None),
                Step::Check => (
                    "Checking installed agent CLIs...".to_string(),
                    false,
                    false,
                    None,
                ),
                Step::Found { agent_id, name } => {
                    (format!("Found {name}"), true, false, Some(agent_id))
                }
                Step::Done => ("Scan complete.".to_string(), false, true, None),
            };
            log.lines.push(ScanLine {
                text,
                at: chrono::Local::now(),
                appeared: due,
                ok,
                done,
                agent_id,
            });
        }
    }

    pub(super) fn scan_log_scanning(&self, now: Instant) -> bool {
        self.agents_loading || self.agents_panel.scan_log.revealing(now)
    }

    pub(super) fn render_scan_log(
        &self,
        s: S,
        now: Instant,
        default_agent: Option<&str>,
        rescan_disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let log = &self.agents_panel.scan_log;
        let scanning = self.scan_log_scanning(now);
        let line = |time: &chrono::DateTime<chrono::Local>, appeared: Instant| {
            let t = entrance(appeared, now, 250);
            div()
                .relative()
                .left(s.px(-4.0 * (1.0 - t)))
                .opacity(t)
                .h(s.px(25.5))
                .flex()
                .items_center()
                .font_family(PLEX_MONO)
                .text_size(s.px(15.0))
                .text_color(hex(0xc9d0dc))
                .child(
                    div()
                        .w(s.px(112.0))
                        .flex_none()
                        .text_color(hex(0xaab2c1))
                        .child(format!("[{}]", clock_stamp(*time))),
                )
        };
        let scan_lines = log.lines.iter().map(|entry| {
            let hot = entry.agent_id.is_some() && entry.agent_id.as_deref() == default_agent;
            line(&entry.at, entry.appeared)
                .when(hot, |this| {
                    this.mx(s.px(-10.0))
                        .px(s.px(10.0))
                        .rounded(s.px(4.0))
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(rgba(60, 90, 200, 0.24), 0.0),
                            linear_color_stop(rgba(60, 90, 200, 0.0), 0.8),
                        ))
                        .shadow(vec![inset_shadow(hex(0x4b73ff), 2.0, 0.0, 0.0, 0.0, s)])
                })
                .child(
                    div()
                        .min_w(s.px(248.0))
                        .flex_none()
                        .whitespace_nowrap()
                        .pr(s.px(12.0))
                        .when(entry.done, |this| {
                            let (family, weight) = super::fonts::face(PLEX_MONO, 600.0);
                            this.text_color(hex(0x3be3a2))
                                .font_family(family)
                                .font_weight(gpui::FontWeight(weight))
                        })
                        .when(hot, |this| this.text_color(gpui::white()))
                        .child(entry.text.clone()),
                )
                .when(entry.ok, |this| {
                    this.child(popping_icon(
                        s,
                        "check",
                        17.0,
                        2.0,
                        hex(0x3be3a2),
                        entry.appeared,
                        now,
                    ))
                })
                .into_any_element()
        });
        let extra_lines = self.agents_panel.extras.iter().map(|extra| {
            let color = match extra.tone {
                ExtraTone::Acc => hex(0x8fabff),
                ExtraTone::Ok => hex(0x3be3a2),
                ExtraTone::Wait => hex(0xffc46b),
            };
            line(&extra.at, extra.appeared)
                .child(
                    div()
                        .min_w(s.px(248.0))
                        .flex_none()
                        .whitespace_nowrap()
                        .pr(s.px(12.0))
                        .text_color(color)
                        .child(extra.text.clone()),
                )
                .into_any_element()
        });
        // CDXC:Onboarding 2026-10-28 WHY: the scan's lines and the panel's own lines (installs,
        // connections, errors) were two blocks, the second hidden while scanning, so a rescan after
        // an install printed newer scan lines above older install lines and the clock column ran
        // backwards. Both now form one list in the order their clocks say.
        let lines = chronological(
            log.lines.iter().map(|entry| entry.at).zip(scan_lines),
            self.agents_panel
                .extras
                .iter()
                .map(|extra| extra.at)
                .zip(extra_lines),
        );
        let loading = self.agents_loading;
        let rescan_enabled = !scanning && !rescan_disabled;
        glass(s)
            .absolute()
            .left(s.px(885.0))
            .top(s.px(174.0))
            .w(s.px(700.0))
            .h(s.px(586.0))
            .py(s.px(20.0))
            .px(s.px(24.0))
            .border_color(rgba(90, 125, 235, 0.25))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(rgba(10, 14, 28, 0.86), 0.0),
                linear_color_stop(rgba(6, 8, 16, 0.9), 1.0),
            ))
            .font_family(PLEX_MONO)
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s.px(18.0))
                    .mb(s.px(22.0))
                    .child(
                        div()
                            .flex_1()
                            .font_family(super::fonts::PLEX_MONO_MEDIUM)
                            .text_size(s.px(17.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .text_color(hex(0xeef1f7))
                            .child("Looking for agents"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(s.px(10.0))
                            .font_family(super::fonts::PLEX_MONO_MEDIUM)
                            .font_weight(gpui::FontWeight(500.0))
                            .text_size(s.px(16.0))
                            .text_color(if scanning {
                                hex(0xaab2c1)
                            } else {
                                hex(0x3be3a2)
                            })
                            .map(|this| {
                                if scanning {
                                    this.child(spinner(s, 18.0, 2.0, self.opened_at, now))
                                        .child("Scanning...")
                                } else {
                                    this.child(icon(s, "checkCircle", 16.0, 1.6, hex(0x3be3a2)))
                                        .child("Complete")
                                }
                            }),
                    )
                    .child({
                        // `.rescan`: no transition of its own, so the host's 120ms button transition.
                        let color = interact::tween_color(
                            "scan-rescan",
                            "color",
                            if rescan_enabled {
                                hex(0xdfe4ee)
                            } else {
                                hex(0x7c8598)
                            },
                            interact::BUTTON_MS,
                        );
                        // A neutral white wash rather than the prototype's outline
                        // (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
                        let bg = interact::tween_color(
                            "scan-rescan",
                            "bg",
                            if rescan_enabled && interact::hovered("scan-rescan") {
                                white(0.14)
                            } else {
                                white(0.10)
                            },
                            interact::BUTTON_MS,
                        );
                        let button = div()
                            .id("scan-rescan")
                            .relative()
                            .flex()
                            .items_center()
                            .gap(s.px(8.0))
                            .h(s.px(32.0))
                            .px(s.px(12.0))
                            .rounded(s.px(8.0))
                            .border_1()
                            .border_color(white(0.0))
                            .bg(bg)
                            .font_family(super::fonts::dm_sans())
                            .font_weight(gpui::FontWeight(500.0))
                            .text_size(s.px(13.5))
                            .text_color(color)
                            .when(rescan_enabled, |this| {
                                this.cursor_pointer().active(|this| this.bg(white(0.18)))
                            })
                            .when(!rescan_enabled, |this| {
                                this.opacity(DISABLED_BUTTON_OPACITY)
                            })
                            .child(if loading {
                                spinning_icon(s, "refresh", 15.0, color, self.opened_at, now, 1.0)
                            } else {
                                icon(s, "refresh", 15.0, 1.6, color)
                            })
                            .child("Rescan");
                        // A disabled button is out of the tab order.
                        if rescan_enabled {
                            self.control(
                                s,
                                button,
                                "scan-rescan",
                                interact::Ring::new(8.0, 1.0),
                                interact::Keys::EnterSpace,
                                cx,
                                |this, _, cx| this.rescan_agents(cx),
                            )
                        } else {
                            interact::watch_hover(button, "scan-rescan")
                        }
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(lines)
                    .when(scanning, |this| {
                        this.child(
                            div()
                                .mt(s.px(4.0))
                                .w(s.px(9.0))
                                .h(s.px(17.0))
                                .bg(hex(0xaab2c1))
                                .opacity(if blink(self.opened_at, now) { 1.0 } else { 0.0 }),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::chronological;

    #[test]
    fn scan_and_panel_lines_read_in_clock_order() {
        let scan = [
            (5, "Starting the scan..."),
            (6, "Found Claude Code"),
            (9, "Scan complete."),
        ];
        let panel = [
            (2, "Installing Claude Code…"),
            (4, "Installed Claude Code"),
            (6, "Connected Claude Code"),
        ];
        assert_eq!(
            chronological(scan.into_iter(), panel.into_iter()),
            vec![
                "Installing Claude Code…",
                "Installed Claude Code",
                "Starting the scan...",
                "Found Claude Code",
                "Connected Claude Code",
                "Scan complete.",
            ]
        );
    }
}
