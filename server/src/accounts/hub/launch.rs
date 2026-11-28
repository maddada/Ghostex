//! Starting a Claude or Codex session On the hub: the session runs in a profile folder of
//! Ghostex's own for that hub (`~/.claude-profiles/ghostex-<hub>`, `~/.codex-profiles/ghostex-<hub>`),
//! which points the CLI at the hub with its client key, and the command only names that folder
//! (`CLAUDE_CONFIG_DIR=… claude`, `CODEX_HOME=… codex`).
//!
//! CDXC:AgentProviders 2026-11-18 WHY:
//! Naming the profile folder in the command is what makes a hub session "uses its own login" (launch.rs `uses_own_login`): it is never wrapped in Claude Swap or Codex Swap, never moved to another account at a limit, and resume and fork keep the folder. The key stays out of the command, the terminal and the session's saved settings: Claude reads it through `apiKeyHelper` from a 0600 file in the folder, Codex from the folder's 0600 `config.toml` (`experimental_bearer_token`). The folders sit where Ghostex already looks for profiles (hooks, transcripts, folder trust, skills), so hub sessions get all of them like any profile.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use ghostex_agent_sync::fsx;
use serde_json::{json, Map, Value};

use crate::accounts::model::{NewSessionAccount, Provider, Registry};
use crate::agents::quote_shell_arg;
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// The runtime setting a hub session carries: its hub's id (the limit wait reads the hub's pool).
pub(crate) const RUNTIME_KEY: &str = "cliproxyHubId";
/// What a launcher sends as the account to start a session on a hub: `cliproxy:<hub id>`.
pub(crate) const ACCOUNT_PREFIX: &str = "cliproxy:";
const KEY_FILE: &str = ".ghostex-hub-key";
const CODEX_PROVIDER: &str = "ghostex-hub";

pub(crate) fn profile_dir(home: &Path, provider: Provider, hub_id: &str) -> PathBuf {
    let parent = match provider {
        Provider::Claude => ".claude-profiles",
        Provider::Codex => ".codex-profiles",
    };
    home.join(parent).join(format!("ghostex-{hub_id}"))
}

