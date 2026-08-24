// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;
use futures::StreamExt as _;
use gpui::http_client::HttpRequestExt as _;
use gpui::{
    AnyElement, App, AppContext as _, Asset, FontWeight, Hsla, Image, ImageCacheError, ImageFormat,
    IntoElement, ParentElement as _, RenderImage, Styled as _, StyledImage as _, Window, div, img,
    prelude::FluentBuilder as _, px, rgb,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn browser_toolbar_background() -> Hsla {
    rgb(0x000000).into()
}

pub(crate) fn browser_toolbar_text_color() -> Hsla {
    rgb(0xf0f0f0).opacity(0.95).into()
}

pub(crate) fn browser_toolbar_security_icon_color() -> Hsla {
    rgb(0xc7c7c7).opacity(0.9).into()
}

pub(crate) fn browser_toolbar_button_icon_color() -> Hsla {
    browser_tab_action_icon_color()
}

pub(crate) fn browser_toolbar_disabled_icon_color() -> Hsla {
    rgb(0xcfcfcf).opacity(0.4).into()
}

pub(crate) fn browser_tab_bar_color() -> Hsla {
    rgb(0x050608).opacity(0.96).into()
}

pub(crate) fn browser_tab_active_color() -> Hsla {
    rgb(0xffffff).opacity(0.13).into()
}

pub(crate) fn browser_tab_inactive_color() -> Hsla {
    rgb(0xffffff).opacity(0.06).into()
}

pub(crate) fn browser_tab_action_cluster_color() -> Hsla {
    rgb(0x0e0e0e).into()
}

pub(crate) fn browser_tab_separator_color() -> Hsla {
    rgb(0x252525).into()
}

pub(crate) fn browser_tab_text_color(state: BrowserTabState, is_active: bool) -> Hsla {
    match (state, is_active) {
        (_, true) => rgb(0xf5f5f5).opacity(0.98).into(),
        (_, false) => rgb(0xc7c7c7).opacity(0.82).into(),
    }
}

pub(crate) fn browser_runtime_favicon_from_url(
    favicon_url: Option<&str>,
) -> (
    Option<String>,
    Option<BrowserFaviconImage>,
    Option<BrowserFaviconFetchSource>,
) {
    let Some(favicon_url) = favicon_url.map(str::trim).filter(|url| !url.is_empty()) else {
        return (None, None, None);
    };

    if let Some(image) = browser_favicon_image_from_data_url(favicon_url) {
        return (None, Some(image), None);
    }

    let Some((marker, source)) = browser_safe_http_favicon_parts(favicon_url) else {
        return (None, None, None);
    };

    (Some(marker), None, Some(source))
}

#[derive(Clone)]
pub(crate) enum BrowserFaviconHttpImageAsset {}

impl Asset for BrowserFaviconHttpImageAsset {
    type Source = BrowserFaviconFetchSource;
    type Output = std::result::Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        let client = cx.http_client();
        let svg_renderer = cx.svg_renderer();
        async move {
            use futures::AsyncReadExt as _;

            let request = gpui::http_client::Request::get(source.url.as_str())
                .follow_redirects(gpui::http_client::RedirectPolicy::FollowLimit(
                    BROWSER_FAVICON_HTTP_REDIRECT_LIMIT,
                ))
                .body(gpui::http_client::AsyncBody::default())
                .map_err(|_| {
                    browser_favicon_fetch_error(BrowserFaviconFetchError::InvalidRequest)
                })?;

            let mut response = client
                .send(request)
                .await
                .map_err(|_| browser_favicon_fetch_error(BrowserFaviconFetchError::Network))?;
            let status = response.status();
            if !status.is_success() {
                return Err(browser_favicon_fetch_error(
                    BrowserFaviconFetchError::BadStatus(status.as_u16()),
                ));
            }

            let content_type = response
                .headers()
                .get(gpui::http_client::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let mut body = Vec::new();
            response
                .body_mut()
                .take(BROWSER_FAVICON_IMAGE_MAX_BYTES as u64 + 1)
                .read_to_end(&mut body)
                .await
                .map_err(|_| browser_favicon_fetch_error(BrowserFaviconFetchError::BodyRead))?;
            if body.is_empty() || body.len() > BROWSER_FAVICON_IMAGE_MAX_BYTES {
                return Err(browser_favicon_fetch_error(
                    BrowserFaviconFetchError::TooLarge,
                ));
            }

            let format = browser_favicon_http_image_format(content_type.as_deref(), &body)
                .map_err(browser_favicon_fetch_error)?;
            browser_favicon_validate_encoded_dimensions(format, &body)
                .map_err(browser_favicon_fetch_error)?;

            let render_image = Image::from_bytes(format, body)
                .to_image_data(svg_renderer)
                .map_err(|_| browser_favicon_fetch_error(BrowserFaviconFetchError::Decode))?;
            browser_favicon_validate_render_image(&render_image)
                .map_err(browser_favicon_fetch_error)?;
            Ok(render_image)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserFaviconFetchError {
    InvalidRequest,
    Network,
    BadStatus(u16),
    BodyRead,
    TooLarge,
    UnsupportedContentType,
    UnsupportedFormat,
    OversizedDimensions,
    Decode,
}

impl fmt::Display for BrowserFaviconFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest => f.write_str("browser favicon request was invalid"),
            Self::Network => f.write_str("browser favicon fetch failed"),
            Self::BadStatus(status) => write!(f, "browser favicon returned HTTP status {status}"),
            Self::BodyRead => f.write_str("browser favicon body could not be read"),
            Self::TooLarge => f.write_str("browser favicon response exceeded the byte limit"),
            Self::UnsupportedContentType => {
                f.write_str("browser favicon content type is unsupported")
            }
            Self::UnsupportedFormat => f.write_str("browser favicon image format is unsupported"),
            Self::OversizedDimensions => {
                f.write_str("browser favicon image dimensions exceeded the decode limit")
            }
            Self::Decode => f.write_str("browser favicon image could not be decoded"),
        }
    }
}

impl std::error::Error for BrowserFaviconFetchError {}

pub(crate) fn browser_favicon_fetch_error(error: BrowserFaviconFetchError) -> ImageCacheError {
    ImageCacheError::Other(Arc::new(anyhow::Error::new(error)))
}

pub(crate) fn browser_favicon_http_image_format(
    content_type: Option<&str>,
    bytes: &[u8],
) -> std::result::Result<ImageFormat, BrowserFaviconFetchError> {
    let mime_format = content_type.and_then(browser_favicon_image_format_for_mime);
    let guessed_format = browser_favicon_guess_image_format(bytes);

    if let Some(format) = guessed_format {
        if content_type
            .map(browser_favicon_content_type_allows_sniffed_format)
            .unwrap_or(true)
        {
            return Ok(format);
        }
        return Err(BrowserFaviconFetchError::UnsupportedContentType);
    }

    if let Some(format) = mime_format {
        return Ok(format);
    }

    if content_type.is_some() {
        Err(BrowserFaviconFetchError::UnsupportedContentType)
    } else {
        Err(BrowserFaviconFetchError::UnsupportedFormat)
    }
}

pub(crate) fn browser_favicon_content_type_allows_sniffed_format(content_type: &str) -> bool {
    let mime_type = browser_favicon_canonical_mime_type(content_type);
    browser_favicon_image_format_for_mime(mime_type).is_some()
        || mime_type.eq_ignore_ascii_case("application/octet-stream")
        || mime_type.eq_ignore_ascii_case("binary/octet-stream")
        || mime_type.eq_ignore_ascii_case("application/ico")
}

pub(crate) fn browser_favicon_canonical_mime_type(content_type: &str) -> &str {
    content_type
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or_default()
}

pub(crate) fn browser_favicon_guess_image_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(&b"WEBP"[..]) {
        Some(ImageFormat::Webp)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if bytes.starts_with(b"BM") {
        Some(ImageFormat::Bmp)
    } else if bytes.get(0..4) == Some(&[0, 0, 1, 0][..]) {
        Some(ImageFormat::Ico)
    } else {
        None
    }
}

pub(crate) fn browser_favicon_validate_encoded_dimensions(
    format: ImageFormat,
    bytes: &[u8],
) -> std::result::Result<(), BrowserFaviconFetchError> {
    let Some(dimensions) = browser_favicon_encoded_dimensions(format, bytes) else {
        return Err(BrowserFaviconFetchError::UnsupportedFormat);
    };
    if browser_favicon_dimensions_within_limit(dimensions.0, dimensions.1) {
        Ok(())
    } else {
        Err(BrowserFaviconFetchError::OversizedDimensions)
    }
}

pub(crate) fn browser_favicon_encoded_dimensions(
    format: ImageFormat,
    bytes: &[u8],
) -> Option<(u32, u32)> {
    match format {
        ImageFormat::Png => browser_favicon_png_dimensions(bytes),
        ImageFormat::Jpeg => browser_favicon_jpeg_dimensions(bytes),
        ImageFormat::Webp => browser_favicon_webp_dimensions(bytes),
        ImageFormat::Gif => browser_favicon_gif_dimensions(bytes),
        ImageFormat::Bmp => browser_favicon_bmp_dimensions(bytes),
        ImageFormat::Ico => browser_favicon_ico_dimensions(bytes),
        ImageFormat::Svg | ImageFormat::Tiff | ImageFormat::Pnm => None,
    }
}

pub(crate) fn browser_favicon_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.get(12..16) != Some(&b"IHDR"[..])
    {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?),
        u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?),
    ))
}

