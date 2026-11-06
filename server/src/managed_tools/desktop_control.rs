//! Fast Computer & Browser Use (Trycua's `cua-driver`) and SpaceO as managed tools, so a remote Mac
//! installs, updates, reinstalls and uninstalls them through its own gxserver like the other tools
//! (Settings > Integrations with that Mac picked, and the phone's machine screen). Both run only on
//! macOS (SpaceO only on Apple Silicon with macOS 14 or later), and both need Accessibility and
//! Screen Recording, which only someone at that Mac can allow, so an installed tool without them
//! says so (`permissionNote`) instead of looking finished.
//!
//! CDXC:ManagedTools 2026-10-26 DECISION:
//! User (Q610), asked whether Fast Computer & Browser Use and SpaceO should be installable on a remote Mac from the machine picker: "Allow it on a remote Mac." This supersedes the 2026-09-29 reason in apps/desktop/src/app/helpers/os_cli/cli_settings_actions.rs for keeping them out of gxserver ("Trycua controls the desktop it runs on"): a Mac's own gxserver runs the official installers here. They are never offered for Linux or Windows, and only the `list` that asks for them (`includeDesktopControl`) shows them, since this computer's Settings already draws its own Desktop control rows.
//! SEE-ALSO: apps/desktop/src/app/helpers/os_cli/cli_settings_actions.rs and spaceo_job.rs (the same official commands, which this computer's own Desktop control rows run as desktop jobs; change both together), apps/desktop/src/app/window/settings_modal/tabs/integrations/remote_machine.rs, apps/mobile/app (the machine screen).
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde_json::Value;

use super::{
    jobs::Log,
    platform::{self, Os},
    run,
    tools::{self, Source, Status, ToolId},
};

/// The official one-liner, which the status plan shows.
const TRYCUA_INSTALL: &str = "/bin/bash -c \"$(curl -fsSL https://cua.ai/driver/install.sh)\"";
/// What the job runs: the official one-liner, or the same installer from GitHub when `cua.ai`
/// does not answer.
///
/// CDXC:ManagedTools 2026-11-07 FIX: Gregg Housh (Discord, 2026-11-06): "On multiple networks and with multiple DNS providers i've found cua.ai wont resolve to an IP ... the normal install command just fails", and "It also happens on one of my digitalocean droplets, and not on another one." The user: "Will switch it to make node first". Node cannot be that first step: Cua publishes `@trycua/cua-driver` on npm only as a Node SDK (no `cua-driver` command, no CuaDriver.app, no `bin` in any of its versions), so an npm install gives agents nothing to run. What `cua.ai` serves is only the two installer scripts; the driver itself downloads from github.com/trycua/cua releases, and the same scripts are in that repository. So when `cua.ai` fails to answer, the job runs `_install-rust.sh` (and its optional `_install-common.sh`, which stops a running daemon before an upgrade) straight from raw.githubusercontent.com, side by side in a temporary folder so the installer finds the sibling on disk. Reporter reply and the open question about a true Node route: ~/.local/share/agent-work/Ghostex/discord-fixes/cua-node.md.
/// SEE-ALSO: apps/desktop/src/app/helpers/os_cli/cli_settings_actions.rs (`GPUI_TRYCUA_INSTALL_JOB_COMMAND`, the same script; change both together).
const TRYCUA_INSTALL_JOB: &str = r#"{
if cua_installer="$(curl -fsSL --connect-timeout 10 https://cua.ai/driver/install.sh)"; then
  /bin/bash -c "$cua_installer"
else
  echo "cua.ai did not answer, so the installer comes from GitHub (trycua/cua) instead."
  cua_dir="$(mktemp -d)" && curl -fsSL -o "$cua_dir/_install-rust.sh" https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/_install-rust.sh && { curl -fsSL -o "$cua_dir/_install-common.sh" https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/_install-common.sh || true; } && /bin/bash "$cua_dir/_install-rust.sh"
  cua_status=$?
  rm -rf "$cua_dir"
  (exit "$cua_status")
