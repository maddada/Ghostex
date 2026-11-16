//! Dictate in a page's text boxes: the floating page window gives its page the app's own Dictate
//! (`voice_agent::Dictate`, the recording and gxserver's transcription the chat box uses), and
//! the page's question kit draws a microphone on each note, text and "Anything else?" box
//! (server/src/visual_pages/kit.rs). The kit asks through the page bridge (`page_dictation.js`);
//! the app answers with a `gx-dictation` event carrying the phase, the words or what went wrong,
//! and the kit types the words into the box the recording was started from.
//!
//! CDXC:Dictate 2026-11-16 DECISION: User, of a page's questions form: "also can we get mic to be able to dictate in these fields in the inline html also". Every note box, text answer and "Anything else?" box of a page shows a microphone that dictates into that box, as the in-chat form's boxes do (Reply got one in 11.8.0, #627): press to record, press again to transcribe, Escape or a click on the window's Hide cancels (the recording is kept, as the chat box keeps it). The words go in at the caret (over the selection) the box had when the recording started, while its text is unchanged, and at its end otherwise. No new speech engine: the page reaches the app's Dictate, so the engine picked in Settings > Dictation transcribes, and the microphone shows only where the app dictates (`DICTATION_ENABLED`). A page cannot prove a press was the reader's, so a start is taken only while the page's window is the active one and shown; what it records goes back only into that page.
//! CDXC:Dictate 2026-11-16 SEE-ALSO: apps/desktop/src/app/native_chat/dictation.rs (the chat box's control and the switch), apps/desktop/src/app/voice_agent/dictation.rs (record first, transcribe second).
use super::GpuiVisualPageModalWindow;
use crate::app::voice_agent::{Dictate, DictateEvent, DictatePhase};
use crate::*;
use gpui::{AppContext as _, Context, Entity, Subscription, Task, Window};
use serde_json::{Value, json};

/// Added to every document the page window loads, after the page bridge's queue.
const PAGE_DICTATION_SCRIPT: &str = include_str!("page_dictation.js");

/// Whether this build dictates: the chat box's switch, on the systems whose audio records.
const PAGE_DICTATION: bool = crate::app::native_chat::dictation::DICTATION_ENABLED
    && cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ));

/// The page window's Dictate, for as long as its page lives.
pub(super) struct PageDictation {
    dictate: Entity<Dictate>,
    _bridge: Task<()>,
    _events: Subscription,
    _phase: Subscription,
}

impl PageDictation {
    /// Gives `surface`'s page the microphone; `None` where the app doesn't dictate or the page has
    /// no bridge.
    pub(super) fn attach(
        surface: &Entity<CefSurface>,
        window: &mut Window,
        cx: &mut Context<GpuiVisualPageModalWindow>,
    ) -> Option<Self> {
        if !PAGE_DICTATION {
            return None;
        }
        let mut bridge = CefSurface::install_page_bridge(surface, PAGE_DICTATION_SCRIPT, cx)?;
        let dictate = cx.new(|_| Dictate::new());
        let bridge_task = cx.spawn_in(window, async move |this, cx| {
            while let Some(message) = bridge.next().await {
                let Ok(message) = serde_json::from_str::<Value>(&message) else {
                    continue;
                };
                if message["type"] != "dictate" {
                    continue;
                }
                let action = message["action"].as_str().unwrap_or_default().to_string();
                if this
                    .update_in(cx, |this, window, cx| {
                        this.page_dictation_asked(&action, window, cx)
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
        let events = cx.subscribe(&dictate, |this, _, event, cx| {
            let detail = match event {
                DictateEvent::Text(text) => json!({ "text": text }),
                DictateEvent::Failure { title, message } => {
                    json!({ "error": title, "message": message })
                }
            };
            this.tell_page_dictation(&detail, cx);
        });
        let phase = cx.observe(&dictate, |this, dictate, cx| {
            let phase = match dictate.read(cx).phase() {
                DictatePhase::Starting => "starting",
                DictatePhase::Recording => "recording",
                DictatePhase::Transcribing => "transcribing",
                DictatePhase::Idle
                | DictatePhase::Failed { .. }
                | DictatePhase::Unavailable { .. }
                | DictatePhase::Notice { .. } => "idle",
            };
            this.tell_page_dictation(&json!({ "phase": phase }), cx);
        });
        Some(Self {
            dictate,
            _bridge: bridge_task,
            _events: events,
            _phase: phase,
        })
    }

    /// Stops a recording without typing it (kept as dismissed); a transcription in flight types
    /// nothing either.
    pub(super) fn cancel(&self, cx: &mut gpui::App) {
        self.dictate.update(cx, |dictate, cx| dictate.cancel(cx));
    }
}

impl GpuiVisualPageModalWindow {
    /// The page's microphone was pressed (`toggle`) or Escape cancelled it (`cancel`).
    fn page_dictation_asked(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dictation) = &self.dictation else {
            return;
        };
        match action {
            "cancel" => dictation.cancel(cx),
            // A start only from the window the reader is using (the DECISION at the top).
            "toggle" if !self.hidden && window.is_window_active() => {
                dictation
                    .dictate
                    .update(cx, |dictate, cx| dictate.toggle(cx));
            }
            _ => {}
        }
    }

    /// Hands the page a `gx-dictation` event with `detail`.
    fn tell_page_dictation(&self, detail: &Value, cx: &mut Context<Self>) {
        let Some(surface) = &self.surface else {
            return;
        };
        let Ok(detail) = serde_json::to_string(detail) else {
            return;
        };
        let script =
            format!("window.dispatchEvent(new CustomEvent('gx-dictation',{{detail:{detail}}}));");
        surface.update(cx, |surface, _| surface.execute_app_owned_script(&script));
    }
}
