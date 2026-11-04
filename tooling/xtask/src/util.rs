use std::cell::{Cell, RefCell};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type Res<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err(format!($($arg)*).into())
    };
}

/// The repository root. The binary is only ever built from this checkout, so the compile-time manifest path is exact.
pub fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("tooling/xtask sits two levels below the repository root")
}

pub fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// The trimmed value of an environment variable, or None when it is unset or blank.
pub fn env_trimmed(key: &str) -> Option<String> {
    env_var(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn truthy(value: Option<String>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// The cargo debug-info level `--debug-info[=<level>]` or `GHOSTEX_DEBUG_INFO` asks for: `line-tables-only` (function names and file:line in backtraces) for a bare flag or a truthy value, otherwise the level given (`limited`, `full`).
pub fn debug_info_level(flag: Option<&str>) -> Res<Option<String>> {
    let requested = match flag {
        Some(level) => Some(level.to_string()),
        None => env_trimmed("GHOSTEX_DEBUG_INFO"),
    };
    let Some(requested) = requested else {
        return Ok(None);
    };
    let level = requested.to_ascii_lowercase();
    match level.as_str() {
        "" | "1" | "true" | "yes" | "on" | "line-tables-only" => {
            Ok(Some("line-tables-only".into()))
        }
        "limited" | "full" => Ok(Some(level)),
        "0" | "false" | "no" | "off" => Ok(None),
        other => crate::bail!(
            "Unknown debug-info level: {other}. Use line-tables-only (the default), limited or full."
        ),
    }
}

/// Environment that turns debug info and symbols back on for every cargo build a command runs.
/// It undoes the root `.cargo/config.toml` (CDXC:Build 2026-10-11): the profile keys for dev and release, and a later `/DEBUG` that overrides its `/DEBUG:NONE` so the MSVC linker writes a .pdb again.
pub fn debug_info_environment(level: &str) -> Vec<(String, String)> {
    let mut environment = Vec::new();
    for profile in ["DEV", "RELEASE"] {
        environment.push((format!("CARGO_PROFILE_{profile}_DEBUG"), level.to_string()));
        environment.push((format!("CARGO_PROFILE_{profile}_STRIP"), "none".to_string()));
    }
    for triple in ["X86_64_PC_WINDOWS_MSVC", "AARCH64_PC_WINDOWS_MSVC"] {
        environment.push((
            format!("CARGO_TARGET_{triple}_RUSTFLAGS"),
            "-C link-arg=/DEBUG".to_string(),
        ));
    }
    environment
}

/// CDXC:Build 2026-10-26 WHY: Zig is resolved here exactly as in tooling/zig-toolchain.sh (which holds the full rationale), so `cargo xtask` finds the required Zig when PATH `zig` is another version and never builds with the wrong one. The series is the vendored Ghostty's `.minimum_zig_version`; GHOSTEX_ZIG, then ZIG, must be that series when set; then PATH `zig`, Homebrew, /usr/local, mise installs, ~/apps tarballs and build/toolchains are searched; no match fails with an install hint.
/// SEE-ALSO: tooling/zig-toolchain.sh, apps/desktop/build.rs (find_zig), server/package-remote-linux.mjs (resolveZigBinary).
pub fn find_zig() -> Res<String> {
    let zon_path = root().join(".dependencies/ghostty/build.zig.zon");
    let zon = fs::read_to_string(&zon_path)?;
    let Some(series) = zon.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(".minimum_zig_version")?;
        let (_, value) = rest.split_once('"')?;
        let mut parts = value.split('.');
        Some(format!("{}.{}", parts.next()?, parts.next()?))
    }) else {
        bail!("No .minimum_zig_version in {}", zon_path.display())
    };
    let wanted = format!("{series}.");
    let version_of = |candidate: &str| -> Option<String> {
        let output = Command::new(candidate).arg("version").output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    for name in ["GHOSTEX_ZIG", "ZIG"] {
        let Some(value) = env_trimmed(name) else {
            continue;
        };
        match version_of(&value) {
            Some(version) if version.starts_with(&wanted) => return Ok(value),
            version => bail!(
                "{name}={value} is not a Zig {series}.x binary (it reports: {}). Point {name} at Zig {series}.x, or unset it so the build finds one.",
                version.as_deref().unwrap_or("unavailable")
            ),
        }
    }
    let home = home_dir();
    let mise_zig = env_trimmed("MISE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share/mise"))
        .join("installs/zig");
    let globbed = |dir: &Path, keep: &dyn Fn(&str) -> bool, suffix: &[&str]| -> Vec<String> {
        let mut entries: Vec<String> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| keep(&entry.file_name().to_string_lossy()))
            .map(|entry| {
                suffix
                    .iter()
                    .fold(entry.path(), |path, part| path.join(part))
                    .display()
                    .to_string()
            })
            .collect();
        entries.sort();
        entries
    };
    let in_series = |name: &str| name.starts_with(&wanted);
    let tarball = |name: &str| name.starts_with("zig-") && name.contains(&format!("-{wanted}"));
    let toolchain = |name: &str| name.starts_with(&format!("zig-{wanted}"));
    let mut candidates = vec![
        "zig".to_string(),
        format!("/opt/homebrew/opt/zig@{series}/bin/zig"),
        "/opt/homebrew/bin/zig".to_string(),
        format!("/usr/local/opt/zig@{series}/bin/zig"),
        "/usr/local/bin/zig".to_string(),
    ];
    candidates.extend(globbed(&mise_zig, &in_series, &["bin", "zig"]));
    candidates.extend(globbed(&mise_zig, &in_series, &["zig"]));
    candidates.extend(globbed(&home.join("apps"), &tarball, &["zig"]));
    candidates.extend(globbed(
        &root().join("build/toolchains"),
        &toolchain,
        &["zig"],
    ));
    candidates.extend(globbed(
        &root().join("build/toolchains"),
        &toolchain,
        &["zig.exe"],
    ));
    for candidate in candidates {
        if version_of(&candidate).is_some_and(|version| version.starts_with(&wanted)) {
            return Ok(candidate);
        }
    }
    bail!(
        "Zig {series}.x is required (the vendored Ghostty's minimum_zig_version), and none was found on PATH, in Homebrew, mise, ~/apps or build/toolchains. Install it with `mise install zig@{series}.0` (or Homebrew's zig when it is {series}), or set GHOSTEX_ZIG to a Zig {series}.x binary."
    )
}

