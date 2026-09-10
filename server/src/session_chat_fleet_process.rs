//! CDXC:SessionStatus 2026-09-10 WHY:
//! A reboot or CLI restart can leave an unfinished child transcript behind without a stop event. Only work from the currently owned agent process can be live; hook timestamps alone miss runs started while gxserver was unavailable.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(crate) fn current_process(session: &Value, agent: &str) -> anyhow::Result<(i64, i64)> {
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
    if let Some(id) = identity.agent_session_id.as_deref() {
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
        return Ok((pid, start));
    }
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
    let start =
        chrono::NaiveDateTime::parse_from_str(result.stdout.trim(), "%a %b %e %H:%M:%S %Y")?
            .and_utc()
            .timestamp_millis();
    if let Ok(mut cache) = starts.lock() {
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), start));
    }
    Ok((pid, start))
}
