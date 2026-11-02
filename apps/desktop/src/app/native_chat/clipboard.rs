use super::state::NativeChatView;
use base64::Engine as _;
use gpui::{App, ClipboardEntry, ClipboardItem, Context, Entity, WeakEntity, Window};
use gpui_component::input::TextareaState;
use serde_json::json;

/// The field a paste lands in.
#[derive(Clone)]
enum PasteInto {
    Composer,
    /// The async question's answer field: its key and state.
    Answer(String, Entity<TextareaState>),
    /// A question form's note or text box (`question_form.rs`), in the transcript or, with
    /// `modal`, in the Resources modal.
    Form {
        key: String,
        input: Entity<TextareaState>,
        modal: bool,
    },
}

impl PasteInto {
    fn field(&self) -> Option<&Entity<TextareaState>> {
        match self {
            Self::Composer => None,
            Self::Answer(_, input) | Self::Form { input, .. } => Some(input),
        }
    }
}

impl NativeChatView {
    /// The `on_paste` hook of the composer (`answer: None`) and of the async question answer
    /// field (its key and state): pasted images and copied files become attachments, anything
    /// else falls through to the field as text.
    ///
    /// CDXC:Clipboard 2026-09-26 WHY: the paste is taken in the field's own `on_paste` hook, not by capturing the `Paste` action on the chat root, because in the browser a paste reaches the focused field as a DOM event carrying the clipboard item (GPUI's `InputHandler::paste`) and never dispatches the action. GPUI Kit offers that item to the same hook, deferred until after the field's own update, so the hook serves the desktop Cmd+V and the web paste alike and must not rely on running inside action dispatch.
    pub(super) fn paste_handler(
        chat: WeakEntity<Self>,
        answer: Option<(String, Entity<TextareaState>)>,
    ) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
        let target = answer.map_or(PasteInto::Composer, |(key, input)| {
            PasteInto::Answer(key, input)
        });
        move |clipboard, window, cx| {
            chat.update(cx, |chat, cx| {
                chat.paste_attachments(clipboard, target.clone(), window, cx)
            })
            .unwrap_or(false)
        }
    }

    /// The `on_paste` hook of a question form's note or text box: a pasted image or copied file
    /// is saved as the composer saves it and its reference written into the box
    /// (`finish_form_attachments`).
    pub(super) fn form_paste_handler(
        chat: WeakEntity<Self>,
        key: String,
        input: Entity<TextareaState>,
        modal: bool,
    ) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
        let target = PasteInto::Form { key, input, modal };
        move |clipboard, window, cx| {
            chat.update(cx, |chat, cx| {
                chat.paste_attachments(clipboard, target.clone(), window, cx)
            })
            .unwrap_or(false)
        }
    }

    /// Stamps when a `Paste` reached a chat field. The field reads the clipboard off the UI thread
    /// and hands the result to `paste_handler`, which logs a slow read
    /// (CDXC:Clipboard in helpers/clipboard_read.rs).
    pub(super) fn note_paste_started(
        &mut self,
        _: &gpui_component::input::Paste,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.paste_started = Some(web_time::Instant::now());
    }

    /// Returns whether the paste was taken; `false` lets the field insert the text.
    fn paste_attachments(
        &mut self,
        clipboard: &ClipboardItem,
        target: PasteInto,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(started) = self.paste_started.take() {
            crate::app::helpers::gpui_note_clipboard_read("chatPaste", started, Some(clipboard));
        }
        if !self.composer_ready {
            return false;
        }
        let images = clipboard
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let external_paths = clipboard
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::ExternalPaths(paths) => Some(&paths.0),
                _ => None,
            })
            .flatten()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        if images.is_empty() && external_paths.is_empty() {
            return false;
        }
        if matches!(target, PasteInto::Answer(..))
            && (self.snapshot["asyncQuestions"]["submitting"] == true
                || self.snapshot["asyncQuestions"]["loading"] == true)
        {
            return true;
        }
        let selection = target.field().map(|input| {
            let state = input.read(cx);
            let text = state.value().to_string();
            let selected = state.selected_range();
            let start = text[..selected.start].encode_utf16().count();
            let end = text[..selected.end].encode_utf16().count();
            (text, start, end)
        });
        match &target {
            PasteInto::Answer(..) => {
                self.async_answer_echo.pending += 1;
                self.async_answer_echo.error = None;
                self.invoke(
                    json!({"type":"asyncQuestionImagesPending","pending":true}),
                    cx,
                );
                cx.notify();
            }
            PasteInto::Form { key, modal, .. } => self.form_paste_started(key, *modal, cx),
            PasteInto::Composer => {
                if images.is_empty() {
                    self.invoke(json!({"type":"attachPaths","paths":external_paths}), cx);
                    return true;
                }
                self.invoke(json!({"type":"attachmentsStarted"}), cx);
            }
        }
        let config = self.config.clone();
        let task = cx.background_executor().spawn(async move {
            if images.is_empty() {
                return match super::attachments::import_paths(&config.remote, &json!({"paths":external_paths,"projectId":config.project_id,"sessionId":config.session_id})) {
                    Ok(paths) => (paths.as_array().cloned().unwrap_or_default(), None),
                    Err(error) => (Vec::new(), Some(error)),
                };
            }
            let mut paths = Vec::new();
            for (index, image) in images.into_iter().enumerate() {
                let prepared = match crate::app::helpers::clipboard_image::prepare_clipboard_image(
                    image.bytes(), image.format(),
                ) {
                    Ok(prepared) => prepared,
                    Err(error) => return (paths, Some(error)),
                };
                let result = super::rpc::request(config.remote.clone(), "/api/saveSessionChatImage", &json!({
                    "projectId":config.project_id, "sessionId":config.session_id,
                    "base64Data":base64::engine::general_purpose::STANDARD.encode(prepared.bytes),
                    "suggestedName":format!("clipboard-image-{}.{}", index+1, prepared.extension),
                })).await;
                match result {
                    Ok(value) if value["path"].is_string() => paths.push(value["path"].clone()),
                    Ok(_) => return (paths, Some("The session machine did not return an image path".to_owned())),
                    Err(error) => return (paths, Some(error["message"].as_str().unwrap_or("The image could not be attached").to_owned())),
                }
            }
            (paths, None)
        });
        cx.spawn_in(window, async move |chat, cx| {
            let (paths, error) = task.await;
            let _ = chat.update_in(cx, |chat, window, cx| match target {
                PasteInto::Answer(key, input) => chat.finish_answer_attachments(
                    key,
                    input,
                    selection.unwrap(),
                    paths,
                    error,
                    window,
                    cx,
                ),
                PasteInto::Form { key, input, modal } => chat.finish_form_attachments(
                    key,
                    modal,
                    input,
                    selection.unwrap(),
                    paths,
                    error,
                    window,
                    cx,
                ),
                PasteInto::Composer => chat.invoke(
                    json!({"type":"attachmentsFinished","paths":paths,"error":error}),
                    cx,
                ),
            });
        })
        .detach();
        true
    }
}
