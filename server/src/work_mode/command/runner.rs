//! Running a command ticket source: one verb per process, no shell, a cleared environment plus
//! an allowlist, JSON on stdin, a time limit and an output cap per verb. Every text that can
//! reach a user or a log has the command's credential values masked.
//!
//! CDXC:WorkMode 2026-10-29 DECISION:
//! The repository owner: "Pass ticket ids on stdin, never as an argument: they can come from branch names and `ghostex link add`, so `-x` must never read as a flag. Refuse `.bat`/`.cmd`." So argv is only the verb and `--json`, and `detail` reads its id from stdin like `list` and `create` read theirs.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use crate::paths::GxserverPaths;
use crate::platform::process::{run_bounded, Keep, RunLimits, Unfinished};

use super::*;

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;
/// How much of the end of stderr is kept: its last non-empty line is the message of a failure
/// that printed no error object.
const STDERR_TAIL: usize = 8 * KIB;
/// What a credential value shows as wherever it would surface.
const MASK: &str = "••••";
/// A credential at least this long is masked where it appears. A shorter one is too: masking
/// every `a` or `ab` would leave nothing readable, so a text that contains it is withheld whole
/// (`WITHHELD`).
const MIN_MASKED_IN_PLACE_CHARS: usize = 3;
const WITHHELD: &str = "(message hidden: it could show a credential)";
/// A refused icon's reason once a credential is set: the reason quotes the icon's text, cut, so a
/// mask could not find a long value in it.
const ICON_REFUSED: &str =
    "it was refused; the reason is hidden because it could show a credential";

/// Variables passed through from gxserver's environment when set. Everything else is left out,
/// so tokens of other tools, the Linear keys and session variables never reach the command.
const INHERITED_VARS: [&str; 16] = [
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
    "TMPDIR",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
];
#[cfg(windows)]
const INHERITED_WINDOWS_VARS: [&str; 11] = [
    "SystemRoot",
    "windir",
    "ComSpec",
    "PATHEXT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "TEMP",
    "TMP",
    "ProgramData",
    "ProgramFiles",
];

/// One process the source can be asked to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandVerb {
    Info,
    /// `list` without ids: the open tickets.
    OpenList,
    /// `list` with ids: exactly those tickets.
    ExactList,
    Detail,
    Create,
}

impl CommandVerb {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::OpenList | Self::ExactList => "list",
            Self::Detail => "detail",
            Self::Create => "create",
        }
    }

    pub(crate) fn timeout(self) -> Duration {
        Duration::from_secs(match self {
            // Settings waits on it.
            Self::Info => 10,
            // As `gh`'s list; the Work page never waits for it.
            Self::OpenList => 45,
            Self::ExactList => 30,
            // The details page waits on it.
            Self::Detail => 20,
            // As GitHub's create.
            Self::Create => 60,
        })
    }

    /// How much stdout it may answer; more fails the call instead of being cut, so a cut answer
    /// never parses as a whole one.
    pub(crate) fn stdout_cap(self) -> usize {
        match self {
            Self::Info | Self::Create => 64 * KIB,
            Self::OpenList | Self::ExactList => 4 * MIB,
            Self::Detail => MIB,
        }
    }
}

/// What the command learns from its environment besides the inherited variables. Its `Debug`
/// never prints a credential.
#[derive(Clone, Default)]
pub(crate) struct CommandEnv {
    /// The source id pinned in the workspace; `None` before the first `info`.
    pub(crate) source_id: Option<String>,
    pub(crate) workspace_id: String,
    /// The project a `detail` or `create` is about.
    pub(crate) project: Option<CommandProject>,
    /// Each declared credential field's name and value.
    pub(crate) credentials: Vec<(String, String)>,
}

