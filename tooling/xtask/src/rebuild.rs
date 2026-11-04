//! `cargo xtask rebuild` (macOS): one unattended `cargo xtask start` for a coordinator's background shell. It refuses to build other sessions' uncommitted edits, checks that this shell can sign, writes the start into a numbered log, retries kache's known flake once, and ends with a short summary.
//!
//! CDXC:Build 2026-11-05 DECISION:
//! User (Q1233): the coordinator's rebuild should be "just a command for this repo", not a ghostex CLI verb, so a coordinator no longer starts a thread per rebuild only to run `cargo xtask start` into a log and check what restarted.
//! Building only committed code: the build has to read its sources from this checkout, because cargo's incremental caches, kache, build-cache.sh stamps and the signed staged bundle are all keyed to these paths. A worktree, stash or exported copy would either touch other sessions' live edits or rebuild from cold in a folder the user has to manage, so instead the command refuses while an uncommitted file would feed the build, lists those files, and waits for them to be committed with `--wait` (a thread's edit is usually minutes from its commit) or builds them anyway with `--allow-dirty`. Notes and docs (`*.md` outside `skills/`, `ai/`, `docs/`) and trees the desktop start never builds (`apps/mobile/`, `apps/gpui-web/`) don't count.
//! Signing: a shell whose macOS security session ended (a gxserver that outlived a logout, or one restarted from such a shell, starts its sessions there) cannot reach the keychain, so codesign fails with errSecInternalComponent after the whole build, and `launchctl print gui/<uid>` answers "141: Reentrancy avoided". A start from such a shell would also relaunch the app and gxserver into that dead session, so the command stops before building and says where to run it instead.
//! SEE-ALSO: server/src/server/macos_login_recovery.rs, tooling/xtask/src/codesign.rs.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::util::{
    env_trimmed, format_duration, home_dir, local_timestamp, output, root, sleep_ms, stdout_if_ok,
    Res,
};
use crate::{bail, codesign, gxserver};

const USAGE: &str = "cargo xtask rebuild [--dry-run] [--allow-dirty] [--wait[=<minutes>]] [--log-dir <dir>] [--optimized] [--debug-info[=<level>]] [-- <more start arguments>]";

/// kunobi-ninja/kache#1810: kache sometimes fails to identify the compiler process; the same build passes when run again.
const KACHE_FLAKE: &str = "cannot read compiler process identity";
const DEFAULT_WAIT_MINUTES: u64 = 20;
const WAIT_POLL: Duration = Duration::from_secs(20);
const APP_EXECUTABLE: &str = "/Applications/Ghostex.app/Contents/MacOS/Ghostex";

struct Options {
    dry_run: bool,
    allow_dirty: bool,
    wait: Option<Duration>,
    log_dir: PathBuf,
    start_args: Vec<String>,
}