fn private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes `body` to `path` readable by this user only, through a rename. Each write has a temp
/// name of its own, so two launches preparing the same profile folder at once never write into or
/// rename away each other's file.
fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(
        "{name}.{}-{}.ghostex-tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

fn read_object(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn io(error: std::io::Error) -> DomainStateError {
    DomainStateError::bad_request(format!("Couldn't set up the hub's profile folder: {error}"))
}

/// The `apiKeyHelper` command that prints the key file.
///
/// CDXC:AgentProviders 2026-11-19 WHY:
/// On native Windows Claude Code runs `apiKeyHelper` through `cmd.exe` (its `ComSpec`), not PowerShell or bash: a probe on Windows with Claude Code 2.1.328 saw `%OS%` expanded and `$0` left as text, and the `cat '…'` helper failed with "'cat' is not recognized", so a session On the hub had no key. cmd's own `type "…"` prints the file there; everywhere else the helper runs in a POSIX shell.
fn key_helper(key_file: &Path) -> String {
    let path = key_file.to_string_lossy();
    if cfg!(windows) {
        format!("type \"{path}\"")
    } else {
        format!("cat {}", quote_shell_arg(&path))
    }
}

/// Same migration lock filename `cswap --share-history` uses inside `~/.claude`, so Ghostex hub
/// launches and `cswap` serialize against each other. Never unlinked once created.
const MIGRATION_LOCK_FILE: &str = ".cswap-history-migration.lock";
const MIGRATION_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const STAGED_LINK_SUFFIX: &str = ".ghostex-link-staging";

struct MigrationLock {
    _process_guard: MutexGuard<'static, ()>,
    #[cfg(unix)]
    file: fs::File,
    #[cfg(windows)]
    file: fs::File,
}

impl MigrationLock {
    fn acquire(claude_home: &Path) -> io::Result<Self> {
        static PROCESS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let process_guard = PROCESS_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let lock_path = claude_home.join(MIGRATION_LOCK_FILE);
        let mut options = fs::OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&lock_path)?;
        let deadline = Instant::now() + MIGRATION_LOCK_TIMEOUT;
        loop {
            match try_lock_exclusive(&file) {
                Ok(true) => {
                    return Ok(Self {
                        _process_guard: process_guard,
                        file,
                    });
                }
                Ok(false) => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "timed out waiting for Claude history migration lock",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for MigrationLock {
    fn drop(&mut self) {
        let _ = unlock_exclusive(&self.file);
    }
}

#[cfg(unix)]
fn try_lock_exclusive(file: &fs::File) -> io::Result<bool> {
    use std::os::unix::io::AsRawFd;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock
        || error.raw_os_error() == Some(libc::EWOULDBLOCK)
        || error.raw_os_error() == Some(libc::EAGAIN)
    {
        return Ok(false);
    }
    Err(error)
}

#[cfg(unix)]
fn unlock_exclusive(file: &fs::File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn try_lock_exclusive(file: &fs::File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION;
    use windows_sys::Win32::Storage::FileSystem::{
        LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    };
    let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle() as _,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if ok != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
        return Ok(false);
    }
    Err(error)
}

#[cfg(windows)]
fn unlock_exclusive(file: &fs::File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
    let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED = unsafe { std::mem::zeroed() };
    let ok = unsafe { UnlockFileEx(file.as_raw_handle() as _, 0, 1, 0, &mut overlapped) };
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// True for symlinks on Unix, and for real symlinks and NTFS directory junctions on Windows.
/// Rust's `file_type().is_symlink()` on Windows checks `IO_REPARSE_TAG_SYMLINK` and
/// `IO_REPARSE_TAG_MOUNT_POINT` only, never OneDrive cloud placeholders or WOF-compressed files.
fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

fn remove_link_only(path: &Path) -> io::Result<()> {
    if !is_link(path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to remove a non-link path",
        ));
    }
    #[cfg(windows)]
    {
        if fs::remove_dir(path).is_ok() {
            return Ok(());
        }
    }
    fs::remove_file(path)
}

/// Creates a real link (`symlink` on Unix; directory symlink with NTFS junction fallback for
/// directories on Windows, strict file symlink for files so `history.jsonl` is never silently
/// copied and forked).
fn create_strict_link(target: &Path, link: &Path, is_dir: bool) -> io::Result<()> {
    if is_dir {
        fsx::create_dir_link(target, link).map(|_| ())
    } else {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link)
        }
    }
}

fn resolved_link_target(target: &Path) -> PathBuf {
    if is_link(target) {
        fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf())
    } else {
        target.to_path_buf()
    }
}

/// Stages and verifies the real link beside `profile_item` before any migration moves or removes
/// the profile's own copy. If linking is unsupported (e.g. standard Windows without file symlink
/// privilege for `history.jsonl`), this fails early and leaves every profile file untouched.
fn stage_link(target: &Path, profile_item: &Path, is_dir: bool) -> io::Result<PathBuf> {
    let name = profile_item
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let staged = profile_item.with_file_name(format!("{name}{STAGED_LINK_SUFFIX}"));
    if fs::symlink_metadata(&staged).is_ok() {
        if is_link(&staged) {
            remove_link_only(&staged)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("staging path {} exists and is not a link", staged.display()),
            ));
        }
    }
    let link_target = resolved_link_target(target);
    create_strict_link(&link_target, &staged, is_dir)?;
    let verified = is_link(&staged)
        && fs::metadata(&staged).is_ok_and(|meta| {
            if is_dir {
                meta.is_dir()
            } else {
                meta.is_file()
            }
        });
    if !verified {
        let _ = remove_link_only(&staged);
        return Err(io::Error::other(format!(
            "staged link {} failed verification",
            staged.display()
        )));
    }
    Ok(staged)
}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn is_pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    if pid <= 1 {
        return false;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if !handle.is_null() {
        unsafe { CloseHandle(handle) };
        return true;
    }
    io::Error::last_os_error().raw_os_error() == Some(ERROR_ACCESS_DENIED as i32)
}

fn pid_matches_proc_start(pid: u32, proc_start: Option<&str>) -> bool {
    let Some(recorded) = proc_start.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    #[cfg(target_os = "linux")]
    if recorded.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
            if let Some((_, rest)) = stat.rsplit_once(')') {
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if let Some(ticks) = fields.get(19) {
                    if ticks.bytes().all(|b| b.is_ascii_digit()) {
                        return *ticks == recorded;
                    }
                }
            }
        }
    }
    let _ = (pid, recorded);
    true
}

/// Checks Claude Code's `sessions/*.json` PID records inside `profile_dir` (matching `cswap`'s
/// `profile_is_quiescent`). Fails closed if `sessions/` or any session record cannot be read.
fn profile_is_quiescent(profile_dir: &Path) -> Result<(), String> {
    let sessions_dir = profile_dir.join("sessions");
    let entries = match fs::read_dir(&sessions_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "could not read {}: {error}",
                sessions_dir.display()
            ))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read session entry: {error}"))?;
        let path = entry.path();
        let is_json = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
        if !is_json {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("unreadable session record {}: {error}", path.display()))?;
        let data: Value = serde_json::from_str(&text)
            .map_err(|error| format!("malformed session record {}: {error}", path.display()))?;
        let pid = data
            .get("pid")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| format!("session record {} is missing a valid pid", path.display()))?;
        if !is_pid_alive(pid) {
            continue;
        }
        let proc_start = data.get("procStart").and_then(Value::as_str);
        if !pid_matches_proc_start(pid, proc_start) {
            continue;
        }
        return Err(format!(
            "live Claude session pid {pid} is using this profile"
        ));
    }
    Ok(())
}

