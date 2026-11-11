//! The change token: tells, without starting bd, whether a Beads workspace changed since it was
//! last read.

use std::fs;
use std::path::{Path, PathBuf};

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// An embedded Dolt workspace keeps every write in `<db>/.dolt/noms/`: the manifest's content and the chunk journal's length (`vvvv…` and `journal.idx`). Every bd process is short-lived, so a write that outlives it, committed or left in the working set by `--dolt-auto-commit off|batch`, lands in those files, while reads (`list`, `show`, `ready`, `where`, `config get`) leave them as they were; proved on bd 1.3.4 on 2026-11-11. Modification times are never part of the token because reads touch them, and the Dolt commit alone misses working-set writes. A server-mode workspace has no local files, so it has no token and re-reads on an interval instead.
pub(crate) type ChangeToken = String;

/// The `.dolt/noms` folder of an embedded workspace, found under `bd where`'s `database_path`
/// (`.beads/embeddeddolt`, which holds one folder per database).
pub(crate) fn embedded_noms_dir(database_path: &str) -> Option<PathBuf> {
    if database_path.trim().is_empty() {
        return None;
    }
    let root = Path::new(database_path);
    let mut candidates = vec![root.join(".dolt").join("noms")];
    if let Ok(entries) = fs::read_dir(root) {
        let mut folders: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path().join(".dolt").join("noms"))
            .collect();
        folders.sort();
        candidates.extend(folders);
    }
    candidates
        .into_iter()
        .find(|noms| noms.join("manifest").is_file())
}

/// The token: the manifest's bytes plus every other storage file's name and length. `None` when
/// the folder cannot be read, which the caller treats like a server-mode workspace.
pub(crate) fn read_change_token(noms: &Path) -> Option<ChangeToken> {
    let manifest = fs::read(noms.join("manifest")).ok()?;
    let mut files: Vec<(String, u64)> = fs::read_dir(noms)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "LOCK" || name == "manifest" {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then_some((name, metadata.len()))
        })
        .collect();
    files.sort();
    let mut token = String::with_capacity(manifest.len() * 2 + files.len() * 48);
    for byte in manifest {
        token.push_str(&format!("{byte:02x}"));
    }
    for (name, len) in files {
        token.push(';');
        token.push_str(&name);
        token.push(':');
        token.push_str(&len.to_string());
    }
    Some(token)
}
