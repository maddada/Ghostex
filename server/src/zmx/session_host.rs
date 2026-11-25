//! Where a new session daemon's zmx runs from on macOS.
//!
//! CDXC:Zmx 2026-11-14 WHY: macOS decides a session's access to Documents, Desktop and Downloads
//! from the program responsible for it, the session's zmx daemon, and judges that program by the
//! signed app it sits in: a zmx inside Ghostex.app gets Ghostex's grants (a zmx in a copy of
//! Ghostex.app elsewhere gets them too, a lone GhostexSessions.app copy does not). A Sparkle update
//! moves the old Ghostex.app into its Installation folder and deletes it, so every daemon still
//! running from it loses its app, macOS reports it with no path, and every program in the session
//! got "Operation not permitted" in ~/Documents (GitHub #611; reproduced on 2026-11-14 by deleting
//! a copied app under a running daemon: Documents, Desktop and Downloads all turned from granted
//! to denied the moment the copy was removed, while merely moving it changed nothing). So a new
//! daemon runs from an APFS clone of the app made once per installed zmx in
//! `<data>/session-hosts.noindex/`, and a clone stays until no process runs from it. A clone
//! shares the app's disk blocks until the app is replaced. When the clone can't be made (the app
//! is on another volume than the data folder, or not on APFS), or macOS won't run zmx from it
//! (`SessionHostPlan::runs`), daemons run from the app as before.
//! Sessions started by a build without this keep running from the app the update deletes.
//! `.noindex` keeps Spotlight out, but LaunchServices still registers cloned bundles when they run
//! and lists every past version in Finder's Open With menu (GitHub #738), so each copy is
//! unregistered with `lsregister -u` (off-thread, best effort) after creation, after launch, at
//! gxserver startup, and before an unused copy is deleted.
//! SEE-ALSO: `remove_leftover_releases` in server/src/setup.rs (the same rule for the release
//! folders of a Ghostex server installed without the app), `macos_zmx_bundle_identifier` in
//! launch.rs (still finds GhostexSessions.app inside the clone).

use crate::toolchain::GxserverResolvedTool;

