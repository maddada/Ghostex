use std::path::PathBuf;

use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};

use crate::server::RoutedResponse;

use super::{
    builtin::with_current_block,
    prepare::PAGE_CONTENT_SECURITY_POLICY,
    store::{is_page_id, read_page, read_record},
    theme::PAGE_STATE_MARKER,
    version::is_current_layout,
};

pub(crate) const VISUAL_PAGE_ROUTE_PREFIX: &str = "/visual/";

/// The query that asks for [`PAGE_SCRIPT`] in the page: `/visual/<id>?gx-page-script=1`.
pub(crate) const PAGE_SCRIPT_QUERY: &str = "gx-page-script=1";

/// The query that opens a page read-only: `/visual/<id>?gx-readonly=1`. The Resources list of a
/// coordinator opens the pages its threads posted this way (gx-chat-core `READ_ONLY_QUERY`).
pub(crate) const READ_ONLY_QUERY: &str = "gx-readonly=1";

/// The floating page window's own script (its picture viewer and its scroll memory), put into the
/// page for a reader that cannot add it itself.
///
/// CDXC:SessionChat 2026-10-24 WHY:
/// The floating window adds this script through CEF; the Files view and the reader's own browser, where Settings > Chat > Open chat pages in can send a page instead, have no way to. They load the page with `?gx-page-script=1`, so the same file gives them the same viewer and the same scroll position on gxserver (`/visual/<id>/scroll`). The phone keeps its own page sheet script (user decision) and never asks for it.
/// SEE-ALSO: apps/desktop/src/app/window/visual_page_modal/page_script.js (the script), apps/desktop/src/app/visual_page_modal_lifecycle.rs (the setting).
const PAGE_SCRIPT: &str =
    include_str!("../../../apps/desktop/src/app/window/visual_page_modal/page_script.js");

/// `GET /visual/<id>`: a page `ghostex show` published, answered before the auth gate.
///
/// CDXC:ServerApi 2026-10-06 WHY:
/// The browser page that opens a chat card has no gxserver auth token, so the unguessable 128-bit id in the path is the only grant, as the `/open-file/` tokens are. The `sandbox` policy in the response header gives the page an opaque origin: its scripts run, but they cannot read gxserver's cookies or storage or call it as a same-origin page, and the content policy beside it keeps every fetch on https:.
///
/// CDXC:SessionChat 2026-10-06 DECISION:
/// User: no JavaScript engine is added to the desktop. An HTML page runs only in a browser page (Ghostex's browser, the system browser, or the phone's WebView) that opens this URL, never inline in the GPUI chat.
pub(crate) async fn serve(
    gxserver_state_dir: PathBuf,
    path: String,
    query: Option<String>,
    host: Option<String>,
) -> RoutedResponse {
    let Some(id) = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .filter(|id| is_page_id(id))
        .map(str::to_string)
    else {
        return status(StatusCode::NOT_FOUND);
    };
    let page_id = id.clone();
    let page_script = query
        .as_deref()
        .is_some_and(|query| query.split('&').any(|pair| pair == PAGE_SCRIPT_QUERY));
    let read_only = query
        .as_deref()
        .is_some_and(|query| query.split('&').any(|pair| pair == READ_ONLY_QUERY));
    let page = tokio::task::spawn_blocking(move || {
        read_page(&gxserver_state_dir, &id)
            .map(|page| with_page_state(page, read_record(&gxserver_state_dir, &id), page_script))
            .map(|page| {
                if read_only {
                    with_read_only(page)
                } else {
                    page
                }
            })
    })
    .await
    .ok()
    .flatten();
    let Some(page) = page else {
        return status(StatusCode::NOT_FOUND);
    };
    let mut response = RoutedResponse {
        endpoint_path: None,
        response: Response::new(Body::from(page)),
    };
    let content_security_policy = format!(
        "sandbox allow-scripts allow-forms allow-popups; {}",
        header_policy(&page_id, host.as_deref())
    );
    for (name, value) in [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (
            header::CONTENT_SECURITY_POLICY,
            content_security_policy.as_str(),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CACHE_CONTROL, "no-store"),
    ] {
        if let Ok(value) = HeaderValue::from_str(value) {
            response.response.headers_mut().insert(name, value);
        }
    }
    response
}

