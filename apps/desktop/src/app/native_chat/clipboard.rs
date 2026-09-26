use super::state::NativeChatView;
use base64::Engine as _;
use gpui::{App, ClipboardEntry, ClipboardItem, Context, Entity, WeakEntity, Window};
use gpui_component::input::TextareaState;
use serde_json::json;

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
        move |clipboard, window, cx| {
            chat.update(cx, |chat, cx| {
                chat.paste_attachments(clipboard, answer.clone(), window, cx)
            })
            .unwrap_or(false)
        }
    }

    /// Returns whether the paste was taken; `false` lets the field insert the text.
    fn paste_attachments(
        &mut self,
        clipboard: &ClipboardItem,
        answer: Option<(String, Entity<TextareaState>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
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
        if answer.is_some()
            && (self.snapshot["asyncQuestions"]["submitting"] == true
                || self.snapshot["asyncQuestions"]["loading"] == true)
        {
            return true;
        }
        let selection = answer.as_ref().map(|(_, input)| {
            let state = input.read(cx);
            let text = state.value().to_string();
            let selected = state.selected_range();
            let start = text[..selected.start].encode_utf16().count();
            let end = text[..selected.end].encode_utf16().count();
            (text, start, end)
        });
        if answer.is_some() {
            self.async_answer_echo.pending += 1;
            self.async_answer_echo.error = None;
            self.invoke(
                json!({"type":"asyncQuestionImagesPending","pending":true}),
                cx,
            );
            cx.notify();
        } else {
            if images.is_empty() {
                self.invoke(json!({"type":"attachPaths","paths":external_paths}), cx);
                return true;
            }
            self.invoke(json!({"type":"attachmentsStarted"}), cx);
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
                let result = super::rpc::request(config.remote.clone(), "/api/saveSessionChatImage", &json!({
                    "projectId":config.project_id, "sessionId":config.session_id,
                    "base64Data":base64::engine::general_purpose::STANDARD.encode(image.bytes()),
                    "suggestedName":format!("clipboard-image-{}.{}", index+1, image.format().extension()),
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
            let _ = chat.update_in(cx, |chat, window, cx| {
                if let Some((key, input)) = answer {
                    chat.finish_answer_attachments(
                        key,
                        input,
                        selection.unwrap(),
                        paths,
                        error,
                        window,
                        cx,
                    );
                    return;
                }
                chat.invoke(
                    json!({"type":"attachmentsFinished","paths":paths,"error":error}),
                    cx,
                )
            });
        })
        .detach();
        true
    }
}
