//! Tools Ghostex installs for the user so every install button works with one click on a fresh
//! computer: Node.js and npm, uv, Homebrew (macOS), Linux system tools, Beads, the GitHub and
//! GitLab CLIs, Google's Gemini API CLI (only when the user installs it), and (Windows) PowerShell 7. SEE-ALSO: docs/2026-09-29/one-click-installs/PLAN.md.
pub(crate) mod binaries;
pub(crate) mod desktop_control;
pub(crate) mod download;
pub(crate) mod endpoint;
pub(crate) mod flight;
pub(crate) mod hex;
#[cfg(test)]
mod hex_tests;
pub(crate) mod homebrew;
pub(crate) mod jobs;
pub(crate) mod mise;
pub(crate) mod node;
pub(crate) mod paths;
pub(crate) mod platform;
pub(crate) mod powershell;
#[cfg(all(test, unix))]
mod probe_tests;
pub(crate) mod run;
#[cfg(all(test, unix))]
mod run_tests;
pub(crate) mod system_tools;
pub(crate) mod tools;
pub(crate) mod uv;

pub(crate) use endpoint::{
    ensure_homebrew, ensure_mise, ensure_npm, ensure_system_tools, ensure_uv,
};
pub(crate) use jobs::{Log, INSTALL_LOCK};

/// The Gemini API CLI's one-time key setup; it stores the key itself (its own keychain entry).
pub(crate) const GEMINI_API_CONFIGURE_COMMAND: &str = "gemini-api configure";
