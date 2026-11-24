//! Tie a Native host's agent child (Claude, Codex, generic ACP) to the host's own life.

use std::process::Command;

/// Ask the kernel to send the agent child SIGTERM when its Native host dies.
///
/// CDXC:AgentProviders 2026-11-24 WHY: the host's own SIGTERM handler stops its child, but a host that dies hard (SIGKILL, panic abort, OOM) left the agent running its turn: on Linux a killed Claude host's `claude` kept running tools and wrote "woke G" into the conversation 20 seconds after self-heal had resumed that same conversation in a new host, so two agents worked one thread. Call this only on the thread that lives as long as the host (each driver's main loop), because the kernel sends the signal when the spawning thread exits. macOS has no equivalent call, so there it still relies on the host's SIGTERM handler.
pub(super) fn end_with_host(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let host_pid = std::process::id() as libc::pid_t;
        // SAFETY: the closure only makes async-signal-safe calls (prctl, getppid, _exit).
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_ulong) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // The host died between fork and prctl: nothing will signal us, so stop now.
                if libc::getppid() != host_pid {
                    libc::_exit(143);
                }
                Ok(())
            });
        }
    }
    command
}
