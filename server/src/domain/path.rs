use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::{DomainResult, DomainStateError};

pub(crate) fn normalize_path_for_comparison(input: &str) -> String {
    let trimmed = input.trim();
    let without_trailing_slash = trimmed.trim_end_matches(&['/', '\\'][..]);
    let candidate = if without_trailing_slash.is_empty() {
        trimmed
    } else {
        without_trailing_slash
    };
    if candidate.is_empty() {
        return String::new();
    }
    fs::canonicalize(candidate)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| candidate.to_string())
}

pub(crate) fn path_file_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/*
CDXC:GxserverProjectPaths 2026-06-22-06:07:
Add Project and session cwd/projectPath resolution must match TypeScript's `normalizeExistingDirectoryPath`: accept absolute paths plus `~` shortcuts, reject non-string/blank/relative inputs with path-specific messages, and store the `path.resolve`-style normalized string so duplicate adds with `..`, `.`, or trailing separators return the existing project.
JSON `null` follows the TypeScript nullish fallback contract (`path ?? projectPath`, `projectPath ?? cwd`); blank strings and non-strings stay selected and fail validation instead of falling through.
*/
pub(crate) fn normalize_existing_directory_path(
    value: Option<&Value>,
    field: &str,
) -> DomainResult<String> {
    normalize_project_root_path(value, field, false)
}

/*
CDXC:AddProjectDialog 2026-07-30:
The Add Project dialog submits a typed path that may not exist yet
("Create & Add"), so `/api/addProjectPath` accepts `createIfMissing` and creates
the workspace root before registering it. The path syntax, absolute/`~` rules,
and the not-found/not-a-directory messages stay exactly what they were, so the
flag-absent behavior is byte-identical to the previous contract; only the
mkdir-failure message is new.
*/
pub(crate) fn normalize_project_root_path(
    value: Option<&Value>,
    field: &str,
    create_if_missing: bool,
) -> DomainResult<String> {
    let Some(path) = value.and_then(Value::as_str).map(str::trim) else {
        return Err(DomainStateError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    };
    if path.is_empty() {
        return Err(DomainStateError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(path);
    if !Path::new(&expanded).is_absolute() {
        return Err(DomainStateError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    let normalized = path_to_string(&resolve_path_syntax(PathBuf::from(expanded)));
    if create_if_missing && !Path::new(&normalized).exists() {
        fs::create_dir_all(&normalized).map_err(|_| {
            DomainStateError::bad_request(format!("Failed to create workspace root: {normalized}"))
        })?;
    }
    let metadata = fs::metadata(&normalized).map_err(|_| {
        DomainStateError::not_found(format!("{field} does not exist: {normalized}"))
    })?;
    if !metadata.is_dir() {
        return Err(DomainStateError::bad_request(format!(
            "{field} is not a directory: {normalized}"
        )));
    }
    Ok(normalized)
}

fn expand_user_path(path: &str) -> String {
    if path == "~" {
        return env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

pub(crate) fn resolve_path_syntax(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        path
    } else {
        normalized
    }
}

pub(crate) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub(crate) fn path_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}