pub(crate) fn browser_favicon_jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\xff\xd8") {
        return None;
    }

    let mut index = 2_usize;
    while index + 3 < bytes.len() {
        while bytes.get(index) == Some(&0xff) {
            index += 1;
        }
        let marker = *bytes.get(index)?;
        index += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }

        let length = usize::from(u16::from_be_bytes(
            bytes.get(index..index + 2)?.try_into().ok()?,
        ));
        if length < 2 || index + length > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if length < 7 {
                return None;
            }
            let height = u32::from(u16::from_be_bytes(
                bytes.get(index + 3..index + 5)?.try_into().ok()?,
            ));
            let width = u32::from(u16::from_be_bytes(
                bytes.get(index + 5..index + 7)?.try_into().ok()?,
            ));
            return Some((width, height));
        }
        index += length;
    }
    None
}

pub(crate) fn browser_favicon_gif_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 10 || !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return None;
    }
    Some((
        u32::from(u16::from_le_bytes(bytes.get(6..8)?.try_into().ok()?)),
        u32::from(u16::from_le_bytes(bytes.get(8..10)?.try_into().ok()?)),
    ))
}

pub(crate) fn browser_favicon_bmp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 26 || !bytes.starts_with(b"BM") {
        return None;
    }
    let dib_header_size = u32::from_le_bytes(bytes.get(14..18)?.try_into().ok()?);
    if dib_header_size == 12 {
        return Some((
            u32::from(u16::from_le_bytes(bytes.get(18..20)?.try_into().ok()?)),
            u32::from(u16::from_le_bytes(bytes.get(20..22)?.try_into().ok()?)),
        ));
    }
    let width = i32::from_le_bytes(bytes.get(18..22)?.try_into().ok()?);
    let height = i32::from_le_bytes(bytes.get(22..26)?.try_into().ok()?);
    Some((width.unsigned_abs(), height.unsigned_abs()))
}

