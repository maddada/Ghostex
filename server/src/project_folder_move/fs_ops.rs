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
    let from_c = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to_c = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both are valid NUL-terminated paths for the duration of the call.
    let result = unsafe { libc::renamex_np(from_c.as_ptr(), to_c.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    // CDXC:Projects 2026-11-20 WHY: exFAT, SMB and other volumes without RENAME_EXCL answer ENOTSUP or EINVAL; like Linux, check, then the plain rename (Opus review, finding 7). The window between the two is the move lock's and the checks' to keep closed.
    if matches!(
        error.raw_os_error(),
        Some(libc::ENOTSUP) | Some(libc::EINVAL)
    ) {
        if std::fs::symlink_metadata(to).is_ok() {
            return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
        }
        return std::fs::rename(from, to);
    }
    Err(error)
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

/// The other processes using the folder: those whose working folder is inside it (risk 8), and
/// those with a file (or their own program) open inside it (Opus review, finding 6), each as
/// `name (pid N)` and listed once.
#[derive(Default, Debug)]
pub(crate) struct OpenInFolder {
    pub(crate) cwd: Vec<String>,
    pub(crate) files: Vec<String>,
}

/// macOS indexers and backup daemons that open files for a moment; they don't keep the folder.
const PASSING_READERS: &[&str] = &[
    "mds",
    "mds_stores",
    "mdworker",
    "mdworker_shared",
    "fseventsd",
    "revisiond",
    "backupd",
];

impl OpenInFolder {
    fn add(&mut self, cwd: bool, command: &str, pid: u32) {
        let name = format!("{command} (pid {pid})");
        if cwd {
            self.files.retain(|known| known != &name);
            if !self.cwd.contains(&name) {
                self.cwd.push(name);
            }
        } else if !self.cwd.contains(&name)
            && !self.files.contains(&name)
            && !PASSING_READERS.contains(&command)
        {
            self.files.push(name);
        }
    }
}

/// Every process other than gxserver using a path `contains` accepts: macOS reads one `lsof` of
/// every open file (not `+D`, which walks the whole folder), Linux `/proc/*/{cwd,exe,fd}`. `Err`
/// when that ran past `limit`. Windows has no cheap way and needs none: the rename itself fails
/// there while a process holds anything inside.
pub(crate) fn processes_using(
    contains: &dyn Fn(&str) -> bool,
    limit: std::time::Duration,
) -> Result<OpenInFolder, ()> {
    let own = std::process::id();
    let mut found = OpenInFolder::default();
    #[cfg(target_os = "macos")]
    {
        let Some(text) = output_within(
            std::process::Command::new("/usr/sbin/lsof").args(["-nP", "-w", "-Fpcfn"]),
            limit,
        )?
        else {
            return Ok(found);
        };
        let (mut pid, mut command, mut fd) = (0u32, String::new(), String::new());
        for line in text.lines() {
            match line.split_at_checked(1) {
                Some(("p", rest)) => pid = rest.parse().unwrap_or(0),
                Some(("c", rest)) => command = rest.to_string(),
                Some(("f", rest)) => fd = rest.to_string(),
                Some(("n", rest)) if pid != own && contains(rest) => {
                    found.add(fd == "cwd", &command, pid)
                }
                _ => {}
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let started = std::time::Instant::now();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Ok(found);
        };
        for entry in entries.flatten() {
            if started.elapsed() > limit {
                return Err(());
            }
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if pid == own {
                continue;
            }
            let command = || {
                std::fs::read_to_string(entry.path().join("comm"))
                    .map(|name| name.trim().to_string())
                    .unwrap_or_else(|_| "process".to_string())
            };
            let inside = |link: std::path::PathBuf| {
                std::fs::read_link(link).is_ok_and(|target| contains(&target.to_string_lossy()))
            };
            if inside(entry.path().join("cwd")) {
                found.add(true, &command(), pid);
                continue;
            }
            let mut links = vec![entry.path().join("exe")];
            if let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) {
                links.extend(fds.flatten().map(|fd| fd.path()));
            }
            if links.into_iter().any(inside) {
                found.add(false, &command(), pid);
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (own, contains, limit);
    Ok(found)
}

/// Runs `command` and returns its standard output, `Ok(None)` when it could not start, or `Err`
/// after killing it when it ran past `limit`.
#[cfg(target_os = "macos")]
fn output_within(
    command: &mut std::process::Command,
    limit: std::time::Duration,
) -> Result<Option<String>, ()> {
    use std::io::Read as _;
    let Ok(mut child) = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return Ok(None);
    };
    let mut stdout = child.stdout.take();
    // Read on a thread so a full pipe never stops the program while it is being waited for.
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_string(&mut text);
        }
        text
    });
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    Ok(reader.join().ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rename_never_replaces_an_existing_folder() {
        let temp = tempfile::tempdir().unwrap();
        let from = temp.path().join("App");
        let to = temp.path().join("Kiwi");
        std::fs::create_dir(&from).unwrap();
        std::fs::create_dir(&to).unwrap();
        std::fs::write(to.join("keep.txt"), "kept").unwrap();
        let error = rename_no_replace(&from, &to, false).unwrap_err();
        assert!(
            error.kind() == std::io::ErrorKind::AlreadyExists
                || error.raw_os_error() == Some(libc_eexist()),
            "{error}"
        );
        assert!(from.is_dir());
        assert_eq!(
            std::fs::read_to_string(to.join("keep.txt")).unwrap(),
            "kept"
        );
        std::fs::remove_dir_all(&to).unwrap();
        rename_no_replace(&from, &to, false).unwrap();
        assert!(!from.exists() && to.is_dir());
    }

    fn libc_eexist() -> i32 {
        #[cfg(unix)]
        return libc::EEXIST;
        #[allow(unreachable_code)]
        80
    }

    /// Opus review, finding 6: a program with a file open in the folder is found, not only one
    /// whose working folder is inside.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn programs_using_the_folder_are_found_by_working_folder_and_by_open_file() {
        let temp = tempfile::tempdir().unwrap();
        let folder = std::fs::canonicalize(temp.path()).unwrap().join("App");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("notes.txt"), "x").unwrap();
        let mut inside = std::process::Command::new("sleep")
            .arg("60")
            .current_dir(&folder)
            .spawn()
            .unwrap();
        let mut holding = std::process::Command::new("tail")
            .args(["-f", folder.join("notes.txt").to_str().unwrap()])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let prefix = folder.to_string_lossy().to_string();
        let found = processes_using(
            &|path| path == prefix || path.starts_with(&format!("{prefix}/")),
            std::time::Duration::from_secs(30),
        );
        let _ = inside.kill();
        let _ = holding.kill();
        let _ = (inside.wait(), holding.wait());
        let found = found.unwrap();
        assert!(
            found.cwd.contains(&format!("sleep (pid {})", inside.id())),
            "{found:?}"
        );
        assert!(
            found
                .files
                .contains(&format!("tail (pid {})", holding.id())),
            "{found:?}"
        );
    }
}
