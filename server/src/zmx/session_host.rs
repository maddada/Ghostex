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
//! is on another volume than the data folder, or not on APFS) daemons run from the app as before.
//! Sessions started by a build without this keep running from the app the update deletes.
//! `.noindex` keeps Spotlight, and through it Launch Services, from listing the copies as apps.
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
        Ok(executable) => GxserverResolvedTool {
            executable_path: executable.to_string_lossy().into_owned(),
            ..zmx
        },
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

/// Removes the app copies no running program uses any more, except the current one. Runs at gxserver start.
#[cfg(target_os = "macos")]
pub(crate) fn remove_unused_session_hosts() {
    let _ = std::thread::Builder::new()
        .name("session-host-cleanup".to_string())
        .spawn(|| {
            let current = crate::toolchain::require_bundled_zmx()
                .ok()
                .and_then(|zmx| {
                    macos::SessionHostPlan::for_zmx(std::path::Path::new(&zmx.executable_path))
                })
                .map(|plan| plan.key);
            macos::remove_unused(
                &macos::hosts_root(),
                current.as_deref(),
                &crate::setup::running_executable_paths(),
            )
        });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn remove_unused_session_hosts() {}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::CString,
        fs, io,
        os::unix::{ffi::OsStrExt, fs::MetadataExt},
        path::{Path, PathBuf},
        sync::Mutex,
    };

    /// Serialises clone creation, so two sessions starting together make one copy.
    static CLONE_LOCK: Mutex<()> = Mutex::new(());

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

        fn host_zmx(&self) -> PathBuf {
            let app_name = self.app.file_name().unwrap_or_default();
            self.host_dir().join(app_name).join(&self.relative_zmx)
        }

        /// The cloned zmx, cloning the app first when this zmx has no copy yet.
        pub(super) fn ensure(&self) -> io::Result<PathBuf> {
            let host_zmx = self.host_zmx();
            if host_zmx.is_file() {
                return Ok(host_zmx);
            }
            let _guard = CLONE_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if host_zmx.is_file() {
                return Ok(host_zmx);
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
            remove_unused(
                &self.root,
                Some(&self.key),
                &crate::setup::running_executable_paths(),
            );
            Ok(host_zmx)
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

    /// Removes every copy but `current` that no running program was started from, and partial copies left by an ended gxserver.
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
