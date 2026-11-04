use crate::app::{helpers::*, model::*};
use base64::Engine as _;
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

pub(super) fn import_paths(
    remote: &Option<GpuiRemoteGxserverRequestTarget>,
    params: &Value,
) -> Result<Value, String> {
    let paths = params["paths"]
        .as_array()
        .ok_or("Attachment paths are missing")?;
    let Some(remote) = remote else {
        return Ok(json!(paths));
    };
    let mut uploaded = Vec::new();
    for path in paths {
        let path = Path::new(path.as_str().ok_or("Invalid attachment path")?);
        let name = path
            .file_name()
            .ok_or("Attachment has no name")?
            .to_string_lossy()
            .into_owned();
        let mut request = json!({"projectId":params["projectId"],"sessionId":params["sessionId"],"suggestedName":name});
        let saved = if path.is_dir() {
            request["uploadId"] = format!(
                "native-{}-{}",
                std::process::id(),
                gpui_remote_install_unique_id()
            )
            .into();
            upload_directory(remote, path, path, &request)?
        } else {
            upload_file(remote, path, &request)?
        };
        uploaded.push(saved);
    }
    Ok(json!(uploaded))
}

fn save(
    remote: &GpuiRemoteGxserverRequestTarget,
    endpoint: &str,
    params: &Value,
) -> Result<String, String> {
    let result =
        gpui_remote_gxserver_rpc_result(remote, endpoint, params, Duration::from_secs(60))?;
    result["path"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "The session machine did not return an attachment path".into())
}

fn upload_file(
    remote: &GpuiRemoteGxserverRequestTarget,
    path: &Path,
    request: &Value,
) -> Result<String, String> {
    let mut request = request.clone();
    let mut bytes = fs::read(path).map_err(|error| error.to_string())?;
    // Folder entries use the attachment endpoint even when their contents are images.
    let image = request.get("uploadId").is_none()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
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
            });
    if image {
        let format = match path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "bmp" => Some(gpui::ImageFormat::Bmp),
            "tif" | "tiff" => Some(gpui::ImageFormat::Tiff),
            "png" => Some(gpui::ImageFormat::Png),
            "jpg" | "jpeg" => Some(gpui::ImageFormat::Jpeg),
            "webp" => Some(gpui::ImageFormat::Webp),
            "gif" => Some(gpui::ImageFormat::Gif),
            _ => None,
        };
        if let Some(format) = format {
            let prepared =
                crate::app::helpers::clipboard_image::prepare_clipboard_image(&bytes, format)?;
            bytes = prepared.bytes;
            request["suggestedName"] = format!("uploaded-image.{}", prepared.extension).into();
        } else if bytes.len() > crate::app::helpers::clipboard_image::CHAT_IMAGE_MAX_BYTES {
            return Err(format!(
                "Image too large: {:.1} MB. Limit: 32 MiB. Try a smaller capture.",
                bytes.len() as f64 / 1_000_000.0
            ));
        }
    }
    request["base64Data"] = base64::engine::general_purpose::STANDARD
        .encode(bytes)
        .into();
    save(
        remote,
        if image {
            "/api/saveSessionChatImage"
        } else {
            "/api/saveSessionChatAttachment"
        },
        &request,
    )
}

fn upload_directory(
    remote: &GpuiRemoteGxserverRequestTarget,
    root: &Path,
    path: &Path,
    request: &Value,
) -> Result<String, String> {
    let mut directory = request.clone();
    directory["directory"] = true.into();
    directory["base64Data"] = "".into();
    let relative_path = path.strip_prefix(root).map_err(|error| error.to_string())?;
    if !relative_path.as_os_str().is_empty() {
        directory["relativePath"] = relative_path.to_string_lossy().replace('\\', "/").into();
    }
    let saved = save(remote, "/api/saveSessionChatAttachment", &directory)?;
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            upload_directory(remote, root, &entry.path(), request)?;
        } else if kind.is_file() {
            let mut file = request.clone();
            file["relativePath"] = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/")
                .into();
            upload_file(remote, &entry.path(), &file)?;
        }
    }
    Ok(saved)
}
