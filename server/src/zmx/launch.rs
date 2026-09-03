use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{paths::get_gxserver_paths, platform::shell::command_shell};

use super::*;

pub(crate) fn run_zmx_interaction_command(
    script: String,
    options: ZmxCommandOptions,
) -> ZmxEndpointResult<ZmxCommandResult> {
    let allow_stdout_truncation = options.allow_stdout_truncation;
    let result =
        run_zsh_script(script, options).map_err(ZmxEndpointError::DependencyUnavailable)?;
    if result.exit_code != 0 && !(allow_stdout_truncation && result.stdout_truncated) {
        let message = if !result.stderr.is_empty() {
            result.stderr.clone()
        } else if !result.stdout.is_empty() {
            result.stdout.clone()
        } else {
            format!(
                "zmx session interaction command exited {}",
                result.exit_code
            )
        };
        return Err(ZmxEndpointError::DependencyUnavailable(message));
    }
    Ok(result)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn run_zmx_start_command(
    _session_name: &str,
    _zmx_executable_path: &str,
    script: String,
) -> ZmxEndpointResult<ZmxStartOutcome> {
    /*
    CDXC:Zmx 2026-09-01:
    This platform's start is a plain detached `zmx run`, so a zero exit means
    the command was accepted, NOT that the session is registered. Report no
    observation; the caller keeps its authoritative probe.
    */
    run_zmx_interaction_command(script, ZmxCommandOptions::default()).map(|result| {
        ZmxStartOutcome {
            observed_alive: false,
            result,
        }
    })
}

/*
CDXC:Zmx 2026-08-07:
The macOS app and gxserver each have launchd/Background Task Management
lifecycles that may be replaced during a development rebuild. A detached zmx
daemon otherwise inherits gxserver's resource coalition, so macOS later kills
the terminal session while cleaning up the old gxserver job. Give every zmx
session its own launchd job whose long-lived shell supervisor remains active
until the exact zmx session disappears. The job runs /bin/zsh rather than an
executable inside Ghostex.app so an app-bundle replacement cannot re-associate
the session with the retiring app coalition.

The generated launch script is private and deletes itself as soon as launchd
starts it because startup text can contain user-authored terminal input and
the inherited terminal environment can contain secrets. The plist contains
only hashed identity and runtime paths.
*/
#[cfg(target_os = "macos")]
pub(crate) fn run_zmx_start_command(
    session_name: &str,
    zmx_executable_path: &str,
    script: String,
) -> ZmxEndpointResult<ZmxStartOutcome> {
    let job = MacosZmxLaunchdJob::new(session_name)?;
    job.prepare(&script)?;

    let bootstrap = job.launchctl(&["bootstrap", &job.domain_target, &job.plist_path_string]);
    if !bootstrap.success {
        if macos_zmx_session_exists(session_name, zmx_executable_path) {
            job.remove_private_launch_script();
            return Ok(zmx_start_observed_success());
        }
        let _ = job.launchctl(&["bootout", &job.job_target]);
        let retry = job.launchctl(&["bootstrap", &job.domain_target, &job.plist_path_string]);
        if !retry.success {
            job.remove_private_launch_script();
            return Err(ZmxEndpointError::DependencyUnavailable(format!(
                "zmx launchd bootstrap failed: {}",
                retry.message()
            )));
        }
    }

    let kickstart = job.launchctl(&["kickstart", &job.job_target]);
    if !kickstart.success && !macos_zmx_session_exists(session_name, zmx_executable_path) {
        job.cleanup();
        return Err(ZmxEndpointError::DependencyUnavailable(format!(
            "zmx launchd kickstart failed: {}",
            kickstart.message()
        )));
    }

    /*
    CDXC:Zmx 2026-09-01:
    The zmx daemon needs roughly 100ms to register after `launchctl kickstart`,
    so the first existence check nearly always misses and a flat 100ms sleep
    then paid the whole interval before the check that would have succeeded
    much earlier. Back off in 10ms steps up to the old 100ms ceiling instead:
    a session that appears at ~100ms is now seen within ~10ms of appearing,
    while a genuinely slow start still settles onto the same coarse cadence and
    the overall deadline is unchanged.
    */
    let mut poll_backoff_ms: u64 = ZMX_START_POLL_BACKOFF_STEP_MS;
    let deadline = Instant::now() + Duration::from_millis(ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS);
    while Instant::now() < deadline {
        if macos_zmx_session_exists(session_name, zmx_executable_path) {
            return Ok(zmx_start_observed_success());
        }
        thread::sleep(Duration::from_millis(poll_backoff_ms));
        poll_backoff_ms =
            (poll_backoff_ms + ZMX_START_POLL_BACKOFF_STEP_MS).min(ZMX_START_POLL_BACKOFF_MAX_MS);
    }

    let launch_log = fs::read_to_string(&job.log_path)
        .ok()
        .map(|text| text.trim().chars().rev().take(4_096).collect::<String>())
        .map(|text| text.chars().rev().collect::<String>())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "the per-session launch job produced no error output".to_string());
    job.cleanup();
    Err(ZmxEndpointError::DependencyUnavailable(format!(
        "zmx session did not become ready: {launch_log}"
    )))
}

