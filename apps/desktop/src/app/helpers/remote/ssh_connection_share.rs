// One SSH login shared by every step of a remote gxserver connect.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::{
    process::{Child, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::app::helpers::*;
use crate::*;

/// ssh binds `<ControlPath>.<16 random characters>` before renaming it into place, and a Unix
/// socket path holds 104 bytes on macOS (108 on Linux) including its terminator.
const GPUI_REMOTE_SSH_CONTROL_PATH_MAX_BYTES: usize = 86;
/// How long a shared login outlives its last command when Ghostex never gets to close it
/// (a crash or force quit mid-connect).
const GPUI_REMOTE_SSH_CONTROL_PERSIST_SECONDS: u32 = 60;

/// CDXC:RemotePairing 2026-10-26 WHY:
/// A connect runs up to seven SSH commands before its tunnel (token read, platform probes, build-identity read, code-server platform), and each one was its own TCP connection and login: the user's Windows PC logged a burst of seven "Accepted publickey" lines, one to two seconds apart, on every Ghostex start. The connect now logs in once as a multiplexing master and runs those commands over it, so one connect costs two logins (this master and the tunnel, which keeps its own process; see `gpui_remote_ssh_tunnel_options`). A command whose socket is gone falls back to its own connection (`ControlMaster=no`), and a client that cannot multiplex (native Windows OpenSSH) keeps one connection per command.
/// SEE-ALSO: ssh_exec.rs (the runners that pass `gpui_remote_ssh_shared_connection_arguments`), connect.rs.
pub(crate) struct GpuiRemoteSshConnectionShare {
    control_path: Option<PathBuf>,
    target_host: String,
}

impl GpuiRemoteSshConnectionShare {
    fn unshared() -> Self {
        Self {
            control_path: None,
            target_host: String::new(),
        }
    }

    /// Logs in once for the connect. `Err` carries a login the machine refused or that could not
    /// reach it, which every command after it would hit too; any other trouble leaves the
    /// connect unshared, running each command over its own connection as before.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) fn open(config: &GpuiRemoteMachineConfig) -> Result<Self, GpuiRemoteProcessResult> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            ^ u128::from(std::process::id());
        let Some(control_path) = gpui_remote_ssh_control_path_for_connect(
            ghostex_state_root().as_path(),
            config.remote_machine_id.as_str(),
            unique,
        ) else {
            return Ok(Self::unshared());
        };
        let Some(directory) = control_path.parent() else {
            return Ok(Self::unshared());
        };
        if !gpui_remote_ssh_prepare_control_directory(directory) {
            return Ok(Self::unshared());
        }
        let (Ok(target_arguments), Ok(askpass)) = (
            gpui_remote_ssh_target_arguments(config),
            gpui_remote_ssh_askpass_script(config),
        ) else {
            // The first command reports the same refusal through the ordinary path.
            return Ok(Self::unshared());
        };
        let stderr_path = control_path.with_extension("log");
        let Ok(stderr_file) = fs::File::create(&stderr_path) else {
            return Ok(Self::unshared());
        };
        let mut command = gpui_remote_background_command(&gpui_remote_ssh_executable());
        command
            .args(gpui_remote_ssh_shared_connection_master_arguments(
                config.has_saved_password,
                control_path.as_path(),
                target_arguments,
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr_file));
        if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
            command.env_clear();
            command.envs(environment);
        }
        let Ok(mut child) = command.spawn() else {
            let _ = fs::remove_file(&stderr_path);
            return Ok(Self::unshared());
        };
        // With ControlPersist the foreground ssh exits as soon as the login is up and the
        // socket listens, leaving the master in the background.
        let exit_code = gpui_remote_ssh_wait_for_master_login(&mut child);
        let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
        let _ = fs::remove_file(&stderr_path);
        match exit_code {
            Some(0) if control_path.exists() => Ok(Self {
                control_path: Some(control_path),
                target_host: config.ssh_target_host(),
            }),
            Some(0) | None => Ok(Self::unshared()),
            Some(exit_code) => {
                let result = GpuiRemoteProcessResult {
                    exit_code,
                    stderr,
                    stdout: String::new(),
                };
                if gpui_remote_ssh_share_failure_is_definite(&result) {
                    Err(result)
                } else {
                    Ok(Self::unshared())
                }
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub(crate) fn open(_config: &GpuiRemoteMachineConfig) -> Result<Self, GpuiRemoteProcessResult> {
        Ok(Self::unshared())
    }

    pub(crate) fn is_shared(&self) -> bool {
        self.control_path.is_some()
    }

    /// The connect's config with its commands pointed at this login.
    pub(crate) fn share_into(
        &self,
        mut config: GpuiRemoteMachineConfig,
    ) -> GpuiRemoteMachineConfig {
        config.ssh_control_path = self.control_path.as_deref().map(gpui_path_string);
        config
    }
}

impl Drop for GpuiRemoteSshConnectionShare {
    fn drop(&mut self) {
        let Some(control_path) = self.control_path.take() else {
            return;
        };
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            // `-O exit` only talks to the local master over its socket; it opens no connection.
            let mut command = gpui_remote_background_command(&gpui_remote_ssh_executable());
            command
                .args([
                    "-o".to_string(),
                    "BatchMode=yes".to_string(),
                    "-o".to_string(),
                    gpui_remote_ssh_control_path_option(control_path.as_path()),
                    "-O".to_string(),
                    "exit".to_string(),
                    self.target_host.clone(),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Ok(mut child) = command.spawn() {
                let deadline = Instant::now() + Duration::from_secs(2);
                while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(20));
                }
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        let _ = fs::remove_file(control_path);
    }
}

/// Waits for the foreground ssh of a ControlPersist master; `None` when it neither logged in nor
/// failed in time (it is stopped then).
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn gpui_remote_ssh_wait_for_master_login(child: &mut Child) -> Option<i32> {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt as _;

    let deadline = Instant::now() + GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Some(status.code().or_else(|| status.signal()).unwrap_or(1));
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn gpui_remote_ssh_prepare_control_directory(directory: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    fs::create_dir_all(directory).is_ok()
        && fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).is_ok()
}

/// A unique socket per connect, so two connects to one machine never race for the same master.
/// `None` when ssh could not bind or parse the path, which leaves the connect unshared.
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
pub(crate) fn gpui_remote_ssh_control_path_for_connect(
    state_root: &Path,
    remote_machine_id: &str,
    unique: u128,
) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    remote_machine_id.hash(&mut hasher);
    unique.hash(&mut hasher);
    let path = state_root
        .join("remote-ssh")
        .join(format!("{:016x}", hasher.finish()));
    let text = path.to_str()?;
    (text.len() <= GPUI_REMOTE_SSH_CONTROL_PATH_MAX_BYTES
        && !text
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '"' | '\'')))
    .then_some(path)
}

