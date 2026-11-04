//! The crash-safe way to replace a file.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Replaces `path` with `bytes` so that a crash or power loss at any moment leaves either the whole
/// old file or the whole new one.
///
/// CDXC:Settings 2026-10-31 WHY:
/// A Windows PC that lost power wiped every setting and saved remote machine. The save wrote a temp file and renamed it with no flush, and a rename only journals the name: NTFS commits the rename but not the data behind it, so the settings file came back empty or filled with NUL bytes. POSIX filesystems have the same hole. So the temp file is flushed before the rename, and the rename itself is flushed afterwards so a save that returned survives a power cut. `MoveFileExW` with `MOVEFILE_WRITE_THROUGH` is not used: Microsoft documents that flag only for a move done as copy and delete, never for this same-folder rename, while flushing the replaced file commits the NTFS log, which holds the rename. `ReplaceFileW` is not used either: it can fail halfway and leave the target missing (`ERROR_UNABLE_TO_MOVE_REPLACEMENT_2`).
/// SEE-ALSO: packages/paths/src/settings_file.rs.
pub fn write_durably(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = parent_folder(path);
    fs::create_dir_all(parent)?;
    let temporary = temporary_path(path);
    let replaced = write_and_flush(&temporary, bytes)
        .and_then(|()| retry_while_busy(|| fs::rename(&temporary, path)));
    if let Err(error) = replaced {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    flush_replacement(path);
    Ok(())
}

/// Removes temp files [`write_durably`] left beside `path` when a crash stopped it before the
/// rename. Files younger than `older_than` may belong to a save in progress and stay.
pub fn remove_stale_temporary_files(path: &Path, older_than: Duration) {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return;
    };
    let prefix = format!(".{stem}.");
    let Ok(entries) = fs::read_dir(parent_folder(path)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".tmp") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > older_than));
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Runs `operation`, trying again for a moment while another program holds the file: on Windows a
/// virus scanner or a reader without delete sharing refuses access briefly. Elsewhere it runs once.
pub(crate) fn retry_while_busy<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut wait = Duration::from_millis(5);
    for _ in 0..5 {
        match operation() {
            Err(error) if is_busy(&error) => std::thread::sleep(wait),
            result => return result,
        }
        wait *= 2;
    }
    operation()
}

#[cfg(windows)]
fn is_busy(error: &io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    matches!(
        error.raw_os_error(),
        Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    )
}

#[cfg(not(windows))]
fn is_busy(_error: &io::Error) -> bool {
    false
}

fn write_and_flush(temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// The new file is complete and in place whatever happens here, so a failure costs only the
/// guarantee that it outlives a power cut in the next few seconds; the old file would come back.
#[cfg(windows)]
fn flush_replacement(path: &Path) {
    if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
        let _ = file.sync_all();
    }
}

/// See the Windows twin. Some filesystems refuse to sync a folder, which is why this is best effort.
#[cfg(not(windows))]
fn flush_replacement(path: &Path) {
    if let Ok(folder) = fs::File::open(parent_folder(path)) {
        let _ = folder.sync_all();
    }
}

fn parent_folder(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn temporary_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_else(|| "file".into());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    parent_folder(path).join(format!(
        ".{stem}.{}.{stamp}.{}{extension}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}
