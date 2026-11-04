//! CDXC:Release 2026-09-22 DECISION:
//! User: Linux does not update itself as a first step because there are too many package managers; the user only needs to learn about the new version from the Update button before the project name and read its changelog in the same Ghostex Update dialog Windows shows, with a button that opens the download page.
//! User: the version and changelog are read from the Windows x64 Velopack feed already attached to every stable GitHub release, rather than from a Linux feed of its own.
//! CDXC:Release 2026-10-28 DECISION:
//! User, on whether Linux should update itself: "Yes, AppImage only". This supersedes the 2026-09-22 notice-only decision for the AppImage alone; deb, rpm, tarball and AUR installs keep the notice and its download-page button. Run from the AppImage's install copy (its `.ghostex-appimage` stamp), the dialog works as on Windows: Download update fetches this architecture's AppImage from the release, checks its embedded signature against the Ghostex AppImage Signing key compiled into this build (appimage_signature.rs), and Restart and update quits, lets the new AppImage's own launcher (tooling/packaging/appimage/AppRun) sync its payload over the install copy, and starts it. A hard-linked snapshot of the copy is taken first and put back if the sync does not finish, so a failed sync or a full disk leaves the version that was running. The version and changelog still come from the Windows feed.
//! SEE-ALSO: apps/desktop/src/app/os_integration/updater.rs (the check loop and the dialog actions), apps/desktop/src/app/window/update_available_modal.rs (the `Notify` state), tooling/release-gpui/windows.ps1 (cuts the CHANGELOG.md section into the feed's `NotesMarkdown`), tooling/release-gpui/linux-appimage.sh (signs the AppImage).
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

const RELEASE_FEED_URL: &str =
    "https://github.com/maddada/Ghostex/releases/latest/download/releases.win-x64-stable.json";
const RELEASE_PAGE_URL_PREFIX: &str = "https://github.com/maddada/Ghostex/releases/tag/v";
const PACKAGED_INSTALL_ROOT: &str = "/opt/ghostex";
/// The payload stamp the AppImage's launcher writes beside the copy it starts.
/// SEE-ALSO: tooling/packaging/appimage/AppRun.
const APPIMAGE_INSTALL_STAMP: &str = ".ghostex-appimage";
const FEED_MAX_BYTES: u64 = 4 * 1024 * 1024;
const RELEASE_DOWNLOAD_URL_PREFIX: &str = "https://github.com/maddada/Ghostex/releases/download/v";
/// No release AppImage comes near this; a larger download is not one.
const APPIMAGE_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Free space kept beyond the download and a second copy of the install.
const SPACE_MARGIN_BYTES: u64 = 128 * 1024 * 1024;
/// Where the AppImage launcher was started from, recorded by `record_appimage_source`.
const APPIMAGE_SOURCE_FILE: &str = "appimage-source";
const UPDATE_DIRECTORY: &str = "appimage-update";
const UPDATE_LOCK_FILE: &str = "appimage-update.lock";
const UPDATE_LOG_FILE: &str = "appimage-update.log";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinuxUpdate {
    version: String,
    notes_markdown: String,
}

impl LinuxUpdate {
    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn notes_markdown(&self) -> &str {
        &self.notes_markdown
    }

    pub(crate) fn release_page_url(&self) -> String {
        format!("{RELEASE_PAGE_URL_PREFIX}{}", self.version)
    }
}

/// Every Linux package (deb, rpm, tarball, AUR) installs under `/opt/ghostex`; a cargo build does not and runs without update checks, like a Windows build without its Velopack manifest.
/// CDXC:Release 2026-10-26 WHY:
/// The AppImage runs from the copy its launcher keeps in the user's data folder, marked by its payload stamp, so it gets the same update notice: the dialog's download page lists the new AppImage, and running that one replaces the copy.
pub(crate) fn is_packaged_install() -> bool {
    std::env::current_exe().is_ok_and(|exe| is_packaged_executable(&exe))
}

fn is_packaged_executable(exe: &Path) -> bool {
    exe.starts_with(Path::new(PACKAGED_INSTALL_ROOT))
        || exe
            .parent()
            .is_some_and(|directory| directory.join(APPIMAGE_INSTALL_STAMP).is_file())
}

/// The newest stable release when it is newer than this build. Blocking: call it off the main thread.
pub(crate) fn check_for_updates() -> Result<Option<LinuxUpdate>, String> {
    let mut response = http_agent(Duration::from_secs(20))
        .get(RELEASE_FEED_URL)
        .call()
        .map_err(|error| error.to_string())?;
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(FEED_MAX_BYTES)
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())?;
    newest_update(&body, env!("GHOSTEX_BUILD_MARKETING_VERSION"))
}

