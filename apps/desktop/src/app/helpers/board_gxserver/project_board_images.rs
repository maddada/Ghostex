// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds project board image request/clipboard handling: saving
// clipboard images, building preview data URLs, and path/extension/MIME
// helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use gpui::{ClipboardEntry, ClipboardItem, ImageFormat};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn project_board_image_request_needs_clipboard(payload: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|request| manage_request_string(&request, "action"))
        .as_deref()
        == Some("pasteImage")
}

pub(crate) fn project_board_image_bridge_response_for_payload(
    payload: &str,
    clipboard_item: Option<ClipboardItem>,
) -> serde_json::Value {
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    let action = manage_request_string(&request, "action").unwrap_or_default();
    let result = match action.as_str() {
        "pasteImage" => {
            project_board_clipboard_image_path(clipboard_item.as_ref()).map(|image_path| {
                let request_id = request_id.clone();
                serde_json::json!({
                    "dataUrl": null,
                    "error": null,
                    "imagePath": image_path,
                    "path": null,
                    "requestId": request_id,
                })
            })
        }
        "loadPreview" => {
            let path = manage_request_string(&request, "path").unwrap_or_default();
            project_board_image_preview_data_url(&path).map(|data_url| {
                let request_id = request_id.clone();
                serde_json::json!({
                    "dataUrl": data_url,
                    "error": null,
                    "imagePath": null,
                    "path": path,
                    "requestId": request_id,
                })
            })
        }
        _ => Err(format!("Unsupported Project Board image action: {action}")),
    };
    result.unwrap_or_else(|error| {
        serde_json::json!({
            "dataUrl": null,
            "error": error,
            "imagePath": null,
            "path": request.get("path").and_then(serde_json::Value::as_str),
            "requestId": request_id,
        })
    })
}

pub(crate) fn project_board_clipboard_image_path(
    clipboard_item: Option<&ClipboardItem>,
) -> Result<String, String> {
    /*
    CDXC:Clipboard 2026-06-24-11:03:
    Kanban image paste through CEF should preserve the native Project Board contract: return a durable path reference, not base64 Markdown. Existing image file references stay as paths; GPUI image clipboard bytes are saved under the resolved Ghostex image directory with their declared image format and returned as the same display path convention.
    */
    let item = clipboard_item
        .ok_or_else(|| "Clipboard does not contain an image path or image data.".to_string())?;
    for entry in &item.entries {
        if let ClipboardEntry::ExternalPaths(paths) = entry {
            for path in paths.paths() {
                if is_project_board_image_file_path(path) {
                    return Ok(project_board_display_image_path_for_existing_path(path));
                }
            }
        }
    }
    for entry in &item.entries {
        if let ClipboardEntry::String(clipboard_string) = entry {
            if let Some(path) = project_board_image_path_from_reference(clipboard_string.text()) {
                if is_project_board_image_file_path(&path) {
                    return Ok(project_board_display_image_path_for_existing_path(&path));
                }
            }
        }
    }
    for entry in &item.entries {
        if let ClipboardEntry::Image(image) = entry {
            let bytes = image.bytes();
            if bytes.is_empty() || bytes.len() > PROJECT_BOARD_CLIPBOARD_IMAGE_MAX_BYTES {
                return Err("Clipboard image is too large to save.".to_string());
            }
            let path =
                unique_project_board_image_path(project_board_image_extension(image.format()))?;
            fs::write(&path, bytes).map_err(|_| "Could not save clipboard image.".to_string())?;
            return Ok(project_board_display_image_path_for_saved_path(&path));
        }
    }
    Err("Clipboard does not contain an image path or image data.".to_string())
}

pub(crate) fn project_board_image_preview_data_url(path: &str) -> Result<String, String> {
    let path = project_board_image_path_from_reference(path)
        .ok_or_else(|| "Image preview path does not point to a local image.".to_string())?;
    if !is_project_board_image_file_path(&path) {
        return Err("Image preview path does not point to a local image.".to_string());
    }
    let metadata = fs::metadata(&path)
        .map_err(|_| "Image preview path does not point to a local image.".to_string())?;
    if !metadata.is_file() || metadata.len() as usize > PROJECT_BOARD_IMAGE_PREVIEW_MAX_BYTES {
        return Err("Image preview data could not be decoded.".to_string());
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime_type = project_board_image_mime_type_for_extension(&extension)
        .ok_or_else(|| "Image preview format is not supported by this CEF runtime.".to_string())?;
    let data =
        fs::read(&path).map_err(|_| "Image preview data could not be decoded.".to_string())?;
    Ok(format!(
        "data:{mime_type};base64,{}",
        project_board_base64_encode(&data)
    ))
}

pub(crate) fn project_board_image_path_from_reference(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }
    if trimmed.starts_with("file://") {
        let parsed = gpui::http_client::Url::parse(trimmed).ok()?;
        if parsed.scheme() == "file" {
            return parsed.to_file_path().ok();
        }
        return None;
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/.ghostex/") {
        return Some(
            shared_settings::ghostex_storage_paths()
                .data_dir
                .join(relative_path),
        );
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/") {
        return Some(home_dir().join(relative_path));
    }
    if trimmed.starts_with('/') {
        return Some(PathBuf::from(trimmed));
    }
    None
}

pub(crate) fn is_project_board_image_file_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    project_board_image_extension_is_allowed(&extension)
        && fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

pub(crate) fn project_board_image_extension_is_allowed(extension: &str) -> bool {
    matches!(
        extension,
        "avif"
            | "bmp"
            | "gif"
            | "heic"
            | "heif"
            | "ico"
            | "jpg"
            | "jpeg"
            | "png"
            | "svg"
            | "tif"
            | "tiff"
            | "webp"
    )
}

pub(crate) fn project_board_image_mime_type_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "avif" => Some("image/avif"),
        "bmp" => Some("image/bmp"),
        "gif" => Some("image/gif"),
        "ico" => Some("image/x-icon"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "svg" => Some("image/svg+xml"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

pub(crate) fn project_board_image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tif",
        ImageFormat::Ico => "ico",
        ImageFormat::Pnm => "pnm",
    }
}

pub(crate) fn unique_project_board_image_path(extension: &str) -> Result<PathBuf, String> {
    let directory = shared_settings::ghostex_storage_paths().images_dir();
    fs::create_dir_all(&directory).map_err(|_| "Could not create image directory.".to_string())?;
    let base_name = system_time_epoch_millis_string(std::time::SystemTime::now());
    let first = directory.join(format!("{base_name}.{extension}"));
    if !first.exists() {
        return Ok(first);
    }
    for index in 2..100 {
        let candidate = directory.join(format!("{base_name}-{index}.{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(directory.join(format!(
        "{}-{}.{}",
        base_name,
        std::process::id(),
        extension
    )))
}

pub(crate) fn project_board_display_image_path_for_saved_path(path: &Path) -> String {
    project_board_display_image_path_for_existing_path(path)
}

pub(crate) fn project_board_display_image_path_for_existing_path(path: &Path) -> String {
    let user_home = home_dir();
    if let Ok(relative_path) = path.strip_prefix(&user_home) {
        return format!("~/{}", relative_path.to_string_lossy());
    }
    path.to_string_lossy().to_string()
}

pub(crate) fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn project_board_base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(b0 >> 2) as usize] as char);
        output.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}
