//! Frozen Live page rendering for public sharing through an isolated ephemeral share-proxy
//! (`server/src/sharing/proxy.rs`).
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! Shared Live pages are served live from this machine over the share-proxy with `window.__gxShared`
//! set so outside visitors enter their name once and submit answers strictly to the share-proxy's
//! narrow `/__gx_share/answers` endpoint. The user's own drafts and answers are never leaked into
//! the shared HTML, and `connect-src 'self' https:` prevents the page from reaching gxserver.

use std::path::Path;

use serde_json::{json, Value};

use super::{
    questions::page_questions,
    serve::{in_head, with_page_state},
    store::{read_page, read_record},
};

/// Content policy for a publicly shared Live page served from its isolated ephemeral share-proxy:
/// allows same-origin fetch ONLY to the share-proxy itself (`/__gx_share/answers`), plus external
/// `https:` assets the page itself loads; never allows frames, arbitrary forms, or loopback gxserver.
pub(crate) const SHARED_PAGE_CONTENT_SECURITY_POLICY: &str =
    "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' https:; style-src 'unsafe-inline' https:; img-src data: blob: https:; font-src data: https:; connect-src 'self' https:; media-src data: blob: https:; frame-src 'none'; form-action 'self'; base-uri 'none'";

/// A frozen, share-ready snapshot of a single Live page.
pub(crate) struct SharedPage {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) html: String,
    pub(crate) answerable: bool,
}

/// Builds the frozen HTML for sharing Live page `id`. Starts blank (never exposing the owner's
/// private drafts or answers) and embeds `window.__gxShared` so `PAGE_API` switches into shared
/// visitor answering mode.
pub(crate) fn shared_page(gxserver_state_dir: &Path, id: &str) -> Option<SharedPage> {
    let record = read_record(gxserver_state_dir, id)?;
    let page = read_page(gxserver_state_dir, id)?;
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Live page")
        .to_string();
    let has_questions =
        page_questions(gxserver_state_dir, id).is_some_and(|(_, questions)| !questions.is_empty());
    let has_session = record
        .get("sessionRef")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    let answerable = has_questions && has_session;
    let html = with_page_state(
        page,
        Some(json!({})),
        false,
        SHARED_PAGE_CONTENT_SECURITY_POLICY,
    );
    let html = String::from_utf8(html).unwrap_or_default();
    let marker = json!({
        "page": id,
        "title": title,
        "answerable": answerable,
    })
    .to_string()
    .replace('<', "\\u003c");
    let html = in_head(
        html,
        &format!("<script>window.__gxShared={marker};</script>"),
    );
    Some(SharedPage {
        id: id.to_string(),
        title,
        html,
        answerable,
    })
}