/// The page opened read-only: `window.__gxReadOnly` makes the page API's `submit` refuse (so no
/// answers leave it, whatever the page's own script does), and a note says why.
///
/// CDXC:SessionChat 2026-10-30 DECISION:
/// User: a coordinator's Resources lists "any of the inline html files that were posted in a thread". A thread's questions are answered by its coordinator bringing them over, and a page answers to the session that published it, so a page opened from the coordinator's list must not send answers on its own. It opens read-only; pressing Send says nothing was sent and why. The thread's own chat opens the same page normally, and it still takes answers there.
fn with_read_only(page: Vec<u8>) -> Vec<u8> {
    const SCRIPT: &str = "<script>window.__gxReadOnly=true;document.addEventListener('DOMContentLoaded',function(){var n=document.createElement('div');n.textContent='Read-only: a thread posted this page, so answers sent from here go nowhere.';n.style.cssText='position:fixed;top:8px;right:8px;z-index:2147483647;max-width:280px;padding:6px 10px;border-radius:8px;font:12px/1.35 system-ui,sans-serif;background:rgba(0,0,0,.72);color:#fff;pointer-events:none';document.body.appendChild(n);});</script>";
    let Ok(html) = String::from_utf8(page) else {
        return Vec::new();
    };
    // After `<head>` (never before the doctype, which would put the page in quirks mode).
    let at = html
        .find("<head")
        .and_then(|head| html[head..].find('>').map(|close| head + close + 1))
        .or_else(|| {
            html.find("<body")
                .and_then(|body| html[body..].find('>').map(|close| body + close + 1))
        })
        .unwrap_or(0);
    let mut out = html;
    out.insert_str(at, SCRIPT);
    out.into_bytes()
}

fn status(code: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: code.into_response(),
    }
}

/// The page's content policy for this response. The page's own copy allows it to reach its server
/// (`'self'`), and a browser enforces both policies, so naming the page's own routes here (its
/// answers, its draft and its scroll position, under `/visual/<id>/`) keeps the page off every
/// other gxserver route.
fn header_policy(id: &str, host: Option<&str>) -> String {
    let host = host.filter(|host| {
        !host.is_empty()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-:[]".contains(&byte))
    });
    match host {
        Some(host) => PAGE_CONTENT_SECURITY_POLICY.replace(
            "connect-src https: 'self'",
            &format!("connect-src https: http://{host}{VISUAL_PAGE_ROUTE_PREFIX}{id}/"),
        ),
        None => {
            PAGE_CONTENT_SECURITY_POLICY.replace("connect-src https: 'self'", "connect-src https:")
        }
    }
}

