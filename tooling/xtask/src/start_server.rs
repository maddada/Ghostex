//! `cargo xtask start-server`: rebuild only gxserver and swap it into the installed app.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::bail;
use crate::gxserver::Probe;
use crate::http::NoAnswer;
use crate::util::{
    self, acquire_start_lock, env_trimmed, env_var, home_dir, output, root, sleep_ms, Res,
};
use crate::{codesign, gxserver};

const INSTALLED_APP: &str = "/Applications/Ghostex.app";
const BASE_URL: &str = "http://127.0.0.1:58744";
const LAUNCHD_LABEL: &str = "com.madda.ghostex.gxserver";
/// zmx (its bundle and its link) is left as installed: a changed zmx needs the full start.
/// The bundled skills are left as installed too: build-macos-app.sh stages them into the package
/// (`Web/gxserver/skills`, which `CLI/skills` links to), and the gxserver-only build has none.
const INSTALL_RSYNC_ARGS: [&str; 8] = [
    "-a",
    "--delete",
    "--exclude",
    "/bin/zmx",
    "--exclude",
    "/GhostexSessions.app",
    "--exclude",
    "/skills",
];

/// CDXC:Build 2026-09-24 DECISION:
/// User: a fix that only needs gxserver should be testable without the full start, which rebuilds, closes and reopens the whole app. `cargo xtask start-server` rebuilds only the gxserver package, signs it like the start does, installs it into /Applications/Ghostex.app in place and restarts only the daemon; the open app reconnects to it.
/// zmx is never replaced here (a changed zmx needs the full start), and the app's outer signature stays stale until the next `cargo xtask start` re-syncs the bundle.
/// SEE-ALSO: GHOSTEX_MACOS_GXSERVER_ONLY in apps/desktop/scripts/prepare-macos-runtime.sh; stop_gxserver_control_plane in tooling/xtask/src/start/install.rs; gpui_spawn_local_gxserver_launchd_job in apps/desktop/src/app/helpers/board_gxserver/gxserver_health_and_daemon.rs.
pub fn run(args: &[String]) -> Res<i32> {
    if !cfg!(target_os = "macos") {
        bail!("only macOS is supported; use `cargo xtask start`.");
    }
    if env_trimmed("GHOSTEX_HOME").is_some() {
        bail!("GHOSTEX_HOME selects an isolated install; `cargo xtask start-server` only updates {INSTALLED_APP}.");
    }
    if let Some(unknown) = args.iter().find(|arg| arg.as_str() != "--optimized") {
        bail!("unknown argument {unknown}. Usage: cargo xtask start-server [--optimized]");
    }
    let optimized = !args.is_empty() || env_var("GHOSTEX_START_OPTIMIZED").as_deref() == Some("1");

    let _lock = acquire_start_lock("start-server")?;
    let runtime_dir = root().join("apps/desktop/runtime/macos/Web/gxserver");
    let stage_dir = root().join("build/start-server.noindex/gxserver");
    let installed_app = Path::new(INSTALLED_APP);
    let installed_dir = installed_app.join("Contents/Resources/Web/gxserver");
    let installed_cli = installed_app.join("Contents/Resources/CLI/ghostex");

    if !installed_dir.exists() {
        bail!(
            "{} does not exist. Run `cargo xtask start` once first.",
            installed_dir.display()
        );
    }
    let probe = installed_dir.join(format!(".ghostex-install-probe-{}", std::process::id()));
    if let Err(error) = fs::write(&probe, "") {
        bail!(
            "macOS will not let this command modify {INSTALLED_APP} ({error}).\nOpen System Settings > Privacy & Security > App Management and turn it on for the app this command runs in (Ghostex, or your terminal), then run it again."
        );
    }
    let _ = fs::remove_file(&probe);
    let sign_identity = codesign::resolve_identity(installed_app)?;
    let timestamp_flag = codesign::timestamp_flag();

    step(&format!(
        "Building gxserver{}...",
        if optimized { " (optimized)" } else { "" }
    ));
    let mut build = util::command("/bin/bash");
    build
        .arg(root().join("apps/desktop/scripts/prepare-macos-runtime.sh"))
        .env("GHOSTEX_APP_VARIANT", "prod")
        .env("GHOSTEX_LOCAL_START", "1")
        .env("GHOSTEX_MACOS_GXSERVER_ONLY", "1");
    if optimized {
        build.env("GHOSTEX_START_OPTIMIZED", "1");
    }
    if util::status_code(&mut build)? != 0 {
        bail!("the gxserver build failed; nothing was installed.");
    }

    let Some(expected) = gxserver::read_build_identity(&runtime_dir.join("build-identity.json"))
    else {
        bail!(
            "the built package has no build identity in {}.",
            runtime_dir.display()
        );
    };
    let built_zmx = zmx_version(&runtime_dir)?;
    let installed_zmx = zmx_version(&installed_dir)?;
    if built_zmx != installed_zmx {
        bail!("the staged zmx ({built_zmx}) differs from the installed one ({installed_zmx}). A zmx change needs `cargo xtask start`.");
    }
    let source_generation = source_zmx_wire_generation()?;
    if !installed_zmx.split(", ").any(|line| {
        line.split_whitespace()
            .eq(["wire_generation", &source_generation])
    }) {
        bail!("the zmx source speaks wire generation {source_generation}, the installed zmx ({installed_zmx}) does not. A zmx change needs `cargo xtask start`.");
    }

    let token = gxserver::read_token_file();
    // A gxserver that answers anything but its health record (it refuses the token, or it stays silent for PATIENT_PROBE) still holds the port; installing beside it would leave the old build running, so say so instead of reading it as stopped.
    let before = match &token {
        None => None,
        Some((token, token_file)) => match gxserver::probe_patiently(BASE_URL, token) {
            Probe::Healthy(health) => Some(health),
            Probe::NoAnswer(NoAnswer::Refused | NoAnswer::ConnectTimedOut) => None,
            probe if probe.answered() => bail!(
                "gxserver at {BASE_URL} {} to the token in {}, so it cannot be stopped from here. Restart it from the app instead.",
                probe.describe(),
                token_file.display()
            ),
            probe => bail!(
                "gxserver at {BASE_URL} {} for {} s; it may still be busy. Try again in a moment, or restart it from the app.",
                probe.describe(),
                gxserver::PATIENT_PROBE.as_secs()
            ),
        },
    };
    let token = token.map(|(token, _)| token);
    if let Some(before) = &before {
        if gxserver::build_identity_of(before) == expected {
            step(&format!(
                "gxserver pid {} already runs this build; nothing to install.",
                pid_of(before)
            ));
            return Ok(0);
        }
    }

    step("Signing the gxserver package...");
    let _ = fs::remove_dir_all(&stage_dir);
    fs::create_dir_all(stage_dir.parent().expect("stage dir has a parent"))?;
    // `cp -pR` keeps timestamps and modes, like the old cpSync with preserveTimestamps.
    run_checked(
        Command::new("/bin/cp")
            .arg("-pR")
            .arg(&runtime_dir)
            .arg(&stage_dir),
    )?;
    // gxserver is the main executable of GhostexServer.app, behind the `bin/gxserver` and `bin/ghostex` links, so that bundle is the only code to sign, the way releases sign it (CDXC:RemotePairing 2026-11-04 in apps/desktop/scripts/codesign-gpui-app.sh).
    let bundle_path = stage_dir.join("GhostexServer.app");
    run_checked(Command::new("/usr/bin/xattr").arg("-cr").arg(&bundle_path))?;
    let mut sign = Command::new("codesign");
    if sign_identity == "-" {
        sign.args(["--force", "--sign", "-"]);
    } else {
        sign.args(["--force", "--options", "runtime"]);
        if !timestamp_flag.is_empty() {
            sign.arg(&timestamp_flag);
        }
        sign.args(["--sign", &sign_identity]);
    }
    run_checked(sign.arg(&bundle_path))?;

    step(&format!("Installing into {INSTALLED_APP}..."));
    // rsync writes each file beside its target and renames it, so the daemon still running from the old binary keeps its inode.
    run_checked(
        util::command("rsync")
            .args(INSTALL_RSYNC_ARGS)
            .arg(format!("{}/", stage_dir.display()))
            .arg(format!("{}/", installed_dir.display())),
    )?;
    link_installed_cli(&installed_cli)?;

    let launchd_target = format!("gui/{}/{LAUNCHD_LABEL}", uid());
    let job = launchd_job(&launchd_target);
    if let Some(program) = &job.program {
        if Path::new(program) != installed_dir.join("bin/gxserver") {
            bail!("launchd job {LAUNCHD_LABEL} runs {program}, not the installed app's gxserver. Restart it from the app instead.");
        }
    }
    if let (Some(before), Some(token)) = (&before, &token) {
        step(&format!("Stopping gxserver pid {}...", pid_of(before)));
        gxserver::request_stop(BASE_URL, token);
        let deadline = Instant::now() + Duration::from_secs(15);
        // Stopped means the port refuses connections: a 401 or a silent port is still the old server.
        let still_running = || {
            !matches!(
                gxserver::probe(BASE_URL, token, Duration::from_millis(500)),
                Probe::NoAnswer(NoAnswer::Refused | NoAnswer::ConnectTimedOut)
            ) || launchd_job(&launchd_target).pid.is_some()
        };
        while Instant::now() < deadline && still_running() {
            sleep_ms(200);
        }
        if still_running() {
            end_unstoppable_gxserver(before, &launchd_target);
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline && still_running() {
                sleep_ms(200);
            }
            if still_running() {
                bail!(
                    "the old gxserver did not stop within 15s, and ending it did not work either."
                );
            }
        }
    }

    step("Starting gxserver...");
    if !launchd_job(&launchd_target).loaded {
        let plist = home_dir()
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist"));
        if !plist.exists() {
            bail!(
                "{} is missing. Open Ghostex once so it registers its gxserver job.",
                plist.display()
            );
        }
        run_checked(
            Command::new("launchctl")
                .arg("bootstrap")
                .arg(format!("gui/{}", uid()))
                .arg(&plist),
        )?;
    }
    run_checked(Command::new("launchctl").args(["kickstart", &launchd_target]))?;
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut after = None;
    while Instant::now() < deadline {
        after = gxserver::read_token()
            .and_then(|token| gxserver::health(BASE_URL, &token, Duration::from_millis(500)));
        if after
            .as_ref()
            .is_some_and(|health| gxserver::build_identity_of(health) == expected)
        {
            break;
        }
        sleep_ms(250);
    }
    let Some(after) = after
        .as_ref()
        .filter(|health| gxserver::build_identity_of(health) == expected)
    else {
        let running = after
            .map(|health| format!("running {}", gxserver::build_identity_of(&health)))
            .unwrap_or_else(|| "unreachable".into());
        bail!("gxserver did not come back with the new build ({running}). See ~/.local/state/ghostex/logs/gxserver/launch.log.");
    };
    step(&format!("gxserver pid {} runs {expected}.", pid_of(after)));
    println!("The app keeps running; its outer code signature is stale until the next `cargo xtask start`.");
    Ok(0)
}

