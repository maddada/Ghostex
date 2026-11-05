//! Runs a rebuild from a shell in an ended macOS login session: the same `cargo xtask rebuild` again, as a one-shot launchd job that launchd starts in the current login, with its output streamed back here and its exit code passed on.
//!
//! CDXC:Build 2026-11-05 WHY:
//! User (approved): a rebuild from a coordinator shell that outlived a logout should "run itself in a process that belongs to the current GUI login session" instead of refusing, with no Terminal or Ghostex window, sudo or prompt. A process keeps the audit session it started in, so nothing this shell spawns itself can reach the new login's keychain; only launchd can start a process in it. The job goes into `gui/<uid>`, the desktop login, because codesign there reaches the keychain; a `user/<uid>` job runs in a Background session and fails with errSecInternalComponent (tried 2026-11-05). This shell may not reach `gui/<uid>` ("141: Reentrancy avoided"), but it can reach `user/<uid>` (gxserver's zmx launcher falls back to it from such a shell, CDXC:Zmx 2026-11-02), and a `user/<uid>` job can reach `gui/<uid>`, so when the direct `launchctl bootstrap gui/<uid>` fails a one-shot `user/<uid>` job runs that bootstrap instead. `launchctl asuser` keeps the caller's audit session, and gxserver has no route that runs a command without opening a session in the user's window, so neither is used. The job is the full rebuild, so it makes its own login and codesign checks before building and refuses like today if it still can't sign; `GHOSTEX_REBUILD_RELAYED` stops it from relaying again. It unloads itself when it ends (this shell may not be able to), and its files (plists, output, pid, exit code) sit beside the numbered logs as `relay-<pid>.*`, which the log numbering skips, and are removed afterwards.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::util::sleep_ms;

/// Set in the job's environment, so a job that still finds its login session ended refuses instead of relaying again.
pub const RELAYED_ENV: &str = "GHOSTEX_REBUILD_RELAYED";
/// How long launchd gets to start the job once it accepted it.
const START_PATIENCE: Duration = Duration::from_secs(30);
const POLL_MS: u64 = 300;
/// Variables that name this shell's ended login session rather than describe the build.
const SESSION_VARIABLES: &[&str] = &[
    "SECURITYSESSIONID",
    "LaunchInstanceID",
    "XPC_SERVICE_NAME",
    "XPC_FLAGS",
];

/// A job loaded into `gui/<uid>`; dropping it unloads it (and the `user/<uid>` job that loaded it, if any) and removes its files.
pub struct Job {
    /// How it was loaded: `gui/<uid>`, or `gui/<uid> through user/<uid>`.
    pub route: String,
    target: String,
    hop_target: Option<String>,
    base: PathBuf,
}

impl Job {
    /// Loads `program args`, run in `cwd`, into `gui/<uid>` (through a `user/<uid>` job when this shell can't reach it) and waits until it has started.
    pub fn start(program: &Path, args: &[String], cwd: &Path, dir: &Path) -> Result<Job, String> {
        Self::start_via(program, args, cwd, dir, false)
    }

    fn start_via(
        program: &Path,
        args: &[String],
        cwd: &Path,
        dir: &Path,
        always_hop: bool,
    ) -> Result<Job, String> {
        fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
        let uid = unsafe { libc::getuid() };
        let pid = std::process::id();
        let label = format!("dev.ghostex.xtask-rebuild-relay.{pid}");
        let gui = format!("gui/{uid}");
        let user = format!("user/{uid}");
        let mut job = Job {
            route: gui.clone(),
            target: format!("{gui}/{label}"),
            hop_target: None,
            base: dir.join(format!("relay-{pid}")),
        };
        job.remove_files();
        let plist = job.file("plist");
        fs::write(
            &plist,
            job_plist(&label, &job.base, &job.target, program, args, cwd),
        )
        .map_err(|error| format!("write {}: {error}", plist.display()))?;
        let plist = plist.to_string_lossy().into_owned();
        let direct = if always_hop {
            Err("skipped".to_string())
        } else {
            launchctl(&["bootstrap", &gui, &plist])
        };
        if let Err(direct) = direct {
            let hop_label = format!("{label}.hop");
            let hop_plist = job.file("hop.plist");
            fs::write(
                &hop_plist,
                hop_plist_text(&hop_label, &job.file("hop.out"), &gui, &plist),
            )
            .map_err(|error| format!("write {}: {error}", hop_plist.display()))?;
            job.hop_target = Some(format!("{user}/{hop_label}"));
            job.route = format!("{gui} through {user}");
            launchctl(&["bootstrap", &user, &hop_plist.to_string_lossy()]).map_err(|hop| {
                format!("launchctl bootstrap {gui}: {direct}; launchctl bootstrap {user}: {hop}")
            })?;
        }
        let began = Instant::now();
        while !job.file("pid").exists() && began.elapsed() < START_PATIENCE {
            sleep_ms(POLL_MS);
        }
        if job.file("pid").exists() {
            return Ok(job);
        }
        let hop_said = fs::read_to_string(job.file("hop.out")).unwrap_or_default();
        Err(format!(
            "launchd didn't start the job in {} within {}s{}",
            job.route,
            START_PATIENCE.as_secs(),
            match hop_said.trim() {
                "" => String::new(),
                said => format!(" ({said})"),
            }
        ))
    }

