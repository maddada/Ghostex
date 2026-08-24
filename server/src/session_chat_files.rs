use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::session_git_status::run_git_probe_command;
use crate::storage::open_gxserver_database;
use axum::http::StatusCode;

/*
CDXC:SessionChatFileMentions 2026-08-18:
The chat composer's "@" picker lists the session project's files. Like chat
skills, the walk happens on the session's machine and the client never submits
a scan path: it names a session, gxserver resolves that session's project, and
answers with project-relative paths the agent can resolve against its own
working directory.

Git owns the list when the project is a repository, so .gitignore decides what
is offered instead of a hand-maintained deny list. Non-repository projects fall
back to a bounded walk with the same ignore names the skill discovery uses.
*/

/** Upper bound on returned paths; the client filters the list in the picker. */
const SESSION_CHAT_FILE_LIMIT: usize = 12_000;
/** Directory depth for the non-git fallback walk. */
const SESSION_CHAT_FILE_MAX_DEPTH: usize = 12;

pub fn read_session_chat_files(project_path: Option<&Path>) -> Value {
    let root = project_path.filter(|path| path.is_absolute() && path.is_dir());
    let (files, truncated) = match root {
        Some(root) => collect_project_files(root),
        None => (Vec::new(), false),
    };

    json!({
        "files": files,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "rootPath": root.map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        "truncated": truncated,
    })
}

fn collect_project_files(root: &Path) -> (Vec<String>, bool) {
    if let Some(listed) = git_tracked_files(root) {
        return listed;
    }
    walk_project_files(root)
}

/// Tracked plus untracked-but-not-ignored paths, exactly what an agent can open.
fn git_tracked_files(root: &Path) -> Option<(Vec<String>, bool)> {
    if !root.join(".git").exists() {
        return None;
    }
    let output = run_git_probe_command(
        &root.to_string_lossy(),
        &["ls-files", "--cached", "--others", "--exclude-standard"],
    )?;
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut truncated = false;
    for line in output.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        if files.len() >= SESSION_CHAT_FILE_LIMIT {
            truncated = true;
            break;
        }
        if seen.insert(path.to_string()) {
            files.push(path.to_string());
        }
    }
    files.sort();
    Some((files, truncated))
}

fn walk_project_files(root: &Path) -> (Vec<String>, bool) {
    let mut files = Vec::new();
    let mut truncated = false;
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name = name.to_string_lossy().to_string();
            if is_ignored_entry(&name) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < SESSION_CHAT_FILE_MAX_DEPTH {
                    pending.push((entry.path(), depth + 1));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if files.len() >= SESSION_CHAT_FILE_LIMIT {
                truncated = true;
                break;
            }
            if let Some(relative) = relative_path(root, &entry.path()) {
                files.push(relative);
            }
        }
        if truncated {
            break;
        }
    }
    files.sort();
    (files, truncated)
}

/// Always forward-slash separated so a Windows project reads like the mention.
fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative: PathBuf = path.strip_prefix(root).ok()?.to_path_buf();
    let joined = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    (!joined.is_empty()).then_some(joined)
}

fn is_ignored_entry(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules"
                | "vendor"
                | "dist"
                | "build"
                | "out"
                | "target"
                | "coverage"
                | "__pycache__"
                | "tmp"
                | "DerivedData"
        )
}

/*
CDXC:SessionChatFileMentions 2026-08-18:
The composer's "@" picker names a session; gxserver resolves that session's
project and walks it on this machine, so the client never chooses a scan root.
The walk itself is blocking work (a git probe or a directory sweep), so it runs
off the request thread the same way the skill discovery does.
*/
pub(crate) async fn handle_read_session_chat_files_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "readSessionChatFiles requires projectId and sessionId.".to_string(),
            },
        );
    }

    let resolved = (|| {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        repository
            .get_session(project_id, session_id)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The session no longer exists.".to_string(),
            })?;
        let project = repository
            .get_project(project_id)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The project no longer exists.".to_string(),
            })?;
        Ok::<_, DomainStateError>(
            project
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        )
    })();
    let project_path = match resolved {
        Ok(project_path) => project_path,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result =
        match tokio::task::spawn_blocking(move || read_session_chat_files(project_path.as_deref()))
            .await
        {
            Ok(result) => result,
            Err(_) => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "internalError",
                        message: "Session chat files could not be listed.".to_string(),
                    },
                )
            }
        };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