pub fn home_dir() -> PathBuf {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    PathBuf::from(env_var(key).unwrap_or_default())
}

/// `<XDG_STATE_HOME or ~/.local/state>`, ignoring a relative XDG value like the app does.
pub fn xdg_state_root() -> PathBuf {
    match env_trimmed("XDG_STATE_HOME") {
        Some(value) if Path::new(&value).is_absolute() => PathBuf::from(value),
        _ => home_dir().join(".local").join("state"),
    }
}

/// CDXC:Build 2026-10-01 WHY:
/// `cargo run` hands its child CARGO_*, RUSTUP_TOOLCHAIN (set to the toolchain that built the xtask) and library-path variables. Children inherit them, and a RUSTUP_TOOLCHAIN in the environment outranks every `rust-toolchain.toml`, so the desktop and gxserver builds the start runs would compile with the machine's default Rust instead of their pinned toolchain. Drop what cargo and rustup added before anything is spawned; a RUSTUP_TOOLCHAIN the user exported themselves (source `env`) stays.
pub fn sanitize_cargo_run_environment() {
    let toolchain_source = env_var("RUSTUP_TOOLCHAIN_SOURCE");
    let rustup_set_toolchain = match toolchain_source.as_deref() {
        Some(source) => source != "env",
        None => env_var("RUST_RECURSION_COUNT").is_some(),
    };
    let toolchain_dir = match (env_var("RUSTUP_HOME"), env_var("RUSTUP_TOOLCHAIN")) {
        (Some(home), Some(toolchain)) => {
            Some(PathBuf::from(home).join("toolchains").join(toolchain))
        }
        _ => None,
    };
    let own_target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    for (key, _) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        let cargo_set = matches!(
            key,
            "CARGO"
                | "CARGO_MANIFEST_DIR"
                | "CARGO_MANIFEST_PATH"
                | "CARGO_BIN_NAME"
                | "CARGO_CRATE_NAME"
                | "CARGO_PRIMARY_PACKAGE"
        ) || key.starts_with("CARGO_PKG_");
        if cargo_set || key == "RUSTUP_TOOLCHAIN_SOURCE" || key == "RUST_RECURSION_COUNT" {
            std::env::remove_var(key);
        }
    }
    if rustup_set_toolchain {
        std::env::remove_var("RUSTUP_TOOLCHAIN");
    }
    let library_path_key = if cfg!(target_os = "macos") {
        "DYLD_FALLBACK_LIBRARY_PATH"
    } else if cfg!(windows) {
        "PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    if let Some(value) = std::env::var_os(library_path_key) {
        let kept: Vec<PathBuf> = std::env::split_paths(&value)
            .filter(|entry| {
                !entry.starts_with(&own_target)
                    && !toolchain_dir
                        .as_ref()
                        .is_some_and(|dir| entry.starts_with(dir))
            })
            .collect();
        if kept.is_empty() {
            std::env::remove_var(library_path_key);
        } else if let Ok(joined) = std::env::join_paths(kept) {
            std::env::set_var(library_path_key, joined);
        }
    }
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.current_dir(root());
    command
}

