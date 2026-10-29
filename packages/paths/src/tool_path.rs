use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

pub const SHELL_PATH_SENTINEL: &str = "__GHOSTEX_SHELL_PATH__:";
const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const CACHE_TTL_EMPTY: Duration = Duration::from_secs(60);

struct CachedPathEntries {
    fetched_at: Instant,
    entries: Vec<String>,
}

static LOGIN_SHELL_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedPathEntries>>> = OnceLock::new();

/// Returns cached or newly-probed login shell PATH entries for `home_dir`.
pub fn login_shell_path_entries(home_dir: &Path) -> Vec<String> {
    let cache = LOGIN_SHELL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(entry) = guard.get(home_dir).filter(|entry| {
            !entry.entries.is_empty() || entry.fetched_at.elapsed() < CACHE_TTL_EMPTY
        }) {
            return entry.entries.clone();
        }
    }

    let entries = probe_login_shell_path_entries(home_dir, DEFAULT_PROBE_TIMEOUT);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            home_dir.to_path_buf(),
            CachedPathEntries {
                fetched_at: Instant::now(),
                entries: entries.clone(),
            },
        );
    }
    entries
}

/// Raw login-shell PATH probe. Spawns the user's login shell interactively to capture
/// the full environment ($PATH) as configured by their shell profile.
pub fn probe_login_shell_path_entries(home_dir: &Path, timeout: Duration) -> Vec<String> {
    let initial_path = env::var_os("PATH");
    probe_login_shell_path_entries_with_path(home_dir, timeout, initial_path.as_deref())
}

/// Raw login-shell PATH probe with a custom initial PATH environment.
pub fn probe_login_shell_path_entries_with_path(
    home_dir: &Path,
    timeout: Duration,
    initial_path: Option<&OsStr>,
) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let _ = (home_dir, timeout);
        split_path(initial_path.and_then(|path| path.to_str()))
    }

    #[cfg(not(target_os = "windows"))]
    {
        for candidate in login_shell_candidates() {
            let candidate_path = PathBuf::from(&candidate);
            if !is_executable(&candidate_path) {
                continue;
            }
            let entries = run_login_shell_path_probe(&candidate, home_dir, timeout, initial_path);
            if !entries.is_empty() {
                return entries;
            }
        }
        Vec::new()
    }
}

/// Derives a normalized PATH for the user tool environment.
///
/// 1. Probes the user's authentic login shell PATH (cached per process).
/// 2. Extends with any existing `current_path` entries.
/// 3. Appends standard platform fallback directories (/usr/local/bin, /usr/bin, etc.).
pub fn normalized_user_tool_path(current_path: Option<&str>, home_dir: &Path) -> String {
    let mut entries = Vec::new();
    entries.extend(login_shell_path_entries(home_dir));
    entries.extend(split_path(current_path));

    #[cfg(target_os = "macos")]
    {
        entries.push("/opt/homebrew/bin".to_string());
        entries.push("/opt/homebrew/sbin".to_string());
    }

    #[cfg(not(target_os = "windows"))]
    {
        entries.extend([
            "/usr/local/bin".to_string(),
            "/usr/local/sbin".to_string(),
            "/usr/bin".to_string(),
            "/bin".to_string(),
            "/usr/sbin".to_string(),
            "/sbin".to_string(),
        ]);
    }

    let unique_entries = unique_path_entries(entries);
    env::join_paths(&unique_entries)
        .map(|joined| joined.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn split_path(value: Option<&str>) -> Vec<String> {
    let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
        return Vec::new();
    };
    env::split_paths(value)
        .map(|p| p.to_string_lossy().trim().to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

pub fn unique_path_entries(entries: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for entry in entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            output.push(trimmed.to_string());
        }
    }
    output
}

#[cfg(not(target_os = "windows"))]
fn run_login_shell_path_probe(
    shell_path: &str,
    home_dir: &Path,
    timeout: Duration,
    initial_path: Option<&OsStr>,
) -> Vec<String> {
    let shell_name = Path::new(shell_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    let mut command = Command::new(shell_path);
    let probe_script = format!("printf '\\n{SHELL_PATH_SENTINEL}%s\\n' \"$PATH\"");
    match shell_name {
        "fish" => {
            command.args(["-l", "-i", "-c", &probe_script]);
        }
        "bash" | "zsh" => {
            command.args(["-lic", &probe_script]);
        }
        _ => {
            command.args(["-l", "-c", &probe_script]);
        }
    }

    command.env("HOME", home_dir);
    if let Some(path) = initial_path {
        command.env("PATH", path);
    }
    for key in [
        "ANSI_COLORS_DISABLED",
        "NO_COLOR",
        "NODE_DISABLE_COLORS",
        "CLICOLOR_FORCE",
    ] {
        command.env_remove(key);
    }

    let Some(stdout) = run_command_stdout_with_timeout(command, timeout) else {
        return Vec::new();
    };

    stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(SHELL_PATH_SENTINEL))
        .map(|path| split_path(Some(path)))
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn run_command_stdout_with_timeout(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command.spawn().ok()?;
    let reader = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            use std::io::Read;
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = reader.and_then(|r| r.join().ok()).unwrap_or_default();
                if status.success() {
                    return String::from_utf8(stdout).ok();
                } else {
                    return None;
                }
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn login_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(configured_shell) = env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        candidates.push(configured_shell);
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push("/bin/zsh".to_string());
        candidates.push("/bin/bash".to_string());
        candidates.push("/bin/sh".to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        candidates.push("/bin/bash".to_string());
        candidates.push("/usr/bin/bash".to_string());
        candidates.push("/bin/zsh".to_string());
        candidates.push("/usr/bin/zsh".to_string());
        candidates.push("/bin/sh".to_string());
        candidates.push("/usr/bin/sh".to_string());
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|c| seen.insert(c.clone()))
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
