// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the SSH askpass script, keychain
// password read, and macOS remote-process spawn/terminate helpers. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

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

#[cfg(target_os = "macos")]
pub(crate) fn gpui_read_remote_ssh_password_from_keychain(remote_machine_id: &str) -> Result<Vec<u8>, String> {
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

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_ssh_askpass_script(
    config: &GpuiRemoteMachineConfig,
) -> Result<Option<GpuiRemoteAskpassScript>, String> {
    if !config.has_saved_password {
        return Ok(None);
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
                    break;
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

#[cfg(target_os = "macos")]
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
    Some(environment)
}

#[cfg(target_os = "macos")]
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
    let mut command = Command::new(executable);
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
                    exit_code: status.code().or_else(|| status.signal()).unwrap_or(1),
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

#[cfg(target_os = "macos")]
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
    let mut command = Command::new(executable);
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
            stderr: "Could not open the SSH package upload stream.".to_string(),
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
                    exit_code: status.code().or_else(|| status.signal()).unwrap_or(1),
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
                    stderr: "Remote SSH package upload timed out.".to_string(),
                    stdout: String::new(),
                };
            }
        }
    }
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminate_remote_process(child: &mut Child) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGTERM: i32 = 15;
    unsafe {
        let _ = kill(child.id() as i32, SIGTERM);
    }
}

