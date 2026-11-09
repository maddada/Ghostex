//! Runs a PowerShell script elevated through one UAC prompt, for the remote
//! access steps that need administrator rights on Windows (turning SSH on,
//! adding a paired phone's key to the administrators keys file).

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

// Exit codes the launcher reports for itself; every other code is the
// elevated script's own. 1223 is Windows' ERROR_CANCELLED, which is what a
// declined UAC prompt raises.
pub(crate) const EXIT_LAUNCH_FAILED: i32 = 91;
pub(crate) const EXIT_NO_PROCESS: i32 = 92;
pub(crate) const EXIT_NO_EXIT_CODE: i32 = 93;
pub(crate) const EXIT_NO_DESKTOP: i32 = 94;
pub(crate) const EXIT_UAC_CANCELLED: i32 = 1223;

/// What the elevated run reported: its exit code (`None` when it was
/// terminated), the reason lines it wrote to its log, and the launcher's
/// stderr (the raw exception when the prompt could not be opened).
pub(crate) struct ElevatedRun {
    pub code: Option<i32>,
    pub log: String,
    pub stderr: String,
}

// The elevated script is written to a temp file and run with `-File`, which
// keeps paths and quoting out of the command line entirely. Its parameters
// are the log path followed by `args`. The elevated process cannot share
// stdout with its parent, so it reports through exit codes and the log file
// the parent reads afterwards; the log holds only the reason for a failure,
// never progress lines, because it is shown to the user verbatim. The
// `Err` is a full sentence naming what could not be started.
pub(crate) fn run_elevated_powershell(
    stem: &str,
    script: &str,
    args: &[&str],
) -> Result<ElevatedRun, String> {
    let files = TempFiles::create(stem, script, args)
        .map_err(|error| format!("Could not prepare the elevation script: {error}."))?;
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&files.launcher)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
    /*
    CDXC:RemoteAccess 2026-11-09 WHY:
    GitHub #520 follow-up: this waited on the prompt with no limit, so a UAC prompt nobody answered
    (the user away from the PC, as when it is controlled from elsewhere) held its request, and the
    phone or computer asking again, for good. After `UAC_PROMPT_LIMIT` the launcher is stopped and
    the caller gets a plain sentence for the app to show; the elevated script itself never started,
    since it only starts once the prompt is accepted.
    */
    let limits = crate::platform::process::RunLimits {
        timeout: UAC_PROMPT_LIMIT,
        stdin: None,
        stdout: crate::platform::process::Keep::Head(64 * 1024),
        stderr: crate::platform::process::Keep::Head(64 * 1024),
    };
    let output = match crate::platform::process::run_bounded(command, limits) {
        Ok(output) => output,
        Err(crate::platform::process::Unfinished::TimedOut(_)) => {
            return Err(PROMPT_NOT_ANSWERED.to_string())
        }
        Err(crate::platform::process::Unfinished::Start(error)) => {
            return Err(format!("Could not open the administrator prompt: {error}."))
        }
        Err(crate::platform::process::Unfinished::TooLarge(_)) => {
            return Err("Could not open the administrator prompt.".to_string())
        }
    };
    Ok(ElevatedRun {
        code: output.status.code(),
        log: files.read_log(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

/// How long the UAC prompt may stay unanswered before Ghostex stops waiting on it.
const UAC_PROMPT_LIMIT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// What the app shows when the UAC prompt went unanswered for [`UAC_PROMPT_LIMIT`].
const PROMPT_NOT_ANSWERED: &str = "The Windows administrator prompt wasn't answered within 5 minutes, so nothing was changed. If it is still open, close it, then try again and choose Yes.";

struct TempFiles {
    launcher: PathBuf,
    script: PathBuf,
    log: PathBuf,
}

impl TempFiles {
    fn create(stem: &str, script: &str, args: &[&str]) -> std::io::Result<Self> {
        let stem = format!(
            "{stem}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        );
        let dir = std::env::temp_dir();
        let files = Self {
            launcher: dir.join(format!("{stem}-launch.ps1")),
            script: dir.join(format!("{stem}.ps1")),
            log: dir.join(format!("{stem}.log")),
        };
        fs::write(&files.log, "")?;
        fs::write(&files.script, script)?;
        fs::write(
            &files.launcher,
            launcher_script(&files.script, &files.log, args),
        )?;
        Ok(files)
    }

    fn read_log(&self) -> String {
        let text = fs::read_to_string(&self.log).unwrap_or_default();
        let lines = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        lines.join(" ").chars().take(600).collect()
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        for path in [&self.launcher, &self.script, &self.log] {
            let _ = fs::remove_file(path);
        }
    }
}

// The launcher waits for the elevated PowerShell and exits with its exit
// code. When the user declines the UAC prompt, `Start-Process` throws with
// a Win32Exception whose NativeErrorCode is 1223 (ERROR_CANCELLED)
// somewhere in the exception chain; that number is matched instead of the
// message, which is localized. A gxserver started from an SSH connection
// lives in session 0 with no interactive window station, where
// `Start-Process -Verb RunAs` can only throw "This operation requires an
// interactive window station"; that case is named up front so the user
// gets the fix (restart the background service from the desktop) instead
// of the raw exception.
fn launcher_script(script: &Path, log: &Path, args: &[&str]) -> String {
    let extra_args = args
        .iter()
        .map(|arg| format!(", '\"{}\"'", single_quoted_literal(arg)))
        .collect::<String>();
    format!(
        r#"$ErrorActionPreference = 'Stop'
if (-not [Environment]::UserInteractive) {{ exit {EXIT_NO_DESKTOP} }}
try {{
    $p = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', '"{script}"', '"{log}"'{extra_args})
}} catch {{
    $e = $_.Exception
    while ($null -ne $e) {{
        if ($e -is [System.ComponentModel.Win32Exception] -and $e.NativeErrorCode -eq {EXIT_UAC_CANCELLED}) {{ exit {EXIT_UAC_CANCELLED} }}
        $e = $e.InnerException
    }}
    [Console]::Error.WriteLine($_.Exception.Message)
    exit {EXIT_LAUNCH_FAILED}
}}
if ($null -eq $p) {{ exit {EXIT_NO_PROCESS} }}
if ($null -eq $p.ExitCode) {{ exit {EXIT_NO_EXIT_CODE} }}
exit $p.ExitCode
"#,
        script = single_quoted_literal(&script.to_string_lossy()),
        log = single_quoted_literal(&log.to_string_lossy()),
    )
}

// Inside a single-quoted PowerShell string the only special characters are
// the single quotes (ASCII and typographic), each written twice.
fn single_quoted_literal(text: &str) -> String {
    crate::platform::shell::powershell_single_quoted_content(text)
}
