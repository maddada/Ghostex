use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use gpui::{Image, ImageFormat};
use sha2::{Digest as _, Sha256};

use crate::app::helpers::*;
use crate::*;

/// CDXC:Browser 2026-09-21 DECISION:
/// User: "cache the favicon across app restarts for the browser in the gpui app". One file per page origin under the cache directory, named by a hash of the origin (plus the remote computer for forwarded localhost pages) so no address reaches the disk; shell state still carries no favicon data. Supersedes the 2026-06-22 runtime-only favicon rule. A restored or freshly navigated tab shows the cached icon until the page reports its own.
fn browser_favicon_cache() -> &'static Mutex<HashMap<String, Option<BrowserFaviconImage>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<BrowserFaviconImage>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn browser_favicon_cache_path(cache_key: &str) -> PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .cache_dir
        .join("browser-favicons")
        .join(cache_key)
}

pub(crate) fn browser_favicon_cache_key(
    page_url: &str,
    remote_machine_id: Option<&str>,
) -> Option<String> {
    let origin = browser_url_origin_key(page_url)?;
    if !origin.starts_with("http://") && !origin.starts_with("https://") {
        return None;
    }
    let identity = format!("{origin}\n{}", remote_machine_id.unwrap_or_default());
    Some(hex::encode(Sha256::digest(identity.as_bytes())))
}

pub(crate) fn browser_favicon_cache_lookup(
    page_url: &str,
    remote_machine_id: Option<&str>,
) -> Option<BrowserFaviconImage> {
    let cache_key = browser_favicon_cache_key(page_url, remote_machine_id)?;
    browser_favicon_cache()
        .lock()
        .ok()?
        .entry(cache_key)
        .or_insert_with_key(|cache_key| browser_favicon_cache_read(cache_key))
        .clone()
}

/// The file is the image's MIME type, a newline, then the encoded bytes.
fn browser_favicon_cache_read(cache_key: &str) -> Option<BrowserFaviconImage> {
    let contents = std::fs::read(browser_favicon_cache_path(cache_key)).ok()?;
    let split = contents.iter().position(|byte| *byte == b'\n')?;
    let format =
        browser_favicon_image_format_for_mime(std::str::from_utf8(&contents[..split]).ok()?)?;
    let bytes = contents[split + 1..].to_vec();
    if bytes.is_empty() || bytes.len() > BROWSER_FAVICON_IMAGE_MAX_BYTES {
        return None;
    }
    browser_favicon_validate_encoded_dimensions(format, &bytes).ok()?;
    Some(BrowserFaviconImage {
        image: Arc::new(Image::from_bytes(format, bytes)),
    })
}

pub(crate) fn browser_favicon_cache_store(cache_key: &str, format: ImageFormat, bytes: &[u8]) {
    let image = BrowserFaviconImage {
        image: Arc::new(Image::from_bytes(format, bytes.to_vec())),
    };
    if let Ok(mut cache) = browser_favicon_cache().lock() {
        if cache.get(cache_key).and_then(Option::as_ref) == Some(&image) {
            return;
        }
        cache.insert(cache_key.to_string(), Some(image));
    }
    let path = browser_favicon_cache_path(cache_key);
    let mut contents = format.mime_type().as_bytes().to_vec();
    contents.push(b'\n');
    contents.extend_from_slice(bytes);
    std::thread::spawn(move || {
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, contents));
        if let Err(error) = written {
            eprintln!("Could not cache a browser favicon: {error}");
        }
    });
}

pub(crate) fn browser_favicon_cache_store_image(cache_key: &str, favicon: &BrowserFaviconImage) {
    browser_favicon_cache_store(cache_key, favicon.image.format(), favicon.image.bytes());
}