impl std::fmt::Debug for CommandEnv {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandEnv")
            .field("source_id", &self.source_id)
            .field("workspace_id", &self.workspace_id)
            .field("project", &self.project)
            .field(
                "credentials",
                &self
                    .credentials
                    .iter()
                    .map(|(name, _)| (name.as_str(), MASK))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl CommandEnv {
    /// The command's whole environment: the inherited variables `inherited` finds, `PATH` as
    /// gxserver gives `gh`, and the `GHOSTEX_*` ones.
    pub(crate) fn vars(
        &self,
        inherited: impl Fn(&str) -> Option<OsString>,
    ) -> Vec<(String, OsString)> {
        let names = INHERITED_VARS.iter();
        #[cfg(windows)]
        let names = names.chain(INHERITED_WINDOWS_VARS.iter());
        let mut vars: Vec<(String, OsString)> = names
            .filter_map(|name| Some((name.to_string(), inherited(name)?)))
            .collect();
        vars.push(("PATH".to_string(), crate::managed_tools::run::job_path(&[])));
        let mut set = |name: &str, value: &str| vars.push((name.to_string(), value.into()));
        set("NO_COLOR", "1");
        set("GHOSTEX_WORKSPACE_ID", &self.workspace_id);
        if let Some(source_id) = &self.source_id {
            set("GHOSTEX_TICKET_SOURCE_ID", source_id);
        }
        if let Some(project) = &self.project {
            set("GHOSTEX_PROJECT_ID", &project.id);
            if let Some(path) = &project.path {
                set("GHOSTEX_PROJECT_PATH", path);
            }
            if let Some(repo) = &project.repo {
                set("GHOSTEX_PROJECT_REPO", repo);
            }
        }
        for (name, value) in &self.credentials {
            set(
                &format!("GHOSTEX_CREDENTIAL_{}", name.to_ascii_uppercase()),
                value,
            );
        }
        vars
    }

    /// The credential values that are set.
    fn secrets(&self) -> impl Iterator<Item = &str> {
        self.credentials
            .iter()
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.is_empty())
    }

    /// Whether any credential is set: the command's own stdout and stderr then never leave the
    /// runner (`CommandFailure::Exited`, `CommandFailure::Unreadable`).
    fn holds_secrets(&self) -> bool {
        self.secrets().next().is_some()
    }

    /// `text` with every credential value masked, or `WITHHELD` when it contains a value too
    /// short to mask in place. Every place any value appears is found in the original text and
    /// overlapping places are masked as one, so two values that share characters both go. Cut a
    /// text only after this, so a cut never keeps part of a value.
    pub(crate) fn redact(&self, text: &str) -> String {
        let mut places: Vec<std::ops::Range<usize>> = Vec::new();
        for value in self.secrets() {
            let mut from = 0;
            while let Some(found) = text[from..].find(value) {
                if value.chars().count() < MIN_MASKED_IN_PLACE_CHARS {
                    return WITHHELD.to_string();
                }
                let start = from + found;
                places.push(start..start + value.len());
                // The next search starts one character on, so a value that overlaps itself is
                // found at every place.
                from = start + text[start..].chars().next().map_or(1, char::len_utf8);
            }
        }
        places.sort_by_key(|place| place.start);
        let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
        for place in places {
            match merged.last_mut() {
                Some(last) if place.start <= last.end => last.end = last.end.max(place.end),
                _ => merged.push(place),
            }
        }
        let mut masked = String::with_capacity(text.len());
        let mut kept_from = 0;
        for place in merged {
            masked.push_str(&text[kept_from..place.start]);
            masked.push_str(MASK);
            kept_from = place.end;
        }
        masked.push_str(&text[kept_from..]);
        masked
    }

    /// `text` as a user or a log may see it: redacted, then cut to `MAX_MESSAGE_CHARS`.
    pub(crate) fn shown(&self, text: &str) -> String {
        cut(&self.redact(text), MAX_MESSAGE_CHARS)
    }
}

/// One run of the command.
#[derive(Debug)]
pub(crate) struct CommandCall<'a> {
    /// The resolved path the user set.
    pub(crate) program: &'a Path,
    pub(crate) verb: CommandVerb,
    /// Written to stdin; `None` gives the command no input (`info`).
    pub(crate) input: Option<&'a Value>,
    /// Where it runs (`command_work_dir`).
    pub(crate) work_dir: &'a Path,
    pub(crate) env: &'a CommandEnv,
    pub(crate) timeout: Duration,
}

