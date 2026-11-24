//! Tie a Native host's agent child (Claude, Codex, generic ACP) to the host's own life across
//! macOS, Linux, and Windows, and ensure a dead host's child process group is gone before
//! self-heal resumes the same conversation.
//!
//! CDXC:AgentProviders 2026-11-24 WHY:
//! The host's own SIGTERM/SIGHUP/SIGINT handler stops its child, but a host that dies hard
//! (`SIGKILL`, panic abort, OOM, `TerminateProcess`) left the agent CLI running mid-turn: after
//! self-heal resumed that same conversation in a new host, the orphan kept running tools and
//! wrote into the thread so two agents shared one conversation.
//!
//! Defense in depth by platform:
//! - Unix (macOS + Linux): every agent child starts in its own process group (`setpgid(0, 0)`,
//!   so `pgid == child_pid`), and the host records `(host_pid, host_start, child_pid, child_pgid,
//!   child_start, session_ref)` under `runtime_dir/acp-children/`.
//! - Linux: `PR_SET_PDEATHSIG(SIGTERM)` signals the child immediately when the host dies.
//! - macOS: no `PR_SET_PDEATHSIG` exists, so the host spawns a lightweight kernel-idle watcher
//!   (`ghostex acp-child-watch`) that blocks in `kqueue(EVFILT_PROC, NOTE_EXIT)` on both
//!   `host_pid` and `child_pid`. On `host_pid` exit it verifies the target group by kernel start
//!   time and pgid (never by name), sends `SIGTERM`, and escalates to `SIGKILL` after a short
//!   grace period.
//! - Windows: each spawned child joins a per-child Job Object with
//!   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` held by `ChildLifetime`; when the host dies or drops
//!   the guard, the kernel terminates the child and its descendants.
//! - Self-heal barrier (macOS + Linux): before `/api/wakeSession` resumes a session whose host
//!   died, `ensure_dead_host_child_stopped` reads the recorded group, verifies `host_pid` is dead
//!   and any survivor provably belongs to that dead host's child group (by start time, pgid, and
//!   parent/session lineage, never by name), terminates it if still alive, and refuses to resume
//!   until the old child is gone.

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use serde::{Deserialize, Serialize};

use super::host::GxserverEndpoint;
use crate::paths::GxserverPaths;

/// Session identity used to persist the host/child process-group record for self-heal cleanup.
#[derive(Clone, Debug)]
pub(super) struct ChildRecordTarget {
    #[cfg_attr(not(unix), allow(dead_code))]
    pub(super) project_id: String,
    #[cfg_attr(not(unix), allow(dead_code))]
    pub(super) session_id: String,
    #[cfg_attr(not(unix), allow(dead_code))]
    pub(super) session_ref: String,
}

impl ChildRecordTarget {
    pub(super) fn from_endpoint(endpoint: &GxserverEndpoint) -> Self {
        Self {
            project_id: endpoint.project_id.clone(),
            session_id: endpoint.session_id.clone(),
            session_ref: endpoint.session_ref.clone(),
        }
    }
}

/// Configure `command` before spawning an agent child inside `acp-host`.
///
/// On Unix, places the child in its own process group (`pgid == child_pid`) so clean shutdown,
/// the macOS watcher, and the self-heal barrier can stop the child and its descendants without
/// touching the host or zmx shell. On Linux, also sets `PR_SET_PDEATHSIG(SIGTERM)` so the kernel
/// signals the child immediately if the host dies hard (call only on the thread that lives as
/// long as the host, i.e. each driver's main loop).
pub(super) fn end_with_host(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        #[cfg(target_os = "linux")]
        let host_pid = std::process::id() as libc::pid_t;
        // SAFETY: the closure only makes async-signal-safe syscalls (setpgid, prctl, getppid, _exit).
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_ulong) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    // The host died between fork and prctl: nothing will signal us, so stop now.
                    if libc::getppid() != host_pid {
                        libc::_exit(143);
                    }
                }
                Ok(())
            });
        }
    }
    command
}

