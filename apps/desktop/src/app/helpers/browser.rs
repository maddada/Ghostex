// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{fmt, fs, io::Read, path::PathBuf, sync::Arc, time::Duration};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use gpui::{
    AnyElement, App, Asset, FontWeight, Hsla, Image, ImageCacheError, ImageFormat, IntoElement,
    ParentElement as _, RenderImage, Styled as _, StyledImage as _, Window, div, img, px, svg,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn browser_toolbar_background() -> Hsla {
    chrome_color(0x000000, 0xffffff).into()
}

pub(crate) fn browser_toolbar_text_color() -> Hsla {
    chrome_color(0xf0f0f0, 0x262626).opacity(0.95).into()
}

pub(crate) fn browser_toolbar_security_icon_color() -> Hsla {
    chrome_color(0xc7c7c7, 0x525252).opacity(0.9).into()
}

pub(crate) fn browser_toolbar_button_icon_color() -> Hsla {
    browser_tab_action_icon_color()
}

pub(crate) fn browser_toolbar_disabled_icon_color() -> Hsla {
    chrome_color(0xcfcfcf, 0x404040).opacity(0.4).into()
}

pub(crate) fn browser_tab_action_cluster_color() -> Hsla {
    chrome_color(0x0e0e0e, 0xfafafa).into()
}

pub(crate) fn browser_tab_separator_color() -> Hsla {
    glass_divider(chrome_color(0x252525, 0xd4d4d4).into())
}

