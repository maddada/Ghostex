//! Videos the window glass can play: the aerial wallpapers macOS has downloaded, and video files
//! the user picks.

use std::path::{Path, PathBuf};

/// A saved glass video that names one of macOS's aerial wallpapers rather than a file, so it keeps
/// working when macOS downloads the aerial again under the same id.
const AERIAL_PREFIX: &str = "aerial:";

const VIDEO_EXTENSIONS: [&str; 3] = ["mov", "mp4", "m4v"];

/// CDXC:Theming 2026-09-23 WHY:
/// macOS keeps the aerial wallpapers it has downloaded, and their names, in its wallpaper agent's private folder. The folder and its manifest are undocumented and may change with any macOS update, and macOS deletes aerials it no longer needs, so the list is read fresh each time Settings asks and an aerial that is gone leaves the live blur.
fn aerials_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/com.apple.wallpaper/aerials"))
}

/// One aerial wallpaper macOS has downloaded.
pub(crate) struct AerialVideo {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// The aerial wallpapers downloaded on this computer, by name.
pub(crate) fn downloaded_aerial_videos() -> Vec<AerialVideo> {
    let Some(folder) = aerials_folder() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(folder.join("videos")) else {
        return Vec::new();
    };
    let names = aerial_names(&folder);
    let mut videos: Vec<AerialVideo> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !is_video_file(&path) {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            let name = names
                .iter()
                .find(|(asset, _)| *asset == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| id.clone());
            Some(AerialVideo { id, name })
        })
        .collect();
    videos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    videos
}

/// (asset id, name) for every aerial in macOS's manifest.
fn aerial_names(folder: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(folder.join("manifest/entries.json")) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    manifest["assets"]
        .as_array()
        .map(|assets| {
            assets
                .iter()
                .filter_map(|asset| {
                    let id = asset["id"].as_str()?;
                    let name = asset["accessibilityLabel"]
                        .as_str()
                        .filter(|name| !name.trim().is_empty())?;
                    Some((id.to_string(), name.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn aerial_reference(id: &str) -> String {
    format!("{AERIAL_PREFIX}{id}")
}

/// The file a saved glass video plays: the aerial's downloaded file, or the picked file, when it
/// exists and is a video.
pub(crate) fn resolve_glass_video(saved: &str) -> Option<PathBuf> {
    let saved = saved.trim();
    if saved.is_empty() {
        return None;
    }
    let path = match saved.strip_prefix(AERIAL_PREFIX) {
        Some(id) => {
            let videos = aerials_folder()?.join("videos");
            VIDEO_EXTENSIONS
                .iter()
                .map(|extension| videos.join(format!("{id}.{extension}")))
                .find(|path| path.is_file())?
        }
        None => PathBuf::from(saved),
    };
    (path.is_file() && is_video_file(&path)).then_some(path)
}

/// Whether `path` names a video the glass can play, by its extension.
pub(crate) fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|video| extension.eq_ignore_ascii_case(video))
        })
}