/// Why a run gave no answer. Every text in it has the credentials masked.
#[derive(Debug)]
pub(crate) enum CommandFailure {
    NotFound(PathBuf),
    NotExecutable(PathBuf),
    /// A `.bat` or `.cmd` file, which Ghostex refuses to run.
    BatchFile(PathBuf),
    /// It could not start for another reason.
    Start(String),
    TimedOut {
        verb: CommandVerb,
        limit: Duration,
    },
    /// Its stdout passed the verb's cap.
    TooLarge(usize),
    /// It failed and said why in its error object.
    Reported {
        code: ErrorCode,
        message: String,
    },
    /// It failed without an error object.
    Exited {
        verb: CommandVerb,
        /// `status 3`, or the signal that ended it.
        status: String,
        /// Its last stderr line. `None` when it wrote none, or when a credential is set:
        /// stderr can spell a credential in ways no mask finds (JSON escapes, URL encoding), so
        /// only its size is kept.
        stderr_line: Option<String>,
        /// How much of stderr was kept (at most `STDERR_TAIL`).
        stderr_bytes: usize,
        /// Whether it wrote more than `STDERR_TAIL`, so only the end was kept.
        stderr_truncated: bool,
    },
    /// Its answer was not readable JSON of the verb's shape. `what` names what could not be read,
    /// with credentials masked. `stdout` is its first `MAX_MESSAGE_CHARS` characters, for the
    /// log, or only its size when a credential is set, for the same reason as stderr.
    Unreadable {
        what: String,
        stdout: String,
    },
}

impl CommandFailure {
    /// What the user sees, naming the source by `name`.
    pub(crate) fn message(&self, name: &str) -> String {
        match self {
            Self::NotFound(path) => format!("{name}: can't find {}", path.display()),
            Self::NotExecutable(path) => format!("{name}: {} isn't executable", path.display()),
            Self::BatchFile(path) => format!(
                "{name}: Ghostex doesn't run .bat or .cmd files ({}); use an .exe or a .ps1 script",
                path.display()
            ),
            Self::Start(error) => format!("{name}: couldn't start ({error})"),
            Self::TimedOut {
                verb: CommandVerb::Create,
                limit,
            } => format!(
                "{name} did not answer within {} s. The ticket may have been created, so check {name} before trying again.",
                limit.as_secs()
            ),
            Self::TimedOut { limit, .. } => {
                format!("{name} did not answer within {} s.", limit.as_secs())
            }
            Self::TooLarge(cap) => format!("{name} answered more than {}.", size_text(*cap)),
            Self::Reported {
                code: ErrorCode::Auth,
                message,
            } => format!(
                "{name}: {}. Check its credentials in Settings.",
                message.trim_end_matches('.')
            ),
            Self::Reported { message, .. }
            | Self::Exited {
                stderr_line: Some(message),
                ..
            } => format!("{name}: {message}"),
            Self::Exited {
                status,
                stderr_bytes: 0,
                ..
            } => format!("{name}: exited with {status}"),
            Self::Exited {
                verb,
                status,
                stderr_bytes,
                stderr_truncated,
                ..
            } => format!(
                "{name}: {} exited with {status}; its error output ({}) is hidden because it could show a credential.",
                verb.word(),
                if *stderr_truncated {
                    format!("more than {}", size_text(STDERR_TAIL))
                } else {
                    format!("{stderr_bytes} bytes")
                }
            ),
            Self::Unreadable { what, .. } => {
                format!("{name} answered something Ghostex can't read ({what})")
            }
        }
    }
}

