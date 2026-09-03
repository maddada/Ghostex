use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/*
CDXC:RemotePairing 2026-09-03:
The identity that goes into a pairing code is what the phone needs to greet
and sign in to this computer: a display name and the login user. Both are
read live, never persisted, so a renamed computer or a different login user
shows up on the next status poll.
*/
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteAccessPlatform {
    Macos,
    Windows,
    Linux,
}

impl RemoteAccessPlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccessIdentity {
    pub computer_name: String,
    pub username: String,
    pub platform: RemoteAccessPlatform,
}

pub fn read_remote_access_identity() -> Result<RemoteAccessIdentity> {
    Ok(RemoteAccessIdentity {
        computer_name: read_computer_name()?,
        username: read_login_username()?,
        platform: RemoteAccessPlatform::current(),
    })
}

/// macOS keeps the user-facing name in `ComputerName` (what Sharing shows),
/// which can differ from the kernel hostname; every other OS shows `hostname`.
fn read_computer_name() -> Result<String> {
    if cfg!(target_os = "macos") {
        if let Some(name) = read_first_line("scutil", &["--get", "ComputerName"]) {
            return Ok(name);
        }
    }
    read_first_line("hostname", &[]).with_context(|| "read this computer's name")
}

fn read_login_username() -> Result<String> {
    let env_name = if cfg!(windows) { "USERNAME" } else { "USER" };
    if let Ok(value) = std::env::var(env_name) {
        let value = value.trim().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
    }
    if let Some(value) = read_first_line("whoami", &[]) {
        // Windows `whoami` prints `DOMAIN\user`; SSH wants only the user part.
        return Ok(value.rsplit('\\').next().unwrap_or(&value).to_string());
    }
    bail!("read the login user name");
}

pub(crate) fn read_first_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or_default().trim().to_string();
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}
