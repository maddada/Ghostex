//! Reading and rewriting the agents' own trust files without disturbing the rest of them.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};

/// The file's text, or `None` when it does not exist.
pub(super) fn read_text(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// A JSON object file read for editing, with the `//` comment lines that open it (Copilot heads its
/// `config.json` with two). `None` when the file does not exist or is empty. A file that holds
/// anything but a JSON object is an error, so it is never overwritten.
pub(crate) fn read_json_object(path: &Path) -> io::Result<Option<JsonFile>> {
    let Some(text) = read_text(path)? else {
        return Ok(None);
    };
    let body_start = text
        .split_inclusive('\n')
        .take_while(|line| line.trim_start().starts_with("//"))
        .map(str::len)
        .sum::<usize>();
    let (header, body) = text.split_at(body_start);
    if body.trim().is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(object)) => Ok(Some(JsonFile {
            header: header.to_string(),
            object,
        })),
        Ok(_) => Err(io::Error::other("it is not a JSON object")),
        Err(error) => Err(io::Error::other(format!("it is not valid JSON ({error})"))),
    }
}

/// A JSON object file and the comment lines above it.
#[derive(Default)]
pub(crate) struct JsonFile {
    pub(crate) header: String,
    pub(crate) object: Map<String, Value>,
}

pub(crate) fn write_json_object(path: &Path, file: &JsonFile) -> io::Result<()> {
    let mut text = file.header.clone();
    text.push_str(&serde_json::to_string_pretty(&file.object).map_err(io::Error::other)?);
    text.push('\n');
    write_atomically(path, &text)
}

/// Replaces the file in one rename so a starting agent never reads half of it, keeping its
/// permissions (Claude's `.claude.json` holds a login). A symlinked config is written through to
/// its target: xswap links every Codex account's `config.toml` to `~/.codex/config.toml`, and a
/// rename onto the link would cut that account off from the shared file.
pub(super) fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let target: PathBuf = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("it has no parent folder"))?;
    fs::create_dir_all(parent)?;
    let file_name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let temporary = parent.join(format!(
        ".{file_name}.ghostex-trust.{}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, contents)?;
    if let Ok(metadata) = fs::metadata(&target) {
        let _ = fs::set_permissions(&temporary, metadata.permissions());
    }
    fs::rename(&temporary, &target).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

/// Every `<root>/<entry>/<file_name>` that exists, for profile folders such as cswap's
/// `sessions/<slot>` or xswap's `accounts/<slot>`.
pub(crate) fn existing_files_in_subfolders(root: &Path, file_name: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(file_name))
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

/// Drops files that are the same file under another spelling (a symlinked profile config).
pub(crate) fn dedupe_files(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut unique = Vec::new();
    for file in files {
        let identity = fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
        if !seen.contains(&identity) {
            seen.push(identity);
            unique.push(file);
        }
    }
    unique
}

/// A configuration folder named by an environment variable when it holds an absolute path,
/// otherwise the folder under home.
pub(super) fn env_folder_or(env_key: &str, fallback: PathBuf) -> PathBuf {
    std::env::var_os(env_key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or(fallback)
}
