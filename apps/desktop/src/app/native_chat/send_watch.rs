//! The chat box's half of a send: it names the message at Enter, watches until the chat core has
//! it, sends the same action again while it cannot see it taken, and leaves a line in the session
//! chat log for every step.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User: "my messages to you weren't being sent from chat view!!! please fix wtf is this", "text
//! vanished but didn't show in the chat, even as optimistic message like we usually do", "after
//! like 2 mins my message actually did get sent"; then, of a watch here that put a message it
//! thought lost back in the box with a notification: "i'm seeing double messages now too", "you
//! should self heal, don't show me this message, why do i need to press enter again when you can
//! press it for me". The message had only been waiting for the core's busy thread: it went out by
//! itself and his Enter on the returned words sent it again under a new id. So:
//!
//! - The box names the message at Enter (composer.rs `submit`: `sendRequestId`). The core takes an
//!   action carrying that id once and publishes the ids it has (`sendsTaken`), and every attempt
//!   of the message reaches gxserver under it, which delivers it once
//!   (packages/gx-chat-core/src/composer/delivery.rs).
//! - A send is never taken back and never handed back. While the core has not said it has the
//!   message, the view sends the SAME action again (`send_watch_due`), which is harmless when the
//!   core was only busy and is what gets the message there when the first call met a core that
//!   was not listening (no runtime yet, a chat the host had let go).
//! - The message's own row is the core's, drawn the moment it takes the action, which is at most
//!   one of its turns away now that a view's call goes ahead of the frames its socket read
//!   (gx_chat/worker.rs). Only when even that takes longer than `ROW_AFTER` does the view draw a
//!   plain "Sending" row of its own above the box, so a message is never nowhere on screen.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! The send's own breadcrumb was routine and behind Settings > Debugging, so that night's fault
//! left no trace on the user's machine. One line per send is written always: the chat's state at
//! Enter (ready, working, why a send would be refused or held) and never the message's words.

use std::time::Duration;

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::json;

use super::appearance::ChatAppearance;

use super::state::NativeChatView;

/// How long the core gets to take a send before the log says it is slow and the action goes
/// again. The host thread answers a call in milliseconds; the watch keeps waiting and says so
/// again after twice as long, up to `SLOW_AT_MOST` apart.
const SLOW_AFTER: Duration = Duration::from_secs(4);
const SLOW_AT_MOST: Duration = Duration::from_secs(32);
/// How long the core gets before the view draws the message itself. Longer than a busy turn of
/// the core's thread, so in ordinary use the only row a message ever has is its real one.
const ROW_AFTER: Duration = Duration::from_millis(250);

/// One message the core has not said it has.
struct Unconfirmed {
    /// Its `sendRequestId`, which `sendsTaken` answers with.
    id: String,
    /// The action as it left, to send again as it was.
    action: serde_json::Value,
    left: web_time::Instant,
}

/// The sends the core has not taken yet.
pub(crate) struct SendWatch {
    /// Oldest first.
    sends: Vec<Unconfirmed>,
    /// The session they were sent to: a view shown another session sends nothing again.
    session: String,
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

    fn session_key(&self) -> String {
        format!("{}:{}", self.config.project_id, self.config.session_id)
    }