pub(crate) fn browser_favicon_ico_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 6 || bytes.get(0..4) != Some(&[0, 0, 1, 0][..]) {
        return None;
    }
    let entry_count = usize::from(u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?));
    if entry_count == 0 || entry_count > BROWSER_FAVICON_IMAGE_MAX_FRAMES {
        return None;
    }
    let mut max_width = 0_u32;
    let mut max_height = 0_u32;
    for entry_index in 0..entry_count {
        let offset = 6 + entry_index * 16;
        let width = match *bytes.get(offset)? {
            0 => 256,
            value => u32::from(value),
        };
        let height = match *bytes.get(offset + 1)? {
            0 => 256,
            value => u32::from(value),
        };
        max_width = max_width.max(width);
        max_height = max_height.max(height);
    }
    Some((max_width, max_height))
}

pub(crate) fn browser_favicon_webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 20 || !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(&b"WEBP"[..]) {
        return None;
    }
    let chunk = bytes.get(12..16)?;
    if chunk == b"VP8X" {
        if bytes.len() < 30 {
            return None;
        }
        let width =
            1 + u32::from(bytes[24]) + (u32::from(bytes[25]) << 8) + (u32::from(bytes[26]) << 16);
        let height =
            1 + u32::from(bytes[27]) + (u32::from(bytes[28]) << 8) + (u32::from(bytes[29]) << 16);
        Some((width, height))
    } else if chunk == b"VP8L" {
        if bytes.len() < 25 || bytes[20] != 0x2f {
            return None;
        }
        let width = 1 + u32::from(bytes[21]) + ((u32::from(bytes[22] & 0x3f)) << 8);
        let height = 1
            + ((u32::from(bytes[22] & 0xc0)) >> 6)
            + (u32::from(bytes[23]) << 2)
            + ((u32::from(bytes[24] & 0x0f)) << 10);
        Some((width, height))
    } else if chunk == b"VP8 " {
        if bytes.len() < 30 || bytes.get(23..26) != Some(&[0x9d, 0x01, 0x2a][..]) {
            return None;
        }
        let width = u32::from(u16::from_le_bytes(bytes.get(26..28)?.try_into().ok()?) & 0x3fff);
        let height = u32::from(u16::from_le_bytes(bytes.get(28..30)?.try_into().ok()?) & 0x3fff);
        Some((width, height))
    } else {
        None
    }
}

pub(crate) fn browser_favicon_validate_render_image(
    render_image: &RenderImage,
) -> std::result::Result<(), BrowserFaviconFetchError> {
    let frame_count = render_image.frame_count();
    if frame_count == 0 || frame_count > BROWSER_FAVICON_IMAGE_MAX_FRAMES {
        return Err(BrowserFaviconFetchError::OversizedDimensions);
    }

    for frame_index in 0..frame_count {
        let size = render_image.size(frame_index);
        let width = u32::try_from(size.width.0)
            .ok()
            .filter(|width| *width > 0)
            .ok_or(BrowserFaviconFetchError::OversizedDimensions)?;
        let height = u32::try_from(size.height.0)
            .ok()
            .filter(|height| *height > 0)
            .ok_or(BrowserFaviconFetchError::OversizedDimensions)?;
        if !browser_favicon_dimensions_within_limit(width, height) {
            return Err(BrowserFaviconFetchError::OversizedDimensions);
        }
    }

    Ok(())
}

pub(crate) fn browser_favicon_dimensions_within_limit(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= BROWSER_FAVICON_IMAGE_MAX_DIMENSION
        && height <= BROWSER_FAVICON_IMAGE_MAX_DIMENSION
        && u64::from(width) * u64::from(height) <= BROWSER_FAVICON_IMAGE_MAX_PIXELS
}

pub(crate) fn browser_favicon_image_from_data_url(value: &str) -> Option<BrowserFaviconImage> {
    let value = value.trim();
    if value.len() > BROWSER_FAVICON_DATA_URL_MAX_CHARS || !browser_has_ascii_prefix(value, "data:")
    {
        return None;
    }

    let (metadata, payload) = value.get("data:".len()..)?.split_once(',')?;
    let (format, is_base64) = browser_favicon_data_url_metadata(metadata)?;
    let bytes = if is_base64 {
        let encoded = browser_favicon_percent_decode(payload, BROWSER_FAVICON_DATA_URL_MAX_CHARS)?;
        browser_favicon_decode_base64(&encoded)?
    } else {
        browser_favicon_percent_decode(payload, BROWSER_FAVICON_IMAGE_MAX_BYTES)?
    };

    if bytes.is_empty() || bytes.len() > BROWSER_FAVICON_IMAGE_MAX_BYTES {
        return None;
    }

    Some(BrowserFaviconImage {
        image: Arc::new(Image::from_bytes(format, bytes)),
    })
}

pub(crate) fn browser_favicon_data_url_metadata(metadata: &str) -> Option<(ImageFormat, bool)> {
    let mut parts = metadata.split(';');
    let mime_type = parts.next()?.trim();
    let format = browser_favicon_image_format_for_mime(mime_type)?;
    let is_base64 = parts.any(|part| part.trim().eq_ignore_ascii_case("base64"));
    Some((format, is_base64))
}

pub(crate) fn browser_favicon_image_format_for_mime(mime_type: &str) -> Option<ImageFormat> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/webp" => Some(ImageFormat::Webp),
        "image/gif" => Some(ImageFormat::Gif),
        "image/bmp" => Some(ImageFormat::Bmp),
        "image/ico" | "image/x-icon" | "image/vnd.microsoft.icon" => Some(ImageFormat::Ico),
        _ => None,
    }
}