/// Runs a JavaScript or TypeScript file with bun from the repository root.
pub fn bun<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = command("bun");
    command.args(args);
    command
}

/// Runs a package binary from node_modules (tsc, vitest, vite, tailwindcss) the way a package.json script did.
pub fn bun_x<I, S>(tool: &str, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = command("bun");
    command.arg("x").arg(tool).args(args);
    command
}

pub fn spawn_error(command: &Command, error: std::io::Error) -> Box<dyn std::error::Error> {
    format!(
        "could not run {}: {error}",
        command.get_program().to_string_lossy()
    )
    .into()
}

/// Runs with inherited stdio and returns the exit code (1 when killed by a signal).
pub fn status_code(command: &mut Command) -> Res<i32> {
    let status = command
        .status()
        .map_err(|error| spawn_error(command, error))?;
    Ok(status.code().unwrap_or(1))
}

/// Runs with inherited stdio and fails on a non-zero exit.
pub fn check(command: &mut Command) -> Res {
    let status = command
        .status()
        .map_err(|error| spawn_error(command, error))?;
    if !status.success() {
        bail!(
            "{} failed with exit code {}.",
            describe(command),
            status.code().unwrap_or(1)
        );
    }
    Ok(())
}

/// Captures stdout and stderr.
pub fn output(command: &mut Command) -> Res<Output> {
    command.stdin(Stdio::null());
    command
        .output()
        .map_err(|error| spawn_error(command, error))
}

/// Captured stdout when the command succeeds, otherwise None.
pub fn stdout_if_ok(command: &mut Command) -> Option<String> {
    command.stdin(Stdio::null()).stderr(Stdio::null());
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn describe(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().into_owned()];
    parts.extend(
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned()),
    );
    parts.join(" ")
}

pub fn format_command(command: &Command) -> String {
    let mut parts = vec![shell_quote(&command.get_program().to_string_lossy())];
    parts.extend(
        command
            .get_args()
            .map(|arg| shell_quote(&arg.to_string_lossy())),
    );
    parts.join(" ")
}

fn shell_quote(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=@%+-".contains(c));
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

pub fn format_duration(duration: Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 10_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}s", (ms as f64 / 1000.0).round() as u64)
    }
}

