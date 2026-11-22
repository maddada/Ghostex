//! How this Linux computer's Ghostex was installed, and the command that updates it.
//!
//! CDXC:Updates 2026-11-18 DECISION:
//! User (Q83, "Show the command and a link", after "i really dont know how to update"; it replaced Autopilot's earlier Q81 pick "Update runs the right command"): on Linux the Ghostex card works out how Ghostex was installed and shows the command that updates it, in words ("To update, run this in a terminal:") with Copy command, and the release page link is always shown; Ghostex does not run the command or open a terminal. The titlebar's Linux update dialog shows the same line and command with Later, Release page and Copy command. gxserver answers it because it runs from the same package as the app (`/opt/ghostex/gxserver/bin`), so the package that owns the app next to it is the one to update, and every client (this desktop, another computer's Settings > Updates, the phone) reads the same answer in `/api/appUpdateStatus` (`linuxUpdate`). The AppImage keeps its own download and swap (linux_updater.rs); a tarball, or an install no package manager owns, gets the release page link only.
//! SEE-ALSO: apps/desktop/src/app/os_integration/linux_update_command.rs (the dialog), apps/desktop/src/app/window/settings_modal/tabs/updates/card.rs, apps/desktop/src/linux_updater.rs, tooling/release-gpui/customer-downloads.mjs (the asset names).
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde_json::{Value, json};

const RELEASE_TAG_URL: &str = "https://github.com/maddada/Ghostex/releases/tag/v";
const RELEASE_LATEST_URL: &str = "https://github.com/maddada/Ghostex/releases/latest";
const RELEASE_DOWNLOAD_URL: &str = "https://github.com/maddada/Ghostex/releases/download/v";
/// The AUR package Ghostex publishes (tooling/release-gpui/publish-aur.mjs).
const AUR_PACKAGE: &str = "ghostex-bin";
/// The AppImage launcher's stamp beside the copy it runs (tooling/packaging/appimage/AppRun).
const APPIMAGE_INSTALL_STAMP: &str = ".ghostex-appimage";

#[derive(Clone, Debug, PartialEq, Eq)]
enum InstallKind {
    /// Owned by a pacman package (the AUR's `ghostex-bin`).
    Pacman(String),
    Deb,
    Rpm,
    AppImage,
    /// The tarball, or files no package manager knows.
    Manual,
}

impl InstallKind {
    fn name(&self) -> &'static str {
        match self {
            Self::Pacman(_) => "pacman",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::AppImage => "appimage",
            Self::Manual => "manual",
        }
    }
}

/// Worked out once: the package that owns the app does not change while this gxserver runs (an
/// update restarts it from the new build).
fn install_kind() -> &'static InstallKind {
    static KIND: OnceLock<InstallKind> = OnceLock::new();
    KIND.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .map(|exe| detect(&exe))
            .unwrap_or(InstallKind::Manual)
    })
}

/// `<install>/gxserver/bin/gxserver` sits beside the app executable `<install>/Ghostex`.
fn detect(gxserver_exe: &Path) -> InstallKind {
    let install = gxserver_exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent);
    if install.is_some_and(|install| install.join(APPIMAGE_INSTALL_STAMP).is_file()) {
        return InstallKind::AppImage;
    }
    let app = install
        .map(|install| install.join("Ghostex"))
        .filter(|app| app.is_file())
        .unwrap_or_else(|| gxserver_exe.to_path_buf());
    let owner = |program: &str, args: &[&str]| -> Option<String> {
        on_path(program)?;
        let output = Command::new(program).args(args).arg(&app).output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|text| !text.is_empty())
    };
    if let Some(package) = owner("pacman", &["-Qqo"]) {
        return InstallKind::Pacman(package.lines().next().unwrap_or(&package).to_string());
    }
    if owner("dpkg", &["-S"]).is_some() {
        return InstallKind::Deb;
    }
    if owner("rpm", &["-qf"]).is_some() {
        return InstallKind::Rpm;
    }
    InstallKind::Manual
}

/// The first `program` on the PATH a terminal would have.
fn on_path(program: &str) -> Option<PathBuf> {
    let path = crate::managed_tools::run::job_path(&[]);
    let directories: Vec<PathBuf> = std::env::split_paths(&path).collect();
    crate::platform::live_path::find_in(program, &directories)
}

