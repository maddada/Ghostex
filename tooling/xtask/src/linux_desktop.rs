//! The graphical login a Linux start opens the rebuilt app in.
//!
//! CDXC:PlatformSupport 2026-11-14 WHY:
//! An agent's shell is the wrong place to start the app from: on Omarchy a gxserver that was started over SSH runs every session in that SSH login, so its shells have no WAYLAND_DISPLAY or DISPLAY, and every shell inside Ghostex carries that session's GHOSTEX_*, ZMX_* and agent variables (CLAUDECODE makes every Claude the app later starts refuse to run as "nested"). The start therefore takes the environment the desktop exported to the systemd user manager (uwsm, GNOME and KDE all import it there) and opens the app as its own transient scope with `systemd-run --user --scope`, the way the desktop's own launcher does. A scope, not a service: gxserver and its zmx sessions are started by the app and land in its unit, and a service's control group is killed when its main process exits, so quitting the app would end every session. Only without a usable user manager does it start the app from this shell, with those variables removed.
//! SEE-ALSO: tooling/xtask/src/start/install.rs (launch_linux_app), tooling/xtask/src/rebuild.rs (the preflight that refuses before building when there is no desktop to open the app in).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::stdout_if_ok;

/// Where the app will be opened and with which environment.
pub struct Desktop {
    /// The complete environment for the app.
    pub env: Vec<(String, String)>,
    /// Launch through `systemd-run --user --scope` (true) or straight from this process (false).
    pub via_systemd: bool,
    pub summary: String,
}

/// The desktop session the app should open in, or why there is none.
pub fn find() -> Result<Desktop, String> {
    let manager = stdout_if_ok(Command::new("systemctl").args(["--user", "show-environment"]))
        .map(|text| parse_show_environment(&text));
    if let Some(env) = &manager {
        if let Some(display) = live_display(env) {
            if stdout_if_ok(Command::new("systemd-run").arg("--version")).is_some() {
                return Ok(Desktop {
                    env: env.clone(),
                    via_systemd: true,
                    summary: format!(
                        "the desktop session the systemd user manager knows ({display}), opened with systemd-run --user --scope"
                    ),
                });
            }
        }
    }
    let own: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| !is_shell_session_variable(key))
        .collect();
    if let Some(display) = live_display(&own) {
        return Ok(Desktop {
            env: own,
            via_systemd: false,
            summary: format!("this shell's display ({display}), opened directly"),
        });
    }
    Err(match manager {
        Some(_) => "no graphical session to open Ghostex in: neither this shell nor the systemd user manager (`systemctl --user show-environment`) has a WAYLAND_DISPLAY or DISPLAY whose socket exists; log in to the desktop".to_string(),
        None => "no graphical session to open Ghostex in: this shell has no WAYLAND_DISPLAY or DISPLAY whose socket exists, and there is no systemd user manager to ask for the desktop's; run it from a terminal on the desktop".to_string(),
    })
}

/// Variables a shell inside a Ghostex session or an agent carries that must not reach the app.
fn is_shell_session_variable(key: &str) -> bool {
    (key.starts_with("GHOSTEX_") && key != "GHOSTEX_HOME")
        || key.starts_with("ZMX_")
        || key == "CLAUDECODE"
        || key.starts_with("CLAUDE_CODE_")
}

/// The display the environment points at when its socket exists, e.g. `WAYLAND_DISPLAY=wayland-1`.
fn live_display(env: &[(String, String)]) -> Option<String> {
    let get = |name: &str| {
        env.iter()
            .find(|(key, value)| key == name && !value.is_empty())
            .map(|(_, value)| value.as_str())
    };
    if let Some(wayland) = get("WAYLAND_DISPLAY") {
        let socket = if wayland.starts_with('/') {
            PathBuf::from(wayland)
        } else {
            Path::new(get("XDG_RUNTIME_DIR").unwrap_or_default()).join(wayland)
        };
        if socket.exists() {
            return Some(format!("WAYLAND_DISPLAY={wayland}"));
        }
    }
    let x11 = get("DISPLAY")?;
    let number = x11
        .strip_prefix(':')?
        .split('.')
        .next()
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))?;
    Path::new(&format!("/tmp/.X11-unix/X{number}"))
        .exists()
        .then(|| format!("DISPLAY={x11}"))
}

/// `systemctl --user show-environment` prints `KEY=value`, quoting values with `$'...'` and C escapes when they hold special characters.
fn parse_show_environment(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.is_empty() {
                return None;
            }
            let value = match value
                .strip_prefix("$'")
                .and_then(|rest| rest.strip_suffix('\''))
            {
                Some(quoted) => unescape_c(quoted),
                None => value.to_string(),
            };
            Some((key.to_string(), value))
        })
        .collect()
}

fn unescape_c(text: &str) -> String {
    let mut bytes = Vec::with_capacity(text.len());
    let mut chars = text.bytes().peekable();
    while let Some(byte) = chars.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let Some(escape) = chars.next() else {
            bytes.push(b'\\');
            break;
        };
        match escape {
            b'a' => bytes.push(0x07),
            b'b' => bytes.push(0x08),
            b'e' => bytes.push(0x1b),
            b'f' => bytes.push(0x0c),
            b'n' => bytes.push(b'\n'),
            b'r' => bytes.push(b'\r'),
            b't' => bytes.push(b'\t'),
            b'v' => bytes.push(0x0b),
            b'x' => {
                let mut value = 0u8;
                for _ in 0..2 {
                    match chars.peek().and_then(|b| (*b as char).to_digit(16)) {
                        Some(digit) => {
                            value = value * 16 + digit as u8;
                            chars.next();
                        }
                        None => break,
                    }
                }
                bytes.push(value);
            }
            b'0'..=b'7' => {
                let mut value = u32::from(escape - b'0');
                for _ in 0..2 {
                    match chars.peek().filter(|b| (b'0'..=b'7').contains(*b)) {
                        Some(digit) => {
                            value = value * 8 + u32::from(digit - b'0');
                            chars.next();
                        }
                        None => break,
                    }
                }
                bytes.push(value as u8);
            }
            other => bytes.push(other),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