pub(crate) fn browser_safe_http_favicon_parts(
    value: &str,
) -> Option<(String, BrowserFaviconFetchSource)> {
    let value = value.trim();
    if value.len() > BROWSER_FAVICON_HTTP_URL_MAX_CHARS
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || value.contains('\\')
    {
        return None;
    }

    let mut parsed = gpui::http_client::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.host_str().is_none()
    {
        return None;
    }
    parsed.set_fragment(None);
    let url = parsed.as_str();
    if url.len() > BROWSER_FAVICON_HTTP_URL_MAX_CHARS {
        return None;
    }

    let (scheme, remainder) = url.split_once("://")?;

    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .map(str::trim)
        .filter(|authority| !authority.is_empty())?;
    if authority.contains('@')
        || authority.contains('\\')
        || authority.bytes().any(|byte| !(0x21..=0x7e).contains(&byte))
    {
        return None;
    }

    let marker = format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    );
    Some((
        marker,
        BrowserFaviconFetchSource {
            url: url.to_string(),
        },
    ))
}

pub(crate) fn browser_has_ascii_prefix(value: &str, prefix: &str) -> bool {
    value
        .as_bytes()
        .get(..prefix.len())
        .is_some_and(|bytes| bytes.eq_ignore_ascii_case(prefix.as_bytes()))
}

pub(crate) fn browser_favicon_percent_decode(value: &str, max_bytes: usize) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len().min(max_bytes));
    let mut index = 0;
    while index < bytes.len() {
        if decoded.len() >= max_bytes {
            return None;
        }
        let byte = bytes[index];
        if byte == b'%' {
            let high = *bytes.get(index + 1)?;
            let low = *bytes.get(index + 2)?;
            let high = browser_hex_value(high)?;
            let low = browser_hex_value(low)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(byte);
            index += 1;
        }
    }
    Some(decoded)
}

pub(crate) fn browser_favicon_decode_base64(encoded: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity((encoded.len() / 4).saturating_mul(3));
    let mut quartet = [0_u8; 4];
    let mut quartet_len = 0_usize;
    let mut finished = false;

    for byte in encoded.iter().copied() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if finished {
            return None;
        }

        quartet[quartet_len] = if byte == b'=' {
            64
        } else {
            browser_base64_value(byte)?
        };
        quartet_len += 1;

        if quartet_len == quartet.len() {
            finished = browser_favicon_decode_base64_quartet(&quartet, &mut decoded)?;
            quartet_len = 0;
        }
    }

    match quartet_len {
        0 => Some(decoded),
        1 => None,
        2 => {
            if quartet[0] == 64 || quartet[1] == 64 {
                return None;
            }
            browser_favicon_push_decoded_byte(&mut decoded, (quartet[0] << 2) | (quartet[1] >> 4))?;
            Some(decoded)
        }
        3 => {
            if quartet[0] == 64 || quartet[1] == 64 || quartet[2] == 64 {
                return None;
            }
            browser_favicon_push_decoded_byte(&mut decoded, (quartet[0] << 2) | (quartet[1] >> 4))?;
            browser_favicon_push_decoded_byte(&mut decoded, (quartet[1] << 4) | (quartet[2] >> 2))?;
            Some(decoded)
        }
        _ => None,
    }
}

pub(crate) fn browser_favicon_decode_base64_quartet(
    quartet: &[u8; 4],
    decoded: &mut Vec<u8>,
) -> Option<bool> {
    if quartet[0] == 64 || quartet[1] == 64 || (quartet[2] == 64 && quartet[3] != 64) {
        return None;
    }

    browser_favicon_push_decoded_byte(decoded, (quartet[0] << 2) | (quartet[1] >> 4))?;
    if quartet[2] != 64 {
        browser_favicon_push_decoded_byte(decoded, (quartet[1] << 4) | (quartet[2] >> 2))?;
        if quartet[3] != 64 {
            browser_favicon_push_decoded_byte(decoded, (quartet[2] << 6) | quartet[3])?;
        }
    }

    Some(quartet[2] == 64 || quartet[3] == 64)
}

pub(crate) fn browser_favicon_push_decoded_byte(decoded: &mut Vec<u8>, byte: u8) -> Option<()> {
    if decoded.len() >= BROWSER_FAVICON_IMAGE_MAX_BYTES {
        return None;
    }
    decoded.push(byte);
    Some(())
}

pub(crate) fn browser_base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

