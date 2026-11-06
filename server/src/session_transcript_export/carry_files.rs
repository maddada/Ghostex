//! Files a handoff carries: the ones the old conversation pointed at in folders the next session
//! may not be able to open, copied next to the transcript with the transcript's links pointing at
//! the copies.
//!
//! CDXC:TranscriptExport 2026-11-06 WHY:
//! A handoff's transcript quoted paths into the old agent's own folders verbatim (Claude's
//! `projects/<project>/<session>/tool-results/`, its image cache, Codex's sessions folder,
//! `/tmp/claude-*` scratch files, Ghostex's pasted attachments in `f/`), so the new session
//! (another agent, or another computer, where those paths don't exist) could not read what the
//! conversation was about (Discord, 2026-11-04). The copies sit in `<transcript>-files/` beside the
//! transcript, which the new session is already given, and a client carrying the handoff to
//! another computer carries them too (`files` in the export's answer).
//!
//! Only files the transcript names are copied, one by one, and only from those folders: never a
//! folder, never the project itself (it is the repository the new session works in), never
//! anything that looks like a credential or another conversation's state.

use super::*;

/// Most files one handoff copies.
const CARRY_FILES_MAX: usize = 50;
/// Largest file copied: what `saveSessionChatAttachment` takes on another computer.
const CARRY_FILE_MAX_BYTES: u64 =
    crate::session_chat_files::SESSION_CHAT_ATTACHMENT_MAX_BYTES as u64;
/// Most bytes copied for one handoff.
const CARRY_TOTAL_MAX_BYTES: u64 = 128 * 1024 * 1024;
/// Longest path the scan considers.
const CARRY_PATH_MAX_CHARS: usize = 1024;
/// Agents' own folders in the home folder. The folder holding this session's transcript is added
/// to them, which covers a moved config folder (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`).
const AGENT_HOME_FOLDERS: &[&str] = &[
    ".claude",
    ".codex",
    ".cursor",
    ".gemini",
    ".grok",
    ".pi",
    ".omp",
    ".hermes",
    ".copilot",
    ".factory",
    ".qwen",
    ".local/share/opencode",
];
/// Ghostex's own folders a conversation points into: pasted and dropped files, images,
/// earlier handoffs.
const GHOSTEX_DATA_FOLDERS: &[&str] = &["f", "i", "exports"];

/// One copied file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarriedFile {
    /// How the transcript wrote the path.
    pub original: String,
    /// The copy, beside the transcript.
    pub path: PathBuf,
    pub bytes: u64,
}

/// Where a conversation's files may be copied from.
pub(super) struct CarryRoots {
    allowed: Vec<PathBuf>,
    project: Option<PathBuf>,
    home: PathBuf,
}

impl CarryRoots {
    pub(super) fn new(
        data_dir: Option<&Path>,
        project_path: Option<&Path>,
        transcript_path: &Path,
    ) -> Self {
        let home = crate::resume_lookup::home_dir();
        let mut allowed: Vec<PathBuf> = AGENT_HOME_FOLDERS
            .iter()
            .map(|folder| home.join(folder))
            .collect();
        if let Some(root) = agent_root_of(&home, transcript_path) {
            allowed.push(root);
        }
        if let Some(data_dir) = data_dir {
            allowed.extend(
                GHOSTEX_DATA_FOLDERS
                    .iter()
                    .map(|folder| data_dir.join(folder)),
            );
        }
        allowed.push(std::env::temp_dir());
        for temp in [
            "/tmp",
            "/private/tmp",
            "/var/tmp",
            "/var/folders",
            "/private/var/folders",
        ] {
            allowed.push(PathBuf::from(temp));
        }
        let mut canonical: Vec<PathBuf> = Vec::new();
        for root in allowed {
            for candidate in [fs::canonicalize(&root).ok(), Some(root)]
                .into_iter()
                .flatten()
            {
                if !canonical.contains(&candidate) {
                    canonical.push(candidate);
                }
            }
        }
        Self {
            allowed: canonical,
            project: project_path.and_then(|path| fs::canonicalize(path).ok()),
            home,
        }
    }

