//! Attachment references (`[Image #1](path)`, `[File #2](path)`, …) whose file is on another
//! computer than the session's: when such text is pasted into the chat box, and again before a
//! send, the file is copied to the session's computer and the reference rewritten to the copy
//! (the rule and the user's words: packages/gx-chat-core/src/composer/attachment_moves.rs).
//!
//! Where the file is found:
//! - On this computer, for a chat on another computer: it is uploaded the way a file dropped into
//!   that chat is (`attachments::import_paths`).
//! - On a connected computer, for a chat on this computer or on a third one: a picture is read
//!   from that computer (`readSessionChatImage`) and saved on the session's (`saveSessionChatImage`).
//!   Other files have no read call on a connected computer, so they stay as they are.
//!
//! A path checked once is not checked again, so a send waits only on a copy, never on a lookup it
//! made before. The web build reaches one gxserver and has no files of its own: nothing to carry.

use super::state::NativeChatView;
use gpui::Context;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// What this chat box knows about its attachment references' files.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Default)]
pub(super) struct CarryState {
    /// Paths on the session's computer, or that no copy can be made of (looked at once).
    checked: HashSet<String>,
    /// Paths copied to the session's computer, and where the copy is.
    moved: HashMap<String, String>,
    /// Copies on their way.
    in_flight: usize,
}

impl CarryState {
    /// Paths a paste or drop just saved on the session's computer.
    pub(super) fn note_on_session_computer<'a>(
        &mut self,
        paths: impl IntoIterator<Item = &'a str>,
    ) {
        self.checked.extend(paths.into_iter().map(str::to_owned));
    }

    pub(super) fn in_flight(&self) -> bool {
        self.in_flight > 0
    }
}

/// How far a look goes: a paste may ask the session's computer whether it has a picture; a send
/// only looks at this computer's files, so a send with nothing to copy never waits.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, PartialEq)]
enum Depth {
    Paste,
    Send,
}

#[cfg(not(target_arch = "wasm32"))]
enum Outcome {
    /// The session's computer has it (with the picture when it was read to find out).
    There(Option<gpui::Image>),
    Moved {
        to: String,
        picture: Option<gpui::Image>,
    },
    /// Nowhere a copy can come from.
    Unavailable,
    Failed(String),
}

