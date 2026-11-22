//! Whether the Grok Bot desktop app is on this computer, for step 1 of Settings > Grok Bot. The
//! bot runs Ghostex's commands here through that app, so the setup starts with it.
//!
//! CDXC:GrokBot 2026-11-19 WHY: on macOS the app is `Grok Bot.app` (bundle id
//! `com.anysphere.sand`) in /Applications or ~/Applications, and a running copy shows as a process
//! under that bundle. Elsewhere Ghostex doesn't know where it installs, so it answers `unknown` and
//! the page asks the user to say it's installed (the `confirmApp` mark in setup.json).

/// `running`, `found` (installed, not running), `missing`, or `unknown` (not a Mac).
pub(crate) fn desktop_app_state() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        let mut bundles = vec![std::path::PathBuf::from("/Applications/Grok Bot.app")];
        if let Some(home) = std::env::var_os("HOME") {
            bundles.push(std::path::PathBuf::from(home).join("Applications/Grok Bot.app"));
        }
        if !bundles.iter().any(|bundle| bundle.is_dir()) {
            return "missing";
        }
        let running = std::process::Command::new("/usr/bin/pgrep")
            .args(["-f", "Grok Bot.app/Contents/MacOS/"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if running {
            "running"
        } else {
            "found"
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "unknown"
    }
}

/// Opens the Grok Bot desktop app (Settings > Grok Bot's Open Grok Bot). macOS only: elsewhere
/// Ghostex doesn't know how it's installed.
pub(crate) fn open_desktop_app() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("/usr/bin/open")
            .args(["-b", "com.anysphere.sand"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|error| format!("Couldn't open Grok Bot: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err("Couldn't open Grok Bot. Is it installed?".to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Open the Grok Bot app yourself on this computer.".to_string())
    }
}
