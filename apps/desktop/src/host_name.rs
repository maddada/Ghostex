//! This computer's own name, the one it goes by everywhere in Ghostex: its computer tab in the
//! sidebar, the chat's where pill and Session details, menus, Settings and messages. Its own file so
//! the app, the web build and the modal preview binary can all include it.
//!
//! CDXC:RemoteMachines 2026-11-19 DECISION:
//! User: "use the computer's own chosen display name wherever one exists ..., falling back to the
//! host name". It reads what gxserver hands phones and other computers
//! (`read_computer_name` in server/src/remote_access/identity.rs): the name given in the system
//! first (macOS `ComputerName`, Linux `/etc/machine-info`'s `PRETTY_HOSTNAME`, Windows'
//! `COMPUTERNAME`), else the host name without its domain. The web build has no system to ask: the
//! page is told the name of the computer serving it by its handshake (`set_served_computer_name`),
//! and says nothing until then, so the chat names it from gxserver's own answer.

/// The name of the computer the app runs on (or, in the web build, the one serving the page).
pub(crate) fn local_computer_name() -> String {
    #[cfg(ghostex_web)]
    {
        SERVED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
    #[cfg(not(ghostex_web))]
    {
        static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        NAME.get_or_init(read_name).clone()
    }
}

#[cfg(ghostex_web)]
static SERVED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// The web page's computer, from its handshake (`machineLabel`).
#[cfg(ghostex_web)]
pub(crate) fn set_served_computer_name(name: &str) {
    *SERVED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = name.trim().to_string();
}

/// A host name without its domain: "studio.local" and "studio.example.com" are "studio".
#[cfg_attr(ghostex_web, allow(dead_code))]
fn short_host_name(host: &str) -> String {
    let host = host.trim();
    host.split('.')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(host)
        .to_string()
}

#[cfg(not(ghostex_web))]
fn read_name() -> String {
    chosen_name()
        .or_else(|| host_name().map(|host| short_host_name(&host)))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Local".to_string())
}

/// The name given in the system, when there is one.
#[cfg(not(ghostex_web))]
fn chosen_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/usr/sbin/scutil")
            .args(["--get", "ComputerName"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        let name = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        (!name.is_empty()).then_some(name)
    }
    #[cfg(target_os = "linux")]
    {
        let info = std::fs::read_to_string("/etc/machine-info").ok()?;
        info.lines().find_map(|line| {
            let value = line.trim().strip_prefix("PRETTY_HOSTNAME=")?;
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            (!value.is_empty()).then(|| value.to_string())
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// The host name (Windows: `COMPUTERNAME`).
#[cfg(not(ghostex_web))]
fn host_name() -> Option<String> {
    #[cfg(unix)]
    {
        let mut name = [0u8; 256];
        // SAFETY: the buffer is writable for its whole length and stays alive for the call.
        (unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } == 0).then(|| {
            let end = name
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name.len());
            String::from_utf8_lossy(&name[..end]).trim().to_string()
        })
    }
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME").ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
