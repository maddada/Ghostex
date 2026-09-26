// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the SSH askpass script, keychain
// password read, and macOS remote-process spawn/terminate helpers. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

use crate::app::helpers::*;
use crate::*;

pub(crate) const TEMP_REMOTE_LOCAL_READY_TITLE: &str = "TEMP_REMOTE_LOCAL_READY_20260814";
pub(crate) const TEMP_REMOTE_SSH_READY_TITLE: &str = "TEMP_REMOTE_SSH_READY_20260814";

pub(crate) fn gpui_remote_ssh_executable() -> String {
    #[cfg(windows)]
    {
        gpui_windows_system_executable("OpenSSH/ssh.exe")
    }
    #[cfg(not(windows))]
    {
        "/usr/bin/ssh".to_string()
    }
}

pub(crate) fn gpui_remote_tar_executable() -> String {
    #[cfg(windows)]
    {
        gpui_windows_system_executable("tar.exe")
    }
    #[cfg(not(windows))]
    {
        "/usr/bin/tar".to_string()
    }
}

pub(crate) fn gpui_remote_background_command(executable: &str) -> Command {
    gpui_background_command(executable)
}

pub(crate) fn gpui_remote_ssh_terminal_environment(
    askpass: Option<&GpuiRemoteAskpassScript>,
) -> Vec<(String, String)> {
    let mut environment: Vec<_> = gpui_remote_ssh_askpass_environment(askpass)
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "DISPLAY"
                    | "SSH_ASKPASS"
                    | "SSH_ASKPASS_REQUIRE"
                    | "GHOSTEX_REMOTE_SSH_ASKPASS_MACHINE"
            )
        })
        .collect();
    #[cfg(windows)]
    environment.push((
        "GHOSTEX_REMOTE_SSH_CLIENT".to_string(),
        "windows".to_string(),
    ));
    environment
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_read_remote_ssh_password_from_keychain(
    remote_machine_id: &str,
) -> Result<Vec<u8>, String> {
    const PASSWORD_CAPACITY: usize = 4_096;
    let remote_machine_id = std::ffi::CString::new(remote_machine_id)
        .map_err(|_| "Could not read the saved SSH password from Keychain.".to_string())?;
    let mut password = vec![0_u8; PASSWORD_CAPACITY];
    let mut password_length = 0_usize;
    let result = unsafe {
        GhostexGpuiCopyRemoteSshPassword(
            remote_machine_id.as_ptr(),
            password.as_mut_ptr(),
            password.len(),
            &mut password_length,
        )
    };
    if result == -1 {
        return Err("The saved SSH password is no longer available in Keychain.".to_string());
    }
    if result != 1 || password_length == 0 || password_length > password.len() {
        password.fill(0);
        return Err("Could not read the saved SSH password from Keychain.".to_string());
    }
    password.truncate(password_length);
    Ok(password)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn gpui_remote_ssh_askpass_script(
    config: &GpuiRemoteMachineConfig,
) -> Result<Option<GpuiRemoteAskpassScript>, String> {
    if !config.has_saved_password {
        return Ok(None);
    }
    #[cfg(target_os = "linux")]
    if !Command::new("/usr/bin/nc")
        .arg("-h")
        .output()
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout).contains("-U")
                || String::from_utf8_lossy(&output.stderr).contains("-U")
        })
    {
        return Err("SSH password authentication on Linux requires netcat-openbsd (nc with Unix socket support).".to_string());
    }
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let directory = env::temp_dir().join(format!("gxap-{}-{unique_id:x}", std::process::id()));
    let script = directory.join("a");
    let socket = directory.join("s");
    fs::create_dir_all(&directory)
        .map_err(|_| "Could not prepare SSH password helper.".to_string())?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|_| {
        let _ = fs::remove_dir_all(&directory);
        "Could not prepare SSH password helper.".to_string()
    })?;
    let contents = format!(
        concat!(
            "#!/bin/sh\n",
            "password=$(/usr/bin/nc -U {} </dev/null) || exit 1\n",
            "if [ -z \"$password\" ]; then\n",
            "  printf '%s\\n' 'Ghostex saved SSH password unavailable.' >&2\n",
            "  exit 1\n",
            "fi\n",
            "printf '%s\\n' \"$password\"\n",
            "unset password\n",
        ),
        gpui_shell_single_quote(gpui_path_string(socket.as_path()).as_str())
    );
    if fs::write(&script, contents).is_err()
        || fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).is_err()
    {
        let _ = fs::remove_dir_all(&directory);
        return Err("Could not prepare SSH password helper.".to_string());
    }

    let listener = match std::os::unix::net::UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(_) => {
            let _ = fs::remove_dir_all(&directory);
            return Err("Could not prepare SSH password helper.".to_string());
        }
    };
    if listener.set_nonblocking(true).is_err() {
        let _ = fs::remove_dir_all(&directory);
        return Err("Could not prepare SSH password helper.".to_string());
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let server_cancel = cancel.clone();
    let remote_machine_id = config.remote_machine_id.clone();
    let askpass_prepared_at = Instant::now();
    let password_server = thread::spawn(move || {
        while !server_cancel.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.remoteNewTerminal.askpassRequested",
                        serde_json::json!({
                            "durationSincePreparedMs": askpass_prepared_at.elapsed().as_millis() as u64,
                            "machineId": remote_machine_id.as_str(),
                        }),
                    );
                    let keychain_read_started = Instant::now();
                    let password_result =
                        gpui_read_remote_ssh_password_from_keychain(remote_machine_id.as_str());
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.remoteNewTerminal.keychainPasswordReadCompleted",
                        serde_json::json!({
                            "durationMs": keychain_read_started.elapsed().as_millis() as u64,
                            "machineId": remote_machine_id.as_str(),
                            "succeeded": password_result.is_ok(),
                        }),
                    );
                    if let Ok(mut password) = password_result {
                        let _ = stream.write_all(password.as_slice());
                        let _ = stream.write_all(b"\n");
                        password.fill(0);
                    }
                    // CDXC:RemoteMachines 2026-09-23 WHY:
                    // The retained terminal wrapper retries SSH after a disconnect. Keep this broker available until its owner drops it so every handshake can read the current saved password.
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    Ok(Some(GpuiRemoteAskpassScript {
        cancel,
        directory,
        password_server: Some(password_server),
        script,
    }))
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_remote_ssh_askpass_environment(
    askpass: Option<&GpuiRemoteAskpassScript>,
) -> Option<HashMap<String, String>> {
    let askpass = askpass?;
    let mut environment: HashMap<String, String> = env::vars().collect();
    environment
        .entry("DISPLAY".to_string())
        .or_insert_with(|| "localhost:0".to_string());
    environment.insert(
        "SSH_ASKPASS".to_string(),
        gpui_path_string(askpass.script.as_path()),
    );
    environment.insert("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string());
    #[cfg(windows)]
    environment.insert(
        "GHOSTEX_REMOTE_SSH_ASKPASS_MACHINE".to_string(),
        askpass.remote_machine_id.clone(),
    );
    Some(environment)
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_process(
    executable: &str,
    arguments: &[String],
    environment: Option<HashMap<String, String>>,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    if !gpui_remote_process_launch_input_is_safe(executable, arguments, environment.as_ref()) {
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Remote gxserver process launch input was invalid.".to_string(),
            stdout: String::new(),
        };
    }
    let mut command = gpui_remote_background_command(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(environment) = environment {
        command.env_clear();
        command.envs(environment);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return GpuiRemoteProcessResult {
                exit_code: 127,
                stderr: error.to_string(),
                stdout: String::new(),
            };
        }
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                #[cfg(unix)]
                use std::os::unix::process::ExitStatusExt as _;

                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return GpuiRemoteProcessResult {
                    exit_code: {
                        #[cfg(unix)]
                        {
                            status.code().or_else(|| status.signal()).unwrap_or(1)
                        }
                        #[cfg(windows)]
                        {
                            status.code().unwrap_or(1)
                        }
                    },
                    stderr,
                    stdout,
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                gpui_terminate_remote_process(&mut child);
                return GpuiRemoteProcessResult {
                    exit_code: 124,
                    stderr: "Remote SSH command timed out.".to_string(),
                    stdout: String::new(),
                };
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_process_with_stdin_file(
    executable: &str,
    arguments: &[String],
    environment: Option<HashMap<String, String>>,
    stdin_path: &Path,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    if !gpui_remote_process_launch_input_is_safe(executable, arguments, environment.as_ref()) {
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Remote gxserver process launch input was invalid.".to_string(),
            stdout: String::new(),
        };
    }
    let input = match fs::File::open(stdin_path) {
        Ok(input) => input,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not read the gxserver package for upload.".to_string(),
                stdout: String::new(),
            };
        }
    };
    gpui_run_remote_process_with_stdin_reader(executable, arguments, environment, input, timeout)
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_run_remote_process_with_stdin_bytes(
    executable: &str,
    arguments: &[String],
    environment: Option<HashMap<String, String>>,
    input: Vec<u8>,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    gpui_run_remote_process_with_stdin_reader(
        executable,
        arguments,
        environment,
        std::io::Cursor::new(input),
        timeout,
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn gpui_run_remote_process_with_stdin_reader(
    executable: &str,
    arguments: &[String],
    environment: Option<HashMap<String, String>>,
    input: impl Read + Send + 'static,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    if !gpui_remote_process_launch_input_is_safe(executable, arguments, environment.as_ref()) {
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Remote gxserver process launch input was invalid.".to_string(),
            stdout: String::new(),
        };
    }
    let mut command = gpui_remote_background_command(executable);
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(environment) = environment {
        command.env_clear();
        command.envs(environment);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return GpuiRemoteProcessResult {
                exit_code: 127,
                stderr: error.to_string(),
                stdout: String::new(),
            };
        }
    };
    let Some(mut child_stdin) = child.stdin.take() else {
        gpui_terminate_remote_process(&mut child);
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Could not open the SSH input stream.".to_string(),
            stdout: String::new(),
        };
    };
    let writer = thread::spawn(move || {
        let mut input = input;
        let _ = std::io::copy(&mut input, &mut child_stdin);
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                #[cfg(unix)]
                use std::os::unix::process::ExitStatusExt as _;

                let _ = writer.join();
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return GpuiRemoteProcessResult {
                    exit_code: {
                        #[cfg(unix)]
                        {
                            status.code().or_else(|| status.signal()).unwrap_or(1)
                        }
                        #[cfg(windows)]
                        {
                            status.code().unwrap_or(1)
                        }
                    },
                    stderr,
                    stdout,
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                gpui_terminate_remote_process(&mut child);
                let _ = child.wait();
                let _ = writer.join();
                return GpuiRemoteProcessResult {
                    exit_code: 124,
                    stderr: "Remote SSH input command timed out.".to_string(),
                    stdout: String::new(),
                };
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_remote_process_launch_input_is_safe(
    executable: &str,
    arguments: &[String],
    environment: Option<&HashMap<String, String>>,
) -> bool {
    if executable.contains('\0') || arguments.iter().any(|argument| argument.contains('\0')) {
        return false;
    }
    if let Some(environment) = environment {
        for (key, value) in environment {
            if key.contains('\0') || value.contains('\0') {
                return false;
            }
        }
    }
    true
}

#[cfg(unix)]
pub(crate) fn gpui_terminate_remote_process(child: &mut Child) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGTERM: i32 = 15;
    unsafe {
        let _ = kill(child.id() as i32, SIGTERM);
    }
}
