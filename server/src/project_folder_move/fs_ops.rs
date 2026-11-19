//! The disk side of a folder move: the rename itself (never over an existing folder), and the
//! facts the checks read (same folder, same volume, links, processes inside, sync folders).

use std::path::{Path, PathBuf};

/// Renames `from` to `to`, failing instead of replacing anything at `to` (risk 16): macOS
/// `renamex_np(RENAME_EXCL)`, Linux `renameat2(RENAME_NOREPLACE)`, Windows `MoveFileExW` without
/// `MOVEFILE_REPLACE_EXISTING`. `case_only` renames a folder to its own name in other capitals,
/// which these calls would refuse as "exists", with the plain rename.
pub(crate) fn rename_no_replace(from: &Path, to: &Path, case_only: bool) -> std::io::Result<()> {
    if case_only {
        return std::fs::rename(from, to);
    }
    rename_exclusive(from, to)
}

#[cfg(target_os = "macos")]
fn rename_exclusive(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both are valid NUL-terminated paths for the duration of the call.
    let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn rename_exclusive(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    let from_c = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to_c = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    const RENAME_NOREPLACE: libc::c_uint = 1;
    // SAFETY: both are valid NUL-terminated paths for the duration of the call.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    // A filesystem without the flag (some network mounts): check, then the plain rename.
    if matches!(
        error.raw_os_error(),
        Some(libc::EINVAL) | Some(libc::ENOSYS)
    ) {
        if std::fs::symlink_metadata(to).is_ok() {
            return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
        }
        return std::fs::rename(from, to);
    }
    Err(error)
}

#[cfg(windows)]
fn rename_exclusive(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    let wide = |path: &Path| -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both are valid NUL-terminated wide strings for the duration of the call; flags 0
    // means fail when `to` exists.
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
    };
    if ok != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn rename_exclusive(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

/// Windows "access denied" (5) or "in use by another process" (32): something has a file open.
pub(crate) fn is_in_use_error(error: &std::io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(5) | Some(32))
}

/// `a` and `b` are one folder on disk (the same device and inode; on Windows, the same path
/// ignoring letter case, since NTFS ignores it).
pub(crate) fn same_folder_on_disk(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        b.exists() && a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    }
}

/// `folder` and `other_folder` are on one volume, so a rename moves without copying.
pub(crate) fn same_volume(folder: &Path, other_folder: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        match (std::fs::metadata(folder), std::fs::metadata(other_folder)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let prefix = |path: &Path| {
            path.components()
                .next()
                .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        };
        prefix(folder) == prefix(other_folder)
    }
}

/// The path itself is a symbolic link or a Windows junction (risk 10). Links in its parents are
/// fine.
pub(crate) fn is_link(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[allow(unreachable_code)]
    false
}

/// Processes other than gxserver whose working folder is inside the folder (risk 8), as
/// `name (pid N)`. macOS reads `lsof`, Linux `/proc/*/cwd`; Windows has no cheap way and needs
/// none, because the rename itself fails there while a process is inside.
pub(crate) fn processes_inside(contains: &dyn Fn(&str) -> bool) -> Vec<String> {
    let own = std::process::id();
    let mut found = Vec::new();
    #[cfg(target_os = "macos")]
    {
        let Ok(output) = std::process::Command::new("/usr/sbin/lsof")
            .args(["-nP", "-a", "-d", "cwd", "-Fpcn"])
            .output()
        else {
            return found;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let (mut pid, mut command) = (0u32, String::new());
        for line in text.lines() {
            match line.split_at_checked(1) {
                Some(("p", rest)) => pid = rest.parse().unwrap_or(0),
                Some(("c", rest)) => command = rest.to_string(),
                Some(("n", rest)) if pid != own && contains(rest) => {
                    found.push(format!("{command} (pid {pid})"))
                }
                _ => {}
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return found;
        };
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if pid == own {
                continue;
            }
            let Ok(cwd) = std::fs::read_link(entry.path().join("cwd")) else {
                continue;
            };
            if contains(&cwd.to_string_lossy()) {
                let command = std::fs::read_to_string(entry.path().join("comm"))
                    .map(|name| name.trim().to_string())
                    .unwrap_or_else(|_| "process".to_string());
                found.push(format!("{command} (pid {pid})"));
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (own, contains);
    found
}

/// The cloud-sync folder `path` is in, if any (risk 9): iCloud Drive and File Provider folders
/// (`~/Library/Mobile Documents`, `~/Library/CloudStorage/<provider>`) on macOS; OneDrive, Dropbox
/// and Google Drive on Windows.
pub(crate) fn sync_root_of(path: &Path, home: &Path) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        roots.push(home.join("Library").join("Mobile Documents"));
        if let Ok(entries) = std::fs::read_dir(home.join("Library").join("CloudStorage")) {
            roots.extend(entries.flatten().map(|entry| entry.path()));
        }
    }
    if cfg!(windows) {
        for variable in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
            if let Some(value) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
                roots.push(PathBuf::from(value));
            }
        }
        roots.push(home.join("Dropbox"));
        roots.push(home.join("Google Drive"));
        roots.push(home.join("My Drive"));
    }
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    roots.into_iter().find(|root| {
        let root_text = lower(root);
        let path_text = lower(path);
        path_text == root_text
            || path_text.starts_with(&format!("{root_text}/"))
            || path_text.starts_with(&format!("{root_text}\\"))
    })
}