fn newest_update(feed_text: &str, current_version: &str) -> Result<Option<LinuxUpdate>, String> {
    let feed: serde_json::Value =
        serde_json::from_str(feed_text).map_err(|error| error.to_string())?;
    let assets = feed
        .get("Assets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "release feed has no Assets array".to_string())?;
    let current = parse_version(current_version)
        .ok_or_else(|| format!("unreadable build version {current_version}"))?;
    // Delta entries carry no notes; only the full package does.
    let newest = assets
        .iter()
        .filter(|asset| {
            asset
                .get("Type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("full"))
        })
        .filter_map(|asset| {
            let version = asset.get("Version")?.as_str()?;
            Some((parse_version(version)?, version, asset))
        })
        .max_by_key(|(parsed, _, _)| *parsed);
    Ok(newest
        .filter(|(parsed, _, _)| *parsed > current)
        .map(|(_, version, asset)| LinuxUpdate {
            version: version.to_string(),
            notes_markdown: asset
                .get("NotesMarkdown")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }))
}

/// Stable releases are plain `major.minor.patch`; anything else is not a stable version and is skipped.
fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim().split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

fn http_agent(timeout: Duration) -> ureq::Agent {
    let tls_config = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .tls_config(tls_config)
        .build();
    ureq::Agent::new_with_config(config)
}

/// The AppImage install copy this build runs from: `<data>/ghostex/appimage`, beside its stamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppImageInstall {
    install_dir: PathBuf,
    data_dir: PathBuf,
}

impl AppImageInstall {
    fn from_executable(exe: &Path) -> Option<Self> {
        let install_dir = exe.parent()?;
        if !install_dir.join(APPIMAGE_INSTALL_STAMP).is_file() {
            return None;
        }
        Some(Self {
            install_dir: install_dir.to_path_buf(),
            data_dir: install_dir.parent()?.to_path_buf(),
        })
    }
}

/// The AppImage install copy, when this build runs from one; deb, rpm, tarball and AUR do not.
pub(crate) fn appimage_install() -> Option<AppImageInstall> {
    AppImageInstall::from_executable(&std::env::current_exe().ok()?)
}

/// Remembers the AppImage file the launcher was started from (`$APPIMAGE`, set by the AppImage
/// runtime), so an update can replace that file too. A start from the launcher entry, which runs
/// the install copy directly, has none and keeps the last one recorded.
pub(crate) fn record_appimage_source(install: &AppImageInstall) {
    let Some(source) = std::env::var_os("APPIMAGE").map(PathBuf::from) else {
        return;
    };
    if source.is_absolute() && source.is_file() && !source.starts_with(&install.install_dir) {
        let _ = std::fs::write(
            install.data_dir.join(APPIMAGE_SOURCE_FILE),
            source.as_os_str().as_encoded_bytes(),
        );
    }
}

fn recorded_appimage_source(install: &AppImageInstall) -> Option<PathBuf> {
    let bytes = std::fs::read(install.data_dir.join(APPIMAGE_SOURCE_FILE)).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let path = PathBuf::from(text.trim());
    (path.is_absolute() && path.is_file()).then_some(path)
}

/// The release asset for this machine: `ghostex-<version>-linux-x64.AppImage` or `-arm64`.
pub(crate) fn appimage_asset_name(version: &str, rust_arch: &str) -> Option<String> {
    let arch = match rust_arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    parse_version(version)?;
    Some(format!("ghostex-{version}-linux-{arch}.AppImage"))
}

fn appimage_asset_url(version: &str, asset: &str) -> String {
    format!("{RELEASE_DOWNLOAD_URL_PREFIX}{version}/{asset}")
}

/// Whether `free` bytes hold a download of `download` bytes plus a second copy of the install.
fn enough_space(free: u64, download: u64, install: u64) -> bool {
    free >= download
        .saturating_add(install)
        .saturating_add(SPACE_MARGIN_BYTES)
}

fn free_bytes(directory: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt as _;
    let path = std::ffi::CString::new(directory.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a NUL-terminated string and `stats` a zeroed statvfs the call fills.
    (unsafe { libc::statvfs(path.as_ptr(), &mut stats) } == 0)
        .then(|| stats.f_bavail as u64 * stats.f_frsize as u64)
}

fn directory_bytes(directory: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => directory_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |meta| meta.len()),
            _ => 0,
        })
        .sum()
}

fn megabytes(bytes: u64) -> u64 {
    bytes.div_ceil(1024 * 1024)
}

