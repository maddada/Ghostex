use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformShell {
    pub executable: String,
    kind: PlatformShellKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PlatformShellKind {
    Bash,
    Sh,
    Zsh,
}

/*
CDXC:PlatformSupport 2026-06-23-07:52:
server must run the same command, zmx, hook, and title-generation script contract on macOS and Ubuntu from one Rust codebase. Keep macOS pinned to /bin/zsh for exact existing behavior, while Linux executes the same POSIX script bodies through a deterministic installed shell instead of assuming zsh exists.
*/
pub fn command_shell() -> PlatformShell {
    #[cfg(target_os = "macos")]
    {
        PlatformShell::new("/bin/zsh")
    }

    #[cfg(not(target_os = "macos"))]
    {
        for candidate in command_shell_candidates() {
            if is_supported_shell(&candidate) && is_executable_file(Path::new(&candidate)) {
                return PlatformShell::new(candidate);
            }
        }
        PlatformShell::new("/bin/sh")
    }
}

pub fn command_shell_for_path(path: &str) -> PlatformShell {
    PlatformShell::new(path)
}

pub fn login_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(configured_shell) = env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        candidates.push(configured_shell);
    }
    candidates.extend(command_shell_candidates());
    dedupe(candidates)
}

/*
CDXC:ServerDaemon 2026-07-24:
Persistence-session interactive shells must be the user's own login shell, not
the pinned script shell: gxserver runs as a launchd agent whose environment
never saw the user's shell profiles, so pinning `/bin/zsh` gave bash/fish users
a shell that skipped their config (missing PATH entries, prompt tools like
oh-my-posh). Script bodies stay on `command_shell()` for POSIX compatibility;
only the final `exec` handed to the terminal user resolves through $SHELL and
the passwd entry.
*/
pub fn user_login_shell_exec_command() -> String {
    login_exec_command_for_shell_path(&user_login_shell_path())
}

pub(crate) fn user_login_shell_path() -> String {
    for candidate in user_login_shell_candidates() {
        if is_executable_file(Path::new(&candidate)) {
            return candidate;
        }
    }
    command_shell().executable
}

fn user_login_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(configured_shell) = env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        candidates.push(configured_shell);
    }
    if let Some(passwd_shell) = passwd_login_shell() {
        candidates.push(passwd_shell);
    }
    candidates.extend(command_shell_candidates());
    dedupe(candidates)
}

#[cfg(unix)]
fn passwd_login_shell() -> Option<String> {
    let shell = unsafe {
        let passwd = libc::getpwuid(libc::getuid());
        if passwd.is_null() {
            return None;
        }
        let raw_shell = (*passwd).pw_shell;
        if raw_shell.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr(raw_shell)
            .to_str()
            .ok()?
            .trim()
            .to_string()
    };
    (!shell.is_empty()).then_some(shell)
}

#[cfg(not(unix))]
fn passwd_login_shell() -> Option<String> {
    None
}

fn login_exec_command_for_shell_path(path: &str) -> String {
    match Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
    {
        "bash" | "zsh" => format!("exec {} -li", shell_quote(path)),
        "fish" | "nu" => format!("exec {} -l -i", shell_quote(path)),
        "sh" | "dash" | "ash" => format!("exec {} -i", shell_quote(path)),
        _ => format!("exec {}", shell_quote(path)),
    }
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

impl PlatformShell {
    fn new(path: impl Into<String>) -> Self {
        let executable = path.into();
        Self {
            kind: shell_kind_for_path(&executable),
            executable,
        }
    }

    pub fn script_args(&self, script: &str) -> Vec<String> {
        vec![self.command_flag(false).to_string(), script.to_string()]
    }

    pub fn interactive_script_args(&self, script: &str) -> Vec<String> {
        vec![self.command_flag(true).to_string(), script.to_string()]
    }

    /*
    CDXC:Zmx 2026-09-01:
    Probe and snapshot pipelines only invoke the bundled zmx binary, `ps`, and
    shell builtins; they run on every ~2s presentation poll and never present
    shell output to a user. Sourcing the login profile there costs tens of
    milliseconds per spawn and buys nothing, so those call sites use these
    profile-free args. Attach/run scripts and the launchd supervisor keep
    `command_flag`, because they hand the user their own shell.
    */
    pub fn profileless_script_args(&self, script: &str) -> Vec<String> {
        vec![
            self.profileless_command_flag().to_string(),
            script.to_string(),
        ]
    }

    pub fn profileless_command_flag(&self) -> &'static str {
        "-c"
    }

    pub fn command_flag(&self, interactive: bool) -> &'static str {
        match (&self.kind, interactive) {
            (PlatformShellKind::Bash | PlatformShellKind::Zsh, true) => "-lic",
            (PlatformShellKind::Bash | PlatformShellKind::Zsh, false) => "-lc",
            (PlatformShellKind::Sh, true) => "-ic",
            (PlatformShellKind::Sh, false) => "-c",
        }
    }

    pub fn command_string(&self, script: &str, interactive: bool) -> String {
        format!(
            "{} {} {}",
            self.executable,
            self.command_flag(interactive),
            shell_quote(script)
        )
    }
}

fn command_shell_candidates() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        vec!["/bin/zsh".to_string()]
    }

    #[cfg(not(target_os = "macos"))]
    {
        vec![
            env::var("SHELL").unwrap_or_default(),
            "/bin/bash".to_string(),
            "/usr/bin/bash".to_string(),
            "/bin/zsh".to_string(),
            "/usr/bin/zsh".to_string(),
            "/bin/sh".to_string(),
            "/usr/bin/sh".to_string(),
        ]
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
    }
}

fn shell_kind_for_path(path: &str) -> PlatformShellKind {
    match Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
    {
        "bash" => PlatformShellKind::Bash,
        "zsh" => PlatformShellKind::Zsh,
        _ => PlatformShellKind::Sh,
    }
}

#[cfg(not(target_os = "macos"))]
fn is_supported_shell(path: &str) -> bool {
    matches!(
        Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default(),
        "bash" | "sh" | "zsh"
    )
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .filter(|value| seen.insert(PathBuf::from(value).to_string_lossy().to_string()))
        .collect()
}
