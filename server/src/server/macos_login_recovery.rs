//! A short-lived Aqua LaunchAgent checks whether a gxserver that stayed alive through logout is
//! still in the security session of the current desktop login.

use anyhow::{anyhow, Context, Result};
use std::{
    env, fs,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{
    auth::read_gxserver_auth_token,
    constants::GXSERVER_VERSION,
    http_client::{fetch_server_health, request_server_stop},
    paths::get_gxserver_paths,
    runtime::{is_process_running, read_current_build_identity},
};

/// One run of the agent checks this many times, this far apart, so a server still starting at
/// login or a probe refused while the login settles gets about a minute before the agent waits
/// for the next login or gxserver start.
const RECOVERY_ATTEMPTS: u32 = 12;
const RECOVERY_RETRY_DELAY: Duration = Duration::from_secs(5);

/// CDXC:ServerDaemon 2026-11-02 DECISION: User: gxserver should notice it stayed in the pre-login security session (Keychain reads fail after a logout and login) and "recover by itself: restarting itself in the current login session (its launchd job) ... Keep running sessions alive." A gxserver left in an ended login session cannot reach the new `gui/<uid>` domain ("141: Reentrancy avoided"), so the check runs from a LaunchAgent that launchd loads into every new desktop login. It compares the running server's audit session with its own, stops only the control plane when they differ (zmx sessions are their own launchd jobs, CDXC:Zmx 2026-08-07, and keep running), and starts gxserver again through its launchd job. Each gxserver start also kickstarts the agent once, which moves a server started outside the desktop login (over SSH, or while logged out) into it. The agent runs at login and on those kickstarts only, never on a timer, so copies left by isolated profiles cost nothing.
pub(super) fn install(port: u16) -> Result<()> {
    let home = env::var_os("HOME").context("HOME is unavailable")?;
    let agents_dir = PathBuf::from(home).join("Library/LaunchAgents");
    fs::create_dir_all(&agents_dir).context("create LaunchAgents directory")?;
    let binary = env::current_exe().context("find gxserver executable")?;
    let label = format!(
        "{}gxserver-login-recovery",
        ghostex_paths::launchd_label_prefix(port)
    );
    let plist_path = agents_dir.join(format!("{label}.plist"));
    let mut variables = vec![("GHOSTEX_GXSERVER_DEV_PORT".to_string(), port.to_string())];
    for key in [
        "GHOSTEX_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        if let Some(value) = env::var(key).ok().filter(|value| value.starts_with('/')) {
            variables.push((key.to_string(), value));
        }
    }
    if let Ok(path) = env::var("PATH") {
        variables.push(("PATH".to_string(), path));
    }
    // gxserver's own launch log, which the app quotes when a start fails, so a recovery shows there.
    let launch_log = get_gxserver_paths(None)
        .logs_dir
        .join("gxserver")
        .join("launch.log");
    if let Some(folder) = launch_log.parent() {
        fs::create_dir_all(folder).with_context(|| format!("create {}", folder.display()))?;
    }
    let environment = variables
        .into_iter()
        .map(|(key, value)| {
            format!(
                "<key>{}</key><string>{}</string>",
                xml_escape(&key),
                xml_escape(&value)
            )
        })
        .collect::<String>();
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>--recover-after-login</string></array><key>EnvironmentVariables</key><dict>{}</dict><key>LimitLoadToSessionType</key><string>Aqua</string><key>RunAtLoad</key><true/><key>KeepAlive</key><false/><key>StandardOutPath</key><string>{log}</string><key>StandardErrorPath</key><string>{log}</string></dict></plist>\n",
        xml_escape(&label),
        xml_escape(&binary.to_string_lossy()),
        environment,
        log = xml_escape(&launch_log.to_string_lossy()),
    );
    // Rewritten only when it changed, so routine server starts do not churn the LaunchAgents folder.
    if fs::read_to_string(&plist_path).ok().as_deref() != Some(plist.as_str()) {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&plist_path)
            .with_context(|| format!("open {}", plist_path.display()))?;
        file.write_all(plist.as_bytes())
            .with_context(|| format!("write {}", plist_path.display()))?;
    }

    let uid = unsafe { libc::getuid() };
    let domain = format!("gui/{uid}");
    let target = format!("{domain}/{label}");
    // Without a reachable desktop login launchd loads the plist at the next login (RunAtLoad).
    if !launchctl(&["print", &domain])?.status.success() {
        return Ok(());
    }
    // A loaded agent is kickstarted, never booted out: it may be the run that just started this
    // server. Without -k a running agent is left alone.
    if launchctl(&["print", &target])?.status.success() {
        let started = launchctl(&["kickstart", &target])?;
        if !started.status.success() {
            return Err(anyhow!(
                "launchctl kickstart {target} failed: {}",
                launchctl_error(&started)
            ));
        }
        return Ok(());
    }
    let loaded = launchctl(&["bootstrap", &domain, &plist_path.to_string_lossy()])?;
    if !loaded.status.success() && !launchctl(&["print", &target])?.status.success() {
        return Err(anyhow!(
            "launchctl bootstrap {target} failed: {}",
            launchctl_error(&loaded)
        ));
    }
    Ok(())
}

