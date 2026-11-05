//! CDXC:SessionStatus 2026-09-10 WHY:
//! A reboot or CLI restart can leave an unfinished child transcript behind without a stop event. Only work from the currently owned agent process can be live; hook timestamps alone miss runs started while gxserver was unavailable.

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(crate) fn current_process(session: &Value, agent: &str) -> anyhow::Result<(i64, i64)> {
    let process = current_process_with_files(session, agent)?;
    Ok((process.pid, process.started_at))
}

pub(crate) struct CurrentProcess {
    pub pid: i64,
    pub started_at: i64,
    pub open_file_paths: Option<Arc<[PathBuf]>>,
}

pub(crate) fn current_process_with_files(
    session: &Value,
    agent: &str,
) -> anyhow::Result<CurrentProcess> {
    let name = session
        .get("zmxName")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Session has no process owner"))?;
    let identities = crate::zmx::read_cached_zmx_session_process_identities(
        &[name.to_string()],
        &crate::paths::get_gxserver_paths(None).home_dir,
    )
    .map_err(|e| anyhow::anyhow!("Cannot read subagent process owner: {e:?}"))?;
    let identity = identities
        .get(name)
        .filter(|identity| identity.agent_id.as_deref() == Some(agent))
        .ok_or_else(|| anyhow::anyhow!("Agent process owner is unavailable"))?;
    // CDXC:SessionIdentity 2026-09-21 WHY: Claude's process id comes from launch argv, which `/clear`, `/resume` and a self-fork leave stale while hooks report the live conversation; comparing it hid the Subagents card for every such session. Codex's id is read from its open rollout, so it stays a valid ownership check.
    if let Some(id) = identity
        .agent_session_id
        .as_deref()
        .filter(|_| agent != "claude")
    {
        anyhow::ensure!(
            session
                .pointer("/runtimeSettings/agentSessionId")
                .and_then(Value::as_str)
                == Some(id),
            "Agent process owns a different conversation"
        );
    }
    let pid = identity
        .process_id
        .filter(|pid| *pid > 0)
        .ok_or_else(|| anyhow::anyhow!("Agent process identity has no PID"))?;
    type Starts = HashMap<(String, i64), (Instant, i64)>;
    static STARTS: OnceLock<Mutex<Starts>> = OnceLock::new();
    let starts = STARTS.get_or_init(Mutex::default);
    let key = (name.to_string(), pid);
    if let Some(start) = starts.lock().ok().and_then(|cache| {
        cache
            .get(&key)
            .filter(|(at, _)| at.elapsed() < Duration::from_secs(5))
            .map(|(_, start)| *start)
    }) {
        return Ok(CurrentProcess {
            pid,
            started_at: start,
            open_file_paths: identity.open_file_paths.clone(),
        });
    }
    let start = process_started_at(pid)?;
    if let Ok(mut cache) = starts.lock() {
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), start));
    }
    Ok(CurrentProcess {
        pid,
        started_at: start,
        open_file_paths: identity.open_file_paths.clone(),
    })
}

/// CDXC:SessionStatus 2026-09-28 WHY:
/// Windows reads the agent's start time from the process itself. The `ps -o lstart=` probe went through PowerShell there and never succeeded, so the Subagents card and the Codex question run boundary were always unavailable and every check started another PowerShell.
#[cfg(windows)]
pub(crate) fn process_started_at(pid: i64) -> anyhow::Result<i64> {
    let created = crate::platform::process::process_creation_filetime(pid)
        .ok_or_else(|| anyhow::anyhow!("Agent process start is unavailable"))?;
    Ok(i64::try_from(
        crate::session_chat::windows_filetime_to_unix_ms(created),
    )?)
}

/// CDXC:SessionStatus 2026-10-23 WHY:
/// macOS reads the start time from the kernel's process record, the same `p_starttime` `ps -o lstart=` prints. The `ps` probe went through a login-free zsh each time, and the fleet pass asks for every running Claude and Codex session every five seconds, so it started one shell and one `ps` per session per pass. Whole seconds, as `lstart` printed them, keep every stored run boundary comparable.
#[cfg(target_os = "macos")]
pub(crate) fn process_started_at(pid: i64) -> anyhow::Result<i64> {
    let pid = i32::try_from(pid)?;
    // A plain C output buffer; only a complete reply is read.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };
    anyhow::ensure!(read == size, "Agent process start is unavailable");
    Ok(i64::try_from(info.pbi_start_tvsec)?.saturating_mul(1000))
}

/// CDXC:SessionStatus 2026-10-23 WHY:
/// Linux reads the start time from `/proc` the way procps computes `lstart` (boot time plus whole seconds of start ticks), instead of starting a shell and `ps` per session per fleet pass.
#[cfg(target_os = "linux")]
pub(crate) fn process_started_at(pid: i64) -> anyhow::Result<i64> {
    anyhow::ensure!(pid > 0, "Agent process start is unavailable");
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    // comm can contain spaces and parentheses; field 22 follows the final closing parenthesis.
    let ticks = stat
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(19))
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| anyhow::anyhow!("Agent process start is unavailable"))?;
    let boot_seconds = std::fs::read_to_string("/proc/stat")?
        .lines()
        .find_map(|line| line.strip_prefix("btime "))
        .and_then(|value| value.trim().parse::<u64>().ok())
        .ok_or_else(|| anyhow::anyhow!("Agent process start is unavailable"))?;
    let hertz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    anyhow::ensure!(hertz > 0, "Agent process start is unavailable");
    let seconds = boot_seconds.saturating_add(ticks / hertz as u64);
    Ok(i64::try_from(seconds)?.saturating_mul(1000))
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub(crate) fn process_started_at(pid: i64) -> anyhow::Result<i64> {
    let result = crate::zmx::run_zmx_probe_script(
        format!("LC_ALL=C TZ=UTC ps -p {pid} -o lstart="),
        crate::zmx::ZmxCommandOptions {
            timeout_ms: Some(2_000),
            stdout_limit_bytes: Some(1024),
            ..Default::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("Cannot read agent process start: {e}"))?;
    anyhow::ensure!(result.exit_code == 0, "Agent process start is unavailable");
    Ok(
        chrono::NaiveDateTime::parse_from_str(result.stdout.trim(), "%a %b %e %H:%M:%S %Y")?
            .and_utc()
            .timestamp_millis(),
    )
}
