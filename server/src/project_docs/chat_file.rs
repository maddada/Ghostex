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
//! `normalized_relative_path` refuses one).
//!
//! CDXC:Docs 2026-11-11 DECISION:
//! User, on a Mac in the chat of a session on his Windows PC: "i clicked on these 2 links in the chat but nothing happened ... i should be able to see them in files", a text file and a picture under `C:\dev\.desk-runner\…`, outside the project. A picture the chat names now opens too: `stat` answers `kind: "image"` and `readResource` returns its bytes (at most [`RESOURCE_MAX_BYTES`]).
//!
//! CDXC:Docs 2026-11-11 WHY:
//! Who reaches this route: it needs the default project-sessions access (`security/capability.rs`), so the owner's desktop and web clients, the `ghostex` CLI with the token file, every agent session's own token (for any session id), paired phones, and the owner's other computers and Cloud Boxes over their remote connection. Not Ghostex Collab guests (`route_http/dispatch.rs`), not a connected computer's agents (not in `PEER_ALLOWED_ENDPOINTS`), not extensions. Every one of those callers can already run commands in a session's terminal and read picture files by path (`readSessionChatImage`), so "the chat names it" is a second layer that keeps Files from becoming a file browser, not the security boundary. Within it (security review 2026-11-11): a network share or device path (`\\host\share`, `//host/share`, `\\?\`, `\\.\`) is refused before the chat or the disk is asked, so a link cannot make a Windows computer sign in to someone's SMB server; a file that is itself a link counts only when the chat names its target too (`linked_target`), and the real file is opened once and everything served is checked on that opened file (a regular file, its size, the real file's own name and a picture's own signature), so a link named `x.png` pointing at another file is never served as picture bytes; a hard link cannot be told from its file and is served as the file the chat named; a read asks the chat again, and only the Files view's frequent `stat` uses the remembered answer.
//! SEE-ALSO: server/src/server/project_docs_http.rs (where the scope is checked),
//! server/src/session_files_links.rs (`session_names_file`, `names_by_text`),
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
/// How much of the file `stat` reads to tell text from a binary file, or a picture from anything else.
const CHAT_FILE_SNIFF_BYTES: u64 = 8 * 1024;

pub const CHAT_FILE_NOT_NAMED: &str =
    "Files opens a file outside the project only when this session's chat links to it.";
pub const CHAT_FILE_TOO_LARGE: &str =
    "This file is larger than 2 MB, too large to open in Files. Use Open in VS Code for it.";
pub const CHAT_FILE_NOT_TEXT: &str =
    "This file is not plain text, so Files can't show it. Use Open in VS Code for it.";
pub const CHAT_FILE_PICTURE_TOO_LARGE: &str =
    "This picture is larger than 12 MB, too large to open in Files.";
pub const CHAT_FILE_NETWORK_PATH: &str =
    "Files can't open a file on a network share or a device path. Open it on the computer that has it.";
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
    /// Set for a path Files never opens (a network share or a device path): answered with this
    /// reason before the chat or the disk is asked.
    pub refused: Option<&'static str>,
}

/// `Some` when `params` asks to `read`, `stat` or `readResource` a path outside the project for a session: an
/// absolute path (or one from the home folder, `~/…`) with a `sessionId`. A network share or device
/// path is taken here too, with or without a session, so it is refused rather than read as a
/// project path. Everything else is an ordinary Docs action and keeps its project-relative rules.
pub fn chat_file_request(params: &Map<String, Value>) -> Option<ChatFileRequest> {
    let action = string_param(params, "action")?;
    if !matches!(action.as_str(), "read" | "stat" | "readResource") {
        return None;
    }
    let path = string_param(params, "path")?.trim().to_string();
    let refused = is_network_or_device_path(&path).then_some(CHAT_FILE_NETWORK_PATH);
    if refused.is_none() && !names_outside_path(&path) {
        return None;
    }
    let session_id = string_param(params, "sessionId")
        .unwrap_or_default()
        .trim()
        .to_string();
    let project_id = string_param(params, "projectId")
        .unwrap_or_default()
        .trim()
        .to_string();
    if refused.is_none() && (session_id.is_empty() || project_id.is_empty()) {
        return None;
    }
    Some(ChatFileRequest {
        action,
        request_id: string_param(params, "requestId").unwrap_or_default(),
        project_id,
        session_id,
        path,
        refused,
    })
}

