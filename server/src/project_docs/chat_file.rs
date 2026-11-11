//! A file a session's chat linked to, read for the Files view of a remote project: any plain-text
//! file or picture the chat names, wherever it is on this computer, read-only.
//!
//! CDXC:Docs 2026-11-07 DECISION:
//! Issue #462: on a remote computer, a chat link to a file outside the project's Docs folders
//! (an agent's log or scratch file, such as Claude Code's `/private/tmp/claude-<uid>/…/dev.log`)
//! failed with "outside the project's Docs folders"; only the VS Code view could show it. The
//! reporter's own scope is the one taken: Files opens a file by its absolute path only when the
//! session's own chat names it (the chat's Files and links list,
//! `session_files_links::session_names_file`), only plain UTF-8 text up to
//! [`CHAT_FILE_MAX_BYTES`], and never writes it (no save, rename or delete takes an absolute path:
//! `normalized_relative_path` refuses one). The caller is a gxserver client already allowed to
//! read that session's chat; a Ghostex Collab guest cannot reach `runProjectDocsAction` at all
//! (`collab_shim::registry::GUEST_READ_ENDPOINTS`). So the API opens no path the chat did not show, which
//! keeps it from becoming a way to read arbitrary files.
//!
//! CDXC:Docs 2026-11-11 DECISION:
//! User, on a Mac in the chat of a session on his Windows PC: "i clicked on these 2 links in the chat but nothing happened ... i should be able to see them in files", a text file and a picture under `C:\dev\.desk-runner\…`, outside the project. A picture the chat names now opens too, under the same rule: `stat` answers `kind: "image"` and `readResource` returns its bytes (at most [`RESOURCE_MAX_BYTES`]), only for a file the chat names whose name ends in a picture extension. The name check compares the Files and links list's own key (`path_key`: one slash, a drive path in one case) first, so the desktop's `C:/dev/…` matches the chat's `C:\dev\…` without touching the disk, and a named file that is gone says so instead of "not linked".
//! SEE-ALSO: server/src/server/project_docs_http.rs (where the scope is checked),
//! apps/desktop/src/app/session_chat_context_menu/remote_files.rs (the desktop's chat link),
//! apps/desktop/src/app/native_docs/documents.rs (`native_docs_read_image`, the picture's read).

use std::{
    fs,
    io::Read as _,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

use super::*;

/// The largest file a chat link opens in Files. Larger logs open in the VS Code view.
pub const CHAT_FILE_MAX_BYTES: u64 = 2_000_000;
/// How much of the file `stat` reads to tell text from a binary file.
const CHAT_FILE_SNIFF_BYTES: usize = 8 * 1024;

pub const CHAT_FILE_NOT_NAMED: &str =
    "Files opens a file outside the project only when this session's chat links to it.";
pub const CHAT_FILE_TOO_LARGE: &str =
    "This file is larger than 2 MB, too large to open in Files. Use Open in VS Code for it.";
pub const CHAT_FILE_NOT_TEXT: &str =
    "This file is not plain text, so Files can't show it. Use Open in VS Code for it.";
pub const CHAT_FILE_PICTURE_TOO_LARGE: &str =
    "This picture is larger than 12 MB, too large to open in Files.";
const CHAT_FILE_UNAVAILABLE: &str = "That file is not on the session computer any more.";
/// The picture kinds Files draws (`DocsMediaKind::Image` on the desktop), by extension.
const CHAT_PICTURE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "jpe", "gif", "webp", "bmp", "ico", "svg", "tif", "tiff", "avif",
];
const CHAT_FILE_NOT_A_FILE: &str = "That path is not a file.";
const CHAT_FILE_NOT_A_PICTURE: &str = "That file is not a picture.";

/// A `read`, `stat` or `readResource` of a file the chat linked to: the action, its request id, the session and
/// the path exactly as the chat named it.
pub struct ChatFileRequest {
    pub action: String,
    pub request_id: String,
    pub project_id: String,
    pub session_id: String,
    pub path: String,
}

/// `Some` when `params` asks to `read`, `stat` or `readResource` a path outside the project for a session: an
/// absolute path (or one from the home folder, `~/…`) with a `sessionId`. Everything else is an
/// ordinary Docs action and keeps its project-relative rules.
pub fn chat_file_request(params: &Map<String, Value>) -> Option<ChatFileRequest> {
    let action = string_param(params, "action")?;
    if !matches!(action.as_str(), "read" | "stat" | "readResource") {
        return None;
    }
    let path = string_param(params, "path")?.trim().to_string();
    if !names_outside_path(&path) {
        return None;
    }
    let session_id = string_param(params, "sessionId")?.trim().to_string();
    let project_id = string_param(params, "projectId")?.trim().to_string();
    if session_id.is_empty() || project_id.is_empty() {
        return None;
    }
    Some(ChatFileRequest {
        action,
        request_id: string_param(params, "requestId").unwrap_or_default(),
        project_id,
        session_id,
        path,
    })
}