/// The page with today's built-in block (builtin.rs) and its answered state where `PAGE_API`
/// reads it: whether the reader already sent
/// answers, what they were, and when, or else the draft they left; and, for a page written without
/// the v2 marker, `oldLayout` and whether its notice was closed. A page published before
/// answering existed has no marker. `page_script` adds [`PAGE_SCRIPT`] there too, or before
/// `</head>` in a page without the marker.
fn with_page_state(page: Vec<u8>, record: Option<serde_json::Value>, page_script: bool) -> Vec<u8> {
    let html = match String::from_utf8(page) {
        Ok(html) => with_current_block(html),
        Err(error) => return error.into_bytes(),
    };
    let script = if page_script {
        // `</` never appears raw inside the script, so nothing in it can close the element.
        format!("<script>{}</script>", PAGE_SCRIPT.replace("</", "<\\/"))
    } else {
        String::new()
    };
    let Some(at) = html.find(PAGE_STATE_MARKER) else {
        return match html.find("</head>").filter(|_| page_script) {
            Some(head_end) => {
                let mut out = html;
                out.insert_str(head_end, &script);
                out.into_bytes()
            }
            None => html.into_bytes(),
        };
    };
    let answered_at = record
        .as_ref()
        .and_then(|record| record.get("answeredAt"))
        .and_then(serde_json::Value::as_str);
    let mut state = match answered_at {
        Some(answered_at) => serde_json::json!({
            "answered": true,
            "answeredAt": answered_at,
            "answers": record.as_ref().and_then(|record| record.get("answers")).cloned(),
        }),
        None => serde_json::json!({
            "answered": false,
            "draft": record.as_ref().and_then(|record| record.get("draft")).cloned(),
        }),
    };
    // A page written without the v2 marker shows the page API's old-layout notice until the
    // reader closes it (version.rs, notice.rs).
    if !is_current_layout(&html) {
        state["oldLayout"] = serde_json::Value::Bool(true);
        state["noticeDismissed"] = serde_json::Value::Bool(
            record
                .as_ref()
                .is_some_and(|record| record.get("layoutNoticeDismissedAt").is_some()),
        );
    }
    // `<` never appears raw inside the script, so no value can close it.
    let state = state.to_string().replace('<', "\\u003c");
    let mut out = String::with_capacity(html.len() + state.len() + script.len() + 64);
    out.push_str(&html[..at]);
    out.push_str("<script>window.ghostex&&ghostex.__state(");
    out.push_str(&state);
    out.push_str(");</script>");
    out.push_str(&script);
    out.push_str(&html[at + PAGE_STATE_MARKER.len()..]);
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual_pages::builtin::current_block;

    fn page(head: &str) -> Vec<u8> {
        format!(
            "<!doctype html><html><head>{}{head}<title>t</title></head><body><p>x</p></body></html>",
            current_block()
        )
        .into_bytes()
    }

    fn state_of(served: Vec<u8>) -> serde_json::Value {
        let html = String::from_utf8(served).unwrap();
        let start = html.find("ghostex.__state(").unwrap() + "ghostex.__state(".len();
        let end = start + html[start..].find(");</script>").unwrap();
        serde_json::from_str(&html[start..end]).unwrap()
    }

    #[test]
    fn a_read_only_page_refuses_answers_and_keeps_its_doctype_first() {
        let served = String::from_utf8(with_read_only(page(""))).unwrap();
        assert!(served.starts_with("<!doctype html>"));
        let head = served.find("<head>").unwrap();
        assert!(served.find("window.__gxReadOnly=true").unwrap() > head);
        // The page API's submit (every page `ghostex show` publishes carries it) checks the flag
        // before it posts anything.
        let api = super::super::theme::PAGE_API;
        let check = api
            .find("if(window.__gxReadOnly){reject(")
            .expect("submit checks the flag");
        let post = api
            .find("fetch(base+\"/answers\"")
            .expect("submit posts the answers");
        assert!(
            check < post,
            "the check runs before any answers leave the page"
        );
        let served =
            String::from_utf8(with_read_only(with_page_state(page(""), None, false))).unwrap();
        assert!(served.contains("window.__gxReadOnly=true"));
        assert!(
            served.contains("if(window.__gxReadOnly){reject("),
            "a served page carries the guard"
        );
    }

    #[test]
    fn a_page_without_the_v2_marker_asks_for_the_old_layout_notice() {
        let state = state_of(with_page_state(page(""), None, false));
        assert_eq!(state["oldLayout"], true);
        assert_eq!(state["noticeDismissed"], false);
    }

    #[test]
    fn a_closed_notice_stays_closed_for_that_page() {
        let record = serde_json::json!({"layoutNoticeDismissedAt": "2026-10-27T00:00:00.000Z"});
        let state = state_of(with_page_state(page(""), Some(record), false));
        assert_eq!(state["oldLayout"], true);
        assert_eq!(state["noticeDismissed"], true);
    }

    #[test]
    fn a_v2_page_gets_no_notice() {
        let state = state_of(with_page_state(
            page("<meta name=\"ghostex-page\" content=\"v2\">"),
            None,
            false,
        ));
        assert!(state.get("oldLayout").is_none());
        assert_eq!(state["answered"], false);
    }
}
