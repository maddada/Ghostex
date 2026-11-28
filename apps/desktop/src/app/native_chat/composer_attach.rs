//! Pictures and files pasted or dropped into the chat box, and a send that waits for them.
//!
//! CDXC:Clipboard 2026-11-08 DECISION: User: "i just pasted an image after some text i wrote then hit enter while the image was still not added in the chat composer and ghostex sent just the chat. this is wrong we should wait for the image to come in or error if we couldn't attach it for any reason we shouldn't just send the text by itself!!!" Enter (and every other send from the chat box: Ctrl+Enter, queue, compact, the Send menu) while a paste or drop is still being saved holds the send: the text stays in the box, the uploading tile says the message goes once the picture is in, and the send leaves with text and references together the moment the last one lands. A paste that fails cancels the held send and its error shows; the text stays in the box. The Send button was already off while a tile spun; Enter went around it.
//!
//! CDXC:Clipboard 2026-11-09 DECISION: That hold became an optimistic send (option A of the paste-send page; the user's words are in packages/gx-chat-core/src/composer/attaching.rs). Enter while a paste or drop is still being saved sends at once: each paste is named to the chat core (`attachmentsStarted` with its id), the send takes every one still on its way (`PasteTiles::claim_all`, the core's `attaching::claim` by the same rule), the chat box empties and drops their tiles, and each lands in that message (`attachmentsFinished` with its paths) instead of the box. Only a paste whose clipboard is still being read when Enter is pressed, which has no tile and no id yet, still holds the send here, for those few milliseconds, with nothing drawn.
//!
//! CDXC:Clipboard 2026-11-08 WHY: User: "pasting an image like the one i pasted in this message is taking a while now". The pixel work was never the cost (a 1938x642 screenshot is 377 KB; prepare, base64 and gxserver's save take a few ms, all off the UI thread). The tile waited on the chat host thread four more times after the save: `attachmentsFinished`, the view's `attachmentReferences` answer, `insertAttachments`, then `loadImage`, which read the whole picture back from gxserver as base64 and decoded it again, each trip queued behind whatever that one thread was settling (a long transcript under load). The view now writes the references itself with the core's own rule (`insert_path_references` in packages/gx-chat-core/src/composer/references.rs), keeps the bytes it just saved as the tile's picture (`seed_chat_image`), and tells the core only that the upload ended (`attachmentsFinished` with no paths). Drops and the file picker go through the same path, so the held send counts them too.

use super::state::NativeChatView;
use base64::Engine as _;
use gpui::{Context, EntityInputHandler as _, Focusable as _, Window};
use serde_json::json;

/// What a background save or import hands back: the session machine's paths, the pictures the
/// view already holds for them, and why it stopped early.
/// How long a paste whose clipboard read has not answered still holds a send.
const PASTE_READ_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The wait before an upload's second try, so a link that was waking up has come back.
const UPLOAD_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(1500);

/// The words under the failed tiles: what happened, and that Send uploads them again.
pub(super) const UPLOAD_FAILED_WORDS: &str = "Couldn't upload. Press Send to try again.";

/// One thing pasted, dropped or picked into the chat box, uploaded on its own.
#[derive(Clone)]
pub(super) enum Upload {
    /// A pasted picture, saved onto the session's machine.
    Picture(gpui::Image),
    /// A dropped or picked file, copied onto the session's machine.
    File(String),
}

/// A picture or file whose upload failed twice: it stays in the chat box until Send uploads it
/// again or its remove button drops it.
pub(super) struct FailedUpload {
    /// The upload's id, which names its tile.
    pub(super) key: u64,
    pub(super) upload: Upload,
    /// Why the last try failed (the tile's tooltip).
    pub(super) error: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingTextHistory {
    Undo,
    Redo,
}

struct RemovedFailedUpload {
    index: usize,
    failed: FailedUpload,
    text_trail: Vec<String>,
}

struct RestoredFailedUpload {
    key: u64,
    text_trail: Vec<String>,
}

/// Undo and redo stack for failed-upload tiles dropped with their `x` button, interleaved in exact
/// edit order with the composer `TextareaState`'s text undo history.
#[derive(Default)]
pub(super) struct FailedUploadUndo {
    undo: Vec<RemovedFailedUpload>,
    redo: Vec<RestoredFailedUpload>,
    pending_text: Option<PendingTextHistory>,
    pending_generation: u64,
}

impl FailedUploadUndo {
    pub(super) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending_text = None;
    }