/// `ControlPath=` for ssh, which expands `%` tokens in the value.
pub(crate) fn gpui_remote_ssh_control_path_option(control_path: &Path) -> String {
    format!(
        "ControlPath={}",
        gpui_path_string(control_path).replace('%', "%%")
    )
}

#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
pub(crate) fn gpui_remote_ssh_shared_connection_master_arguments(
    has_saved_password: bool,
    control_path: &Path,
    target_arguments: Vec<String>,
) -> Vec<String> {
    let mut arguments = vec!["-N".to_string()];
    arguments.extend(gpui_remote_ssh_client_options(has_saved_password));
    arguments.extend([
        "-o".to_string(),
        "ControlMaster=yes".to_string(),
        "-o".to_string(),
        gpui_remote_ssh_control_path_option(control_path),
        "-o".to_string(),
        format!("ControlPersist={GPUI_REMOTE_SSH_CONTROL_PERSIST_SECONDS}"),
    ]);
    arguments.extend(target_arguments);
    arguments
}

/// The options that send one command over the connect's shared login; empty outside a connect.
pub(crate) fn gpui_remote_ssh_shared_connection_arguments(
    config: &GpuiRemoteMachineConfig,
) -> Vec<String> {
    let Some(control_path) = config.ssh_control_path.as_deref() else {
        return Vec::new();
    };
    vec![
        "-o".to_string(),
        "ControlMaster=no".to_string(),
        "-o".to_string(),
        gpui_remote_ssh_control_path_option(Path::new(control_path)),
    ]
}

