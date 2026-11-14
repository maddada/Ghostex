use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
    response::IntoResponse,
};

use crate::server::RoutedResponse;

pub(crate) const FILE_LINK_ROUTE_PREFIX: &str = "/open-file/";

/// A ranged answer is capped so seeking in a long video never reads the whole file at once; the
/// browser asks again for the rest.
const RANGE_CHUNK_MAX_BYTES: u64 = 16 * 1024 * 1024;
const DRAWING_MAX_BYTES: usize = 64 * 1024 * 1024;

/// CDXC:Docs 2026-11-14 DECISION:
/// A link answer a browser shows as a document (an HTML or SVG file, an XML file, the rendered
/// Markdown page) gets the sandbox the Files preview has: its scripts run in their own opaque
/// origin, so a page from a project can no longer read the link's other files, gxserver's cookies
/// or storage as a same-origin page. A Ghostex page (`/visual/`) already has its own sandbox, and
/// the Excalidraw editor page, which Ghostex writes and which saves back to its own link, keeps its
/// origin. Same flags as the Files preview frame (apps/desktop/views/manage/preview/html-viewer.tsx)
/// without `allow-same-origin`.
const DOCUMENT_SANDBOX_POLICY: &str = "sandbox allow-scripts allow-forms allow-modals allow-popups allow-popups-to-escape-sandbox allow-downloads allow-presentation";

