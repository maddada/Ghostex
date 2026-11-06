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
    ///
    /// CDXC:Clipboard 2026-11-06 WHY: User: "pasting an image here in chat on macos takes like 3 seconds for image to paste now". The image work is not what took the time: measured in release on a 3456x2234 Retina screenshot, the pasteboard read, the TIFF-to-PNG conversion, base64, gxserver's save and read back, the digest and the thumbnail come to about 120 ms, and CleanShot's copy (a file URL beside the picture) skips the upload entirely (`importNativeAttachments`). The chip appears after four trips through the one chat host thread (`attachmentsStarted`, the import's answer, `insertAttachments`, `loadImage`), and that thread was settling a long coordinator chat's whole transcript again, 20 to 30 seconds each time, whenever the sidebar's session order moved; the paste queued behind it. The fix is in the chat core (`same_names` in packages/gx-chat-core/src/event.rs, `ProjectionScope::seen_pictures` and `RecipientDirectory`); the user's own log showed the import-to-picture chain at about 50 ms with that thread free.
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
                // The uploading tile shows on this frame, not after the chat host's turn.
                self.paste_tiles.started();
                cx.notify();
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
                PasteInto::Composer => {
                    // The core asks for the references (`attachmentReferences`) only when there
                    // are paths.
                    chat.paste_tiles.finished(!paths.is_empty());
                    cx.notify();
                    chat.invoke(
                        json!({"type":"attachmentsFinished","paths":paths,"error":error}),
                        cx,
                    )
                }
            });
        })
        .detach();
        true
    }
}

/// The uploading tiles of composer pastes, counted by the view itself.
///
/// CDXC:Clipboard 2026-11-06 WHY: the tile used to wait for the chat host to count the paste
/// (`attachmentsStarted` or `attachPaths` in the core's `pendingAttachments`), so a host busy with
/// another chat's transcript held the chip back for as long as that took (the 3 seconds the user
/// saw; CDXC:Clipboard on `paste_attachments`). The view counts each paste from the frame it
/// handles Cmd+V, until the chat box takes the paste's references or the paste ends without any,
/// and the tiles show the larger of its count and the host's. Only the composer's pastes: an
/// answer field or a form note already counts its own.
#[derive(Debug, Default)]
pub(super) struct PasteTiles {
    /// Pastes whose files are still being saved or imported.
    saving: u64,
    /// Saved pastes whose references the chat box has not received yet.
    inserting: u64,
}

impl PasteTiles {
    pub(super) fn started(&mut self) {
        self.saving += 1;
    }

    /// A paste's files are saved; with `references`, its tile stays until they reach the chat box.
    /// A drop's import (which this view did not count) changes nothing.
    pub(super) fn finished(&mut self, references: bool) {
        if self.saving == 0 {
            return;
        }
        self.saving -= 1;
        if references {
            self.inserting += 1;
        }
    }

    /// The chat box received text from the core: a waiting paste's references are in it.
    pub(super) fn inserted(&mut self) {
        self.inserting = self.inserting.saturating_sub(1);
    }

    /// How many uploading tiles to show, given what the host counts.
    pub(super) fn shown(&self, host_pending: u64) -> u64 {
        host_pending.max(self.saving + self.inserting)
    }
}

#[cfg(test)]
mod tests {
    use super::PasteTiles;

    #[test]
    fn a_paste_shows_its_tile_before_the_host_counts_it_until_its_references_arrive() {
        let mut tiles = PasteTiles::default();
        tiles.started();
        assert_eq!(tiles.shown(0), 1);
        tiles.finished(true);
        // Saved, the host has not answered yet: the tile stays.
        assert_eq!(tiles.shown(0), 1);
        tiles.inserted();
        assert_eq!(tiles.shown(0), 0);
        // The host's own count still shows when it is the larger.
        assert_eq!(tiles.shown(2), 2);
    }

    #[test]
    fn a_failed_paste_or_a_drop_leaves_no_tile_behind() {
        let mut tiles = PasteTiles::default();
        tiles.started();
        tiles.finished(false);
        assert_eq!(tiles.shown(0), 0);
        // A drop's import finishing, and text the chat box gets for other reasons, count nothing.
        tiles.finished(true);
        tiles.inserted();
        assert_eq!(tiles.shown(0), 0);
    }
}