/*
CDXC:SessionChatImagePaste 2026-08-01:
Chat-composer image paste mirrors the gpui terminal paste contract: bytes
land in the resolved Ghostex image directory on THIS machine (the machine the
session runs on — remote clients reach here via their per-machine RPC), and
the returned absolute path is what the client interpolates into
"[Image #N](path)". suggestedName is only ever mined for its extension;
the stored file name is always generated, so no caller-controlled path
segments touch the filesystem.
*/
pub(crate) const SESSION_CHAT_IMAGE_MAX_BYTES: usize = 12 * 1024 * 1024;

pub(crate) fn session_chat_image_extension(
    base64_bytes: &[u8],
    suggested_name: Option<&str>,
) -> String {
    const KNOWN_EXTENSIONS: &[&str] = &[
        "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg", "png", "svg", "tif", "tiff",
        "webp",
    ];
    if let Some(extension) = suggested_name
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .filter(|extension| KNOWN_EXTENSIONS.contains(&extension.as_str()))
    {
        return if extension == "jpeg" {
            "jpg".to_string()
        } else {
            extension
        };
    }
    if base64_bytes.starts_with(b"\x89PNG") {
        "png".to_string()
    } else if base64_bytes.starts_with(b"\xff\xd8\xff") {
        "jpg".to_string()
    } else if base64_bytes.starts_with(b"GIF8") {
        "gif".to_string()
    } else if base64_bytes.len() >= 12 && &base64_bytes[8..12] == b"WEBP" {
        "webp".to_string()
    } else if base64_bytes.starts_with(b"BM") {
        "bmp".to_string()
    } else {
        "png".to_string()
    }
}

pub(crate) fn unique_session_chat_image_path(
    data_dir: &std::path::Path,
    extension: &str,
) -> std::result::Result<std::path::PathBuf, DomainStateError> {
    let directory = data_dir.join("i");
    std::fs::create_dir_all(&directory).map_err(|_| DomainStateError {
        code: "internalError",
        message: "Could not create the image directory.".to_string(),
    })?;
    let base_name = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
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

pub(crate) fn handle_save_session_chat_image_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let base64_data = params
        .get("base64Data")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Tolerate a full data URL so hosts can pass FileReader output verbatim.
    let base64_payload = base64_data
        .split_once(",")
        .filter(|(prefix, _)| prefix.starts_with("data:"))
        .map(|(_, payload)| payload)
        .unwrap_or(base64_data)
        .trim();
    if base64_payload.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "saveSessionChatImage requires base64Data.".to_string(),
            },
        );
    }
    let bytes = match BASE64_STANDARD.decode(base64_payload) {
        Ok(bytes) => bytes,
        Err(_) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: "saveSessionChatImage base64Data is not valid base64.".to_string(),
                },
            );
        }
    };
    if bytes.is_empty() || bytes.len() > SESSION_CHAT_IMAGE_MAX_BYTES {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: format!(
                    "saveSessionChatImage image must be between 1 byte and {SESSION_CHAT_IMAGE_MAX_BYTES} bytes."
                ),
            },
        );
    }
    let suggested_name = params.get("suggestedName").and_then(Value::as_str);
    let extension = session_chat_image_extension(&bytes, suggested_name);
    let path = match unique_session_chat_image_path(&state.paths.app_data_dir, &extension) {
        Ok(path) => path,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if std::fs::write(&path, &bytes).is_err() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: "Could not write the pasted image.".to_string(),
            },
        );
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({ "path": path.to_string_lossy(), "bytes": bytes.len() }),
        ),
    )
}

/*
CDXC:SessionChatAttachments 2026-08-02:
Non-image sibling of the image paste: any attached file's bytes land in
the resolved Ghostex attachment directory on THIS machine and the returned
absolute path becomes the composer's "[File #N](path)" reference. The
suggested name is sanitized to one flat file name (path segments and
non-portable characters stripped), so no caller-controlled path components
touch the filesystem.
*/
pub(crate) const SESSION_CHAT_ATTACHMENT_MAX_BYTES: usize = 32 * 1024 * 1024;

pub(crate) fn sanitized_session_chat_attachment_name(
    suggested_name: Option<&str>,
) -> Option<String> {
    let base = suggested_name?
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    let cleaned: String = base
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|character| matches!(character, '.' | '-'));
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(80).collect())
}

