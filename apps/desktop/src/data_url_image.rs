//! Images drawn from `data:` URLs (project icons, agent logos, favicons): the decoders and one
//! bounded cache. Standalone, so the Settings preview binary (bin/native_modal_demo.rs) and the
//! web build (apps/gpui-web) compile the same code the app does.
use gpui::{Image, ImageFormat};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// The most bytes one decoded image may hold.
pub(crate) const BROWSER_FAVICON_IMAGE_MAX_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct ImageCache {
    images: HashMap<String, Arc<Image>>,
    order: VecDeque<String>,
    source_bytes: usize,
}

thread_local! {
    static IMAGES: RefCell<[ImageCache; 2]> = RefCell::default();
}

pub(crate) fn image(value: &str, monochrome: bool) -> Option<Arc<Image>> {
    let value = value.trim();
    if value.len() > 4 * 1024 * 1024 {
        return None;
    }
    // CDXC:Sidebar 2026-09-17 WHY:
    // Scroll and spinner frames reuse the same icon data; decoding and hashing new GPUI images on every row render wastes UI-thread time.
    // Bound retained source data and entries independently, including separate light-theme artwork.
    IMAGES.with_borrow_mut(|caches| {
        let cache = &mut caches[usize::from(monochrome)];
        if let Some(image) = cache.images.get(value) {
            return Some(image.clone());
        }
        let image = decode_image(value, monochrome)?;
        while cache.images.len() >= 128 || cache.source_bytes + value.len() > 4 * 1024 * 1024 {
            let oldest = cache.order.pop_front()?;
            cache.source_bytes -= oldest.len();
            cache.images.remove(&oldest);
        }
        cache.source_bytes += value.len();
        cache.order.push_back(value.to_owned());
        cache.images.insert(value.to_owned(), image.clone());
        Some(image)
    })
}

fn decode_image(value: &str, monochrome: bool) -> Option<Arc<Image>> {
    let (metadata, payload) = value.strip_prefix("data:")?.split_once(',')?;
    let (format, encoded) = if metadata.split(';').next()? == "image/svg+xml" {
        (
            ImageFormat::Svg,
            metadata
                .split(';')
                .any(|part| part.eq_ignore_ascii_case("base64")),
        )
    } else {
        browser_favicon_data_url_metadata(metadata)?
    };
    let bytes = browser_favicon_percent_decode(payload, 4 * 1024 * 1024)?;
    let bytes = if encoded {
        browser_favicon_decode_base64(&bytes)?
    } else {
        bytes
    };
    if bytes.is_empty() {
        return None;
    }
    let bytes = if monochrome && format == ImageFormat::Svg {
        let mut svg = String::from_utf8(bytes).ok()?;
        let start = svg.find("<svg")?;
        let content = start + svg[start..].find('>')? + 1;
        let end = svg.rfind("</svg>")?;
        svg.insert_str(end, "</g>");
        svg.insert_str(content, r##"<defs><filter id="native-sidebar-monochrome" color-interpolation-filters="sRGB"><feColorMatrix type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0"/></filter></defs><g filter="url(#native-sidebar-monochrome)">"##);
        svg.into_bytes()
    } else {
        bytes
    };
    Some(Arc::new(Image::from_bytes(format, bytes)))
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