/// `4 MB`, `64 KB`.
fn size_text(bytes: usize) -> String {
    if bytes >= MIB && bytes % MIB == 0 {
        format!("{} MB", bytes / MIB)
    } else {
        format!("{} KB", bytes / KIB)
    }
}

/// The folder a source's command runs in, created private on first use:
/// `<gxserver data>/work-sources/<sourceId>/`, or `work-sources/` itself before the first `info`
/// pinned an id. It gives the command a stable place for a cache of its own; project context
/// travels in its environment, since `list` spans every project of a workspace.
pub(crate) fn command_work_dir(
    paths: &GxserverPaths,
    source_id: Option<&str>,
) -> std::io::Result<PathBuf> {
    let mut dir = paths.app_data_dir.join("work-sources");
    if let Some(source_id) = source_id {
        dir.push(source_id);
    }
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

/// Runs the call and returns its JSON answer.
pub(crate) fn run_command(call: &CommandCall) -> Result<Value, CommandFailure> {
    run_answer(call).map(|answer| answer.value)
}

/// A JSON answer and the stdout it was read from.
struct Answer {
    value: Value,
    stdout: Vec<u8>,
}

fn run_answer(call: &CommandCall) -> Result<Answer, CommandFailure> {
    // Not `metadata`: a Windows app execution alias launches fine but fails a followed `stat`.
    if !ghostex_paths::powershell::launchable(call.program) {
        return Err(CommandFailure::NotFound(call.program.to_path_buf()));
    }
    let mut command = program_command(call.program)?;
    command
        .arg(call.verb.word())
        .arg("--json")
        .current_dir(call.work_dir)
        .env_clear()
        .envs(call.env.vars(|name| std::env::var_os(name)));
    let limits = RunLimits {
        timeout: call.timeout,
        stdin: call.input.map(|input| input.to_string().into_bytes()),
        stdout: Keep::AtMost(call.verb.stdout_cap()),
        // One byte past the tail tells an error output of exactly `STDERR_TAIL` from a longer one.
        stderr: Keep::Tail(STDERR_TAIL + 1),
    };
    let output = run_bounded(command, limits).map_err(|unfinished| match unfinished {
        Unfinished::Start(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            CommandFailure::NotExecutable(call.program.to_path_buf())
        }
        Unfinished::Start(error) => CommandFailure::Start(call.env.shown(&error.to_string())),
        Unfinished::TimedOut(limit) => CommandFailure::TimedOut {
            verb: call.verb,
            limit,
        },
        Unfinished::TooLarge(cap) => CommandFailure::TooLarge(cap),
    })?;
    if !output.status.success() {
        return Err(exit_failure(&output, call.verb, call.env));
    }
    match parse_answer(&output.stdout) {
        Ok(value) => Ok(Answer {
            value,
            stdout: output.stdout,
        }),
        Err(what) => Err(unreadable(call.env, &what, &output.stdout)),
    }
}

/// Runs `info` and reads its answer.
pub(crate) fn command_info(call: &CommandCall) -> Result<CommandInfo, CommandFailure> {
    let answer = run_answer(call)?;
    let mut info =
        parse_info(&answer.value).map_err(|what| unreadable(call.env, &what, &answer.stdout))?;
    if call.env.holds_secrets() {
        info.icon = info
            .icon
            .map(|icon| icon.map_err(|_| ICON_REFUSED.to_string()));
    }
    Ok(info)
}

/// Runs `list` (open or exact, as `call.input` asks) and reads its answer.
pub(crate) fn command_list(
    call: &CommandCall,
    format: IdFormat,
    projects: &[CommandProject],
) -> Result<CommandList, CommandFailure> {
    let answer = run_answer(call)?;
    parse_list(&answer.value, format, projects)
        .map_err(|what| unreadable(call.env, &what, &answer.stdout))
}

/// Runs `detail` and reads its answer.
pub(crate) fn command_detail(
    call: &CommandCall,
    format: IdFormat,
    projects: &[CommandProject],
) -> Result<CommandDetail, CommandFailure> {
    let answer = run_answer(call)?;
    parse_detail(&answer.value, format, projects)
        .map_err(|what| unreadable(call.env, &what, &answer.stdout))
}

/// Runs `create` and reads its answer.
pub(crate) fn command_create(
    call: &CommandCall,
    format: IdFormat,
) -> Result<CommandCreated, CommandFailure> {
    let answer = run_answer(call)?;
    parse_created(&answer.value, format).map_err(|what| unreadable(call.env, &what, &answer.stdout))
}

/// `what` is built from decoded JSON values, which the mask matches; the raw stdout is kept only
/// when no credential is set (`CommandFailure::Unreadable`).
fn unreadable(env: &CommandEnv, what: &str, stdout: &[u8]) -> CommandFailure {
    CommandFailure::Unreadable {
        what: env.shown(what),
        stdout: if env.holds_secrets() {
            format!(
                "({} bytes, hidden because they could show a credential)",
                stdout.len()
            )
        } else {
            cut(&String::from_utf8_lossy(stdout), MAX_MESSAGE_CHARS)
        },
    }
}

/// How the program is started: directly, or a `.ps1` script through PowerShell.
fn program_command(program: &Path) -> Result<Command, CommandFailure> {
    let extension = program
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("bat" | "cmd") => Err(CommandFailure::BatchFile(program.to_path_buf())),
        Some("ps1") => {
            let mut command = Command::new(powershell());
            command
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(program);
            Ok(command)
        }
        _ => Ok(Command::new(program)),
    }
}