pub(crate) fn browser_hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn browser_tab_icon_element(
    profile_id: BrowserProfileId,
    chrome_status: BrowserTabChromeStatus,
    runtime_favicon_url: Option<&str>,
    runtime_favicon_image: Option<&BrowserFaviconImage>,
    runtime_favicon_fetch: Option<&BrowserFaviconFetchSource>,
) -> AnyElement {
    /*
    CDXC:GPUIBrowserFavicons 2026-06-22-09:11:
    GPUI Browser tabs must visibly distinguish address-only placeholders, generic loaded pages, and loaded pages that reported favicon metadata. Use a deterministic native shell glyph/color derived from the runtime-only safe favicon marker whenever an actual favicon image is unavailable.

    CDXC:GPUIBrowserFavicons 2026-06-22-10:41:
    Render decoded runtime favicon images when CEF provides a safe capped data:image URL, but keep tab chrome as normal in-layout GPUI elements and fall back to the deterministic URL marker or generic dot when decoding is unavailable.

    CDXC:GPUIBrowserFavicons 2026-06-22-11:05:
    HTTP(S) favicon images must render through a favicon-only non-AssetLogger asset loader inside the existing tab icon slot. Loading, failed status, unsupported MIME/format, oversized bodies, oversized decode dimensions, and decode failures all show the safe marker or generic icon without logging raw URLs, response bodies, headers, cookies, tokens, paths, titles, command text, stdout/stderr, or user content.

    CDXC:GPUIBrowserTabs 2026-06-22-16:48:
    Restored loaded Browser placeholders have loaded shell state but no materialized CEF surface, so their tab chrome uses a restored teal status dot and suppresses runtime favicon image/fetch rendering until a live surface exists. Address-only tabs keep the neutral placeholder dot and cannot borrow stale page or favicon state.

    Non-default profiles replace the favicon slot with their stable generated profile number in a circular badge. The badge is tab-owned chrome, so mixed-profile tabs remain visible without adding overlays or another interactive hit region.
    */
    if let Some(profile_number) = profile_id.display_number() {
        return div()
            .flex()
            .flex_shrink_0()
            .size(px(BROWSER_TAB_ICON_SIZE))
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(rgb(0xffffff).opacity(0.42))
            .bg(rgb(0xffffff).opacity(0.12))
            .text_size(px(if profile_number < 10 { 9.0 } else { 7.0 }))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xffffff).opacity(0.9))
            .child(profile_number.to_string())
            .into_any_element();
    }

    let runtime_favicon_url =
        runtime_favicon_url.filter(|_| chrome_status.allows_runtime_favicon());
    let runtime_favicon_image =
        runtime_favicon_image.filter(|_| chrome_status.allows_runtime_favicon());
    let runtime_favicon_fetch =
        runtime_favicon_fetch.filter(|_| chrome_status.allows_runtime_favicon());
    let base = div()
        .relative()
        .flex()
        .flex_shrink_0()
        .size(px(BROWSER_TAB_ICON_SIZE))
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .border_1();

    if let Some(favicon_image) = runtime_favicon_image {
        let fallback_favicon_url = runtime_favicon_url.map(str::to_string);
        return base
            .overflow_hidden()
            .border_color(browser_tab_favicon_bitmap_border_color(runtime_favicon_url))
            .bg(browser_tab_favicon_bitmap_background_color(
                runtime_favicon_url,
            ))
            .child(
                img(favicon_image.image.clone())
                    .size(px(BROWSER_TAB_ICON_SIZE - 2.0))
                    .rounded(px(2.0))
                    .with_fallback(move || {
                        browser_tab_favicon_fallback_inner_element(
                            BrowserTabChromeStatus::LoadedSurface,
                            fallback_favicon_url.as_deref(),
                        )
                    }),
            )
            .into_any_element();
    }

    if let Some(favicon_fetch_source) = runtime_favicon_fetch {
        let favicon_fetch_source = favicon_fetch_source.clone();
        let fallback_favicon_url = runtime_favicon_url.map(str::to_string);
        let loading_fallback_favicon_url = fallback_favicon_url.clone();
        return base
            .overflow_hidden()
            .border_color(browser_tab_favicon_bitmap_border_color(runtime_favicon_url))
            .bg(browser_tab_favicon_bitmap_background_color(
                runtime_favicon_url,
            ))
            .child(
                img(move |window: &mut Window, cx: &mut App| {
                    window.use_asset::<BrowserFaviconHttpImageAsset>(&favicon_fetch_source, cx)
                })
                .size(px(BROWSER_TAB_ICON_SIZE - 2.0))
                .rounded(px(2.0))
                .with_loading(move || {
                    browser_tab_favicon_fallback_inner_element(
                        BrowserTabChromeStatus::LoadedSurface,
                        loading_fallback_favicon_url.as_deref(),
                    )
                })
                .with_fallback(move || {
                    browser_tab_favicon_fallback_inner_element(
                        BrowserTabChromeStatus::LoadedSurface,
                        fallback_favicon_url.as_deref(),
                    )
                }),
            )
            .into_any_element();
    }

    if let Some(favicon_url) = runtime_favicon_url {
        return base
            .border_color(browser_tab_favicon_icon_border_color(favicon_url))
            .bg(browser_tab_favicon_icon_background_color(favicon_url))
            .child(browser_tab_favicon_marker_inner_element(favicon_url))
            .into_any_element();
    }

    base.border_color(browser_tab_icon_border_color(chrome_status))
        .bg(browser_tab_icon_background_color(chrome_status))
        .child(browser_tab_generic_icon_inner_element(chrome_status))
        .into_any_element()
}

pub(crate) fn browser_tab_favicon_fallback_inner_element(
    chrome_status: BrowserTabChromeStatus,
    runtime_favicon_url: Option<&str>,
) -> AnyElement {
    if let Some(favicon_url) = runtime_favicon_url {
        return browser_tab_favicon_marker_inner_element(favicon_url);
    }
    browser_tab_generic_icon_inner_element(chrome_status)
}

