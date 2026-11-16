//! Dictate in a Ghostex page's text boxes: the app gives a published page the app's own Dictate
//! (`voice_agent::Dictate`, the recording and gxserver's transcription the chat box uses), and
//! the page's question kit draws a microphone on each note, text and "Anything else?" box
//! (server/src/visual_pages/kit.rs). The kit asks through the page bridge (`page_dictation.js`);
//! the app answers with a `gx-dictation` event carrying the phase, the words or what went wrong,
//! and the kit types the words into the box the recording was started from. The floating page
//! window (app/window/visual_page_modal.rs) and the Files view's page
//! (app/native_docs/published_page.rs) each attach one to their page.
//!
//! CDXC:Dictate 2026-11-16 DECISION: User, of a page's questions form: "also can we get mic to be able to dictate in these fields in the inline html also". Every note box, text answer and "Anything else?" box of a page shows a microphone that dictates into that box, as the in-chat form's boxes do (Reply got one in 11.8.0, #627): press to record, press again to transcribe, Escape cancels, and so does the page leaving the screen (the recording is kept, as the chat box keeps it). The words go in at the caret (over the selection) the box had when the recording started, while its text is unchanged, and at its end otherwise. No new speech engine: the page reaches the app's Dictate, so the engine picked in Settings > Dictation transcribes, and the microphone shows only where the app dictates (`DICTATION_ENABLED`). The floating page window, the Files view's page and the phone's page sheet (apps/mobile/app/src/chat/native/cards/pageDictation.ts) all do this. A page cannot prove a press was the reader's, so a start is taken only while that page is on screen in the app the reader is using (`PageShown`); what it records goes back only into that page.
//! CDXC:Dictate 2026-11-16 SEE-ALSO: apps/desktop/src/app/native_chat/dictation.rs (the chat box's control and the switch), apps/desktop/src/app/voice_agent/dictation.rs (record first, transcribe second).
use crate::app::voice_agent::{Dictate, DictateEvent, DictatePhase};
use crate::*;
use gpui::{App, AppContext as _, Entity, Subscription, Task};
use serde_json::{Value, json};
use std::rc::Rc;
use std::time::Duration;

/// Added to every document the page's browser loads, after the page bridge's queue.
const PAGE_DICTATION_SCRIPT: &str = include_str!("page_dictation.js");

/// Whether this build dictates: the chat box's switch, on the systems whose audio records.
const PAGE_DICTATION: bool = crate::app::native_chat::dictation::DICTATION_ENABLED
    && cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ));

/// How often a running recording checks that its page is still on screen.
const SHOWN_CHECK: Duration = Duration::from_millis(500);

/// Whether the page is on screen in the window the reader is using: a start is taken only then,
/// and a recording whose page left the screen is cancelled.
pub(crate) type PageShown = Rc<dyn Fn(&mut App) -> bool>;

/// A page's Dictate, for as long as its browser lives.
pub(crate) struct PageDictation {
    dictate: Entity<Dictate>,
    _bridge: Task<()>,
    _watch: Task<()>,
    _events: Subscription,
    _phase: Subscription,
}

impl PageDictation {
    /// Gives `surface`'s page the microphone; `None` where the app doesn't dictate or the page has
    /// no bridge.
    pub(crate) fn attach(
        surface: &Entity<CefSurface>,
        shown: PageShown,
        cx: &mut App,
    ) -> Option<Self> {
        if !PAGE_DICTATION {
            return None;
        }
        let mut bridge = CefSurface::install_page_bridge(surface, PAGE_DICTATION_SCRIPT, cx)?;
        let dictate = cx.new(|_| Dictate::new());
        let bridge_task = cx.spawn({
            let dictate = dictate.downgrade();
            let shown = shown.clone();
            async move |cx| {
                while let Some(message) = bridge.next().await {
                    let Ok(message) = serde_json::from_str::<Value>(&message) else {
                        continue;
                    };
                    if message["type"] != "dictate" {
                        continue;
                    }
                    let action = message["action"].as_str().unwrap_or_default().to_string();
                    let _ = cx.update(|cx| {
                        let Some(dictate) = dictate.upgrade() else {
                            return;
                        };
                        match action.as_str() {
                            "cancel" => dictate.update(cx, |dictate, cx| dictate.cancel(cx)),
                            // A start only from a page on screen (the DECISION at the top).
                            "toggle" if shown(cx) => {
                                dictate.update(cx, |dictate, cx| dictate.toggle(cx))
                            }
                            _ => {}
                        }
                    });
                }
            }
        });
        let watch = cx.spawn({
            let dictate = dictate.downgrade();
            async move |cx| {
                loop {
                    cx.background_executor().timer(SHOWN_CHECK).await;
                    let alive = cx.update(|cx| {
                        let Some(dictate) = dictate.upgrade() else {
                            return false;
                        };
                        if dictate.read(cx).recording() && !shown(cx) {
                            dictate.update(cx, |dictate, cx| dictate.cancel(cx));
                        }
                        true
                    });
                    if !alive {
                        return;
                    }
                }
            }
        });
        let page = surface.downgrade();
        let events = cx.subscribe(&dictate, {
            let page = page.clone();
            move |_, event, cx| {
                let detail = match event {
                    DictateEvent::Text(text) => json!({ "text": text }),
                    DictateEvent::Failure { title, message } => {
                        json!({ "error": title, "message": message })
                    }
                };
                tell_page(&page, &detail, cx);
            }
        });
        let phase = cx.observe(&dictate, move |dictate, cx| {
            let phase = match dictate.read(cx).phase() {
                DictatePhase::Starting => "starting",
                DictatePhase::Recording => "recording",
                DictatePhase::Transcribing => "transcribing",
                DictatePhase::Idle
                | DictatePhase::Failed { .. }
                | DictatePhase::Unavailable { .. }
                | DictatePhase::Notice { .. } => "idle",
            };
            tell_page(&page, &json!({ "phase": phase }), cx);
        });
        Some(Self {
            dictate,
            _bridge: bridge_task,
            _watch: watch,
            _events: events,
            _phase: phase,
        })
    }

    /// Stops a recording without typing it (kept as dismissed); a transcription in flight types
    /// nothing either.
    pub(crate) fn cancel(&self, cx: &mut App) {
        self.dictate.update(cx, |dictate, cx| dictate.cancel(cx));
    }
}

/// Hands the page a `gx-dictation` event with `detail`.
fn tell_page(page: &gpui::WeakEntity<CefSurface>, detail: &Value, cx: &mut App) {
    let (Some(page), Ok(detail)) = (page.upgrade(), serde_json::to_string(detail)) else {
        return;
    };
    let script =
        format!("window.dispatchEvent(new CustomEvent('gx-dictation',{{detail:{detail}}}));");
    page.update(cx, |page, _| page.execute_app_owned_script(&script));
}