/// CDXC:ServerDaemon 2026-10-17 WHY:
/// A gxserver that outlived its stop (2026-10-17: a held request kept the drain waiting, see server/src/server/shutdown_bounds.rs) made this command give up before starting the new build, which left the computer with no working gxserver until someone killed the old one by hand. The old process is ended instead, like the desktop's restart does; the zmx sessions do not depend on it and keep running.
fn end_unstoppable_gxserver(before: &serde_json::Value, launchd_target: &str) {
    let mut pids: Vec<i32> = Vec::new();
    for pid in [
        before.get("pid").and_then(|pid| pid.as_i64()),
        launchd_job(launchd_target)
            .pid
            .and_then(|pid| pid.parse::<i64>().ok()),
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(pid) = i32::try_from(pid) {
            if pid > 1 && !pids.contains(&pid) && is_gxserver_process(pid) {
                pids.push(pid);
            }
        }
    }
    for pid in pids {
        step(&format!(
            "gxserver pid {pid} did not stop within 15s; ending it."
        ));
        #[cfg(unix)]
        util::kill_pid(pid, true);
    }
}

/// Whether `pid` still runs a gxserver binary, so a recycled pid is never signalled.
fn is_gxserver_process(pid: i32) -> bool {
    output(Command::new("/bin/ps").args(["-p", &pid.to_string(), "-o", "comm="]))
        .ok()
        .filter(|out| out.status.success())
        .is_some_and(|out| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .rsplit('/')
                .next()
                .is_some_and(|name| name == "gxserver")
        })
}