/// Pre-scans `src` against `dst` without following symlinks: rejects any symlink/junction inside
/// `src`, any non-regular/non-dir entry, and any directory-vs-file type collision against `dst`
/// before a single file is moved.
fn validate_projects_tree(src: &Path, dst: &Path) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        if is_link(&from) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "profile projects directory contains a symlink ({}); skipping migration",
                    from.display()
                ),
            ));
        }
        let meta = fs::symlink_metadata(&from)?;
        let to = dst.join(entry.file_name());
        if meta.is_dir() {
            match fs::symlink_metadata(&to) {
                Ok(dst_meta) if is_link(&to) || !dst_meta.is_dir() => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "dir-vs-file collision between {} and {}; skipping migration",
                            from.display(),
                            to.display()
                        ),
                    ));
                }
                Ok(_) => validate_projects_tree(&from, &to)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    validate_projects_tree(&from, &to)?;
                }
                Err(error) => return Err(error),
            }
        } else if !meta.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing to migrate non-regular entry {}", from.display()),
            ));
        }
    }
    Ok(())
}

/// Moves a regular file `from` to `to`, falling back to a streamed, fsynced temp-file copy + rename
/// across filesystems and only unlinking `from` after `to` is safely in place.
fn move_file_safely(from: &Path, to: &Path) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    if let Some(parent) = to.parent() {
        private_dir(parent)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let temp = to.with_file_name(format!(
        "{name}.{}-{}.ghostex-tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let copy_result = (|| -> io::Result<()> {
        let mut src = fs::File::open(from)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut dst = options.open(&temp)?;
        io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
        Ok(())
    })();
    if let Err(error) = copy_result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    fs::rename(&temp, to).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })?;
    fs::remove_file(from)
}

/// Picks a non-colliding quarantine path right next to `target` (e.g. `MEMORY.md.ghostex-conflict`,
/// `sess.jsonl.ghostex-conflict`), never overwriting an existing file and only treating
/// `ErrorKind::NotFound` as unused.
fn quarantine_path_next_to(target: &Path) -> io::Result<PathBuf> {
    let base_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let first = target.with_file_name(format!("{base_name}.ghostex-conflict"));
    match fs::symlink_metadata(&first) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(first),
        Ok(_) => {}
        Err(error) => return Err(error),
    }
    for n in 2.. {
        let candidate = target.with_file_name(format!("{base_name}.ghostex-conflict.{n}"));
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

/// Streamed comparison: true when `profile_file` adds no information beyond `shared_file`
/// (identical bytes, or a `.jsonl` prefix already contained at the start of `shared_file`).
fn adds_nothing(shared_file: &Path, profile_file: &Path) -> io::Result<bool> {
    let mut shared = fs::File::open(shared_file)?;
    let mut profile = fs::File::open(profile_file)?;
    let shared_len = shared.metadata()?.len();
    let profile_len = profile.metadata()?.len();
    let is_jsonl = profile_file
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"));

    if profile_len > shared_len {
        return Ok(false);
    }
    if profile_len < shared_len && !is_jsonl {
        return Ok(false);
    }
    if profile_len == 0 {
        return Ok(shared_len == 0 || is_jsonl);
    }

    const CHUNK: usize = 64 * 1024;
    let mut p_buf = [0u8; CHUNK];
    let mut s_buf = [0u8; CHUNK];
    let mut remaining = profile_len;
    let mut last_profile_byte = 0u8;

    while remaining > 0 {
        let want = (remaining as usize).min(CHUNK);
        let n = profile.read(&mut p_buf[..want])?;
        if n == 0 {
            // File shrank unexpectedly while comparing; fail safe so we don't drop it.
            return Ok(false);
        }
        shared.read_exact(&mut s_buf[..n])?;
        if p_buf[..n] != s_buf[..n] {
            return Ok(false);
        }
        last_profile_byte = p_buf[n - 1];
        remaining -= n as u64;
    }

    if profile_len == shared_len {
        // Confirm neither file grew past `profile_len` during comparison.
        let mut extra = [0u8; 1];
        return Ok(profile.read(&mut extra)? == 0 && shared.read(&mut extra)? == 0);
    }
    // `is_jsonl` and `profile_len < shared_len`: prefix must end at a newline boundary.
    if last_profile_byte == b'\n' {
        return Ok(true);
    }
    let mut next_shared = [0u8; 1];
    Ok(shared.read(&mut next_shared)? == 1 && next_shared[0] == b'\n')
}

fn merge_projects_recursive(src: &Path, dst: &Path, quarantined: &mut usize) -> io::Result<()> {
    private_dir(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        if is_link(&from) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing to follow symlink {}", from.display()),
            ));
        }
        let meta = fs::symlink_metadata(&from)?;
        let to = dst.join(entry.file_name());
        if meta.is_dir() {
            merge_projects_recursive(&from, &to, quarantined)?;
        } else if meta.is_file() {
            match fs::symlink_metadata(&to) {
                Ok(target_meta) => {
                    if !is_link(&to) && target_meta.is_file() && adds_nothing(&to, &from)? {
                        fs::remove_file(&from)?;
                    } else {
                        let parked = quarantine_path_next_to(&to)?;
                        move_file_safely(&from, &parked)?;
                        *quarantined += 1;
                        eprintln!(
                            "[cliproxy-hub] kept colliding project file beside shared copy: {}",
                            parked.display()
                        );
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    move_file_safely(&from, &to)?;
                }
                Err(error) => return Err(error),
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing to migrate non-regular entry {}", from.display()),
            ));
        }
    }
    fs::remove_dir(src)
}

