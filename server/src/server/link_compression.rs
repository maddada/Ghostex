//! Compression for the phone's link: deflated event socket frames and gzipped HTTP answers.
//!
//! CDXC:StateSync 2026-11-12 DECISION:
//! The user chose compression the phone can undo in JavaScript, so it ships over the air, negotiated so nothing changes for older phones or computers ("New behaviour only when both sides announce a capability"); SSH-level compression needs native builds and is only proposed. Nothing on the phone's link was compressed: on a test computer the sessions list compressed 13.6x, a working agent's chat stream 3.5x and the 79 KiB model catalog 7x. `ghostex server endpoint` lists `linkCompression`, and only a client that saw it asks:
//! - An `/api/events` socket opened with `encoding=deflate` gets every frame as a binary message: the frame's text through one raw deflate stream per socket, flushed after each frame (permessage-deflate's framing, kept in the app so iOS needs no WebSocket extension). The phone inflates it with its own window (apps/mobile/app/src/remote/inflate.ts).
//! - An HTTP request with [`COMPRESS_HEADER`]`: gzip` and `Accept-Encoding: gzip` gets a JSON answer of at least [`MIN_GZIP_BYTES`] gzipped with `Content-Encoding: gzip`, which the phone's native HTTP stacks undo before JavaScript sees it. The header keeps every client that sends `Accept-Encoding: gzip` on its own (old phones, the CLI's ureq) on today's bytes.
//! SEE-ALSO: `GXSERVER_ENDPOINT_CAPABILITY_LINK_COMPRESSION` in packages/gx-protocol/src/lib.rs, apps/mobile/app/src/chat/rust/machine-link.ts, apps/mobile/app/src/inventory/summaryStream.ts.

use std::io::Write;

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Response};
use flate2::write::{DeflateEncoder, GzEncoder};
use flate2::Compression;

/// The request header a client sends to ask for gzipped answers (with `Accept-Encoding: gzip`).
pub(crate) const COMPRESS_HEADER: &str = "x-gxserver-compress";
/// Smaller answers are sent as they are: gzip's header and the work cost more than they save.
pub(crate) const MIN_GZIP_BYTES: usize = 1024;
/// Larger answers (a big image) are sent as they are rather than held in memory twice.
const MAX_GZIP_BYTES: usize = 32 * 1024 * 1024;

/// One socket's deflate stream: each frame comes out as the bytes that decode to exactly it, given the
/// frames before it.
pub(crate) struct FrameDeflater {
    encoder: DeflateEncoder<Vec<u8>>,
}

impl FrameDeflater {
    pub(crate) fn new() -> Self {
        Self {
            encoder: DeflateEncoder::new(Vec::new(), Compression::new(6)),
        }
    }

    /// The deflated bytes of `text`, ending at a sync flush (`00 00 ff ff`), so the receiver can inflate it
    /// now. None only if the in-memory encoder failed, which leaves the stream unusable.
    pub(crate) fn frame(&mut self, text: &str) -> Option<Vec<u8>> {
        self.encoder.write_all(text.as_bytes()).ok()?;
        self.encoder.flush().ok()?;
        Some(std::mem::take(self.encoder.get_mut()))
    }
}

fn header_has(headers: &HeaderMap, name: impl header::AsHeaderName, token: &str) -> bool {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|part| {
            part.split(';')
                .next()
                .is_some_and(|coding| coding.trim().eq_ignore_ascii_case(token))
        })
}

/// Whether the request asked for gzipped answers the Ghostex way.
pub(crate) fn wants_gzip(request: &HeaderMap) -> bool {
    header_has(request, COMPRESS_HEADER, "gzip")
        && header_has(request, header::ACCEPT_ENCODING, "gzip")
}

/// `response` gzipped when the request asked for it and the answer is JSON of a known size worth it.
pub(crate) async fn gzip_answer(wanted: bool, response: Response<Body>) -> Response<Body> {
    if !wanted || response.headers().contains_key(header::CONTENT_ENCODING) {
        return response;
    }
    let json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    let size = axum::body::HttpBody::size_hint(response.body()).exact();
    let Some(size) =
        size.filter(|size| json && (MIN_GZIP_BYTES as u64..=MAX_GZIP_BYTES as u64).contains(size))
    else {
        return response;
    };
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, size as usize).await else {
        // The body could not be read again; nothing usable is left to send.
        return Response::from_parts(parts, Body::empty());
    };
    let mut encoder = GzEncoder::new(Vec::with_capacity(bytes.len() / 4), Compression::new(5));
    let gzipped = encoder.write_all(&bytes).and_then(|()| encoder.finish());
    let Ok(gzipped) = gzipped else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    parts
        .headers
        .insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    parts
        .headers
        .insert(header::CONTENT_LENGTH, HeaderValue::from(gzipped.len()));
    parts
        .headers
        .append(header::VARY, HeaderValue::from_static("accept-encoding"));
    Response::from_parts(parts, Body::from(gzipped))
}