pub fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Local wall-clock time as `YYYY-MM-DD hh:mm:ss am`, the last line every start prints.
pub fn local_timestamp() -> String {
    let (year, month, day, hour, minute, second) = local_time();
    let meridiem = if hour >= 12 { "pm" } else { "am" };
    let hour12 = match hour % 12 {
        0 => 12,
        other => other,
    };
    format!("{year}-{month:02}-{day:02} {hour12:02}:{minute:02}:{second:02} {meridiem}")
}

#[cfg(unix)]
fn local_time() -> (i32, u32, u32, u32, u32, u32) {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    (
        tm.tm_year + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
        tm.tm_sec as u32,
    )
}

#[cfg(windows)]
fn local_time() -> (i32, u32, u32, u32, u32, u32) {
    #[repr(C)]
    struct SystemTimeParts {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(time: *mut SystemTimeParts);
    }
    let mut time: SystemTimeParts = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut time) };
    (
        time.year as i32,
        time.month as u32,
        time.day as u32,
        time.hour as u32,
        time.minute as u32,
        time.second as u32,
    )
}

/// Step logging and quiet command runs shared by `start` and `start-server`.
pub struct Log {
    pub verbose: bool,
    /// The command a failure tells the user to rerun with `--verbose`.
    pub rerun_hint: String,
    step: Cell<u32>,
    active: RefCell<Option<(String, Instant)>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Summary {
    None,
    Rsync,
    Codesign,
}

const QUIET_LOG_TAIL_BYTES: u64 = 256 * 1024;
const QUIET_LOG_TAIL_LINES: usize = 220;
const QUIET_LOG_LINE_MAX_CHARS: usize = 1200;
const QUIET_LOG_LINE_HEAD_CHARS: usize = 760;
const QUIET_LOG_LINE_TAIL_CHARS: usize = 260;

impl Log {
    pub fn new(verbose: bool, rerun_hint: String) -> Self {
        Self {
            verbose,
            rerun_hint,
            step: Cell::new(0),
            active: RefCell::new(None),
        }
    }

    pub fn step(&self, message: &str) {
        if self.verbose {
            return;
        }
        self.finish_step();
        self.step.set(self.step.get() + 1);
        *self.active.borrow_mut() = Some((message.to_string(), Instant::now()));
        println!("[{}] {message}", self.step.get());
    }

    pub fn detail(&self, message: &str) {
        if !self.verbose {
            println!("    {message}");
        }
    }

    /// A detail line that verbose runs print too.
    pub fn notice(&self, message: &str) {
        if self.verbose {
            println!("{message}");
        } else {
            self.detail(message);
        }
    }

    pub fn finish_step(&self) {
        if self.verbose {
            return;
        }
        if let Some((_, started)) = self.active.borrow_mut().take() {
            println!("    Completed in {}.", format_duration(started.elapsed()));
        }
    }

    /// Runs a command with live output, or (unless verbose) into a log file that is printed only when it fails.
    pub fn run(&self, command: &mut Command, label: &str, summary: Summary) -> Res {
        if self.verbose {
            return check(command);
        }
        let log_path = quiet_log_path(label);
        let status = run_into_log(command, &log_path)?;
        if !status.success() {
            self.report_failure(label, status.code().unwrap_or(1), &log_path);
            bail!(
                "{} failed with exit code {}.",
                describe(command),
                status.code().unwrap_or(1)
            );
        }
        match summary {
            Summary::Rsync => {
                self.summarize_rsync(&fs::read_to_string(&log_path).unwrap_or_default())
            }
            Summary::Codesign => {
                self.summarize_codesign(&fs::read_to_string(&log_path).unwrap_or_default())
            }
            Summary::None => {}
        }
        let _ = fs::remove_file(&log_path);
        Ok(())
    }

