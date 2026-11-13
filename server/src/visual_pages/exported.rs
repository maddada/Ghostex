//! Exported pages: any Ghostex page as one HTML file that works in any browser without Ghostex,
//! to send to someone who answers it there. The file is the page as `serve` would give it, with
//! today's built-in block (the theme, the page API and the question kit inlined), a content policy
//! that allows no requests (only what the page itself loads from https), and `window.__gxExported` set, which turns the page's
//! Send into Export answers: the page API keeps its draft in the browser and shows the answers as
//! JSON to copy or download (theme.rs `PAGE_API`). The answers come back through
//! `/api/importVisualPageAnswers` (import.rs).
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User: "let's add the ability to do standalone briefs that gives you an html file you can download from Ghostex and attach anywhere so people can answer and give you back the json export of the answers", then: every Ghostex page can be exported this way (no special page type), inside Ghostex a page works exactly as before, and the action is named plainly: an Export button on every page card and page window downloads the "exported page", whose Send button reads Export answers (Q2909).
//! SEE-ALSO: theme.rs `PAGE_API` (exported mode, the answers dialog), kit.rs `KIT_SCRIPT` (the per-question detail and the Export answers button), import.rs (taking the answers back), skills/ghostex-visuals/SKILL.md and skills/ghostex-help/references/features.md (described there).

use std::{fs, path::Path};

use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use serde_json::{json, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::{
    file_page::page_for_file,
    prepare::{page_too_large, prepare_page},
    serve::{in_head, with_page_state, VISUAL_PAGE_ROUTE_PREFIX},
    store::{is_page_id, new_page_id, read_page, read_record, VISUAL_PAGE_MAX_BYTES},
};

pub(crate) const VISUAL_PAGE_EXPORT_SUFFIX: &str = "/export";
pub(crate) const EXPORT_VISUAL_PAGE_ENDPOINT: &str = "/api/exportVisualPage";

/// An exported page's content policy: the published one (prepare.rs) with no requests at all
/// (`connect-src 'none'`), so the file talks to nothing and tells nobody it was opened. What the
/// page itself loads from https (a library from a CDN, a web font) still loads when the reader is
/// online, as it does in Ghostex; everything Ghostex adds is inline.
pub(super) const EXPORTED_PAGE_CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' https:; style-src 'unsafe-inline' https:; img-src data: blob: https:; font-src data: https:; connect-src 'none'; media-src data: blob: https:; frame-src 'none'; form-action 'none'; base-uri 'none'";

/// A page made into its exported file.
pub(crate) struct ExportedPage {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) file_name: String,
    pub(crate) html: String,
}

/// Whether `GET <path>` is a page's export route, `/visual/<id>/export`.
pub(crate) fn is_export_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_EXPORT_SUFFIX)
}

/// The exported file of page `id`, whether or not it was answered: the file is always blank, for
/// someone else to answer.
pub(crate) fn exported_page(gxserver_state_dir: &Path, id: &str) -> Option<ExportedPage> {
    let record = read_record(gxserver_state_dir, id)?;
    let page = read_page(gxserver_state_dir, id)?;
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Ghostex page")
        .to_string();
    let awaits_seen = record.get("awaitsSeen").and_then(Value::as_bool) == Some(true);
    Some(export(page, id, title, awaits_seen))
}

fn export(page: Vec<u8>, id: &str, title: String, awaits_seen: bool) -> ExportedPage {
    let state = if awaits_seen {
        json!({"awaitsSeen": true})
    } else {
        json!({})
    };
    let html = with_page_state(
        page,
        Some(state),
        false,
        EXPORTED_PAGE_CONTENT_SECURITY_POLICY,
    );
    let html = String::from_utf8(html).unwrap_or_default();
    // `<` never appears raw inside the script, so the title cannot close it.
    let marker = json!({"page": id, "title": title})
        .to_string()
        .replace('<', "\\u003c");
    let html = in_head(
        html,
        &format!("<script>window.__gxExported={marker};</script>"),
    );
    ExportedPage {
        id: id.to_string(),
        file_name: file_name(&title),
        title,
        html,
    }
}

/// `<title as words>.html`, in plain ASCII so every browser and mail program keeps it.
fn file_name(title: &str) -> String {
    let mut name = String::new();
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
        if name.len() >= 60 {
            break;
        }
    }
    let name = name.trim_end_matches('-');
    if name.is_empty() {
        "ghostex-page.html".to_string()
    } else {
        format!("{name}.html")
    }
}

/// `GET /visual/<id>/export`: the exported page as a download, answered before the auth gate like
/// the page itself (its unguessable id is the grant), so the page card and the page window can hand
/// it to the reader's browser, on this computer or through a remote computer's forward.
pub(crate) async fn download(
    gxserver_state_dir: std::path::PathBuf,
    path: String,
) -> RoutedResponse {
    let id = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_EXPORT_SUFFIX))
        .filter(|id| is_page_id(id))
        .map(str::to_string);
    let exported = match id {
        Some(id) => tokio::task::spawn_blocking(move || exported_page(&gxserver_state_dir, &id))
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let Some(exported) = exported else {
        return RoutedResponse {
            endpoint_path: None,
            response: StatusCode::NOT_FOUND.into_response(),
        };
    };
    let mut response = Response::new(Body::from(exported.html));
    let disposition = format!("attachment; filename=\"{}\"", exported.file_name);
    for (name, value) in [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (header::CONTENT_DISPOSITION, disposition.as_str()),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CACHE_CONTROL, "no-store"),
    ] {
        if let Ok(value) = HeaderValue::from_str(value) {
            response.headers_mut().insert(name, value);
        }
    }
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}

