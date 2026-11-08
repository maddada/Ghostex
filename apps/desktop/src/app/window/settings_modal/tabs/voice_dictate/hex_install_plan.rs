//! The command Settings › Dictation's Install Hex runs: Hex's own installer for this computer,
//! shown word for word in the confirm dialog and run by the app (`app/voice_agent/hex_install.rs`).
//! Pure data, so the Settings preview shows the same command the app would run.
//!
//! CDXC:Dictate 2026-11-08 DECISION: User, on Install Hex: "for hex here are the links on their
//! website for installing, we can just use those and ask the user that we're downloading hex for
//! you and he approves basically with touch id etc". The installers are Hex's own (macOS
//! Homebrew's `anomalyco/tap/hex` cask, `install-linux.sh`, `install-windows.ps1`), run by the
//! desktop app after the system presence prompt, never by gxserver and never as root.

/// Hex's website, where people can also download it themselves.
pub(crate) const HEX_SITE: &str = "https://anoma.ly/hex/";

/// Where Hex's installer is downloaded from, named in the confirm dialog.
pub(crate) const HEX_HOST: &str = "anoma.ly";

/// One installer: what to run and how to show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HexInstallPlan {
    /// The command line as the dialog and the failure note show it (what to run by hand).
    pub(crate) display: &'static str,
    pub(crate) program: &'static str,
    pub(crate) args: &'static [&'static str],
    /// The installer is a Homebrew cask, so the run needs `brew` (macOS).
    pub(crate) needs_homebrew: bool,
}

/// The installer for the computer the app runs on.
pub(crate) fn plan() -> HexInstallPlan {
    #[cfg(target_os = "macos")]
    {
        HexInstallPlan {
            display: "brew install --cask anomalyco/tap/hex",
            program: "brew",
            args: &["install", "--cask", "anomalyco/tap/hex"],
            needs_homebrew: true,
        }
    }
    #[cfg(target_os = "windows")]
    {
        HexInstallPlan {
            display: "powershell -ExecutionPolicy Bypass -Command \"iwr https://anoma.ly/hex/install-windows.ps1 -OutFile install-windows.ps1; ./install-windows.ps1\"",
            program: "powershell",
            args: &[
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "iwr https://anoma.ly/hex/install-windows.ps1 -OutFile install-windows.ps1; ./install-windows.ps1",
            ],
            needs_homebrew: false,
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        HexInstallPlan {
            display: "curl -fsSL https://anoma.ly/hex/install-linux.sh | sh",
            program: "sh",
            args: &[
                "-c",
                "curl -fsSL https://anoma.ly/hex/install-linux.sh | sh",
            ],
            needs_homebrew: false,
        }
    }
}

/// Where the system prompt's line says the install happens ("Install Hex from anoma.ly").
pub(crate) fn presence_reason() -> String {
    format!("download and install Hex from {HEX_HOST}")
}

/// The part of the installer's output worth showing when it failed: the last few non-empty lines.
pub(crate) fn last_lines(output: &str, count: usize) -> String {
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}