/// First readiness re-check delay, and the amount each further wait grows by.
#[cfg(target_os = "macos")]
const ZMX_START_POLL_BACKOFF_STEP_MS: u64 = 10;
/// The ceiling the readiness backoff settles on, matching the previous fixed
/// poll interval.
#[cfg(target_os = "macos")]
const ZMX_START_POLL_BACKOFF_MAX_MS: u64 = 100;

#[cfg(target_os = "macos")]
fn macos_zmx_session_exists(session_name: &str, zmx_executable_path: &str) -> bool {
    run_zmx_probe_script(
        build_zmx_exists_command(session_name, zmx_executable_path),
        ZmxCommandOptions {
            timeout_ms: Some(1_000),
            ..ZmxCommandOptions::default()
        },
    )
    .map(|result| result.exit_code == 0)
    .unwrap_or(false)
}

/// A successful start whose session this call SAW in `zmx list`, so the caller
/// may reuse that observation instead of probing again.
#[cfg(target_os = "macos")]
fn zmx_start_observed_success() -> ZmxStartOutcome {
    ZmxStartOutcome {
        observed_alive: true,
        result: ZmxCommandResult {
            exit_code: 0,
            stderr: String::new(),
            stdout: String::new(),
            stdout_truncated: false,
        },
    }
}

#[cfg(target_os = "macos")]
struct MacosLaunchctlResult {
    success: bool,
    stderr: String,
    stdout: String,
}

#[cfg(target_os = "macos")]
impl MacosLaunchctlResult {
    fn message(&self) -> String {
        if !self.stderr.trim().is_empty() {
            self.stderr.trim().to_string()
        } else if !self.stdout.trim().is_empty() {
            self.stdout.trim().to_string()
        } else {
            "launchctl exited without an error message".to_string()
        }
    }
}

#[cfg(target_os = "macos")]
struct MacosZmxLaunchdJob {
    domain_target: String,
    job_target: String,
    label: String,
    launch_script_path: PathBuf,
    log_path: PathBuf,
    plist_path: PathBuf,
    plist_path_string: String,
    socket_path: PathBuf,
}

#[cfg(target_os = "macos")]
impl MacosZmxLaunchdJob {
    fn new(session_name: &str) -> ZmxEndpointResult<Self> {
        let uid = unsafe { libc::getuid() };
        let key = macos_zmx_launchd_session_key(session_name);
        let label = format!("com.madda.ghostex.zmx.{key}");
        let runtime_dir = get_gxserver_paths(None).runtime_dir.join("zmx-launchd");
        fs::create_dir_all(&runtime_dir).map_err(|error| {
            ZmxEndpointError::DependencyUnavailable(format!(
                "create zmx launchd runtime directory failed: {error}"
            ))
        })?;
        let plist_path = runtime_dir.join(format!("{key}.plist"));
        let plist_path_string = plist_path.to_string_lossy().to_string();
        Ok(Self {
            domain_target: format!("gui/{uid}"),
            job_target: format!("gui/{uid}/{label}"),
            label,
            launch_script_path: runtime_dir.join(format!("{key}.sh")),
            log_path: runtime_dir.join(format!("{key}.log")),
            plist_path,
            plist_path_string,
            socket_path: macos_zmx_socket_path(session_name),
        })
    }

