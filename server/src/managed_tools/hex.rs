//! Hex 3 (anomalyco/hex), the local dictation app whose local API Dictate's Hex engine talks to
//! (`voice_agent/speech/hex.rs`): found, installed and updated the way Hex's own instructions do it.
//!
//! - macOS (Apple silicon only): `Hex.app` from the Anomaly release service's stable manifest
//!   (size and SHA-256 checked, then the code signature Hex's own updater requires), put in
//!   /Applications (or ~/Applications when Hex is already there).
//! - Linux (x64, arm64): Hex's `install-linux.sh`, which checks the release's ed25519 signature
//!   and installs into `~/.local/share/hex` with `hex` in `~/.local/bin`.
//! - Windows (x64, arm64): Hex's `install-windows.ps1`, which checks the Authenticode signature
//!   and installs into `%APPDATA%\hex`.
//!
//! CDXC:Dictate 2026-11-07 DECISION:
//! User: Ghostex's install and Updates surfaces cover Hex: the Updates list shows Hex with its installed version, says when a newer Hex release is out (GitHub releases of anomalyco/hex), and offers to install it when it's missing, on macOS, Linux and Windows. Installing or updating happens only when the user clicks, so Hex never updates automatically from Ghostex.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{
    download::{self, Scratch},
    jobs::Log,
    platform::{self, Arch, Os},
    tools::{self, Source, Status},
};

/// Where the newest release is read from (the user's pick: GitHub releases of anomalyco/hex).
pub(crate) const RELEASES_LATEST: &str = "https://github.com/anomalyco/hex/releases/latest";
/// The download page the Settings row links to when Ghostex can't install Hex here.
pub(crate) const DOWNLOAD_PAGE: &str = "https://github.com/anomalyco/hex#downloads";
/// The Anomaly release service Hex's own updater and installers use.
const RELEASE_ORIGIN: &str = "https://anoma.ly";
const MANIFEST_PATH: &str = "/release/hex/desktop/stable.json";
const LINUX_INSTALLER: &str = "https://anoma.ly/hex/install-linux.sh";
const WINDOWS_INSTALLER: &str = "https://anoma.ly/hex/install-windows.ps1";

/// Hex 3's bundle identifier; the Swift Hex before it (0.x, no local API) used the old one.
const MAC_BUNDLE_ID: &str = "anomaly.hex";
const OLD_MAC_BUNDLE_ID: &str = "com.kitlangton.Hex";
/// What Hex's own updater requires of a new `Hex.app` (`app/update-requirement.txt` in Hex).
const MAC_REQUIREMENT: &str = r#"anchor apple generic and identifier "anomaly.hex" and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = "5NZ4Q7NXJ4" and notarized"#;
const INSTALLER_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Whether Hex 3 ships for this computer (its README's downloads table: macOS on Apple silicon,
/// Windows and Linux on x64 and arm64).
pub(crate) fn supported() -> Result<(), String> {
    match (platform::os(), platform::arch()) {
        (Os::Mac, Some(Arch::Arm64)) => Ok(()),
        (Os::Mac, _) => Err("Hex 3 runs only on Macs with Apple silicon.".to_string()),
        (Os::Linux | Os::Windows, Some(_)) => Ok(()),
        (_, None) => Err(format!(
            "Hex 3 has no build for a {} processor.",
            std::env::consts::ARCH
        )),
    }
}

/// A Hex found on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    /// Hex 3 or newer, which has the local API.
    Current {
        version: Option<String>,
        path: PathBuf,
        /// Installed by a package manager (a `.deb` or `.rpm`), which updates it.
        packaged: bool,
    },
    /// The Swift Hex before 3 (macOS only), which has no local API.
    Older {
        version: Option<String>,
        path: PathBuf,
    },
}

/// Hex's own data folder (`dirs::data_dir()/hex` in Hex's `app_paths.rs`, or
/// `HEX_APPLICATION_SUPPORT_DIR`), where it writes `local-api.json`.
pub(crate) fn support_dir(home: &Path) -> PathBuf {
    support_dir_from(home, |name| std::env::var_os(name))
}