/// `/api/exportVisualPage` `{pageId}` or `{file}`: `{pageId, title, fileName, html}`, the exported
/// page (`ghostex visual export`). A file is the page `ghostex show` published from it when there
/// is one, so answers imported later mark that page answered; any other HTML file is prepared as
/// `ghostex show` would and exported under a new id, so its answers reach the chat they are
/// imported into.
pub(crate) fn handle_export_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let state_dir = &state.paths.root_dir;
    let exported = if let Some(url) = params.get("url").and_then(Value::as_str) {
        export_url(state_dir, url).map_err(DomainStateError::bad_request)
    } else if let Some(id) = params.get("pageId").and_then(Value::as_str) {
        let id = id.trim().to_ascii_lowercase();
        if !is_page_id(&id) {
            Err(DomainStateError::bad_request("pageId is not a page id."))
        } else {
            exported_page(state_dir, &id)
                .ok_or_else(|| DomainStateError::not_found("No such page."))
        }
    } else if let Some(file) = params.get("file").and_then(Value::as_str) {
        export_file(state_dir, Path::new(file)).map_err(DomainStateError::bad_request)
    } else {
        Err(DomainStateError::bad_request(
            "This call requires pageId or file.",
        ))
    };
    match exported {
        Ok(exported) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(
                request_id,
                json!({
                    "pageId": exported.id,
                    "title": exported.title,
                    "fileName": exported.file_name,
                    "html": exported.html,
                }),
            ),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/// The page at `url` (`http://127.0.0.1:<port>/visual/<id>`, this computer's own or another
/// computer's through its tunnel), exported here: the page this computer stores under that id, or
/// else the page as its own computer serves it, with today's built-in block from this gxserver in
/// place of the one it was served with (`with_current_block`), so a page on a computer running an
/// older Ghostex, which has no `/export` route, exports all the same.
///
/// CDXC:SessionChat 2026-11-13 WHY:
/// Export page on a page from another computer handed the system browser the page's tunnel address with `/export` (`http://127.0.0.1:50423/visual/<id>/export`): the other computer ran 11.5.0, which has no such route, so the browser showed a 404, and a tunnel port means nothing to a browser anyway. The desktop now asks its own gxserver, which reads the page over the tunnel the way the page window loads it and saves the file locally.
fn export_url(gxserver_state_dir: &Path, url: &str) -> Result<ExportedPage, String> {
    let parsed =
        url::Url::parse(url.trim()).map_err(|_| "That is not a page address.".to_string())?;
    let id = parsed
        .path()
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .map(|id| id.trim_end_matches('/').to_ascii_lowercase())
        .filter(|id| is_page_id(id))
        .ok_or_else(|| "That is not a page address.".to_string())?;
    if let Some(exported) = exported_page(gxserver_state_dir, &id) {
        return Ok(exported);
    }
    // Only a page on this machine's loopback: this computer's gxserver or a tunnel to another's.
    let loopback = matches!(parsed.scheme(), "http" | "https")
        && matches!(
            parsed.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("[::1]") | Some("::1")
        );
    if !loopback {
        return Err("Only a Ghostex page's own address can be exported.".to_string());
    }
    let page_url = format!(
        "{}://{}{}{id}",
        parsed.scheme(),
        parsed.host_str().unwrap_or("127.0.0.1"),
        parsed
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default()
            + VISUAL_PAGE_ROUTE_PREFIX
    );
    let mut response = crate::http_agent::answering_agent()
        .get(&page_url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| format!("The page's computer could not be reached: {error}"))?;
    if !response.status().is_success() {
        return Err("The page's computer no longer has this page.".to_string());
    }
    let html = response
        .body_mut()
        .with_config()
        .limit(VISUAL_PAGE_MAX_BYTES as u64 * 2)
        .read_to_string()
        .map_err(|error| format!("The page could not be read: {error}"))?;
    let title = page_title(&html).unwrap_or_else(|| "Ghostex page".to_string());
    Ok(export(html.into_bytes(), &id, title, false))
}

/// The text of the page's `<title>`, when it has one.
fn page_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open = start + lower[start..].find('>')? + 1;
    let close = open + lower[open..].find("</title")?;
    let title = html[open..close]
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

fn export_file(gxserver_state_dir: &Path, file: &Path) -> Result<ExportedPage, String> {
    if !file.is_absolute() {
        return Err("The file's path must be absolute.".to_string());
    }
    if let Some(page) = page_for_file(gxserver_state_dir, file)? {
        return exported_page(gxserver_state_dir, &page.id)
            .ok_or_else(|| "The page could not be read.".to_string());
    }
    let metadata =
        fs::metadata(file).map_err(|error| format!("Could not read the file: {error}"))?;
    if metadata.len() > VISUAL_PAGE_MAX_BYTES as u64 {
        return Err(page_too_large(metadata.len() as usize));
    }
    let source =
        fs::read_to_string(file).map_err(|error| format!("Could not read the file: {error}"))?;
    let prepared = prepare_page(&source, file.parent().unwrap_or(Path::new("/")))?;
    let title = prepared.title.clone().unwrap_or_else(|| {
        file.file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Ghostex page".to_string())
    });
    Ok(export(
        prepared.html.into_bytes(),
        &new_page_id(),
        title,
        false,
    ))
}
