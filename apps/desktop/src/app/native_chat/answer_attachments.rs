use super::{appearance::ChatAppearance, composer_references, state::NativeChatView};
use gpui::{Context, Entity, MouseDownEvent, Window};
use gpui_component::input::TextareaState;
use serde_json::{Value, json};
use std::{ops::Range, time::Duration};

impl NativeChatView {
    /// CDXC:Clipboard 2026-09-23 DECISION:
    /// User: images pasted into a question answer should appear as [Image #1] and render exactly like the GPUI chat composer.
    pub(super) fn finish_answer_attachments(
        &mut self,
        key: String,
        input: Entity<TextareaState>,
        (original, start, end): (String, usize, usize),
        paths: Vec<Value>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.async_answer_echo.error = error;
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
            if let Some(result) = result {
                self.apply_answer_edit(&key, &input, result, window, cx);
            } else {
                self.async_answer_echo.error = Some(
                    "The image reference could not be inserted. Please paste it again.".into(),
                );
            }
        }
        self.async_answer_echo.pending = self.async_answer_echo.pending.saturating_sub(1);
        self.invoke(json!({"type":"asyncQuestionImagesPending","pending":self.async_answer_echo.pending > 0}), cx);
        cx.notify();
    }

    fn apply_answer_edit(
        &mut self,
        key: &str,
        input: &Entity<TextareaState>,
        result: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = result["text"].as_str() else {
            return;
        };
        let utf16 = result["caret"].as_u64().unwrap_or(0) as usize;
        let mut units = 0;
        let caret = text
            .char_indices()
            .find_map(|(byte, ch)| {
                if units >= utf16 {
                    return Some(byte);
                }
                units += ch.len_utf16();
                None
            })
            .unwrap_or(text.len());
        input.update(cx, |input, cx| {
            input.replace_all(text.to_owned(), window, cx);
            input.set_selected_range(caret..caret, cx);
        });
        // The input can have been unmounted by a collapsed/retired question while saving.
        // Persist to its captured key rather than whichever question is now on screen.
        if self
            .async_answer_input
            .as_ref()
            .is_some_and(|(active, _)| active == key)
        {
            self.async_answer_echo.sent.push(text.to_owned());
        }
        self.invoke(
            json!({"type":"asyncQuestionText","key":key,"text":text}),
            cx,
        );
        cx.notify();
    }

    pub(super) fn sync_answer_references(&mut self, p: &ChatAppearance, cx: &mut Context<Self>) {
        let Some((_, input)) = self.async_answer_input.clone() else {
            return;
        };
        let text = input.read(cx).value().to_string();
        if self.async_answer_echo.reference_draft.as_deref() != Some(&text) {
            let parsed = self.runtime.as_ref().and_then(|runtime| {
                runtime.query(
                    "composerReferences",
                    vec![json!(text), json!(true)],
                    Duration::from_millis(40),
                )
            });
            let Some(parsed) = parsed else {
                if self.runtime.is_some() && self.async_answer_echo.reference_retry.is_none() {
                    self.async_answer_echo.reference_retry =
                        Some(cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(30))
                                .await;
                            let _ = this.update(cx, |this, cx| {
                                this.async_answer_echo.reference_retry = None;
                                cx.notify();
                            });
                        }));
                }
                return;
            };
            self.async_answer_echo.references = composer_references::parse(&text, &parsed);
            self.async_answer_echo.reference_draft = Some(text);
            self.async_answer_echo.reference_retry = None;
            self.async_answer_echo.hovered_image = None;
            self.cancel_composer_reference_open();
        }
        let replacements = composer_references::replacements(&self.async_answer_echo.references, p);
        input.update(cx, |input, cx| {
            input.set_inline_replacements(replacements, cx)
        });
    }

    pub(super) fn remove_answer_attachment(
        &mut self,
        key: &str,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((active, input)) = self
            .async_answer_input
            .clone()
            .filter(|(active, _)| active == key)
        else {
            return;
        };
        let text = input.read(cx).value().to_string();
        if self.async_answer_echo.reference_draft.as_deref() != Some(&text) {
            return;
        }
        let start = text[..range.start].encode_utf16().count();
        let end = text[..range.end].encode_utf16().count();
        if let Some(result) = self.runtime.as_ref().and_then(|runtime| {
            runtime.query(
                "removeChatReference",
                vec![json!(text), json!(start), json!(end)],
                Duration::from_millis(500),
            )
        }) {
            self.apply_answer_edit(&active, &input, result, window, cx);
        }
    }

    pub(super) fn click_answer_reference(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_composer_reference_open();
        let Some((key, input)) = self.async_answer_input.clone() else {
            return;
        };
        let text = input.read(cx).value().to_string();
        if self.async_answer_echo.reference_draft.as_deref() != Some(&text) {
            return;
        }
        let Some(range) = input.read(cx).inline_replacement_at(event.position) else {
            return;
        };
        let Some(reference) = self
            .async_answer_echo
            .references
            .iter()
            .find(|reference| reference.range == range)
            .cloned()
        else {
            return;
        };
        if event.click_count >= 2 {
            if let Some((text, caret)) = composer_references::revealed(&text, &reference) {
                self.apply_answer_edit(
                    &key,
                    &input,
                    json!({"text":text,"caret":caret}),
                    window,
                    cx,
                );
            }
            return;
        }
        let click = self.composer_reference_click;
        self.composer_reference_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let _ = this.update(cx, |this, cx| {
                if this.composer_reference_click != click || !this.async_answer_input.as_ref().is_some_and(|(active, _)| active == &key) || !this.async_answer_echo.references.contains(&reference) { return; }
                this.composer_reference_task = None;
                if reference.kind == "image" {
                    let references: Vec<_> = this.async_answer_echo.references.iter().filter(|reference| reference.kind == "image").collect();
                    let index = references.iter().position(|other| other.path == reference.path).unwrap_or(0);
                    let images = references.iter().map(|reference| json!({"transport":"read","path":reference.path,"label":reference.path,"alt":"Pasted image"})).collect();
                    this.open_image_viewer(images, index, cx);
                } else {
                    this.invoke(json!({"type":"openComposerReference","href":reference.path}), cx);
                }
            });
        }));
    }

    pub(super) fn hover_answer_reference(
        &mut self,
        position: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let hovered = position
            .zip(self.async_answer_input.as_ref())
            .and_then(|(position, (_, input))| input.read(cx).inline_replacement_at(position))
            .and_then(|range| {
                self.async_answer_echo
                    .references
                    .iter()
                    .find(|reference| reference.range == range && reference.kind == "image")
            })
            .map(|reference| reference.path.clone());
        if self.async_answer_echo.hovered_image != hovered {
            self.async_answer_echo.hovered_image = hovered;
            cx.notify();
        }
    }
}