fn first_on_path<'a>(programs: &[&'a str]) -> Option<&'a str> {
    programs
        .iter()
        .copied()
        .find(|program| on_path(program).is_some())
}

/// The release asset's processor name, as the release names it (`deb`, else `rpm`).
fn asset_arch(deb: bool) -> Option<&'static str> {
    match (std::env::consts::ARCH, deb) {
        ("x86_64", true) => Some("amd64"),
        ("aarch64", true) => Some("arm64"),
        ("x86_64", false) => Some("x86_64"),
        ("aarch64", false) => Some("aarch64"),
        _ => None,
    }
}

/// `cd` into a fresh folder, download `file` from the release, then hand it to `install`.
fn download_then(version: &str, file: &str, install: &str) -> Option<String> {
    let url = format!("{RELEASE_DOWNLOAD_URL}{version}/{file}");
    let download = match first_on_path(&["curl", "wget"])? {
        "curl" => format!("curl -fLO {url}"),
        _ => format!("wget {url}"),
    };
    Some(format!(
        "cd \"$(mktemp -d)\" && {download} && {install} ./{file}"
    ))
}

/// A plain stable version only (`11.9.0`): it goes into a command.
fn safe_version(version: &str) -> Option<&str> {
    let parts: Vec<&str> = version.split('.').collect();
    (parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())))
    .then_some(version)
}

/// The command that updates Ghostex installed as `kind` to `version`, or `None` when only the
/// release page can (the AppImage updates itself; a tarball has no package manager).
fn update_command(kind: &InstallKind, version: Option<&str>) -> Option<String> {
    match kind {
        InstallKind::Pacman(package) => {
            if !package
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.+@".contains(&byte))
            {
                return None;
            }
            if let Some(helper) = first_on_path(&["yay", "paru"]) {
                Some(format!("{helper} -S {package}"))
            } else if package == AUR_PACKAGE {
                Some(format!(
                    "cd \"$(mktemp -d)\" && git clone https://aur.archlinux.org/{AUR_PACKAGE}.git && cd {AUR_PACKAGE} && makepkg -si"
                ))
            } else {
                Some(format!("sudo pacman -S {package}"))
            }
        }
        InstallKind::Deb => {
            let version = safe_version(version?)?;
            let file = format!("ghostex_{version}_{}.deb", asset_arch(true)?);
            download_then(version, &file, "sudo apt install")
        }
        InstallKind::Rpm => {
            let version = safe_version(version?)?;
            let file = format!("ghostex-{version}-1.{}.rpm", asset_arch(false)?);
            // Release rpms are not GPG signed; zypper refuses those unless told.
            let install = match first_on_path(&["dnf", "zypper"])? {
                "dnf" => "sudo dnf install",
                _ => "sudo zypper install --allow-unsigned-rpm",
            };
            download_then(version, &file, install)
        }
        InstallKind::AppImage | InstallKind::Manual => None,
    }
}

fn release_url(version: Option<&str>) -> String {
    match version.and_then(safe_version) {
        Some(version) => format!("{RELEASE_TAG_URL}{version}"),
        None => RELEASE_LATEST_URL.to_string(),
    }
}

/// `{ kind, command, releaseUrl }` for a Linux app, `null` elsewhere. Only an available version
/// gets a command: there is nothing to run without one.
pub(super) fn linux_update(app_platform: Option<&str>, available_version: Option<&str>) -> Value {
    if !cfg!(target_os = "linux") || app_platform != Some("linux") {
        return Value::Null;
    }
    let kind = install_kind();
    let command = available_version.and_then(|version| update_command(kind, Some(version)));
    json!({
        "kind": kind.name(),
        "command": command,
        "releaseUrl": release_url(available_version),
    })
}

/// Why the app can't update itself, in words every client shows as they are (phones and desktops
/// from before `linuxUpdate` read only this): the command, or where to download.
pub(super) fn unsupported_reason(
    linux_update: &Value,
    available_version: Option<&str>,
) -> Option<String> {
    available_version?;
    if let Some(command) = linux_update["command"].as_str() {
        return Some(format!("Update it in a terminal: {command}"));
    }
    match linux_update["kind"].as_str()? {
        "appimage" => Some("Use the update button in Ghostex's window there.".to_string()),
        _ => Some("Download the new version from github.com/maddada/Ghostex/releases.".to_string()),
    }
}