pub fn run(args: &[String]) -> Res<i32> {
    if !cfg!(target_os = "macos") {
        bail!("cargo xtask rebuild is macOS only; use cargo xtask start elsewhere.");
    }
    let opts = parse_options(args)?;
    let began = Instant::now();

    // 1. Only committed code, unless asked otherwise.
    let mut dirty = dirty_build_inputs()?;
    if !dirty.is_empty() && !opts.allow_dirty && !opts.dry_run {
        if let Some(wait) = opts.wait {
            println!(
                "Waiting up to {} for {} uncommitted build input(s) to be committed...",
                format_duration(wait),
                dirty.len()
            );
            let deadline = Instant::now() + wait;
            while !dirty.is_empty() && Instant::now() < deadline {
                std::thread::sleep(WAIT_POLL);
                dirty = dirty_build_inputs()?;
            }
        }
        if !dirty.is_empty() {
            println!(
                "Rebuild refused: {} uncommitted file(s) would be built into the app:",
                dirty.len()
            );
            print_paths(&dirty);
            println!("Wait for their sessions to commit (--wait), or build them anyway with --allow-dirty.");
            return Ok(2);
        }
    }

    // 2. This shell must be able to sign, and must be in the current login.
    if let Some(problem) = stale_login_session() {
        println!("Rebuild refused: {}", stale_session_advice(&problem));
        return Ok(3);
    }
    let identity = env_trimmed("GHOSTEX_GPUI_SIGN_IDENTITY").or_else(codesign::remembered);
    if let Some(identity) = identity.as_deref().filter(|identity| *identity != "-") {
        if let Some(failure) = codesign::signing_failure(identity) {
            println!("Rebuild refused: {}", signing_advice(&failure));
            return Ok(3);
        }
    }

    let log_path = next_log_path(&opts.log_dir)?;
    let mut start = Command::new(std::env::current_exe()?);
    start
        .arg("start")
        .args(&opts.start_args)
        .current_dir(root());
    // The start finds a Zig of the right series by itself; a stale override in a long-lived agent shell made it fail.
    start.env_remove("GHOSTEX_ZIG").env_remove("ZIG");
    if let Some(identity) = &identity {
        start.env("GHOSTEX_GPUI_SIGN_IDENTITY", identity);
    }
    let head = git_head_summary();
    let built = if dirty.is_empty() {
        format!("{head} (no uncommitted build inputs)")
    } else {
        format!(
            "{head} plus {} uncommitted file(s){}",
            dirty.len(),
            if opts.allow_dirty {
                " (--allow-dirty)"
            } else {
                ""
            }
        )
    };

    if opts.dry_run {
        println!("Dry run: nothing was built.");
        println!("Would build: {built}");
        if !dirty.is_empty() {
            println!(
                "Uncommitted build inputs (a real run {}):",
                if opts.allow_dirty {
                    "builds them because of --allow-dirty"
                } else {
                    "refuses until they are committed"
                }
            );
            print_paths(&dirty);
        }
        println!(
            "Signing: {} (this shell can sign; login session is current)",
            identity.as_deref().unwrap_or("resolved by the start")
        );
        println!("Would run: cargo xtask start {}", opts.start_args.join(" "));
        println!("Log: {}", log_path.display());
        println!("gxserver now: {}", describe_gxserver(read_gxserver()));
        println!("App now: {}", describe_pid(app_pid()));
        return Ok(0);
    }

    // 3. Build, install and relaunch.
    let gxserver_before = read_gxserver();
    let app_before = app_pid();
    let head_before = git_head();
    println!("Rebuilding {built}; log: {}", log_path.display());
    let mut code = run_start(&mut start, &log_path, "")?;
    let mut retried = false;
    if code != 0 && log_contains(&log_path, KACHE_FLAKE) {
        retried = true;
        code = run_start(
            &mut start,
            &log_path,
            "Retrying once after kache's \"cannot read compiler process identity\" (kunobi-ninja/kache#1810).",
        )?;
    }

    // 4. Summary.
    let elapsed = format_duration(began.elapsed());
    let log_text = fs::read_to_string(&log_path).unwrap_or_default();
    if code == 0 {
        println!("Rebuild succeeded in {elapsed}.");
    } else {
        println!("Rebuild FAILED (exit {code}) after {elapsed}.");
    }
    if retried {
        println!(
            "Retried once after kache's \"{KACHE_FLAKE}\" (kunobi-ninja/kache#1810){}.",
            if code == 0 { "; the retry passed" } else { "" }
        );
    }
    println!("Built: {built}");
    let gxserver_after = wait_for_gxserver();
    println!(
        "gxserver: {}",
        match (&gxserver_before, &gxserver_after) {
            (_, None) => "not answering".to_string(),
            (Some(before), Some(after)) if before.pid == after.pid => {
                format!("kept running (pid {})", after.pid)
            }
            (Some(before), Some(after)) => {
                format!("restarted (pid {} -> {})", before.pid, after.pid)
            }
            (None, Some(after)) => format!("started (pid {})", after.pid),
        }
    );
    let app_after = if code == 0 { wait_for_app() } else { app_pid() };
    println!(
        "App: {}",
        match (app_before, app_after) {
            (_, None) => "not running".to_string(),
            (Some(before), Some(after)) if before == after => {
                format!("pid {after} (not relaunched)")
            }
            (Some(before), Some(after)) => format!("relaunched (pid {before} -> {after})"),
            (None, Some(after)) => format!("launched (pid {after})"),
        }
    );
    let changed = tree_changed_during_build(&head_before, &dirty);
    if let Some(changed) = changed {
        println!("Note: {changed}");
    }
    println!("Log: {}", log_path.display());
    if code != 0 {
        if log_text.contains("errSecInternalComponent") {
            let problem = stale_login_session().unwrap_or_default();
            println!(
                "{}",
                if problem.is_empty() {
                    signing_advice("errSecInternalComponent")
                } else {
                    stale_session_advice(&problem)
                }
            );
        }
        println!("Last error lines:");
        for line in error_lines(&log_text) {
            println!("  {line}");
        }
    }
    println!("{}", local_timestamp());
    Ok(code)
}