pub(crate) fn support_dir_from(
    home: &Path,
    env: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(dir) = env("HEX_APPLICATION_SUPPORT_DIR").filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    let absolute = |name: &str| {
        env(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let data = match platform::os() {
        Os::Mac => home.join("Library").join("Application Support"),
        Os::Windows => absolute("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming")),
        Os::Linux => absolute("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share")),
    };
    data.join("hex")
}

/// The Hex on this computer, if any.
pub(crate) fn find(home: &Path) -> Option<Found> {
    match platform::os() {
        Os::Mac => find_mac(&mac_app_dirs(home)),
        Os::Linux => find_direct(&support_dir(home), home),
        Os::Windows => find_direct(&support_dir(home), home),
    }
}

fn mac_app_dirs(home: &Path) -> [PathBuf; 2] {
    [PathBuf::from("/Applications"), home.join("Applications")]
}

/// `Hex.app` in the first of `dirs` that has one, told apart by its bundle identifier.
pub(crate) fn find_mac(dirs: &[PathBuf]) -> Option<Found> {
    dirs.iter().find_map(|dir| {
        let app = dir.join("Hex.app");
        let plist = app.join("Contents").join("Info.plist");
        if !plist.is_file() {
            return None;
        }
        let version = plist_value(&plist, "CFBundleShortVersionString");
        match plist_value(&plist, "CFBundleIdentifier").as_deref() {
            Some(MAC_BUNDLE_ID) => Some(Found::Current {
                version,
                path: app,
                packaged: false,
            }),
            Some(OLD_MAC_BUNDLE_ID) => Some(Found::Older { version, path: app }),
            _ => None,
        }
    })
}

