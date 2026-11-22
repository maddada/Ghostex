//! Theme artworks, downloaded the first time one is shown and kept on disk: what Art paints behind
//! the window (apps/desktop/src/app/helpers/window_art.rs). Each comes from the components
//! repository's `theme-art` release (`ARTWORK_RELEASE` in packages/settings-catalog/src/data/themes.rs)
//! and is kept only when its size and SHA-256 match the ones built into the app, in the component
//! store's `theme-art/` folder (`<id>-<first 8 of the hash>.webp`, so a changed artwork is a new file).
//!
//! CDXC:Theming 2026-11-21 WHY:
//! The app's other on-demand parts are signed archives sealed into the release manifest; an artwork is one public image whose hash and size are compiled into the app, which pins it as firmly without a release-time seal, so a theme can add one without a new component release. `GHOSTEX_THEME_ART_BASE_URL` points the download somewhere else (a test server serving the files locally).

use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::time::Duration;

use ghostex_settings_catalog::{ARTWORK_RELEASE, Artwork, artwork_by_id, artwork_file_name};
use sha2::{Digest as _, Sha256};

/// Where artworks are kept.
pub(crate) fn art_dir() -> PathBuf {
    crate::components::component_root("theme-art")
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
