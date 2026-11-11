//! A message sent from the chat box shows above the box until the chat core has it, and every send
//! leaves one line in the session chat log.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User: "my messages to you weren't being sent from chat view!!! please fix wtf is this", then "text
//! vanished but didn't show in the chat, even as optimistic message like we usually do", and "after
//! like 2 mins my message actually did get sent". The chat box empties on Enter before the core has
//! the message (composer.rs `submit`). The core counts what it takes (`submissionsReceived`); until
//! it counts a send, the view draws it itself as "Sending" (`render_unconfirmed_sends`).
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User: "i'm seeing double messages now too", and of the notification that his message was back in
//! the chat box: "you should self heal, don't show me this message, why do i need to press enter
//! again when you can press it for me". A send the core had not counted within `SLOW_AFTER` used to
//! go back into the box with a notification. The core's thread was only busy (18 s at 05:06): the
//! message was still in its channel and went out by itself, and the Enter on the returned words
//! sent it a second time under a new send request id. So a send is never taken back from the core:
//! the watch only waits and writes `sessionChat.sendSlow`, and the row stays "Sending" until the
//! core counts it. A message the core takes but holds says why under its own bubble
//! (gx-chat-core composer/policy.rs `hold_words`).
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! The send's own breadcrumb was routine and behind Settings > Debugging, so tonight's fault left no
//! trace on the user's machine. One line per send is written always: the chat's state at Enter
//! (ready, working, why a send would be refused or held) and never the message's words.

use std::time::Duration;

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::json;

use super::appearance::ChatAppearance;

use super::state::NativeChatView;

/// How long the core gets to count a send before the log says it is slow. The host thread answers
/// a call in milliseconds; a busy thread still takes the message later, so the watch keeps waiting
/// and says so again after twice as long, up to `SLOW_AT_MOST` apart.
const SLOW_AFTER: Duration = Duration::from_secs(8);
const SLOW_AT_MOST: Duration = Duration::from_secs(64);

/// The sends the core has not counted yet.
pub(crate) struct SendWatch {
    /// Each send's words, the `submissionsReceived` that says the core has it, and when it left
    /// the box, oldest first.
    sends: Vec<(u64, String, web_time::Instant)>,
    /// The count when the oldest of them left: a count below it is another core.
    base: u64,
    token: u64,
    /// The host thread drew something after the newest send left, so it is alive.
    heard_since: bool,
    /// How long the watch waits before its next `sessionChat.sendSlow` line.
    wait: Duration,
}

/// A message the send gate holds, as things stood when the hold began.
pub(crate) struct HeldLog {
    since: web_time::Instant,
    reason: serde_json::Value,
    notice: serde_json::Value,
    working: serde_json::Value,
    chat_shown: bool,
}

/// The terminal notice as the send log records it: what it is, whether it holds the agent's input,
/// and whether its card is drawn above the chat box.
fn notice_facts(snapshot: &serde_json::Value) -> serde_json::Value {
    let notice = &snapshot["terminalNotice"];
    json!({
        "kind": notice["kind"],
        "title": notice["title"],
        "detectedAt": notice["detectedAt"],
        "holdsInput": notice["choices"].as_array().is_some_and(|rows| !rows.is_empty())
            || notice["dialog"].is_object(),
        "cardShown": notice.is_object() && snapshot["noticeVisible"] == true,
    })
}

impl NativeChatView {
    /// The always-kept line for a send from the chat box, written before anything can stop it. An
    /// Enter on an empty box is no send and leaves no line.
    pub(super) fn log_send(&self, mode: &str, interrupt: bool) {
        if self.draft.trim().is_empty() && !self.attachments_on_their_way() {
            return;
        }
        let snapshot = &self.snapshot;
        crate::support_logs::append_always(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "sessionChat.send",
            json!({
                "session": format!("{}:{}", self.config.project_id, self.config.session_id),
                "mode": mode,
                "interrupt": interrupt,
                "composerReady": self.composer_ready,
                "hasRuntime": self.runtime.is_some(),
                "chars": self.draft.chars().count(),
                "pendingSend": self.pending_send,
                "status": snapshot["status"],
                "working": snapshot["working"],
                "refused": snapshot["sendBlockedReason"],
                "held": snapshot["sendHoldReason"],
                "received": snapshot["submissionsReceived"],
                // What stood above the chat box at Enter: the terminal's notice and whether its
                // card was drawn (a notice that holds the agent's input with no card holds sends),
                // and a question that replaces the box; and whether the chat was on screen at all
                // (its session may show its terminal instead).
                "notice": notice_facts(snapshot),
                "chatShown": !self.pane_hidden,
                "questionCard": snapshot["questionCard"]["visible"],
                "unconfirmed": self.send_watch.as_ref().map_or(0, |watch| watch.sends.len()),
            }),
        );
    }

