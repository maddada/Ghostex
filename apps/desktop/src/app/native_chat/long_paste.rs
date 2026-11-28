//! A text paste too long for the chat box goes up as a file, with the chat's Undo toast offering
//! Keep inline (the rule, its limit and the user's decision: CDXC:Clipboard 2026-11-28 in
//! packages/gx-chat-core/src/composer/long_paste.rs). The upload is the chat box's own
//! (composer_attach.rs `Upload::Text`), so the paste gets the same uploading tile, the same retry,
//! and goes with a send made while it is still on its way.

use super::state::{NativeChatEvent, NativeChatView};
use base64::Engine as _;
use ghostex_gx_chat_core::composer::long_paste::{
    KEEP_INLINE_LABEL, LONG_PASTE_TOAST_DESCRIPTION, LONG_PASTE_TOAST_TITLE, PASTED_TEXT_FILE_NAME,
    insert_long_paste, is_long_paste, keep_inline,
};
use gpui::Context;
use serde_json::{Value, json};

/// The chat command the toast's Keep inline runs (`NativeChatView::invoke` answers it here, never
/// the chat core).
pub(super) const KEEP_PASTE_INLINE: &str = "keepPasteInline";

/// How long the toast stays: long enough to read it and decide.
const TOAST_MS: u64 = 10_000;

/// Saves pasted text onto the session's machine as a text file; answers its path there.
pub(super) async fn save_pasted_text(
    remote: Option<crate::app::model::GpuiRemoteGxserverRequestTarget>,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<String, String> {
    let result = super::rpc::request(
        remote,
        "/api/saveSessionChatAttachment",
        &json!({
            "projectId": project_id,
            "sessionId": session_id,
            "suggestedName": PASTED_TEXT_FILE_NAME,
            "base64Data": base64::engine::general_purpose::STANDARD.encode(text.as_bytes()),
        }),
    )
    .await;
    match result {
        Ok(value) => value["path"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "The session machine did not return a file path".to_owned()),
        Err(error) => Err(error["message"]
            .as_str()
            .unwrap_or("The pasted text could not be saved")
            .to_owned()),
    }
}

impl NativeChatView {
    /// A text paste into the chat box past the limit: uploaded as a file instead of going into the
    /// field. Answers whether it was taken.
    pub(super) fn take_long_paste(&mut self, text: Option<String>, cx: &mut Context<Self>) -> bool {
        let Some(text) = text.filter(|text| is_long_paste(text)) else {
            return false;
        };
        self.start_upload(super::composer_attach::Upload::Text(text), cx);
        true
    }

    /// The pasted text is saved at `path`: its reference goes where the selection is, and the
    /// toast offers Keep inline.
    pub(super) fn insert_long_paste(
        &mut self,
        path: &str,
        text: String,
        (start, end): (usize, usize),
        cx: &mut Context<Self>,
    ) {
        let (edit, paste) = insert_long_paste(&self.draft, path, text, start, end);
        self.replace_draft(&edit.text, false, false, cx);
        self.input_caret = Some(edit.caret);
        self.long_paste = Some(paste);
        cx.emit(NativeChatEvent::Host(json!({
            "type": "toast",
            // One toast: a newer long paste's takes the older's place, as `long_paste` does.
            "toastId": "long-paste",
            "level": "success",
            "title": LONG_PASTE_TOAST_TITLE,
            "description": LONG_PASTE_TOAST_DESCRIPTION,
            "durationMs": TOAST_MS,
            "chatUndo": {"type": KEEP_PASTE_INLINE, "path": path},
            "chatUndoLabel": KEEP_INLINE_LABEL,
        })));
    }

    /// The toast's Keep inline: the file's reference becomes the pasted text. Nothing happens once
    /// the reference has left the chat box (removed, or sent).
    pub(super) fn keep_paste_inline(&mut self, action: &Value, cx: &mut Context<Self>) {
        let path = action["path"].as_str().unwrap_or_default();
        let Some(paste) = self.long_paste.take_if(|paste| paste.path == path) else {
            return;
        };
        let Some(edit) = keep_inline(&self.draft, &paste) else {
            return;
        };
        self.replace_draft(&edit.text, false, false, cx);
        self.input_caret = Some(edit.caret);
        // References in it to files on another computer are copied to the session's, as for any
        // text pasted (carry_attachments.rs).
        self.carry_pasted_attachments(&paste.text, cx);
    }
}