pub(crate) fn browser_tab_favicon_marker_inner_element(favicon_url: &str) -> AnyElement {
    div()
        .flex()
        .size(px(9.0))
        .items_center()
        .justify_center()
        .rounded(px(2.0))
        .bg(browser_tab_favicon_icon_color(favicon_url))
        .text_size(px(7.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(browser_tab_favicon_glyph_color())
        .child("F")
        .into_any_element()
}

pub(crate) fn browser_tab_generic_icon_inner_element(
    chrome_status: BrowserTabChromeStatus,
) -> AnyElement {
    let is_placeholder = chrome_status == BrowserTabChromeStatus::AddressOnly;
    div()
        .size(px(if is_placeholder { 5.0 } else { 6.0 }))
        .rounded_full()
        .bg(browser_tab_icon_dot_color(chrome_status))
        .into_any_element()
}

pub(crate) fn browser_tab_favicon_bitmap_border_color(runtime_favicon_url: Option<&str>) -> Hsla {
    runtime_favicon_url
        .map(browser_tab_favicon_icon_border_color)
        .unwrap_or_else(|| rgb(0xffffff).opacity(0.24).into())
}

pub(crate) fn browser_tab_favicon_bitmap_background_color(
    runtime_favicon_url: Option<&str>,
) -> Hsla {
    runtime_favicon_url
        .map(browser_tab_favicon_icon_background_color)
        .unwrap_or_else(|| rgb(0xffffff).opacity(0.08).into())
}

pub(crate) fn browser_tab_icon_border_color(chrome_status: BrowserTabChromeStatus) -> Hsla {
    match chrome_status {
        BrowserTabChromeStatus::LoadedSurface | BrowserTabChromeStatus::RestoredPlaceholder => {
            rgb(browser_tab_chrome_status_color(chrome_status))
                .opacity(0.48)
                .into()
        }
        BrowserTabChromeStatus::AddressOnly => rgb(0xffffff).opacity(0.18).into(),
    }
}

pub(crate) fn browser_tab_icon_background_color(chrome_status: BrowserTabChromeStatus) -> Hsla {
    match chrome_status {
        BrowserTabChromeStatus::LoadedSurface | BrowserTabChromeStatus::RestoredPlaceholder => {
            rgb(browser_tab_chrome_status_color(chrome_status))
                .opacity(0.14)
                .into()
        }
        BrowserTabChromeStatus::AddressOnly => rgb(0xffffff).opacity(0.055).into(),
    }
}

pub(crate) fn browser_tab_icon_dot_color(chrome_status: BrowserTabChromeStatus) -> Hsla {
    rgb(browser_tab_chrome_status_color(chrome_status))
        .opacity(match chrome_status {
            BrowserTabChromeStatus::LoadedSurface => 0.82,
            BrowserTabChromeStatus::RestoredPlaceholder => 0.82,
            BrowserTabChromeStatus::AddressOnly => 0.36,
        })
        .into()
}

pub(crate) fn browser_tab_chrome_status_color(chrome_status: BrowserTabChromeStatus) -> u32 {
    match chrome_status {
        BrowserTabChromeStatus::LoadedSurface => 0x58b7ff,
        BrowserTabChromeStatus::RestoredPlaceholder => 0x41d7b5,
        BrowserTabChromeStatus::AddressOnly => 0xffffff,
    }
}

pub(crate) fn browser_tab_favicon_icon_color(url: &str) -> Hsla {
    rgb(browser_tab_favicon_palette_color(url))
        .opacity(0.92)
        .into()
}

pub(crate) fn browser_tab_favicon_icon_border_color(url: &str) -> Hsla {
    rgb(browser_tab_favicon_palette_color(url))
        .opacity(0.62)
        .into()
}

pub(crate) fn browser_tab_favicon_icon_background_color(url: &str) -> Hsla {
    rgb(browser_tab_favicon_palette_color(url))
        .opacity(0.16)
        .into()
}

pub(crate) fn browser_tab_favicon_glyph_color() -> Hsla {
    rgb(0x061014).opacity(0.86).into()
}

pub(crate) fn browser_tab_favicon_palette_color(url: &str) -> u32 {
    let hash = url.bytes().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u64::from(byte))
    });
    BROWSER_TAB_FAVICON_COLORS[hash as usize % BROWSER_TAB_FAVICON_COLORS.len()]
}

pub(crate) fn browser_tab_close_color() -> Hsla {
    rgb(0xcfcfcf).into()
}

pub(crate) fn browser_tab_close_background_color() -> Hsla {
    rgb(0x0e0e0e).into()
}

pub(crate) fn browser_tab_close_hover_color() -> Hsla {
    tab_bar_button_hover_color()
}

pub(crate) fn browser_tab_action_hover_color() -> Hsla {
    tab_bar_button_hover_color()
}

pub(crate) fn browser_tab_action_icon_color() -> Hsla {
    rgb(0xcfcfcf).into()
}

pub(crate) fn browser_find_count_label(find: &GpuiBrowserFindState) -> String {
    if find.query.trim().is_empty() {
        return String::new();
    }
    if find.match_count <= 0 {
        return find.final_update.then_some("N/A").unwrap_or("").to_string();
    }
    format!(
        "{}/{}",
        find.active_match_ordinal.clamp(1, find.match_count),
        find.match_count
    )
}

pub(crate) fn browser_split_separator_color() -> Hsla {
    rgb(0x333333).into()
}

pub(crate) fn browser_security_icon_path(url: &str) -> &'static str {
    if url.trim_start().to_lowercase().starts_with("https://") {
        BROWSER_ICON_LOCK_FILLED
    } else {
        BROWSER_ICON_WORLD
    }
}

pub(crate) fn browser_feedback_tool_unavailable_url(url: &str) -> bool {
    let Some(host) = browser_url_host(url) else {
        return false;
    };
    host == "github.com" || host.ends_with(".github.com")
}

pub(crate) fn browser_feedback_js_string_literal(value: &str) -> String {
    serde_json::to_string(value).expect("browser feedback constants must serialize as strings")
}