fn parse_options(args: &[String]) -> Res<Options> {
    let mut opts = Options {
        dry_run: false,
        allow_dirty: false,
        wait: None,
        log_dir: home_dir().join(".local/share/agent-work/Ghostex/rebuilds"),
        start_args: Vec::new(),
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--dry-run" => opts.dry_run = true,
            "--allow-dirty" => opts.allow_dirty = true,
            "--wait" => opts.wait = Some(Duration::from_secs(DEFAULT_WAIT_MINUTES * 60)),
            "--log-dir" => {
                let Some(dir) = iter.next() else {
                    bail!("--log-dir needs a folder.\nUsage: {USAGE}");
                };
                opts.log_dir = PathBuf::from(dir);
            }
            "--optimized" => opts.start_args.push(arg.clone()),
            "--" => opts.start_args.extend(iter.by_ref().cloned()),
            "--help" | "-h" => {
                println!("Usage: {USAGE}");
                std::process::exit(0);
            }
            other if other.starts_with("--wait=") => {
                let minutes: u64 = other["--wait=".len()..]
                    .parse()
                    .map_err(|_| format!("--wait takes whole minutes, got `{other}`."))?;
                opts.wait = Some(Duration::from_secs(minutes * 60));
            }
            other if other.starts_with("--debug-info") => opts.start_args.push(arg.clone()),
            other => bail!("Unknown argument `{other}`.\nUsage: {USAGE}"),
        }
    }
    if !opts.log_dir.is_absolute() {
        opts.log_dir = std::env::current_dir()?.join(&opts.log_dir);
    }
    Ok(opts)
}

/// Uncommitted paths (modified, staged, deleted, untracked, or a submodule with changes) that the desktop start reads.
fn dirty_build_inputs() -> Res<Vec<String>> {
    let out = output(Command::new("git").current_dir(root()).args([
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--ignore-submodules=none",
    ]))?;
    if !out.status.success() {
        bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut entries = text.split('\0').filter(|entry| !entry.is_empty());
    let mut paths = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (status, path) = entry.split_at(3);
        // A rename or copy is followed by its source path.
        if status.starts_with('R') || status.starts_with('C') {
            entries.next();
        }
        if is_build_input(path) {
            paths.push(format!("{}{path}", status));
        }
    }
    Ok(paths)
}

fn is_build_input(path: &str) -> bool {
    const NOT_BUILT: [&str; 5] = ["ai/", "docs/", ".github/", "apps/mobile/", "apps/gpui-web/"];
    // Markdown under skills/ is bundled and compiled in (include_str!); everywhere else it is notes.
    let notes = path.ends_with(".md") && !path.starts_with("skills/");
    !notes && !NOT_BUILT.iter().any(|prefix| path.starts_with(prefix))
}

fn print_paths(paths: &[String]) {
    const SHOWN: usize = 40;
    for path in paths.iter().take(SHOWN) {
        println!("  {path}");
    }
    if paths.len() > SHOWN {
        println!("  ... and {} more", paths.len() - SHOWN);
    }
}

/// Why this shell is outside the current desktop login, when it is: `launchctl print gui/<uid>` fails ("141: Reentrancy avoided") from a security session that ended.
fn stale_login_session() -> Option<String> {
    let uid = unsafe { libc::getuid() };
    let out = output(Command::new("launchctl").args(["print", &format!("gui/{uid}")])).ok()?;
    if out.status.success() {
        return None;
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let said = said
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no output")
        .to_string();
    Some(format!("launchctl print gui/{uid}: {said}"))
}

fn stale_session_advice(problem: &str) -> String {
    format!(
        "this shell runs in an ended macOS login session ({problem}), so codesign can't reach the keychain (errSecInternalComponent) and a start would relaunch the app there; run the rebuild from a session started since the last login (a new Ghostex session, or Terminal.app)."
    )
}

fn signing_advice(failure: &str) -> String {
    let manager = stdout_if_ok(Command::new("launchctl").arg("managername"))
        .map(|name| name.trim().to_string())
        .unwrap_or_default();
    let failure = failure.lines().next().unwrap_or(failure);
    if failure.contains("errSecInternalComponent") && !manager.is_empty() && manager != "Aqua" {
        format!(
            "codesign failed ({failure}) because this shell is in a {manager} session without keychain access; run the rebuild from a session inside the desktop login (a new Ghostex session, or Terminal.app)."
        )
    } else if failure.contains("no identity found") {
        format!(
            "codesign can't sign from this shell ({failure}); check GHOSTEX_GPUI_SIGN_IDENTITY against `security find-identity -v -p codesigning`."
        )
    } else {
        format!(
            "codesign can't sign from this shell ({failure}); unlock the login keychain in Terminal.app (`security unlock-keychain`) and run it again."
        )
    }
}

/// The next `rebuild-NNN.log` in `dir`.
fn next_log_path(dir: &Path) -> Res<PathBuf> {
    fs::create_dir_all(dir)?;
    let last = fs::read_dir(dir)?
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_prefix("rebuild-")?
                .strip_suffix(".log")?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    Ok(dir.join(format!("rebuild-{:03}.log", last + 1)))
}

/// Runs the start with its output appended to the log; returns its exit code.
fn run_start(start: &mut Command, log_path: &Path, banner: &str) -> Res<i32> {
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    if !banner.is_empty() {
        writeln!(log, "\n==== {banner}")?;
    }
    writeln!(
        log,
        "==== {} $ cargo xtask {}",
        local_timestamp(),
        start
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    )?;
    let stderr: File = log.try_clone()?;
    let status = start
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(stderr)
        .status()?;
    let code = status.code().unwrap_or(1);
    let mut log = OpenOptions::new().append(true).open(log_path)?;
    writeln!(log, "==== exit {code}")?;
    Ok(code)
}

fn log_contains(log_path: &Path, needle: &str) -> bool {
    fs::read_to_string(log_path).is_ok_and(|text| text.contains(needle))
}

/// The lines most likely to say why the start failed, or the end of the log.
fn error_lines(log: &str) -> Vec<String> {
    const KEEP: usize = 12;
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty() && !line.starts_with("===="))
        .collect();
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error") || lower.contains("failed") || lower.contains("errsec")
        })
        .collect();
    let chosen = if errors.is_empty() { &lines } else { &errors };
    chosen[chosen.len().saturating_sub(KEEP)..]
        .iter()
        .map(|line| {
            if line.chars().count() > 300 {
                format!("{}...", line.chars().take(300).collect::<String>())
            } else {
                line.to_string()
            }
        })
        .collect()
}