    fn may_hold(&self, path: &Path) -> bool {
        self.allowed.iter().any(|root| path.starts_with(root))
    }

    /// The real file behind a path the transcript wrote, when it may be copied.
    fn resolve(&self, written: &str) -> Option<(PathBuf, u64)> {
        let expanded = match written.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None => PathBuf::from(written),
        };
        if !expanded.is_absolute() || !self.may_hold(&expanded) {
            return None;
        }
        // Checked again on the real path: a link out of these folders is not followed.
        let real = fs::canonicalize(&expanded).ok()?;
        if !self.may_hold(&real) || is_private(&real) {
            return None;
        }
        if self
            .project
            .as_ref()
            .is_some_and(|project| real.starts_with(project))
        {
            return None;
        }
        let metadata = fs::metadata(&real).ok()?;
        (metadata.is_file() && metadata.len() <= CARRY_FILE_MAX_BYTES)
            .then_some((real, metadata.len()))
    }
}

/// `~/.claude-swap-backup` for `~/.claude-swap-backup/sessions/…/x.jsonl`; one level deeper under
/// `~/.local` and `~/.config`, which hold every app's folders.
fn agent_root_of(home: &Path, transcript: &Path) -> Option<PathBuf> {
    let relative = transcript.strip_prefix(home).ok()?;
    let mut components = relative.components();
    let first = components.next()?.as_os_str().to_string_lossy().to_string();
    if !first.starts_with('.') {
        return None;
    }
    match first.as_str() {
        ".local" | ".config" => {
            let mut root = home.join(&first);
            let second = components.next()?;
            root.push(second);
            if first == ".local" {
                root.push(components.next()?);
            }
            Some(root)
        }
        _ => Some(home.join(first)),
    }
}

/// Credentials, keys and other conversations' state: never copied, wherever they are.
fn is_private(path: &Path) -> bool {
    let private_folder = path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            ".ssh" | ".gnupg" | ".aws" | ".kube" | ".docker" | "keychains" | "auth" | "credentials"
        )
    });
    if private_folder {
        return true;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    name.starts_with(".env")
        || name.starts_with("id_")
        || name == "auth"
        || name.starts_with("auth.")
        || [
            "token",
            "secret",
            "credential",
            "oauth",
            "apikey",
            "api_key",
            "password",
        ]
        .iter()
        .any(|word| name.contains(word))
        || matches!(
            extension.as_str(),
            "pem" | "key" | "p12" | "pfx" | "jsonl" | "sqlite" | "sqlite3" | "db" | "wal" | "shm"
        )
        || name.ends_with("-wal")
        || name.ends_with("-shm")
}

/// Characters that end a path in the transcript's text for sure.
fn ends_path(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '`' | '"' | '\'' | '<' | '>' | '|' | '*' | '\u{0}'
        )
}

/// Where a shorter path may end: before a space or punctuation that may follow a path in prose.
fn may_end_before(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '.' | ',' | ';' | ':' | ')' | ']' | '}' | '!' | '?'
        )
}

/// A path can start here: `/`, `~/` or `C:\`, at the start of the text or after a character that
/// can't be part of a path.
fn path_start(text: &str, index: usize) -> bool {
    let rest = &text[index..];
    let starts = rest.starts_with('/') || rest.starts_with("~/") || {
        let bytes = rest.as_bytes();
        bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
    };
    if !starts {
        return false;
    }
    let Some(before) = text[..index].chars().next_back() else {
        return true;
    };
    !(before.is_alphanumeric() || matches!(before, '/' | '\\' | '.' | '_' | '-' | '~' | ':'))
        || text[..index].ends_with("file://")
}