    fn prepare(&self, start_script: &str) -> ZmxEndpointResult<()> {
        let _ = fs::remove_file(&self.log_path);
        let environment_exports = macos_zmx_launchd_environment_exports();
        let supervisor_script = format!(
            r#"#!/bin/zsh
/bin/rm -f {}
{}
(
{}
)
zmx_start_status=$?
if [ "$zmx_start_status" -ne 0 ]; then
  exit "$zmx_start_status"
fi
zmx_socket={}
zmx_absent_checks=0
zmodload zsh/zselect
while [ "$zmx_absent_checks" -lt 3 ]; do
  if [ ! -S "$zmx_socket" ]; then
    zmx_absent_checks=$((zmx_absent_checks + 1))
  else
    zmx_absent_checks=0
  fi
  zselect -t 200
done
"#,
            shell_quote(&self.launch_script_path.to_string_lossy()),
            environment_exports,
            start_script,
            shell_quote(&self.socket_path.to_string_lossy()),
        );
        write_private_macos_launchd_file(&self.launch_script_path, &supervisor_script)?;

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/zsh</string>
    <string>{launch_script}</string>
  </array>
  <key>RunAtLoad</key>
  <false/>
  <key>KeepAlive</key>
  <false/>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>StandardOutPath</key>
  <string>{launch_log}</string>
  <key>StandardErrorPath</key>
  <string>{launch_log}</string>
</dict>
</plist>
"#,
            label = macos_launchd_xml_escape(&self.label),
            launch_script = macos_launchd_xml_escape(&self.launch_script_path.to_string_lossy()),
            launch_log = macos_launchd_xml_escape(&self.log_path.to_string_lossy()),
        );
        write_private_macos_launchd_file(&self.plist_path, &plist)
    }

    fn launchctl(&self, arguments: &[&str]) -> MacosLaunchctlResult {
        match Command::new("/bin/launchctl")
            .args(arguments)
            .stdin(Stdio::null())
            .output()
        {
            Ok(output) => MacosLaunchctlResult {
                success: output.status.success(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            },
            Err(error) => MacosLaunchctlResult {
                success: false,
                stderr: error.to_string(),
                stdout: String::new(),
            },
        }
    }

    fn remove_private_launch_script(&self) {
        let _ = fs::remove_file(&self.launch_script_path);
    }

    fn cleanup(&self) {
        let _ = self.launchctl(&["bootout", &self.job_target]);
        self.remove_private_launch_script();
        let _ = fs::remove_file(&self.plist_path);
        let _ = fs::remove_file(&self.log_path);
    }
}

/*
CDXC:Zmx 2026-09-03:
The supervisor script must not export gxserver's PATH into the session
(GitHub issue #118, nix-darwin). gxserver runs under launchd with the
synthesized tool PATH from the desktop app, while the supervisor's own
`/bin/zsh` has already run `/etc/zshenv` by the time these exports execute.
On nix-darwin that file sources `set-environment`, which sets the real PATH
and exports the guard `__NIX_DARWIN_SET_ENVIRONMENT_DONE=1`; re-exporting
gxserver's PATH afterwards replaced it, and the `-lic` / `-li` login shells
downstream saw the guard and never recomputed it. The user ended up with
Homebrew/volta/mise/asdf/nodenv directories they do not have and no
`claude`. The login shells own PATH exactly as they do under Terminal.app
(`/etc/zprofile` path_helper plus the user's profiles); every Ghostex
binary the scripts run (zmx, gxserver in the notify hook, the prompt-editor
wrapper) is referenced by absolute path, so nothing in a session needs the
daemon's PATH. Linux keeps forwarding PATH: there gxserver's PATH is
inherited, not synthesized, and `/etc/profile` recomputes it in the `-lc`
script shell.
*/
#[cfg(target_os = "macos")]
fn macos_zmx_launchd_environment_exports() -> String {
    let mut environment = build_gxserver_zmx_child_environment()
        .into_iter()
        .filter(|(key, _)| key != "PATH" && is_shell_environment_name(key))
        .collect::<Vec<_>>();
    environment.sort_by(|left, right| left.0.cmp(&right.0));
    environment
        .into_iter()
        .map(|(key, value)| format!("export {key}={}\n", shell_quote(&value)))
        .collect()
}