/// An absolute path on this computer, or one from the home folder (`~/…`).
fn names_outside_path(path: &str) -> bool {
    !path.contains('\0')
        && (Path::new(path).is_absolute()
            || path.starts_with('/')
            || path == "~"
            || path.starts_with("~/")
            || path.starts_with("~\\"))
}

/// `path` with a leading `~` made the home folder.
pub fn expanded_chat_file_path(path: &str) -> PathBuf {
    let Some(rest) = path.strip_prefix('~') else {
        return PathBuf::from(path);
    };
    let home = crate::paths::get_gxserver_paths(None).home_dir.clone();
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home
    } else {
        home.join(rest)
    }
}

/// The answer to a chat file request, given whether the session's chat names the path.
pub fn run_chat_file_action(request: &ChatFileRequest, named: bool) -> Value {
    let result = if named {
        let file = expanded_chat_file_path(&request.path);
        match request.action.as_str() {
            "stat" if is_picture(&file) => chat_picture_metadata(&file, &request.path),
            "stat" => chat_file_metadata(&file, &request.path),
            "readResource" => return chat_picture_resource(request, &file),
            _ => chat_file_preview(&file, &request.path),
        }
    } else {
        Err(CHAT_FILE_NOT_NAMED.to_string())
    };
    match result {
        Ok(file) => json!({
            "action": request.action,
            "requestId": request.request_id,
            "rootName": DOCS_RELATIVE_PATH,
            "file": file,
        }),
        Err(error) => json!({
            "action": request.action,
            "requestId": request.request_id,
            "error": error,
        }),
    }
}

fn regular_file(file: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::metadata(file).map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    if !metadata.is_file() {
        return Err(CHAT_FILE_NOT_A_FILE.to_string());
    }
    if metadata.len() > CHAT_FILE_MAX_BYTES {
        return Err(CHAT_FILE_TOO_LARGE.to_string());
    }
    Ok(metadata)
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The file's name, size and time, after checking it is a text file small enough to open. Only
/// its start is read, since the Files view's change watch asks this every few seconds.
fn chat_file_metadata(file: &Path, named: &str) -> Result<Value, String> {
    let metadata = regular_file(file)?;
    let mut start = Vec::with_capacity(CHAT_FILE_SNIFF_BYTES);
    fs::File::open(file)
        .and_then(|handle| {
            handle
                .take(CHAT_FILE_SNIFF_BYTES as u64)
                .read_to_end(&mut start)
        })
        .map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    if !looks_like_text(&start) {
        return Err(CHAT_FILE_NOT_TEXT.to_string());
    }
    Ok(json!({
        "kind": "text",
        "modifiedAt": modified_at(&metadata),
        "name": file_name(file),
        "path": named,
        "readOnly": true,
        "size": metadata.len(),
    }))
}

/// The file's text, read-only, as the Files view's `read` answers it.
fn chat_file_preview(file: &Path, named: &str) -> Result<Value, String> {
    let metadata = regular_file(file)?;
    let mut data = Vec::new();
    fs::File::open(file)
        .and_then(|handle| handle.take(CHAT_FILE_MAX_BYTES + 1).read_to_end(&mut data))
        .map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    // It grew past the limit between the check and the read.
    if data.len() as u64 > CHAT_FILE_MAX_BYTES {
        return Err(CHAT_FILE_TOO_LARGE.to_string());
    }
    if data.contains(&0) {
        return Err(CHAT_FILE_NOT_TEXT.to_string());
    }
    let content = String::from_utf8(data).map_err(|_| CHAT_FILE_NOT_TEXT.to_string())?;
    Ok(json!({
        "content": content,
        "displayPath": named,
        "gitBaseline": Value::Null,
        "kind": "text",
        "modifiedAt": modified_at(&metadata),
        "name": file_name(file),
        "path": named,
        "readOnly": true,
        "size": metadata.len(),
    }))
}

/// Whether the file's name ends in a picture extension Files draws.
fn is_picture(file: &Path) -> bool {
    file.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| CHAT_PICTURE_EXTENSIONS.contains(&extension.as_str()))
}

/// A picture's name, size and time, after checking it is small enough to open.
fn chat_picture_metadata(file: &Path, named: &str) -> Result<Value, String> {
    let metadata = fs::metadata(file).map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    if !metadata.is_file() {
        return Err(CHAT_FILE_NOT_A_FILE.to_string());
    }
    if metadata.len() > RESOURCE_MAX_BYTES {
        return Err(CHAT_FILE_PICTURE_TOO_LARGE.to_string());
    }
    Ok(json!({
        "kind": "image",
        "modifiedAt": modified_at(&metadata),
        "name": file_name(file),
        "path": named,
        "readOnly": true,
        "size": metadata.len(),
    }))
}

