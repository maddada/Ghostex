//! Filesystem helpers shared by scan, plan, and apply: symlink inspection,
//! relative link creation, backup names, and content hashing.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The raw symlink target when `path` is a symlink.
pub fn link_target(path: &Path) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_symlink() {
        return None;
    }
    fs::read_link(path).ok()
}

pub fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

/// A symlink whose target cannot be resolved (missing, or a loop such as a
/// link pointing at itself).
pub fn is_dangling_link(path: &Path) -> bool {
    is_symlink(path) && fs::metadata(path).is_err()
}

/// True when both paths resolve to the same filesystem entry.
pub fn same_entry(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Lexically normalize `path` (collapse `.` and `..`) without touching disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The absolute path a link points at, resolving relative targets against the
/// link's parent folder (lexically, so dangling links still get a path).
pub fn absolute_link_target(link: &Path) -> Option<PathBuf> {
    let target = link_target(link)?;
    if target.is_absolute() {
        return Some(normalize(&target));
    }
    Some(normalize(&link.parent()?.join(target)))
}

/// A relative path from `from_dir` to `to`, both absolute. Falls back to the
/// absolute target when they share no root.
pub fn relative_path(from_dir: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = normalize(from_dir)
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    let to_parts: Vec<_> = normalize(to)
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    let mut common = 0;
    while common < from.len() && common < to_parts.len() && from[common] == to_parts[common] {
        common += 1;
    }
    if common == 0 {
        return to.to_path_buf();
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to_parts[common..] {
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// The link target to write for a per-skill link at `link` pointing at
/// `target`: relative to the *real* parent folder of the link, so a link inside
/// a symlinked folder still resolves (the `skills` CLI's `resolveParentSymlinks`
/// rule). Windows keeps absolute targets because directory symlinks there do
/// not resolve relative targets reliably.
pub fn link_target_for(link: &Path, target: &Path) -> PathBuf {
    if cfg!(windows) {
        return target.to_path_buf();
    }
    let parent = link.parent().map(Path::to_path_buf).unwrap_or_default();
    let real_parent = fs::canonicalize(&parent).unwrap_or(parent);
    let real_target = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    relative_path(&real_parent, &real_target)
}

/// What a link ended up being on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Symlink,
    /// A Windows directory junction: reads as a link (`is_symlink`, `read_link`, `canonicalize`
    /// and `remove_link` all treat it as one), so nothing that checks links needs to know.
    Junction,
    /// A plain copy of a single file: only its bytes say it is Ghostex's (`same_file_content`).
    Copy,
}

/// Windows refuses a symbolic link to an app that is not an administrator unless Developer Mode
/// is on: `ERROR_PRIVILEGE_NOT_HELD` (1314), or access denied.
pub fn symlink_refused(error: &io::Error) -> bool {
    error.raw_os_error() == Some(1314) || error.kind() == io::ErrorKind::PermissionDenied
}

/// `target` made absolute against the link's folder (a junction cannot hold a relative target).
fn absolute_target(target: &Path, link: &Path) -> PathBuf {
    if target.is_absolute() {
        return target.to_path_buf();
    }
    normalize(&link.parent().unwrap_or(Path::new("")).join(target))
}

fn both_failed(first: io::Error, kind: &str, second: io::Error) -> io::Error {
    io::Error::new(
        first.kind(),
        format!("{first}; a {kind} did not work either: {second}"),
    )
}

/// A folder link: a symbolic link, else (when Windows refused it) a directory junction. Generic
/// over the two OS calls so the decision is tested on every platform.
///
/// CDXC:AgentSync 2026-10-27 DECISION:
/// User (Q614), told that a Windows tester's hooks stopped linking until Developer Mode was on because Ghostex only made symbolic links: "Fall back to a junction, restore on failure". A refused folder link becomes a directory junction, which Windows allows without Developer Mode, and a refused file link a copy; when even that fails, whatever was moved or removed to make room is put back (apply.rs, server/src/agent_skills.rs).
pub fn link_dir_with(
    target: &Path,
    link: &Path,
    symlink: impl FnOnce(&Path, &Path) -> io::Result<()>,
    junction: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<LinkKind> {
    match symlink(target, link) {
        Ok(()) => Ok(LinkKind::Symlink),
        Err(error) if symlink_refused(&error) => {
            match junction(&absolute_target(target, link), link) {
                Ok(()) => Ok(LinkKind::Junction),
                Err(second) => Err(both_failed(error, "directory junction", second)),
            }
        }
        Err(error) => Err(error),
    }
}

/// A single-file link: a symbolic link, else (when Windows refused it) a copy of the file.
pub fn link_file_with(
    target: &Path,
    link: &Path,
    symlink: impl FnOnce(&Path, &Path) -> io::Result<()>,
    copy: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<LinkKind> {
    match symlink(target, link) {
        Ok(()) => Ok(LinkKind::Symlink),
        Err(error) if symlink_refused(&error) => match copy(&absolute_target(target, link), link) {
            Ok(()) => Ok(LinkKind::Copy),
            Err(second) => Err(both_failed(error, "copy", second)),
        },
        Err(error) => Err(error),
    }
}

/// Both are regular files with the same bytes: a file link Windows made as a copy
/// (`link_file_with`) still counts as linked while it matches what it copied.
pub fn same_file_content(a: &Path, b: &Path) -> bool {
    let regular = |path: &Path| {
        fs::symlink_metadata(path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    };
    if !regular(a) || !regular(b) {
        return false;
    }
    match (fs::read(a), fs::read(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(unix)]
pub fn create_dir_link(target: &Path, link: &Path) -> io::Result<LinkKind> {
    std::os::unix::fs::symlink(target, link).map(|()| LinkKind::Symlink)
}

#[cfg(windows)]
pub fn create_dir_link(target: &Path, link: &Path) -> io::Result<LinkKind> {
    link_dir_with(
        target,
        link,
        |target, link| std::os::windows::fs::symlink_dir(target, link),
        |target, link| junction::create(target, link),
    )
}

#[cfg(unix)]
pub fn create_file_link(target: &Path, link: &Path) -> io::Result<LinkKind> {
    std::os::unix::fs::symlink(target, link).map(|()| LinkKind::Symlink)
}

#[cfg(windows)]
pub fn create_file_link(target: &Path, link: &Path) -> io::Result<LinkKind> {
    link_file_with(
        target,
        link,
        |target, link| std::os::windows::fs::symlink_file(target, link),
        |target, link| fs::copy(target, link).map(|_| ()),
    )
}

/// Remove a symlink itself, never what it points at.
pub fn remove_link(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_symlink() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a symlink"));
    }
    #[cfg(windows)]
    {
        if fs::remove_dir(path).is_ok() {
            return Ok(());
        }
    }
    fs::remove_file(path)
}

/// Where Agent Sync moves anything in its way: Ghostex's state folder, never the
/// agent's own folders, so a backed-up skill is not picked up as a second skill.
pub fn default_backup_root() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("agent-sync-backups")
}

/// `<backup_root>/<stamp>/<path relative to home>`, so every backup can be put
/// back by moving it to the same place under HOME.
pub fn backup_path(path: &Path, home: &Path, backup_root: &Path, stamp: &str) -> PathBuf {
    let relative = path.strip_prefix(home).unwrap_or(path);
    let mut target = backup_root.join(stamp);
    for component in relative.components() {
        if let std::path::Component::Normal(part) = component {
            target.push(part);
        }
    }
    target
}

/// SHA-256 over every regular file in `dir` (sorted relative path plus bytes),
/// following symlinks and skipping `.DS_Store`. `None` when unreadable.
pub fn dir_content_hash(dir: &Path) -> Option<String> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files).ok()?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(dir.join(&relative)).ok()?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Some(hex::encode(hasher.finalize()))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".DS_Store" {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            collect_files(root, &path, out)?;
        } else if metadata.is_file() {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}

/// Sorted child names of a folder, skipping `.DS_Store`.
pub fn child_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name != ".DS_Store")
        .collect();
    names.sort();
    names
}

/// Display form with HOME shortened to `~`.
pub fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        // `/` between every part, so a Windows path built with `join` does not show as `~/.claude\hooks`.
        Ok(rest) => format!(
            "~/{}",
            rest.components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        ),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A fresh folder under the system temp folder, removed when dropped.
    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub(crate) fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            let dir = std::env::temp_dir().join(format!(
                "ghostex-agent-sync-{label}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn refused() -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "A required privilege is not held by the client. (os error 1314)",
        )
    }

    #[test]
    fn a_folder_link_is_a_symlink_when_the_os_allows_it() {
        let junction_called = RefCell::new(false);
        let kind = link_dir_with(
            Path::new("/home/u/.agents/hooks"),
            Path::new("/home/u/.claude/hooks"),
            |_, _| Ok(()),
            |_, _| {
                *junction_called.borrow_mut() = true;
                Ok(())
            },
        )
        .expect("linked");
        assert_eq!(kind, LinkKind::Symlink);
        assert!(!*junction_called.borrow());
    }

    #[test]
    fn a_refused_folder_link_becomes_a_junction_to_the_absolute_target() {
        let seen = RefCell::new(None);
        let kind = link_dir_with(
            Path::new("../.agents/hooks"),
            Path::new("/home/u/.claude/hooks"),
            |_, _| Err(refused()),
            |target, link| {
                *seen.borrow_mut() = Some((target.to_path_buf(), link.to_path_buf()));
                Ok(())
            },
        )
        .expect("junction");
        assert_eq!(kind, LinkKind::Junction);
        assert_eq!(
            seen.into_inner(),
            Some((
                PathBuf::from("/home/u/.agents/hooks"),
                PathBuf::from("/home/u/.claude/hooks")
            ))
        );
        // Windows' own code for a missing privilege counts too.
        let kind = link_dir_with(
            Path::new("/t"),
            Path::new("/l"),
            |_, _| Err(io::Error::from_raw_os_error(1314)),
            |_, _| Ok(()),
        );
        assert_eq!(kind.ok(), Some(LinkKind::Junction));
    }

    #[test]
    fn when_the_junction_fails_too_both_reasons_are_kept() {
        let error = link_dir_with(
            Path::new("/t"),
            Path::new("/l"),
            |_, _| Err(refused()),
            |_, _| Err(io::Error::other("the target is on a network share")),
        )
        .expect_err("both failed");
        let message = error.to_string();
        assert!(message.starts_with("A required privilege is not held by the client."));
        assert!(message.contains(
            "a directory junction did not work either: the target is on a network share"
        ));
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn other_errors_do_not_fall_back() {
        let junction_called = RefCell::new(false);
        let error = link_dir_with(
            Path::new("/t"),
            Path::new("/l"),
            |_, _| Err(io::Error::new(io::ErrorKind::AlreadyExists, "exists")),
            |_, _| {
                *junction_called.borrow_mut() = true;
                Ok(())
            },
        )
        .expect_err("not refused");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(!*junction_called.borrow());
    }

    #[test]
    fn a_refused_file_link_becomes_a_copy_that_counts_as_linked() {
        let temp = TempDir::new("file-copy");
        let source = temp.0.join("source.json");
        let link = temp.0.join("agent.json");
        fs::write(&source, b"{\"skills\":{}}").expect("source");
        let kind = link_file_with(
            &source,
            &link,
            |_, _| Err(refused()),
            |target, link| fs::copy(target, link).map(|_| ()),
        )
        .expect("copied");
        assert_eq!(kind, LinkKind::Copy);
        assert!(same_file_content(&link, &source));
        fs::write(&source, b"{\"skills\":{\"x\":{}}}").expect("source changed");
        assert!(!same_file_content(&link, &source));
        assert!(!same_file_content(&temp.0, &source));
    }
}
