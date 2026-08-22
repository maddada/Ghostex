use super::*;

pub(crate) fn handle_web_bootstrap(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    method: Method,
    request_id: String,
) -> RoutedResponse {
    let endpoint_path = Some("/api/webBootstrap".to_string());
    if method != Method::POST {
        return routed_json(
            endpoint_path,
            StatusCode::METHOD_NOT_ALLOWED,
            rpc_error(
                "methodNotAllowed",
                "/api/webBootstrap requires POST.",
                Some(request_id),
            ),
        );
    }
    let Some(base_url) = request_host_origin(headers, uri) else {
        return routed_json(
            endpoint_path,
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                "A valid Host header is required for web bootstrap.",
                Some(request_id),
            ),
        );
    };
    if !web_bootstrap_origin_matches(headers, &base_url) {
        return routed_json(
            endpoint_path,
            StatusCode::FORBIDDEN,
            rpc_error(
                "forbidden",
                "/api/webBootstrap is available only to the same origin.",
                Some(request_id),
            ),
        );
    }
    let machine_label = match local_machine_label() {
        Ok(machine_label) => machine_label,
        Err(message) => {
            return routed_json(
                endpoint_path,
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error("internalError", message, Some(request_id)),
            );
        }
    };
    routed_json(
        endpoint_path,
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "authToken": state.auth_token,
                "baseUrl": base_url,
                "machineLabel": machine_label,
                "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            }),
        ),
    )
}

pub(crate) fn request_host_origin(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    let host = headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .or_else(|| uri.authority().map(|authority| authority.as_str()))?
        .trim();
    let authority = host.parse::<axum::http::uri::Authority>().ok()?;
    Some(format!("http://{authority}"))
}

pub(crate) fn web_bootstrap_origin_matches(headers: &HeaderMap, base_url: &str) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    match origins.next() {
        None => true,
        Some(origin) => {
            matches!(origin.to_str(), Ok(origin) if origin == base_url) && origins.next().is_none()
        }
    }
}

#[cfg(unix)]
pub(crate) fn local_machine_label() -> std::result::Result<String, String> {
    let mut buffer = [0_u8; 256];
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if status != 0 {
        return Err("Failed to read the local machine hostname.".to_string());
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| "The local machine hostname is too long.".to_string())?;
    let hostname = std::str::from_utf8(&buffer[..length])
        .map_err(|_| "The local machine hostname is not valid UTF-8.".to_string())?;
    if hostname.is_empty() {
        return Err("The local machine hostname is empty.".to_string());
    }
    Ok(hostname.to_string())
}

#[cfg(windows)]
pub(crate) fn local_machine_label() -> std::result::Result<String, String> {
    use windows_sys::Win32::System::WindowsProgramming::GetComputerNameW;

    let mut buffer = [0_u16; 256];
    let mut length = buffer.len() as u32;
    if unsafe { GetComputerNameW(buffer.as_mut_ptr(), &mut length) } == 0 {
        return Err("Failed to read the local machine hostname.".to_string());
    }
    let hostname = String::from_utf16(&buffer[..length as usize])
        .map_err(|_| "The local machine hostname is not valid UTF-16.".to_string())?;
    if hostname.is_empty() {
        return Err("The local machine hostname is empty.".to_string());
    }
    Ok(hostname)
}

pub(crate) async fn serve_web_static(config: &GxserverConfig, request_path: &str) -> RoutedResponse {
    let config = config.clone();
    let request_path = request_path.to_string();
    match tokio::task::spawn_blocking(move || serve_web_static_sync(&config, &request_path)).await {
        Ok(response) => response,
        Err(_) => static_status_response(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub(crate) fn serve_web_static_sync(config: &GxserverConfig, request_path: &str) -> RoutedResponse {
    let relative_path = match decode_web_path(request_path) {
        Ok(path) => path,
        Err(()) => return static_status_response(StatusCode::FORBIDDEN),
    };
    let dist_dir = resolve_web_dist_dir(config);
    let canonical_dist_dir = match fs::canonicalize(&dist_dir) {
        Ok(path) if path.is_dir() => path,
        _ => return web_not_built_response(),
    };
    let requested_relative = if relative_path.as_os_str().is_empty() {
        PathBuf::from("index.html")
    } else {
        relative_path
    };
    let requested_path = canonical_dist_dir.join(&requested_relative);

    match read_static_file(&canonical_dist_dir, &requested_path) {
        Ok(Some(response)) => return response,
        Ok(None) => {}
        Err(()) => return static_status_response(StatusCode::FORBIDDEN),
    }
    if requested_relative.extension().is_none() {
        let index_path = canonical_dist_dir.join("index.html");
        match read_static_file(&canonical_dist_dir, &index_path) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(()) => return static_status_response(StatusCode::FORBIDDEN),
        }
    }
    static_status_response(StatusCode::NOT_FOUND)
}

pub(crate) fn resolve_web_dist_dir(config: &GxserverConfig) -> PathBuf {
    if let Some(configured) = config.web.dist_dir.as_ref() {
        return configured.clone();
    }
    // Packaged builds resolve beside the executable; source builds use the checkout containing this crate.
    let executable_candidate = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .map(|directory| directory.join("apps/web/dist"));
    if let Some(candidate) = executable_candidate.filter(|candidate| candidate.is_dir()) {
        return candidate;
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join("apps/web/dist")
}

pub(crate) fn decode_web_path(request_path: &str) -> std::result::Result<PathBuf, ()> {
    let bytes = request_path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let high = decode_hex_digit(bytes[index + 1]).ok_or(())?;
            let low = decode_hex_digit(bytes[index + 2]).ok_or(())?;
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
    for segment in decoded.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => return Err(()),
            segment => relative.push(segment),
        }
    }
    Ok(relative)
}

pub(crate) fn decode_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn read_static_file(
    dist_dir: &Path,
    path: &Path,
) -> std::result::Result<Option<RoutedResponse>, ()> {
    let canonical_path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Ok(None),
    };
    if !canonical_path.starts_with(dist_dir) {
        return Err(());
    }
    if !canonical_path.is_file() {
        return Ok(None);
    }
    let bytes = match fs::read(&canonical_path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let content_type = static_content_type(&canonical_path);
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    let cache_control =
        if canonical_path.file_name().and_then(|name| name.to_str()) == Some("index.html") {
            "no-cache"
        } else if is_hashed_asset(&canonical_path) {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    Ok(Some(RoutedResponse {
        endpoint_path: None,
        response,
    }))
}

pub(crate) fn static_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        Some("map") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub(crate) fn is_hashed_asset(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some((_, suffix)) = file_name.rsplit_once('-') else {
        return false;
    };
    let hash = suffix.split('.').next().unwrap_or_default();
    hash.len() >= 8
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub(crate) fn web_not_built_response() -> RoutedResponse {
    let html = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Ghostex Web</title></head><body><h1>The Ghostex web app is not built</h1><p>Run <code>bun run web:build</code> from the Ghostex checkout.</p></body></html>";
    let mut response = Response::new(Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}

pub(crate) fn static_status_response(status: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: status.into_response(),
    }
}
