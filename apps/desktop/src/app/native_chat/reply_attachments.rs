//! Voice and pictures in the Reply card (`reply_popover.rs`): its microphone dictates into its
//! field, and a picture pasted into it or picked with its paperclip is saved the way the chat box
//! saves one and written into the field as the chat box's `[Image #1](path)` pill, which reaches
//! the agent with the words.
//!
//! CDXC:Coordinators 2026-11-16 DECISION:
//! User, on issue #627 ("the Reply box has no dictation and no pictures"), picked voice and
//! pictures right in the Reply: a dictate button in it, and pictures pasted or attached there.
//! The paste follows the question forms' boxes (`form_attachments.rs`, the same save and pill);
//! the words follow Dictate's rules for the box it was aimed at (`dictation.rs`), at the caret or
//! over the selection, spaced while Insert with spacing is on. The card is a window of its own, so
//! its field is written through that window.
//! SEE-ALSO: apps/mobile/app/src/chat/native/transcript/ReplySheet.tsx (the phone's Reply sheet).

use super::{clipboard::PasteInto, state::NativeChatView};
use gpui::{App, ClipboardEntry, ClipboardItem, Context, Entity, ExternalPaths, Window};
use gpui_component::input::TextareaState;
use serde_json::{Value, json};
use std::time::Duration;

/// The dictation target that names the Reply card's field (`DictationControl::target`); form box
/// keys are `<form>:<slot>`, which this can never be.
pub(super) const REPLY_DICTATION: &str = "\u{1}reply";

/// What the Reply card's microphone shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplyDictation {
    Idle,
    Recording,
    Transcribing,
}