/// One key of an `Info.plist` (XML or binary), read with macOS's `plutil`.
fn plist_value(plist: &Path, key: &str) -> Option<String> {
    let output = crate::platform::process::background_command("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(plist)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}

/// Linux and Windows: the direct install's current version (Linux `current -> versions/<v>`,
/// Windows `current-version`), else a packaged `hex` (Linux `.deb`/`.rpm` put it in /usr/bin).
fn find_direct(support: &Path, home: &Path) -> Option<Found> {
    if let Some(version) = direct_version(support) {
        let path = match platform::os() {
            Os::Windows => support.join("bin").join("hex.com"),
            _ => support.join("current").join("hex"),
        };
        return Some(Found::Current {
            version: Some(version),
            path,
            packaged: false,
        });
    }
    let (path, _) = tools::locate("hex", home)?;
    // Another program may be called `hex`; Hex's own `--version` starts with its name.
    let output = {
        let mut command = crate::platform::process::background_command(&path);
        command.arg("--version");
        super::run::output_within(command, super::run::PROBE_TIMEOUT).ok()?
    };
    let text = format!("{}\n{}", output.stdout, output.stderr);
    if !text.trim_start().to_ascii_lowercase().starts_with("hex") {
        return None;
    }
    let packaged = path.starts_with("/usr");
    Some(Found::Current {
        version: tools::version_in(&text),
        path,
        packaged,
    })
}

pub(crate) fn direct_version(support: &Path) -> Option<String> {
    let text = match platform::os() {
        Os::Windows => std::fs::read_to_string(support.join("current-version")).ok()?,
        _ => std::fs::read_link(support.join("current"))
            .ok()?
            .to_string_lossy()
            .into_owned(),
    };
    let version = text
        .trim()
        .trim_start_matches("versions/")
        .trim_matches('/');
    (!version.is_empty()).then(|| version.to_string())
}

pub(crate) fn latest_version() -> Result<String, String> {
    download::latest_release_tag(RELEASES_LATEST).map(|tag| tag.trim_start_matches('v').to_string())
}

/// The Tools row's and Updates list's view of Hex.
pub(crate) fn status(home: &Path) -> Status {
    let found = find(home);
    if let Err(reason) = supported() {
        let mut status = Status::unsupported(reason);
        status.download_url = Some(DOWNLOAD_PAGE.to_string());
        return status;
    }
    let mut status = Status::new(plan().to_string());
    status.download_url = Some(DOWNLOAD_PAGE.to_string());
    match found {
        None => status.operations = vec!["install"],
        Some(Found::Current {
            version,
            path,
            packaged,
        }) => {
            status.version = version;
            status.executable = Some(path);
            status.source = Some(Source::System);
            if packaged {
                status.detail =
                    Some("Installed by your package manager, which keeps it updated.".to_string());
            } else {
                status.operations = vec!["update"];
            }
        }
        Some(Found::Older { version, path }) => {
            status.detail = Some(format!(
                "Hex {} has no local API, so Dictate can't use it. Update installs Hex 3 in its place.",
                version.as_deref().unwrap_or("here")
            ));
            status.version = version;
            status.executable = Some(path);
            status.source = Some(Source::System);
            status.operations = vec!["update"];
        }
    }
    status
}

fn plan() -> &'static str {
    match platform::os() {
        Os::Mac => {
            "Downloads Hex.app from Hex's release service, checks its size, SHA-256 and Hex's code signature, and puts it in Applications (replacing an older Hex there). Hex quits and opens again if it was running. No password needed."
        }
        Os::Linux => {
            "Runs Hex's own installer (anoma.ly/hex/install-linux.sh), which checks the release's signature and installs Hex for you in ~/.local/share/hex with hex in ~/.local/bin. No password needed."
        }
        Os::Windows => {
            "Runs Hex's own installer (anoma.ly/hex/install-windows.ps1), which checks the release's signature and installs Hex for you in %APPDATA%\\hex. No administrator needed."
        }
    }
}

/// Install and Update are the same: the newest stable Hex, the way Hex installs itself here.
pub(crate) fn install(home: &Path, log: &Log) -> Result<(), String> {
    supported()?;
    match platform::os() {
        Os::Mac => {
            let dirs = mac_app_dirs(home);
            // Replace a Hex where it is; a new one goes into /Applications.
            let target = dirs
                .iter()
                .find(|dir| dir.join("Hex.app").exists())
                .unwrap_or(&dirs[0])
                .clone();
            install_mac(RELEASE_ORIGIN, &target, true, log).map(|_| ())
        }
        Os::Linux => {
            let scratch = Scratch::new()?;
            let script = scratch.0.join("install-linux.sh");
            download::download(LINUX_INSTALLER, &script, "Hex's installer", log)?;
            log.line("Running Hex's installer.");
            super::run::run(
                "/bin/sh",
                &[&script.to_string_lossy(), "install"],
                &[],
                &[],
                log,
                INSTALLER_TIMEOUT,
            )
        }
        Os::Windows => {
            let scratch = Scratch::new()?;
            let script = scratch.0.join("install-windows.ps1");
            download::download(WINDOWS_INSTALLER, &script, "Hex's installer", log)?;
            log.line("Running Hex's installer.");
            super::run::run(
                "powershell.exe",
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &script.to_string_lossy(),
                ],
                &[],
                &[],
                log,
                INSTALLER_TIMEOUT,
            )
        }
    }?;
    tools::forget_latest(tools::ToolId::Hex);
    Ok(())
}

/// One file of a release, as the manifest names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) version: String,
    pub(crate) url: String,
    pub(crate) size: u64,
    pub(crate) sha256: String,
}

/// `release.targets[name]` of the release service's manifest, its URL resolved against `origin`
/// (the manifest gives absolute paths). Only HTTPS, or a loopback address (a test feed).
pub(crate) fn manifest_target(
    manifest: &Value,
    name: &str,
    origin: &str,
) -> Result<Target, String> {
    let release = &manifest["release"];
    let version = release["version"]
        .as_str()
        .filter(|version| !version.is_empty())
        .ok_or("Hex's release feed names no version.")?;
    let target = &release["targets"][name];
    let path = target["url"]
        .as_str()
        .ok_or_else(|| format!("Hex's release feed has no {name} download."))?;
    let url = if path.starts_with('/') {
        format!("{}{path}", origin.trim_end_matches('/'))
    } else {
        path.to_string()
    };
    let loopback = url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:");
    if !url.starts_with("https://") && !loopback {
        return Err("Hex's release feed gave a download that isn't HTTPS.".to_string());
    }
    let size = target["size"]
        .as_u64()
        .filter(|size| *size > 0)
        .ok_or("Hex's release feed gave no size for the download.")?;
    let sha256 = target["sha256"]
        .as_str()
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("Hex's release feed gave no SHA-256 for the download.")?
        .to_ascii_lowercase();
    Ok(Target {
        version: version.to_string(),
        url,
        size,
        sha256,
    })
}