    /// Starts watching the send that just left the box with `text`.
    pub(super) fn watch_send(&mut self, text: String, cx: &mut Context<Self>) {
        let now = web_time::Instant::now();
        self.last_send_at = Some(now);
        // A core too old to count publishes no count: nothing to compare with.
        let Some(seen) = self.snapshot["submissionsReceived"].as_u64() else {
            return;
        };
        self.send_watch_token = self.send_watch_token.wrapping_add(1);
        let token = self.send_watch_token;
        let watch = self.send_watch.get_or_insert_with(|| SendWatch {
            sends: Vec::new(),
            base: seen,
            token,
            heard_since: false,
            wait: SLOW_AFTER,
        });
        // A send still unconfirmed when the next leaves is counted by the same core, in order.
        let expected = watch.sends.last().map_or(seen, |(expected, ..)| *expected) + 1;
        watch.sends.push((expected, text, now));
        watch.token = token;
        watch.heard_since = false;
        watch.wait = SLOW_AFTER;
        self.arm_send_watch(token, SLOW_AFTER, cx);
        // The "Sending" row shows on this frame, before the core's thread has done anything.
        cx.notify();
    }

    fn arm_send_watch(&mut self, token: u64, wait: Duration, cx: &mut Context<Self>) {
        cx.spawn(async move |chat, cx| {
            cx.background_executor().timer(wait).await;
            let _ = chat.update(cx, |chat, cx| chat.send_watch_due(token, cx));
        })
        .detach();
    }

    /// A frame from the core: it counts sends, or at least shows the thread is alive.
    pub(super) fn note_send_progress(&mut self) {
        let Some(watch) = self.send_watch.as_mut() else {
            return;
        };
        watch.heard_since = true;
        let Some(received) = self.snapshot["submissionsReceived"].as_u64() else {
            return;
        };
        // A count below the one the watch started from is another core (a new session bound to
        // this view), which cannot be compared with.
        if received < watch.base {
            self.send_watch = None;
            return;
        }
        let session = format!("{}:{}", self.config.project_id, self.config.session_id);
        watch.sends.retain(|(expected, _, left)| {
            if *expected > received {
                return true;
            }
            // How long the core's thread took to take it in: the wait a starved app shows as a
            // message missing from the chat.
            crate::support_logs::append_always(
                crate::support_logs::GpuiSupportLog::SessionChat,
                "sessionChat.sendTaken",
                json!({ "session": session, "afterEnterMs": left.elapsed().as_millis() as u64 }),
            );
            false
        });
        if watch.sends.is_empty() {
            self.send_watch = None;
        }
    }

    /// The core has not counted the watched sends yet. They are still its to send (in its thread's
    /// channel, or behind the send in flight), so nothing is taken back and nothing is sent again:
    /// the row stays "Sending", the log says how long it has been, and the watch waits on.
    fn send_watch_due(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some(watch) = self
            .send_watch
            .as_mut()
            .filter(|watch| watch.token == token)
        else {
            return;
        };
        crate::support_logs::append_always(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "sessionChat.sendSlow",
            json!({
                "session": format!("{}:{}", self.config.project_id, self.config.session_id),
                "sends": watch.sends.len(),
                "waitedMs": watch.sends.first().map(|(_, _, left)| left.elapsed().as_millis() as u64),
                "expected": watch.sends.last().map(|(expected, ..)| *expected),
                "received": self.snapshot["submissionsReceived"],
                "heardSince": watch.heard_since,
                "hasRuntime": self.runtime.is_some(),
            }),
        );
        watch.wait = (watch.wait * 2).min(SLOW_AT_MOST);
        let wait = watch.wait;
        self.arm_send_watch(token, wait, cx);
    }

