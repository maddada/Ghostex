//! The "Previous question" pill's attention dot and its ping (chat_pills.rs draws the pill): the
//! amber dot sits on the pill while a question waits, and a ring spreads out of it when a question
//! arrives and every 30 s after that. Which questions wait is the core's (`openQuestions`,
//! packages/gx-chat-core/src/questions/open_questions.rs); this times and draws the ping.
//!
//! CDXC:SessionChat 2026-10-17 SEE-ALSO:
//! The phone pings the same way in apps/mobile/app/src/chat/native/cards/ChatPills.tsx, with the timings of `questionPing` in packages/gx-chat-core/visual/scroll-bottom.json. Change them together.

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{AnyElement, Context, IntoElement, Styled as _, Task, Window, deferred, div, px, rgb};
use std::time::Duration;
use web_time::Instant;

/// The pill's ping state: the question that last arrived, the ping on screen and the next beat.
#[derive(Default)]
pub(super) struct QuestionPing {
    newest: Option<String>,
    count: u64,
    started: Option<Instant>,
    next: Option<Instant>,
    hovered: bool,
    timer_for: Option<Instant>,
    timer: Option<Task<()>>,
    /// The pill shows the count instead of the ?, flipped by `count_swap` every `countSwapMs`
    /// while two or more questions wait (chat_pills.rs, where the user's decision lives).
    showing_count: bool,
    count_swap: Option<Task<()>>,
}

impl QuestionPing {
    pub(super) fn showing_count(&self) -> bool {
        self.showing_count
    }
}

/// How far the ping on screen has played, 0 to 1.
pub(super) struct PingFrame {
    progress: f32,
}

impl NativeChatView {
    /// Plays the ping on arrival and on every beat while a question waits, skipping a beat the
    /// pointer is on the pill for, and forgets everything once nothing waits.
    ///
    /// CDXC:SessionChat 2026-10-17 DECISION:
    /// User picked the attention dot ping (Q236 G) and "Once on arrival, then every 30 s" (Q237): the pill keeps the Threads pill's amber attention dot while a question waits, and the dot sends out one ping (under a second) when a question arrives and every 30 s while any is unanswered. A beat is skipped while the pointer is on the pill, hovering stops a ping at once, a new question restarts the 30 s, and the pings stop as soon as nothing waits. With Reduce Motion on, the dot stays and never pings. The pinned "Needs you" strip was declined for now (Q238).
    pub(super) fn sync_question_ping(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = &self.snapshot["openQuestions"];
        let count = open["count"].as_u64().unwrap_or(0);
        let newest = open["newest"].as_str().map(str::to_owned);
        let ping = &mut self.question_ping;
        if count == 0 {
            *ping = QuestionPing {
                hovered: ping.hovered,
                ..QuestionPing::default()
            };
            return;
        }
        let now = Instant::now();
        let every = Duration::from_millis(super::scroll_bottom::question_ping_every_ms());
        // A different newest question without the count dropping is a new one (answering the
        // newest also changes it, but takes the count down with it).
        if newest.is_some() && newest != ping.newest && count >= ping.count {
            ping.next = Some(now);
        }
        ping.newest = newest;
        ping.count = count;
        if count < 2 {
            ping.showing_count = false;
            ping.count_swap = None;
        } else if ping.count_swap.is_none() {
            let every = Duration::from_millis(super::scroll_bottom::question_count_swap_ms());
            ping.count_swap = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(every).await;
                    let flipped = this.update(cx, |chat, cx| {
                        chat.question_ping.showing_count ^= true;
                        cx.notify();
                    });
                    if flipped.is_err() {
                        break;
                    }
                }
            }));
        }
        let next = *ping.next.get_or_insert(now + every);
        if now >= next {
            if !ping.hovered && !crate::app::helpers::gpui_macos_reduce_motion_enabled() {
                ping.started = Some(now);
            }
            ping.next = Some(now + every);
        }
        if let Some(started) = ping.started {
            if started.elapsed() < ping_duration() {
                window.request_animation_frame();
            } else {
                ping.started = None;
            }
        }
        let next = ping.next;
        if ping.timer_for != next {
            ping.timer_for = next;
            let wait = next.map_or(Duration::ZERO, |next| {
                next.saturating_duration_since(Instant::now())
            });
            ping.timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }));
        }
    }

    /// The pointer came onto the pill (a ping on screen stops) or left it.
    pub(super) fn question_pill_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.question_ping.hovered = hovered;
        if hovered && self.question_ping.started.take().is_some() {
            cx.notify();
        }
    }

    /// The ping on screen, if one is playing.
    pub(super) fn question_ping_frame(&self) -> Option<PingFrame> {
        let started = self.question_ping.started?;
        let progress = started.elapsed().as_secs_f32() / ping_duration().as_secs_f32();
        (progress < 1.0).then_some(PingFrame { progress })
    }
}

fn ping_duration() -> Duration {
    Duration::from_millis(super::scroll_bottom::question_ping_ms())
}

/// The dot in the pill's top right corner, inside its edge so the pills' scroller never cuts it,
/// and, while a ping plays, the ring spreading out of it.
///
/// CDXC:SessionChat 2026-10-17 WHY:
/// The pills' sideways scroller clips everything outside its own box, which is exactly the pills' height, so a ring drawn as a plain child was cut flat. Padding the scroller (with negative margins to keep the row still) was dropped because its hit area would then cover the transcript's bottom and the composer's top edge. The ring is deferred instead: painted after the pane, outside the scroller's clip, and with no id it takes no input.
pub(super) fn attention_dot(frame: Option<PingFrame>, p: &ChatAppearance) -> Vec<AnyElement> {
    let s = p.scale;
    let dot = super::scroll_bottom::question_ping_dot();
    let color = gpui::Hsla::from(rgb(super::chat_pills::ATTENTION_COLOR));
    // The dot's centre, which the ring spreads from.
    let center_top = super::chat_pills::CORNER_DOT_TOP + dot / 2.0;
    let center_right = super::chat_pills::CORNER_DOT_RIGHT + dot / 2.0;
    let mut parts = vec![super::chat_pills::corner_dot(s).into_any_element()];
    if let Some(frame) = frame {
        // Eased out: it leaves fast and slows as it fades.
        let spread = 1.0 - (1.0 - frame.progress).powi(3);
        let size = dot * (1.0 + (super::scroll_bottom::question_ping_scale() - 1.0) * spread);
        let alpha = super::scroll_bottom::question_ping_alpha() * (1.0 - frame.progress);
        parts.push(
            deferred(
                div()
                    .absolute()
                    .top(px((center_top - size / 2.0) * s))
                    .right(px((center_right - size / 2.0) * s))
                    .size(px(size * s))
                    .rounded_full()
                    .bg(color.opacity(alpha)),
            )
            .into_any_element(),
        );
    }
    parts
}
