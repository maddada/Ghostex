use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock, PoisonError},
    time::{Duration, Instant},
};

#[cfg(not(target_os = "windows"))]
use std::{
    fs,
    io::Read,
    process::{Command, Stdio},
};

pub const SHELL_PATH_SENTINEL: &str = "__GHOSTEX_SHELL_PATH__:";
/// Total time one probe may take, across every shell it tries.
const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const CACHE_TTL_EMPTY: Duration = Duration::from_secs(60);
/*
CDXC:OsIntegration 2026-10-31 WHY:
The macOS app sets its own PATH once, before GPUI starts any thread, so app startup waits on the
login-shell probe. A slow or hanging shell profile must never delay the window by more than this:
past it the app keeps its inherited PATH plus `fallback_tool_dirs` (what it used before the probe
existed) and the probe finishes in the background for gxserver's launch PATH.
*/
pub const STARTUP_PROBE_WAIT: Duration = Duration::from_millis(500);

#[derive(Default)]
struct ProbeSlot {
    probing: bool,
    result: Option<ProbedEntries>,
}

struct ProbedEntries {
    fetched_at: Instant,
    entries: Vec<String>,
}

impl ProbeSlot {
    fn fresh_entries(&self) -> Option<Vec<String>> {
        self.result
            .as_ref()
            .filter(|probed| {
                !probed.entries.is_empty() || probed.fetched_at.elapsed() < CACHE_TTL_EMPTY
            })
            .map(|probed| probed.entries.clone())
    }
}

type SharedProbeSlot = Arc<(Mutex<ProbeSlot>, Condvar)>;

static LOGIN_SHELL_CACHE: OnceLock<Mutex<HashMap<PathBuf, SharedProbeSlot>>> = OnceLock::new();

/// Returns cached or newly-probed login shell PATH entries for `home_dir`, waiting for the probe
/// (at most `DEFAULT_PROBE_TIMEOUT`). A failed probe yields no entries.
pub fn login_shell_path_entries(home_dir: &Path) -> Vec<String> {
    login_shell_path_entries_within(home_dir, DEFAULT_PROBE_TIMEOUT + Duration::from_millis(250))
        .unwrap_or_default()
}

/// `login_shell_path_entries` that waits at most `wait`. `None` means the probe is still running:
/// it keeps going on its own thread and a later call gets its result. Concurrent callers share one
/// probe instead of each starting a shell.
pub fn login_shell_path_entries_within(home_dir: &Path, wait: Duration) -> Option<Vec<String>> {
    // Read PATH here, not on the probe thread: the caller may replace PATH right after a timeout.
    let initial_path = env::var_os("PATH");
    let home = home_dir.to_path_buf();
    entries_within(home_dir, wait, move || {
        probe_login_shell_path_entries_with_path(
            &home,
            DEFAULT_PROBE_TIMEOUT,
            initial_path.as_deref(),
        )
    })
}

fn entries_within(
    key: &Path,
    wait: Duration,
    probe: impl FnOnce() -> Vec<String> + Send + 'static,
) -> Option<Vec<String>> {
    let slot = {
        let cache = LOGIN_SHELL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut slots = cache.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(slots.entry(key.to_path_buf()).or_default())
    };
    let (lock, ready) = &*slot;
    let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(entries) = state.fresh_entries() {
        return Some(entries);
    }
    if !state.probing {
        state.probing = true;
        let probe_slot = Arc::clone(&slot);
        let spawned = std::thread::Builder::new()
            .name("ghostex-login-shell-path".to_string())
            .spawn(move || {
                // A panicking probe must still clear `probing`, or every later caller waits it out.
                let entries = std::panic::catch_unwind(std::panic::AssertUnwindSafe(probe))
                    .unwrap_or_default();
                let (lock, ready) = &*probe_slot;
                let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
                state.probing = false;
                state.result = Some(ProbedEntries {
                    fetched_at: Instant::now(),
                    entries,
                });
                ready.notify_all();
            });
        if spawned.is_err() {
            state.probing = false;
            return None;
        }
    }
    let deadline = Instant::now() + wait;
    while state.probing {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        state = ready
            .wait_timeout(state, remaining)
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
    Some(
        state
            .result
            .as_ref()
            .map(|probed| probed.entries.clone())
            .unwrap_or_default(),
    )
}

/// Raw login-shell PATH probe. Spawns the user's login shell interactively to capture
/// the full environment ($PATH) as configured by their shell profile.
pub fn probe_login_shell_path_entries(home_dir: &Path, timeout: Duration) -> Vec<String> {
    let initial_path = env::var_os("PATH");
    probe_login_shell_path_entries_with_path(home_dir, timeout, initial_path.as_deref())
}

/// Raw login-shell PATH probe with a custom initial PATH environment. `timeout` bounds the whole
/// probe, not each shell it tries. Windows has no login shell to ask and returns no entries.
pub fn probe_login_shell_path_entries_with_path(
    home_dir: &Path,
    timeout: Duration,
    initial_path: Option<&OsStr>,
) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let _ = (home_dir, timeout, initial_path);
        Vec::new()
    }

    #[cfg(not(target_os = "windows"))]
    {
        probe_candidates(login_shell_candidates(), home_dir, timeout, initial_path)
    }
}