/// A picture's bytes, as the Files view's `readResource` answers them. Only a picture: any other
/// file the chat names is read as text (`read`), never handed over as raw bytes.
fn chat_picture_resource(request: &ChatFileRequest, file: &Path) -> Value {
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
    let data = if is_picture(file) {
        chat_picture_metadata(file, &request.path).and_then(|_| {
            let mut data = Vec::new();
            fs::File::open(file)
                .and_then(|handle| handle.take(RESOURCE_MAX_BYTES + 1).read_to_end(&mut data))
                .map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
            if data.len() as u64 > RESOURCE_MAX_BYTES {
                return Err(CHAT_FILE_PICTURE_TOO_LARGE.to_string());
            }
            Ok(data)
        })
    } else {
        Err(CHAT_FILE_NOT_A_PICTURE.to_string())
    };
    match data {
        Ok(data) => json!({
            "action": request.action,
            "requestId": request.request_id,
            "dataBase64": BASE64_STANDARD.encode(data),
        }),
        Err(error) => json!({
            "action": request.action,
            "requestId": request.request_id,
            "error": error,
        }),
    }
}

/// No NUL byte and valid UTF-8, a character cut off by the sample's end aside.
fn looks_like_text(start: &[u8]) -> bool {
    if start.contains(&0) {
        return false;
    }
    match std::str::from_utf8(start) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: &str, path: &str) -> ChatFileRequest {
        ChatFileRequest {
            action: action.to_string(),
            request_id: "r1".to_string(),
            project_id: "p1".to_string(),
            session_id: "s1".to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn only_an_outside_read_or_stat_with_a_session_is_a_chat_file_request() {
        let params = |value: Value| value.as_object().cloned().unwrap();
        let absolute =
            json!({"action": "read", "path": "/tmp/a.log", "projectId": "p", "sessionId": "s"});
        assert!(chat_file_request(&params(absolute.clone())).is_some());
        let mut home = absolute.clone();
        home["path"] = json!("~/notes.txt");
        assert!(chat_file_request(&params(home)).is_some());
        let mut relative = absolute.clone();
        relative["path"] = json!("docs/a.md");
        assert!(chat_file_request(&params(relative)).is_none());
        let mut no_session = absolute.clone();
        no_session.as_object_mut().unwrap().remove("sessionId");
        assert!(chat_file_request(&params(no_session)).is_none());
        for action in ["save", "delete", "rename", "list"] {
            let mut other = absolute.clone();
            other["action"] = json!(action);
            assert!(chat_file_request(&params(other)).is_none(), "{action}");
        }
    }

    #[test]
    fn a_named_text_file_reads_read_only_and_an_unnamed_one_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("dev-8788.log");
        fs::write(&file, "server listening on 8788\n").unwrap();
        let path = file.to_string_lossy().into_owned();

        let read = run_chat_file_action(&request("read", &path), true);
        assert_eq!(read["file"]["content"], "server listening on 8788\n");
        assert_eq!(read["file"]["readOnly"], true);
        assert_eq!(read["file"]["path"], path.as_str());
        let stat = run_chat_file_action(&request("stat", &path), true);
        assert_eq!(stat["file"]["size"], 25);
        assert!(stat.get("error").is_none());

        for action in ["read", "stat"] {
            let refused = run_chat_file_action(&request(action, &path), false);
            assert_eq!(refused["error"], CHAT_FILE_NOT_NAMED);
            assert!(refused.get("file").is_none());
        }
    }

    #[test]
    fn a_binary_or_too_large_file_is_refused_even_when_named() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("core.bin");
        fs::write(&binary, [0x7f, b'E', b'L', b'F', 0, 1, 2]).unwrap();
        let latin1 = temp.path().join("latin1.txt");
        fs::write(&latin1, [b'c', b'a', b'f', 0xe9, b'\n']).unwrap();
        let large = temp.path().join("huge.log");
        fs::write(&large, vec![b'a'; CHAT_FILE_MAX_BYTES as usize + 1]).unwrap();
        let folder = temp.path().join("folder");
        fs::create_dir(&folder).unwrap();

        for action in ["read", "stat"] {
            let answer =
                |file: &Path| run_chat_file_action(&request(action, &file.to_string_lossy()), true);
            assert_eq!(answer(&binary)["error"], CHAT_FILE_NOT_TEXT, "{action}");
            assert_eq!(answer(&latin1)["error"], CHAT_FILE_NOT_TEXT, "{action}");
            assert_eq!(answer(&large)["error"], CHAT_FILE_TOO_LARGE, "{action}");
            assert_eq!(answer(&folder)["error"], CHAT_FILE_NOT_A_FILE, "{action}");
            assert_eq!(
                answer(&temp.path().join("gone.log"))["error"],
                CHAT_FILE_UNAVAILABLE,
                "{action}"
            );
        }
    }
}