/// A path that starts with two separators of either kind: a UNC network share (`\\host\share`,
/// `//host/share`) or a Windows device or verbatim path (`\\?\…`, `\\.\…`). Decided on the text
/// alone, since asking the disk about one is what signs a Windows computer in to that host.
pub fn is_network_or_device_path(path: &str) -> bool {
    let mut characters = path.trim().chars();
    matches!(
        (characters.next(), characters.next()),
        (Some('/' | '\\'), Some('/' | '\\'))
    )
}

/// Where the file at `path` points when it is itself a link (a symlink, or a Windows junction
/// to a file), as a plain absolute path; `None` for a file that is not a link. The chat must name
/// that target too, so a link the chat names cannot hand over a file the chat never named. Links
/// among the folders above it (macOS's `/tmp` is `/private/tmp`) are left alone.
pub fn linked_target(path: &str) -> Option<String> {
    let file = expanded_chat_file_path(path);
    if !fs::symlink_metadata(&file).ok()?.file_type().is_symlink() {
        return None;
    }
    let target = fs::canonicalize(&file).ok()?;
    let target = target.to_string_lossy();
    // `\\?\C:\x` as the chat would write it, `C:\x`.
    Some(match target.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => rest.to_string(),
        _ => target.into_owned(),
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
    let result = match (request.refused, named) {
        (Some(reason), _) => Err(reason.to_string()),
        (None, false) => Err(CHAT_FILE_NOT_NAMED.to_string()),
        (None, true) => serve(request),
    };
    let mut answer = json!({
        "action": request.action,
        "requestId": request.request_id,
    });
    match result {
        Ok(Served::File(file)) => {
            answer["rootName"] = json!(DOCS_RELATIVE_PATH);
            answer["file"] = file;
        }
        Ok(Served::Bytes(data)) => {
            use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
            answer["dataBase64"] = json!(BASE64_STANDARD.encode(data));
        }
        Err(error) => answer["error"] = json!(error),
    }
    answer
}

enum Served {
    File(Value),
    Bytes(Vec<u8>),
}

/// Serves a named file: links resolved, the real file opened once, and every check made on what
/// was opened.
fn serve(request: &ChatFileRequest) -> Result<Served, String> {
    let file = expanded_chat_file_path(&request.path);
    let real = fs::canonicalize(&file).map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    // The real file's own name decides what it is, not the link's.
    let picture = picture_extension(&real);
    let (limit, too_large) = if picture.is_some() {
        (RESOURCE_MAX_BYTES, CHAT_FILE_PICTURE_TOO_LARGE)
    } else {
        (CHAT_FILE_MAX_BYTES, CHAT_FILE_TOO_LARGE)
    };
    let (handle, metadata) = open_regular_file(&real, limit, too_large)?;
    let described = |kind: &str| {
        json!({
            "kind": kind,
            "modifiedAt": modified_at(&metadata),
            "name": file_name(&file),
            "path": request.path,
            "readOnly": true,
            "size": metadata.len(),
        })
    };
    match (request.action.as_str(), picture) {
        ("stat", picture) => {
            let start = read_up_to(handle, CHAT_FILE_SNIFF_BYTES)?;
            match picture {
                Some(extension) if has_picture_signature(extension, &start) => {
                    Ok(Served::File(described("image")))
                }
                Some(_) => Err(CHAT_FILE_NOT_A_PICTURE.to_string()),
                None if looks_like_text(&start) => Ok(Served::File(described("text"))),
                None => Err(CHAT_FILE_NOT_TEXT.to_string()),
            }
        }
        ("readResource", Some(extension)) => {
            let data = read_up_to(handle, limit + 1)?;
            if data.len() as u64 > limit {
                return Err(too_large.to_string());
            }
            if !has_picture_signature(extension, &data) {
                return Err(CHAT_FILE_NOT_A_PICTURE.to_string());
            }
            Ok(Served::Bytes(data))
        }
        // Only a picture is handed over as bytes; any other file the chat names is read as text.
        ("readResource", None) => Err(CHAT_FILE_NOT_A_PICTURE.to_string()),
        _ => {
            let data = read_up_to(handle, CHAT_FILE_MAX_BYTES + 1)?;
            // It grew past the limit after it was opened.
            if data.len() as u64 > CHAT_FILE_MAX_BYTES {
                return Err(CHAT_FILE_TOO_LARGE.to_string());
            }
            if data.contains(&0) {
                return Err(CHAT_FILE_NOT_TEXT.to_string());
            }
            let content = String::from_utf8(data).map_err(|_| CHAT_FILE_NOT_TEXT.to_string())?;
            let mut file = described("text");
            file["content"] = json!(content);
            file["displayPath"] = json!(request.path);
            file["gitBaseline"] = Value::Null;
            Ok(Served::File(file))
        }
    }
}

/// Opens `real` for reading once it is known to be a regular file no larger than `limit`, and
/// checks again on the opened handle, which is what is then read. The open does not wait: a FIFO
/// swapped in between the two checks would otherwise hold the thread until something wrote to it.
fn open_regular_file(
    real: &Path,
    limit: u64,
    too_large: &str,
) -> Result<(fs::File, fs::Metadata), String> {
    let check = |metadata: &fs::Metadata| {
        if !metadata.is_file() {
            Err(CHAT_FILE_NOT_A_FILE.to_string())
        } else if metadata.len() > limit {
            Err(too_large.to_string())
        } else {
            Ok(())
        }
    };
    check(&fs::metadata(real).map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let handle = options
        .open(real)
        .map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    let metadata = handle
        .metadata()
        .map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    check(&metadata)?;
    Ok((handle, metadata))
}

fn read_up_to(handle: fs::File, limit: u64) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    handle
        .take(limit)
        .read_to_end(&mut data)
        .map_err(|_| CHAT_FILE_UNAVAILABLE.to_string())?;
    Ok(data)
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The picture extension Files draws that the file's name ends in, lower-cased.
fn picture_extension(file: &Path) -> Option<&'static str> {
    let extension = file.extension()?.to_str()?.to_ascii_lowercase();
    CHAT_PICTURE_EXTENSIONS
        .iter()
        .copied()
        .find(|known| *known == extension)
}

/// Whether the file's first bytes are those of the picture kind its extension names: the
/// format's own signature, or for SVG, text that holds an `<svg` element.
fn has_picture_signature(extension: &str, start: &[u8]) -> bool {
    match extension {
        "png" => start.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" | "jfif" | "jpe" => start.starts_with(b"\xff\xd8\xff"),
        "gif" => start.starts_with(b"GIF87a") || start.starts_with(b"GIF89a"),
        "webp" => start.len() >= 12 && &start[..4] == b"RIFF" && &start[8..12] == b"WEBP",
        "bmp" => start.starts_with(b"BM"),
        "ico" => start.starts_with(b"\x00\x00\x01\x00"),
        "tif" | "tiff" => start.starts_with(b"II*\x00") || start.starts_with(b"MM\x00*"),
        "avif" => start.len() >= 12 && matches!(&start[4..12], b"ftypavif" | b"ftypavis"),
        "svg" => {
            let sample = &start[..start.len().min(CHAT_FILE_SNIFF_BYTES as usize)];
            looks_like_text(sample)
                && String::from_utf8_lossy(sample)
                    .to_ascii_lowercase()
                    .contains("<svg")
        }
        _ => false,
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
            refused: None,
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
        // Windows cannot be run here: a network share or device path is refused on its text, with
        // or without a session, before the chat or the disk is asked.
        for path in [
            r"\\attacker\share\x.png",
            "//attacker/share/x.png",
            r"\\?\C:\x.txt",
            r"\\.\pipe\x",
            r"\\?\UNC\host\share\x.txt",
        ] {
            let mut network = absolute.clone();
            network["path"] = json!(path);
            let request = chat_file_request(&params(network.clone())).expect(path);
            assert_eq!(request.refused, Some(CHAT_FILE_NETWORK_PATH), "{path}");
            assert_eq!(
                run_chat_file_action(&request, true)["error"],
                CHAT_FILE_NETWORK_PATH
            );
            network.as_object_mut().unwrap().remove("sessionId");
            assert!(chat_file_request(&params(network)).is_some_and(|r| r.refused.is_some()));
        }
        for path in [r"C:\dev\.desk-runner\a.txt", "C:/dev/a.txt", "/tmp/a.log"] {
            assert!(!is_network_or_device_path(path), "{path}");
        }
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