/// Owns the per-child lifetime resources for one spawned agent subprocess:
/// - macOS: the `kqueue` sibling watcher (`ghostex acp-child-watch`) and the persisted pgid record.
/// - Linux: the persisted pgid record for the self-heal barrier.
/// - Windows: the `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` Job Object handle.
///
/// Dropping or calling `cleanup()` disarms the watcher/record/job cleanly during normal shutdown,
/// child restart, or switch-to-terminal. On `SIGKILL`/crash/abort, `Drop` does not run, so the
/// macOS watcher, Windows Job Object, and self-heal barrier take over.
pub(super) struct ChildLifetime {
    #[cfg(target_os = "macos")]
    watcher: Option<Child>,
    #[cfg(unix)]
    record: Option<ChildRecordGuard>,
    #[cfg(windows)]
    job: Option<std::os::windows::io::OwnedHandle>,
}

impl ChildLifetime {
    /// Attach platform lifetime protection to a freshly spawned agent `child`.
    pub(super) fn attach(child: &Child, target: Option<&ChildRecordTarget>) -> Self {
        #[cfg(unix)]
        {
            let child_pid = child.id();
            let host_pid = std::process::id();
            let host_start = unix_impl::process_start_id(host_pid);
            let child_start = unix_impl::process_start_id(child_pid);

            let record = match (target, host_start, child_start) {
                (Some(t), Some(hs), Some(cs)) => {
                    let paths = crate::paths::get_gxserver_paths(None);
                    unix_impl::write_child_record(
                        &paths,
                        t,
                        unix_impl::AcpChildRecord {
                            host_pid,
                            host_start: hs,
                            child_pid,
                            child_pgid: child_pid,
                            child_start: cs,
                            session_ref: Some(t.session_ref.clone()),
                        },
                    )
                }
                _ => None,
            };

            #[cfg(target_os = "macos")]
            let watcher = child_start.and_then(|cs| {
                macos_watcher::spawn_watcher(
                    host_pid,
                    child_pid,
                    cs,
                    target.map(|t| t.session_ref.as_str()),
                )
            });

            Self {
                #[cfg(target_os = "macos")]
                watcher,
                record,
            }
        }
        #[cfg(windows)]
        {
            let _ = target;
            Self {
                job: windows_impl::attach_kill_on_close_job(child),
            }
        }
        #[cfg(all(not(unix), not(windows)))]
        {
            let _ = (child, target);
            Self {}
        }
    }

    /// Clean up watcher, pgid record, and job handle after the child has been stopped.
    pub(super) fn cleanup(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(mut watcher) = self.watcher.take() {
            let _ = watcher.kill();
            let _ = watcher.wait();
        }
        #[cfg(unix)]
        if let Some(record) = self.record.take() {
            record.remove();
        }
        #[cfg(windows)]
        {
            self.job = None;
        }
    }
}