/// The zmx a new session daemon should run: a clone of the app the bundled zmx sits in, when it sits inside one.
#[cfg(target_os = "macos")]
pub(crate) fn session_daemon_zmx(zmx: GxserverResolvedTool) -> GxserverResolvedTool {
    let Some(plan) = macos::SessionHostPlan::for_zmx(std::path::Path::new(&zmx.executable_path))
    else {
        return zmx;
    };
    match plan.ensure() {
        Ok((executable, created)) => {
            let runs = plan.runs(&executable);
            if created {
                macos::unregister_bundle_async(plan.host_app());
            }
            if runs {
                GxserverResolvedTool {
                    executable_path: executable.to_string_lossy().into_owned(),
                    ..zmx
                }
            } else {
                zmx
            }
        }
        Err(error) => {
            eprintln!(
                "[gxserver] session daemons run from {} because its copy could not be made: {error}",
                plan.app.display()
            );
            zmx
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn session_daemon_zmx(zmx: GxserverResolvedTool) -> GxserverResolvedTool {
    zmx
}

/// Unregisters the session-host app copy containing `zmx_executable` from LaunchServices off-thread.
#[cfg(target_os = "macos")]
pub(crate) fn unregister_session_host_for_zmx(zmx_executable: &std::path::Path) {
    if let Some(app_bundle) = macos::session_host_app_for_zmx(&macos::hosts_root(), zmx_executable)
    {
        macos::unregister_bundle_async(app_bundle);
    }
}

/// Removes the app copies no running program uses any more (unregistering each from LaunchServices
/// first), and unregisters all remaining copies so stale Open With entries are cleaned up.
/// Runs off-thread at gxserver start.
#[cfg(target_os = "macos")]
pub(crate) fn remove_unused_session_hosts() {
    let _ = std::thread::Builder::new()
        .name("session-host-cleanup".to_string())
        .spawn(|| {
            let root = macos::hosts_root();
            let current = crate::toolchain::require_bundled_zmx()
                .ok()
                .and_then(|zmx| {
                    macos::SessionHostPlan::for_zmx(std::path::Path::new(&zmx.executable_path))
                })
                .map(|plan| plan.key);
            macos::remove_unused(
                &root,
                current.as_deref(),
                &crate::setup::running_executable_paths(),
            );
            macos::unregister_existing(&root);
        });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn remove_unused_session_hosts() {}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        collections::HashMap,
        ffi::CString,
        fs, io,
        os::unix::{ffi::OsStrExt, fs::MetadataExt},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{LazyLock, Mutex},
        thread,
        time::{Duration, Instant},
    };

    /// Serialises clone creation, so two sessions starting together make one copy.
    static CLONE_LOCK: Mutex<()> = Mutex::new(());

    /// Whether each clone's zmx runs, by clone key; sessions starting together wait for one check.
    static RUN_CHECKS: LazyLock<Mutex<HashMap<String, bool>>> = LazyLock::new(Default::default);

    /// A working zmx prints its version in milliseconds; one macOS holds never starts.
    const RUN_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

    pub(super) fn hosts_root() -> PathBuf {
        crate::paths::get_gxserver_paths(None)
            .app_data_dir
            .join("session-hosts.noindex")
    }

    pub(super) struct SessionHostPlan {
        /// The outermost app bundle the bundled zmx sits in (Ghostex.app).
        pub(super) app: PathBuf,
        /// zmx inside that app, relative to it.
        relative_zmx: PathBuf,
        /// Names the installed zmx file: it changes whenever an install replaces zmx.
        pub(super) key: String,
        root: PathBuf,
    }

    impl SessionHostPlan {
        /// `None` for a zmx that is not inside another app: a source build, or the Ghostex server
        /// installed without the app, whose release folders already outlive their daemons.
        pub(super) fn for_zmx(zmx: &Path) -> Option<Self> {
            let root = hosts_root();
            let zmx = fs::canonicalize(zmx).ok()?;
            let canonical_root = fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
            if zmx.starts_with(&canonical_root) {
                return None;
            }
            let apps: Vec<&Path> = zmx
                .ancestors()
                .skip(1)
                .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
                .collect();
            // The app zmx is the executable of (GhostexSessions.app) is not enough: it must sit in another.
            if apps.len() < 2 {
                return None;
            }
            let app = apps.last()?.to_path_buf();
            let relative_zmx = zmx.strip_prefix(&app).ok()?.to_path_buf();
            let metadata = fs::metadata(&zmx).ok()?;
            let identity = format!(
                "{}:{}:{}:{}:{}:{}",
                app.display(),
                metadata.dev(),
                metadata.ino(),
                metadata.size(),
                metadata.mtime(),
                metadata.mtime_nsec()
            );
            Some(Self {
                app,
                relative_zmx,
                key: fnv1a_hex(identity.as_bytes()),
                root,
            })
        }

        fn host_dir(&self) -> PathBuf {
            self.root.join(&self.key)
        }

        pub(super) fn host_app(&self) -> PathBuf {
            let app_name = self.app.file_name().unwrap_or_default();
            self.host_dir().join(app_name)
        }

        fn host_zmx(&self) -> PathBuf {
            self.host_app().join(&self.relative_zmx)
        }

        /// The cloned zmx (and whether this call created the clone), cloning the app first when this zmx has no copy yet.
        pub(super) fn ensure(&self) -> io::Result<(PathBuf, bool)> {
            let host_zmx = self.host_zmx();
            if host_zmx.is_file() {
                return Ok((host_zmx, false));
            }
            let _guard = CLONE_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if host_zmx.is_file() {
                return Ok((host_zmx, false));
            }
            fs::create_dir_all(&self.root)?;
            let partial = self
                .root
                .join(format!(".{}.partial-{}", self.key, std::process::id()));
            let _ = fs::remove_dir_all(&partial);
            fs::create_dir(&partial)?;
            let made = clone_tree(
                &self.app,
                &partial.join(self.app.file_name().unwrap_or_default()),
            )
            .and_then(|()| fs::rename(&partial, self.host_dir()));
            if let Err(error) = made {
                let _ = fs::remove_dir_all(&partial);
                return Err(error);
            }
            if !host_zmx.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} is missing from the copy", self.relative_zmx.display()),
                ));
            }
            let root = self.root.clone();
            let key = self.key.clone();
            let _ = thread::Builder::new()
                .name("session-host-cleanup".to_string())
                .spawn(move || {
                    remove_unused(&root, Some(&key), &crate::setup::running_executable_paths());
                });
            Ok((host_zmx, true))
        }

        /// CDXC:Zmx 2026-11-14 WHY: macOS can refuse to run zmx from the clone while it runs the same bytes from the app: on 2026-11-14 a source build signed with an Apple Development certificate (not notarized) ran from /Applications, but from the clone the kernel logged "ASP: Security policy would not allow process" and the process waited in dyld forever, so every new session's launchd job hung, never registered, and the app showed "Couldn't open this terminal". Only running it tells, so the clone's `zmx version` must finish once per installed zmx before daemons run from it; one that macOS holds is killed and daemons run from the app as before this clone existed.
        pub(super) fn runs(&self, host_zmx: &Path) -> bool {
            let mut verdicts = RUN_CHECKS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *verdicts.entry(self.key.clone()).or_insert_with(|| {
                let runs = runs_within(host_zmx, RUN_CHECK_TIMEOUT);
                if !runs {
                    eprintln!(
                        "[gxserver] session daemons run from {} because macOS did not let its copy's zmx run within {} s",
                        self.app.display(),
                        RUN_CHECK_TIMEOUT.as_secs()
                    );
                }
                runs
            })
        }
    }

    /// Whether `zmx version` from this executable exits successfully in time; a run still waiting is killed.
    fn runs_within(zmx: &Path, timeout: Duration) -> bool {
        let Ok(mut child) = Command::new(zmx)
            .arg("version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
            }
        }
    }

    /// One `clonefile` call clones a whole folder on APFS and fails on other volumes or file systems.
    fn clone_tree(source: &Path, destination: &Path) -> io::Result<()> {
        let source = CString::new(source.as_os_str().as_bytes())?;
        let destination = CString::new(destination.as_os_str().as_bytes())?;
        if unsafe { libc::clonefile(source.as_ptr(), destination.as_ptr(), 0) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

    /// Returns `<root>/<key>/<App>.app` when `zmx_executable` sits inside a session-host copy.
    pub(super) fn session_host_app_for_zmx(root: &Path, zmx_executable: &Path) -> Option<PathBuf> {
        let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let canonical_zmx =
            fs::canonicalize(zmx_executable).unwrap_or_else(|_| zmx_executable.to_path_buf());
        let relative = canonical_zmx.strip_prefix(&canonical_root).ok()?;
        let mut components = relative.components();
        let key = components.next()?;
        let app = components.next()?;
        let app_path = canonical_root.join(key).join(app);
        (app_path.extension().is_some_and(|ext| ext == "app") && app_path.is_dir())
            .then_some(app_path)
    }

    /// Best-effort `lsregister -u` so macOS stops offering a session-host copy in Finder's Open
    /// With menu (GitHub #738). Does not edit Info.plist (preserves the code signature) and does
    /// not affect running processes.
    pub(super) fn unregister_bundle(app_path: &Path) {
        match Command::new(LSREGISTER)
            .arg("-u")
            .arg(app_path)
            .stdin(Stdio::null())
            .output()
        {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                // -10814 (kLSApplicationNotFoundErr): bundle is already absent from LaunchServices.
                if stderr.contains("-10814") || stdout.contains("-10814") {
                    return;
                }
                let detail = if !stderr.trim().is_empty() {
                    stderr.trim()
                } else {
                    stdout.trim()
                };
                eprintln!(
                    "[gxserver] lsregister -u {} failed ({}): {}",
                    app_path.display(),
                    output.status,
                    detail
                );
            }
            Err(error) => {
                eprintln!(
                    "[gxserver] failed to run lsregister -u {}: {error}",
                    app_path.display()
                );
            }
        }
    }

    /// Spawns a background thread to unregister `app_path` from LaunchServices without blocking.
    pub(super) fn unregister_bundle_async(app_path: PathBuf) {
        let _ = thread::Builder::new()
            .name("session-host-lsregister".to_string())
            .spawn(move || unregister_bundle(&app_path));
    }

    /// Unregisters every `.app` bundle directly inside a session-host entry directory.
    fn unregister_host_dir_bundles(host_dir: &Path) {
        let Ok(entries) = fs::read_dir(host_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "app") {
                unregister_bundle(&path);
            }
        }
    }

    /// Unregisters every existing session-host app copy under `root` (skipping in-progress partial
    /// clones). Called once at gxserver startup so stale Open With entries from earlier versions
    /// are cleaned up.
    pub(super) fn unregister_existing(root: &Path) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            unregister_host_dir_bundles(&path);
        }
    }

    /// Removes every copy but `current` that no running program was started from, and partial copies left by an ended gxserver.
    /// Unregisters each copy from LaunchServices before deleting it, because `lsregister -u` cannot remove an entry once the bundle is gone.
    pub(super) fn remove_unused(root: &Path, current: Option<&str>, running: &[PathBuf]) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        let running: Vec<PathBuf> = running
            .iter()
            .map(|exe| fs::canonicalize(exe).unwrap_or_else(|_| exe.clone()))
            .collect();
        let own_partial_suffix = format!(".partial-{}", std::process::id());
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if Some(name.as_str()) == current || name.ends_with(&own_partial_suffix) {
                continue;
            }
            let path = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            if running.iter().any(|exe| exe.starts_with(&path)) {
                continue;
            }
            unregister_host_dir_bundles(&path);
            let _ = fs::remove_dir_all(&path);
        }
    }

    fn fnv1a_hex(bytes: &[u8]) -> String {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }
}
