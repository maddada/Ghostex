//! Images and files pasted into a question form's note or text boxes (`question_form.rs`): saved
//! the way the composer saves them (`clipboard.rs`), written into the box as the composer's
//! `[Image #N](path)` reference, and drawn as the composer's pills.
//!
//! CDXC:Clipboard 2026-11-02 DECISION:
//! User: "i also want to be able to paste images into the notes of questions in inline html or the inline questions you send please. when i paste an image now nothing happens. pls fix. (it can just show like this like we show it in chat: [Image #1](…/2026-11-02_Ghostex_06-18-47@2x.png)". A form's boxes take a pasted image or copied file exactly as the composer does: the same save, the same reference, the same pill, opened in the picture viewer on a click. The answers message carries the reference, so the agent can read the file. The numbers count across all the form's boxes, which reach the agent in one message (`insert_answer_attachments` in gx-chat-core). A published page's boxes do the same through gxserver (server/src/visual_pages/kit.rs).
//! SEE-ALSO: apps/desktop/src/app/native_chat/answer_attachments.rs (the async question's answer field, the same rules), apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx (the phone's form, which cannot take a pasted image).

use super::{
    appearance::ChatAppearance, composer_references, question_form::FormInput,
    state::NativeChatView,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, MouseDownEvent, ParentElement as _,
    Styled as _, Window, div, px, rgb,
};
use serde_json::{Value, json};
use std::time::Duration;

/// What the form box `key` sends as it changes (`question_form.rs` `sync_form_inputs`).
fn change_action(key: &str, text: &str) -> Option<Value> {
    let (form, slot) = key.rsplit_once(':')?;
    if slot == "notes" {
        return Some(json!({"type":"formNotes","form":form,"text":text}));
    }
    let question = slot.parse::<usize>().ok()?;
    Some(json!({"type":"formText","form":form,"question":question,"text":text}))
}

/// The byte offset of `units` UTF-16 code units into `text`.
pub(super) fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    text.char_indices()
        .find_map(|(byte, ch)| {
            if seen >= units {
                return Some(byte);
            }
            seen += ch.len_utf16();
            None
        })
        .unwrap_or(text.len())
}

impl NativeChatView {
    pub(super) fn form_paste_started(&mut self, key: &str, modal: bool, cx: &mut Context<Self>) {
        if let Some(entry) = self.inputs_mut(modal).get_mut(key) {
            entry.pending += 1;
            entry.paste_error = None;
        }
        cx.notify();
    }