impl NativeChatView {
    /// Text was pasted into the chat box: its attachment references whose file is elsewhere are
    /// copied to the session's computer.
    pub(super) fn carry_pasted_attachments(&mut self, text: &str, cx: &mut Context<Self>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let paths = self.unchecked_paths(text);
            if !paths.is_empty() {
                self.start_carry(paths, Depth::Paste, cx);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (text, cx);
    }

    /// A send from the chat box: references already copied point at their copies, and a reference
    /// to a file this computer holds for a chat elsewhere (or one missing here for a chat here)
    /// holds the send until its copy lands. Returns whether the send was held.
    pub(super) fn hold_send_for_carry(&mut self, mode: &str, cx: &mut Context<Self>) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let moved: Vec<(String, String)> = self
                .carry
                .moved
                .iter()
                .map(|(from, to)| (from.clone(), to.clone()))
                .collect();
            self.retarget_draft(&moved, cx);
            let paths = self.unchecked_paths(&self.draft.clone());
            let remote_session = self.config.remote.is_some();
            let (copy, fine): (Vec<String>, Vec<String>) =
                paths.into_iter().partition(|path| {
                    let here = on_this_computer(path);
                    if remote_session {
                        here
                    } else {
                        !here
                            && ghostex_gx_chat_core::composer::reference_pills::ends_with_image_extension(path)
                    }
                });
            self.carry.checked.extend(fine);
            if !copy.is_empty() {
                self.start_carry(copy, Depth::Send, cx);
            }
            if self.carry.in_flight() {
                self.held_send = Some(mode.to_owned());
                cx.notify();
                return true;
            }
            false
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (mode, cx);
            false
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn unchecked_paths(&self, text: &str) -> Vec<String> {
        // A chat on a computer this app has no way to reach yet (a Cloud Box before it connects)
        // has nowhere to copy to.
        if self.config.remote.is_none() && self.config.machine_id != "local" {
            return Vec::new();
        }
        ghostex_gx_chat_core::composer::attachment_moves::attachment_reference_paths(text)
            .into_iter()
            .filter(|path| {
                !self.carry.checked.contains(path)
                    && !self.carry.moved.contains_key(path)
                    && !ghostex_gx_chat_core::composer::reference_pills::is_web_url(path)
            })
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_carry(&mut self, paths: Vec<String>, depth: Depth, cx: &mut Context<Self>) {
        // Marked now, so the same path is not looked at twice while its look runs.
        self.carry.checked.extend(paths.iter().cloned());
        self.carry.in_flight += 1;
        let config = self.config.clone();
        cx.spawn(async move |chat, cx| {
            // The other connected computers, the places a picture may be read from. Read here, not
            // in the caller, which may run inside the app's own update.
            let others = config
                .app
                .as_ref()
                .and_then(|app| {
                    app.read_with(cx, |app, _| {
                        app.remote_gxserver_connections
                            .iter()
                            .filter(|(machine_id, _)| **machine_id != config.machine_id)
                            .map(|(_, connection)| connection.request_target())
                            .collect::<Vec<_>>()
                    })
                    .ok()
                })
                .unwrap_or_default();
            let session = Session {
                remote: config.remote.clone(),
                project_id: config.project_id.clone(),
                session_id: config.session_id.clone(),
            };
            let outcomes = cx
                .background_executor()
                .spawn(async move {
                    let mut outcomes = Vec::new();
                    for path in paths {
                        let outcome = carry_one(&path, &session, &others, depth).await;
                        outcomes.push((path, outcome));
                    }
                    outcomes
                })
                .await;
            let _ = chat.update(cx, |chat, cx| chat.finish_carry(outcomes, cx));
        })
        .detach();
    }

    /// Points the draft's references to moved files at their copies, the caret staying after the
    /// same words.
    #[cfg(not(target_arch = "wasm32"))]
    fn retarget_draft(&mut self, moved: &[(String, String)], cx: &mut Context<Self>) {
        use ghostex_gx_chat_core::composer::attachment_moves::retarget_attachment_references;
        if moved.is_empty() {
            return;
        }
        let next = retarget_attachment_references(&self.draft, moved);
        if next == self.draft {
            return;
        }
        let caret = self
            .input
            .as_ref()
            .map(|input| input.read(cx).selected_range().end)
            .unwrap_or(self.draft.len())
            .min(self.draft.len());
        let caret = self.draft.get(..caret).map(|before| {
            retarget_attachment_references(before, moved)
                .encode_utf16()
                .count()
        });
        self.replace_draft(&next, false, false, cx);
        self.input_caret = caret;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_carry(&mut self, outcomes: Vec<(String, Outcome)>, cx: &mut Context<Self>) {
        self.carry.in_flight = self.carry.in_flight.saturating_sub(1);
        let mut moved = Vec::new();
        let mut failures = Vec::new();
        for (path, outcome) in outcomes {
            match outcome {
                Outcome::There(picture) => {
                    if let Some(picture) = picture {
                        self.seed_chat_image(&path, picture, cx);
                    }
                }
                Outcome::Moved { to, picture } => {
                    if let Some(picture) = picture {
                        self.seed_chat_image(&to, picture, cx);
                    }
                    self.carry.checked.insert(to.clone());
                    self.carry.moved.insert(path.clone(), to.clone());
                    moved.push((path, to));
                }
                Outcome::Unavailable => {}
                Outcome::Failed(error) => failures.push(error),
            }
        }
        crate::support_logs::append_for_scenario(
            crate::support_logs::GpuiSupportLog::SessionChat,
            "gpui.sessionChat.viewState",
            "sessionChat.attachmentsCarried",
            json!({"moved": moved.len(), "failed": failures.len()}),
        );
        self.retarget_draft(&moved, cx);
        if let Some(error) = failures.into_iter().next() {
            // The held send (if any) stays held: the reference still points where the session's
            // computer can't read it, and the box keeps the text with the reason.
            self.held_send = None;
            self.error = Some(error);
        }
        cx.notify();
    }
}

/// Whether `path` is a file or folder on this computer. A path written for another system (a
/// `C:\…` path on a Mac, a `/Users/…` path on Windows) is not absolute here.
#[cfg(not(target_arch = "wasm32"))]
fn on_this_computer(path: &str) -> bool {
    let path = std::path::Path::new(path);
    path.is_absolute() && path.exists()
}

/// Where the chat's session runs, for the background copy.
#[cfg(not(target_arch = "wasm32"))]
struct Session {
    remote: Option<crate::app::model::GpuiRemoteGxserverRequestTarget>,
    project_id: String,
    session_id: String,
}

#[cfg(not(target_arch = "wasm32"))]
async fn carry_one(
    path: &str,
    config: &Session,
    others: &[crate::app::model::GpuiRemoteGxserverRequestTarget],
    depth: Depth,
) -> Outcome {
    let here = on_this_computer(path);
    let picture_path =
        ghostex_gx_chat_core::composer::reference_pills::ends_with_image_extension(path);
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("image.png")
        .to_owned();
    let session = &config.remote;
    if session.is_none() {
        if here {
            return Outcome::There(None);
        }
    } else if here {
        let params =
            json!({"paths":[path],"projectId":config.project_id,"sessionId":config.session_id});
        return match super::attachments::import_paths(session, &params) {
            Ok(saved) => match saved[0].as_str() {
                Some(to) => Outcome::Moved {
                    to: to.to_owned(),
                    picture: super::composer_attach::local_pictures(
                        &[path.to_owned()],
                        &[to.to_owned()],
                    )
                    .pop()
                    .map(|(_, picture)| picture),
                },
                None => Outcome::Failed(format!("Couldn't copy {name} to this chat's computer.")),
            },
            Err(error) => Outcome::Failed(format!(
                "Couldn't copy {name} to this chat's computer: {error}"
            )),
        };
    } else if depth == Depth::Send || !picture_path {
        // Not on this computer: the session's own path, as far as a send looks.
        return Outcome::There(None);
    } else if let Ok(read) = super::rpc::request(
        session.clone(),
        "/api/readSessionChatImage",
        &json!({"path": path}),
    )
    .await
    {
        return Outcome::There(picture(&read));
    }
    if !picture_path {
        return Outcome::Unavailable;
    }
    for other in others {
        let Ok(read) = super::rpc::request(
            Some(other.clone()),
            "/api/readSessionChatImage",
            &json!({"path": path}),
        )
        .await
        else {
            continue;
        };
        let Some(base64) = read["base64Data"].as_str() else {
            continue;
        };
        return match super::rpc::request(
            session.clone(),
            "/api/saveSessionChatImage",
            &json!({"projectId":config.project_id,"sessionId":config.session_id,
                "base64Data":base64,"suggestedName":name}),
        )
        .await
        {
            Ok(saved) => match saved["path"].as_str() {
                Some(to) => Outcome::Moved {
                    to: to.to_owned(),
                    picture: picture(&read),
                },
                None => Outcome::Failed(format!("Couldn't copy {name} to this chat's computer.")),
            },
            Err(error) => Outcome::Failed(format!(
                "Couldn't copy {name} to this chat's computer: {}",
                error["message"].as_str().unwrap_or("the save failed")
            )),
        };
    }
    Outcome::Unavailable
}

/// The picture a `readSessionChatImage` answer carries, for the reference's thumbnail.
#[cfg(not(target_arch = "wasm32"))]
fn picture(read: &Value) -> Option<gpui::Image> {
    use base64::Engine as _;
    let format = match read["mediaType"].as_str()? {
        "image/png" => gpui::ImageFormat::Png,
        "image/jpeg" => gpui::ImageFormat::Jpeg,
        "image/gif" => gpui::ImageFormat::Gif,
        "image/webp" => gpui::ImageFormat::Webp,
        "image/bmp" => gpui::ImageFormat::Bmp,
        "image/tiff" => gpui::ImageFormat::Tiff,
        "image/svg+xml" => gpui::ImageFormat::Svg,
        _ => return None,
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(read["base64Data"].as_str()?)
        .ok()?;
    Some(gpui::Image::from_bytes(format, bytes))
}
