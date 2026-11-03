//! The candidates of a project's "Send to the box" list (Q1020 "ticked list"): its `.env*` files.
//! Only names are listed here; values are read at boot and never stored in the project settings.

use std::path::Path;

/// Folders a search for env files never enters.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".turbo",
    ".cache",
    ".venv",
    "venv",
    "vendor",
    ".dependencies",
];
/// Deep enough for `apps/web/.env.local`, shallow enough to stay fast on a big repository.
const MAX_DEPTH: usize = 3;
const MAX_FILES: usize = 60;

/// Project-relative paths of the `.env*` files under `root`, sorted. Blocking.
pub(super) fn env_files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    walk(root, root, 0, &mut found);
    found.sort();
    found
}

fn walk(root: &Path, dir: &Path, depth: usize, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        if found.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if depth < MAX_DEPTH && !SKIPPED_DIRS.contains(&name.as_str()) {
                folders.push(entry.path());
            }
        } else if kind.is_file() && is_env_file(&name) {
            if let Ok(relative) = entry.path().strip_prefix(root) {
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for folder in folders {
        walk(root, &folder, depth + 1, found);
    }
}

/// `.env`, `.env.local`, `.env.production`, … but not templates that hold no secret.
fn is_env_file(name: &str) -> bool {
    (name == ".env" || name.starts_with(".env."))
        && ![".example", ".sample", ".template", ".dist"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}