/// PowerShell 7 when it is installed, else Windows PowerShell.
#[cfg(windows)]
fn powershell() -> PathBuf {
    ghostex_paths::powershell::find_powershell7(
        &ghostex_paths::powershell::Environment::from_process(),
        crate::platform::live_path::directories,
    )
    .map(|found| found.executable)
    .unwrap_or_else(|| PathBuf::from("powershell.exe"))
}

#[cfg(not(windows))]
fn powershell() -> PathBuf {
    PathBuf::from("pwsh")
}

/// One JSON value; surrounding whitespace and a UTF-8 BOM are tolerated, anything that is not
/// UTF-8 is not.
fn parse_answer(stdout: &[u8]) -> Result<Value, String> {
    let stdout = stdout.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(stdout);
    let text = std::str::from_utf8(stdout).map_err(|_| "its answer isn't UTF-8".to_string())?;
    serde_json::from_str(text.trim()).map_err(|error| format!("invalid JSON: {error}"))
}

fn exit_failure(
    output: &crate::platform::process::RunOutput,
    verb: CommandVerb,
    env: &CommandEnv,
) -> CommandFailure {
    if let Some((code, message)) = parse_answer(&output.stdout)
        .ok()
        .as_ref()
        .and_then(reported_error)
    {
        return CommandFailure::Reported {
            code,
            message: cut(env.redact(&message).trim(), MAX_MESSAGE_CHARS),
        };
    }
    // A tail cut from a longer log starts on a character boundary.
    let stderr_truncated = output.stderr.len() > STDERR_TAIL;
    let tail = &output.stderr[output.stderr.len().saturating_sub(STDERR_TAIL)..];
    let start = if stderr_truncated {
        tail.iter()
            .position(|byte| byte & 0b1100_0000 != 0b1000_0000)
            .unwrap_or(tail.len())
    } else {
        0
    };
    let stderr = String::from_utf8_lossy(&tail[start..]);
    let stderr_line = stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .filter(|_| !env.holds_secrets())
        .map(|line| cut(line, MAX_MESSAGE_CHARS));
    CommandFailure::Exited {
        verb,
        status: match output.status.code() {
            Some(code) => format!("status {code}"),
            None => output.status.to_string(),
        },
        stderr_line,
        stderr_bytes: tail.len(),
        stderr_truncated,
    }
}
