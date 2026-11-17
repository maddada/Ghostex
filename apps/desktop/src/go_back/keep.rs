//! Keeping the running app before an update replaces it (Q3729: the whole old app, so going back
//! takes seconds and works offline).
//!
//! macOS: Sparkle's will-install delegate (native/macos/GpuiSparkleUpdater.m) calls
//! `GhostexGpuiKeepAppBeforeUpdate`, which clones the bundle into the data folder. A clone on
//! the same APFS volume costs no space until the update rewrites the files; on another volume it
//! is a full copy (`ditto`).
//! Windows: the finished Velopack download copies `current` (the folder Ghostex runs from) before
//! Restart and update hands it to Velopack (os_integration/updater.rs).
//! Linux AppImage: the update's swap script already snapshots the install copy with hard links;
//! it keeps that snapshot here instead of deleting it (linux_updater.rs `SWAP_SCRIPT`).
use super::{KeptApp, THIS_VERSION, kept_dir, now_ms, write_kept};
use std::path::{Path, PathBuf};

/// The `.app` this process runs from.
#[cfg(target_os = "macos")]
pub(crate) fn running_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(Path::to_path_buf)
}

/// Where the kept app lives, when there is one.
pub(crate) fn kept_app_path() -> Option<PathBuf> {
    let path = kept_app_location()?;
    path.is_dir().then_some(path)
}

fn kept_app_location() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let bundle = running_app_bundle()?;
        Some(kept_dir().join(bundle.file_name()?))
    }
    #[cfg(target_os = "windows")]
    {
        Some(kept_dir().join("app"))
    }
    #[cfg(target_os = "linux")]
    {
        Some(kept_dir().join("appimage"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// The folder an AppImage update moves its snapshot of the old install copy to. Gated like
/// `linux_updater` (main.rs), which test builds compile on every platform.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn kept_appimage_location() -> PathBuf {
    kept_dir().join("appimage")
}

/// Copies the running app to the kept place, replacing what was kept before. Returns why not.
pub(crate) fn keep_running_app() -> Result<(), String> {
    if !super::release_install() {
        return Err("not a release install".to_string());
    }
    let destination = kept_app_location().ok_or("no place to keep it")?;
    let source = running_app_root().ok_or("can't tell where Ghostex runs from")?;
    let directory = kept_dir();
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let partial = destination.with_extension("partial");
    remove_dir(&partial);
    copy_app(&source, &partial)?;
    remove_dir(&destination);
    std::fs::rename(&partial, &destination).map_err(|error| {
        remove_dir(&partial);
        error.to_string()
    })?;
    write_kept(&KeptApp {
        version: THIS_VERSION.to_string(),
        build: running_build_number(&source),
        kept_at_ms: now_ms(),
    });
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::HostLifecycle,
        "goBack.kept",
        serde_json::json!({ "version": THIS_VERSION }),
    );
    Ok(())
}

/// What gets kept: the bundle on macOS, the folder the executable is in elsewhere.
fn running_app_root() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        running_app_bundle()
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::current_exe()
            .ok()?
            .parent()
            .map(Path::to_path_buf)
    }
}

fn remove_dir(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_dir_all(path);
    }
}

#[cfg(target_os = "macos")]
fn copy_app(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt as _;
    let source_c =
        std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let destination_c = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .map_err(|error| error.to_string())?;
    // SAFETY: both are NUL-terminated paths that outlive the call.
    if unsafe { libc::clonefile(source_c.as_ptr(), destination_c.as_ptr(), 0) } == 0 {
        return Ok(());
    }
    remove_dir(destination);
    // Another volume: a real copy, keeping signatures, extended attributes and symlinks.
    let status = std::process::Command::new("/usr/bin/ditto")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("ditto exited with {status}"))
}

#[cfg(not(target_os = "macos"))]
fn copy_app(source: &Path, destination: &Path) -> Result<(), String> {
    copy_tree(source, destination).map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(entry.path())?, &target)?;
            #[cfg(windows)]
            std::fs::copy(entry.path(), &target).map(|_| ())?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// The build number Sparkle compares (CFBundleVersion), so going back can skip this version.
#[cfg(target_os = "macos")]
fn running_build_number(bundle: &Path) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleVersion", "raw", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .ok()?;
    let build = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !build.is_empty()).then_some(build)
}

#[cfg(not(target_os = "macos"))]
fn running_build_number(_root: &Path) -> Option<String> {
    None
}

/// Sparkle is about to replace this app (`updater:willInstallUpdate:` and the install-on-quit
/// path in GpuiSparkleUpdater.m).
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiKeepAppBeforeUpdate() {
    if let Err(reason) = keep_running_app() {
        crate::support_logs::append(
            crate::support_logs::GpuiSupportLog::HostLifecycle,
            "goBack.keepFailed",
            serde_json::json!({ "reason": reason }),
        );
    }
}