    /// Copies the job's output to `out` as it is written, and returns its exit code once it ends.
    pub fn wait(&self, out: &mut dyn Write) -> Result<i32, String> {
        let output = self.file("out");
        let mut offset = 0;
        loop {
            // The exit file is read before the last copy, so no output written before it is missed.
            let exit = fs::read_to_string(self.file("exit")).ok();
            offset = copy_new(&output, offset, out);
            if let Some(code) = exit {
                return code
                    .trim()
                    .parse()
                    .map_err(|_| format!("the job wrote exit code `{}`", code.trim()));
            }
            if !self.running() && !self.file("exit").exists() {
                copy_new(&output, offset, out);
                return Err(format!(
                    "the job in {} ended without an exit code",
                    self.route
                ));
            }
            sleep_ms(POLL_MS);
        }
    }

    fn running(&self) -> bool {
        fs::read_to_string(self.file("pid"))
            .ok()
            .and_then(|pid| pid.trim().parse::<libc::pid_t>().ok())
            .is_some_and(|pid| unsafe { libc::kill(pid, 0) == 0 })
    }

    fn file(&self, extension: &str) -> PathBuf {
        self.base.with_extension(extension)
    }

    fn remove_files(&self) {
        for extension in [
            "plist",
            "out",
            "pid",
            "exit",
            "exit.part",
            "hop.plist",
            "hop.out",
        ] {
            let _ = fs::remove_file(self.file(extension));
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // The job unloads itself when it ends; this covers one stopped early.
        for target in std::iter::once(&self.target).chain(&self.hop_target) {
            let _ = launchctl(&["bootout", target]);
        }
        self.remove_files();
    }
}

/// Appends what `path` gained since `offset` to `out`; returns the new offset.
fn copy_new(path: &Path, offset: u64, out: &mut dyn Write) -> u64 {
    let Ok(mut file) = fs::File::open(path) else {
        return offset;
    };
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(offset)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return offset;
    }
    let _ = out.write_all(&bytes);
    let _ = out.flush();
    offset + bytes.len() as u64
}

fn launchctl(args: &[&str]) -> Result<(), String> {
    let out = Command::new("/bin/launchctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    Err(said
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no output")
        .to_string())
}

const PLIST_HEAD: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>";
const PLIST_ONE_SHOT: &str = "<key>LimitLoadToSessionType</key><array><string>Aqua</string><string>Background</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><false/>";