/// `GET` reads a file under the link's folder (Markdown and drawings as pages unless `?raw=1`),
/// `PUT` saves a drawing from the Excalidraw page.
pub(crate) async fn serve(
    gxserver_state_dir: PathBuf,
    method: Method,
    path: String,
    query: Option<String>,
    headers: HeaderMap,
    body: Body,
) -> RoutedResponse {
    let body = if method == Method::PUT {
        match axum::body::to_bytes(body, DRAWING_MAX_BYTES).await {
            Ok(bytes) => Some(bytes),
            Err(_) => return status(StatusCode::PAYLOAD_TOO_LARGE),
        }
    } else {
        None
    };
    tokio::task::spawn_blocking(move || {
        let Some(file) = resolve(&path) else {
            return status(StatusCode::NOT_FOUND);
        };
        let raw = query
            .as_deref()
            .is_some_and(|query| query.split('&').any(|pair| pair == "raw=1"));
        answer(&gxserver_state_dir, method, body, &file, raw, &headers)
    })
    .await
    .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

/// The answer for one resolved file of a link.
fn answer(
    gxserver_state_dir: &Path,
    method: Method,
    body: Option<axum::body::Bytes>,
    file: &Path,
    raw: bool,
    headers: &HeaderMap,
) -> RoutedResponse {
    match (method, body) {
        (Method::PUT, Some(body)) if is_drawing(file) => save_drawing(file, &body),
        (Method::GET, _) if !raw && is_markdown(file) => super::pages::markdown(file)
            .map(html)
            .map(sandboxed)
            .unwrap_or_else(|| status(StatusCode::NOT_FOUND)),
        (Method::GET, _) if !raw && is_drawing(file) => html(super::pages::drawing(file)),
        // A Ghostex page file opens as its page, with the page kit (visual_pages/file_page.rs).
        (Method::GET, _) if !raw && is_html(file) => {
            match crate::visual_pages::page_for_file(gxserver_state_dir, file) {
                Ok(Some(page)) => redirect(&format!(
                    "{}{}?{}",
                    crate::visual_pages::VISUAL_PAGE_ROUTE_PREFIX,
                    page.id,
                    crate::visual_pages::PAGE_SCRIPT_QUERY
                )),
                _ => read(file, headers),
            }
        }
        (Method::GET, _) => read(file, headers),
        _ => status(StatusCode::METHOD_NOT_ALLOWED),
    }
}

fn resolve(path: &str) -> Option<PathBuf> {
    let (token, relative) = path.strip_prefix(FILE_LINK_ROUTE_PREFIX)?.split_once('/')?;
    let grant = super::registry::grant_for_token(token)?;
    let relative = crate::extensions::serve::decode_relative_path(relative).ok()?;
    match grant {
        super::registry::Grant::Folder(root) => {
            if super::protected::names_hidden_entry(&relative) {
                return None;
            }
            let file = fs::canonicalize(root.join(relative)).ok()?;
            (file.starts_with(&root) && file.is_file() && !super::protected::is_protected(&file))
                .then_some(file)
        }
        // A one-file link answers its own name only.
        super::registry::Grant::File(file) => {
            let named = file
                .file_name()
                .is_some_and(|name| relative.as_os_str() == name);
            (named && file.is_file() && !super::protected::is_protected(&file)).then_some(file)
        }
    }
}

fn extension(file: &Path) -> String {
    file.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_markdown(file: &Path) -> bool {
    matches!(extension(file).as_str(), "md" | "markdown" | "mdx")
}

fn is_html(file: &Path) -> bool {
    matches!(extension(file).as_str(), "html" | "htm")
}

/// A relative redirect, so it keeps whatever host and tunnel the browser reached this through.
fn redirect(location: &str) -> RoutedResponse {
    let mut response = status(StatusCode::FOUND);
    insert(&mut response, header::LOCATION, location);
    no_store(response)
}

/// Files a browser renders as a document that can run script when opened on their own.
fn is_document(file: &Path) -> bool {
    matches!(extension(file).as_str(), "html" | "htm" | "svg" | "xml")
}

/// A sandboxed page loads its fonts in CORS mode from another origin; a font shows nothing of its
/// bytes to the page, so it may be read from anywhere (the Files resource origin's rule).
fn is_font(file: &Path) -> bool {
    matches!(extension(file).as_str(), "woff" | "woff2" | "ttf" | "otf")
}

fn is_drawing(file: &Path) -> bool {
    extension(file) == "excalidraw"
}

fn content_type(file: &Path) -> &'static str {
    match extension(file).as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" | "cjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" | "excalidraw" => "application/json; charset=utf-8",
        "md" | "markdown" | "mdx" | "txt" | "csv" | "log" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn read(file: &Path, headers: &HeaderMap) -> RoutedResponse {
    let Ok(mut handle) = fs::File::open(file) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Ok(total) = handle.metadata().map(|metadata| metadata.len()) else {
        return status(StatusCode::NOT_FOUND);
    };
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_range(value, total));
    let (start, end) = match range {
        Some(range) => range,
        None if headers.contains_key(header::RANGE) && total > 0 => {
            let mut response = status(StatusCode::RANGE_NOT_SATISFIABLE);
            insert(
                &mut response,
                header::CONTENT_RANGE,
                &format!("bytes */{total}"),
            );
            return response;
        }
        None => (0, total.saturating_sub(1)),
    };
    let length = if total == 0 { 0 } else { end - start + 1 };
    let mut bytes = vec![0; length as usize];
    if handle.seek(SeekFrom::Start(start)).is_err() || handle.read_exact(&mut bytes).is_err() {
        return status(StatusCode::NOT_FOUND);
    }
    let mut response = RoutedResponse {
        endpoint_path: None,
        response: Response::new(Body::from(bytes)),
    };
    if range.is_some() {
        *response.response.status_mut() = StatusCode::PARTIAL_CONTENT;
        insert(
            &mut response,
            header::CONTENT_RANGE,
            &format!("bytes {start}-{end}/{total}"),
        );
    }
    insert(&mut response, header::CONTENT_TYPE, content_type(file));
    insert(&mut response, header::ACCEPT_RANGES, "bytes");
    if is_font(file) {
        insert(&mut response, header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    }
    let response = no_store(response);
    if is_document(file) {
        sandboxed(response)
    } else {
        response
    }
}

/// `bytes=start-end`, `bytes=start-` or `bytes=-suffix`; one range only.
fn parse_range(value: &str, total: u64) -> Option<(u64, u64)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    if total == 0 || end.contains(',') {
        return None;
    }
    let (start, end) = match (start.trim(), end.trim()) {
        ("", suffix) => {
            let suffix = suffix.parse::<u64>().ok()?.min(total);
            (total - suffix, total - 1)
        }
        (start, "") => {
            let start = start.parse::<u64>().ok()?;
            (start, (start + RANGE_CHUNK_MAX_BYTES - 1).min(total - 1))
        }
        (start, end) => (
            start.parse::<u64>().ok()?,
            end.parse::<u64>().ok()?.min(total - 1),
        ),
    };
    (start <= end && start < total).then_some((start, end))
}

fn save_drawing(file: &Path, body: &[u8]) -> RoutedResponse {
    if serde_json::from_slice::<serde_json::Value>(body).is_err() {
        return status(StatusCode::BAD_REQUEST);
    }
    match fs::write(file, body) {
        Ok(()) => status(StatusCode::NO_CONTENT),
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn html(page: String) -> RoutedResponse {
    let mut response = RoutedResponse {
        endpoint_path: None,
        response: Response::new(Body::from(page)),
    };
    insert(
        &mut response,
        header::CONTENT_TYPE,
        "text/html; charset=utf-8",
    );
    no_store(response)
}

fn no_store(mut response: RoutedResponse) -> RoutedResponse {
    insert(&mut response, header::CACHE_CONTROL, "no-store");
    insert(&mut response, header::REFERRER_POLICY, "no-referrer");
    insert(&mut response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    response
}

fn sandboxed(mut response: RoutedResponse) -> RoutedResponse {
    insert(
        &mut response,
        header::CONTENT_SECURITY_POLICY,
        DOCUMENT_SANDBOX_POLICY,
    );
    response
}

fn insert(response: &mut RoutedResponse, name: header::HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        response.response.headers_mut().insert(name, value);
    }
}

fn status(code: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: code.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(state: &Path, file: &Path, raw: bool) -> RoutedResponse {
        answer(state, Method::GET, None, file, raw, &HeaderMap::new())
    }

    fn header_value(response: &RoutedResponse, name: header::HeaderName) -> Option<String> {
        response
            .response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    }

    fn project() -> (tempfile::TempDir, PathBuf) {
        let folder = tempfile::tempdir().expect("temp folder");
        let state = folder.path().join("state");
        fs::create_dir_all(&state).expect("state folder");
        for (name, text) in [
            (
                "page.html",
                "<!doctype html><script>fetch('notes.txt')</script>",
            ),
            ("note.md", "# Note\n\n<img src=x onerror=alert(1)>"),
            (
                "logo.svg",
                "<svg xmlns='http://www.w3.org/2000/svg'><script>1</script></svg>",
            ),
            (
                "plan.excalidraw",
                "{\"type\":\"excalidraw\",\"elements\":[]}",
            ),
            ("font.woff2", "font"),
            ("data.json", "{}"),
        ] {
            fs::write(folder.path().join(name), text).expect("write");
        }
        (folder, state)
    }

    #[test]
    fn html_markdown_and_svg_answers_are_sandboxed() {
        let (folder, state) = project();
        for (name, raw) in [
            ("page.html", false),
            ("page.html", true),
            ("note.md", false),
            ("logo.svg", false),
        ] {
            let response = get(&state, &folder.path().join(name), raw);
            assert_eq!(response.response.status(), StatusCode::OK, "{name}");
            let policy = header_value(&response, header::CONTENT_SECURITY_POLICY)
                .unwrap_or_else(|| panic!("{name} (raw={raw}) has no content policy"));
            assert!(policy.starts_with("sandbox "), "{name}: {policy}");
            assert!(policy.contains("allow-scripts"), "{name}: {policy}");
            assert!(!policy.contains("allow-same-origin"), "{name}: {policy}");
            assert_eq!(
                header_value(&response, header::X_CONTENT_TYPE_OPTIONS).as_deref(),
                Some("nosniff")
            );
        }
    }

    #[test]
    fn a_ranged_html_answer_is_sandboxed_too() {
        let (folder, state) = project();
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=0-4"));
        let response = answer(
            &state,
            Method::GET,
            None,
            &folder.path().join("page.html"),
            true,
            &headers,
        );
        assert_eq!(response.response.status(), StatusCode::PARTIAL_CONTENT);
        assert!(header_value(&response, header::CONTENT_SECURITY_POLICY).is_some());
    }

    #[test]
    fn drawings_data_and_fonts_keep_their_origin() {
        let (folder, state) = project();
        for (name, raw) in [
            ("plan.excalidraw", false),
            ("plan.excalidraw", true),
            ("data.json", false),
            ("note.md", true),
            ("font.woff2", false),
        ] {
            let response = get(&state, &folder.path().join(name), raw);
            assert_eq!(response.response.status(), StatusCode::OK, "{name}");
            assert!(
                header_value(&response, header::CONTENT_SECURITY_POLICY).is_none(),
                "{name} (raw={raw}) should not be sandboxed"
            );
        }
        let font = get(&state, &folder.path().join("font.woff2"), false);
        assert_eq!(
            header_value(&font, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
            Some("*")
        );
        let data = get(&state, &folder.path().join("data.json"), false);
        assert!(header_value(&data, header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }
}
