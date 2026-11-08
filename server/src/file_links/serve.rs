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
        match (method, body) {
            (Method::PUT, Some(body)) if is_drawing(&file) => save_drawing(&file, &body),
            (Method::GET, _) if !raw && is_markdown(&file) => super::pages::markdown(&file)
                .map(html)
                .unwrap_or_else(|| status(StatusCode::NOT_FOUND)),
            (Method::GET, _) if !raw && is_drawing(&file) => html(super::pages::drawing(&file)),
            // A Ghostex page file opens as its page, with the page kit (visual_pages/file_page.rs).
            (Method::GET, _) if !raw && is_html(&file) => {
                match crate::visual_pages::page_for_file(&gxserver_state_dir, &file) {
                    Ok(Some(page)) => redirect(&format!(
                        "{}{}?{}",
                        crate::visual_pages::VISUAL_PAGE_ROUTE_PREFIX,
                        page.id,
                        crate::visual_pages::PAGE_SCRIPT_QUERY
                    )),
                    _ => read(&file, &headers),
                }
            }
            (Method::GET, _) => read(&file, &headers),
            _ => status(StatusCode::METHOD_NOT_ALLOWED),
        }
    })
    .await
    .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
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
    no_store(response)
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