    /// Milliseconds since the last send left the chat box.
    pub(super) fn since_last_send_ms(&self) -> Option<u64> {
        self.last_send_at
            .map(|left| left.elapsed().as_millis() as u64)
    }

    /// A message's call to gxserver answered: when it left after Enter and how long it took.
    pub(super) fn log_send_call(
        &self,
        method: &str,
        after_enter_ms: Option<u64>,
        started: web_time::Instant,
        error: &serde_json::Value,
    ) {
        crate::support_logs::append_always(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "sessionChat.sendCall",
            json!({
                "session": format!("{}:{}", self.config.project_id, self.config.session_id),
                "method": method,
                "leftAfterEnterMs": after_enter_ms,
                "tookMs": started.elapsed().as_millis() as u64,
                "succeeded": error.is_null(),
                "errorCode": error["code"],
                "chatShown": !self.pane_hidden,
            }),
        );
    }

    /// A frame moved the held message: the gate took one (`sessionChat.sendHeld`) or let it go
    /// (`sessionChat.sendReleased`), with what changed in between, so a later reader can tell what
    /// released it: the notice leaving the agent's screen (for example once its terminal view was
    /// opened and the screen read again), the turn ending, or a switch finishing.
    pub(super) fn note_held_send(&mut self, before: &serde_json::Value) {
        let now = &self.snapshot["sendHeld"];
        let session = format!("{}:{}", self.config.project_id, self.config.session_id);
        match (before["sendHeld"].is_string(), now.is_string()) {
            (false, true) => {
                let held = HeldLog {
                    since: web_time::Instant::now(),
                    reason: now.clone(),
                    notice: notice_facts(&self.snapshot),
                    working: self.snapshot["working"].clone(),
                    chat_shown: !self.pane_hidden,
                };
                crate::support_logs::append_always(
                    crate::support_logs::GpuiSupportLog::SessionChat,
                    "sessionChat.sendHeld",
                    json!({
                        "session": session,
                        "reason": held.reason,
                        "notice": held.notice,
                        "working": held.working,
                        "chatShown": held.chat_shown,
                    }),
                );
                self.held_log = Some(held);
            }
            (true, false) => {
                let held = self.held_log.take();
                crate::support_logs::append_always(
                    crate::support_logs::GpuiSupportLog::SessionChat,
                    "sessionChat.sendReleased",
                    json!({
                        "session": session,
                        "heldMs": held.as_ref().map(|held| held.since.elapsed().as_millis() as u64),
                        "reasonAtHold": held.as_ref().map(|held| held.reason.clone()),
                        "noticeAtHold": held.as_ref().map(|held| held.notice.clone()),
                        "noticeNow": notice_facts(&self.snapshot),
                        "workingAtHold": held.as_ref().map(|held| held.working.clone()),
                        "workingNow": self.snapshot["working"],
                        "chatShownAtHold": held.as_ref().map(|held| held.chat_shown),
                        "chatShownNow": !self.pane_hidden,
                    }),
                );
            }
            _ => {}
        }
    }

    /// The messages that left the box and the core has not taken in yet, drawn by the view itself
    /// at once ("Sending"), so a busy core thread never leaves a sent message nowhere on screen.
    /// Each row goes the frame the core counts it, which is the frame its own echo arrives in.
    pub(super) fn render_unconfirmed_sends(&self, p: &ChatAppearance) -> Option<AnyElement> {
        let watch = self.send_watch.as_ref()?;
        if watch.sends.is_empty() {
            return None;
        }
        let s = p.scale;
        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(6.0 * s))
                .children(watch.sends.iter().map(|(expected, text, _)| {
                    div()
                        .id(gpui::SharedString::from(format!(
                            "unconfirmed-send:{expected}"
                        )))
                        .role(gpui::Role::Status)
                        .aria_label("Sending")
                        .max_w(gpui::relative(0.8))
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(2.0 * s))
                        .child(
                            div()
                                .px(px(12.0 * s))
                                .py(px(8.0 * s))
                                .rounded(px(12.0 * s))
                                .bg(p.user_band)
                                .text_color(p.foreground)
                                .text_size(px(14.0 * s))
                                .line_clamp(3)
                                .child(text.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0 * s))
                                .text_color(p.muted)
                                .child("Sending\u{2026}"),
                        )
                })),
        )
        .map(IntoElement::into_any_element)
    }
}