/// Downloads and verifies the update's AppImage, reporting progress in percent. Returns the
/// verified file, ready for `apply_appimage_update_after_exit`. Blocking: call it off the main
/// thread. Errors are plain sentences for the user.
pub(crate) fn download_appimage_update(
    update: &LinuxUpdate,
    progress: std::sync::mpsc::Sender<i16>,
) -> Result<PathBuf, String> {
    let install = appimage_install()
        .ok_or_else(|| "This copy of Ghostex was not installed from the AppImage.".to_string())?;
    let asset = appimage_asset_name(&update.version, std::env::consts::ARCH)
        .ok_or_else(|| "There is no Ghostex AppImage for this kind of computer.".to_string())?;
    let directory = install.data_dir.join(UPDATE_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not make {}: {error}", directory.display()))?;
    // One download at a time, also across two running copies of Ghostex.
    let lock = std::fs::File::create(install.data_dir.join(UPDATE_LOCK_FILE))
        .map_err(|error| format!("Could not start the update: {error}"))?;
    lock.try_lock()
        .map_err(|_| "Another Ghostex window is already downloading this update.".to_string())?;
    // Earlier attempts leave nothing behind but the file this one replaces.
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    let url = appimage_asset_url(&update.version, &asset);
    let mut response = http_agent(Duration::from_secs(30 * 60))
        .get(&url)
        .call()
        .map_err(|error| match error {
            ureq::Error::StatusCode(404) => {
                format!("The {} release has no {asset} to download.", update.version)
            }
            other => format!("The download did not start: {other}"),
        })?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if total.is_some_and(|total| total > APPIMAGE_MAX_BYTES) {
        return Err("The download is larger than any Ghostex AppImage.".to_string());
    }
    let install_bytes = directory_bytes(&install.install_dir);
    let expected = total.unwrap_or(install_bytes);
    if let Some(free) = free_bytes(&directory)
        && !enough_space(free, expected, install_bytes)
    {
        return Err(format!(
            "Not enough disk space: the update needs about {} MB free in {}, and {} MB are free.",
            megabytes(expected + install_bytes + SPACE_MARGIN_BYTES),
            install.data_dir.display(),
            megabytes(free),
        ));
    }

    let partial = directory.join(format!("{asset}.part"));
    let result = (|| {
        let mut file = std::fs::File::create(&partial)
            .map_err(|error| format!("Could not save the download: {error}"))?;
        let mut reader = response.body_mut().as_reader();
        let mut buffer = vec![0u8; 256 * 1024];
        let mut received = 0u64;
        let mut last_percent = -1i16;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| format!("The download stopped: {error}"))?;
            if read == 0 {
                break;
            }
            received += read as u64;
            if received > APPIMAGE_MAX_BYTES {
                return Err("The download is larger than any Ghostex AppImage.".to_string());
            }
            file.write_all(&buffer[..read]).map_err(|error| {
                if error.raw_os_error() == Some(libc::ENOSPC) {
                    "The disk filled up during the download.".to_string()
                } else {
                    format!("Could not save the download: {error}")
                }
            })?;
            if let Some(total) = total.filter(|total| *total > 0) {
                let percent = ((received * 100) / total).min(100) as i16;
                if percent != last_percent {
                    last_percent = percent;
                    let _ = progress.send(percent);
                }
            }
        }
        if total.is_some_and(|total| total != received) {
            return Err("The download ended early.".to_string());
        }
        file.sync_all()
            .map_err(|error| format!("Could not save the download: {error}"))?;
        drop(file);
        let mut image = std::fs::File::open(&partial)
            .map_err(|error| format!("Could not read the download: {error}"))?;
        crate::appimage_signature::verify_appimage(
            &mut image,
            crate::appimage_signature::GHOSTEX_SIGNING_PUBLIC_KEY,
            crate::appimage_signature::GHOSTEX_SIGNING_FINGERPRINT,
        )?;
        let finished = directory.join(&asset);
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))
            .and_then(|_| std::fs::rename(&partial, &finished))
            .map_err(|error| format!("Could not save the download: {error}"))?;
        Ok(finished)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