    pub fn report_failure(&self, label: &str, status: i32, log_path: &Path) {
        let relative = log_path
            .strip_prefix(root())
            .unwrap_or(log_path)
            .display()
            .to_string();
        eprintln!("{label} failed with exit code {status}.");
        eprintln!("Full log: {relative}");
        eprintln!("Rerun with {} --verbose for live output.", self.rerun_hint);
        let tail = read_quiet_log_tail(log_path);
        if !tail.is_empty() {
            eprintln!(
                "\nLast {QUIET_LOG_TAIL_LINES} log lines (long lines shortened; full lines remain in {relative}):\n{tail}"
            );
        }
    }

    fn summarize_rsync(&self, log: &str) {
        let (mut deleted, mut files, mut directories, mut links, mut other) = (0, 0, 0, 0, 0);
        for raw in log.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("$ ") {
                continue;
            }
            if line.starts_with("*deleting ") {
                deleted += 1;
                continue;
            }
            let bytes = line.as_bytes();
            if bytes.len() < 2 || !b"<>ch.*".contains(&bytes[0]) || !b"fdLDS".contains(&bytes[1]) {
                continue;
            }
            match bytes[1] {
                b'f' => files += 1,
                b'd' => directories += 1,
                b'L' => links += 1,
                _ => other += 1,
            }
        }
        let updated = files + directories + links + other;
        if updated == 0 && deleted == 0 {
            self.detail("Install sync: installed bundle was already current.");
            return;
        }
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        let mut parts = Vec::new();
        if files > 0 {
            parts.push(plural(files, "file", "files"));
        }
        if directories > 0 {
            parts.push(plural(directories, "directory", "directories"));
        }
        if links > 0 {
            parts.push(plural(links, "link", "links"));
        }
        if other > 0 {
            parts.push(plural(other, "other item", "other items"));
        }
        let updated_text = if parts.is_empty() {
            format!("{updated} updated")
        } else {
            format!("{updated} updated ({})", parts.join(", "))
        };
        self.detail(&format!("Install sync: {updated_text}, {deleted} deleted."));
    }

    fn summarize_codesign(&self, log: &str) {
        let lines: Vec<&str> = log
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if let Some(identity) = lines.iter().find(|line| line.starts_with("Identity: ")) {
            self.detail(identity);
        }
        let replaced = lines
            .iter()
            .filter(|line| line.contains("replacing existing signature"))
            .count();
        if replaced > 0 {
            self.detail(&format!(
                "Re-signed {replaced} nested code item{}.",
                if replaced == 1 { "" } else { "s" }
            ));
        }
        if lines.iter().any(|line| line.contains("valid on disk"))
            && lines
                .iter()
                .any(|line| line.contains("satisfies its Designated Requirement"))
        {
            self.detail("Code signature verified.");
        }
    }
}

pub fn quiet_log_path(label: &str) -> PathBuf {
    let mut normalized = String::new();
    for c in label.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            normalized.push(c);
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
    }
    let normalized = normalized.trim_matches('-');
    let normalized = if normalized.is_empty() {
        "command"
    } else {
        normalized
    };
    root().join("build").join("local-start-logs").join(format!(
        "{}-{}-{normalized}.log",
        unix_millis(),
        std::process::id()
    ))
}

/// Opens the log, writes the command line, and points the child's stdout and stderr at it.
pub fn redirect_into_log(command: &mut Command, log_path: &Path) -> Res<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(log_path)?;
    writeln!(file, "$ {}", format_command(command))?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(file.try_clone()?))
        .stderr(Stdio::from(file));
    Ok(())
}

fn run_into_log(command: &mut Command, log_path: &Path) -> Res<ExitStatus> {
    redirect_into_log(command, log_path)?;
    command
        .status()
        .map_err(|error| spawn_error(command, error))
}