/// Safely merges `profile_projects` into `shared_projects` without following symlinks or losing
/// colliding files, then removes the emptied `profile_projects`.
fn merge_projects_dir(profile_projects: &Path, shared_projects: &Path) -> io::Result<usize> {
    if is_link(profile_projects) || fsx::same_entry(profile_projects, shared_projects) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "profile projects directory already points at shared projects",
        ));
    }
    validate_projects_tree(profile_projects, shared_projects)?;
    let mut quarantined = 0;
    merge_projects_recursive(profile_projects, shared_projects, &mut quarantined)?;
    Ok(quarantined)
}

/// Ensures `path` exists as a private (`0600` on Unix) file without truncating existing content.
fn ensure_private_file(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(file) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

/// Deduplicates and appends lines from `profile_history` into `shared_history` (fixing any missing
/// trailing newline, creating `shared_history` as 0600, fsyncing before unlinking `profile_history`).
fn merge_history_file(profile_history: &Path, shared_history: &Path) -> io::Result<()> {
    if is_link(profile_history) || fsx::same_entry(profile_history, shared_history) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "profile history.jsonl already points at shared history.jsonl",
        ));
    }
    ensure_private_file(shared_history)?;
    let profile_text = fs::read_to_string(profile_history)?;
    let shared_text = match fs::read_to_string(shared_history) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let mut seen: HashSet<&str> = shared_text.lines().collect();
    let mut additions: Vec<&str> = Vec::new();
    for line in profile_text.lines() {
        if !line.is_empty() && seen.insert(line) {
            additions.push(line);
        }
    }
    if !additions.is_empty() {
        let torn = !shared_text.is_empty() && !shared_text.ends_with('\n');
        let mut payload = String::new();
        if torn {
            payload.push('\n');
        }
        payload.push_str(&additions.join("\n"));
        payload.push('\n');

        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut target = options.open(shared_history)?;
        target.write_all(payload.as_bytes())?;
        target.sync_all()?;
    }
    fs::remove_file(profile_history)
}

fn finalize_staged_link(staged: &Path, shared_target: &Path, profile_item: &Path, is_dir: bool) {
    if let Err(rename_error) = fs::rename(staged, profile_item) {
        let target = resolved_link_target(shared_target);
        if let Err(link_error) = create_strict_link(&target, profile_item, is_dir) {
            eprintln!(
                "[cliproxy-hub] merged {} into {}, but placing link failed ({rename_error}; {link_error})",
                profile_item.display(),
                shared_target.display()
            );
        }
        let _ = remove_link_only(staged);
    }
}

