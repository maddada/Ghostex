use std::process::Stdio;

use anyhow::{Context, Result, bail};
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

/// The name this computer goes by everywhere in Ghostex: the sidebar, the chat, phones and other
/// computers, gxserver's own messages. The name the user gave it in the system first (macOS keeps
/// it in `ComputerName`, what Sharing shows; Linux in `/etc/machine-info`'s `PRETTY_HOSTNAME`),
/// else the host name without its domain ("studio.local" is "studio"). Unix reads the kernel value
/// directly so a minimal gxserver PATH does not make remote pairing fail.
///
/// CDXC:RemoteMachines 2026-11-19 DECISION:
/// User: "use the computer's own chosen display name wherever one exists ..., falling back to the
/// host name". One name per computer from this one reader (`this_computer_name` in
/// connected_computers/http.rs; the desktop's apps/desktop/src/host_name.rs reads it the same way).
fn read_computer_name() -> Result<String> {
    if cfg!(target_os = "macos") {
        if let Some(name) = read_first_line("scutil", &["--get", "ComputerName"]) {
            return Ok(name);
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(name) = read_pretty_hostname() {
            return Ok(name);
        }
    }
    #[cfg(unix)]
    if let Some(name) = read_unix_hostname() {
        return Ok(short_host_name(&name));
    }
    #[cfg(windows)]
    if let Some(name) = windows_host_name() {
        return Ok(short_host_name(&name));
    }
    read_first_line("hostname", &[])
        .map(|name| short_host_name(&name))
        .with_context(|| "read this computer's name")
}

/// Windows' host name as the user typed it (its DNS host name: their capitals and its full length,
/// which `COMPUTERNAME`'s 15 upper-case letters are not). gxserver and the app read it the same way
/// (server/src/remote_access/identity.rs, apps/desktop/src/host_name.rs).
#[cfg(windows)]
fn windows_host_name() -> Option<String> {
    use windows_sys::Win32::System::SystemInformation::{
        ComputerNamePhysicalDnsHostname, GetComputerNameExW,
    };
    let mut size: u32 = 0;
    // SAFETY: a null buffer with size 0 only asks for the needed size.
    unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0u16; size as usize];
    // SAFETY: the buffer holds `size` UTF-16 units and stays alive for the call.
    if unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            buffer.as_mut_ptr(),
            &mut size,
        )
    } == 0
    {
        return None;
    }
    let name = String::from_utf16_lossy(&buffer[..size as usize])
        .trim()
        .to_string();
    (!name.is_empty()).then_some(name)
}

/// A host name without its domain: "studio.local" and "studio.example.com" are "studio".
pub(crate) fn short_host_name(host: &str) -> String {
    let host = host.trim();
    host.split('.')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(host)
        .to_string()
}

/// Linux's pretty host name (`hostnamectl set-hostname --pretty`), kept in `/etc/machine-info`.
fn read_pretty_hostname() -> Option<String> {
    let info = std::fs::read_to_string("/etc/machine-info").ok()?;
    info.lines().find_map(|line| {
        let value = line.trim().strip_prefix("PRETTY_HOSTNAME=")?;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(unix)]
fn read_unix_hostname() -> Option<String> {
    let mut buffer = [0_u8; 256];
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if status != 0 {
        return None;
    }
    let length = buffer.iter().position(|byte| *byte == 0)?;
    let hostname = std::str::from_utf8(&buffer[..length]).ok()?.trim();
    (!hostname.is_empty()).then(|| hostname.to_string())
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
    let output = crate::platform::process::background_command(program)
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
    if line.is_empty() { None } else { Some(line) }
}