impl Drop for ChildLifetime {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Force-kill an agent child and (on Unix) its dedicated process group when graceful stdin-close
/// shutdown timed out.
pub(super) fn force_kill_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let cpid = child.id() as libc::pid_t;
        if cpid > 1 {
            // SAFETY: `child` was spawned with `setpgid(0, 0)` and is not yet reaped by us, so
            // `-cpid` is still its dedicated process group.
            unsafe {
                let _ = libc::kill(-cpid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Signal and reap the agent child's process group from `acp-host`'s termination signal thread
/// (`SIGTERM` -> wait up to 1s -> `SIGKILL`), and remove the session's pgid record if present.
#[cfg(unix)]
pub(super) fn terminate_child_on_host_signal(pid: u32, target: Option<&ChildRecordTarget>) {
    if pid > 1 {
        let cpid = pid as libc::pid_t;
        unsafe {
            if libc::kill(-cpid, libc::SIGTERM) != 0 {
                let _ = libc::kill(cpid, libc::SIGTERM);
            }
        }
        let mut reaped = false;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            let waited = unsafe { libc::waitpid(cpid, std::ptr::null_mut(), libc::WNOHANG) };
            if waited > 0
                || (waited < 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR))
            {
                reaped = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        unsafe {
            if !reaped {
                let _ = libc::kill(-cpid, libc::SIGKILL);
                let _ = libc::kill(cpid, libc::SIGKILL);
                let _ = libc::waitpid(cpid, std::ptr::null_mut(), libc::WNOHANG);
            } else {
                // Clean up any lingering descendants in the child's own process group.
                let _ = libc::kill(-cpid, libc::SIGKILL);
            }
        }
    }
    if let Some(t) = target {
        let paths = crate::paths::get_gxserver_paths(None);
        unix_impl::remove_record_if_host_matches(&paths, &t.project_id, &t.session_id, pid);
    }
}

/// Self-heal barrier: before starting a new `acp-host` on `(project_id, session_id)`, verify that
/// any previously recorded dead host's agent child process group has completely exited (terminating
/// verified members if still alive). Returns `Err` only if verified old processes remain alive
/// after `SIGKILL`, preventing two agents from writing into the same conversation.
pub(crate) fn ensure_dead_host_child_stopped(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
) -> Result<(), String> {
    #[cfg(unix)]
    {
        unix_impl::ensure_dead_host_child_stopped(paths, project_id, session_id)
    }
    #[cfg(not(unix))]
    {
        let _ = (paths, project_id, session_id);
        Ok(())
    }
}

/// Entry point for the internal macOS watcher subcommand `ghostex acp-child-watch`.
pub fn run_watcher_command(args: &[String]) -> i32 {
    #[cfg(target_os = "macos")]
    {
        macos_watcher::run(args)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = args;
        0
    }
}

#[cfg(unix)]
struct ChildRecordGuard {
    path: PathBuf,
    host_pid: u32,
    child_pid: u32,
}

#[cfg(unix)]
impl ChildRecordGuard {
    fn remove(self) {
        unix_impl::remove_record_if_matches(&self.path, self.host_pid, self.child_pid);
    }
}

#[cfg(unix)]
mod unix_impl {
    use super::*;

    const ACP_CHILDREN_DIR: &str = "acp-children";
    const SELF_HEAL_SIGTERM_GRACE: Duration = Duration::from_secs(2);
    const SELF_HEAL_SIGKILL_WAIT: Duration = Duration::from_secs(2);
    const POLL_STEP: Duration = Duration::from_millis(50);

    #[cfg(target_os = "macos")]
    pub(super) type ProcessStartId = (u64, u64);

    #[cfg(not(target_os = "macos"))]
    pub(super) type ProcessStartId = u64;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct AcpChildRecord {
        pub(super) host_pid: u32,
        pub(super) host_start: ProcessStartId,
        pub(super) child_pid: u32,
        pub(super) child_pgid: u32,
        pub(super) child_start: ProcessStartId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) session_ref: Option<String>,
    }

    #[derive(Clone, Debug)]
    pub(super) struct LiveProcInfo {
        pub(super) ppid: u32,
        pub(super) pgid: u32,
        pub(super) uid: u32,
        pub(super) start: ProcessStartId,
        pub(super) zombie: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum VerifiedTarget {
        /// None of the dead host's children remain alive (or the pid was recycled).
        Gone,
        /// `child_pid` is still alive with matching kernel start time and leads `pgid`.
        Group(u32),
        /// Specific kernel-verified PIDs belonging to the dead host's child tree.
        Pids(Vec<u32>),
    }

    fn sanitize_filename_segment(raw: &str) -> String {
        raw.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub(super) fn record_path(
        paths: &GxserverPaths,
        project_id: &str,
        session_id: &str,
    ) -> PathBuf {
        let p = sanitize_filename_segment(project_id);
        let s = sanitize_filename_segment(session_id);
        paths
            .runtime_dir
            .join(ACP_CHILDREN_DIR)
            .join(format!("{p}-{s}.json"))
    }

    pub(super) fn write_child_record(
        paths: &GxserverPaths,
        target: &ChildRecordTarget,
        record: AcpChildRecord,
    ) -> Option<ChildRecordGuard> {
        let path = record_path(paths, &target.project_id, &target.session_id);
        let dir = path.parent()?;
        fs::create_dir_all(dir).ok()?;
        let payload = serde_json::to_vec(&record).ok()?;
        let tmp = dir.join(format!(
            ".{}-{}-{}.tmp",
            sanitize_filename_segment(&target.project_id),
            sanitize_filename_segment(&target.session_id),
            std::process::id()
        ));
        fs::write(&tmp, &payload).ok()?;
        if fs::rename(&tmp, &path).is_err() {
            let _ = fs::remove_file(&tmp);
            return None;
        }
        Some(ChildRecordGuard {
            path,
            host_pid: record.host_pid,
            child_pid: record.child_pid,
        })
    }

    fn read_record(path: &Path) -> Option<AcpChildRecord> {
        let raw = fs::read(path).ok()?;
        serde_json::from_slice(&raw).ok()
    }

    pub(super) fn remove_record_if_matches(path: &Path, host_pid: u32, child_pid: u32) {
        if let Some(existing) = read_record(path) {
            if existing.host_pid == host_pid && existing.child_pid == child_pid {
                let _ = fs::remove_file(path);
            }
        }
    }

    pub(super) fn remove_record_if_host_matches(
        paths: &GxserverPaths,
        project_id: &str,
        session_id: &str,
        child_pid: u32,
    ) {
        let path = record_path(paths, project_id, session_id);
        remove_record_if_matches(&path, std::process::id(), child_pid);
    }

    /// Inspect the kernel process table to determine which processes still provably belong to
    /// `(child_pid, child_pgid, child_start)` after its host died. Never matches by process name.
    pub(super) fn resolve_verified_targets(
        host_pid: u32,
        child_pid: u32,
        child_pgid: u32,
        child_start: ProcessStartId,
        session_ref: Option<&str>,
        require_strong_orphan_proof: bool,
    ) -> VerifiedTarget {
        if child_pid <= 1 || child_pgid <= 1 {
            return VerifiedTarget::Gone;
        }
        let my_uid = unsafe { libc::geteuid() };
        let my_pid = std::process::id();

        if let Some(info) = proc_info(child_pid) {
            // If `child_pid` now has a different start time, the OS recycled the PID. POSIX
            // forbids reusing a PID while any member of PGID `child_pid` exists, so the old
            // child and its entire old process group are already gone.
            if info.start != child_start || info.uid != my_uid {
                return VerifiedTarget::Gone;
            }
            if !info.zombie {
                if info.pgid == child_pgid {
                    return VerifiedTarget::Group(child_pgid);
                }
                return VerifiedTarget::Pids(vec![child_pid]);
            }
            // `child_pid` is already a zombie with matching start time: fall through to check if
            // any live descendants remain in `child_pgid`.
        }

        // `child_pid` itself has exited (or is a zombie). Check for surviving descendants in
        // `child_pgid` owned by our UID that started at or after `child_start`.
        let mut pids = Vec::new();
        for pid in list_all_pids() {
            if pid <= 1 || pid == my_pid || pid == host_pid || pid == child_pid {
                continue;
            }
            let Some(info) = proc_info(pid) else {
                continue;
            };
            if info.zombie || info.uid != my_uid || info.pgid != child_pgid {
                continue;
            }
            if info.start < child_start {
                continue;
            }
            if require_strong_orphan_proof {
                let parent_matches = info.ppid == child_pid || info.ppid == host_pid;
                let env_matches = session_ref.is_some_and(|expected| {
                    read_process_env_var(pid, "GHOSTEX_GLOBAL_SESSION_REF").as_deref()
                        == Some(expected)
                });
                if !parent_matches && !env_matches {
                    continue;
                }
            } else if let Some(expected) = session_ref {
                // When we can read the candidate's environment, refuse if it belongs to a
                // different Ghostex session.
                if let Some(actual) = read_process_env_var(pid, "GHOSTEX_GLOBAL_SESSION_REF") {
                    if actual != expected {
                        continue;
                    }
                }
            }
            pids.push(pid);
        }
        if pids.is_empty() {
            VerifiedTarget::Gone
        } else {
            VerifiedTarget::Pids(pids)
        }
    }

    fn signal_target(target: &VerifiedTarget, signal: libc::c_int) {
        match target {
            VerifiedTarget::Gone => {}
            VerifiedTarget::Group(pgid) => {
                if let Ok(pgid) = libc::pid_t::try_from(*pgid) {
                    if pgid > 1 {
                        unsafe {
                            let _ = libc::kill(-pgid, signal);
                        }
                    }
                }
            }
            VerifiedTarget::Pids(pids) => {
                for pid in pids {
                    if let Ok(pid) = libc::pid_t::try_from(*pid) {
                        if pid > 1 {
                            unsafe {
                                let _ = libc::kill(pid, signal);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Terminate all kernel-verified processes belonging to a dead host's child group
    /// (`SIGTERM` -> wait -> `SIGKILL` -> wait) and confirm they have exited.
    pub(super) fn terminate_verified_group(
        host_pid: u32,
        child_pid: u32,
        child_pgid: u32,
        child_start: ProcessStartId,
        session_ref: Option<&str>,
        require_strong_orphan_proof: bool,
        term_grace: Duration,
        kill_wait: Duration,
    ) -> Result<(), String> {
        let initial = resolve_verified_targets(
            host_pid,
            child_pid,
            child_pgid,
            child_start,
            session_ref,
            require_strong_orphan_proof,
        );
        if initial == VerifiedTarget::Gone {
            return Ok(());
        }
        signal_target(&initial, libc::SIGTERM);

        let term_deadline = Instant::now() + term_grace;
        while Instant::now() < term_deadline {
            std::thread::sleep(POLL_STEP);
            let current = resolve_verified_targets(
                host_pid,
                child_pid,
                child_pgid,
                child_start,
                session_ref,
                require_strong_orphan_proof,
            );
            if current == VerifiedTarget::Gone {
                return Ok(());
            }
        }

        // Re-verify before escalating to SIGKILL so a recycled PID can never be killed.
        let survivors = resolve_verified_targets(
            host_pid,
            child_pid,
            child_pgid,
            child_start,
            session_ref,
            require_strong_orphan_proof,
        );
        if survivors == VerifiedTarget::Gone {
            return Ok(());
        }
        signal_target(&survivors, libc::SIGKILL);

        let kill_deadline = Instant::now() + kill_wait;
        while Instant::now() < kill_deadline {
            std::thread::sleep(POLL_STEP);
            let current = resolve_verified_targets(
                host_pid,
                child_pid,
                child_pgid,
                child_start,
                session_ref,
                require_strong_orphan_proof,
            );
            if current == VerifiedTarget::Gone {
                return Ok(());
            }
        }

        let remaining = resolve_verified_targets(
            host_pid,
            child_pid,
            child_pgid,
            child_start,
            session_ref,
            require_strong_orphan_proof,
        );
        if remaining == VerifiedTarget::Gone {
            Ok(())
        } else {
            Err(format!(
                "Dead Native host {host_pid}'s agent child group {child_pgid} did not exit after SIGKILL."
            ))
        }
    }

    pub(super) fn ensure_dead_host_child_stopped(
        paths: &GxserverPaths,
        project_id: &str,
        session_id: &str,
    ) -> Result<(), String> {
        let path = record_path(paths, project_id, session_id);
        let Some(record) = read_record(&path) else {
            return Ok(());
        };
        // Never touch the child if the recorded host is still alive.
        if let Some(host_info) = proc_info(record.host_pid) {
            if !host_info.zombie && host_info.start == record.host_start {
                return Ok(());
            }
        }
        terminate_verified_group(
            record.host_pid,
            record.child_pid,
            record.child_pgid,
            record.child_start,
            record.session_ref.as_deref(),
            true,
            SELF_HEAL_SIGTERM_GRACE,
            SELF_HEAL_SIGKILL_WAIT,
        )?;
        remove_record_if_matches(&path, record.host_pid, record.child_pid);
        Ok(())
    }

    pub(super) fn process_start_id(pid: u32) -> Option<ProcessStartId> {
        proc_info(pid).map(|info| info.start)
    }

    #[cfg(target_os = "macos")]
    pub(super) fn proc_info(pid: u32) -> Option<LiveProcInfo> {
        const MACOS_SZOMB: u32 = 5;
        let pid_i32 = i32::try_from(pid).ok().filter(|p| *p > 0)?;
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let read = unsafe {
            libc::proc_pidinfo(
                pid_i32,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        if read != size {
            return None;
        }
        Some(LiveProcInfo {
            ppid: info.pbi_ppid,
            pgid: info.pbi_pgid,
            uid: info.pbi_uid,
            start: (info.pbi_start_tvsec, info.pbi_start_tvusec),
            zombie: info.pbi_status == MACOS_SZOMB,
        })
    }

    #[cfg(target_os = "macos")]
    fn list_all_pids() -> Vec<u32> {
        let mut pids = vec![0 as libc::pid_t; 4096];
        loop {
            let size = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
            let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
            if count <= 0 {
                return Vec::new();
            }
            if count as usize >= pids.len() && pids.len() < 1 << 20 {
                pids.resize(pids.len() * 4, 0);
                continue;
            }
            pids.truncate(count as usize);
            return pids
                .into_iter()
                .filter_map(|p| u32::try_from(p).ok().filter(|p| *p > 0))
                .collect();
        }
    }

    #[cfg(target_os = "macos")]
    fn read_process_env_var(pid: u32, name: &str) -> Option<String> {
        crate::zmx::process_table_macos::process_environment_value(i64::from(pid), name)
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn proc_info(pid: u32) -> Option<LiveProcInfo> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let rest = stat.get(stat.rfind(')')? + 1..)?;
        let fields: Vec<&str> = rest.split_whitespace().collect();
        // After ")": state(0) ppid(1) pgrp(2) session(3) ... starttime(19)
        let state = fields.first()?.chars().next()?;
        let ppid = fields.get(1)?.parse::<u32>().ok()?;
        let pgid = fields.get(2)?.parse::<u32>().ok()?;
        let start = fields.get(19)?.parse::<u64>().ok()?;
        use std::os::unix::fs::MetadataExt as _;
        let uid = fs::metadata(format!("/proc/{pid}")).ok()?.uid();
        Some(LiveProcInfo {
            ppid,
            pgid,
            uid,
            start,
            zombie: matches!(state, 'Z' | 'X' | 'x'),
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn list_all_pids() -> Vec<u32> {
        let Ok(entries) = fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
            .filter(|pid| *pid > 0)
            .collect()
    }

    #[cfg(not(target_os = "macos"))]
    fn read_process_env_var(pid: u32, name: &str) -> Option<String> {
        let bytes = fs::read(format!("/proc/{pid}/environ")).ok()?;
        let prefix = format!("{name}=");
        for entry in bytes.split(|b| *b == 0) {
            if let Ok(text) = std::str::from_utf8(entry) {
                if let Some(value) = text.strip_prefix(&prefix) {
                    return Some(value.to_string());
                }
            }
        }
        None
    }
}

#[cfg(target_os = "macos")]
mod macos_watcher {
    use super::unix_impl::{
        proc_info, process_start_id, terminate_verified_group, ProcessStartId,
    };
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    const WATCHER_SIGTERM_GRACE: Duration = Duration::from_millis(1500);
    const WATCHER_SIGKILL_WAIT: Duration = Duration::from_millis(1500);

    pub(super) fn spawn_watcher(
        host_pid: u32,
        child_pid: u32,
        child_start: ProcessStartId,
        session_ref: Option<&str>,
    ) -> Option<Child> {
        let exe = std::env::current_exe().ok()?;
        let mut cmd = Command::new(exe);
        // `ghostex` is a symlink to `gxserver` in installed builds, and `main.rs::invoked_as_cli`
        // dispatches on argv[0]'s basename (`ghostex`). Setting `arg0("ghostex")` guarantees the
        // watcher enters the CLI subcommand even when `current_exe()` resolved the symlink.
        cmd.arg0("ghostex")
            .arg("acp-child-watch")
            .arg("--host-pid")
            .arg(host_pid.to_string())
            .arg("--child-pid")
            .arg(child_pid.to_string())
            .arg("--child-start")
            .arg(format!("{}.{}", child_start.0, child_start.1));
        if let Some(sref) = session_ref.filter(|s| !s.is_empty()) {
            cmd.arg("--session-ref").arg(sref);
        }
        cmd.env_remove(super::super::channel::ACP_CHANNEL_SECRET_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        cmd.spawn().ok()
    }

    fn parse_start(raw: &str) -> Option<ProcessStartId> {
        let (sec, usec) = raw.split_once('.')?;
        Some((sec.parse().ok()?, usec.parse().ok()?))
    }

    fn register_exit_event(kq: libc::c_int, pid: u32) -> Result<(), Option<i32>> {
        let change = libc::kevent {
            ident: pid as libc::uintptr_t,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let rc = unsafe {
            libc::kevent(
                kq,
                &change,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().raw_os_error())
        }
    }

    pub(super) fn run(args: &[String]) -> i32 {
        let mut host_pid: Option<u32> = None;
        let mut child_pid: Option<u32> = None;
        let mut child_start: Option<ProcessStartId> = None;
        let mut session_ref: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--host-pid" => {
                    i += 1;
                    host_pid = args.get(i).and_then(|v| v.parse().ok());
                }
                "--child-pid" => {
                    i += 1;
                    child_pid = args.get(i).and_then(|v| v.parse().ok());
                }
                "--child-start" => {
                    i += 1;
                    child_start = args.get(i).and_then(|v| parse_start(v));
                }
                "--session-ref" => {
                    i += 1;
                    session_ref = args.get(i).cloned();
                }
                _ => {}
            }
            i += 1;
        }

        let (Some(host_pid), Some(child_pid), Some(child_start)) =
            (host_pid, child_pid, child_start)
        else {
            return 2;
        };
        let self_pid = std::process::id();
        if host_pid <= 1 || child_pid <= 1 || host_pid == child_pid || child_pid == self_pid {
            return 2;
        }

        let terminate_now = || {
            let _ = terminate_verified_group(
                host_pid,
                child_pid,
                child_pid,
                child_start,
                session_ref.as_deref(),
                false,
                WATCHER_SIGTERM_GRACE,
                WATCHER_SIGKILL_WAIT,
            );
        };

        let kq = unsafe { libc::kqueue() };
        if kq < 0 {
            return 1;
        }

        // Register host exit first, then verify `getppid() == host_pid`. Because this watcher is
        // spawned directly by `host_pid`, checking `getppid()` after registration is race-free
        // and immune to PID recycling of `host_pid`.
        if let Err(err) = register_exit_event(kq, host_pid) {
            unsafe {
                libc::close(kq);
            }
            if err == Some(libc::ESRCH) || unsafe { libc::getppid() } != host_pid as libc::pid_t {
                terminate_now();
            }
            return 0;
        }
        if unsafe { libc::getppid() } != host_pid as libc::pid_t {
            unsafe {
                libc::close(kq);
            }
            terminate_now();
            return 0;
        }

        // Register child exit so the watcher terminates promptly if the child exits while the host
        // is still alive (e.g. normal shutdown or child restart).
        if let Err(err) = register_exit_event(kq, child_pid) {
            unsafe {
                libc::close(kq);
            }
            if unsafe { libc::getppid() } != host_pid as libc::pid_t {
                terminate_now();
            } else if err != Some(libc::ESRCH) {
                return 1;
            }
            return 0;
        }
        // If `child_pid` already exited (or was recycled) before registration, exit unless host
        // also died.
        let child_matches = proc_info(child_pid)
            .is_some_and(|info| !info.zombie && info.start == child_start);
        if !child_matches {
            unsafe {
                libc::close(kq);
            }
            if unsafe { libc::getppid() } != host_pid as libc::pid_t {
                terminate_now();
            }
            return 0;
        }

        // Block in kernel with 0% CPU until either `host_pid` or `child_pid` exits.
        let mut event: libc::kevent = unsafe { std::mem::zeroed() };
        loop {
            let n = unsafe {
                libc::kevent(
                    kq,
                    std::ptr::null(),
                    0,
                    &mut event,
                    1,
                    std::ptr::null(),
                )
            };
            if n < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }
            if n == 0 {
                continue;
            }
            let exited_pid = event.ident as u32;
            if exited_pid == host_pid {
                unsafe {
                    libc::close(kq);
                }
                terminate_now();
                return 0;
            }
            if exited_pid == child_pid {
                unsafe {
                    libc::close(kq);
                }
                // If the host died concurrently, still ensure no descendants survive in the group.
                if unsafe { libc::getppid() } != host_pid as libc::pid_t
                    || process_start_id(host_pid).is_none()
                {
                    terminate_now();
                }
                return 0;
            }
        }

        unsafe {
            libc::close(kq);
        }
        if unsafe { libc::getppid() } != host_pid as libc::pid_t {
            terminate_now();
        }
        0
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::Child;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// Put `child` into a per-child Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
    /// When `acp-host` exits, crashes, or is terminated (or drops `ChildLifetime`), Windows closes
    /// the uninherited handle and immediately terminates the agent child and all its descendants.
    pub(super) fn attach_kill_on_close_job(child: &Child) -> Option<OwnedHandle> {
        let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw_job.is_null() {
            return None;
        }
        // SAFETY: `raw_job` is a newly created kernel handle owned exclusively by this process.
        let job = unsafe { OwnedHandle::from_raw_handle(raw_job as _) };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let limited = unsafe {
            SetInformationJobObject(
                job.as_raw_handle() as _,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if limited == 0 {
            return None;
        }
        let assigned = unsafe {
            AssignProcessToJobObject(job.as_raw_handle() as _, child.as_raw_handle() as _)
        };
        if assigned == 0 {
            return None;
        }
        Some(job)
    }
}
