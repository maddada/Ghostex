//! Running a command ticket source: one verb per process, no shell, a cleared environment plus
//! an allowlist, JSON on stdin, a time limit and an output cap per verb. Every text that can
//! reach a user or a log has the command's credential values masked.
//!
//! CDXC:WorkMode 2026-10-29 DECISION:
//! The repository owner: "Pass ticket ids on stdin, never as an argument: they can come from branch names and `ghostex link add`, so `-x` must never read as a flag. Refuse `.bat`/`.cmd`." So argv is only the verb and `--json`, and `detail` reads its id from stdin like `list` and `create` read theirs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{Map, Value};

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
pub(crate) const ICON_REFUSED: &str =
    "it was refused; the reason is hidden because it could show a credential";

/// Variables passed through from gxserver's environment when set. Everything else is left out,
/// so tokens of other tools, the Linear keys and session variables never reach the command.
pub(crate) const INHERITED_VARS: [&str; 16] = [
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
/// Also passed through on Windows. Compiled everywhere so its tests run on every platform.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const INHERITED_WINDOWS_VARS: [&str; 14] = [
    "SystemRoot",
    "windir",
    "ComSpec",
    "PATHEXT",
    "USERPROFILE",
    "USERNAME",
    "HOMEDRIVE",
    "HOMEPATH",
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
    /// The declared credential fields that have a value.
    pub(crate) credentials: Vec<CommandCredential>,
}

/// One credential the command gets: a declared field and its stored value.
#[derive(Clone)]
pub(crate) struct CommandCredential {
    pub(crate) name: String,
    pub(crate) value: String,
    /// Declared secret (the default when `info` says nothing): masked wherever it would surface,
    /// and while one is set the command's raw stdout and stderr never leave the runner. A value
    /// declared not secret is shown in Settings anyway, so it is neither masked nor hidden.
    pub(crate) secret: bool,
}

