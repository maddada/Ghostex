//! SpaceO's running processes, read from `ps`.

use std::{path::Path, time::Duration};

use crate::run::run_capturing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `spaceo daemon`, the session host (normally launchd's `com.spaceo.daemon`).
    Daemon,
    /// `spaceo mcp`, one stdio MCP server per agent session that registered SpaceO.
    Mcp,
    /// SpaceO Viewer's own app process.
    Viewer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Process {
    pub pid: i32,
    pub ppid: i32,
    pub kind: Kind,
    pub command: String,
}

impl Process {
    /// CDXC:ManagedTools 2026-11-16 WHY:
    /// An MCP server talks to its agent over stdin and stdout, so once the agent that started it exits it can never serve anyone again; macOS hands such an orphan to launchd (parent pid 1). That is the only SpaceO process Ghostex ends on its own: the daemon and every MCP server whose agent is alive are left alone.
    pub fn is_orphaned_mcp(&self) -> bool {
        self.kind == Kind::Mcp && self.ppid == 1
    }
}

/// What a `ps` command line is, when it is one of SpaceO's.
fn kind_of(command: &str) -> Option<Kind> {
    if command.contains("/SpaceO Viewer.app/Contents/MacOS/") {
        return Some(Kind::Viewer);
    }
    // The executable may sit in a folder with spaces (SpaceO Viewer's bundled helper).
    let (_, rest) = command.rsplit_once("/spaceo ")?;
    let mut words = rest.split_whitespace();
    match words.next()? {
        "mcp" => Some(Kind::Mcp),
        // `spaceo daemon [--socket P]` runs the host; `spaceo daemon stop|wait|...` are clients.
        "daemon" => match words.next() {
            None => Some(Kind::Daemon),
            Some(next) if next.starts_with("--") => Some(Kind::Daemon),
            Some(_) => None,
        },
        _ => None,
    }
}

/// Every SpaceO process running for any user this process can see.
pub fn list() -> Vec<Process> {
    let Ok(output) = run_capturing(
        Path::new("/bin/ps"),
        &["-axww", "-o", "pid=,ppid=,args="],
        Duration::from_secs(5),
        16 * 1024 * 1024,
    ) else {
        return Vec::new();
    };
    output
        .stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim_start().splitn(2, char::is_whitespace);
            let pid = fields.next()?.parse().ok()?;
            let rest = fields.next()?.trim_start();
            let mut fields = rest.splitn(2, char::is_whitespace);
            let ppid = fields.next()?.parse().ok()?;
            let command = fields.next()?.trim().to_string();
            let kind = kind_of(&command)?;
            Some(Process {
                pid,
                ppid,
                kind,
                command,
            })
        })
        .collect()
}

pub fn alive(pid: i32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

pub fn terminate(pid: i32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: sends SIGTERM to one process the caller identified as SpaceO's.
        unsafe { libc::kill(pid, libc::SIGTERM) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