/// A one-shot job whose shell records its pid, runs `program args`, records the exit code (renamed into place, so a reader never sees half of it) and unloads the job.
fn job_plist(
    label: &str,
    base: &Path,
    target: &str,
    program: &Path,
    args: &[String],
    cwd: &Path,
) -> String {
    let script = r#"echo $$ > "$0.pid"; target=$1; shift; "$@"; echo $? > "$0.exit.part"; mv "$0.exit.part" "$0.exit"; exec /bin/launchctl bootout "$target""#;
    let arguments = strings(
        [
            "/bin/sh",
            "-c",
            script,
            &base.to_string_lossy(),
            target,
            &program.to_string_lossy(),
        ]
        .into_iter()
        .map(str::to_string)
        .chain(args.iter().cloned()),
    );
    let environment: String = std::env::vars()
        .filter(|(key, _)| !SESSION_VARIABLES.contains(&key.as_str()) && !key.starts_with("__CF"))
        .chain([(RELAYED_ENV.to_string(), "1".to_string())])
        .map(|(key, value)| {
            format!(
                "<key>{}</key><string>{}</string>",
                xml_escape(&key),
                xml_escape(&value)
            )
        })
        .collect();
    let output = xml_escape(&base.with_extension("out").to_string_lossy());
    format!(
        "{PLIST_HEAD}<key>Label</key><string>{}</string><key>ProgramArguments</key><array>{arguments}</array><key>WorkingDirectory</key><string>{}</string><key>EnvironmentVariables</key><dict>{environment}</dict>{PLIST_ONE_SHOT}<key>ProcessType</key><string>Interactive</string><key>StandardOutPath</key><string>{output}</string><key>StandardErrorPath</key><string>{output}</string></dict></plist>\n",
        xml_escape(label),
        xml_escape(&cwd.to_string_lossy()),
    )
}

/// A one-shot `user/<uid>` job that loads `plist` into `domain`.
fn hop_plist_text(label: &str, output: &Path, domain: &str, plist: &str) -> String {
    let arguments = strings(
        ["/bin/launchctl", "bootstrap", domain, plist]
            .into_iter()
            .map(str::to_string),
    );
    let output = xml_escape(&output.to_string_lossy());
    format!(
        "{PLIST_HEAD}<key>Label</key><string>{}</string><key>ProgramArguments</key><array>{arguments}</array>{PLIST_ONE_SHOT}<key>StandardOutPath</key><string>{output}</string><key>StandardErrorPath</key><string>{output}</string></dict></plist>\n",
        xml_escape(label),
    )
}

fn strings(items: impl Iterator<Item = String>) -> String {
    items
        .map(|item| format!("<string>{}</string>", xml_escape(&item)))
        .collect()
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_runs_the_program_through_the_recording_shell() {
        let plist = job_plist(
            "dev.ghostex.test",
            Path::new("/logs/relay-1"),
            "gui/501/dev.ghostex.test",
            Path::new("/repo/target/debug/xtask"),
            &["rebuild".into(), "--wait=5".into(), "a<b&c".into()],
            Path::new("/repo"),
        );
        assert!(plist.contains("<string>/bin/sh</string><string>-c</string>"));
        assert!(plist.contains("<string>/logs/relay-1</string><string>gui/501/dev.ghostex.test</string><string>/repo/target/debug/xtask</string><string>rebuild</string><string>--wait=5</string><string>a&lt;b&amp;c</string></array>"));
        assert!(plist.contains("<key>GHOSTEX_REBUILD_RELAYED</key><string>1</string>"));
        assert!(plist.contains("<key>StandardOutPath</key><string>/logs/relay-1.out</string>"));
        assert!(!plist.contains("<key>SECURITYSESSIONID</key>"));
    }

    /// Loads real launchd jobs (`cargo test -p xtask -- --ignored relays`), directly and through `user/<uid>`: each runs in the desktop login (Aqua), its output and exit code come back, it unloads itself, and its files are gone afterwards.
    #[test]
    #[ignore]
    fn relays_a_launchd_jobs_output_and_exit_code() {
        let dir =
            crate::util::home_dir().join(".local/share/agent-work/Ghostex/rebuild-relay-test");
        let script = "echo to-stdout; echo to-stderr >&2; launchctl managername; exit 7";
        for always_hop in [false, true] {
            let job = Job::start_via(
                Path::new("/bin/sh"),
                &["-c".into(), script.into()],
                &dir,
                &dir,
                always_hop,
            )
            .unwrap();
            let mut out = Vec::new();
            let code = job.wait(&mut out).unwrap();
            let text = String::from_utf8_lossy(&out).into_owned();
            println!("{}:\n{text}", job.route);
            sleep_ms(1000);
            assert!(
                launchctl(&["print", &job.target]).is_err(),
                "{} is still loaded",
                job.target
            );
            drop(job);
            assert_eq!(code, 7);
            for line in ["to-stdout", "to-stderr", "Aqua"] {
                assert!(text.contains(line), "missing {line}");
            }
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        }
        fs::remove_dir(&dir).unwrap();
    }
}
