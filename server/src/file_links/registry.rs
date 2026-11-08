use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use super::serve::FILE_LINK_ROUTE_PREFIX;

/// What a path segment must escape; everything else stays readable in the address bar.
const PATH_SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// What one link's token may read.
#[derive(Clone)]
pub(super) enum Grant {
    /// Every file under a folder (`link_path`).
    Folder(PathBuf),
    /// Exactly one file (`file_link_path`).
    File(PathBuf),
}

/// What links may read, by token. Tokens live in memory only, so every link stops working when
/// gxserver restarts.
#[derive(Default)]
struct Registry {
    grants: HashMap<String, Grant>,
    folder_tokens: HashMap<PathBuf, String>,
    file_tokens: HashMap<PathBuf, String>,
}

/// The kinds a one-file link serves: what the phone plays or shows itself (pictures, video, audio,
/// PDFs) and Excalidraw drawings, whose page loads and saves only the drawing. HTML and Markdown
/// pages load their sibling files, so they keep to folder links; SVG is left out because, opened
/// as a page, it runs script on gxserver's own origin.
const ONE_FILE_EXTENSIONS: &[&str] = &[
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "avif",
    "bmp",
    "ico",
    "mp4",
    "m4v",
    "webm",
    "mov",
    "ogv",
    "mp3",
    "m4a",
    "wav",
    "ogg",
    "oga",
    "opus",
    "flac",
    "pdf",
    "excalidraw",
];

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// CDXC:Docs 2026-10-01 WHY:
/// The link's folder is the whole root the file was opened from (the project or the mounted Docs directory), not the file's own folder, so an HTML page's `../shared.css` and a Markdown file's `../images/x.png` still load. The unguessable token in the path is the only grant: the browser has no gxserver auth token, and one token per root keeps the table from growing with every click.
pub(crate) fn link_path(root: &Path, relative: &str) -> String {
    let mut registry = registry().lock().unwrap_or_else(|error| error.into_inner());
    let token = match registry.folder_tokens.get(root) {
        Some(token) => token.clone(),
        None => {
            let token = uuid::Uuid::new_v4().simple().to_string();
            registry
                .grants
                .insert(token.clone(), Grant::Folder(root.to_path_buf()));
            registry
                .folder_tokens
                .insert(root.to_path_buf(), token.clone());
            token
        }
    };
    format!(
        "{FILE_LINK_ROUTE_PREFIX}{token}/{}",
        encode_relative(relative)
    )
}

/// A link that reads exactly `file` (an absolute path on this machine), for a file a session
/// showed outside its project and Docs folders: `/open-file/<token>/<file name>`.
///
/// CDXC:Docs 2026-10-16 DECISION:
/// User picked letting the computer serve video, audio, pictures and PDFs to the phone "from any folder the session can read, not just the project and Docs roots", "keeping it safe ... never an open file server". The grant is the gxserver auth the phone already holds: only an authenticated `runProjectDocsAction` (`openSessionFileLink`) mints one, and that caller can already read any picture by path (`readSessionChatImage`) and run commands in the session's terminal, so a link adds nothing it could not reach. What is new is the link itself, which needs no token to open; it reads one file, never its folder, only of the kinds above, from a 128-bit token kept in memory until gxserver restarts.
/// CDXC:Docs 2026-10-16 WHY:
/// Tying the grant to paths a session's transcript names was not done: it means reading a whole transcript on every open, misses files a page or a terminal named, and still grants nothing the authenticated caller cannot already read.
pub(crate) fn file_link_path(file: &Path) -> Result<String, String> {
    let file = std::fs::canonicalize(file)
        .ok()
        .filter(|file| file.is_file())
        .ok_or_else(|| "The file does not exist on this computer.".to_string())?;
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !ONE_FILE_EXTENSIONS.contains(&extension.as_str()) {
        return Err("This kind of file can't be opened from here.".to_string());
    }
    if super::protected::is_protected(&file) {
        return Err("Files in this folder can't be opened from here.".to_string());
    }
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut registry = registry().lock().unwrap_or_else(|error| error.into_inner());
    let token = match registry.file_tokens.get(&file) {
        Some(token) => token.clone(),
        None => {
            let token = uuid::Uuid::new_v4().simple().to_string();
            registry
                .grants
                .insert(token.clone(), Grant::File(file.clone()));
            registry.file_tokens.insert(file, token.clone());
            token
        }
    };
    Ok(format!(
        "{FILE_LINK_ROUTE_PREFIX}{token}/{}",
        encode_relative(&name)
    ))
}

fn encode_relative(relative: &str) -> String {
    relative
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| percent_encoding::utf8_percent_encode(segment, PATH_SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn grant_for_token(token: &str) -> Option<Grant> {
    registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .grants
        .get(token)
        .cloned()
}