/// Links `profile_dir/<name>` to `claude_home/<name>` like `cswap --share-history`, safely
/// migrating any pre-existing `projects/` or `history.jsonl` under a lock and quiescence check.
fn share_claude_home(claude_home: &Path, profile_dir: &Path) {
    let shared_projects = claude_home.join("projects");
    let profile_projects = profile_dir.join("projects");
    let shared_history = claude_home.join("history.jsonl");
    let profile_history = profile_dir.join("history.jsonl");

    // Fast path: skip the cross-process migration lock when projects/, history.jsonl, and any
    // present CLAUDE.md / skills/ are already in place, so hub launches never stall behind cswap.
    let projects_done =
        is_link(&profile_projects) || fsx::same_entry(&profile_projects, &shared_projects);
    let history_done =
        is_link(&profile_history) || fsx::same_entry(&profile_history, &shared_history);
    let extras_done = ["CLAUDE.md", "skills"].iter().all(|name| {
        fs::symlink_metadata(claude_home.join(name)).is_err()
            || fs::symlink_metadata(profile_dir.join(name)).is_ok()
    });
    if projects_done && history_done && extras_done {
        return;
    }

    if let Err(error) = private_dir(claude_home) {
        eprintln!(
            "[cliproxy-hub] could not prepare {}: {error}",
            claude_home.display()
        );
        return;
    }
    let _lock = match MigrationLock::acquire(claude_home) {
        Ok(lock) => lock,
        Err(error) => {
            eprintln!(
                "[cliproxy-hub] skipping Claude shared-history setup for {}: {error}",
                profile_dir.display()
            );
            return;
        }
    };

    // 1. projects/ (ensure target exists, migrate safely if profile has its own directory, then link).
    if let Err(error) = private_dir(&shared_projects) {
        eprintln!(
            "[cliproxy-hub] could not prepare {}: {error}",
            shared_projects.display()
        );
    } else if !is_link(&profile_projects) && !fsx::same_entry(&profile_projects, &shared_projects) {
        match fs::symlink_metadata(&profile_projects) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let target = resolved_link_target(&shared_projects);
                if let Err(link_error) = create_strict_link(&target, &profile_projects, true) {
                    eprintln!(
                        "[cliproxy-hub] could not link {} -> {}: {link_error}",
                        profile_projects.display(),
                        target.display()
                    );
                }
            }
            Ok(meta) if meta.is_dir() => match profile_is_quiescent(profile_dir) {
                Err(reason) => {
                    eprintln!(
                        "[cliproxy-hub] skipping projects/ migration for {}: {reason}",
                        profile_dir.display()
                    );
                }
                Ok(()) => match stage_link(&shared_projects, &profile_projects, true) {
                    Err(error) => {
                        eprintln!(
                            "[cliproxy-hub] not sharing projects/ for {} (leaving profile files untouched): {error}",
                            profile_dir.display()
                        );
                    }
                    Ok(staged) => match merge_projects_dir(&profile_projects, &shared_projects) {
                        Ok(_) => {
                            finalize_staged_link(&staged, &shared_projects, &profile_projects, true)
                        }
                        Err(error) => {
                            let _ = remove_link_only(&staged);
                            eprintln!(
                                "[cliproxy-hub] skipping projects/ migration for {}: {error}",
                                profile_dir.display()
                            );
                        }
                    },
                },
            },
            Ok(_) => {}
            Err(error) => {
                eprintln!(
                    "[cliproxy-hub] could not inspect {}: {error}",
                    profile_projects.display()
                );
            }
        }
    }

    // 2. history.jsonl (create shared file as 0600, migrate safely with dedupe + fsync, then link).
    if let Err(error) = ensure_private_file(&shared_history) {
        eprintln!(
            "[cliproxy-hub] could not prepare {}: {error}",
            shared_history.display()
        );
    } else if !is_link(&profile_history) && !fsx::same_entry(&profile_history, &shared_history) {
        match fs::symlink_metadata(&profile_history) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let target = resolved_link_target(&shared_history);
                if let Err(link_error) = create_strict_link(&target, &profile_history, false) {
                    eprintln!(
                        "[cliproxy-hub] could not link {} -> {}: {link_error}",
                        profile_history.display(),
                        target.display()
                    );
                }
            }
            Ok(meta) if meta.is_file() => match profile_is_quiescent(profile_dir) {
                Err(reason) => {
                    eprintln!(
                        "[cliproxy-hub] skipping history.jsonl migration for {}: {reason}",
                        profile_dir.display()
                    );
                }
                Ok(()) => match stage_link(&shared_history, &profile_history, false) {
                    Err(error) => {
                        eprintln!(
                            "[cliproxy-hub] not sharing history.jsonl for {} (leaving profile file untouched): {error}",
                            profile_dir.display()
                        );
                    }
                    Ok(staged) => match merge_history_file(&profile_history, &shared_history) {
                        Ok(()) => {
                            finalize_staged_link(&staged, &shared_history, &profile_history, false)
                        }
                        Err(error) => {
                            let _ = remove_link_only(&staged);
                            eprintln!(
                                "[cliproxy-hub] skipping history.jsonl migration for {}: {error}",
                                profile_dir.display()
                            );
                        }
                    },
                },
            },
            Ok(_) => {}
            Err(error) => {
                eprintln!(
                    "[cliproxy-hub] could not inspect {}: {error}",
                    profile_history.display()
                );
            }
        }
    }

    // 3. CLAUDE.md and skills/ (link into hub profile whenever present in ~/.claude and absent in profile).
    for (name, is_dir) in [("CLAUDE.md", false), ("skills", true)] {
        let src = claude_home.join(name);
        let dst = profile_dir.join(name);
        if fs::symlink_metadata(&src).is_ok() && fs::symlink_metadata(&dst).is_err() {
            let target = resolved_link_target(&src);
            if let Err(error) = create_strict_link(&target, &dst, is_dir) {
                eprintln!(
                    "[cliproxy-hub] could not link {} -> {}: {error}",
                    dst.display(),
                    target.display()
                );
            }
        }
    }
}