fi
}"#;
/// `--keep-tcc` keeps the Accessibility and Screen Recording grants for a later reinstall
/// (CDXC:Extensions 2026-09-26). Falls back to GitHub's copy of the (standalone) uninstaller like
/// the installer (CDXC:ManagedTools 2026-11-07 above).
const TRYCUA_UNINSTALL: &str = r#"if cua_uninstaller="$(curl -fsSL --connect-timeout 10 https://cua.ai/driver/uninstall.sh)"; then
  /bin/bash -c "$cua_uninstaller" -- --keep-tcc
else
  echo "cua.ai did not answer, so the uninstaller comes from GitHub (trycua/cua) instead."
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/uninstall.sh)" -- --keep-tcc
fi"#;
const TRYCUA_START: &str = "/usr/bin/open -n -g -a CuaDriver --args serve";
const TRYCUA_APP_BINARY: &str = "/Applications/CuaDriver.app/Contents/MacOS/cua-driver";
const SPACEO_INSTALL: &str = "curl -fsSL https://raw.githubusercontent.com/ParthJadhav/SpaceO/main/install.sh | bash -s -- --no-clients";
const SPACEO_UNINSTALL: &str = "curl -fsSL https://raw.githubusercontent.com/ParthJadhav/SpaceO/main/install.sh | bash -s -- --uninstall --yes";
/// SpaceO runs as its own LaunchAgent so its grants belong to one stable identity
/// (CDXC:ManagedTools 2026-10-01 in spaceo_job.rs).
const SPACEO_SERVICE: &str = "spaceo=\"$HOME/.local/bin/spaceo\"; if [ -f \"$HOME/Library/LaunchAgents/com.spaceo.daemon.plist\" ]; then \"$spaceo\" daemon restart --operator; else \"$spaceo\" daemon stop --operator >/dev/null 2>&1 || true; \"$spaceo\" daemon install --yes; fi";
const SPACEO_LATEST_RELEASE_URL: &str = "https://github.com/ParthJadhav/SpaceO/releases/latest";
const JOB_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const SKILLS_TIMEOUT: Duration = Duration::from_secs(180);

pub(crate) fn is_desktop_control(tool: ToolId) -> bool {
    matches!(tool, ToolId::Trycua | ToolId::Spaceo)
}

/// Which grants a tool still lacks: `None` when they could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Grants {
    pub accessibility: Option<bool>,
    pub screen_recording: Option<bool>,
}

/// What an installed tool's row says while it can't work yet; `None` once both grants are in.
pub(crate) fn permission_note(grants: &Grants) -> Option<String> {
    let missing: Vec<&str> = [
        ("Accessibility", grants.accessibility),
        ("Screen Recording", grants.screen_recording),
    ]
    .into_iter()
    .filter(|(_, granted)| *granted != Some(true))
    .map(|(name, _)| name)
    .collect();
    if missing.is_empty() {
        return None;
    }
    let unknown = grants.accessibility.is_none() && grants.screen_recording.is_none();
    Some(format!(
        "{} Only someone at that Mac can allow {}: in Ghostex's Settings > Integrations there, or System Settings > Privacy & Security.",
        if unknown {
            "Installed, but Ghostex couldn't check its permissions yet."
        } else {
            "Installed, but it can't control that Mac yet."
        },
        missing.join(" and ")
    ))
}

/// Runs `program args` with a deadline and returns its stdout whatever its exit code (`spaceo
/// doctor` exits 1 when it found something to fix, with its JSON complete).
fn probe(program: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    probed(program, args, timeout).ok()
}

