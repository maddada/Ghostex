use std::{
    io,
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};

use portable_pty::{Child, ChildKiller};

use super::TerminalExit;

/// Owns the spawned PTY child, never the daemon reached through that child.
pub(crate) struct TerminalChild {
    pid: Option<u32>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    reaped: Arc<Mutex<bool>>,
    exit: Arc<OnceLock<TerminalExit>>,
    detached: bool,
    kill_requested: bool,
}

impl TerminalChild {
    pub(crate) fn spawn(
        mut child: Box<dyn Child + Send + Sync>,
        on_exit: impl FnOnce(TerminalExit) + Send + 'static,
    ) -> io::Result<Self> {
        let owner = Self {
            pid: child.process_id(),
            killer: child.clone_killer(),
            reaped: Arc::new(Mutex::new(false)),
            exit: Arc::new(OnceLock::new()),
            detached: false,
            kill_requested: false,
        };
        let pid = owner.pid;
        let reaped = Arc::clone(&owner.reaped);
        let exit = Arc::clone(&owner.exit);
        thread::Builder::new()
            .name("ghostex-terminal-child-wait".into())
            .spawn(move || {
                // Keep an exited Unix child unreaped until the same lock used
                // for viewer group termination is held. Its PID cannot then be
                // recycled between the identity check and the last signal.
                #[cfg(unix)]
                let ready = wait_until_exited(pid);
                #[cfg(not(unix))]
                let ready: io::Result<()> = Ok(());
                #[cfg(unix)]
                let mut reaped = reaped.lock().expect("terminal child lock poisoned");
                let result = ready.and_then(|()| child.wait());
                #[cfg(not(unix))]
                let mut reaped = reaped.lock().expect("terminal child lock poisoned");
                *reaped = true;
                let status = match result {
                    Ok(status) => TerminalExit {
                        code: Some(status.exit_code()),
                        success: status.success(),
                    },
                    Err(_) => TerminalExit {
                        code: None,
                        success: false,
                    },
                };
                let _ = exit.set(status);
                drop(reaped);
                on_exit(status);
            })?;
        Ok(owner)
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub(crate) fn exit_status(&self) -> Option<TerminalExit> {
        self.exit.get().copied()
    }

    pub(crate) fn is_detached(&self) -> bool {
        self.detached
    }

    pub(crate) fn kill(&mut self) -> io::Result<()> {
        let reaped = self.reaped.lock().expect("terminal child lock poisoned");
        if *reaped {
            return Ok(());
        }
        let result = self.killer.kill();
        self.kill_requested |= result.is_ok();
        result
    }

    /** CDXC:Terminal 2026-09-13 WHY:
     * Killing only the reconnect wrapper PID leaves its SSH/tee descendants holding the PTY open.
     * An explicitly proven daemon viewer owns a separate local PTY session; terminate that session's attach groups without sending input or a daemon shutdown command.
     * Reaping and group signals share a lock so the bounded escalation cannot target a recycled child PID.
     */
    pub(crate) fn detach_viewer(
        &mut self,
        foreground_group: Option<i32>,
        after_termination: impl FnOnce() + Send + 'static,
    ) -> io::Result<()> {
        if self.detached {
            return Ok(());
        }
        let pid = self.pid;
        let reaped = Arc::clone(&self.reaped);
        #[cfg(not(unix))]
        let mut killer = self.killer.clone_killer();
        thread::Builder::new()
            .name("ghostex-terminal-viewer-dispose".into())
            .spawn(move || {
                let reaped = reaped.lock().expect("terminal child lock poisoned");
                if !*reaped {
                    #[cfg(unix)]
                    if let Some(pid) = pid.and_then(|pid| i32::try_from(pid).ok()) {
                        terminate_viewer_groups(pid, foreground_group);
                    }
                    // Windows ChildKiller holds a process handle, not a reusable
                    // numeric PID. Closing the ConPTY owner releases its console.
                    #[cfg(not(unix))]
                    let _ = killer.kill();
                }
                drop(reaped);
                // portable-pty's Unix writer Drop injects newline/VEOF. Keep it
                // alive until attach clients have been terminated.
                after_termination();
            })?;
        self.detached = true;
        Ok(())
    }
}

impl Drop for TerminalChild {
    fn drop(&mut self) {
        if !self.detached && !self.kill_requested {
            let _ = self.kill();
        }
    }
}

#[cfg(unix)]
fn wait_until_exited(pid: Option<u32>) -> io::Result<()> {
    let pid = pid.ok_or_else(|| io::Error::other("terminal child has no process id"))?;
    loop {
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(unix)]
fn terminate_viewer_groups(pid: i32, foreground_group: Option<i32>) {
    if pid <= 1 {
        return;
    }
    // portable-pty calls setsid before exec, so this unreaped child owns the
    // session even when its exiting leader no longer answers getsid.
    let foreground_group = foreground_group
        .filter(|group| *group > 1 && *group != pid && unsafe { libc::getsid(*group) } == pid);
    // A distinct foreground job group can disappear independently of its
    // unreaped session leader. Terminate that validated group immediately,
    // leaving no delayed signal that could hit a reused foreground-group ID.
    if let Some(group) = foreground_group {
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    unsafe {
        libc::kill(-pid, libc::SIGHUP);
    }
    thread::sleep(Duration::from_millis(100));
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}
