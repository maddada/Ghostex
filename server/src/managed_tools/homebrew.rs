//! Homebrew on macOS: installed when an install the user chooses runs through it.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use super::{
    download::{self, Scratch},
    jobs::Log,
    platform::{self, Os},
    run,
    tools::{self, Source, Status, ToolId},
};

pub(crate) fn brew_executable(home: &Path) -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .or_else(|| tools::locate("brew", home).map(|(path, _)| path))
}

/// Homebrew's installer needs `sudo`, which only administrators have.
fn is_admin() -> bool {
    crate::platform::process::background_command("/usr/bin/id")
        .arg("-Gn")
        .stdin(Stdio::null())
        .output()
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .any(|group| group == "admin")
        })
}

pub(crate) fn can_install_homebrew() -> Result<(), String> {
    if platform::os() != Os::Mac {
        return Err("Homebrew is installed by Ghostex only on macOS.".into());
    }
    if !is_admin() {
        return Err("Installing Homebrew needs an administrator account on this Mac. Ask an administrator to install Homebrew from brew.sh.".into());
    }
    Ok(())
}

const HOMEBREW_PLAN: &str = "Runs Homebrew's official installer from brew.sh. macOS asks for your password once; Homebrew also installs Apple's Command Line Tools if they are missing, which can take several minutes.";

pub(crate) fn homebrew_status(home: &Path) -> Status {
    if platform::os() != Os::Mac {
        return Status::unsupported("Homebrew is managed by Ghostex only on macOS.".into());
    }
    let mut status = Status::new(HOMEBREW_PLAN.into());
    match brew_executable(home) {
        Some(path) => {
            status.read_version(&path, &["--version"]);
            status.executable = Some(path);
            status.source = Some(Source::System);
            status.operations = vec!["update"];
        }
        None => {
            status.operations = vec!["install"];
            status.needs_password = true;
            status.install_blocker = can_install_homebrew().err();
        }
    }
    status
}

pub(crate) fn latest_homebrew_version() -> Result<String, String> {
    download::latest_release_tag("https://github.com/Homebrew/brew/releases/latest")
}

/// CDXC:ManagedTools 2026-09-29 DECISION:
/// User (3B): on a Mac, an install that runs through Homebrew installs Homebrew first, with one password prompt.
/// WHY: Homebrew refuses to run as root, so `do shell script … with administrator privileges` cannot run its installer, and its `NONINTERACTIVE` mode uses `sudo -n`, which fails without a cached password. Its script honours `SUDO_ASKPASS`, so Ghostex asks once in a macOS dialog, checks the password with `sudo -v`, and hands it to every `sudo -A` the script runs through a helper in a private folder that is deleted when the job ends. Homebrew.pkg is not an alternative: it installs only on Apple silicon with macOS 15 or later.
pub(crate) fn install_homebrew(home: &Path, log: &Log) -> Result<PathBuf, String> {
    if let Some(brew) = brew_executable(home) {
        return Ok(brew);
    }
    can_install_homebrew()?;
    log.line("Installing Homebrew with its official installer.");
    let password = ask_password(log)?;
    let scratch = Scratch::new()?;
    private_dir(&scratch.0)?;
    let password_file = scratch.0.join("password");
    write_private(&password_file, &password, 0o600)?;
    let askpass = scratch.0.join("askpass.sh");
    write_private(
        &askpass,
        &format!("#!/bin/sh\ncat '{}'\n", password_file.display()),
        0o700,
    )?;
    let script = scratch.0.join("install.sh");
    std::fs::write(
        &script,
        download::get_text("https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh")?,
    )
    .map_err(|error| error.to_string())?;
    log.line("Running Homebrew's installer. This can take several minutes, longer when Apple's Command Line Tools are installed too.");
    run::run(
        "/bin/bash",
        &[script.to_string_lossy().as_ref()],
        &[
            ("NONINTERACTIVE", "1".as_ref()),
            ("SUDO_ASKPASS", askpass.as_os_str()),
        ],
        &[],
        log,
        Duration::from_secs(60 * 60),
    )?;
    drop(scratch);
    let brew =
        brew_executable(home).ok_or("Homebrew's installer finished, but brew was not found.")?;
    let shellenv = format!("eval \"$({} shellenv)\"", brew.display());
    match crate::agent_cli::path_setup::append_to_login_profile(
        home,
        "Added by Ghostex after installing Homebrew, as Homebrew's installer recommends.",
        &shellenv,
    ) {
        Ok(profile) => log.line(&format!("Added Homebrew to {}.", profile.display())),
        Err(shell) => log.line(&format!(
            "Add `{shellenv}` to your {shell} configuration so new terminals find Homebrew."
        )),
    }
    tools::forget_latest(ToolId::Homebrew);
    log.line("Homebrew is installed.");
    Ok(brew)
}

fn private_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    let _ = dir;
    Ok(())
}

fn write_private(path: &Path, contents: &str, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(mode)
            .open(path)
            .map_err(|error| error.to_string())?;
        file.write_all(contents.as_bytes())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        std::fs::write(path, contents).map_err(|error| error.to_string())
    }
}

/// Shows the macOS password dialog until `sudo` accepts the password (three tries).
fn ask_password(log: &Log) -> Result<String, String> {
    let mut message = "Ghostex needs your Mac password to install Homebrew.";
    for _ in 0..3 {
        let output = crate::platform::process::background_command("/usr/bin/osascript")
            .args([
                "-e",
                &format!(
                    "display dialog \"{message}\" default answer \"\" with hidden answer with title \"Install Homebrew\" with icon caution buttons {{\"Cancel\", \"Install\"}} default button \"Install\" cancel button \"Cancel\""
                ),
                "-e",
                "text returned of result",
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("Could not show the password prompt: {error}"))?;
        if !output.status.success() {
            return Err("Homebrew was not installed: the password prompt was cancelled.".into());
        }
        let password = String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\r', '\n'])
            .to_string();
        match check_password(&password) {
            Ok(()) => {
                log.line("Password accepted.");
                return Ok(password);
            }
            Err(PasswordError::NotAdmin) => {
                return Err(
                    "Installing Homebrew needs an administrator account on this Mac.".into(),
                )
            }
            Err(PasswordError::Wrong) => {
                message = "That password didn't work. Enter your Mac password to install Homebrew.";
            }
        }
    }
    Err("Homebrew was not installed: the password was not accepted.".into())
}

enum PasswordError {
    NotAdmin,
    Wrong,
}

fn check_password(password: &str) -> Result<(), PasswordError> {
    use std::io::Write;
    let _ = crate::platform::process::background_command("/usr/bin/sudo")
        .arg("-k")
        .stdin(Stdio::null())
        .output();
    let mut child = crate::platform::process::background_command("/usr/bin/sudo")
        .args(["-S", "-v", "-p", ""])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| PasswordError::Wrong)?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{password}\n").as_bytes());
    }
    let output = child.wait_with_output().map_err(|_| PasswordError::Wrong)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("not in the sudoers") || stderr.contains("may not run sudo") {
        Err(PasswordError::NotAdmin)
    } else {
        Err(PasswordError::Wrong)
    }
}

pub(crate) fn update_homebrew(home: &Path, log: &Log) -> Result<(), String> {
    let brew = brew_executable(home).ok_or("Homebrew is not installed.")?;
    log.line("Updating Homebrew (brew update).");
    run::run(
        &brew,
        &["update"],
        &[],
        &[],
        log,
        Duration::from_secs(20 * 60),
    )?;
    tools::forget_latest(ToolId::Homebrew);
    Ok(())
}