pub(crate) fn unique_session_chat_attachment_path(
    data_dir: &std::path::Path,
    file_name: &str,
) -> std::result::Result<std::path::PathBuf, DomainStateError> {
    let directory = data_dir.join("f");
    std::fs::create_dir_all(&directory).map_err(|_| DomainStateError {
        code: "internalError",
        message: "Could not create the attachment directory.".to_string(),
    })?;
    let base_name = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    let first = directory.join(format!("{base_name}-{file_name}"));
    if !first.exists() {
        return Ok(first);
    }
    for index in 2..100 {
        let candidate = directory.join(format!("{base_name}-{index}-{file_name}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(directory.join(format!(
        "{}-{}-{}",
        base_name,
        std::process::id(),
        file_name
    )))
}

pub(crate) fn handle_save_session_chat_attachment_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let base64_data = params
        .get("base64Data")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Tolerate a full data URL so hosts can pass FileReader output verbatim.
    let base64_payload = base64_data
        .split_once(",")
        .filter(|(prefix, _)| prefix.starts_with("data:"))
        .map(|(_, payload)| payload)
        .unwrap_or(base64_data)
        .trim();
    if base64_payload.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "saveSessionChatAttachment requires base64Data.".to_string(),
            },
        );
    }
    let bytes = match BASE64_STANDARD.decode(base64_payload) {
        Ok(bytes) => bytes,
        Err(_) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "invalidParams",
                    message: "saveSessionChatAttachment base64Data is not valid base64."
                        .to_string(),
                },
            );
        }
    };
    if bytes.is_empty() || bytes.len() > SESSION_CHAT_ATTACHMENT_MAX_BYTES {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: format!(
                    "saveSessionChatAttachment files must be between 1 byte and {SESSION_CHAT_ATTACHMENT_MAX_BYTES} bytes."
                ),
            },
        );
    }
    let suggested_name = params.get("suggestedName").and_then(Value::as_str);
    let file_name = sanitized_session_chat_attachment_name(suggested_name)
        .unwrap_or_else(|| "attachment.bin".to_string());
    let path = match unique_session_chat_attachment_path(&state.paths.app_data_dir, &file_name) {
        Ok(path) => path,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if std::fs::write(&path, &bytes).is_err() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: "Could not write the attached file.".to_string(),
            },
        );
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({ "path": path.to_string_lossy(), "bytes": bytes.len() }),
        ),
    )
}

/*
readSessionChatImage serves the bytes behind an "[Image #N](path)" reference
so chat-log thumbnails and image links can render: the path lives on THIS
machine (clients call over their per-machine RPC) and only files whose magic
bytes or extension identify an image are returned.
*/
pub(crate) const SESSION_CHAT_IMAGE_READ_MAX_BYTES: u64 = 20 * 1024 * 1024;

pub(crate) fn session_chat_image_media_type(
    bytes: &[u8],
    path: &std::path::Path,
) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.len() >= 12 && (&bytes[4..12] == b"ftypavif" || &bytes[4..12] == b"ftypavis") {
        return Some("image/avif");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    match path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("svg") => Some("image/svg+xml"),
        Some("ico") => Some("image/x-icon"),
        Some("tif" | "tiff") => Some("image/tiff"),
        Some("heic" | "heif") => Some("image/heic"),
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("bmp") => Some("image/bmp"),
        Some("avif") => Some("image/avif"),
        _ => None,
    }
}

pub(crate) fn handle_read_session_chat_image_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
    let _ = state;
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let raw_path = params
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let path = std::path::Path::new(raw_path);
    if raw_path.is_empty() || !path.is_absolute() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "readSessionChatImage requires an absolute path.".to_string(),
            },
        );
    }
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "notFound",
                    message: "The image file does not exist on this machine.".to_string(),
                },
            );
        }
    };
    if metadata.len() == 0 || metadata.len() > SESSION_CHAT_IMAGE_READ_MAX_BYTES {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: format!(
                    "readSessionChatImage serves files between 1 byte and {SESSION_CHAT_IMAGE_READ_MAX_BYTES} bytes."
                ),
            },
        );
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: "Could not read the image file.".to_string(),
                },
            );
        }
    };
    let Some(media_type) = session_chat_image_media_type(&bytes, path) else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "The file is not a recognized image.".to_string(),
            },
        );
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "base64Data": BASE64_STANDARD.encode(&bytes),
                "mediaType": media_type,
                "bytes": bytes.len(),
            }),
        ),
    )
}