    /// Starts watching the send that just left the box as `action`, named `id`.
    pub(super) fn watch_send(
        &mut self,
        id: String,
        action: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let now = web_time::Instant::now();
        self.last_send_at = Some(now);
        self.send_watch_token = self.send_watch_token.wrapping_add(1);
        let token = self.send_watch_token;
        let session = self.session_key();
        // Sends to another session are not this watch's to send again.
        if self
            .send_watch
            .as_ref()
            .is_some_and(|watch| watch.session != session)
        {
            self.send_watch = None;
        }
        let watch = self.send_watch.get_or_insert_with(|| SendWatch {
            sends: Vec::new(),
            session,
            token,
            heard_since: false,
            wait: SLOW_AFTER,
        });
        watch.sends.push(Unconfirmed {
            id,
            action,
            left: now,
        });
        watch.token = token;
        watch.heard_since = false;
        watch.wait = SLOW_AFTER;
        self.arm_send_watch(token, SLOW_AFTER, cx);
        // The view's own row, only if the core's has not come by then.
        cx.spawn(async move |chat, cx| {
            cx.background_executor().timer(ROW_AFTER).await;
            let _ = chat.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn arm_send_watch(&mut self, token: u64, wait: Duration, cx: &mut Context<Self>) {
        cx.spawn(async move |chat, cx| {
            cx.background_executor().timer(wait).await;
            let _ = chat.update(cx, |chat, cx| chat.send_watch_due(token, cx));
        })
        .detach();
    }

    /// A frame from the core: it says which sends it has, or at least shows the thread is alive.
    pub(super) fn note_send_progress(&mut self) {
        let session = self.session_key();
        let Some(watch) = self.send_watch.as_mut() else {
            return;
        };
        watch.heard_since = true;
        if watch.session != session {
            return;
        }
        let taken = self.snapshot["sendsTaken"].as_array();
        watch.sends.retain(|send| {
            let has = taken.is_some_and(|taken| taken.iter().any(|id| id == send.id.as_str()));
            if !has {
                return true;
            }
            // How long the core's thread took to take it in: the wait a starved app shows as a
            // message missing from the chat.
            crate::support_logs::append_always(
                crate::support_logs::GpuiSupportLog::SessionChat,
                "sessionChat.sendTaken",
                json!({ "session": session, "afterEnterMs": send.left.elapsed().as_millis() as u64 }),
            );
            false
        });
        if watch.sends.is_empty() {
            self.send_watch = None;
        }
    }

    /// The core has not said it has the watched sends. Nothing is taken back: the log says how
    /// long it has been, and each goes to the core again as the action it was, under its own
    /// id. A core that was only busy takes the first copy and passes over the rest; a core that
    /// never heard the first (no runtime then, a chat the host had let go) takes this one.
    fn send_watch_due(&mut self, token: u64, cx: &mut Context<Self>) {
        let session = self.session_key();
        let Some(watch) = self
            .send_watch
            .as_mut()
            .filter(|watch| watch.token == token)
        else {
            return;
        };
        let same_session = watch.session == session;
        crate::support_logs::append_always(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "sessionChat.sendSlow",
            json!({
                "session": watch.session,
                "sends": watch.sends.len(),
                "waitedMs": watch.sends.first().map(|send| send.left.elapsed().as_millis() as u64),
                "taken": self.snapshot["sendsTaken"].as_array().map(Vec::len),
                "heardSince": watch.heard_since,
                "hasRuntime": self.runtime.is_some(),
                "sentAgain": same_session && self.runtime.is_some(),
            }),
        );
        watch.wait = (watch.wait * 2).min(SLOW_AT_MOST);
        let wait = watch.wait;
        let again: Vec<serde_json::Value> = if same_session {
            watch.sends.iter().map(|send| send.action.clone()).collect()
        } else {
            Vec::new()
        };
        self.arm_send_watch(token, wait, cx);
        if let Some(runtime) = &self.runtime {
            for action in again {
                runtime.call("action", vec![action]);
            }
        }
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

    /// The messages that left the box more than `ROW_AFTER` ago and the core still has not
    /// taken, drawn by the view itself ("Sending"), so a core thread busy for longer than it ever
    /// should be never leaves a sent message nowhere on screen. Each row goes the frame the core
    /// says it has the message, which is the frame its own row arrives in.
    pub(super) fn render_unconfirmed_sends(&self, p: &ChatAppearance) -> Option<AnyElement> {
        let watch = self.send_watch.as_ref()?;
        if watch.session != self.session_key() {
            return None;
        }
        let late: Vec<&Unconfirmed> = watch
            .sends
            .iter()
            .filter(|send| send.left.elapsed() >= ROW_AFTER)
            .collect();
        if late.is_empty() {
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
                .children(late.into_iter().map(|send| {
                    let text = plain_words(send.action["text"].as_str().unwrap_or_default());
                    div()
                        .id(gpui::SharedString::from(format!(
                            "unconfirmed-send:{}",
                            send.id
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
                                .child(text),
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

/// A message's words for the view's own "Sending" row: a picture or file reference reads as its
/// label alone (`[Image #1]`), not as its Markdown link with the path.
fn plain_words(text: &str) -> String {
    let mut words = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let (before, from) = rest.split_at(open);
        words.push_str(before);
        let link = from.find("](").and_then(|close| {
            let label = &from[1..close];
            let target = &from[close + 2..];
            let end = target.find(')')?;
            // A label on one line with a target on one line: a reference, not prose in brackets.
            (!label.is_empty() && !label.contains('\n') && !target[..end].contains('\n'))
                .then(|| (label, &target[end + 1..]))
        });
        match link {
            Some((label, after)) => {
                words.push('[');
                words.push_str(label);
                words.push(']');
                rest = after;
            }
            None => {
                words.push('[');
                rest = &from[1..];
            }
        }
    }
    words.push_str(rest);
    words
}

#[cfg(test)]
mod tests {
    use super::plain_words;

    #[test]
    fn a_reference_reads_as_its_label() {
        assert_eq!(
            plain_words("look\n[Image #1](/Users/me/Screenshots/shot@2x.png) and [notes](a/b.md)."),
            "look\n[Image #1] and [notes]."
        );
        assert_eq!(
            plain_words("no [brackets] here (at all)"),
            "no [brackets] here (at all)"
        );
        assert_eq!(plain_words("an [open"), "an [open");
        assert_eq!(plain_words(""), "");
    }
}