pub(crate) fn browser_agentation_feedback_injection_script() -> String {
    /*
    CDXC:GPUIBrowserFeedback 2026-06-23-11:04:
    Browser feedback toolbar parity now injects the Settings-selected Agentation tool into the active GPUI CEF main frame instead of showing a placeholder notification. Keep the script bounded to pinned module URLs, auto-start feedback mode, and avoid persistent logs, console page metadata, raw URLs, titles, page content, cookies, tokens, paths, command text, terminal content, or JS error payloads.
    */
    const TEMPLATE: &str = r##"
(function() {
  const packageModuleUrl = __AGENTATION_PACKAGE_MODULE_URL__;
  const reactModuleUrl = __AGENTATION_REACT_MODULE_URL__;
  const reactDOMClientModuleUrl = __AGENTATION_REACT_DOM_CLIENT_MODULE_URL__;
  const stateKey = '__GHOSTEX_AGENTATION__';
  const rootId = 'ghostex-agentation-root';
  const directionStyleId = 'ghostex-agentation-direction-style';
  const existing = window[stateKey];
  if (existing && typeof existing.unmount === 'function') {
    existing.unmount();
    return;
  }

  const state = {
    canceled: false,
    container: null,
    directionStyle: null,
    root: null,
    activated: false,
    failed: false,
    unmount: function() {
      this.canceled = true;
      if (this.root && typeof this.root.unmount === 'function') {
        try {
          this.root.unmount();
        } catch (_) {}
      }
      if (this.container && this.container.parentNode) {
        this.container.parentNode.removeChild(this.container);
      }
      if (this.directionStyle && this.directionStyle.parentNode) {
        this.directionStyle.parentNode.removeChild(this.directionStyle);
      }
      if (window[stateKey] === this) {
        delete window[stateKey];
      }
    }
  };
  window[stateKey] = state;

  const findStartButton = function() {
    const root = state.container || document.getElementById(rootId);
    return document.querySelector('[data-agentation-toolbar] [title="Start feedback mode"][role="button"]')
      || document.querySelector('[data-agentation-toolbar][title="Start feedback mode"][role="button"]')
      || document.querySelector('[title="Start feedback mode"][role="button"]')
      || (root && root.querySelector('[title="Start feedback mode"][role="button"]'))
      || document.querySelector('[data-agentation-toolbar][title="Start feedback mode"]')
      || document.querySelector('[title="Start feedback mode"]')
      || (root && root.querySelector('[title="Start feedback mode"]'));
  };

  const autoActivate = function(attempt) {
    if (state.canceled) {
      return;
    }
    const startButton = findStartButton();
    if (startButton && typeof startButton.click === 'function') {
      startButton.click();
      state.activated = true;
      return;
    }
    if (attempt < 20) {
      window.setTimeout(function() {
        autoActivate(attempt + 1);
      }, 50);
    }
  };

  const scheduleAutoActivate = function() {
    const run = function() {
      autoActivate(0);
    };
    if (typeof window.requestAnimationFrame === 'function') {
      window.requestAnimationFrame(function() {
        window.requestAnimationFrame(run);
      });
    } else {
      window.setTimeout(run, 0);
    }
  };

  const mount = async function() {
    const modules = await Promise.all([
      import(reactModuleUrl),
      import(reactDOMClientModuleUrl),
      import(packageModuleUrl)
    ]);
    if (state.canceled) {
      return;
    }
    const React = modules[0].default || modules[0];
    const ReactDOMClient = modules[1];
    const Agentation = modules[2].Agentation;
    if (!React || typeof React.createElement !== 'function' || !ReactDOMClient.createRoot || !Agentation) {
      state.failed = true;
      state.unmount();
      return;
    }

    const staleContainer = document.getElementById(rootId);
    if (staleContainer && staleContainer.parentNode) {
      staleContainer.parentNode.removeChild(staleContainer);
    }
    const staleDirectionStyle = document.getElementById(directionStyleId);
    if (staleDirectionStyle && staleDirectionStyle.parentNode) {
      staleDirectionStyle.parentNode.removeChild(staleDirectionStyle);
    }
    // Agentation portals its visible UI into document.body, outside this
    // mount container. Give that portal an explicit writing-mode boundary so
    // RTL page content cannot reverse Agentation's own controls.
    const directionStyle = document.createElement('style');
    directionStyle.id = directionStyleId;
    directionStyle.textContent = '[data-agentation-root][data-agentation-theme] { direction: ltr !important; text-align: left !important; }';
    (document.head || document.documentElement).appendChild(directionStyle);
    const container = document.createElement('div');
    container.id = rootId;
    container.setAttribute('data-agentation-root', 'true');
    (document.body || document.documentElement).appendChild(container);

    state.container = container;
    state.directionStyle = directionStyle;
    state.root = ReactDOMClient.createRoot(container);
    state.root.render(React.createElement(Agentation));
    scheduleAutoActivate();
  };

  const start = function() {
    mount().catch(function() {
      state.failed = true;
      state.unmount();
    });
  };

  if (document.body || document.readyState !== 'loading') {
    start();
  } else {
    window.addEventListener('DOMContentLoaded', start, { once: true });
  }
})();
"##;

    TEMPLATE
        .replace(
            "__AGENTATION_PACKAGE_MODULE_URL__",
            &browser_feedback_js_string_literal(BROWSER_FEEDBACK_AGENTATION_PACKAGE_MODULE_URL),
        )
        .replace(
            "__AGENTATION_REACT_MODULE_URL__",
            &browser_feedback_js_string_literal(BROWSER_FEEDBACK_AGENTATION_REACT_MODULE_URL),
        )
        .replace(
            "__AGENTATION_REACT_DOM_CLIENT_MODULE_URL__",
            &browser_feedback_js_string_literal(
                BROWSER_FEEDBACK_AGENTATION_REACT_DOM_CLIENT_MODULE_URL,
            ),
        )
}

pub(crate) fn browser_url_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let after_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let without_userinfo = after_scheme.rsplit('@').next().unwrap_or(after_scheme);
    let authority_end = without_userinfo
        .find(['/', '?', '#'])
        .unwrap_or(without_userinfo.len());
    let authority = &without_userinfo[..authority_end];
    if authority.is_empty() {
        return None;
    }
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split_once(']').map(|(host, _)| host).unwrap_or("")
    } else {
        authority
            .split_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority)
    };
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn browser_tab_title_for_url(url: &str) -> String {
    if let Some(host) = browser_url_host(url) {
        return host;
    }

    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == BROWSER_ADDRESS_ONLY_CEF_URL {
        "New Tab".to_string()
    } else {
        trimmed.to_string()
    }
}