/// Swaps in a verified AppImage once this process has quit, then starts it: the caller quits right
/// after this returns. See `SWAP_SCRIPT` for what happens, and when it is undone.
pub(crate) fn apply_appimage_update_after_exit(appimage: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt as _;
    let install = appimage_install()
        .ok_or_else(|| "This copy of Ghostex was not installed from the AppImage.".to_string())?;
    let source = recorded_appimage_source(&install);
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(SWAP_SCRIPT)
        .arg("ghostex-appimage-update")
        .arg(std::process::id().to_string())
        .arg(appimage)
        .arg(&install.install_dir)
        .arg(source.as_deref().unwrap_or(Path::new("")))
        .arg(install.data_dir.join(UPDATE_LOG_FILE))
        .arg(install.data_dir.join(UPDATE_LOCK_FILE))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // The new Ghostex starts in this desktop session, with the Wayland socket this process hid, and
    // without what this version's launcher set for this version: its Chromium folder and loader
    // path, and the X11 scale it pinned (which the next start would read as the user's own).
    crate::restore_wayland_display_for_process_command(&mut command);
    if let Some(cef_dir) = std::env::var_os(crate::cef_component_window::CEF_RUNTIME_DIR_ENV) {
        command.env_remove(crate::cef_component_window::CEF_RUNTIME_DIR_ENV);
        if let Some(paths) = std::env::var_os("LD_LIBRARY_PATH") {
            let kept: Vec<PathBuf> = std::env::split_paths(&paths)
                .filter(|path| path.as_os_str() != cef_dir)
                .collect();
            match std::env::join_paths(&kept) {
                Ok(joined) if !kept.is_empty() => command.env("LD_LIBRARY_PATH", joined),
                _ => command.env_remove("LD_LIBRARY_PATH"),
            };
        }
    }
    #[cfg(target_os = "linux")]
    if crate::linux_x11_scale::pinned_scale_factor().is_some() {
        command.env_remove("GPUI_X11_SCALE_FACTOR");
    }
    // SAFETY: setsid only detaches the child from this process's session; it allocates nothing.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start the update: {error}"))
}

/// Runs detached after Ghostex quits: `$1` Ghostex's pid, `$2` the verified AppImage, `$3` the
/// install copy, `$4` the AppImage file the user started (empty when unknown), `$5` a log, `$6` the
/// update lock. The new AppImage's launcher copies its payload over the install copy with
/// `cp --remove-destination` and writes the payload stamp last, then starts Ghostex from it. The
/// copy is snapshotted with hard links first (`cp -al`): unlinking a file leaves its old content in
/// the snapshot, so it costs no space. If the launcher exits before a new stamp appears (a full
/// disk, a missing FUSE, a copy that failed), the snapshot goes back and the previous version
/// starts again. A writable original AppImage file is replaced by the new one, under the new
/// version's name when it had a release name.
const SWAP_SCRIPT: &str = r#"
set -u
pid=$1 new=$2 install=$3 source=$4 log=$5 lock=$6
exec >>"$log" 2>&1
echo "$(date '+%F %T') update to $(basename "$new")"
if command -v flock >/dev/null 2>&1; then
	exec 9>"$lock"
	flock -n 9 || { echo "another update is running"; exit 0; }
fi
waited=0
while kill -0 "$pid" 2>/dev/null; do
	sleep 0.2
	waited=$((waited + 1))
	[ "$waited" -lt 900 ] || { echo "Ghostex did not quit; nothing changed"; exit 1; }
done
stamp="$install/.ghostex-appimage"
old_id=$(cat "$stamp" 2>/dev/null || true)
snapshot="$install.previous"
rm -rf "$snapshot"
cp -al "$install" "$snapshot" || { echo "could not snapshot the install copy; nothing changed"; rm -rf "$snapshot"; exec "$install/Ghostex" >/dev/null 2>&1; }
# Ghostex writes its own logs; only the update's lines go to this one.
"$new" >/dev/null 2>&1 &
launcher=$!
installed() { id=$(cat "$stamp" 2>/dev/null || true); [ -n "$id" ] && [ "$id" != "$old_id" ]; }
while ! installed; do
	if ! kill -0 "$launcher" 2>/dev/null; then
		installed && break
		echo "the new version did not install; putting $old_id back"
		rm -rf "$install" && mv "$snapshot" "$install"
		exec "$install/Ghostex" >/dev/null 2>&1
	fi
	sleep 0.3
done
echo "installed $(cat "$stamp")"
rm -rf "$snapshot"
if [ -n "$source" ] && [ -f "$source" ]; then
	dir=$(dirname "$source")
	case "$(basename "$source")" in
	ghostex-*-linux-*.AppImage) target="$dir/$(basename "$new")" ;;
	*) target="$source" ;;
	esac
	if [ -w "$dir" ] && cp "$new" "$dir/.ghostex-update.part" && chmod 755 "$dir/.ghostex-update.part" && mv -f "$dir/.ghostex-update.part" "$target"; then
		[ "$target" = "$source" ] || rm -f "$source"
		echo "replaced $source with $target"
	else
		rm -f "$dir/.ghostex-update.part"
		echo "left $source as it was"
	fi