/// Every text through `shown`, and each credential as its name and `MASK`, so a credential that
/// equals or sits inside an id, a path or a name never prints.
impl std::fmt::Debug for CommandEnv {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown = |text: &str| self.shown(text);
        formatter
            .debug_struct("CommandEnv")
            .field("source_id", &self.source_id.as_deref().map(shown))
            .field("workspace_id", &shown(&self.workspace_id))
            .field(
                "project",
                &self.project.as_ref().map(|project| {
                    [
                        Some(project.id.as_str()),
                        Some(project.name.as_str()),
                        project.path.as_deref(),
                        project.repo.as_deref(),
                    ]
                    .map(|text| text.map(shown))
                }),
            )
            .field(
                "credentials",
                &self
                    .credentials
                    .iter()
                    .map(|credential| (shown(&credential.name), MASK))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl CommandEnv {
    /// The environment for a source whose accepted `info` declared `declared`: one credential per
    /// declared field with a stored value, trimmed, empty ones left out. Fields the info no longer
    /// declares are never passed.
    pub(crate) fn new(
        source_id: Option<&str>,
        workspace_id: &str,
        project: Option<CommandProject>,
        declared: &[CredentialField],
        stored: Option<&Map<String, Value>>,
    ) -> Self {
        let credentials = declared
            .iter()
            .filter_map(|field| {
                let value = stored?.get(&field.name)?.as_str()?.trim();
                (!value.is_empty()).then(|| CommandCredential {
                    name: field.name.clone(),
                    value: value.to_string(),
                    secret: field.secret,
                })
            })
            .collect();
        Self {
            source_id: source_id.map(str::to_string),
            workspace_id: workspace_id.to_string(),
            project,
            credentials,
        }
    }

    /// This environment with `names` secret too: an `info` being checked may declare secret a
    /// field the accepted info did not, and it is masked in that very answer (the union of both).
    pub(crate) fn with_secret(&self, names: &[String]) -> Self {
        let mut widened = self.clone();
        for credential in &mut widened.credentials {
            credential.secret |= names.contains(&credential.name);
        }
        widened
    }

    /// The names of the fields that have a value, which a candidate `info` may declare secret.
    pub(crate) fn credential_names(&self) -> Vec<&str> {
        self.credentials
            .iter()
            .map(|credential| credential.name.as_str())
            .collect()
    }

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
        for credential in &self.credentials {
            set(
                &format!(
                    "GHOSTEX_CREDENTIAL_{}",
                    credential.name.to_ascii_uppercase()
                ),
                &credential.value,
            );
        }
        vars
    }

    /// The secret values that are set, trimmed: a value stored with spaces around it is still
    /// found where the command echoes it without them.
    fn secrets(&self) -> impl Iterator<Item = &str> {
        self.credentials
            .iter()
            .filter(|credential| credential.secret)
            .map(|credential| credential.value.trim())
            .filter(|value| !value.is_empty())
    }

    /// Whether a secret is set: the command's own stdout and stderr then never leave the runner
    /// (`CommandFailure::Exited`, `CommandFailure::Unreadable`), nor does a refused icon's reason.
    pub(crate) fn holds_secrets(&self) -> bool {
        self.secrets().next().is_some()
    }

    /// Whether `text` contains a secret value, however short.
    pub(crate) fn reveals_secret(&self, text: &str) -> bool {
        self.secrets().any(|value| text.contains(value))
    }

    /// `text` with every secret value masked, or `WITHHELD` when it contains a value too
    /// short to mask in place. Not cut: `shown` cuts a message, `AnswerGuard::display` cuts an
    /// answer's field to its own cap. Every place any value appears is found in the original text and
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
pub(crate) struct CommandCall<'a> {
    /// The resolved path the user set.
    pub(crate) program: &'a Path,
    pub(crate) verb: CommandVerb,
    /// Written to stdin; `None` gives the command no input (`info`).
    pub(crate) input: Option<&'a Value>,
    /// Where it runs (`command_work_dir`).
    pub(crate) work_dir: &'a Path,
    pub(crate) env: &'a CommandEnv,
    /// Always the verb's own limit, except in tests.
    timeout: Duration,
}

impl<'a> CommandCall<'a> {
    pub(crate) fn new(
        program: &'a Path,
        verb: CommandVerb,
        input: Option<&'a Value>,
        work_dir: &'a Path,
        env: &'a CommandEnv,
    ) -> Self {
        Self {
            program,
            verb,
            input,
            work_dir,
            env,
            timeout: verb.timeout(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// The program and folder through `shown`, the input only as its size.
impl std::fmt::Debug for CommandCall<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandCall")
            .field(
                "program",
                &self.env.shown(&self.program.display().to_string()),
            )
            .field("verb", &self.verb)
            .field(
                "input_bytes",
                &self.input.map(|input| input.to_string().len()),
            )
            .field(
                "work_dir",
                &self.env.shown(&self.work_dir.display().to_string()),
            )
            .field("env", self.env)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Why a run gave no answer. Every string in it went through `CommandEnv::shown` where the runner
/// built it (the paths too), so its `Debug` and `message` never show a credential.
#[derive(Debug)]
pub(crate) enum CommandFailure {
    /// The program's path, shown.
    NotFound(String),
    NotExecutable(String),
    /// A `.bat` or `.cmd` file, which Ghostex refuses to run.
    BatchFile(String),
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
        /// Whether a line was held back because a credential is set (else it wrote only blanks).
        stderr_hidden: bool,
        /// How much of stderr was kept (at most `STDERR_TAIL`).
        stderr_bytes: usize,
        /// Whether it wrote more than `STDERR_TAIL`, so only the end was kept.
        stderr_truncated: bool,
    },
    /// Its answer was not readable JSON of the verb's shape. `what` names what could not be read,
    /// with credentials masked. `stdout` is its first `MAX_MESSAGE_CHARS` characters, or only its
    /// size while a secret is set, for the same reason as stderr. It goes to the person checking
    /// the command (the Settings or CLI check's answer), never to the log, which keeps no command
    /// output.
    Unreadable {
        what: String,
        stdout: String,
    },
}

impl CommandFailure {
    /// What the user sees, naming the source by `name`, which the caller passes through
    /// `CommandEnv::shown`.
    pub(crate) fn message(&self, name: &str) -> String {
        match self {
            Self::NotFound(path) => format!("{name}: can't find {path}"),
            Self::NotExecutable(path) => format!("{name}: {path} isn't executable"),
            Self::BatchFile(path) => format!(
                "{name}: Ghostex doesn't run .bat or .cmd files ({path}); use an .exe or a .ps1 script"
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
                stderr_hidden: false,
                ..
            } => format!(
                "{name}: {} exited with {status}; its error output was blank.",
                verb.word()
            ),
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
    /// For `info`: the environment widened with the secrets the answer itself declares.
    info_env: Option<CommandEnv>,
}

fn run_answer(call: &CommandCall) -> Result<Answer, CommandFailure> {
    // Not `metadata`: a Windows app execution alias launches fine but fails a followed `stat`.
    if !ghostex_paths::powershell::launchable(call.program) {
        return Err(CommandFailure::NotFound(shown_path(call.env, call.program)));
    }
    let command = command_for(call)?;
    let limits = RunLimits {
        timeout: call.timeout(),
        stdin: call.input.map(|input| input.to_string().into_bytes()),
        stdout: Keep::AtMost(call.verb.stdout_cap()),
        // One byte past the tail tells an error output of exactly `STDERR_TAIL` from a longer one.
        stderr: Keep::Tail(STDERR_TAIL + 1),
    };
    let output = run_bounded(command, limits).map_err(|unfinished| match unfinished {
        Unfinished::Start(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            CommandFailure::NotExecutable(shown_path(call.env, call.program))
        }
        Unfinished::Start(error) => CommandFailure::Start(call.env.shown(&error.to_string())),
        Unfinished::TimedOut(limit) => CommandFailure::TimedOut {
            verb: call.verb,
            limit,
        },
        Unfinished::TooLarge(cap) => CommandFailure::TooLarge(cap),
    })?;
    // An `info` answer may declare secret a field the accepted info had as not secret. Its own
    // declarations widen the mask before anything it printed is read, whatever its exit status, so
    // a failing `info` can't show that value in its error object or on stderr either.
    let info_env = (call.verb == CommandVerb::Info).then(|| info_env(call.env, &output.stdout));
    let env = info_env.as_ref().unwrap_or(call.env);
    if !output.status.success() {
        return Err(exit_failure(&output, call.verb, env));
    }
    match parse_answer(&output.stdout) {
        Ok(value) => Ok(Answer {
            value,
            stdout: output.stdout,
            info_env,
        }),
        Err(what) => Err(unreadable(env, &what, &output.stdout)),
    }
}

/// `env` with every field that has a value and that `info`'s stdout declares secret (the union of
/// both); `env` as it is when stdout isn't JSON, which declares nothing.
fn info_env(env: &CommandEnv, stdout: &[u8]) -> CommandEnv {
    match parse_answer(stdout) {
        Ok(answer) => env.with_secret(&credential_secrecy(&answer, &env.credential_names())),
        Err(_) => env.clone(),
    }
}

/// Runs `info` and reads its answer in two phases. First, which fields with a value the answer
/// declares secret, read before anything else in it is checked, whatever the exit status
/// (`run_answer`): a field the accepted info had as not secret is then masked in this very answer,
/// even when the rest of it is refused or the command failed. Then the whole answer, with the
/// secrets of both. Stdout that isn't JSON declares nothing, so the accepted flags apply to it.
pub(crate) fn command_info(call: &CommandCall) -> Result<CommandInfo, CommandFailure> {
    let answer = run_answer(call)?;
    let env = answer.info_env.as_ref().unwrap_or(call.env);
    let mut info = parse_info(&answer.value, &AnswerGuard::new(env))
        .map_err(|what| unreadable(env, &what, &answer.stdout))?;
    if env.holds_secrets() {
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
    parse_list(&answer.value, format, projects, &AnswerGuard::new(call.env))
        .map_err(|what| unreadable(call.env, &what, &answer.stdout))
}

/// Runs `detail` and reads its answer.
pub(crate) fn command_detail(
    call: &CommandCall,
    format: IdFormat,
    projects: &[CommandProject],
) -> Result<CommandDetail, CommandFailure> {
    let answer = run_answer(call)?;
    parse_detail(&answer.value, format, projects, &AnswerGuard::new(call.env))
        .map_err(|what| unreadable(call.env, &what, &answer.stdout))
}

/// Runs `create` and reads its answer.
pub(crate) fn command_create(
    call: &CommandCall,
    format: IdFormat,
) -> Result<CommandCreated, CommandFailure> {
    let answer = run_answer(call)?;
    parse_created(&answer.value, format, &AnswerGuard::new(call.env))
        .map_err(|what| unreadable(call.env, &what, &answer.stdout))
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
            env.shown(&String::from_utf8_lossy(stdout))
        },
    }
}

/// The process for a call: the program (directly, or a `.ps1` script through PowerShell), then
/// only the verb and `--json`, in the call's folder, with the cleared environment.
pub(crate) fn command_for(call: &CommandCall) -> Result<Command, CommandFailure> {
    let mut command = program_command(call.program)
        .ok_or_else(|| CommandFailure::BatchFile(shown_path(call.env, call.program)))?;
    command
        .arg(call.verb.word())
        .arg("--json")
        .current_dir(call.work_dir)
        .env_clear()
        .envs(call.env.vars(|name| std::env::var_os(name)));
    Ok(command)
}

/// The extension Windows goes by: it drops trailing dots and spaces from a file name, so
/// `run.cmd.` and `run.bat ` open as `run.cmd` and `run.bat`.
fn launch_extension(program: &Path) -> Option<String> {
    let name = program.file_name()?.to_str()?;
    let (_, extension) = name.trim_end_matches(['.', ' ']).rsplit_once('.')?;
    Some(extension.to_ascii_lowercase())
}

fn shown_path(env: &CommandEnv, path: &Path) -> String {
    env.shown(&path.display().to_string())
}

/// How the program is started: directly, or a `.ps1` script through PowerShell. `None` for a
/// `.bat` or `.cmd` file, which is never run.
fn program_command(program: &Path) -> Option<Command> {
    match launch_extension(program).as_deref() {
        Some("bat" | "cmd") => None,
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
            Some(command)
        }
        _ => Some(Command::new(program)),
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
    let last_line = stderr.lines().map(str::trim).rfind(|line| !line.is_empty());
    let stderr_hidden = last_line.is_some() && env.holds_secrets();
    let stderr_line = last_line
        .filter(|_| !stderr_hidden)
        .map(|line| env.shown(line));
    CommandFailure::Exited {
        verb,
        status: match output.status.code() {
            Some(code) => format!("status {code}"),
            None => output.status.to_string(),
        },
        stderr_line,
        stderr_hidden,
        stderr_bytes: tail.len(),
        stderr_truncated,
    }
}