/// `probe`, telling a program that ran past `timeout` (stopped with everything it started) apart.
fn probed(program: &Path, args: &[&str], timeout: Duration) -> Result<String, run::Unfinished> {
    let mut command = crate::platform::process::background_command(program);
    command.args(args).env("PATH", run::job_path(&[]));
    run::output_within(command, timeout).map(|output| output.stdout)
}

/// The JSON object in a command's output (Trycua may print a notice before it).
pub(crate) fn json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (start <= end)
        .then(|| serde_json::from_str(&text[start..=end]).ok())
        .flatten()
}

fn spaceo_supported() -> Result<(), String> {
    let reason = || "SpaceO runs only on Apple Silicon Macs with macOS 14 or later.".to_string();
    if platform::os() != Os::Mac || platform::arch() != Some(platform::Arch::Arm64) {
        return Err(reason());
    }
    let major = crate::platform::process::background_command("/usr/bin/sw_vers")
        .arg("-productVersion")
        .stdin(Stdio::null())
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .split('.')
                .next()?
                .parse::<u32>()
                .ok()
        });
    if major.is_some_and(|major| major < 14) {
        return Err(reason());
    }
    Ok(())
}

pub(crate) fn trycua_executable(home: &Path) -> Option<PathBuf> {
    tools::locate("cua-driver", home)
        .map(|(path, _)| path)
        .or_else(|| {
            let app = PathBuf::from(TRYCUA_APP_BINARY);
            app.is_file().then_some(app)
        })
}

pub(crate) fn spaceo_executable(home: &Path) -> Option<PathBuf> {
    tools::locate("spaceo", home)
        .map(|(path, _)| path)
        .or_else(|| {
            let installed = home.join(".local/bin/spaceo");
            installed.is_file().then_some(installed)
        })
}

