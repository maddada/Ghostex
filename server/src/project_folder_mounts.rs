//! Which projects' folders the 15-second folder watch (project_folder_problem.rs) may read, worked
//! out from the kernel's mount table alone, so the watch never wakes a sleeping disk and never
//! hangs on a network share that stopped answering.
//!
//! CDXC:Projects 2026-11-20 WHY: The security review asked how the watch behaves on a sleeping
//! external disk or a dead network mount. A stat every 15 seconds would keep an external disk from
//! ever sleeping (or spin it up), and a stat on a hard-mounted share whose server is gone blocks
//! for minutes, stalling the watch. So the watch reads only folders on the startup disk; a folder
//! on another local disk only while its project wears the mark (to see the disk come back: a disk
//! that was just mounted is awake), and a folder on a network share never. The mount table is read
//! without asking any file system (`getmntinfo(MNT_NOWAIT)` on a Mac, /proc/self/mounts on Linux),
//! and symlinks are followed only while they sit on the startup disk. An unplugged drive needs no
//! read of the drive: its `/Volumes/<name>` is then a plain path on the startup disk.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// How the watch may treat one project's folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchRead {
    /// On the startup disk: read it every pass.
    Read,
    /// On another local disk (an external drive, a disk image): read it only while it is marked
    /// missing.
    WhileMissing,
    /// On a network share, or where the mount table can't be read: the watch leaves it alone and
    /// it is read when a client asks for the project.
    Never,
}

#[derive(Clone, Debug)]
pub(crate) struct Mount {
    point: PathBuf,
    /// The startup disk (the root and, on a Mac, its /System/Volumes/* siblings).
    startup: bool,
    network: bool,
}

/// The mount table, or `None` where it can't be read without touching a file system.
pub(crate) fn mounts() -> Option<Vec<Mount>> {
    platform_mounts()
}

#[cfg(target_os = "macos")]
fn platform_mounts() -> Option<Vec<Mount>> {
    let mut table: *mut libc::statfs = std::ptr::null_mut();
    // MNT_NOWAIT answers from the kernel's copy: no file system is asked, so a dead share can't
    // block it.
    let count = unsafe { libc::getmntinfo(&mut table, libc::MNT_NOWAIT) };
    if count <= 0 || table.is_null() {
        return None;
    }
    let entries = unsafe { std::slice::from_raw_parts(table, count as usize) };
    Some(
        entries
            .iter()
            .map(|entry| {
                let point = unsafe { std::ffi::CStr::from_ptr(entry.f_mntonname.as_ptr()) }
                    .to_string_lossy()
                    .into_owned();
                let local = entry.f_flags & (libc::MNT_LOCAL as u32) != 0;
                Mount {
                    startup: point == "/" || point.starts_with("/System/Volumes/"),
                    network: !local,
                    point: PathBuf::from(point),
                }
            })
            .collect(),
    )
}

#[cfg(target_os = "linux")]
fn platform_mounts() -> Option<Vec<Mount>> {
    const NETWORK: &[&str] = &[
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "smbfs",
        "fuse.sshfs",
        "sshfs",
        "9p",
        "ceph",
        "glusterfs",
        "davfs",
        "fuse.rclone",
        "afs",
    ];
    let text = std::fs::read_to_string("/proc/self/mounts").ok()?;
    Some(
        text.lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let _device = fields.next()?;
                let point = fields.next()?.replace("\\040", " ");
                let kind = fields.next()?;
                let removable_place = point.starts_with("/media/")
                    || point.starts_with("/run/media/")
                    || point.starts_with("/mnt/");
                Some(Mount {
                    startup: !removable_place && !NETWORK.contains(&kind),
                    network: NETWORK.contains(&kind),
                    point: PathBuf::from(point),
                })
            })
            .collect(),
    )
}

/// Windows: the system drive is read every pass; another drive letter or a share only while
/// marked, and a UNC share never.
#[cfg(windows)]
fn platform_mounts() -> Option<Vec<Mount>> {
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    Some(vec![Mount {
        point: PathBuf::from(format!("{system}\\")),
        startup: true,
        network: false,
    }])
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn platform_mounts() -> Option<Vec<Mount>> {
    None
}

/// The mount `path` sits on: the one with the longest mount point it starts with.
fn mount_of<'a>(path: &Path, mounts: &'a [Mount]) -> Option<&'a Mount> {
    mounts
        .iter()
        .filter(|mount| path.starts_with(&mount.point))
        .max_by_key(|mount| mount.point.components().count())
}

fn on_startup_disk(path: &Path, mounts: &[Mount]) -> bool {
    mount_of(path, mounts).is_some_and(|mount| mount.startup)
}

/// `path` with its symlinks followed as long as each one sits on the startup disk; a link on any
/// other disk is left as it is, unread.
fn resolve_on_startup_disk(path: &Path, mounts: &[Mount]) -> PathBuf {
    let parts = |path: &Path| -> Vec<OsString> {
        path.components()
            .map(|component| component.as_os_str().to_os_string())
            .rev()
            .collect()
    };
    let mut rest = parts(path);
    let mut current = PathBuf::new();
    let mut followed = 0;
    while let Some(part) = rest.pop() {
        if part == ".." {
            current.pop();
            continue;
        }
        let next = current.join(&part);
        if !on_startup_disk(&next, mounts) || followed > 40 {
            current = next;
            continue;
        }
        let link = std::fs::symlink_metadata(&next)
            .ok()
            .filter(|meta| meta.file_type().is_symlink())
            .and_then(|_| std::fs::read_link(&next).ok());
        match link {
            Some(target) => {
                followed += 1;
                if target.is_absolute() {
                    current = PathBuf::new();
                }
                rest.extend(parts(&target));
            }
            None => current = next,
        }
    }
    current
}

/// How the watch may treat the folder `path`.
pub(crate) fn watch_read(path: &str, mounts: Option<&[Mount]>) -> WatchRead {
    let Some(mounts) = mounts else {
        return WatchRead::Never;
    };
    let path = Path::new(path.trim());
    #[cfg(windows)]
    {
        let unc = path.to_string_lossy().starts_with(r"\\");
        if unc {
            return WatchRead::Never;
        }
    }
    if !on_startup_disk(path, mounts) {
        return match mount_of(path, mounts) {
            Some(mount) if mount.network => WatchRead::Never,
            _ => WatchRead::WhileMissing,
        };
    }
    let real = resolve_on_startup_disk(path, mounts);
    match mount_of(&real, mounts) {
        Some(mount) if mount.startup => WatchRead::Read,
        Some(mount) if mount.network => WatchRead::Never,
        _ => WatchRead::WhileMissing,
    }
}
