use std::path::Path;

/// Media files clicked in a terminal belong to the user's OS file handler,
/// not Ghostex's source/document workareas. Keep this extension-only so the
/// dispatch decision is cross-platform; the caller separately verifies that
/// the resolved path is a real local file before launching it.
pub(crate) fn gpui_terminal_file_opens_with_os_default(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    matches!(
        extension.as_str(),
        // Images
        "apng"
            | "avif"
            | "bmp"
            | "gif"
            | "heic"
            | "heif"
            | "ico"
            | "jfif"
            | "jp2"
            | "jpe"
            | "jpeg"
            | "jpg"
            | "jxl"
            | "png"
            | "svg"
            | "tif"
            | "tiff"
            | "webp"
            // Videos
            | "3g2"
            | "3gp"
            | "asf"
            | "avi"
            | "flv"
            | "m2ts"
            | "m4v"
            | "mkv"
            | "mov"
            | "mp4"
            | "mpeg"
            | "mpg"
            | "mts"
            | "ogm"
            | "ogv"
            | "vob"
            | "webm"
            | "wmv"
    )
}