/// Downloads the newest `Hex.app` from `origin`'s manifest and puts it in `apps_dir`, replacing
/// the Hex there. Refuses a download whose size, SHA-256, bundle identifier or code signature
/// isn't Hex's. `reopen` opens Hex afterwards when it was running (or wasn't installed).
pub(crate) fn install_mac(
    origin: &str,
    apps_dir: &Path,
    reopen: bool,
    log: &Log,
) -> Result<PathBuf, String> {
    let manifest = download::get_json(&format!("{}{MANIFEST_PATH}", origin.trim_end_matches('/')))?;
    let target = manifest_target(&manifest, "macos-arm64", origin)?;
    log.line(&format!(
        "Installing Hex {} from Hex's release service.",
        target.version
    ));
    let scratch = Scratch::new()?;
    let archive = scratch.0.join("Hex.zip");
    download::download(&target.url, &archive, "Hex", log)?;
    let size = std::fs::metadata(&archive)
        .map(|meta| meta.len())
        .unwrap_or(0);
    if size != target.size {
        return Err("The Hex download wasn't the size Hex's release feed gave, so Ghostex did not install it. Try again.".to_string());
    }
    download::verify(&archive, &target.sha256, "Hex", log)?;
    let unpacked = scratch.0.join("unpacked");
    let status = crate::platform::process::background_command("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&archive)
        .arg(&unpacked)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("Could not unpack Hex: {error}"))?;
    let app = unpacked.join("Hex.app");
    if !status.success() || !app.is_dir() {
        return Err("The Hex download didn't contain Hex.app.".to_string());
    }
    if plist_value(
        &app.join("Contents").join("Info.plist"),
        "CFBundleIdentifier",
    )
    .as_deref()
        != Some(MAC_BUNDLE_ID)
    {
        return Err("The downloaded app isn't Hex, so Ghostex did not install it.".to_string());
    }
    let signed = crate::platform::process::background_command("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(format!("-R={MAC_REQUIREMENT}"))
        .arg(&app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !signed {
        return Err(
            "The downloaded Hex isn't signed by Hex's developer, so Ghostex did not install it."
                .to_string(),
        );
    }
    log.line("Hex's code signature verified.");
    std::fs::create_dir_all(apps_dir)
        .map_err(|error| format!("Could not create {}: {error}", apps_dir.display()))?;
    // Copied next to its place first, so the swap is a rename on one volume.
    let staged = apps_dir.join(format!(".Hex.app.new-{}", uuid::Uuid::new_v4().simple()));
    let copied = crate::platform::process::background_command("/usr/bin/ditto")
        .arg(&app)
        .arg(&staged)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !copied {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!(
            "Ghostex couldn't write to {}. Install Hex from {DOWNLOAD_PAGE} instead.",
            apps_dir.display()
        ));
    }
    let destination = apps_dir.join("Hex.app");
    let was_installed = destination.exists();
    let was_running = quit_running_hex(log);
    if let Err(error) = download::replace_path(&staged, &destination) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(error);
    }
    log.line(&format!(
        "Hex {} is installed in {}.",
        target.version,
        apps_dir.display()
    ));
    if reopen && (was_running || !was_installed) {
        let _ = crate::platform::process::background_command("/usr/bin/open")
            .arg(&destination)
            .stdin(Stdio::null())
            .status();
        log.line("Opened Hex.");
    }
    Ok(destination)
}

/// Quits a running Hex (the new or the old one) before its app is replaced; says whether one was.
fn quit_running_hex(log: &Log) -> bool {
    let mut was_running = false;
    for id in [MAC_BUNDLE_ID, OLD_MAC_BUNDLE_ID] {
        if !mac_app_running(id) {
            continue;
        }
        was_running = true;
        log.line("Quitting Hex to replace it.");
        let _ = crate::platform::process::background_command("/usr/bin/osascript")
            .args(["-e", &format!("tell application id \"{id}\" to quit")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let started = Instant::now();
        while mac_app_running(id) && started.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    was_running
}

/// Asks without launching the app (`is running` never starts it).
fn mac_app_running(id: &str) -> bool {
    crate::platform::process::background_command("/usr/bin/osascript")
        .args(["-e", &format!("application id \"{id}\" is running")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
}