    fn is_empty(&self) -> bool {
        self.undo.is_empty() && self.redo.is_empty()
    }

    fn record_removed(&mut self, index: usize, failed: FailedUpload) {
        self.redo.clear();
        self.pending_text = None;
        self.undo.push(RemovedFailedUpload {
            index,
            failed,
            text_trail: Vec::new(),
        });
    }

    pub(super) fn note_text_edit(&mut self, after: &str) {
        self.redo.clear();
        self.pending_text = None;
        if let Some(top) = self.undo.last_mut() {
            top.text_trail.push(after.to_owned());
        }
    }

    pub(super) fn note_input_change(&mut self, before: &str, after: &str) {
        match self.pending_text.take() {
            Some(PendingTextHistory::Undo) => {
                if let Some(top) = self.undo.last_mut()
                    && !top.text_trail.is_empty()
                {
                    top.text_trail.pop();
                    while top.text_trail.last().is_some_and(|text| text != after) {
                        top.text_trail.pop();
                    }
                }
                if let Some(top) = self.redo.last_mut() {
                    top.text_trail.push(before.to_owned());
                }
            }
            Some(PendingTextHistory::Redo) => {
                if let Some(top) = self.redo.last_mut()
                    && !top.text_trail.is_empty()
                {
                    while top.text_trail.last().is_some_and(|text| text != after) {
                        top.text_trail.pop();
                    }
                    if top.text_trail.last().is_some_and(|text| text == after) {
                        top.text_trail.pop();
                    }
                }
                if let Some(top) = self.undo.last_mut() {
                    top.text_trail.push(after.to_owned());
                }
            }
            None => self.note_text_edit(after),
        }
    }

    fn pop_undo_if_top(&mut self) -> Option<RemovedFailedUpload> {
        if self
            .undo
            .last()
            .is_some_and(|top| top.text_trail.is_empty())
        {
            self.undo.pop()
        } else {
            None
        }
    }

    fn push_redo(&mut self, key: u64) {
        self.redo.push(RestoredFailedUpload {
            key,
            text_trail: Vec::new(),
        });
    }

    fn pop_redo_if_top(&mut self) -> Option<RestoredFailedUpload> {
        if self
            .redo
            .last()
            .is_some_and(|top| top.text_trail.is_empty())
        {
            self.redo.pop()
        } else {
            None
        }
    }

    fn push_undo_from_redo(&mut self, index: usize, failed: FailedUpload) {
        self.undo.push(RemovedFailedUpload {
            index,
            failed,
            text_trail: Vec::new(),
        });
    }

    fn mark_pending_text(&mut self, direction: PendingTextHistory) -> u64 {
        self.pending_generation = self.pending_generation.wrapping_add(1);
        self.pending_text = Some(direction);
        self.pending_generation
    }

    fn clear_pending_if(&mut self, generation: u64) {
        if self.pending_generation == generation {
            self.pending_text = None;
        }
    }
}

/// The error card's words when a message's picture failed to upload twice: Send uploads it again.
fn picture_failed_message(reason: &str) -> String {
    let reason = reason.trim().trim_end_matches(['.', ' ']);
    let why = if reason.is_empty() {
        String::new()
    } else {
        format!(" ({reason})")
    };
    format!(
        "The picture couldn't be uploaded{why}. It is still in the chat box: press Send to try again."
    )
}

/// Where an upload goes: the parts of the chat's config it needs, which cross threads.
#[derive(Clone)]
struct UploadTarget {
    remote: Option<crate::app::model::GpuiRemoteGxserverRequestTarget>,
    project_id: String,
    session_id: String,
}