    /// The text of the form's other boxes, which go to the agent in the same message, so the new
    /// reference's number goes on from theirs.
    fn form_others_text(&mut self, key: &str, modal: bool, cx: &Context<Self>) -> String {
        let Some((form, _)) = key.rsplit_once(':') else {
            return String::new();
        };
        let prefix = format!("{form}:");
        self.inputs_mut(modal)
            .iter()
            .filter(|(other, _)| other.as_str() != key && other.starts_with(&prefix))
            .map(|(_, entry)| entry.input.read(cx).value().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(super) fn finish_form_attachments(
        &mut self,
        key: String,
        modal: bool,
        (original, start, end): (String, usize, usize),
        paths: Vec<Value>,
        mut error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The reference goes into a copy of the box that is drawn now: the one the paste started
        // in, or the box's other copy (the transcript's, the Resources modal's) when that one is
        // gone. Its text is the draft the write is made from, with the `core` and `rev` that say
        // so; the text of a box no longer drawn is not, and with no copy drawn nothing is written.
        let drawn = [modal, !modal]
            .into_iter()
            .find_map(|modal| Some((modal, self.inputs_mut(modal).get(&key)?.input.clone())));
        if let Some((modal, input)) = drawn.filter(|_| !paths.is_empty()) {
            let current = input.read(cx).value().to_string();
            let others = self.form_others_text(&key, modal, cx);
            let result = self.runtime.as_ref().and_then(|runtime| {
                runtime.query(
                    "insertAnswerAttachments",
                    vec![
                        json!(current),
                        json!(paths),
                        json!(original),
                        json!(start),
                        json!(end),
                        json!(others),
                    ],
                    Duration::from_millis(500),
                )
            });
            match result.as_ref().and_then(|result| result["text"].as_str()) {
                Some(text) => {
                    let units = result.as_ref().unwrap()["caret"].as_u64().unwrap_or(0);
                    let caret = utf16_to_byte(text, units as usize);
                    input.update(cx, |input, cx| {
                        input.replace_all(text.to_owned(), window, cx);
                        input.set_selected_range(caret..caret, cx);
                    });
                    // Like the box's own edits (`sync_form_input_in`), the write says which draft
                    // its text was made from: the core can have typed a dictation into the box
                    // that this copy has not drawn yet.
                    let stamp = self
                        .inputs_mut(modal)
                        .get(&key)
                        .map(|entry| (entry.core.clone(), entry.rev));
                    if let Some(((core, rev), mut action)) = stamp.zip(change_action(&key, text)) {
                        action["core"] = json!(core);
                        action["rev"] = json!(rev);
                        action["caret"] = json!(units);
                        self.invoke(action, cx);
                    }
                }
                None => {
                    error = Some(
                        "The image reference could not be inserted. Please paste it again.".into(),
                    )
                }
            }
        }
        if let Some(entry) = self.inputs_mut(modal).get_mut(&key) {
            entry.pending = entry.pending.saturating_sub(1);
            entry.paste_error = error;
            entry.settled = true;
        }
        cx.notify();
    }

    /// Whether an image pasted into one of form `id`'s boxes is still being saved; the form's
    /// Send waits for it, so the answers never go without the picture.
    pub(super) fn form_images_pending(&self, id: &str) -> bool {
        let prefix = format!("{id}:");
        self.form_inputs
            .iter()
            .chain(self.modal_form_inputs.iter())
            .any(|(key, entry)| key.starts_with(&prefix) && entry.pending > 0)
    }

    /// Each box's pills, parsed by the core's own parser as the composer's are
    /// (`sync_answer_references` for the async answer field).
    pub(super) fn sync_form_references(&mut self, modal: bool, cx: &mut Context<Self>) {
        let p = ChatAppearance::current(&self.snapshot);
        for entry in self.inputs_mut(modal).values_mut() {
            let text = entry.input.read(cx).value().to_string();
            if entry.reference_draft.as_deref() == Some(text.as_str()) {
                continue;
            }
            let parsed = serde_json::to_value(
                ghostex_gx_chat_core::composer::queries::composer_references(&text),
            )
            .unwrap_or(Value::Null);
            entry.references = composer_references::parse(&text, &parsed);
            entry.reference_draft = Some(text);
            let replacements = composer_references::replacements(&entry.references, &p);
            entry.input.update(cx, |input, cx| {
                input.set_inline_replacements(replacements, cx)
            });
        }
    }

    /// A click on an image pill in a form box opens it in the picture viewer, with the box's other
    /// images beside it, as a click on the composer's pill does.
    fn click_form_reference(
        &mut self,
        key: &str,
        modal: bool,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.click_count != 1 {
            return;
        }
        let Some(entry) = self.inputs_mut(modal).get(key) else {
            return;
        };
        let Some(range) = entry.input.read(cx).inline_replacement_at(event.position) else {
            return;
        };
        let images: Vec<_> = entry
            .references
            .iter()
            .filter(|reference| reference.kind == "image")
            .collect();
        let Some(index) = images.iter().position(|reference| reference.range == range) else {
            return;
        };
        let images = images
            .iter()
            .map(|reference| json!({"transport":"read","path":reference.path,"label":reference.path,"alt":"Pasted image"}))
            .collect();
        self.open_image_viewer(images, index, cx);
    }

    /// A form's note or text box with what a paste adds: the paste hook, the pills' click, and a
    /// line while an image is saved or when it could not be. While a dictation that started in
    /// the box records or transcribes, the box is marked as where its words will be typed
    /// (`note_box`; the pill over the microphone keeps the clock and the ✕, dictation.rs).
    pub(super) fn form_box(
        &self,
        key: &str,
        entry: &FormInput,
        label: &'static str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let modal = self.form_in_modal.get();
        let paste = Self::form_paste_handler(
            cx.entity().downgrade(),
            key.to_string(),
            entry.input.clone(),
            modal,
        );
        let clicked = key.to_string();
        let status = if entry.pending > 0 {
            Some(("Saving the pasted image…".to_string(), false))
        } else {
            entry.paste_error.clone().map(|error| (error, true))
        };
        // The modal draws its own copy of the box, which no dictation starts in.
        let dictated_into = !modal && self.dictation_target(cx) == Some(key);
        div()
            .id(gpui::SharedString::from(format!("form-box:{key}")))
            .w_full()
            .min_w_0()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.click_form_reference(&clicked, modal, event, cx)
                }),
            )
            .child(super::question_form::note_box(
                &entry.input,
                label,
                paste,
                dictated_into,
                p,
            ))
            .when_some(status, |column, (status, error)| {
                column.child(
                    div()
                        .mt(px(4.0 * p.scale))
                        .text_size(px(12.5 * p.scale))
                        .text_color(if error {
                            rgb(if p.light { 0xb91c1c } else { 0xfb7185 }).into()
                        } else {
                            p.muted
                        })
                        .child(status),
                )
            })
            .into_any_element()
    }
}