#[cfg(target_os = "macos")]
fn macos_zmx_socket_path(session_name: &str) -> PathBuf {
    zmx_session_socket_path(session_name)
}

#[cfg(target_os = "macos")]
fn is_shell_environment_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|char| char.is_ascii_alphanumeric() || char == '_')
}

#[cfg(target_os = "macos")]
fn macos_zmx_launchd_session_key(session_name: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in session_name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(target_os = "macos")]
fn macos_launchd_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(target_os = "macos")]
fn write_private_macos_launchd_file(path: &Path, contents: &str) -> ZmxEndpointResult<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| {
            ZmxEndpointError::DependencyUnavailable(format!(
                "write {} failed: {error}",
                path.display()
            ))
        })?;
    file.write_all(contents.as_bytes()).map_err(|error| {
        ZmxEndpointError::DependencyUnavailable(format!("write {} failed: {error}", path.display()))
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn cleanup_macos_zmx_launchd_job(session_name: &str) {
    if let Ok(job) = MacosZmxLaunchdJob::new(session_name) {
        job.cleanup();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZmxShellProfileMode {
    /// Sources the user's login profile. Required whenever the script hands the
    /// user their own shell or runs user-authored terminal content.
    Login,
    /// Skips the login profile. Only for probe/snapshot pipelines that run the
    /// bundled zmx binary, `ps`, and shell builtins.
    Profileless,
}

pub(crate) fn run_zsh_script(
    script: String,
    options: ZmxCommandOptions,
) -> Result<ZmxCommandResult, String> {
    run_zsh_script_blocking(&script, options, ZmxShellProfileMode::Login)
}

/*
CDXC:Zmx 2026-09-01:
The probe/snapshot pipelines (`zmx list`, `zmx kill`, `ps -axo`) never need the
user's login profile, but they run on every presentation poll. Give them a
profile-free spawn instead.
*/
pub(crate) fn run_zmx_probe_script(
    script: String,
    options: ZmxCommandOptions,
) -> Result<ZmxCommandResult, String> {
    run_zsh_script_blocking(&script, options, ZmxShellProfileMode::Profileless)
}

/// `run_zmx_interaction_command`'s error contract on a profile-free spawn, for
/// probe reads that only run the bundled zmx binary.
pub(crate) fn run_zmx_probe_command(
    script: String,
    options: ZmxCommandOptions,
) -> ZmxEndpointResult<ZmxCommandResult> {
    let allow_stdout_truncation = options.allow_stdout_truncation;
    let result =
        run_zmx_probe_script(script, options).map_err(ZmxEndpointError::DependencyUnavailable)?;
    if result.exit_code != 0 && !(allow_stdout_truncation && result.stdout_truncated) {
        let message = if !result.stderr.is_empty() {
            result.stderr.clone()
        } else if !result.stdout.is_empty() {
            result.stdout.clone()
        } else {
            format!(
                "zmx session interaction command exited {}",
                result.exit_code
            )
        };
        return Err(ZmxEndpointError::DependencyUnavailable(message));
    }
    Ok(result)
}

fn run_zsh_script_blocking(
    script: &str,
    options: ZmxCommandOptions,
    profile_mode: ZmxShellProfileMode,
) -> Result<ZmxCommandResult, String> {
    let shell = command_shell();
    let shell_args = match profile_mode {
        ZmxShellProfileMode::Login => shell.script_args(script),
        ZmxShellProfileMode::Profileless => shell.profileless_script_args(script),
    };
    let mut child = Command::new(&shell.executable)
        .args(shell_args)
        /*
        CDXC:ServerDaemon 2026-07-18:
        Command::envs does not remove inherited variables that are absent from
        the supplied map. Clear first so environment_keys_to_strip actually
        removes NO_COLOR and other host-process suppression/session values from
        interactive zmx terminals, then install the complete sanitized copy.
        */
        .env_clear()
        .envs(build_gxserver_zmx_child_environment())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    /*
    CDXC:Zmx 2026-08-24:
    `zmx send` reads the payload from this pipe, so a failed or short write
    means the agent gets a truncated prompt or nothing at all while zmx still
    exits 0. Discarding the error here made gxserver report those sends as
    delivered. Report the write failure as a command failure instead, and kill
    the child first so a half-fed `zmx send` cannot submit the prefix it did
    receive.
    */
    if let Some(mut stdin) = child.stdin.take() {
        let input = options.stdin.clone().unwrap_or_default();
        if let Err(error) = stdin.write_all(input.as_bytes()) {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("writing zmx command stdin failed: {error}"));
        }
    }
    let terminate = Arc::new(AtomicBool::new(false));
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "missing zmx stdout pipe".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "missing zmx stderr pipe".to_string())?;
    let stdout_limit = options
        .stdout_limit_bytes
        .unwrap_or(GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES);
    let stderr_limit = options
        .stderr_limit_bytes
        .unwrap_or(GXSERVER_ZMX_COMMAND_STDERR_LIMIT_BYTES);
    let stdout_terminate = terminate.clone();
    let stderr_terminate = terminate.clone();
    let stdout_thread = thread::spawn(move || read_capped(stdout, stdout_limit, stdout_terminate));
    let stderr_thread = thread::spawn(move || read_capped(stderr, stderr_limit, stderr_terminate));

    let timeout = Duration::from_millis(
        options
            .timeout_ms
            .unwrap_or(ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS),
    );
    let started = Instant::now();
    let mut timed_out = false;
    let mut terminate_started: Option<Instant> = None;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            let (stdout, stdout_truncated) = stdout_thread
                .join()
                .map_err(|_| "zmx stdout reader panicked".to_string())?;
            let (stderr, stderr_truncated) = stderr_thread
                .join()
                .map_err(|_| "zmx stderr reader panicked".to_string())?;
            let mut exit_code = status.code().unwrap_or(1);
            if timed_out {
                exit_code = 124;
            } else if stdout_truncated || stderr_truncated {
                exit_code = 125;
            }
            let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
            let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
            let mut stderr_lines = Vec::new();
            if !stderr_text.is_empty() {
                stderr_lines.push(stderr_text);
            }
            if timed_out {
                stderr_lines.push(format!(
                    "zmx lifecycle command timed out after {}ms",
                    timeout.as_millis()
                ));
            }
            if stdout_truncated {
                stderr_lines.push(format!("zmx command stdout exceeded {stdout_limit} bytes"));
            }
            if stderr_truncated {
                stderr_lines.push(format!("zmx command stderr exceeded {stderr_limit} bytes"));
            }
            return Ok(ZmxCommandResult {
                exit_code,
                stderr: stderr_lines.join("\n"),
                stdout: stdout_text,
                stdout_truncated,
            });
        }
        let should_terminate = terminate.load(Ordering::SeqCst) || started.elapsed() >= timeout;
        if should_terminate {
            timed_out = timed_out || started.elapsed() >= timeout;
            if terminate_started.is_none() {
                terminate_started = Some(Instant::now());
                send_sigterm(&mut child);
            } else if terminate_started
                .map(|instant| instant.elapsed() >= Duration::from_millis(1_000))
                .unwrap_or(false)
            {
                let _ = child.kill();
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_capped<R: Read>(
    mut reader: R,
    limit: usize,
    terminate: Arc<AtomicBool>,
) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        if read > remaining {
            if remaining > 0 {
                output.extend_from_slice(&buffer[..remaining]);
            }
            truncated = true;
            terminate.store(true, Ordering::SeqCst);
            break;
        }
        output.extend_from_slice(&buffer[..read]);
    }
    (output, truncated)
}

fn send_sigterm(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}