/// Each path in the text a copy can be made for, as `(start, end, real file, size)`, longest
/// spelling first at each start.
fn find_carriable_paths(text: &str, roots: &CarryRoots) -> Vec<(usize, usize, PathBuf, u64)> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < text.len() {
        if !text.is_char_boundary(index) || !path_start(text, index) {
            index += 1;
            continue;
        }
        let mut hard_end = text.len();
        for (offset, character) in text[index..].char_indices() {
            if ends_path(character) || offset >= CARRY_PATH_MAX_CHARS {
                hard_end = index + offset;
                break;
            }
        }
        // The longest spelling that names a file: a path may hold spaces and dots, so each place
        // it could end is tried, longest first.
        let candidate = &text[index..hard_end];
        let mut ends: Vec<usize> = candidate
            .char_indices()
            .filter(|(_, character)| may_end_before(*character))
            .map(|(offset, _)| offset)
            .collect();
        ends.push(candidate.len());
        let hit = ends
            .into_iter()
            .rev()
            .filter(|end| *end > 1)
            .find_map(|end| {
                roots
                    .resolve(&candidate[..end])
                    .map(|(real, bytes)| (end, real, bytes))
            });
        match hit {
            Some((end, real, bytes)) => {
                found.push((index, index + end, real, bytes));
                index += end;
            }
            None => index += 1,
        }
    }
    found
}

/// `report final.png` for `/x/report final.png`, kept to characters every client finds again in
/// the text (`[A-Za-z0-9._-]`).
fn copy_name(real: &Path) -> String {
    let name = real
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(['-', '.']).to_string();
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

/// The folder a transcript's copies go in: `<exports>/<name>-files/` for `<exports>/<name>.md`.
pub fn carried_files_folder(transcript: &Path) -> PathBuf {
    let stem = transcript
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "handoff".to_string());
    transcript.with_file_name(format!("{stem}-files"))
}

/// Copies the files `markdown` names into `folder` and answers the markdown with its links
/// pointing at the copies. Nothing is created when the text names none.
pub(super) fn carry_referenced_files(
    markdown: &str,
    roots: &CarryRoots,
    folder: &Path,
) -> (String, Vec<CarriedFile>) {
    let found = find_carriable_paths(markdown, roots);
    if found.is_empty() {
        return (markdown.to_string(), Vec::new());
    }
    let mut copies: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut used_names: Vec<String> = Vec::new();
    let mut carried: Vec<CarriedFile> = Vec::new();
    let mut total: u64 = 0;
    let mut out = String::with_capacity(markdown.len());
    let mut last = 0;
    for (start, end, real, bytes) in found {
        let copy = match copies.get(&real) {
            Some(copy) => Some(copy.clone()),
            None if carried.len() < CARRY_FILES_MAX && total + bytes <= CARRY_TOTAL_MAX_BYTES => {
                let name = copy_name(&real);
                let mut unique = name.clone();
                let mut index = 2;
                while used_names.contains(&unique) {
                    unique = match name.rsplit_once('.') {
                        Some((base, extension)) if !base.is_empty() => {
                            format!("{base}-{index}.{extension}")
                        }
                        _ => format!("{name}-{index}"),
                    };
                    index += 1;
                }
                let copy = folder.join(&unique);
                let copied = fs::create_dir_all(folder).is_ok() && fs::copy(&real, &copy).is_ok();
                if copied {
                    used_names.push(unique);
                    total += bytes;
                    copies.insert(real.clone(), copy.clone());
                    carried.push(CarriedFile {
                        original: markdown[start..end].to_string(),
                        path: copy.clone(),
                        bytes,
                    });
                    Some(copy)
                } else {
                    None
                }
            }
            None => None,
        };
        if let Some(copy) = copy {
            out.push_str(&markdown[last..start]);
            out.push_str(&copy.to_string_lossy());
            last = end;
        }
    }
    out.push_str(&markdown[last..]);
    (out, carried)
}

/// The note under the transcript's header that lists the copies.
pub(super) fn carried_files_block(carried: &[CarriedFile]) -> String {
    if carried.is_empty() {
        return String::new();
    }
    let mut block = String::from("## 📎 Files from this conversation\n\n");
    block.push_str(
        "The earlier session kept these files in its own folders, so copies are saved next to this transcript, and every mention of them below points at the copy:\n\n",
    );
    for file in carried {
        block.push_str(&format!(
            "- `{}` (was `{}`)\n",
            file.path.to_string_lossy(),
            file.original
        ));
    }
    block.push('\n');
    block
}