struct GxserverRun {
    pid: u64,
}

/// The gxserver that runtime/server.json records, when it answers its health check.
fn read_gxserver() -> Option<GxserverRun> {
    let record = gxserver::ghostex_state_dir()
        .join("gxserver")
        .join("runtime")
        .join("server.json");
    let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(record).ok()?).ok()?;
    let pid = parsed.get("pid")?.as_u64()?;
    let port = parsed.get("port").and_then(|p| p.as_u64()).unwrap_or(58744);
    let token = gxserver::read_token()?;
    gxserver::health(
        &format!("http://127.0.0.1:{port}"),
        &token,
        Duration::from_secs(2),
    )?;
    Some(GxserverRun { pid })
}

fn wait_for_gxserver() -> Option<GxserverRun> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(run) = read_gxserver() {
            return Some(run);
        }
        if Instant::now() >= deadline {
            return None;
        }
        sleep_ms(500);
    }
}

fn describe_gxserver(run: Option<GxserverRun>) -> String {
    run.map_or("not answering".into(), |run| format!("pid {}", run.pid))
}

fn app_pid() -> Option<u32> {
    stdout_if_ok(Command::new("pgrep").args(["-f", &format!("^{APP_EXECUTABLE}$")]))?
        .lines()
        .find_map(|line| line.trim().parse().ok())
}

fn wait_for_app() -> Option<u32> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(pid) = app_pid() {
            return Some(pid);
        }
        if Instant::now() >= deadline {
            return None;
        }
        sleep_ms(500);
    }
}

fn describe_pid(pid: Option<u32>) -> String {
    pid.map_or("not running".into(), |pid| format!("pid {pid}"))
}

fn git_head() -> String {
    stdout_if_ok(
        Command::new("git")
            .current_dir(root())
            .args(["rev-parse", "HEAD"]),
    )
    .map(|head| head.trim().to_string())
    .unwrap_or_default()
}

fn git_head_summary() -> String {
    stdout_if_ok(
        Command::new("git")
            .current_dir(root())
            .args(["log", "-1", "--format=%h %s"]),
    )
    .map(|line| {
        let line = line.trim();
        let (hash, subject) = line.split_once(' ').unwrap_or((line, ""));
        let subject: String = subject.chars().take(70).collect();
        format!("HEAD {hash} \"{subject}\"")
    })
    .unwrap_or_else(|| "HEAD".into())
}

/// Whether commits or edits landed while the build ran, so the app may hold some of them and not others.
fn tree_changed_during_build(head_before: &str, dirty_before: &[String]) -> Option<String> {
    let head_after = git_head();
    let dirty_after = dirty_build_inputs().unwrap_or_default();
    let new_edits: Vec<&String> = dirty_after
        .iter()
        .filter(|path| !dirty_before.contains(path))
        .collect();
    let mut notes = Vec::new();
    if head_after != head_before {
        notes.push("new commits landed during the build".to_string());
    }
    if !new_edits.is_empty() {
        let shown: Vec<&str> = new_edits.iter().take(5).map(|p| p.trim()).collect();
        notes.push(format!(
            "{} file(s) were edited during the build ({}{})",
            new_edits.len(),
            shown.join(", "),
            if new_edits.len() > 5 { ", ..." } else { "" }
        ));
    }
    (!notes.is_empty()).then(|| {
        format!(
            "{}; the app may include part of them, rebuild again to be sure.",
            notes.join(" and ")
        )
    })
}