/// One try at saving a picture or copying a file onto the session's machine.
async fn upload_once(config: UploadTarget, upload: &Upload) -> Attached {
    let image = match upload {
        Upload::File(path) => {
            let external_paths = vec![path.clone()];
            let params = json!({"paths":external_paths,"projectId":config.project_id,"sessionId":config.session_id});
            return match super::attachments::import_paths(&config.remote, &params) {
                Ok(paths) => {
                    let paths: Vec<String> = paths
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|path| path.as_str().map(str::to_owned))
                        .collect();
                    let pictures = local_pictures(&external_paths, &paths);
                    Attached {
                        paths,
                        pictures,
                        error: None,
                    }
                }
                Err(error) => Attached {
                    paths: Vec::new(),
                    pictures: Vec::new(),
                    error: Some(error),
                },
            };
        }
        Upload::Picture(image) => image,
    };
    let failed = |error: String| Attached {
        paths: Vec::new(),
        pictures: Vec::new(),
        error: Some(error),
    };
    let prepared = match crate::app::helpers::clipboard_image::prepare_clipboard_image(
        image.bytes(),
        image.format(),
    ) {
        Ok(prepared) => prepared,
        Err(error) => return failed(error),
    };
    let result = super::rpc::request(
        config.remote.clone(),
        "/api/saveSessionChatImage",
        &json!({
            "projectId":config.project_id, "sessionId":config.session_id,
            "base64Data":base64::engine::general_purpose::STANDARD.encode(&prepared.bytes),
            "suggestedName":format!("clipboard-image-1.{}", prepared.extension),
        }),
    )
    .await;
    match result {
        Ok(value) if value["path"].is_string() => {
            let path = value["path"].as_str().unwrap_or_default().to_owned();
            let format = match prepared.extension {
                "png" => gpui::ImageFormat::Png,
                "jpg" => gpui::ImageFormat::Jpeg,
                _ => image.format(),
            };
            Attached {
                paths: vec![path.clone()],
                pictures: vec![(path, gpui::Image::from_bytes(format, prepared.bytes))],
                error: None,
            }
        }
        Ok(_) => failed("The session machine did not return an image path".to_owned()),
        Err(error) => failed(
            error["message"]
                .as_str()
                .unwrap_or("The image could not be attached")
                .to_owned(),
        ),
    }
}

struct Attached {
    paths: Vec<String>,
    pictures: Vec<(String, gpui::Image)>,
    error: Option<String>,
}

impl NativeChatView {
    /// A paste whose clipboard is still being read: the desktop's `Paste` action stamped it
    /// (`note_paste_started`), the page's paste event in the web build (paste_watch.rs). A read
    /// that never answers stops counting after a while.
    fn paste_reading(&self) -> bool {
        [
            self.paste_started,
            super::paste_watch::image_paste_on_its_way(),
        ]
        .into_iter()
        .flatten()
        .any(|since| since.elapsed() < PASTE_READ_LIMIT)
    }

    /// Pictures or files on their way into the chat box, which a send from it takes along; a
    /// failed one counts too, since Send uploads it again (`retry_failed_uploads`).
    pub(super) fn attachments_on_their_way(&self) -> bool {
        self.paste_tiles.in_flight() > 0
            || !self.failed_uploads.is_empty()
            || self.snapshot["pendingAttachments"].as_u64().unwrap_or(0) > 0
    }

