//! Pictures and files pasted or dropped into the chat box, and a send that waits for them.
//!
//! CDXC:Clipboard 2026-11-08 DECISION: User: "i just pasted an image after some text i wrote then hit enter while the image was still not added in the chat composer and ghostex sent just the chat. this is wrong we should wait for the image to come in or error if we couldn't attach it for any reason we shouldn't just send the text by itself!!!" Enter (and every other send from the chat box: Ctrl+Enter, queue, compact, the Send menu) while a paste or drop is still being saved holds the send: the text stays in the box, the uploading tile says the message goes once the picture is in, and the send leaves with text and references together the moment the last one lands. A paste that fails cancels the held send and its error shows; the text stays in the box. The Send button was already off while a tile spun; Enter went around it.
//!
//! CDXC:Clipboard 2026-11-08 WHY: User: "pasting an image like the one i pasted in this message is taking a while now". The pixel work was never the cost (a 1938x642 screenshot is 377 KB; prepare, base64 and gxserver's save take a few ms, all off the UI thread). The tile waited on the chat host thread four more times after the save: `attachmentsFinished`, the view's `attachmentReferences` answer, `insertAttachments`, then `loadImage`, which read the whole picture back from gxserver as base64 and decoded it again, each trip queued behind whatever that one thread was settling (a long transcript under load). The view now writes the references itself with the core's own rule (`insert_path_references` in packages/gx-chat-core/src/composer/references.rs), keeps the bytes it just saved as the tile's picture (`seed_chat_image`), and tells the core only that the upload ended (`attachmentsFinished` with no paths). Drops and the file picker go through the same path, so the held send counts them too.

use super::state::NativeChatView;
use base64::Engine as _;
use gpui::{Context, Window};
use serde_json::json;

/// What a background save or import hands back: the session machine's paths, the pictures the
/// view already holds for them, and why it stopped early.
/// How long a paste whose clipboard read has not answered still holds a send.
const PASTE_READ_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

struct Attached {
    paths: Vec<String>,
    pictures: Vec<(String, gpui::Image)>,
    error: Option<String>,
}

impl NativeChatView {
    /// Pastes and drops on their way into the chat box: the view's own count and the core's.
    pub(super) fn attachments_in_flight(&self) -> u64 {
        // A paste whose clipboard is still being read: the desktop's `Paste` action stamped it
        // (`note_paste_started`), the page's paste event in the web build (paste_watch.rs). A
        // read that never answers stops counting after a while.
        let reading = [
            self.paste_started,
            super::paste_watch::image_paste_on_its_way(),
        ]
        .into_iter()
        .flatten()
        .any(|since| since.elapsed() < PASTE_READ_LIMIT);
        self.paste_tiles
            .shown(self.snapshot["pendingAttachments"].as_u64().unwrap_or(0))
            + u64::from(reading)
    }

    /// Saves pasted pictures, or copies pasted, dropped or picked files, onto the session's
    /// machine and writes their references into the chat box. The uploading tile shows on this
    /// frame.
    pub(super) fn attach_to_composer(
        &mut self,
        images: Vec<gpui::Image>,
        external_paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if images.is_empty() && external_paths.is_empty() {
            return;
        }
        self.paste_tiles.started();
        cx.notify();
        // The core's own count (the collapse gate, the phone's spinner) follows in its own turn.
        self.invoke(json!({"type":"attachmentsStarted"}), cx);
        let config = self.config.clone();
        let started = web_time::Instant::now();
        let task = cx.background_executor().spawn(async move {
            if images.is_empty() {
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
            let mut attached = Attached {
                paths: Vec::new(),
                pictures: Vec::new(),
                error: None,
            };
            for (index, image) in images.into_iter().enumerate() {
                let prepared = match crate::app::helpers::clipboard_image::prepare_clipboard_image(
                    image.bytes(),
                    image.format(),
                ) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        attached.error = Some(error);
                        return attached;
                    }
                };
                let result = super::rpc::request(config.remote.clone(), "/api/saveSessionChatImage", &json!({
                    "projectId":config.project_id, "sessionId":config.session_id,
                    "base64Data":base64::engine::general_purpose::STANDARD.encode(&prepared.bytes),
                    "suggestedName":format!("clipboard-image-{}.{}", index+1, prepared.extension),
                })).await;
                match result {
                    Ok(value) if value["path"].is_string() => {
                        let path = value["path"].as_str().unwrap_or_default().to_owned();
                        let format = match prepared.extension {
                            "png" => gpui::ImageFormat::Png,
                            "jpg" => gpui::ImageFormat::Jpeg,
                            _ => image.format(),
                        };
                        attached
                            .pictures
                            .push((path.clone(), gpui::Image::from_bytes(format, prepared.bytes)));
                        attached.paths.push(path);
                    }
                    Ok(_) => {
                        attached.error =
                            Some("The session machine did not return an image path".to_owned());
                        return attached;
                    }
                    Err(error) => {
                        attached.error = Some(
                            error["message"]
                                .as_str()
                                .unwrap_or("The image could not be attached")
                                .to_owned(),
                        );
                        return attached;
                    }
                }
            }
            attached
        });
        cx.spawn(async move |chat, cx| {
            let attached = task.await;
            let _ = chat.update(cx, |chat, cx| {
                chat.finish_composer_attach(attached, started, cx)
            });
        })
        .detach();
    }

    fn finish_composer_attach(
        &mut self,
        attached: Attached,
        started: web_time::Instant,
        cx: &mut Context<Self>,
    ) {
        let Attached {
            paths,
            pictures,
            error,
        } = attached;
        self.paste_tiles.finished(false);
        for (path, picture) in pictures {
            self.seed_chat_image(&path, picture, cx);
        }
        if !paths.is_empty() {
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
                "heldSend": self.held_send.is_some(),
            }),
        );
        // A paste that brought nothing back leaves the held send in the box, with its error.
        if paths.is_empty() || error.is_some() {
            self.held_send = None;
        }
        // The core counts the upload down and shows the error; the references are already in.
        self.invoke(
            json!({"type":"attachmentsFinished","paths":[],"error":error}),
            cx,
        );
        cx.notify();
    }

    /// A send from the chat box while a picture is still on its way: kept until it lands.
    /// Returns whether the send was held.
    pub(super) fn hold_send_for_attachments(&mut self, mode: &str, cx: &mut Context<Self>) -> bool {
        if self.attachments_in_flight() == 0 {
            return false;
        }
        self.held_send = Some(mode.to_owned());
        cx.notify();
        true
    }

    /// Sends the held message once nothing is on its way any more (called from the paint, which
    /// runs after every reference and count change).
    pub(super) fn release_held_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        super::paste_watch::watch();
        if self.held_send.is_none() {
            return;
        }
        if self.attachments_in_flight() > 0 {
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
fn local_pictures(originals: &[String], imported: &[String]) -> Vec<(String, gpui::Image)> {
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
