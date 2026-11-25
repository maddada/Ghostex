//! The picture on a published page's chat card (the "Window preview" card).
//!
//! The desktop app takes a PNG of a page when the user opens it in the floating page window and
//! stores it with the page in gxserver (`server/src/visual_pages/thumbnail.rs`). A card asks for it with the `loadPageThumbnail`
//! action; the answer reaches the renderer as a `pageThumbnail` host action carrying the picture
//! (or none yet) and, while the picture is missing or older than the page's answers, how long to
//! wait before asking again. The renderer keeps the bytes and asks again when told to, so every
//! client draws the same picture and waits the same way.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The user's decision is on server/src/visual_pages/thumbnail.rs. The cards are apps/desktop/src/app/native_chat/page_card.rs (desktop and the GPUI web build) and apps/mobile/app/src/chat/native/transcript/PageCard.tsx (phone); their measurements and the wait between asks are visual/page-card.json.

use std::sync::LazyLock;

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// `visual/page-card.json`, the part the core reads.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageCardVisual {
    /// The waits between asks while a picture is on its way; the last one repeats.
    retry_delays_ms: Vec<u64>,
}

static VISUAL: LazyLock<PageCardVisual> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../visual/page-card.json")).expect("shared page card")
});

fn is_page_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// `{type: "loadPageThumbnail", pageId, theme?, fresh?}`: reads the page's picture, unless a read
/// is already out for that `(pageId, theme)`. `fresh` (the card just opened its page, which
/// pictures it) starts the waits over.
pub fn load(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let page_id = action
        .param("pageId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let theme = action
        .param("theme")
        .and_then(Value::as_str)
        .filter(|theme| matches!(*theme, "dark" | "light"))
        .map(str::to_string);
    let key = (page_id.clone(), theme.clone());
    if action.param("fresh").and_then(Value::as_bool) == Some(true) {
        state
            .transcript_view
            .page_thumbnail_attempts
            .remove(&key);
    }
    if !is_page_id(&page_id)
        || state
            .transcript_view
            .page_thumbnail_requests
            .values()
            .any(|asked| *asked == key)
    {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .transcript_view
        .page_thumbnail_requests
        .insert(request_id, key);
    let params = match theme {
        Some(theme) => json!({ "pageId": page_id, "theme": theme }),
        None => json!({ "pageId": page_id }),
    };
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadVisualPageThumbnail,
        params: Box::new(params),
    }]
}

/// The answer to a `load`, as the `pageThumbnail` host action; `None` when `request_id` is not one.
///
/// `{pageId, png, capturedAt, settled, retryInMs, backgroundPicture, theme, transparent}`: `png`
/// is base64 or null; `settled` means this is the page's current picture (or the page is gone) and
/// nothing more will come, otherwise the renderer asks again after `retryInMs`. `backgroundPicture`
/// (gxserver's own answer) says the page has no picture and may be pictured without being opened,
/// which the desktop does for a card it draws (apps/desktop/src/app/visual_page_background_pictures.rs).
/// A refused read (an older gxserver without the call) is settled with no picture, and never
/// reaches the composer's error line.
pub fn settle_rpc(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let key = state
        .transcript_view
        .page_thumbnail_requests
        .remove(&request_id)?;
    let (ref page_id, ref requested_theme) = key;
    state.core.claim_refusal();
    let background_picture = matches!(
        outcome,
        RpcOutcome::Ok { result } if result.get("backgroundPicture").and_then(Value::as_bool) == Some(true)
    );
    let (png, captured_at, settled, resolved_theme, transparent) = match outcome {
        RpcOutcome::Ok { result } => {
            let png = result
                .get("png")
                .and_then(Value::as_str)
                .filter(|png| !png.is_empty())
                .map(str::to_string);
            let stale = result.get("stale").and_then(Value::as_bool) != Some(false);
            let gone = result.get("gone").and_then(Value::as_bool) == Some(true);
            let resolved_theme = result
                .get("theme")
                .and_then(Value::as_str)
                .filter(|theme| matches!(*theme, "dark" | "light"))
                .map(str::to_string);
            let transparent = result
                .get("transparent")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (
                png.clone(),
                result.get("capturedAt").cloned().unwrap_or(Value::Null),
                gone || (png.is_some() && !stale),
                resolved_theme,
                transparent,
            )
        }
        RpcOutcome::Err { .. } => (None, Value::Null, true, None, false),
    };
    let retry_in_ms = if settled {
        state
            .transcript_view
            .page_thumbnail_attempts
            .remove(&key);
        Value::Null
    } else {
        let attempts = state
            .transcript_view
            .page_thumbnail_attempts
            .entry(key.clone())
            .or_insert(0);
        let delays = &VISUAL.retry_delays_ms;
        let delay = delays
            .get(*attempts as usize)
            .or(delays.last())
            .copied()
            .unwrap_or(30_000);
        *attempts = attempts.saturating_add(1);
        json!(delay)
    };
    Some(vec![Effect::HostAction {
        action: "pageThumbnail".to_string(),
        params: Box::new(json!({
            "pageId": page_id,
            "png": png,
            "capturedAt": captured_at,
            "settled": settled,
            "retryInMs": retry_in_ms,
            "backgroundPicture": background_picture,
            "theme": resolved_theme.as_deref().or(requested_theme.as_deref()),
            "transparent": transparent,
        })),
    }])
}