    /// The answers an exported page gave, as a `.json` file dropped, pasted or picked into the chat
    /// box: each goes to the chat core's import (packages/gx-chat-core/src/questions/page_import.rs)
    /// instead of being attached; the other paths are handed back. Files are read where the app
    /// runs, so a page answered on this computer imports into a chat on any computer.
    fn import_exported_answers(
        &mut self,
        paths: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Vec<String> {
        #[cfg(not(ghostex_web))]
        {
            let mut rest = Vec::with_capacity(paths.len());
            for path in paths {
                let text = std::path::Path::new(&path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                    .then(|| std::fs::metadata(&path).ok())
                    .flatten()
                    .filter(|metadata| metadata.is_file() && metadata.len() <= 256 * 1024)
                    .and_then(|_| std::fs::read_to_string(&path).ok())
                    .filter(|text| {
                        ghostex_gx_chat_core::questions::page_import::is_exported_answers(text)
                    });
                match text {
                    Some(text) => {
                        self.invoke(json!({"type": "importPageAnswers", "text": text}), cx)
                    }
                    None => rest.push(path),
                }
            }
            rest
        }
        #[cfg(ghostex_web)]
        {
            let _ = cx;
            paths
        }
    }

    /// Saves pasted pictures, or copies pasted, dropped or picked files, onto the session's
    /// machine and writes their references into the chat box, or into the message a send took
    /// them with. Each picture or file is its own upload with its own tile, so one that fails
    /// stays in the chat box on its own while the others land. The uploading tiles show on this
    /// frame.
    pub(super) fn attach_to_composer(
        &mut self,
        images: Vec<gpui::Image>,
        external_paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let external_paths = self.import_exported_answers(external_paths, cx);
        let uploads = images
            .into_iter()
            .map(Upload::Picture)
            .chain(external_paths.into_iter().map(Upload::File));
        for upload in uploads {
            self.start_upload(upload, cx);
        }
    }

    /// One picture's or file's upload, tried once more by itself after a short wait (a link that
    /// was waking up, a dropped transfer). One that fails twice stays in the chat box, marked
    /// failed (`failed_uploads`), and the next Send uploads it again.
    ///
    /// CDXC:Clipboard 2026-11-15 DECISION (issue #618, Q3274 A): a failed picture never leaves the chat box; Ghostex retries once by itself, then shows it failed ("Couldn't upload. Press Send to try again."); pressing Send again retries the upload (no "tap to retry"). A message a send took it with ends as a refused send does: its words back in the chat box with the error card saying why.
    /// SEE-ALSO: apps/mobile/app/src/chat/rust/host.ts (`attachOne`, `retryFailedPictures`).
    fn start_upload(&mut self, upload: Upload, cx: &mut Context<Self>) {
        let paste = self.paste_tiles.started();
        cx.notify();
        // The core's own count (the collapse gate, the phone's spinner) follows in its own turn,
        // and a send made before this lands takes it by this id.
        self.invoke(json!({"type":"attachmentsStarted","id":paste}), cx);
        let config = UploadTarget {
            remote: self.config.remote.clone(),
            project_id: self.config.project_id.clone(),
            session_id: self.config.session_id.clone(),
        };
        let started = web_time::Instant::now();
        cx.spawn(async move |chat, cx| {
            let attempt = |upload: Upload| {
                let config = config.clone();
                cx.background_executor().spawn(async move {
                    let attached = upload_once(config, &upload).await;
                    (attached, upload)
                })
            };
            let (mut attached, mut upload) = attempt(upload).await;
            if attached.error.is_some() {
                cx.background_executor().timer(UPLOAD_RETRY_DELAY).await;
                (attached, upload) = attempt(upload).await;
            }
            let _ = chat.update(cx, |chat, cx| {
                chat.finish_composer_attach(attached, upload, paste, started, cx)
            });
        })
        .detach();
    }

    /// Send: the pictures and files whose upload failed go up again, and the send takes them
    /// along as it takes any upload still on its way (Q3274 A).
    pub(super) fn retry_failed_uploads(&mut self, cx: &mut Context<Self>) {
        self.failed_upload_undo.clear();
        for failed in std::mem::take(&mut self.failed_uploads) {
            self.start_upload(failed.upload, cx);
        }
    }

    /// Break `TextareaState` typing coalescing so text edits before and after a failed-upload tile
    /// removal stay separate undo steps.
    fn break_composer_undo_coalescing(
        input: &gpui::Entity<gpui_component::input::TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        input.update(cx, |input, cx| {
            let selected = input.selected_range();
            let reversed = input.is_selection_reversed();
            let at = input
                .value()
                .get(..selected.start)
                .map(|prefix| prefix.encode_utf16().count())
                .unwrap_or(0);
            input.replace_text_in_range(Some(at..at), "", window, cx);
            input.set_selected_range_with_direction(selected, reversed, cx);
        });
    }

    /// A failed tile's remove button: it leaves the chat box, kept as one undo step so Cmd+Z /
    /// Ctrl+Z puts it back in the same place.
    pub(super) fn drop_failed_upload(
        &mut self,
        key: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .failed_uploads
            .iter()
            .position(|failed| failed.key == key)
        else {
            return;
        };
        let failed = self.failed_uploads.remove(index);
        self.failed_upload_undo.record_removed(index, failed);
        if let Some(input) = self.input.clone() {
            Self::break_composer_undo_coalescing(&input, window, cx);
            if !input.read(cx).focus_handle(cx).is_focused(window) {
                self.focus_requested = true;
            }
        }
        self.notify_composer(cx);
        cx.notify();
    }

    /// Cmd+Z / Ctrl+Z while the composer has focus: restores a removed failed-upload tile when
    /// that removal is the latest undoable action, otherwise lets the text field undo normally.
    pub(super) fn composer_undo(
        &mut self,
        _: &gpui_component::input::Undo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self
            .input
            .clone()
            .filter(|input| input.read(cx).focus_handle(cx).is_focused(window))
        else {
            cx.propagate();
            return;
        };
        if let Some(removed) = self.failed_upload_undo.pop_undo_if_top() {
            cx.stop_propagation();
            let key = removed.failed.key;
            let at = removed.index.min(self.failed_uploads.len());
            self.failed_uploads.insert(at, removed.failed);
            self.failed_upload_undo.push_redo(key);
            Self::break_composer_undo_coalescing(&input, window, cx);
            self.expand_composer(true, cx);
            self.notify_composer(cx);
            cx.notify();
            return;
        }
        if !self.failed_upload_undo.is_empty() {
            let generation = self
                .failed_upload_undo
                .mark_pending_text(PendingTextHistory::Undo);
            let chat = cx.weak_entity();
            window.defer(cx, move |_, cx| {
                let _ = chat.update(cx, |chat, _| {
                    chat.failed_upload_undo.clear_pending_if(generation);
                });
            });
        }
        cx.propagate();
    }

    /// Cmd+Shift+Z / Ctrl+Shift+Z / Ctrl+Y while the composer has focus: re-removes a restored
    /// failed-upload tile when that step is next on the redo timeline, otherwise forwards to the
    /// text field.
    pub(super) fn composer_redo(
        &mut self,
        _: &gpui_component::input::Redo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self
            .input
            .clone()
            .filter(|input| input.read(cx).focus_handle(cx).is_focused(window))
        else {
            cx.propagate();
            return;
        };
        while let Some(restored) = self.failed_upload_undo.pop_redo_if_top() {
            if let Some(index) = self
                .failed_uploads
                .iter()
                .position(|failed| failed.key == restored.key)
            {
                cx.stop_propagation();
                let failed = self.failed_uploads.remove(index);
                self.failed_upload_undo.push_undo_from_redo(index, failed);
                Self::break_composer_undo_coalescing(&input, window, cx);
                self.notify_composer(cx);
                cx.notify();
                return;
            }
        }
        if !self.failed_upload_undo.is_empty() {
            let generation = self
                .failed_upload_undo
                .mark_pending_text(PendingTextHistory::Redo);
            let chat = cx.weak_entity();
            window.defer(cx, move |_, cx| {
                let _ = chat.update(cx, |chat, _| {
                    chat.failed_upload_undo.clear_pending_if(generation);
                });
            });
        }
        cx.propagate();
    }

    fn finish_composer_attach(
        &mut self,
        attached: Attached,
        upload: Upload,
        paste: u64,
        started: web_time::Instant,
        cx: &mut Context<Self>,
    ) {
        let Attached {
            paths,
            pictures,
            error,
        } = attached;
        // A send took this paste while it was on its way: its references go to that message.
        let sent = self.paste_tiles.finished(Some(paste), false);
        self.carry
            .note_on_session_computer(paths.iter().map(String::as_str));
        for (path, picture) in pictures {
            self.seed_chat_image(&path, picture, cx);
        }
        if !paths.is_empty() && !sent {
            let selection = self
                .input
                .as_ref()
                .map(|input| input.read(cx).selected_range())
                .unwrap_or(self.draft.len()..self.draft.len());
            let clamp = |offset: usize| offset.min(self.draft.len());
            let start = self.draft[..clamp(selection.start)].encode_utf16().count();
            let end = self.draft[..clamp(selection.end)].encode_utf16().count();
            let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
            let edit = ghostex_gx_chat_core::composer::references::insert_path_references(
                &self.draft,
                &paths,
                start,
                end,
            );
            self.replace_draft(&edit.text, false, false, cx);
            self.input_caret = Some(edit.caret);
        }
        crate::support_logs::append_for_scenario(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "gpui.sessionChat.viewState",
            "sessionChat.pasteAttached",
            json!({
                "elapsedMs": started.elapsed().as_millis() as u64,
                "references": paths.len(),
                "failed": error.is_some(),
                "sent": sent,
            }),
        );
        // Failed twice: it stays in the chat box, marked failed, and its own words say so; a
        // message that waited for it comes back to the box with the error card (attaching.rs).
        let error = error.map(|reason| {
            self.failed_uploads.push(FailedUpload {
                key: paste,
                upload,
                error: reason.clone(),
            });
            picture_failed_message(&reason)
        });
        let error = if sent { error } else { None };
        // The core counts the upload down. The chat box's references are already in; a sent
        // message's go with this answer.
        let paths = if sent { paths } else { Vec::new() };
        self.invoke(
            json!({"type":"attachmentsFinished","id":paste,"paths":paths,"error":error}),
            cx,
        );
        cx.notify();
    }

    /// A send from the chat box while a paste's clipboard is still being read: kept, with nothing
    /// drawn, until the read hands over the picture (whose tile a send then takes along) or brings
    /// none. Returns whether the send was held.
    pub(super) fn hold_send_for_attachments(&mut self, mode: &str, cx: &mut Context<Self>) -> bool {
        if !self.paste_reading() {
            return false;
        }
        self.held_send = Some(mode.to_owned());
        cx.notify();
        true
    }

    /// Sends the held message once the clipboard read is over (called from the paint, which runs
    /// after every reference and count change).
    pub(super) fn release_held_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        super::paste_watch::watch();
        if self.held_send.is_none() {
            return;
        }
        // A reference's file still being copied to the session's computer
        // (carry_attachments.rs); its landing repaints.
        if self.carry.in_flight() {
            return;
        }
        if self.paste_reading() {
            // A clipboard read ends without telling the view when it brings nothing (or times
            // out), so a held send looks again shortly.
            if self.held_send_recheck.is_none() {
                self.held_send_recheck = Some(cx.spawn(async move |chat, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(200))
                        .await;
                    let _ = chat.update(cx, |chat, cx| {
                        chat.held_send_recheck = None;
                        cx.notify();
                    });
                }));
            }
            return;
        }
        let Some(mode) = self.held_send.take() else {
            return;
        };
        // Not inside the paint: `submit` clears the field and invokes the core.
        cx.defer_in(window, move |chat, window, cx| {
            chat.submit(&mode, window, cx)
        });
    }
}

