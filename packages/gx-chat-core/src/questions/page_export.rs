//! The phone's Export on a page card or page sheet: the page's exported file (server/src/visual_pages/exported.rs)
//! read over the chat's own link (`exportVisualPage`), handed to the phone as the `pageExported` host action
//! (`{pageId, title, fileName, html}`), which shares it (iPhone) or saves it in a folder the person picks
//! (Android). The desktop downloads the file through the browser instead (apps/desktop/src/app/native_chat/page_card.rs).
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User picked "build both on the phone now" (Q2911): an Export button on the phone's page card and sheet, an Import answers picker for the .json, and paste-and-send import, JavaScript only so it ships over the air. The rest of the user's decision is on server/src/visual_pages/exported.rs.
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/pageExport.ts (the phone's side).

use serde_json::{json, Value};

use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

fn is_page_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// `exportPage {pageId}`: asks the page's computer for its exported file, unless one is already on its way.
pub fn export(state: &mut ChatState, page_id: &str) -> Vec<Effect> {
    let page_id = page_id.trim().to_ascii_lowercase();
    if !is_page_id(&page_id)
        || state
            .questions
            .page_exports
            .values()
            .any(|asked| *asked == page_id)
    {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .page_exports
        .insert(request_id, page_id.clone());
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ExportVisualPage,
        params: Box::new(json!({ "pageId": page_id })),
    }]
}

/// The export's answer, or `None` when `request_id` is not one: the file for the phone to share, or a toast that
/// says why there is none.
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let page_id = state.questions.page_exports.remove(&request_id)?;
    state.core.claim_refusal();
    let refused = |message: String| {
        Some(vec![Effect::Toast {
            level: "error".to_string(),
            message,
        }])
    };
    let result = match outcome {
        RpcOutcome::Ok { result } => result,
        RpcOutcome::Err { message, .. } => {
            return if message.contains("exportVisualPage") || message.contains("Unknown") {
                refused("Update Ghostex on the computer to export pages.".to_string())
            } else {
                refused(format!("The page could not be exported: {message}"))
            };
        }
    };
    let text = |key: &str| result.get(key).and_then(Value::as_str).unwrap_or_default();
    if text("html").is_empty() {
        return refused("Update Ghostex on the computer to export pages.".to_string());
    }
    Some(vec![Effect::HostAction {
        action: "pageExported".to_string(),
        params: Box::new(json!({
            "pageId": page_id,
            "title": text("title"),
            "fileName": text("fileName"),
            "html": text("html"),
        })),
    }])
}