/// Claude's folder: the key file, `settings.json` with the hub's address and the key helper (the
/// hooks Ghostex adds there are kept), shared `CLAUDE.md`, `skills/`, `projects/` and
/// `history.jsonl` from `~/.claude`, and `.claude.json` marked as past Claude's first-run steps.
fn prepare_claude(
    home: &Path,
    dir: &Path,
    url: &str,
    key: &str,
    statusline: Option<&Path>,
) -> std::io::Result<()> {
    private_dir(dir)?;
    share_claude_home(&home.join(".claude"), dir);
    let key_file = dir.join(KEY_FILE);
    write_private(&key_file, key.as_bytes())?;
    let settings_path = dir.join("settings.json");
    let mut settings = read_object(&settings_path);
    let env = settings
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(env) = env {
        env.insert("ANTHROPIC_BASE_URL".into(), json!(url));
        env.remove("ANTHROPIC_AUTH_TOKEN");
        env.remove("ANTHROPIC_API_KEY");
    } else {
        settings.insert("env".into(), json!({ "ANTHROPIC_BASE_URL": url }));
    }
    settings.insert("apiKeyHelper".into(), json!(key_helper(&key_file)));
    // What Ghostex gives every Claude profile with or without hooks: its statusLine (the model,
    // effort and limits the chat shows) while Read live status from Claude Code and Cursor is on
    // (agent_hooks/statusline_setting.rs), and transcripts kept for good.
    let mut settings = Value::Object(settings);
    match statusline {
        Some(script) => {
            crate::agent_hooks::statusline::register_statusline(&mut settings, script);
        }
        None => {
            crate::agent_hooks::statusline::unregister_statusline(&mut settings);
        }
    }
    crate::agent_hooks::claude_retention::ensure_claude_transcript_retention(&mut settings);
    write_private(
        &settings_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&settings).unwrap_or_default()
        )
        .as_bytes(),
    )?;
    let state_path = dir.join(".claude.json");
    let mut state = read_object(&state_path);
    if state.get("hasCompletedOnboarding") != Some(&Value::Bool(true)) {
        state.insert("hasCompletedOnboarding".into(), json!(true));
        write_private(
            &state_path,
            serde_json::to_string_pretty(&Value::Object(state))
                .unwrap_or_default()
                .as_bytes(),
        )?;
    }
    Ok(())
}

/// Codex's folder: `config.toml` with the hub as its model provider (other lines, such as the
/// folder trust and the hooks Ghostex adds, are kept).
fn prepare_codex(dir: &Path, url: &str, key: &str) -> std::io::Result<()> {
    private_dir(dir)?;
    let path = dir.join("config.toml");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut document = text.parse::<toml_edit::DocumentMut>().unwrap_or_default();
    document["model_provider"] = toml_edit::value(CODEX_PROVIDER);
    let providers = document
        .entry("model_providers")
        .or_insert(toml_edit::table());
    if let Some(providers) = providers.as_table_mut() {
        providers.set_implicit(true);
        let mut provider = toml_edit::Table::new();
        provider["name"] = toml_edit::value("CLIProxyAPI hub");
        provider["base_url"] = toml_edit::value(format!("{url}/v1"));
        provider["wire_api"] = toml_edit::value("responses");
        provider["experimental_bearer_token"] = toml_edit::value(key);
        provider["requires_openai_auth"] = toml_edit::value(false);
        providers.insert(CODEX_PROVIDER, toml_edit::Item::Table(provider));
    }
    write_private(&path, document.to_string().as_bytes())
}

