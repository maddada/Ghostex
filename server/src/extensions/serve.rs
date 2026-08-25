use std::{fs, path::PathBuf};

use axum::{
    body::Body,
    http::{HeaderValue, Response, StatusCode, header},
    response::IntoResponse,
};

use crate::server::RoutedResponse;

use super::{ExtensionRegistry, validate_extension_id};

pub(crate) async fn serve_extension_static(
    registry: ExtensionRegistry,
    request_path: String,
) -> RoutedResponse {
    tokio::task::spawn_blocking(move || serve_extension_static_sync(&registry, &request_path))
        .await
        .unwrap_or_else(|_| status_response(StatusCode::INTERNAL_SERVER_ERROR))
}

fn serve_extension_static_sync(registry: &ExtensionRegistry, request_path: &str) -> RoutedResponse {
    let Some(remainder) = request_path.strip_prefix("/ext/") else {
        return status_response(StatusCode::NOT_FOUND);
    };
    let (id, encoded_path) = remainder.split_once('/').unwrap_or((remainder, ""));
    if validate_extension_id(id).is_err() {
        return status_response(StatusCode::FORBIDDEN);
    }
    let relative_path = match decode_relative_path(encoded_path) {
        Ok(path) => path,
        Err(()) => return status_response(StatusCode::FORBIDDEN),
    };
    let canonical_path = match registry.static_icon_path(id, &relative_path) {
        Ok(Some(path)) => path,
        Ok(None) => {
            let static_root = match registry.static_root(id) {
                Ok(path) => path,
                Err(error) if error.code == "notFound" => {
                    return status_response(StatusCode::NOT_FOUND);
                }
                Err(error) if error.code == "badRequest" => {
                    return status_response(StatusCode::FORBIDDEN);
                }
                Err(_) => return status_response(StatusCode::INTERNAL_SERVER_ERROR),
            };
            let canonical_root = match fs::canonicalize(&static_root) {
                Ok(path) if path.is_dir() => path,
                _ => return status_response(StatusCode::NOT_FOUND),
            };
            let requested_relative = if relative_path.as_os_str().is_empty() {
                PathBuf::from("index.html")
            } else {
                relative_path
            };
            match fs::canonicalize(canonical_root.join(requested_relative)) {
                Ok(path) if path.starts_with(&canonical_root) && path.is_file() => path,
                Ok(_) => return status_response(StatusCode::FORBIDDEN),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return status_response(StatusCode::NOT_FOUND);
                }
                Err(_) => return status_response(StatusCode::NOT_FOUND),
            }
        }
        Err(error) if error.code == "notFound" => return status_response(StatusCode::NOT_FOUND),
        Err(error) if error.code == "badRequest" => return status_response(StatusCode::FORBIDDEN),
        Err(_) => return status_response(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let bytes = match fs::read(&canonical_path) {
        Ok(bytes) => bytes,
        Err(_) => return status_response(StatusCode::NOT_FOUND),
    };
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(extension_content_type(&canonical_path)),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}

fn decode_relative_path(value: &str) -> Result<PathBuf, ()> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let high = decode_hex(bytes[index + 1]).ok_or(())?;
            let low = decode_hex(bytes[index + 2]).ok_or(())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    if decoded.contains(&0) {
        return Err(());
    }
    let decoded = std::str::from_utf8(&decoded).map_err(|_| ())?;
    let mut relative = PathBuf::new();
    for segment in decoded.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains('\\') {
            return Err(());
        }
        relative.push(segment);
    }
    Ok(relative)
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn extension_content_type(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        Some("json" | "map") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn status_response(status: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: status.into_response(),
    }
}