/*
CDXC:GPUIBrowserTabTitleCache 2026-07-12:
The cached title persisted into browser shell state is the user-visible tab
label only: trimmed, bounded, and never a URL/query/credential field. It exists
so restart keeps showing the last displayed title instead of the URL-host
fallback.
*/
pub(crate) const BROWSER_TAB_CACHED_TITLE_MAX_CHARS: usize = 256;

pub(crate) fn sanitize_browser_tab_cached_title(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .take(BROWSER_TAB_CACHED_TITLE_MAX_CHARS)
            .collect(),
    )
}

pub(crate) fn browser_shell_default_url(project_path: Option<&Path>) -> String {
    project_path
        .and_then(browser_repository_remote_web_url)
        .unwrap_or_else(|| DEFAULT_BROWSER_URL.to_string())
}

pub(crate) fn browser_url_origin_key(url: &str) -> Option<String> {
    // Mirrors the macOS sidebar's `browserUrlOriginKey`: a lowercased
    // scheme+host (host keeps its port, drops userinfo) or None for
    // unparseable/hostless URLs so they can never match each other.
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    if scheme.is_empty() || host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}").to_ascii_lowercase())
}

pub(crate) fn encode_search_query(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }

    encoded
}

pub(crate) fn gpui_browser_tabs_project_key_allowed(value: &str) -> bool {
    gpui_remote_sidebar_project_id_allowed(value)
        || gpui_remote_project_reference_from_project_id(value).is_some()
}

pub(crate) fn gpui_open_existing_project_pull_request_in_browser(
    project_id: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUISidebarGit 2026-06-24-15:43:
    Existing PR browser opens must derive the URL from gxserver's current GitHub state for the supplied project id. Renderer-provided URLs, DOM text, browser titles, cached labels, and arbitrary payload fields are not accepted as launch authority.
    */
    let result = gpui_gxserver_rpc_result(
        "/api/runGitHubAction",
        &serde_json::json!({
            "action": "prView",
            "projectId": project_id,
        }),
        Duration::from_secs(10),
    )?;
    if gpui_typed_operation_exit_code(&result) != Some(0) {
        return Err("No open pull request is available for this project.".to_string());
    }
    let url = gpui_trusted_github_pull_request_url_from_pr_view_stdout(
        gpui_typed_operation_stdout(&result),
    )
    .ok_or_else(|| "No open pull request is available for this project.".to_string())?;
    gpui_spawn_os_open(std::ffi::OsStr::new(&url))
        .map_err(|_| "GPUI could not open the pull request.".to_string())
}

pub(crate) fn gpui_trusted_github_pull_request_url_from_pr_view_stdout(
    stdout: &str,
) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(stdout.trim()).ok()?;
    let object = value.as_object()?;
    if !json_string_field(object, "state")?.eq_ignore_ascii_case("open") {
        return None;
    }
    let candidate = json_string_field(object, "url")?.trim();
    if candidate.is_empty()
        || candidate.chars().count() > 2048
        || candidate.contains('\\')
        || candidate
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return None;
    }
    let parsed = gpui::http_client::Url::parse(candidate).ok()?;
    if parsed.scheme() != "https"
        || !parsed
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let segments = parsed.path_segments()?.collect::<Vec<_>>();
    if segments.len() != 4
        || segments[0].is_empty()
        || segments[1].is_empty()
        || segments[2] != "pull"
        || segments[3].is_empty()
        || !segments[3].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(parsed.as_str().to_string())
}

pub(crate) fn gpui_browser_media_permission_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-browser-media-permissions.json")
}

/// Browser microphone/camera answers live in GPUI-owned state rather than a
/// CEF profile: Alloy-style CEF never persists media content settings, and the
/// shell (not Chromium) owns the prompt that produced them.
pub(crate) fn load_gpui_browser_media_permission_decisions() -> GpuiBrowserMediaPermissionDecisions
{
    let Some(value) = fs::read_to_string(gpui_browser_media_permission_state_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return GpuiBrowserMediaPermissionDecisions::default();
    };
    let Some(origins) = value.get("origins").and_then(serde_json::Value::as_object) else {
        return GpuiBrowserMediaPermissionDecisions::default();
    };
    let read_decision = |value: &serde_json::Value, key: &str| match value
        .get(key)
        .and_then(serde_json::Value::as_str)
    {
        Some("allow") => Some(true),
        Some("block") => Some(false),
        _ => None,
    };
    GpuiBrowserMediaPermissionDecisions {
        origins: origins
            .iter()
            .filter_map(|(key, value)| {
                let decision = GpuiBrowserMediaPermissionDecision {
                    microphone: read_decision(value, "microphone"),
                    camera: read_decision(value, "camera"),
                };
                (!decision.is_empty()).then(|| (key.clone(), decision))
            })
            .collect(),
    }
}

pub(crate) fn persist_gpui_browser_media_permission_decisions(
    decisions: &GpuiBrowserMediaPermissionDecisions,
) {
    let path = gpui_browser_media_permission_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let write_decision = |decision: Option<bool>| match decision {
        Some(true) => Some(serde_json::Value::from("allow")),
        Some(false) => Some(serde_json::Value::from("block")),
        None => None,
    };
    let mut origins = serde_json::Map::new();
    for (key, decision) in &decisions.origins {
        if decision.is_empty() {
            continue;
        }
        let mut entry = serde_json::Map::new();
        if let Some(value) = write_decision(decision.microphone) {
            entry.insert("microphone".to_string(), value);
        }
        if let Some(value) = write_decision(decision.camera) {
            entry.insert("camera".to_string(), value);
        }
        origins.insert(key.clone(), serde_json::Value::Object(entry));
    }
    let payload = serde_json::json!({ "origins": origins });
    let _ = fs::write(path, payload.to_string());
}