/// Makes the files Ghostex writes in a hub profile readable by this user only.
fn tighten(dir: &Path) {
    #[cfg(unix)]
    for name in ["settings.json", ".claude.json", "config.toml", KEY_FILE] {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        if path.is_file() {
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Takes the key out of a hub's profile folders when the hub is removed. The folders stay: they
/// hold the conversations of the sessions that ran there.
pub(crate) fn forget_key(home: &Path, hub_id: &str) {
    let _ = fs::remove_file(profile_dir(home, Provider::Claude, hub_id).join(KEY_FILE));
    let path = profile_dir(home, Provider::Codex, hub_id).join("config.toml");
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(mut document) = text.parse::<toml_edit::DocumentMut>() {
            if let Some(provider) = document
                .get_mut("model_providers")
                .and_then(|providers| providers.get_mut(CODEX_PROVIDER))
                .and_then(toml_edit::Item::as_table_mut)
            {
                provider.remove("experimental_bearer_token");
                let _ = write_private(&path, document.to_string().as_bytes());
            }
        }
    }
}

/// `base` started in `dir`: the profile variable before the command, as a custom agent's command
/// would set it.
fn in_profile(provider: Provider, dir: &Path, base: &str) -> String {
    let variable = match provider {
        Provider::Claude => "CLAUDE_CONFIG_DIR",
        Provider::Codex => "CODEX_HOME",
    };
    let dir = dir.to_string_lossy();
    if cfg!(windows) {
        format!("$env:{variable} = '{}'; {base}", dir.replace('\'', "''"))
    } else {
        format!(
            "{variable}={} {}",
            quote_shell_arg(&dir),
            crate::accounts::launch::without_shell_function(base, provider)
        )
    }
}

/// Prepares a new session to start on hub `hub_id`: its profile folder is written with the hub's
/// address and client key, and the command it starts with names that folder. Returns the command;
/// `runtime` loses the picked account and gains [`RUNTIME_KEY`].
pub(crate) fn apply(
    paths: &GxserverPaths,
    provider: Provider,
    hub_id: &str,
    configured_command: Option<&str>,
    runtime: &mut Map<String, Value>,
) -> Result<String, DomainStateError> {
    if !super::enabled(paths) {
        return Err(DomainStateError::bad_request(
            ghostex_settings_catalog::built_in_extensions::turned_off_message(
                ghostex_settings_catalog::built_in_extensions::CLI_PROXY_API,
            ),
        ));
    }
    let hub = super::store::find(paths, hub_id).ok_or_else(|| {
        DomainStateError::bad_request(
            "That hub was removed. Pick another place to start the session.",
        )
    })?;
    if let Some(reason) = super::google_login_refusal(paths, &hub) {
        return Err(DomainStateError::bad_request(reason));
    }
    let base = configured_command
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .unwrap_or(provider.id());
    if crate::accounts::launch::uses_own_login(base, provider) {
        return Err(DomainStateError::bad_request(
            "This agent's command chooses its own login, so it can't start on the hub. Start it from the built-in agent instead.",
        ));
    }
    let key =
        super::store::open_key(paths, &hub.client_key).map_err(DomainStateError::bad_request)?;
    // The home gxserver gives agents (hooks, skills and folder trust use it too).
    let home = paths.agent_config_home_dir().to_path_buf();
    let dir = profile_dir(&home, provider, &hub.id);
    match provider {
        Provider::Claude => prepare_claude(
            &home,
            &dir,
            &hub.url,
            &key,
            crate::agent_hooks::statusline_setting::read_agent_live_status(paths)
                .then(|| {
                    crate::agent_hooks::config::HookPaths::from_paths(paths).statusline_hook_path
                })
                .as_deref(),
        ),
        Provider::Codex => prepare_codex(&dir, &hub.url, &key),
    }
    .map_err(io)?;
    // Ghostex's hooks in the new folder only while Use agent hooks is on: Claude and Codex are
    // followed without them otherwise (CDXC:AgentHooks 2026-11-12 in agent_hooks/hook_free.rs).
    // Best effort: the session still runs without them, and gxserver's start repairs them.
    if !crate::agent_hooks::hook_free::hook_free(paths, provider.id()) {
        let _ = crate::agent_hooks::install_agent_hooks_for_running_agent(paths, provider.id());
    }
    // The hook installer rewrites the folder's settings with the usual permissions; the folder is
    // this user's only (0700), and its files go back to 0600 too.
    tighten(&dir);
    for key in [
        "accountId",
        "accountProvider",
        "accountName",
        "accountSlot",
        "accountCommand",
    ] {
        runtime.remove(key);
    }
    runtime.insert(RUNTIME_KEY.into(), json!(hub.id));
    let command = in_profile(provider, &dir, base);
    runtime.insert("accountBaseCommand".into(), json!(command));
    Ok(command)
}

/// The hub `id` names (`cliproxy:<hub id>`) while the extension is on and the hub is still saved.
fn saved_hub(paths: &GxserverPaths, id: &str) -> Option<String> {
    let hub_id = id.strip_prefix(ACCOUNT_PREFIX)?;
    (super::enabled(paths) && super::store::find(paths, hub_id).is_some())
        .then(|| hub_id.to_string())
}

/// The hub `provider`'s New sessions start on is pinned to, while it can be used. A pin to a hub
/// that was removed, or while the extension is off, reads as no hub (the rule falls back to Auto).
///
/// CDXC:AgentProviders 2026-11-19 DECISION:
/// User: "make my macbook and windows machine call claude and codex thro that hub which should balance my different accounts". A hub is a choice in New sessions start on, saved as a pinned account `cliproxy:<hub id>`, so every launch that names no account (quick launch, the launchers, `start-thread`, `agents create`, the phone) starts On the hub, and the hub balances its own accounts. A picked account and a workspace's Claude account still win, as they do over every rule.
/// SEE-ALSO: default_account.rs (`pick`, `quick_launch_accounts`), agents/launch_plan.rs, apps/desktop/src/app/window/settings_modal/tabs/accounts/rules.rs.
pub(crate) fn default_hub(registry: &Registry, provider: Provider) -> Option<String> {
    match registry.new_session_accounts.get(&provider)? {
        NewSessionAccount::Pinned { id } if id.starts_with(ACCOUNT_PREFIX) => {
            saved_hub(&crate::paths::get_gxserver_paths(None), id)
        }
        _ => None,
    }
}

/// Whether `id` is a saved hub (`cliproxy:<hub id>`) that New sessions start on can be pinned to.
pub(crate) fn pinnable(paths: &GxserverPaths, id: &str) -> bool {
    saved_hub(paths, id).is_some()
}

/// The hub `wanted` names (`ghostex coordinator start-thread --account`): its id, `cliproxy:<id>`,
/// its name or host (any case), or `hub` while exactly one is saved. `None` when the extension is
/// off or nothing matches, so the name is read as a Claude Swap or Codex Swap account.
pub(crate) fn hub_named(paths: &GxserverPaths, wanted: &str) -> Option<String> {
    if !super::enabled(paths) {
        return None;
    }
    let hubs = super::store::read(paths).ok()?;
    let wanted = wanted.trim();
    let id = wanted.strip_prefix(ACCOUNT_PREFIX).unwrap_or(wanted);
    if let Some(hub) = hubs.iter().find(|hub| {
        hub.id == id
            || (!hub.label.trim().is_empty() && hub.label.trim().eq_ignore_ascii_case(wanted))
            || super::host(&hub.url).eq_ignore_ascii_case(wanted)
    }) {
        return Some(hub.id.clone());
    }
    match hubs.as_slice() {
        [only] if wanted.eq_ignore_ascii_case("hub") => Some(only.id.clone()),
        _ => None,
    }
}

/// Whether hub `hub_id` answers with its client key now, before a session starts on it: a hub that
/// is down or refuses the key would only fail the session's first message.
pub(crate) fn check_reachable(paths: &GxserverPaths, hub_id: &str) -> Result<(), DomainStateError> {
    let Some(hub) = super::store::find(paths, hub_id) else {
        return Ok(());
    };
    let key =
        super::store::open_key(paths, &hub.client_key).map_err(DomainStateError::bad_request)?;
    super::client::check_client_key(&hub.url, &key).map_err(|error| {
        DomainStateError::bad_request(format!(
            "The session didn't start on the hub. {error} Or start it on another account."
        ))
    })
}

/// The name a New sessions start on pin to a hub (`cliproxy:<hub id>`) shows, whether or not the
/// extension is on: the hub's name, else its host.
pub(crate) fn pinned_label(paths: &GxserverPaths, id: &str) -> Option<String> {
    let hub = super::store::find(paths, id.strip_prefix(ACCOUNT_PREFIX)?)?;
    Some(if hub.label.trim().is_empty() {
        super::host(&hub.url)
    } else {
        hub.label
    })
}

/// Why a launch that follows New sessions start on doesn't start on the rule's hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotOnHub {
    /// The hub is down or refuses the key.
    Unreachable,
    /// The hub holds a Google login (`google_login_refusal`, Q5230).
    GoogleLogin,
}

/// A launch whose rule's hub can't be reached, or holds a Google login, starts on the account Auto
/// would pick instead (or, with none, on the computer's own login), and the session's switch card
/// says so.
pub(crate) fn start_elsewhere(
    paths: &GxserverPaths,
    registry: &Registry,
    provider: Provider,
    hub_id: &str,
    why: NotOnHub,
    runtime: &mut Map<String, Value>,
) {
    let why = match why {
        NotOnHub::Unreachable => "can't be reached",
        NotOnHub::GoogleLogin => "holds a Google login, which Ghostex doesn't use through a hub",
    };
    let snapshot = crate::accounts::runtime::current_snapshot();
    let account = crate::accounts::default_account::hub_fallback_accounts(registry, &snapshot)
        .remove(&provider)
        .and_then(|id| registry.accounts.iter().find(|a| a.id == id));
    let hub = pinned_label(paths, &format!("{ACCOUNT_PREFIX}{hub_id}"))
        .unwrap_or_else(|| "The hub".to_string());
    // The card shows the account itself (masked like every account name there), so the note
    // names none.
    let (to, note) = match account {
        Some(account) => {
            runtime.insert("accountId".into(), json!(account.id));
            (
                Some(account.id.clone()),
                format!("{hub} {why}, so this started on the account below."),
            )
        }
        None => {
            runtime.remove("accountId");
            (
                None,
                format!("{hub} {why}, so this started on this computer's own login."),
            )
        }
    };
    let now = chrono::Utc::now().to_rfc3339();
    runtime.insert(
        "accountSwitch".into(),
        json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "provider": provider,
            "source": "automatic",
            "phase": "success",
            "accountReady": true,
            "toAccountId": to,
            "note": note,
            "updatedAt": now,
        }),
    );
}