fn step(message: &str) {
    println!("==> {message}");
}

fn pid_of(health: &serde_json::Value) -> String {
    health
        .get("pid")
        .map(|pid| pid.to_string())
        .unwrap_or_default()
}

/// Points the installed CLI/ghostex at the gxserver just installed; an app installed before gxserver and the CLI became one program still has the separate CLI binary there. The rename swaps it in one step, so a hook that runs `ghostex` meanwhile never finds it missing.
fn link_installed_cli(installed_cli: &Path) -> Res {
    #[cfg(unix)]
    {
        let staged =
            installed_cli.with_file_name(format!(".ghostex.start-server-{}", std::process::id()));
        let _ = fs::remove_file(&staged);
        std::os::unix::fs::symlink("../Web/gxserver/bin/gxserver", &staged)?;
        fs::rename(&staged, installed_cli)?;
    }
    #[cfg(not(unix))]
    let _ = installed_cli;
    Ok(())
}

fn run_checked(command: &mut Command) -> Res {
    let out = output(command)?;
    if !out.status.success() {
        bail!(
            "{} failed:\n{}{}",
            util::describe(command),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// The `zmx` and `wire_generation` lines of `zmx version`.
fn zmx_version(package_dir: &Path) -> Res<String> {
    let mut version = Command::new(package_dir.join("bin/zmx"));
    version.arg("version");
    let out = output(&mut version)?;
    if !out.status.success() {
        bail!(
            "{} failed:\n{}",
            util::describe(&version),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| {
            ["zmx", "wire_generation"].iter().any(|key| {
                line.strip_prefix(key)
                    .is_some_and(|rest| rest.starts_with(char::is_whitespace))
            })
        })
        .collect::<Vec<_>>()
        .join(", "))
}

/// The `WIRE_GENERATION` the zmx source declares in `ipc.zig`.
///
/// CDXC:ZmxWireGeneration 2026-10-24 WHY:
/// gxserver writes some zmx frames itself (screen capture, the chat claim), so it is built for the wire generation of the zmx source beside it. The gxserver-only build reuses the zmx staged by the last full start, so comparing staged with installed misses a zmx source whose generation moved, and the new gxserver would send the new framing to every live daemon of the old one, which drops those frames without an error.
fn source_zmx_wire_generation() -> Res<String> {
    let path = root().join(".dependencies/zmx/src/ipc.zig");
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    match source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("pub const WIRE_GENERATION: u32 = ")?
            .strip_suffix(';')
    }) {
        Some(generation) => Ok(generation.to_string()),
        None => bail!("{} declares no WIRE_GENERATION.", path.display()),
    }
}

struct LaunchdJob {
    loaded: bool,
    pid: Option<String>,
    program: Option<String>,
}

/// Only the first `pid` and `program` lines of `launchctl print` belong to the job itself.
fn launchd_job(target: &str) -> LaunchdJob {
    let Ok(out) = output(Command::new("launchctl").args(["print", target])) else {
        return LaunchdJob {
            loaded: false,
            pid: None,
            program: None,
        };
    };
    if !out.status.success() {
        return LaunchdJob {
            loaded: false,
            pid: None,
            program: None,
        };
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let field = |name: &str| {
        text.lines().find_map(|line| {
            line.trim_start()
                .strip_prefix(&format!("{name} = "))
                .map(|v| v.trim().to_string())
        })
    };
    LaunchdJob {
        loaded: true,
        pid: field("pid").filter(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())),
        program: field("program"),
    }
}

fn uid() -> u32 {
    #[cfg(unix)]
    {
        unsafe { libc::getuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}