/// `cua-driver check_permissions {"prompt":false}`: the driver answers for its own app's grants.
fn trycua_grants(executable: &Path) -> Grants {
    let payload = probe(
        executable,
        &["check_permissions", r#"{"prompt":false}"#],
        Duration::from_secs(5),
    )
    .and_then(|text| json_object(&text));
    let granted = |key: &str| payload.as_ref()?.get(key)?.as_bool();
    Grants {
        accessibility: granted("accessibility"),
        screen_recording: granted("screen_recording"),
    }
}

/// The grants of SpaceO's running daemon, read from `spaceo doctor --json`'s `daemon` section.
///
/// CDXC:ManagedTools 2026-10-26 WHY: gxserver can't spawn `spaceo` as its own privacy identity the way the desktop does (disclaimed_spawn.rs), so a check `spaceo` ran itself would report gxserver's grants. The LaunchAgent daemon is SpaceO's own identity, so only its answer is used; with the daemon not running the grants are unknown.
pub(crate) fn spaceo_grants_from_doctor(doctor: Option<&Value>) -> Grants {
    let daemon = doctor.and_then(|doctor| doctor.get("daemon"));
    let running = daemon
        .and_then(|daemon| daemon.get("running"))
        .and_then(Value::as_bool)
        == Some(true);
    let granted = |key: &str| running.then(|| daemon?.get(key)?.as_bool()).flatten();
    Grants {
        accessibility: granted("accessibilityGranted"),
        screen_recording: granted("screenRecordingGranted"),
    }
}

/// The tool's status, and (installed) the grants it still needs.
pub(crate) fn status(tool: ToolId, home: &Path) -> (Status, Option<Grants>) {
    let trycua = tool == ToolId::Trycua;
    let supported = if platform::os() != Os::Mac {
        Err(if trycua {
            "Ghostex installs Fast Computer & Browser Use only on a Mac.".to_string()
        } else {
            "SpaceO runs only on Apple Silicon Macs with macOS 14 or later.".to_string()
        })
    } else if trycua {
        Ok(())
    } else {
        spaceo_supported()
    };
    if let Err(reason) = supported {
        return (Status::unsupported(reason), None);
    }
    let plan = if trycua {
        format!("Runs Trycua's official installer ({TRYCUA_INSTALL}) in the background (from GitHub instead when cua.ai does not answer), starts CuaDriver, then installs its cua-driver skill and Ghostex Computer Use. Someone at that Mac then allows Accessibility and Screen Recording.")
    } else {
        format!("Runs SpaceO's official installer from GitHub ({SPACEO_INSTALL}) in the background, keeps SpaceO running as its own background service, then installs Ghostex SpaceO. Someone at that Mac then allows Accessibility and Screen Recording.")
    };
    let mut status = Status::new(plan);
    let executable = if trycua {
        trycua_executable(home)
    } else {
        spaceo_executable(home)
    };
    let Some(executable) = executable else {
        status.operations = vec!["install"];
        return (status, None);
    };
    status.source = Some(Source::System);
    status.operations = vec!["update", "reinstall", "uninstall"];
    // A tool that doesn't say its version isn't asked for its grants either (GitHub #336).
    let grants = if trycua {
        status.read_version(&executable, &["--version"]);
        status
            .not_answering
            .is_none()
            .then(|| trycua_grants(&executable))
    } else {
        let args = ["version", "--json"];
        match probed(&executable, &args, Duration::from_secs(3)) {
            Ok(text) => {
                status.version = json_object(&text)
                    .and_then(|value| value.get("version")?.as_str().map(str::to_string))
                    .or_else(|| tools::version_in(&text));
                let doctor = probe(&executable, &["doctor", "--json"], Duration::from_secs(10))
                    .and_then(|text| json_object(&text));
                Some(spaceo_grants_from_doctor(doctor.as_ref()))
            }
            Err(run::Unfinished::TimedOut(limit)) => {
                status.not_answering = Some(tools::not_answering(&executable, &args, limit));
                None
            }
            Err(run::Unfinished::Start) => Some(spaceo_grants_from_doctor(None)),
        }
    };
    status.executable = Some(executable);
    (status, grants)
}

/// The newest release: Trycua's own update check, SpaceO's latest GitHub release tag.
pub(crate) fn latest_version(
    tool: ToolId,
    home: &Path,
    fresh: bool,
) -> Option<Result<String, String>> {
    if tool == ToolId::Trycua {
        let executable = trycua_executable(home)?;
        let args: &[&str] = if fresh {
            &["check-update", "--json", "--no-cache"]
        } else {
            &["check-update", "--json"]
        };
        let payload =
            probe(&executable, args, Duration::from_secs(15)).and_then(|text| json_object(&text));
        return Some(
            payload
                .as_ref()
                .and_then(|payload| payload.get("latest_version")?.as_str())
                .map(str::to_string)
                .ok_or_else(|| "Trycua's update check did not answer.".to_string()),
        );
    }
    let effective = probe(
        Path::new("/usr/bin/curl"),
        &[
            "-fsSL",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--connect-timeout",
            "10",
            "--max-time",
            "15",
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
            SPACEO_LATEST_RELEASE_URL,
        ],
        Duration::from_secs(20),
    );
    Some(
        effective
            .as_deref()
            .and_then(spaceo_tag)
            .ok_or_else(|| "GitHub did not answer with SpaceO's latest release.".to_string()),
    )
}

/// `…/releases/tag/v1.2.3` → `1.2.3`.
pub(crate) fn spaceo_tag(url: &str) -> Option<String> {
    let tag = url.trim().rsplit('/').next()?.strip_prefix('v')?;
    (tag.split('.').count() == 3
        && tag
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())))
    .then(|| tag.to_string())
}