fn read_quiet_log_tail(log_path: &Path) -> String {
    let Ok(mut file) = File::open(log_path) else {
        return String::new();
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = size.saturating_sub(QUIET_LOG_TAIL_BYTES);
    if size == start || file.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect();
    if start > 0 {
        lines[0] = "[output truncated]".to_string();
    }
    let from = lines.len().saturating_sub(QUIET_LOG_TAIL_LINES);
    lines[from..]
        .iter()
        .map(|line| shorten_log_line(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

fn shorten_log_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= QUIET_LOG_LINE_MAX_CHARS {
        return line.to_string();
    }
    let omitted = chars.len() - QUIET_LOG_LINE_HEAD_CHARS - QUIET_LOG_LINE_TAIL_CHARS;
    let prefix: String = chars[..QUIET_LOG_LINE_HEAD_CHARS].iter().collect();
    let suffix: String = chars[chars.len() - QUIET_LOG_LINE_TAIL_CHARS..]
        .iter()
        .collect();
    format!(
        "{} ... [shortened {omitted} characters from one log line; full line remains in the log file] ... {}",
        prefix.trim_end(),
        suffix.trim_start()
    )
}

/// Removes the variables that switch off colour in the build tools' output; the start's quiet logs keep colour for the terminal.
pub fn remove_color_disabling_environment() {
    for key in ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"] {
        std::env::remove_var(key);
    }
    if let Some(value) = env_var("FORCE_COLOR") {
        if matches!(value.trim().to_ascii_lowercase().as_str(), "0" | "false") {
            std::env::remove_var("FORCE_COLOR");
        }
    }
}

/// CDXC:PlatformSupport 2026-09-18 WHY:
/// PowerShell 7 prepends its own module directories to PSModulePath, and every child process inherits them.
/// The Windows build and install scripts run under Windows PowerShell 5.1 (powershell.exe), which then autoloads PowerShell 7's Microsoft.PowerShell.Utility 7.0 instead of its own and loses cmdlets such as Get-FileHash, so a start from a pwsh terminal failed with "Get-FileHash is not recognized".
/// Drop exactly PowerShell 7's three entries (its $PSHOME, shared and per-user module directories) so 5.1 resolves its own modules.
/// The match is anchored because other products also install under a PowerShell\Modules folder (SQL Server ships ...\Tools\PowerShell\Modules), and those must survive.
pub fn remove_powershell7_module_paths() {
    if !cfg!(windows) {
        return;
    }
    let Some((key, value)) =
        std::env::vars().find(|(name, _)| name.eq_ignore_ascii_case("psmodulepath"))
    else {
        return;
    };
    let kept: Vec<&str> = value
        .split(';')
        .filter(|entry| !entry.is_empty() && !is_powershell7_module_entry(entry))
        .collect();
    std::env::set_var(key, kept.join(";"));
}

fn is_powershell7_module_entry(entry: &str) -> bool {
    let normalized = entry
        .trim()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
        .replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').collect();
    let n = segments.len();
    // .../powershell/7*/modules
    if n >= 3
        && segments[n - 1] == "modules"
        && segments[n - 2].starts_with('7')
        && segments[n - 3] == "powershell"
    {
        return true;
    }
    // .../documents/powershell/modules
    if n >= 3
        && segments[n - 1] == "modules"
        && segments[n - 2] == "powershell"
        && segments[n - 3] == "documents"
    {
        return true;
    }
    // <drive>:/program files/powershell/modules
    if n == 4
        && segments[0].len() == 2
        && segments[0].ends_with(':')
        && segments[0].as_bytes()[0].is_ascii_lowercase()
        && segments[1] == "program files"
        && segments[2] == "powershell"
        && segments[3] == "modules"
    {
        return true;
    }
    // Portable PowerShell installs can use any directory name. Their Core-only built-ins must not shadow Windows PowerShell's Desktop modules.
    let manifest = Path::new(entry.trim())
        .join("Microsoft.PowerShell.Utility")
        .join("Microsoft.PowerShell.Utility.psd1");
    let Ok(text) = fs::read_to_string(manifest) else {
        return false;
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = strip_prefix_ignore_case(trimmed, "CompatiblePSEditions") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix("@(") else {
            continue;
        };
        let Some(end) = rest.find(')') else { continue };
        let editions = rest[..end].to_lowercase();
        let has = |name: &str| {
            editions.contains(&format!("'{name}'")) || editions.contains(&format!("\"{name}\""))
        };
        return has("core") && !has("desktop");
    }
    false
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    (text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| &text[prefix.len()..])
}

/// Signals a process: SIGTERM, or SIGKILL when `force`. A process that already exited is ignored.
#[cfg(unix)]
pub fn kill_pid(pid: i32, force: bool) {
    unsafe {
        libc::kill(pid, if force { libc::SIGKILL } else { libc::SIGTERM });
    }
}

#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    stdout_if_ok(Command::new("tasklist").args([
        "/FI",
        &format!("PID eq {pid}"),
        "/FO",
        "CSV",
        "/NH",
    ]))
    .is_some_and(|out| out.contains(&format!("\"{pid}\"")))
}