/// Invoked only by the Aqua LaunchAgent, before normal gxserver startup touches storage.
pub(crate) async fn recover() -> Result<()> {
    let manager = launchctl(&["managername"])?;
    if !manager.status.success() || String::from_utf8_lossy(&manager.stdout).trim() != "Aqua" {
        return Ok(());
    }
    let mut last_error = None;
    for attempt in 0..RECOVERY_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(RECOVERY_RETRY_DELAY).await;
        }
        match recover_once().await {
            Ok(true) => return Ok(()),
            Ok(false) => last_error = None,
            Err(error) => last_error = Some(error),
        }
    }
    last_error.map_or(Ok(()), Err)
}

/// `Ok(false)` while no gxserver answers on this profile's port yet.
async fn recover_once() -> Result<bool> {
    let paths = get_gxserver_paths(None);
    let auth = read_gxserver_auth_token(&paths)?;
    let token = auth.as_ref().map(|auth| auth.token.as_str());
    let Some(health) = fetch_server_health(token, 2_000)? else {
        return Ok(false);
    };
    let gui_asid = launchctl_asid(&format!("pid/{}", std::process::id()))?
        .context("current GUI login has no audit session")?;
    let server_asid = launchctl_asid(&format!("pid/{}", health.pid))?
        .context("running gxserver has no readable audit session")?;
    if !needs_recovery(gui_asid, server_asid) {
        return Ok(true);
    }

    // Recheck the owner before asking it to stop: a normal app restart may have replaced it while
    // the two launchctl probes ran. /api/control/stop drains the server and leaves zmx alone.
    if fetch_server_health(token, 2_000)?
        .as_ref()
        .map(|server| server.pid)
        != Some(health.pid)
    {
        return Err(anyhow!("gxserver changed while its login session was read"));
    }
    if !request_server_stop(token, 2_000)? {
        return Err(anyhow!(
            "the stale gxserver did not accept a control-plane stop"
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while is_process_running(health.pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if is_process_running(health.pid) {
        return Err(anyhow!("the stale gxserver did not stop within 20 seconds"));
    }
    let version = GXSERVER_VERSION.to_string();
    let build_identity = read_current_build_identity(&version)?;
    let started = crate::cli::start_gxserver_background(&build_identity, &version).await?;
    if !started.ok || started.state != "running" {
        return Err(anyhow!(
            "gxserver did not start in the new login session: {}",
            started.message
        ));
    }
    eprintln!(
        "gxserver pid {} ran outside this login's audit session ({server_asid}, login {gui_asid}); restarted it: {}",
        health.pid, started.message
    );
    Ok(true)
}

fn launchctl_asid(target: &str) -> Result<Option<u32>> {
    let output = launchctl(&["print", target])?;
    if !output.status.success() {
        return Err(anyhow!(
            "launchctl print {target} failed: {}",
            launchctl_error(&output)
        ));
    }
    Ok(security_context_asid(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn launchctl(arguments: &[&str]) -> Result<std::process::Output> {
    Command::new("/bin/launchctl")
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .context("run launchctl")
}

fn launchctl_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr.trim().to_string()
    }
}

fn security_context_asid(text: &str) -> Option<u32> {
    text.split_once("security context = {")?
        .1
        .lines()
        .take_while(|line| !line.trim_start().starts_with('}'))
        .find_map(|line| {
            line.trim()
                .strip_prefix("asid = ")?
                .trim()
                .parse::<u32>()
                .ok()
        })
}

fn needs_recovery(gui_asid: u32, server_asid: u32) -> bool {
    gui_asid != server_asid
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_different_login_session_needs_recovery() {
        assert!(!needs_recovery(193708, 193708));
        assert!(needs_recovery(193708, 100042));
        assert!(needs_recovery(193708, 193700));
    }

    #[test]
    fn launchctl_audit_session_is_read_from_security_context() {
        let gui = "gui/501 = {\n\ttype = login\n\tsecurity context = {\n\t\tuid = 501\n\t\tasid = 193708\n\t}\n}";
        let pid = "pid/123 = {\n\tsecurity context = {\n\t\tuid = 501\n\t\tasid = 100042\n\t}\n}";
        assert_eq!(security_context_asid(gui), Some(193708));
        assert_eq!(security_context_asid(pid), Some(100042));
        assert_eq!(
            security_context_asid("pid/123 = {\n\tservices = {\n}"),
            None
        );
    }
}
