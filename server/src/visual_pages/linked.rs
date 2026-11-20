//! A Live page's links to the HTML files beside it: `GET /visual/<id>/link?href=<as written>`
//! answers with a redirect to the linked file's own page.
//!
//! CDXC:SessionChat 2026-11-20 WHY:
//! User, after a cover page's link to one of its decks answered "HTTP ERROR 404"
//! (`http://127.0.0.1:58048/visual/clickhouse/clickhouse-analytics.html`, a deck set an agent on
//! another computer published): "when i navigate between urls that use localhost cuz now i see
//! 404". A page is served alone at `/visual/<id>`, so a link written relative to its file
//! (`clickhouse/deck.html`) resolved to a gxserver route that does not exist; the address itself
//! was right (this computer's tunnel to the computer that published the page). The page's
//! bootstrap (theme.rs `PAGE_BOOTSTRAP`) now sends such a link here, and the redirect lands on the
//! page `ghostex show` already published from that file, else on one made for it the way the Files
//! view opens a Ghostex page file (file_page.rs), read-only when no session published it. The
//! redirect names only a path, so the reader stays on whatever address reached this gxserver (a
//! tunnel from another computer, the phone's forward).
//! The page id is the only grant on these routes (serve.rs), so a link opens only when the page's
//! own markup holds it as an `<a href>` (not text, a comment or script), and only to an `.html`
//! file that declares itself a Ghostex page or that `ghostex show` published: a reader reaches what
//! the page's agent linked, nothing else. Inside the page's own folder and its subfolders that is
//! the whole rule; a link that leads out of it (`../cover.html`, after symlinks are resolved) opens
//! only a page `ghostex show` already published from that file, so a deck's link back to its cover
//! works and nothing elsewhere on the disk is published by a link. The route sends no cross-origin
//! headers (http_dispatch.rs), so only a navigation follows it: a page's own script cannot fetch it
//! to read the linked page.

use std::{
    fs,
    path::{Path, PathBuf},
};

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
};

use crate::server::RoutedResponse;

use super::{
    file_page::{page_for_linked_file, page_show_published},
    prepare::anchor_hrefs,
    serve::{PAGE_SCRIPT_QUERY, READ_ONLY_QUERY, VISUAL_PAGE_ROUTE_PREFIX},
    store::{is_page_id, page_source_path, read_page},
};

pub(crate) const VISUAL_PAGE_LINK_SUFFIX: &str = "/link";

/// Whether `GET <path>` is a page's link route, `/visual/<id>/link`.
pub(crate) fn is_link_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_LINK_SUFFIX)
}

/// `GET /visual/<id>/link?href=…`: a redirect to the page of the file `href` names, relative to
/// the page's own file. The reader's `gx-page-script` and `gx-readonly` flags carry over; the
/// fragment the bootstrap added (the theme, the floating window) is kept by the browser.
pub(crate) async fn follow(
    gxserver_state_dir: PathBuf,
    path: String,
    query: Option<String>,
) -> RoutedResponse {
    let Some(id) = page_of(&path) else {
        return not_found();
    };
    let query = query.unwrap_or_default();
    let Some(href) = url::form_urlencoded::parse(query.as_bytes())
        .find(|(name, _)| name == "href")
        .map(|(_, value)| value.into_owned())
    else {
        return not_found();
    };
    let flags: Vec<&str> = query
        .split('&')
        .filter(|pair| *pair == PAGE_SCRIPT_QUERY || *pair == READ_ONLY_QUERY)
        .collect();
    let linked = tokio::task::spawn_blocking(move || linked_page(&gxserver_state_dir, &id, &href))
        .await
        .ok()
        .flatten();
    let Some(linked) = linked else {
        return not_found();
    };
    let mut location = format!("{VISUAL_PAGE_ROUTE_PREFIX}{linked}");
    if !flags.is_empty() {
        location.push('?');
        location.push_str(&flags.join("&"));
    }
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}

/// The id of the page `href` (as page `id` writes it) opens, when the page links to it and the
/// file is one a link may open.
fn linked_page(gxserver_state_dir: &Path, id: &str, href: &str) -> Option<String> {
    let relative = relative_html_path(href)?;
    let page = read_page(gxserver_state_dir, id)?;
    let href = href.trim();
    if !anchor_hrefs(&String::from_utf8_lossy(&page))
        .iter()
        .any(|written| written == href)
    {
        return None;
    }
    let source = page_source_path(gxserver_state_dir, id)?;
    // Symlinks resolved on both sides, so a link inside the folder cannot lead out of it.
    let folder = fs::canonicalize(Path::new(&source).parent()?).ok()?;
    let file = fs::canonicalize(folder.join(relative)).ok()?;
    let linked = if file.starts_with(&folder) {
        page_for_linked_file(gxserver_state_dir, &file)
            .ok()
            .flatten()
    } else {
        page_show_published(gxserver_state_dir, &file)
    };
    linked.map(|page| page.id)
}

/// The file a relative link to an HTML file names, percent-decoded, without its query and
/// fragment: `None` for an absolute address or path, another scheme, or any other kind of file.
fn relative_html_path(href: &str) -> Option<PathBuf> {
    let href = href.trim();
    if href.is_empty()
        || href.starts_with(['/', '\\', '#', '?'])
        || href.contains(['\\', '\0'])
        || has_scheme(href)
    {
        return None;
    }
    let path = href.split(['?', '#']).next()?;
    let path = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;
    let lower = path.to_ascii_lowercase();
    if path.contains('\0') || !(lower.ends_with(".html") || lower.ends_with(".htm")) {
        return None;
    }
    // The same checks on the decoded path, so `%2Fetc%2Fx.html`, `%5C%5Chost%5C…` and `C%3A%5C…`
    // are no way around them.
    if path.contains('\\')
        || has_scheme(&path)
        || Path::new(path.as_ref()).components().any(|part| {
            matches!(
                part,
                std::path::Component::RootDir | std::path::Component::Prefix(_)
            )
        })
    {
        return None;
    }
    Some(PathBuf::from(path.as_ref()))
}

/// Whether `href` starts with a URL scheme (`https:`, `file:`, `c:`), which makes it no relative
/// path.
fn has_scheme(href: &str) -> bool {
    let Some(colon) = href.find(':') else {
        return false;
    };
    let scheme = &href[..colon];
    scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn page_of(path: &str) -> Option<String> {
    path.strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_LINK_SUFFIX))
        .filter(|id| is_page_id(id))
        .map(str::to_string)
}

fn not_found() -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: StatusCode::NOT_FOUND.into_response(),
    }
}
