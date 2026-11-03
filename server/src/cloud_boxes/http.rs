//! Sends the requests `ghostex_gx_cloud` builds. Blocking: call from `spawn_blocking`. Requests
//! carry keys in their headers, so nothing here logs a request.

use std::sync::OnceLock;
use std::time::Duration;

use ghostex_gx_cloud::http::{HttpBody, HttpRequest, HttpResponse, Method, ProviderError};

const USER_AGENT: &str = "Ghostex-CloudBoxes/1.0";

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        crate::http_agent::config()
            .timeout_connect(Some(Duration::from_secs(15)))
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .build()
            .into()
    })
}

fn form_encode(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                ghostex_gx_cloud::http::encode_query(key),
                ghostex_gx_cloud::http::encode_query(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub(super) fn send(request: &HttpRequest) -> Result<HttpResponse, ProviderError> {
    let builder = ureq::http::Request::builder()
        .method(request.method.as_str())
        .uri(&request.url);
    let builder = request
        .headers
        .iter()
        .fold(builder, |builder, (name, value)| {
            builder.header(name, value)
        });
    let timeout = Some(Duration::from_secs(request.timeout_seconds.max(1)));
    let (content_type, bytes): (Option<&str>, Vec<u8>) = match &request.body {
        None => (None, Vec::new()),
        Some(HttpBody::Json(value)) => (
            Some("application/json"),
            serde_json::to_vec(value).unwrap_or_default(),
        ),
        Some(HttpBody::Form(fields)) => (
            Some("application/x-www-form-urlencoded"),
            form_encode(fields).into_bytes(),
        ),
        Some(HttpBody::Bytes {
            content_type,
            bytes,
        }) => (Some(content_type.as_str()), bytes.clone()),
    };
    let builder = match content_type {
        Some(content_type) => builder.header("Content-Type", content_type),
        None => builder,
    };
    let has_body =
        request.body.is_some() || !matches!(request.method, Method::Get | Method::Delete);
    let http_request = builder
        .body(if has_body { bytes } else { Vec::new() })
        .map_err(|error| ProviderError::unexpected(format!("Bad provider request: {error}")))?;
    let http_request = agent()
        .configure_request(http_request)
        .timeout_global(timeout)
        .build();
    let result = agent().run(http_request);
    match result {
        Ok(mut response) => {
            let status = response.status().as_u16();
            let body = response
                .body_mut()
                .with_config()
                .limit(32 * 1024 * 1024)
                .read_to_string()
                .unwrap_or_default();
            Ok(HttpResponse { status, body })
        }
        Err(error) => Err(ProviderError::transport(format!(
            "Couldn't reach the provider: {error}"
        ))),
    }
}

/// Sends a request and reads its answer with `parse`.
pub(super) fn call<T>(
    request: HttpRequest,
    parse: impl FnOnce(&HttpResponse) -> Result<T, ProviderError>,
) -> Result<T, ProviderError> {
    parse(&send(&request)?)
}