#[cfg(not(target_os = "windows"))]
fn probe_candidates(
    candidates: Vec<String>,
    home_dir: &Path,
    timeout: Duration,
    initial_path: Option<&OsStr>,
) -> Vec<String> {
    let deadline = Instant::now() + timeout;
    for candidate in candidates {
        if !is_executable(Path::new(&candidate)) {
            continue;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let entries = run_login_shell_path_probe(&candidate, home_dir, remaining, initial_path);
        if !entries.is_empty() {
            return entries;
        }
    }
    Vec::new()
}

/// Derives a normalized PATH for the user tool environment.
///
/// 1. Probes the user's authentic login shell PATH (cached per process).
/// 2. Extends with any existing `current_path` entries.
/// 3. Appends `fallback_tool_dirs`, so a failed probe still finds the usual tool folders.
pub fn normalized_user_tool_path(current_path: Option<&str>, home_dir: &Path) -> String {
    normalized_user_tool_path_with(login_shell_path_entries(home_dir), current_path, home_dir)
}

/// `normalized_user_tool_path` with the login-shell entries already in hand.
pub fn normalized_user_tool_path_with(
    login_shell_entries: Vec<String>,
    current_path: Option<&str>,
    home_dir: &Path,
) -> String {
    let mut entries = login_shell_entries;
    entries.extend(split_path(current_path));
    entries.extend(fallback_tool_dirs(home_dir));
    let mut unique_entries = unique_path_entries(entries);
    keep_setuid_wrappers_ahead_of_nix(&mut unique_entries);
    env::join_paths(&unique_entries)
        .map(|joined| joined.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/*
CDXC:OsIntegration 2026-10-31 WHY:
The folders the app searched before it asked the login shell (Homebrew, version-manager shims, Nix
profiles, `~/.local/bin`). They come after the probed and inherited PATH, so they never shadow what
the user's shell chose, but a probe that fails or times out, or a profile that never adds them,
still leaves those tools findable. A missing folder costs nothing at lookup.
*/
pub fn fallback_tool_dirs(home_dir: &Path) -> Vec<String> {
    let mut entries = Vec::new();

    #[cfg(target_os = "macos")]
    entries.extend([
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
    ]);

    #[cfg(not(target_os = "windows"))]
    {
        entries.extend(["/usr/local/bin".to_string(), "/usr/local/sbin".to_string()]);
        for relative in [
            ".volta/bin",
            ".local/share/mise/shims",
            ".local/bin",
            ".asdf/shims",
            ".nodenv/shims",
            ".nix-profile/bin",
        ] {
            entries.push(home_dir.join(relative).to_string_lossy().into_owned());
        }
        // NixOS keeps its setuid `sudo` and `pkexec` here; the store copies cannot elevate.
        #[cfg(target_os = "linux")]
        entries.push(NIXOS_SETUID_WRAPPERS.to_string());
        if let Some(user) = env::var("USER")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            entries.push(format!("/etc/profiles/per-user/{user}/bin"));
        }
        entries.extend([
            "/run/current-system/sw/bin".to_string(),
            "/nix/var/nix/profiles/default/bin".to_string(),
            "/usr/bin".to_string(),
            "/bin".to_string(),
            "/usr/sbin".to_string(),
            "/sbin".to_string(),
        ]);
    }

    #[cfg(target_os = "windows")]
    let _ = home_dir;

    entries
}

const NIXOS_SETUID_WRAPPERS: &str = "/run/wrappers/bin";

/// `pkexec must be setuid root`: on NixOS the wrappers folder has to win over every Nix profile.
fn keep_setuid_wrappers_ahead_of_nix(entries: &mut Vec<String>) {
    let Some(wrappers) = entries
        .iter()
        .position(|entry| entry == NIXOS_SETUID_WRAPPERS)
    else {
        return;
    };
    let Some(first_nix) = entries.iter().position(|entry| is_nix_entry(entry)) else {
        return;
    };
    if first_nix < wrappers {
        let entry = entries.remove(wrappers);
        entries.insert(first_nix, entry);
    }
}

fn is_nix_entry(entry: &str) -> bool {
    entry.starts_with("/nix/")
        || entry.starts_with("/run/current-system/")
        || entry.starts_with("/etc/profiles/per-user/")
        || entry.contains("/.nix-profile/")
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

    let Some(stdout) = run_probe_command(command, timeout) else {
        return Vec::new();
    };

    stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(SHELL_PATH_SENTINEL))
        .map(|path| split_path(Some(path)))
        .unwrap_or_default()
        .into_iter()
        // A relative entry (`node_modules/.bin`) would resolve inside whatever project folder a
        // command runs in, letting that project's files shadow the user's tools.
        .filter(|entry| Path::new(entry).is_absolute())
        .collect()
}

/*
CDXC:OsIntegration 2026-10-31 WHY:
Stdout is drained on its own thread while the shell runs, so a profile that prints more than a pipe
holds cannot block it. A background process the profile started inherits that pipe and can hold it
open long after the shell exits, so nothing here waits for end of file: once the shell has exited
and its sentinel line has arrived, the probe is done, and every wait ends at `timeout`. A shell
still running at `timeout` is killed. It stays in the caller's process group (an interactive shell
in a group of its own stops itself with SIGTTIN when the `ghostex` CLI runs it from a terminal), so
only the shell itself is killed.
*/
#[cfg(not(target_os = "windows"))]
fn run_probe_command(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let deadline = Instant::now() + timeout;
    let mut child = command.spawn().ok()?;
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let reader = child.stdout.take().map(|mut pipe| {
        let buffer = Arc::clone(&buffer);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            while let Ok(read) = pipe.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                buffer
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(&chunk[..read]);
            }
        })
    });

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }

    let output = || {
        buffer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    };
    loop {
        let finished = reader.as_ref().is_none_or(|reader| reader.is_finished());
        if finished || has_complete_sentinel_line(&output()) || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Some(String::from_utf8_lossy(&output()).into_owned())
}