/// The shell script one click runs.
pub(crate) fn script(tool: ToolId, operation: &str, home: &Path) -> String {
    match (tool, operation) {
        (ToolId::Trycua, "uninstall") => TRYCUA_UNINSTALL.to_string(),
        (ToolId::Trycua, "update") => match trycua_executable(home) {
            Some(executable) => {
                let quoted = format!("'{}'", executable.to_string_lossy().replace('\'', r"'\''"));
                format!(
                    "{quoted} check-update --no-cache && {quoted} update --apply && {TRYCUA_START}"
                )
            }
            None => format!("{TRYCUA_INSTALL_JOB} && {TRYCUA_START}"),
        },
        (ToolId::Trycua, _) => format!("{TRYCUA_INSTALL_JOB} && {TRYCUA_START}"),
        (_, "uninstall") => SPACEO_UNINSTALL.to_string(),
        _ => format!("set -o pipefail; {SPACEO_INSTALL} && {{ {SPACEO_SERVICE}; }}"),
    }
}

/// Runs the official installer, updater or uninstaller.
pub(crate) fn run_operation(
    tool: ToolId,
    operation: &str,
    home: &Path,
    log: &Log,
) -> Result<(), String> {
    run::run(
        "/bin/bash",
        &["-c", &script(tool, operation, home)],
        &[],
        &[],
        log,
        JOB_TIMEOUT,
    )?;
    if tool == ToolId::Trycua && operation != "uninstall" {
        // Trycua's own skill pack follows the driver it describes; a failure only adds a line.
        if let Some(executable) = trycua_executable(home) {
            let refresh = home
                .join(".cua-driver/skills/cua-driver/SKILL.md")
                .is_file();
            let verb = if refresh { "update" } else { "install" };
            if run::run(
                &executable,
                &["skills", verb],
                &[],
                &[],
                log,
                SKILLS_TIMEOUT,
            )
            .is_err()
            {
                log.line("The cua-driver skill could not be installed; install it again later.");
            }
        }
    }
    Ok(())
}

