//! PowerShell 7 on Windows, installed with the Windows Package Manager. Where it can already be
//! found is `ghostex_paths::powershell`; sessions pick it up on their next start.
//!
//! CDXC:ManagedTools 2026-10-08 DECISION:
//! User: "offer a one-click 'Install PowerShell 7' (via winget) in Settings › Terminal when only 5.1 is present. Sessions keep using 5.1 until 7 is installed, so nothing breaks." The install is `winget install --id Microsoft.PowerShell --source winget` (plus the flags an unattended run needs); when winget is missing the row shows the official download page instead. After it, new sessions use 7 and running sessions keep their shell.

use super::{jobs::Log, tools::Status};

#[cfg(windows)]
const DOWNLOAD_PAGE: &str =
    "https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows";

#[cfg(not(windows))]
pub(crate) fn status() -> Status {
    Status::unsupported("PowerShell 7 is installed by Ghostex only on Windows.".into())
}

#[cfg(not(windows))]
pub(crate) fn install(_log: &Log) -> Result<(), String> {
    Err("PowerShell 7 is installed by Ghostex only on Windows.".into())
}

#[cfg(windows)]
fn environment() -> ghostex_paths::powershell::Environment {
    ghostex_paths::powershell::Environment::from_process()
}

#[cfg(windows)]
fn winget() -> Option<std::path::PathBuf> {
    ghostex_paths::powershell::find_app_execution_alias(
        "winget.exe",
        &environment(),
        crate::platform::live_path::directories,
    )
}

#[cfg(windows)]
pub(crate) fn status() -> Status {
    const PLAN: &str = "Runs `winget install --id Microsoft.PowerShell --source winget`, the Windows Package Manager's official PowerShell package. Windows may ask you to allow the installer. New terminals use it afterwards; open ones keep their shell.";
    let mut status = Status::new(PLAN.into());
    if let Some(found) = ghostex_paths::powershell::find_powershell7(
        &environment(),
        crate::platform::live_path::directories,
    ) {
        // Installed anywhere counts (a preview build too: sessions use it), and nothing is offered.
        status.read_version(&found.executable, &["--version"]);
        status.source = Some(super::tools::Source::System);
        status.executable = Some(found.executable);
        return status;
    }
    status.operations = vec!["install"];
    if winget().is_none() {
        status.install_blocker = Some(
            "winget isn't available on this computer. Download PowerShell 7 from Microsoft instead."
                .into(),
        );
        status.download_url = Some(DOWNLOAD_PAGE.into());
    }
    status
}

#[cfg(windows)]
pub(crate) fn install(log: &Log) -> Result<(), String> {
    let winget = winget().ok_or_else(|| {
        format!(
            "winget isn't available on this computer. Download PowerShell 7 from {DOWNLOAD_PAGE}"
        )
    })?;
    log.line("Installing PowerShell 7 with winget. Windows may ask you to allow the installer.");
    super::run::run(
        &winget,
        &[
            "install",
            "--id",
            "Microsoft.PowerShell",
            "--source",
            "winget",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ],
        &[],
        &[],
        log,
        std::time::Duration::from_secs(20 * 60),
    )?;
    // The installer put it in Program Files and on the machine PATH, which this process has not
    // seen, so the lookup reads the registry PATH like a new terminal does.
    let found = ghostex_paths::powershell::find_powershell7(
        &environment(),
        crate::platform::live_path::directories,
    )
    .ok_or("winget finished, but PowerShell 7 was not found afterwards.")?;
    log.line(&format!(
        "PowerShell 7{} is installed at {}. New terminals use it; open ones keep their shell.",
        super::tools::version_of(&found.executable, &["--version"])
            .map(|version| format!(" {version}"))
            .unwrap_or_default(),
        found.executable.display()
    ));
    Ok(())
}
