use crate::*;

const SESSION_CHAT_IMAGE_SAVE_CHUNK_MAX_CHARS: usize = 256 * 1024;
const SESSION_CHAT_IMAGE_SAVE_BASE64_MAX_CHARS: usize = 64 * 1024 * 1024;
const SESSION_CHAT_IMAGE_SAVE_REQUEST_ID_MAX_CHARS: usize = 128;
const SESSION_CHAT_IMAGE_SAVE_NAME_MAX_CHARS: usize = 255;

pub(crate) struct GpuiPendingSessionChatImageSave {
    suggested_name: String,
    base64_data: String,
    next_chunk_index: u32,
}

impl GhostexGpuiApp {
    /*
    CDXC:GPUISessionChatImageSave 2026-09-02:
    Chat images often exceed the app-modal bridge's one-message 1 MiB bound.
    Keep that security boundary intact and transfer the already-base64 image in
    ordered 256 KiB messages instead. The native side caps the assembled
    transfer, rejects missing/out-of-order chunks, and writes only after the
    explicit finish action arrives.
    */
    pub(crate) fn receive_session_chat_image_save_action(
        &mut self,
        session_id: TerminalSessionId,
        action: &str,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if action == "saveImageStart" {
            let request_id = message
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let suggested_name = message
                .get("suggestedName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("image.png");
            if request_id.is_empty()
                || request_id.chars().count() > SESSION_CHAT_IMAGE_SAVE_REQUEST_ID_MAX_CHARS
                || suggested_name.is_empty()
                || suggested_name.chars().count() > SESSION_CHAT_IMAGE_SAVE_NAME_MAX_CHARS
            {
                self.deliver_session_chat_image_save(
                    session_id,
                    request_id,
                    Some("The image save request was invalid."),
                    cx,
                );
                return true;
            }
            self.pending_session_chat_image_saves.insert(
                (session_id, request_id.to_string()),
                GpuiPendingSessionChatImageSave {
                    suggested_name: suggested_name.to_string(),
                    base64_data: String::new(),
                    next_chunk_index: 0,
                },
            );
            return true;
        }

        let request_id = message
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let key = (session_id, request_id.to_string());
        if action == "saveImageCancel" {
            self.pending_session_chat_image_saves.remove(&key);
            return true;
        }
        if action == "saveImageChunk" {
            let chunk = message
                .get("base64Chunk")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let chunk_index = message
                .get("chunkIndex")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok());
            let accepted = match self.pending_session_chat_image_saves.get_mut(&key) {
                Some(pending)
                    if chunk_index == Some(pending.next_chunk_index)
                        && chunk.len() <= SESSION_CHAT_IMAGE_SAVE_CHUNK_MAX_CHARS
                        && pending
                            .base64_data
                            .len()
                            .checked_add(chunk.len())
                            .is_some_and(|size| {
                                size <= SESSION_CHAT_IMAGE_SAVE_BASE64_MAX_CHARS
                            }) =>
                {
                    pending.base64_data.push_str(chunk);
                    pending.next_chunk_index += 1;
                    true
                }
                _ => false,
            };
            if !accepted {
                self.pending_session_chat_image_saves.remove(&key);
                self.deliver_session_chat_image_save(
                    session_id,
                    request_id,
                    Some("The image transfer was incomplete."),
                    cx,
                );
            }
            return true;
        }
        if action == "saveImageFinish" {
            let Some(pending) = self.pending_session_chat_image_saves.remove(&key) else {
                self.deliver_session_chat_image_save(
                    session_id,
                    request_id,
                    Some("The image transfer was incomplete."),
                    cx,
                );
                return true;
            };
            self.request_session_chat_image_save(
                session_id,
                request_id.to_string(),
                pending.suggested_name,
                pending.base64_data,
                cx,
            );
            return true;
        }
        false
    }
}