/// The address of hub `hub_id`, while it is saved.
pub(crate) fn hub_url(paths: &GxserverPaths, hub_id: &str) -> Option<String> {
    super::store::find(paths, hub_id).map(|hub| hub.url)
}

/// The hub id a session uses: its applied [`RUNTIME_KEY`], or a launcher's picked
/// `cliproxy:<hub id>`.
pub(crate) fn session_hub_id(runtime: &Map<String, Value>) -> Option<String> {
    runtime
        .get(RUNTIME_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| picked_hub(runtime))
}

/// The hub id a launcher's picked account names (`cliproxy:<hub id>`), if it names one.
pub(crate) fn picked_hub(runtime: &Map<String, Value>) -> Option<String> {
    runtime
        .get("accountId")
        .and_then(Value::as_str)
        .and_then(|id| id.strip_prefix(ACCOUNT_PREFIX))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hub_command_uses_its_own_login() {
        let command = in_profile(
            Provider::Claude,
            Path::new("/Users/sam/.claude-profiles/ghostex-hub-1"),
            "claude --model opus",
        );
        assert!(crate::accounts::launch::uses_own_login(
            &command,
            Provider::Claude
        ));
        let command = in_profile(
            Provider::Codex,
            Path::new("/Users/sam/.codex-profiles/ghostex-hub-1"),
            "codex",
        );
        assert!(crate::accounts::launch::uses_own_login(
            &command,
            Provider::Codex
        ));
    }
}
