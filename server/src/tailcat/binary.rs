use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

/*
CDXC:RemotePairing 2026-09-05 SEE-ALSO:
Keep this lookup in sync with apps/desktop/src/app/helpers/remote/easy_connect_forward.rs so both serving and connecting find the helper installed by Settings.
The managed install follows explicit overrides and PATH; existing Go and Homebrew installs are still recognized.
*/
pub fn resolve_tailcat_binary() -> Option<PathBuf> {
    tailcat_binary_candidates()
        .into_iter()
        .find(|candidate| is_executable_file(candidate))
}

fn tailcat_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = env::var_os("GHOSTEX_TAILCAT_BIN") {
        if !value.is_empty() {
            candidates.push(PathBuf::from(value));
        }
    }
    if let Some(path) = env::var_os("PATH") {
        candidates
            .extend(env::split_paths(&path).map(|directory| directory.join(executable_name())));
    }
    candidates.push(managed_tailcat_binary());
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join("go").join("bin").join(executable_name()));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin").join(executable_name()));
    candidates
}

pub(crate) fn managed_tailcat_binary() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .data_dir
        .join("tailcat/bin")
        .join(executable_name())
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "tailcat.exe"
    } else {
        "tailcat"
    }
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

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

pub fn read_tailcat_binary_version(binary_path: &Path) -> Option<String> {
    let mut child = Command::new(binary_path)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    }
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.lines().next().unwrap_or_default().trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}