/// The Ghostex skill an install adds after the tool itself.
pub(crate) fn ghostex_skill(tool: ToolId) -> &'static str {
    if tool == ToolId::Trycua {
        "ghostex-computer-use"
    } else {
        "ghostex-spaceo"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_installed_tool_without_its_grants_says_who_can_allow_them() {
        let all = Grants {
            accessibility: Some(true),
            screen_recording: Some(true),
        };
        assert_eq!(permission_note(&all), None);
        let one = Grants {
            accessibility: Some(true),
            screen_recording: Some(false),
        };
        let note = permission_note(&one).expect("note");
        assert!(note.contains("can't control that Mac yet"));
        assert!(note.contains("Only someone at that Mac can allow Screen Recording"));
        let unknown = Grants {
            accessibility: None,
            screen_recording: None,
        };
        let note = permission_note(&unknown).expect("note");
        assert!(note.contains("couldn't check"));
        assert!(note.contains("Accessibility and Screen Recording"));
    }

    #[test]
    fn spaceo_grants_count_only_while_its_own_daemon_runs() {
        let doctor = json!({ "daemon": { "running": true, "accessibilityGranted": true, "screenRecordingGranted": false } });
        assert_eq!(
            spaceo_grants_from_doctor(Some(&doctor)),
            Grants {
                accessibility: Some(true),
                screen_recording: Some(false)
            }
        );
        let stopped = json!({ "daemon": { "running": false, "accessibilityGranted": true } });
        assert_eq!(
            spaceo_grants_from_doctor(Some(&stopped)),
            Grants {
                accessibility: None,
                screen_recording: None
            }
        );
        assert_eq!(spaceo_grants_from_doctor(None).accessibility, None);
    }

    #[test]
    fn spaceo_latest_release_comes_from_the_redirect_tag() {
        assert_eq!(
            spaceo_tag("https://github.com/ParthJadhav/SpaceO/releases/tag/v0.9.12\n"),
            Some("0.9.12".to_string())
        );
        assert_eq!(
            spaceo_tag("https://github.com/ParthJadhav/SpaceO/releases"),
            None
        );
        assert_eq!(spaceo_tag("https://x/tag/vlatest"), None);
    }

    #[test]
    fn each_operation_runs_the_official_command() {
        let home = Path::new("/nonexistent-home");
        assert!(script(ToolId::Trycua, "install", home).starts_with(TRYCUA_INSTALL_JOB));
        assert!(TRYCUA_INSTALL_JOB.contains("https://cua.ai/driver/install.sh"));
        assert!(script(ToolId::Trycua, "install", home).ends_with(TRYCUA_START));
        assert_eq!(script(ToolId::Trycua, "uninstall", home), TRYCUA_UNINSTALL);
        assert!(script(ToolId::Spaceo, "install", home).contains(SPACEO_INSTALL));
        assert!(script(ToolId::Spaceo, "update", home).contains("daemon"));
        assert_eq!(script(ToolId::Spaceo, "uninstall", home), SPACEO_UNINSTALL);
        assert_eq!(
            json_object("notice\n{\"latest_version\":\"1.2.3\"}\n"),
            Some(json!({ "latest_version": "1.2.3" }))
        );
    }

    /// Runs `script` with a fake `curl` first on PATH: it fails for cua.ai when `cua_ai_up` is false,
    /// and otherwise answers every URL with a script that prints where it came from.
    #[cfg(unix)]
    fn run_with_fake_curl(script: &str, cua_ai_up: bool) -> (bool, String) {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("gx-cua-curl-{}-{cua_ai_up}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let curl = dir.join("curl");
        std::fs::write(
            &curl,
            format!(
                "#!/bin/bash\nout=\"\"; url=\"\"\nwhile [ $# -gt 0 ]; do case \"$1\" in -o) out=\"$2\"; shift 2;; -*) shift;; *) url=\"$1\"; shift;; esac; done\ncase \"$url\" in https://cua.ai/*) {} ;; esac\nbody=\"echo ran-from:$url\"\nif [ -n \"$out\" ]; then echo \"$body\" > \"$out\"; else echo \"$body\"; fi\n",
                if cua_ai_up { "true" } else { "exit 6" }
            ),
        )
        .expect("shim");
        std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let output = std::process::Command::new("/bin/bash")
            .args(["-c", script])
            .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
            .output()
            .expect("bash");
        let _ = std::fs::remove_dir_all(&dir);
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).to_string(),
        )
    }

    #[cfg(unix)]
    #[test]
    fn the_install_job_falls_back_to_github_only_when_cua_ai_does_not_answer() {
        let (ok, out) = run_with_fake_curl(TRYCUA_INSTALL_JOB, true);
        assert!(ok, "{out}");
        assert!(
            out.contains("ran-from:https://cua.ai/driver/install.sh"),
            "{out}"
        );
        assert!(!out.contains("GitHub"), "{out}");

        let (ok, out) = run_with_fake_curl(TRYCUA_INSTALL_JOB, false);
        assert!(ok, "{out}");
        assert!(out.contains("installer comes from GitHub"), "{out}");
        assert!(
            out.contains("ran-from:https://raw.githubusercontent.com/trycua/cua/main/libs/cua-driver/scripts/_install-rust.sh"),
            "{out}"
        );

        let (ok, out) = run_with_fake_curl(&format!("{TRYCUA_INSTALL_JOB} && echo started"), false);
        assert!(ok && out.contains("started"), "{out}");

        let (ok, out) = run_with_fake_curl(TRYCUA_UNINSTALL, false);
        assert!(ok, "{out}");
        assert!(out.contains("uninstall.sh"), "{out}");
    }

    #[test]
    fn neither_is_offered_off_a_mac() {
        if platform::os() == Os::Mac {
            return;
        }
        let home = Path::new("/nonexistent-home");
        for tool in [ToolId::Trycua, ToolId::Spaceo] {
            let (status, grants) = status(tool, home);
            assert!(status.supported.is_err());
            assert!(status.operations.is_empty());
            assert!(grants.is_none());
        }
    }
}