fi
rm -f "$new"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_and_the_appimage_copy_check_for_updates_but_a_cargo_build_does_not() {
        assert!(is_packaged_executable(Path::new(
            "/opt/ghostex/ghostex-gpui-runtime"
        )));

        let root =
            std::env::temp_dir().join(format!("ghostex-linux-updater-{}", std::process::id()));
        let appimage_copy = root.join("appimage");
        std::fs::create_dir_all(&appimage_copy).unwrap();
        let runtime = appimage_copy.join("ghostex-gpui-runtime");
        assert!(!is_packaged_executable(&runtime));
        std::fs::write(
            appimage_copy.join(APPIMAGE_INSTALL_STAMP),
            "10.37.0-0123456789abcdef\n",
        )
        .unwrap();
        assert!(is_packaged_executable(&runtime));
        assert!(!is_packaged_executable(Path::new(
            "/home/me/Ghostex/apps/desktop/target/release/ghostex-gpui"
        )));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_a_newer_stable_release_is_an_update() {
        let feed = |versions: &[(&str, &str)]| {
            let assets: Vec<_> = versions
                .iter()
                .map(|(version, kind)| {
                    serde_json::json!({ "Version": version, "Type": kind, "NotesMarkdown": format!("notes {version}") })
                })
                .collect();
            serde_json::json!({ "Assets": assets }).to_string()
        };
        let releases = feed(&[
            ("10.38.1", "Full"),
            ("10.38.1", "Delta"),
            ("10.38.0", "Full"),
        ]);
        let update = newest_update(&releases, "10.38.0").unwrap().unwrap();
        assert_eq!(update.version(), "10.38.1");
        assert_eq!(update.notes_markdown(), "notes 10.38.1");
        assert_eq!(newest_update(&releases, "10.38.1").unwrap(), None);
        assert_eq!(newest_update(&releases, "10.39.0").unwrap(), None);
        // Numbers compare as numbers, and a pre-release tag is not a stable version.
        let releases = feed(&[
            ("10.40.0", "Full"),
            ("10.100.0", "Full"),
            ("11.0.0-beta.1", "Full"),
        ]);
        assert_eq!(
            newest_update(&releases, "10.38.0")
                .unwrap()
                .unwrap()
                .version(),
            "10.100.0"
        );
        assert!(newest_update(&releases, "not a version").is_err());
    }

    #[test]
    fn the_asset_matches_this_computer() {
        assert_eq!(
            appimage_asset_name("10.38.1", "x86_64").as_deref(),
            Some("ghostex-10.38.1-linux-x64.AppImage")
        );
        assert_eq!(
            appimage_asset_name("10.38.1", "aarch64").as_deref(),
            Some("ghostex-10.38.1-linux-arm64.AppImage")
        );
        assert_eq!(appimage_asset_name("10.38.1", "riscv64"), None);
        assert_eq!(appimage_asset_name("../../evil", "x86_64"), None);
        assert_eq!(
            appimage_asset_url("10.38.1", "ghostex-10.38.1-linux-x64.AppImage"),
            "https://github.com/maddada/Ghostex/releases/download/v10.38.1/ghostex-10.38.1-linux-x64.AppImage"
        );
    }

    #[test]
    fn the_space_check_counts_the_download_and_a_second_install() {
        let mb = 1024 * 1024;
        assert!(enough_space(1000 * mb, 300 * mb, 500 * mb));
        assert!(!enough_space(800 * mb, 300 * mb, 500 * mb));
        assert!(!enough_space(0, u64::MAX, u64::MAX));
    }

    #[test]
    fn only_the_appimage_copy_updates_itself() {
        let root =
            std::env::temp_dir().join(format!("ghostex-appimage-install-{}", std::process::id()));
        let install_dir = root.join("ghostex").join("appimage");
        std::fs::create_dir_all(&install_dir).unwrap();
        let exe = install_dir.join("ghostex-gpui-runtime");
        assert_eq!(AppImageInstall::from_executable(&exe), None);
        std::fs::write(install_dir.join(APPIMAGE_INSTALL_STAMP), "10.38.0-abc\n").unwrap();
        let install = AppImageInstall::from_executable(&exe).unwrap();
        assert_eq!(install.data_dir, root.join("ghostex"));
        assert_eq!(
            AppImageInstall::from_executable(Path::new("/opt/ghostex/ghostex-gpui-runtime")),
            None
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
