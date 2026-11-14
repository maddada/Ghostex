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
    let environment: Vec<_> = gpui_remote_ssh_askpass_environment(askpass)
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
    let mut environment = environment;
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

/// The saved password reaches ssh's askpass helper through a named pipe in a private (0700)
/// folder: the helper reads it with `/bin/cat` and this app's broker thread answers each read
/// with the password from the system keyring.
///
/// CDXC:RemoteMachines 2026-11-14 WHY: The helper used to read a Unix socket with `nc -U`, which
/// Linux distributions don't install by default (Arch/Omarchy has no `nc` at all), so a computer
/// added through Easy Connect, which signs in with its saved SSH password, failed with "requires
/// netcat-openbsd" until the user installed it by hand. A pipe needs only `cat`, on macOS and on
/// every Linux, with the same access rule (only this user can open the folder) and nothing
/// written to disk. Don't bring back a helper that needs a program the system may lack.
#[cfg(any(target_os = "macos", target_os = "linux"))]
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
    let pipe = directory.join("p");
    let prepare_failed = |directory: &Path| {
        let _ = fs::remove_dir_all(directory);
        "Could not prepare SSH password helper.".to_string()
    };
    fs::create_dir_all(&directory)
        .map_err(|_| "Could not prepare SSH password helper.".to_string())?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|_| prepare_failed(&directory))?;
    let contents = format!(
        concat!(
            "#!/bin/sh\n",
            "password=$(/bin/cat {} </dev/null) || exit 1\n",
            "if [ -z \"$password\" ]; then\n",
            "  printf '%s\\n' 'Ghostex saved SSH password unavailable.' >&2\n",
            "  exit 1\n",
            "fi\n",
            "printf '%s\\n' \"$password\"\n",
            "unset password\n",
        ),
        gpui_shell_single_quote(gpui_path_string(pipe.as_path()).as_str())
    );
    if fs::write(&script, contents).is_err()
        || fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).is_err()
    {
        return Err(prepare_failed(&directory));
    }
    let Ok(pipe_path) = std::ffi::CString::new(gpui_path_string(pipe.as_path())) else {
        return Err(prepare_failed(&directory));
    };
    // SAFETY: `pipe_path` is a valid NUL-terminated path inside the private folder made above.
    if unsafe { libc::mkfifo(pipe_path.as_ptr(), 0o600) } != 0 {
        return Err(prepare_failed(&directory));
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let server_cancel = cancel.clone();
    let remote_machine_id = config.remote_machine_id.clone();
    let server_pipe = pipe.clone();
    let password_server = thread::spawn(move || {
        while !server_cancel.load(Ordering::Acquire) {
            // A non-blocking open for writing succeeds only while the helper has the pipe open
            // for reading (ENXIO otherwise), so the loop never blocks and stops when cancelled.
            match gpui_open_askpass_pipe_for_writing(server_pipe.as_path()) {
                Ok(mut stream) => {
                    if let Ok(mut password) =
                        gpui_read_remote_ssh_password_from_keychain(remote_machine_id.as_str())
                    {
                        let _ = stream.write_all(password.as_slice());
                        let _ = stream.write_all(b"\n");
                        password.fill(0);
                    }
                    // Closing the pipe ends the helper's read; with nothing written it reports the
                    // password as unavailable.
                    drop(stream);
                    // CDXC:RemoteMachines 2026-09-23 WHY:
                    // The retained terminal wrapper retries SSH after a disconnect. Keep this broker available until its owner drops it so every handshake can read the current saved password.
                }
                Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
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

/// Opens the askpass pipe for writing without waiting for a reader (see
/// `gpui_remote_ssh_askpass_script`).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn gpui_open_askpass_pipe_for_writing(pipe: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(pipe)
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