/// A file under `folder` the sync client has not downloaded (macOS `SF_DATALESS`; Windows
/// `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN` or `OFFLINE`). `Err` when the walk ran out of time,
/// which the caller treats like a placeholder: a move it can't vouch for is refused.
pub(crate) fn first_placeholder(
    folder: &Path,
    limit: std::time::Duration,
) -> Result<Option<PathBuf>, ()> {
    let started = std::time::Instant::now();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if started.elapsed() > limit {
                return Err(());
            }
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if is_placeholder(&metadata) {
                return Ok(Some(path));
            }
            if metadata.is_dir() {
                stack.push(path);
            }
        }
    }
    Ok(None)
}

fn is_placeholder(metadata: &std::fs::Metadata) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt as _;
        const SF_DATALESS: u32 = 0x4000_0000;
        return metadata.st_flags() & SF_DATALESS != 0;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x4_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
        return metadata.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS)
            != 0;
    }
    #[allow(unreachable_code)]
    {
        let _ = metadata;
        false
    }
}

/// On Windows, a warning when the new folder's longer name would push files under it past 260
/// characters while long paths are off (risk 16). `None` elsewhere or when nothing would.
pub(crate) fn long_path_warning(old: &Path, new: &Path) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let growth = new.as_os_str().len() as isize - old.as_os_str().len() as isize;
    if growth <= 0 {
        return None;
    }
    let enabled = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\FileSystem",
            "/v",
            "LongPathsEnabled",
        ])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains("0x1"))
        .unwrap_or(false);
    if enabled {
        return None;
    }
    let started = std::time::Instant::now();
    let mut longest = 0usize;
    let mut stack = vec![old.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if started.elapsed() > std::time::Duration::from_secs(5) {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            longest = longest.max(path.as_os_str().len());
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                stack.push(path);
            }
        }
    }
    let after = longest as isize + growth;
    (after > 259).then(|| {
        format!("Some files will have paths of about {after} characters after the move, past Windows' 260 limit, and long paths are off on this computer. Turn on long paths (LongPathsEnabled) and run git config --global core.longpaths true, or pick a shorter name.")
    })
}

/// The folders a project must never move to or from.
pub(crate) fn is_system_folder(path: &Path, home: &Path) -> bool {
    let text = path.to_string_lossy().to_lowercase();
    let trimmed = text.trim_end_matches(['/', '\\']);
    if path.parent().is_none() || path == home {
        return true;
    }
    if cfg!(windows) {
        let windows_roots = [
            "c:\\windows",
            "c:\\program files",
            "c:\\program files (x86)",
            "c:\\programdata",
        ];
        return windows_roots
            .iter()
            .any(|root| trimmed == *root || trimmed.starts_with(&format!("{root}\\")));
    }
    let unix_roots = [
        "/system",
        "/usr",
        "/bin",
        "/sbin",
        "/etc",
        "/var",
        "/private/var",
        "/private/etc",
        "/library",
        "/applications",
        "/opt",
        "/dev",
        "/proc",
        "/sys",
        "/boot",
    ];
    let home_text = home.to_string_lossy().to_lowercase();
    trimmed == home_text.trim_end_matches('/')
        || unix_roots
            .iter()
            .any(|root| trimmed == *root || trimmed.starts_with(&format!("{root}/")))
}
