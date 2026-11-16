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
//! - A sent message is only ever a row of the transcript, and it is there on the Enter itself.
//!   User, of a row this file drew above the chat box with "Sending…" under it: "seeing my sent
//!   message appear below the working bar. this should never happen. i dont know what is this
//!   Sending... crap too. we already have a ux for when i send a message and the agent didnt pick
//!   it up yet: it should show spinner next to the message or show the play / undo buttons next to
//!   it", "show it bottom of the chat transcript like we always did before". The row is the
//!   core's own projection of the message (gx-chat-core `composer::delivery::sent_row`), run here
//!   on the Enter so a busy core thread cannot delay it, and drawn by the transcript's own row
//!   code as the list's last row (`sent_row`, after the transcript's rows and before the status
//!   line). The core's row takes its place in the frame that says it has the message
//!   (`sendsTaken`). It carries the play button of a message sent to a working agent, as the
//!   core's row will, and the spinner of a message on its way once it has waited `SPINNER_AFTER`.
//!   Supersedes the "Sending" row of the 2026-11-11 decision above, which is gone.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! The send's own breadcrumb was routine and behind Settings > Debugging, so that night's fault
//! left no trace on the user's machine. One line per send is written always: the chat's state at
//! Enter (ready, working, why a send would be refused or held) and never the message's words.

use std::time::Duration;

use ghostex_gx_chat_core::TranscriptItem;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div};
use serde_json::json;

use super::appearance::ChatAppearance;

use super::state::NativeChatView;

/// How long the core gets to take a send before the log says it is slow and the action goes
/// again. The host thread answers a call in milliseconds; the watch keeps waiting and says so
/// again after twice as long, up to `SLOW_AT_MOST` apart.
const SLOW_AFTER: Duration = Duration::from_secs(4);
const SLOW_AT_MOST: Duration = Duration::from_secs(32);
/// How long the core gets before the view's row for a message shows the spinner of a message on
/// its way. In ordinary use the core's own row has replaced it long before.
const SPINNER_AFTER: Duration = Duration::from_millis(1_500);

/// One message the core has not said it has.
struct Unconfirmed {
    /// Its `sendRequestId`, which `sendsTaken` answers with.
    id: String,
    /// The action as it left, to send again as it was.
    action: serde_json::Value,
    left: web_time::Instant,
    /// The transcript row the core will draw for it, and the same row with the spinner of a
    /// message on its way. `None` for a queue or a compact, which the queue strip and the
    /// command's marker show.
    rows: Option<[TranscriptItem; 2]>,
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
        self.send_watch.get_or_insert_with(|| SendWatch {
            sends: Vec::new(),
            session,
            token,
            heard_since: false,
            wait: SLOW_AFTER,
        });
        let rows = self.sent_rows_for(&id, &action);
        let watch = self.send_watch.as_mut().expect("the watch made above");
        watch.sends.push(Unconfirmed {
            id,
            action,
            left: now,
            rows,
        });
        watch.token = token;
        watch.heard_since = false;
        watch.wait = SLOW_AFTER;
        self.arm_send_watch(token, SLOW_AFTER, cx);
        // The message is the transcript's last row from this frame on.
        self.sync_sent_rows();
        cx.notify();
        // Its spinner, if the core's row has not replaced it by then.
        cx.spawn(async move |chat, cx| {
            cx.background_executor().timer(SPINNER_AFTER).await;
            let _ = chat.update(cx, |chat, cx| {
                if chat.sent_rows_listed > 0 {
                    chat.list.remeasure();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The transcript rows for a message that just left as `action`: the core's own projection of
    /// it, plain and with the spinner. Only a send has one.
    fn sent_rows_for(&self, id: &str, action: &serde_json::Value) -> Option<[TranscriptItem; 2]> {
        use ghostex_gx_chat_core::composer::delivery::{SentRow, sent_row, sent_row_state};
        if action["type"] != "send" {
            return None;
        }
        let text = action["text"]
            .as_str()
            .filter(|text| !text.trim().is_empty())?;
        let sessions = crate::app::gx_chat::known_sessions();
        let project = |on_its_way: bool| {
            let row = SentRow {
                id,
                text,
                agent: self.snapshot["agent"].as_str(),
                working: self.snapshot["working"] == true,
                on_its_way,
                sessions: &sessions,
            };
            let state = sent_row_state(&row, crate::app::gx_chat::now_millis());
            sent_row(&state, &crate::app::gx_chat::chat_context(&state))
        };
        Some([project(false)?, project(true)?])
    }

    /// The rows the view draws for messages the core has not drawn yet: this session's, oldest
    /// first.
    fn sent_rows(&self) -> impl Iterator<Item = &Unconfirmed> {
        let session = self.session_key();
        self.send_watch
            .iter()
            .filter(move |watch| watch.session == session)
            .flat_map(|watch| watch.sends.iter())
            .filter(|send| send.rows.is_some())
    }

    /// Keeps the transcript list holding one row per message just sent, after the transcript's
    /// own rows (and before the status row, status_tail.rs).
    /// Answers whether the list changed.
    pub(super) fn sync_sent_rows(&mut self) -> bool {
        let wanted = self.sent_rows().count();
        if wanted == self.sent_rows_listed {
            return false;
        }
        if wanted > self.sent_rows_listed {
            self.follow_tail_if_near_end();
        }
        let rows = self.items.len();
        self.list.splice(rows..rows + self.sent_rows_listed, wanted);
        self.sent_rows_listed = wanted;
        true
    }

    /// The transcript row at list index `index` for the `at`th message just sent, drawn by the
    /// transcript's own row code.
    pub(super) fn sent_row(
        &mut self,
        index: usize,
        at: usize,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = ChatAppearance::current(&self.snapshot).on_window_glass(
            crate::app::helpers::window_glass_active_for(self.main_window),
        );
        let rows = self.transcript_list_rows();
        self.in_subagent = false;
        self.detail_rows.start_row(true, index, rows);
        let item = self.sent_rows().nth(at).and_then(|send| {
            let [plain, on_its_way] = send.rows.as_ref()?;
            Some(if send.left.elapsed() >= SPINNER_AFTER {
                on_its_way.clone()
            } else {
                plain.clone()
            })
        });
        // A row the list still holds for a moment after the core's took its place draws nothing.
        let content = item.map(|item| self.item_content(&item, None, true, &p, window, cx));
        self.detail_rows.end_row();
        div()
            .w_full()
            .flex()
            .justify_center()
            .children(content.map(|content| self.transcript_row_column(&p).child(content)))
            .into_any_element()
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
        // The core's own rows arrived in this frame; the view's give way to them.
        self.sync_sent_rows();
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
}