/// The local pictures behind imported paths, read so their tiles need no read back. Only when the
/// import answered one path per file, in order, and only for pictures (a missing or unreadable
/// file, as in the web build, is just skipped).
pub(super) fn local_pictures(
    originals: &[String],
    imported: &[String],
) -> Vec<(String, gpui::Image)> {
    if originals.len() != imported.len() {
        return Vec::new();
    }
    originals
        .iter()
        .zip(imported)
        .filter_map(|(original, imported)| {
            let format = image::ImageFormat::from_path(original).ok()?;
            let format = match format {
                image::ImageFormat::Png => gpui::ImageFormat::Png,
                image::ImageFormat::Jpeg => gpui::ImageFormat::Jpeg,
                image::ImageFormat::Gif => gpui::ImageFormat::Gif,
                image::ImageFormat::WebP => gpui::ImageFormat::Webp,
                _ => return None,
            };
            let size = std::fs::metadata(original).ok()?.len();
            if size == 0
                || size as usize > crate::app::helpers::clipboard_image::CHAT_IMAGE_MAX_BYTES
            {
                return None;
            }
            let bytes = std::fs::read(original).ok()?;
            Some((imported.clone(), gpui::Image::from_bytes(format, bytes)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{UPLOAD_FAILED_WORDS, picture_failed_message};

    #[test]
    fn a_failed_picture_says_send_tries_again_never_tap_to_retry() {
        assert_eq!(
            UPLOAD_FAILED_WORDS,
            "Couldn't upload. Press Send to try again."
        );
        assert_eq!(
            picture_failed_message("Connection reset."),
            "The picture couldn't be uploaded (Connection reset). It is still in the chat box: press Send to try again."
        );
        assert!(!picture_failed_message("").contains("()"));
        assert!(!picture_failed_message("x").to_lowercase().contains("tap"));
    }
}