pub(crate) fn browser_tab_text_color(state: BrowserTabState, is_active: bool) -> Hsla {
    match (state, is_active) {
        (_, true) => chrome_color(0xf5f5f5, 0x262626).opacity(0.98).into(),
        (_, false) => chrome_color(0xc7c7c7, 0x525252).opacity(0.82).into(),
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

/// The favicon's content type and bytes, capped at `BROWSER_FAVICON_IMAGE_MAX_BYTES`. Blocking: call
/// it off the main thread.
fn browser_favicon_fetch_bytes(
    url: &str,
) -> std::result::Result<(Option<String>, Vec<u8>), BrowserFaviconFetchError> {
    let tls_config = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(BROWSER_FAVICON_HTTP_REDIRECT_LIMIT)
        .http_status_as_error(false)
        .tls_config(tls_config)
        .build();
    let mut response = ureq::Agent::new_with_config(config)
        .get(url)
        .call()
        .map_err(|_| BrowserFaviconFetchError::Network)?;
    let status = response.status();
    if !status.is_success() {
        return Err(BrowserFaviconFetchError::BadStatus(status.as_u16()));
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(BROWSER_FAVICON_IMAGE_MAX_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|_| BrowserFaviconFetchError::BodyRead)?;
    if body.is_empty() || body.len() > BROWSER_FAVICON_IMAGE_MAX_BYTES {
        return Err(BrowserFaviconFetchError::TooLarge);
    }
    Ok((content_type, body))
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
        /*
        CDXC:Browser 2026-09-21 WHY:
        The app never installs a GPUI HTTP client, so `cx.http_client()` is GPUI's null client and
        every favicon request failed before it left the process: tabs only ever showed the "F"
        marker. The fetch runs on the in-process HTTPS client the component store already uses, on
        a background thread, with the same redirect, byte and decode limits as before.
        */
        let executor = cx.background_executor().clone();
        let svg_renderer = cx.svg_renderer();
        async move {
            let url = source.url.clone();
            let (content_type, body) = executor
                .spawn(async move { browser_favicon_fetch_bytes(url.as_str()) })
                .await
                .map_err(browser_favicon_fetch_error)?;

            let format = browser_favicon_http_image_format(content_type.as_deref(), &body)
                .map_err(browser_favicon_fetch_error)?;
            browser_favicon_validate_encoded_dimensions(format, &body)
                .map_err(browser_favicon_fetch_error)?;

            let cached_body = body.clone();
            let render_image = Image::from_bytes(format, body)
                .to_image_data(svg_renderer)
                .map_err(|_| browser_favicon_fetch_error(BrowserFaviconFetchError::Decode))?;
            browser_favicon_validate_render_image(&render_image)
                .map_err(browser_favicon_fetch_error)?;
            if let Some(cache_key) = source.cache_key.as_deref() {
                browser_favicon_cache_store(cache_key, format, &cached_body);
            }
            Ok(render_image)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserFaviconFetchError {
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
            cache_key: None,
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
    runtime_favicon_image: Option<&BrowserFaviconImage>,
    runtime_favicon_fetch: Option<&BrowserFaviconFetchSource>,
) -> AnyElement {
    /*
    CDXC:Browser 2026-09-21 DECISION:
    User: "please stop putting a bg behind the favicon. I want the default icon if we don't have a favicon to be the default browser icon and then we replace it with the actual site's favicon". The slot is a bare image with no tile, border or tint. Supersedes the 2026-06-22 status dots and coloured "F" marker: loading, failed, unsupported or oversized favicons, restored tabs and address-only tabs all show the globe (or the icon cached for the site) instead. HTTP(S) favicons still load through the favicon-only asset loader with its byte, redirect, format and dimension caps, and nothing about a failed fetch is logged.

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
            .border_color(chrome_ink().opacity(0.42))
            .bg(chrome_ink().opacity(0.12))
            .text_size(px(if profile_number < 10 { 9.0 } else { 7.0 }))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(chrome_ink().opacity(0.9))
            .child(profile_number.to_string())
            .into_any_element();
    }

    let runtime_favicon_image =
        runtime_favicon_image.filter(|_| chrome_status.allows_runtime_favicon());
    let runtime_favicon_fetch =
        runtime_favicon_fetch.filter(|_| chrome_status.allows_runtime_favicon());
    browser_favicon_element(
        BROWSER_TAB_ICON_SIZE,
        runtime_favicon_image,
        runtime_favicon_fetch,
    )
}

/// A page's favicon at `size`: the live fetch when there is one, else the icon cached for the site, else the default browser icon.
pub(crate) fn browser_favicon_element(
    size: f32,
    runtime_favicon_image: Option<&BrowserFaviconImage>,
    runtime_favicon_fetch: Option<&BrowserFaviconFetchSource>,
) -> AnyElement {
    let base = div()
        .flex()
        .flex_shrink_0()
        .size(px(size))
        .items_center()
        .justify_center();

    if let Some(favicon_fetch_source) = runtime_favicon_fetch {
        let favicon_fetch_source = favicon_fetch_source.clone();
        let loading_favicon_image = runtime_favicon_image.cloned();
        let fallback_favicon_image = loading_favicon_image.clone();
        return base
            .child(
                img(move |window: &mut Window, cx: &mut App| {
                    window.use_asset::<BrowserFaviconHttpImageAsset>(&favicon_fetch_source, cx)
                })
                .size(px(size))
                .with_loading(move || {
                    browser_tab_favicon_pending_element(size, loading_favicon_image.as_ref())
                })
                .with_fallback(move || {
                    browser_tab_favicon_pending_element(size, fallback_favicon_image.as_ref())
                }),
            )
            .into_any_element();
    }

    base.child(browser_tab_favicon_pending_element(
        size,
        runtime_favicon_image,
    ))
    .into_any_element()
}

/// What the icon slot shows while the page's own favicon is not drawable: the icon cached for the site, else the default browser icon.
pub(crate) fn browser_tab_favicon_pending_element(
    size: f32,
    cached_favicon_image: Option<&BrowserFaviconImage>,
) -> AnyElement {
    let Some(favicon_image) = cached_favicon_image else {
        return browser_tab_default_icon_element(size);
    };
    img(favicon_image.image.clone())
        .size(px(size))
        .with_fallback(move || browser_tab_default_icon_element(size))
        .into_any_element()
}

pub(crate) fn browser_tab_default_icon_element(size: f32) -> AnyElement {
    svg()
        .path(BROWSER_ICON_WORLD)
        .size(px(size))
        .text_color(chrome_color(0xc7c7c7, 0x525252))
        .into_any_element()
}

pub(crate) fn browser_tab_action_icon_color() -> Hsla {
    chrome_color(0xcfcfcf, 0x404040).into()
}

pub(crate) fn browser_find_count_label(find: &GpuiBrowserFindState) -> String {
    if find.query.trim().is_empty() {
        return String::new();
    }
    if find.match_count <= 0 {
        return if find.final_update { "N/A" } else { "" }.to_string();
    }
    format!(
        "{}/{}",
        find.active_match_ordinal.clamp(1, find.match_count),
        find.match_count
    )
}

pub(crate) fn browser_split_separator_color() -> Hsla {
    chrome_color(0x333333, 0xd4d4d4).into()
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

/// The Annotate script's Agentation: Ghostex's fork, bundled (see `cargo xtask build-agentation`).
const ANNOTATE_AGENTATION_BUNDLE: &str =
    include_str!("../../../vendor/agentation/agentation.generated.js");

/// What a screenshot reference in Annotate's Send markdown points at until the app has saved the
/// picture: `ghostex-annotation-screenshot:<annotation id>` (`app/page_annotations.rs`).
pub(crate) const ANNOTATE_SCREENSHOT_REFERENCE_PREFIX: &str = "ghostex-annotation-screenshot:";

/// Stamped into every Annotate script this run and checked on what comes back over the page
/// bridge, so a page's own posts to the bridge are not taken for annotations by mistake.
pub(crate) fn browser_annotate_token() -> &'static str {
    static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TOKEN.get_or_init(|| uuid::Uuid::new_v4().simple().to_string())
}

pub(crate) fn browser_agentation_feedback_injection_script() -> String {
    /*
    CDXC:Browser 2026-09-16 DECISION:
    User: Annotate must land in the frame that owns the page content. Agentation picks elements with elementFromPoint on its own document, so a toolbar mounted in the Storybook manager could only select the sidebar and header while the story sat in the preview iframe. The script therefore runs itself, via Function.prototype.toString, inside the largest same-origin iframe that covers at least a third of the viewport, toggles whichever realm already hosts it, and stays in the host page when there is no such frame or the frame's document cannot run an injected script. Supersedes the 2026-06-23 main-frame-only injection. Keep the script bounded to the bundled Agentation, auto-start feedback mode, and avoid persistent logs, console page metadata, raw URLs, titles, page content, cookies, tokens, paths, command text, terminal content, or JS error payloads.
    User: "i want clearing when i click copy to be enabled by default in agentation". Agentation's "Clear on copy/send" setting is turned on once per origin through its stored settings; a user who turns it off afterwards is not overridden.
    User (2026-11-07, after a Discord tester said Annotate attaches no screenshot and the copying is manual): "for g pls just fork agentation and add screenshot capturing but keep same tool and bundle it". The script runs Ghostex's fork of Agentation, bundled into the app (`ANNOTATE_AGENTATION_BUNDLE`, built by `cargo xtask build-agentation` from .dependencies/agentation) instead of importing agentation@3.1.2 from esm.sh, and each annotation carries a picture of what was picked. When the page has Ghostex's page bridge (Browser tabs, Storybook), Agentation's own Send button is turned on: it posts the markdown and the pictures to the app, which attaches them to the agent session the Docs review would send to (`app/page_annotations.rs`). Copy keeps working as before.
    SEE-ALSO: app/page_annotations.rs (receives Send), packages/client-storage/catalog.ts (agentationToolbar, agentationAnnotations), apps/desktop/views/manage/preview/html-viewer.tsx (same default for Docs HTML files), tooling/client-storage/check.mjs (externalStoreScripts).
    */
    const TEMPLATE: &str = r##"
(function() {
  const stateKey = '__GHOSTEX_AGENTATION__';
  const mount = function() {
    const annotateToken = __ANNOTATE_TOKEN__;
    const screenshotPrefix = __ANNOTATE_SCREENSHOT_PREFIX__;
    const stateKey = '__GHOSTEX_AGENTATION__';
    const rootId = 'ghostex-agentation-root';
    const directionStyleId = 'ghostex-agentation-direction-style';
    const settingsKey = 'feedback-toolbar-settings';
    const settingsStampKey = 'ghostexDefaults';
    const settingsStamp = 1;
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
      // Only the collapsed launcher matches, so a click can never close an
      // active toolbar (see CDXC:Docs 2026-10-23 in html-viewer.tsx).
      const host = document.querySelector('agentation-toolbar');
      return host && host.shadowRoot
        ? host.shadowRoot.querySelector('[data-agentation-toolbar] button[aria-expanded="false"][aria-label="Start feedback mode"]')
        : null;
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

    // The app reads the page bridge of the top document only (an iframe gets
    // its own unread copy), so a frame posts to its top document when it can.
    const pageBridge = function() {
      try {
        if (window.top !== window && typeof window.top.__ghostexPageBridge === 'function') {
          return window.top.__ghostexPageBridge;
        }
      } catch (_) {}
      return typeof window.__ghostexPageBridge === 'function' ? window.__ghostexPageBridge : null;
    };

    // Browser tabs and Storybook always have the bridge, though Storybook's
    // can arrive just after this script; Send asks for it when pressed and
    // fails (keeping the notes) when nothing answers.
    const agentationProps = function() {
      const props = {};
      props.screenshotReference = function(annotation, index) {
        return '[Image #' + index + '](' + screenshotPrefix + annotation.id + ')';
      };
      props.onSubmit = function(output, annotations) {
        const bridge = pageBridge();
        if (!bridge) {
          throw new Error('Ghostex is not listening on this page.');
        }
        const screenshots = [];
        for (let index = 0; index < annotations.length; index++) {
          const annotation = annotations[index];
          if (annotation.screenshot && output.indexOf(screenshotPrefix + annotation.id + ')') !== -1) {
            screenshots.push({ id: String(annotation.id), png: annotation.screenshot });
          }
        }
        bridge(JSON.stringify({
          type: 'annotate',
          token: annotateToken,
          markdown: output,
          screenshots: screenshots
        }));
      };
      return props;
    };

    const mount = function() {
      const lib = (function() {
__AGENTATION_BUNDLE__
;
        return __ghostexAgentation;
      })();
      if (state.canceled) {
        return;
      }
      if (!lib || typeof lib.createElement !== 'function' || typeof lib.createRoot !== 'function' || !lib.Agentation) {
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
      // Agentation reads its settings from this key once, on mount, and merges
      // them over its own defaults; it exposes no prop for them. Ghostex turns
      // "Clear on copy/send" on once per origin and keeps the stamp inside the
      // stored object, which Agentation preserves, so a later manual toggle
      // stays in force.
      let settings = null;
      try {
        settings = JSON.parse(localStorage.getItem(settingsKey) || 'null');
      } catch (_) {
        settings = null;
      }
      if (!settings || typeof settings !== 'object') {
        settings = {};
      }
      if (settings[settingsStampKey] !== settingsStamp) {
        settings.autoClearAfterCopy = true;
        settings[settingsStampKey] = settingsStamp;
        localStorage.setItem(settingsKey, JSON.stringify(settings));
      }
      state.root = lib.createRoot(container);
      state.root.render(lib.createElement(lib.Agentation, agentationProps()));
      scheduleAutoActivate();
    };

    const start = function() {
      try {
        mount();
      } catch (_) {
        state.failed = true;
        state.unmount();
      }
    };

    if (document.body || document.readyState !== 'loading') {
      start();
    } else {
      window.addEventListener('DOMContentLoaded', start, { once: true });
    }
  };
  const existing = window[stateKey];
  if (existing && typeof existing.unmount === 'function') {
    existing.unmount();
    return;
  }
  const contentFrame = function() {
    const viewportArea = Math.max(1, window.innerWidth * window.innerHeight);
    const frames = document.querySelectorAll('iframe');
    let best = null;
    let bestArea = 0;
    for (let index = 0; index < frames.length; index++) {
      const frame = frames[index];
      let frameDocument = null;
      try {
        frameDocument = frame.contentDocument;
      } catch (_) {
        frameDocument = null;
      }
      if (!frameDocument || !frameDocument.body) {
        continue;
      }
      const rect = frame.getBoundingClientRect();
      const area = Math.max(0, rect.width) * Math.max(0, rect.height);
      if (area > bestArea) {
        best = frame;
        bestArea = area;
      }
    }
    return best && bestArea * 3 >= viewportArea ? best : null;
  };
  const frame = contentFrame();
  if (frame) {
    const frameWindow = frame.contentWindow;
    const frameDocument = frame.contentDocument;
    const framed = frameWindow && frameWindow[stateKey];
    if (framed && typeof framed.unmount === 'function') {
      framed.unmount();
      return;
    }
    const script = frameDocument.createElement('script');
    script.textContent = '(' + mount.toString() + ')();';
    (frameDocument.head || frameDocument.documentElement).appendChild(script);
    if (script.parentNode) {
      script.parentNode.removeChild(script);
    }
    if (frameWindow && frameWindow[stateKey]) {
      return;
    }
  }
  mount();
})();
"##;

    static SCRIPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SCRIPT
        .get_or_init(|| {
            TEMPLATE
                .replace(
                    "__ANNOTATE_TOKEN__",
                    &browser_feedback_js_string_literal(browser_annotate_token()),
                )
                .replace(
                    "__ANNOTATE_SCREENSHOT_PREFIX__",
                    &browser_feedback_js_string_literal(ANNOTATE_SCREENSHOT_REFERENCE_PREFIX),
                )
                .replace("__AGENTATION_BUNDLE__", ANNOTATE_AGENTATION_BUNDLE)
        })
        .clone()
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
CDXC:Browser 2026-07-12:
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

pub(crate) fn browser_remote_web_url(remote_url: &str) -> Option<String> {
    let remote = remote_url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let web_url = if let Some(path) = remote.strip_prefix("git@") {
        let (host, repository) = path.split_once(':')?;
        format!("https://{host}/{repository}")
    } else if let Some(path) = remote.strip_prefix("ssh://git@") {
        let (host, repository) = path.split_once('/')?;
        format!("https://{host}/{repository}")
    } else if remote.starts_with("https://") || remote.starts_with("http://") {
        remote.to_string()
    } else {
        return None;
    };

    sanitize_browser_tab_url_for_state(&web_url)
}

pub(crate) fn browser_shell_default_url(browser_home_url: Option<&str>) -> String {
    browser_home_url
        .map(str::to_owned)
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
    CDXC:Git 2026-06-24-15:43:
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
        .map_err(|_| "Ghostex couldn't open the pull request.".to_string())
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