impl NativeChatView {
    /// The `on_paste` hook of the Reply card's field.
    pub(super) fn reply_paste_handler(
        chat: gpui::WeakEntity<Self>,
        input: Entity<TextareaState>,
    ) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
        move |clipboard, _, cx| {
            let Some(main) = chat.upgrade().and_then(|chat| chat.read(cx).main_window) else {
                return false;
            };
            let chat = chat.clone();
            let input = input.clone();
            let clipboard = clipboard.clone();
            // Only a picture or a file is taken; text falls through to the field.
            let takes = clipboard.entries.iter().any(|entry| {
                matches!(
                    entry,
                    ClipboardEntry::Image(_) | ClipboardEntry::ExternalPaths(_)
                )
            });
            if !takes {
                return false;
            }
            cx.defer(move |cx| {
                let _ = main.update(cx, |_, window, cx| {
                    let _ = chat.update(cx, |chat, cx| {
                        chat.paste_attachments(&clipboard, PasteInto::Reply(input), window, cx)
                    });
                });
            });
            true
        }
    }

    /// Files picked with the Reply card's paperclip: saved as a pasted file is.
    pub(super) fn attach_reply_paths(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        input: Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        let clipboard = ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
                paths.into_iter().collect(),
            ))],
        };
        self.paste_attachments(&clipboard, PasteInto::Reply(input), window, cx);
    }

    pub(super) fn reply_paste_started(&mut self, cx: &mut Context<Self>) {
        self.reply_popover.pending += 1;
        self.reply_popover.attach_error = None;
        cx.notify();
    }

    /// The saved files' references go into the Reply card's field at the selection the paste
    /// replaced, numbered after the pictures already in it.
    pub(super) fn finish_reply_attachments(
        &mut self,
        input: Entity<TextareaState>,
        (original, start, end): (String, usize, usize),
        paths: Vec<Value>,
        mut error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.reply_popover.pending = self.reply_popover.pending.saturating_sub(1);
        if !paths.is_empty() {
            let current = input.read(cx).value().to_string();
            let result = self.runtime.as_ref().and_then(|runtime| {
                runtime.query(
                    "insertAnswerAttachments",
                    vec![
                        json!(current),
                        json!(paths),
                        json!(original),
                        json!(start),
                        json!(end),
                    ],
                    Duration::from_millis(500),
                )
            });
            match result.as_ref().and_then(|result| {
                Some((
                    result["text"].as_str()?.to_owned(),
                    result["caret"].as_u64().unwrap_or(0) as usize,
                ))
            }) {
                Some((text, caret)) => self.set_reply_text(&input, text, caret, cx),
                None => {
                    error =
                        Some("The picture could not be added. Please paste it again.".to_string())
                }
            }
        }
        self.reply_popover.attach_error = error;
        cx.notify();
    }

    /// Writes `text` into the Reply card's field with the caret at `caret` (UTF-16), through the
    /// card's own window; nothing when the card is gone.
    fn set_reply_text(
        &self,
        input: &Entity<TextareaState>,
        text: String,
        caret: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.reply_popover.handle else {
            return;
        };
        let input = input.clone();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let caret = super::form_attachments::utf16_to_byte(&text, caret);
                input.update(cx, |input, cx| {
                    input.replace_all(text.clone(), window, cx);
                    input.set_selected_range(caret..caret, cx);
                });
            });
        });
    }

    /// The Reply card's microphone: start recording into its field, or stop and transcribe.
    pub(super) fn toggle_reply_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dictate = self.dictate(window, cx);
        self.aim_dictation(&dictate, Some(REPLY_DICTATION.to_string()), cx);
        dictate.update(cx, |dictate, cx| dictate.toggle(cx));
        cx.notify();
    }

    /// What the Reply card's microphone shows: a run aimed at its field records or transcribes.
    pub(super) fn reply_dictation(&self, cx: &App) -> ReplyDictation {
        let Some(control) = self.dictation.as_ref() else {
            return ReplyDictation::Idle;
        };
        if control.target.as_deref() != Some(REPLY_DICTATION) {
            return ReplyDictation::Idle;
        }
        let dictate = control.dictate.read(cx);
        if dictate.recording() {
            ReplyDictation::Recording
        } else if dictate.transcribing() {
            ReplyDictation::Transcribing
        } else {
            ReplyDictation::Idle
        }
    }

    /// The Reply card's field and its selection, while the card is open.
    pub(super) fn reply_field(&self, cx: &App) -> Option<(String, std::ops::Range<usize>)> {
        self.reply_popover.handle?;
        let input = self.reply_popover.input.as_ref()?.upgrade()?;
        let input = input.read(cx);
        Some((input.value().to_string(), input.selected_range()))
    }

    /// Dictate's words for the Reply card's field: at the place it was aimed at while the field
    /// still shows that text, else at its caret, spaced from their neighbours while Insert with
    /// spacing is on. Answers false when the card is gone, so the words go to the chat box.
    pub(super) fn insert_reply_dictation(
        &mut self,
        text: &str,
        anchor: &Option<(String, Option<std::ops::Range<usize>>)>,
        spacing: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self
            .reply_popover
            .input
            .as_ref()
            .and_then(|input| input.upgrade())
            .filter(|_| self.reply_popover.handle.is_some())
        else {
            return false;
        };
        let (shown, selected) = {
            let input = input.read(cx);
            (input.value().to_string(), input.selected_range())
        };
        let range = match anchor {
            Some((aimed, Some(range))) if *aimed == shown => range.clone(),
            _ => selected,
        };
        let (start, end) = (range.start.min(shown.len()), range.end.min(shown.len()));
        let words = if spacing {
            ghostex_gx_chat_core::questions::dictation::spaced_at_caret(
                shown[..start].chars().next_back(),
                shown[end..].chars().next(),
                text,
            )
        } else {
            text.to_string()
        };
        let next = format!("{}{words}{}", &shown[..start], &shown[end..]);
        let caret = format!("{}{words}", &shown[..start]).encode_utf16().count();
        self.set_reply_text(&input, next, caret, cx);
        true
    }
}