/// A master login failure every later command would repeat: the machine refused the login or
/// could not be reached. Anything else is retried command by command.
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
pub(crate) fn gpui_remote_ssh_share_failure_is_definite(result: &GpuiRemoteProcessResult) -> bool {
    matches!(
        gpui_remote_process_stderr_category(result),
        "savedPasswordUnavailable"
            | "passwordHelperFailed"
            | "authenticationFailed"
            | "timedOut"
            | "hostResolutionFailed"
            | "connectionRefused"
            | "hostKeyVerificationFailed"
            | "noRouteToHost"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_result(exit_code: i32, stderr: &str) -> GpuiRemoteProcessResult {
        GpuiRemoteProcessResult {
            exit_code,
            stderr: stderr.to_string(),
            stdout: String::new(),
        }
    }

    #[test]
    fn control_path_is_unique_per_connect_and_fits_a_socket() {
        let root = Path::new("/Users/someone/.local/state/ghostex");
        let first = gpui_remote_ssh_control_path_for_connect(root, "remote-a", 1).unwrap();
        let second = gpui_remote_ssh_control_path_for_connect(root, "remote-a", 2).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(root.join("remote-ssh").as_path()));
        assert!(first.to_str().unwrap().len() <= GPUI_REMOTE_SSH_CONTROL_PATH_MAX_BYTES);
    }

    #[test]
    fn control_path_is_refused_when_ssh_could_not_use_it() {
        let long_root = PathBuf::from(format!("/Users/{}/.local/state/ghostex", "x".repeat(60)));
        assert_eq!(
            gpui_remote_ssh_control_path_for_connect(long_root.as_path(), "remote-a", 1),
            None
        );
        assert_eq!(
            gpui_remote_ssh_control_path_for_connect(Path::new("/Users/a b/state"), "remote-a", 1),
            None
        );
    }

    #[test]
    fn control_path_option_escapes_ssh_percent_tokens() {
        assert_eq!(
            gpui_remote_ssh_control_path_option(Path::new("/Users/100%/remote-ssh/ab")),
            "ControlPath=/Users/100%%/remote-ssh/ab"
        );
    }

    #[test]
    fn master_logs_in_without_a_command_and_persists_briefly() {
        let arguments = gpui_remote_ssh_shared_connection_master_arguments(
            false,
            Path::new("/state/remote-ssh/ab"),
            vec!["-p".to_string(), "2222".to_string(), "me@host".to_string()],
        );
        assert_eq!(arguments.first().map(String::as_str), Some("-N"));
        assert_eq!(arguments.last().map(String::as_str), Some("me@host"));
        for option in [
            "ControlMaster=yes",
            "ControlPath=/state/remote-ssh/ab",
            "ControlPersist=60",
            "BatchMode=yes",
        ] {
            assert!(
                arguments.iter().any(|argument| argument == option),
                "{option}"
            );
        }
    }

    #[test]
    fn only_a_connect_config_shares_its_login() {
        let mut config = GpuiRemoteMachineConfig {
            remote_machine_id: "remote-a".to_string(),
            transport: GpuiRemoteMachineTransport::Ssh,
            easy_connect_address: None,
            easy_connect_ssh_port: 22,
            ssh_host: "host".to_string(),
            ssh_identity_file: None,
            ssh_port: None,
            ssh_user: None,
            has_saved_password: false,
            wsl_distribution: None,
            disabled: false,
            ssh_control_path: None,
        };
        assert!(gpui_remote_ssh_shared_connection_arguments(&config).is_empty());
        config.ssh_control_path = Some("/state/remote-ssh/ab".to_string());
        assert_eq!(
            gpui_remote_ssh_shared_connection_arguments(&config),
            vec![
                "-o".to_string(),
                "ControlMaster=no".to_string(),
                "-o".to_string(),
                "ControlPath=/state/remote-ssh/ab".to_string(),
            ]
        );
    }

    #[test]
    fn only_login_and_reachability_failures_end_the_connect() {
        assert!(gpui_remote_ssh_share_failure_is_definite(&process_result(
            255,
            "madda@host: Permission denied (publickey,password)."
        )));
        assert!(gpui_remote_ssh_share_failure_is_definite(&process_result(
            255,
            "ssh: connect to host 100.1.2.3 port 22: Operation timed out"
        )));
        assert!(!gpui_remote_ssh_share_failure_is_definite(&process_result(
            255,
            "unix_listener: path too long for Unix domain socket"
        )));
        assert!(!gpui_remote_ssh_share_failure_is_definite(&process_result(
            255, ""
        )));
    }
}
