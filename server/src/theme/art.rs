//! Theme artworks, downloaded the first time one is shown and kept on disk: what Art paints behind
//! the window (apps/desktop/src/app/helpers/window_art.rs). Each comes from the components
//! repository's `theme-art` release (`ARTWORK_RELEASE` in packages/settings-catalog/src/data/themes.rs)
//! and is kept only when its size and SHA-256 match the ones built into the app, in the component
//! store's `theme-art/` folder (`<id>-<first 8 of the hash>.webp`, so a changed artwork is a new file).
//!
//! CDXC:Theming 2026-11-21 WHY:
//! The app's other on-demand parts are signed archives sealed into the release manifest; an artwork is one public image whose hash and size are compiled into the app, which pins it as firmly without a release-time seal, so a theme can add one without a new component release. `GHOSTEX_THEME_ART_BASE_URL` points the download somewhere else (a test server serving the files locally).

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ghostex_settings_catalog::{ARTWORK_RELEASE, Artwork, artwork_by_id, artwork_file_name};
use sha2::{Digest as _, Sha256};

/// Where artworks are kept.
pub(crate) fn art_dir() -> PathBuf {
    crate::components::component_root("theme-art")
}

/// Where Your image pictures are kept (CDXC:Theming 2026-11-22 on `YOUR_IMAGE_ART_ID` in
/// packages/settings-catalog/src/data/themes.rs): `<first 16 of the hash>.<format>`, so picking the
/// same picture twice keeps one copy and a saved theme's picture stays put.
pub(crate) fn your_images_dir() -> PathBuf {
    art_dir().join("yours")
}

/// The largest picture Your image takes.
const YOUR_IMAGE_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The longest side Your image takes, in pixels.
const YOUR_IMAGE_MAX_SIDE: u32 = 16_384;

/// Copies picture `source` into [`your_images_dir`] and returns the kept file. Refuses a file
/// that is not a PNG, JPEG, WebP, GIF, BMP or TIFF picture the app can paint. Blocking: run it
/// off the async runtime.
pub(crate) fn keep_your_image(source: &Path) -> Result<PathBuf, String> {
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.display().to_string());
    let meta =
        std::fs::metadata(source).map_err(|error| format!("Could not open {name}: {error}."))?;
    if !meta.is_file() {
        return Err(format!("{name} is not a file."));
    }
    if meta.len() > YOUR_IMAGE_MAX_BYTES {
        return Err(format!(
            "{name} is larger than 64 MB. Pick a smaller picture."
        ));
    }
    let bytes =
        std::fs::read(source).map_err(|error| format!("Could not read {name}: {error}."))?;
    let unsupported = || {
        format!(
            "{name} is not a picture Ghostex can show. Pick a PNG, JPEG, WebP, GIF, BMP or TIFF."
        )
    };
    let format = image::guess_format(&bytes).map_err(|_| unsupported())?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::Bmp => "bmp",
        image::ImageFormat::Tiff => "tiff",
        _ => return Err(unsupported()),
    };
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| unsupported())?;
    if width == 0 || height == 0 || width.max(height) > YOUR_IMAGE_MAX_SIDE {
        return Err(format!(
            "{name} is {width} by {height} pixels. Pick a picture no wider or taller than {YOUR_IMAGE_MAX_SIDE}."
        ));
    }
    let digest = hex::encode(Sha256::digest(&bytes));
    let dir = your_images_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not make {}: {error}", dir.display()))?;
    let kept = dir.join(format!("{}.{extension}", &digest[..16]));
    if std::fs::metadata(&kept).is_ok_and(|meta| meta.len() == bytes.len() as u64) {
        return Ok(kept);
    }
    let partial = dir.join(format!(
        ".copy-{}-{}",
        std::process::id(),
        crate::components::unique()
    ));
    let written = std::fs::write(&partial, &bytes).and_then(|()| std::fs::rename(&partial, &kept));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("Could not keep {name}: {error}."));
    }
    Ok(kept)
}

/// The kept Your image picture `path` names (a `windowArtImage` value), refused unless it is one
/// of [`your_images_dir`]'s files.
pub(crate) fn kept_your_image(path: &str) -> Result<PathBuf, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("No picture is chosen for Your image.".to_string());
    }
    let dir = std::fs::canonicalize(your_images_dir())
        .map_err(|_| "The picture for Your image is not on this computer.".to_string())?;
    let file = std::fs::canonicalize(path)
        .map_err(|_| "The picture for Your image is not on this computer.".to_string())?;
    if !file.starts_with(&dir) || !file.is_file() {
        return Err("The picture for Your image is not one Ghostex kept.".to_string());
    }
    Ok(file)
}

/// The kept file of `artwork`, whether or not it is there yet.
fn kept_path(artwork: &Artwork) -> PathBuf {
    art_dir().join(artwork_file_name(artwork))
}

/// Whether `artwork` is downloaded already.
pub(crate) fn is_downloaded(artwork: &Artwork) -> bool {
    std::fs::metadata(kept_path(artwork)).is_ok_and(|meta| meta.len() == artwork.size_bytes)
}

fn download_url(artwork: &Artwork) -> String {
    let base = std::env::var("GHOSTEX_THEME_ART_BASE_URL")
        .ok()
        .filter(|base| !base.trim().is_empty())
        .unwrap_or_else(|| ARTWORK_RELEASE.to_string());
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        artwork_file_name(artwork)
    )
}

/// The kept file of artwork `id`, downloading it first when it is not here. Blocking: run it off
/// the async runtime.
pub(crate) fn ensure_artwork(id: &str) -> Result<PathBuf, String> {
    let artwork = artwork_by_id(id).ok_or_else(|| format!("There is no artwork \"{id}\"."))?;
    let path = kept_path(artwork);
    if is_downloaded(artwork) {
        return Ok(path);
    }
    let dir = art_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not make {}: {error}", dir.display()))?;
    let response = crate::http_agent::agent()
        .get(&download_url(artwork))
        .config()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .call()
        .map_err(|error| {
            if crate::http_agent::never_connected(&error) {
                format!(
                    "Could not reach GitHub to download the {} artwork.",
                    artwork.name
                )
            } else {
                format!("The {} artwork download failed: {error}", artwork.name)
            }
        })?;
    let mut reader = response.into_body().into_reader();
    let partial = dir.join(format!(
        ".download-{}-{}",
        std::process::id(),
        crate::components::unique()
    ));
    let result = (|| {
        let mut file = std::fs::File::create(&partial).map_err(|error| error.to_string())?;
        let mut hasher = Sha256::new();
        let mut done = 0_u64;
        let mut buffer = vec![0_u8; 128 * 1024];
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            done += read as u64;
            if done > artwork.size_bytes {
                return Err("larger than expected".to_string());
            }
            hasher.update(&buffer[..read]);
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
        }
        file.flush().map_err(|error| error.to_string())?;
        if done != artwork.size_bytes || hex::encode(hasher.finalize()) != artwork.sha256 {
            return Err("it does not match the artwork this app expects".to_string());
        }
        std::fs::rename(&partial, &path).map_err(|error| error.to_string())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "The {} artwork download was refused: {error}.",
            artwork.name
        ));
    }
    Ok(path)
}