/// Holds the start lock for the life of the process.
pub enum StartLock {
    #[cfg(unix)]
    Flock(#[allow(dead_code)] File),
    #[cfg(windows)]
    PidFile(PathBuf),
}

#[cfg(windows)]
impl Drop for StartLock {
    fn drop(&mut self) {
        let StartLock::PidFile(path) = self;
        // A stale PID lock is detected and replaced by the next start.
        let _ = fs::remove_file(path);
    }
}

/// CDXC:Build 2026-06-21-18:43:
/// The start prevents overlapping rebuilds: `start` and `start-server` take the same lock, so the two can never write the installed app at once. On macOS and Linux it is an flock on build/ghostex-gpui-local-start.lock (what lockf(1) and flock(1) took when the JS launcher re-executed itself under them); a second start waits for the first. Windows records the holder's pid and refuses while that process lives.
pub fn acquire_start_lock(command_name: &str) -> Res<StartLock> {
    let lock_path = root().join("build").join("ghostex-gpui-local-start.lock");
    fs::create_dir_all(lock_path.parent().expect("lock path has a parent"))?;
    #[cfg(unix)]
    {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            println!("Waiting for another `cargo xtask {command_name}` to finish...");
            if unsafe { libc::flock(fd, libc::LOCK_EX) } != 0 {
                bail!(
                    "could not lock {}: {}",
                    lock_path.display(),
                    std::io::Error::last_os_error()
                );
            }
        }
        Ok(StartLock::Flock(file))
    }
    #[cfg(windows)]
    {
        for _ in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    write!(file, "{}", std::process::id())?;
                    return Ok(StartLock::PidFile(lock_path));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let holder = fs::read_to_string(&lock_path)
                        .unwrap_or_default()
                        .trim()
                        .parse::<u32>()
                        .ok();
                    if let Some(pid) = holder.filter(|pid| *pid > 0 && pid_alive(*pid)) {
                        bail!("Another `cargo xtask {command_name}` (pid {pid}) is already rebuilding the GPUI app.");
                    }
                    let _ = fs::remove_file(&lock_path);
                }
                Err(error) => return Err(error.into()),
            }
        }
        bail!(
            "Could not acquire the GPUI start lock at {}.",
            lock_path.display()
        )
    }
}

/// Downloads a public URL with curl (shipped by macOS, Linux distributions and Windows 10+), writing through a `.partial` file.
pub fn download(url: &str, destination: &Path) -> Res {
    let partial = PathBuf::from(format!("{}.partial", destination.display()));
    let curl = if cfg!(windows) { "curl.exe" } else { "curl" };
    let result = output(
        Command::new(curl)
            .args(["-fsSL", "--retry", "2", "-o"])
            .arg(&partial)
            .arg(url),
    )?;
    if !result.status.success() {
        let _ = fs::remove_file(&partial);
        bail!(
            "Could not download {url}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    fs::rename(&partial, destination)?;
    Ok(())
}
