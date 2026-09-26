//! The user's own video the Live glass can play in place of one of the app's animations.

use std::path::{Path, PathBuf};

const VIDEO_EXTENSIONS: [&str; 3] = ["mov", "mp4", "m4v"];

/// The file a saved glass video plays, when it still exists and is a video.
///
/// CDXC:Theming 2026-09-26 WHY:
/// Only a picked file is a glass video now. macOS's aerial wallpapers were dropped ("they don't look good blurred"), and the Ghostex video library was removed, so a saved `aerial:<id>` or `library:<id>` names no file and plays nothing; normalize.ts clears them when settings are next saved.
pub(crate) fn resolve_glass_video(saved: &str) -> Option<PathBuf> {
    let saved = saved.trim();
    if saved.is_empty() {
        return None;
    }
    let path = PathBuf::from(saved);
    (path.is_absolute() && path.is_file() && is_video_file(&path)).then_some(path)
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