#[cfg(not(target_os = "windows"))]
fn has_complete_sentinel_line(output: &[u8]) -> bool {
    let sentinel = SHELL_PATH_SENTINEL.as_bytes();
    output
        .windows(sentinel.len())
        .rposition(|window| window == sentinel)
        .is_some_and(|start| output[start..].contains(&b'\n'))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(not(target_os = "windows"))]
    fn scratch_home(name: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let home = env::temp_dir().join(format!(
            "ghostex-tool-path-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        home
    }

    fn joined_entries(path: &str) -> Vec<String> {
        split_path(Some(path))
    }

    #[test]
    fn fallbacks_follow_probed_and_inherited_entries() {
        let home = Path::new("/home/someone");
        let probed = vec!["/probe/bin".to_string(), "/usr/bin".to_string()];
        let current = env::join_paths(["/inherited/bin", "/probe/bin"]).unwrap();
        let path = normalized_user_tool_path_with(probed, current.to_str(), home);
        let entries = joined_entries(&path);

        assert_eq!(&entries[..3], ["/probe/bin", "/usr/bin", "/inherited/bin"]);
        assert_eq!(entries.len(), entries.iter().collect::<HashSet<_>>().len());
        #[cfg(not(target_os = "windows"))]
        {
            let position = |entry: &str| entries.iter().position(|value| value == entry);
            for relative in [
                ".volta/bin",
                ".local/share/mise/shims",
                ".local/bin",
                ".asdf/shims",
                ".nodenv/shims",
                ".nix-profile/bin",
            ] {
                let entry = home.join(relative).to_string_lossy().into_owned();
                assert!(position(&entry).unwrap() > 2, "{entry} is a fallback");
            }
            assert!(position("/run/current-system/sw/bin").unwrap() > 2);
            assert!(position("/nix/var/nix/profiles/default/bin").unwrap() > 2);
            // A fallback the probe already listed keeps the probe's place.
            assert_eq!(position("/usr/bin"), Some(1));
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn failed_probe_still_finds_the_usual_tool_folders() {
        let home = Path::new("/home/someone");
        let path = normalized_user_tool_path_with(Vec::new(), Some("/usr/bin:/bin"), home);
        let entries = joined_entries(&path);
        assert_eq!(&entries[..2], ["/usr/bin", "/bin"]);
        for expected in [
            "/home/someone/.local/share/mise/shims",
            "/home/someone/.asdf/shims",
            "/home/someone/.volta/bin",
            "/home/someone/.nodenv/shims",
            "/home/someone/.nix-profile/bin",
            "/home/someone/.local/bin",
            "/usr/local/bin",
        ] {
            assert!(entries.iter().any(|entry| entry == expected), "{expected}");
        }
        #[cfg(target_os = "macos")]
        assert!(entries.iter().any(|entry| entry == "/opt/homebrew/bin"));
    }

    #[test]
    fn setuid_wrappers_stay_ahead_of_nix_profiles() {
        let mut entries = vec![
            "/home/someone/.nix-profile/bin".to_string(),
            "/run/current-system/sw/bin".to_string(),
            "/usr/bin".to_string(),
            NIXOS_SETUID_WRAPPERS.to_string(),
        ];
        keep_setuid_wrappers_ahead_of_nix(&mut entries);
        assert_eq!(
            entries,
            [
                NIXOS_SETUID_WRAPPERS,
                "/home/someone/.nix-profile/bin",
                "/run/current-system/sw/bin",
                "/usr/bin",
            ]
        );

        // Already first (the NixOS default): untouched.
        let mut nixos = vec![
            NIXOS_SETUID_WRAPPERS.to_string(),
            "/run/current-system/sw/bin".to_string(),
        ];
        keep_setuid_wrappers_ahead_of_nix(&mut nixos);
        assert_eq!(nixos[0], NIXOS_SETUID_WRAPPERS);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_nix_entries_do_not_shadow_the_wrappers_fallback() {
        // A probe that failed, with an inherited PATH that has the store `pkexec` but no wrappers.
        let path = normalized_user_tool_path_with(
            Vec::new(),
            Some("/run/current-system/sw/bin:/usr/bin"),
            Path::new("/home/someone"),
        );
        let entries = joined_entries(&path);
        assert_eq!(entries[0], NIXOS_SETUID_WRAPPERS);
        assert_eq!(entries[1], "/run/current-system/sw/bin");
    }

    #[test]
    fn concurrent_callers_share_one_probe_and_stop_waiting_at_their_limit() {
        let key = PathBuf::from(format!("/ghostex-test-key/{}", std::process::id()));
        let probes = Arc::new(AtomicUsize::new(0));
        let slow_probe = |probes: Arc<AtomicUsize>| {
            move || {
                probes.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(600));
                vec!["/probe/bin".to_string()]
            }
        };

        let started = Instant::now();
        assert_eq!(
            entries_within(&key, Duration::from_millis(50), slow_probe(probes.clone())),
            None
        );
        assert!(started.elapsed() < Duration::from_millis(400));
        // A second caller while the first probe runs waits for it instead of starting another.
        assert_eq!(
            entries_within(&key, Duration::from_secs(5), slow_probe(probes.clone())),
            Some(vec!["/probe/bin".to_string()])
        );
        assert_eq!(probes.load(Ordering::SeqCst), 1);
        // Cached from now on.
        assert_eq!(
            entries_within(&key, Duration::ZERO, slow_probe(probes.clone())),
            Some(vec!["/probe/bin".to_string()])
        );
        assert_eq!(probes.load(Ordering::SeqCst), 1);
    }

    #[cfg(not(target_os = "windows"))]
    const MARKER_PATH: &str = "/ghostex-probe-test/bin:/usr/bin:/bin";

    #[cfg(not(target_os = "windows"))]
    fn probe_sh_with_profile(profile: &str, timeout: Duration) -> (Vec<String>, Duration) {
        // `sh -l` sources `$HOME/.profile`; the probe sets HOME to this folder.
        let home = scratch_home("profile");
        fs::write(home.join(".profile"), profile).unwrap();
        let started = Instant::now();
        let entries =
            run_login_shell_path_probe("/bin/sh", &home, timeout, Some(OsStr::new(MARKER_PATH)));
        let elapsed = started.elapsed();
        let _ = fs::remove_dir_all(&home);
        (entries, elapsed)
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn probe_returns_while_a_background_process_holds_stdout_open() {
        // The background `sleep` inherits the probe's stdout pipe and keeps it open after the
        // shell exits; waiting for end of file would take its full five seconds.
        let (entries, elapsed) = probe_sh_with_profile("sleep 5 &\n", Duration::from_secs(4));
        assert!(
            entries
                .iter()
                .any(|entry| entry == "/ghostex-probe-test/bin"),
            "{entries:?}"
        );
        assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn probe_drops_relative_entries() {
        let (entries, _) = probe_sh_with_profile(
            "PATH=node_modules/.bin:.:$PATH\nexport PATH\n",
            Duration::from_secs(4),
        );
        assert!(entries
            .iter()
            .any(|entry| entry == "/ghostex-probe-test/bin"));
        assert!(
            entries.iter().all(|entry| entry.starts_with('/')),
            "{entries:?}"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn probe_gives_up_on_a_hanging_profile_at_its_deadline() {
        let (entries, elapsed) = probe_sh_with_profile("sleep 5\n", Duration::from_millis(300));
        assert!(entries.is_empty());
        assert!(elapsed < Duration::from_millis(1500), "took {elapsed:?}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn probe_reads_output_larger_than_a_pipe_buffer() {
        let line = "x".repeat(99);
        let profile = format!("i=0\nwhile [ $i -lt 2000 ]; do echo {line}; i=$((i+1)); done\n");
        let (entries, elapsed) = probe_sh_with_profile(&profile, Duration::from_secs(4));
        assert!(
            entries
                .iter()
                .any(|entry| entry == "/ghostex-probe-test/bin"),
            "{entries:?}"
        );
        assert!(elapsed < Duration::from_secs(4));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn one_deadline_covers_every_candidate_shell() {
        let home = scratch_home("candidates");
        fs::write(home.join(".profile"), "sleep 5\n").unwrap();
        let started = Instant::now();
        let entries = probe_candidates(
            vec!["/bin/sh".to_string(); 3],
            &home,
            Duration::from_millis(400),
            Some(OsStr::new(MARKER_PATH)),
        );
        let elapsed = started.elapsed();
        let _ = fs::remove_dir_all(&home);
        assert!(entries.is_empty());
        // Three shells at 400ms each would take 1.2s.
        assert!(elapsed < Duration::from_millis(1000), "took {elapsed:?}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn sentinel_line_must_be_complete() {
        assert!(!has_complete_sentinel_line(
            b"noise\n__GHOSTEX_SHELL_PATH__:/a:/b"
        ));
        assert!(has_complete_sentinel_line(
            b"noise\n__GHOSTEX_SHELL_PATH__:/a:/b\n"
        ));
        assert!(!has_complete_sentinel_line(b"noise only\n"));
    }
}
