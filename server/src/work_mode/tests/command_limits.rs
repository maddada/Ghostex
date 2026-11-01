//! The runner's fixed limits and the process it builds, pinned on every platform: each verb's
//! time limit and answer cap, the variables passed through, and the exact argv, so a change to
//! any of them fails here instead of passing unnoticed.

use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use super::*;

#[test]
fn each_verb_keeps_its_time_limit_and_answer_cap() {
    let limits: Vec<(CommandVerb, &str, u64, usize)> = [
        CommandVerb::Info,
        CommandVerb::OpenList,
        CommandVerb::ExactList,
        CommandVerb::Detail,
        CommandVerb::Create,
    ]
    .into_iter()
    .map(|verb| {
        (
            verb,
            verb.word(),
            verb.timeout().as_secs(),
            verb.stdout_cap(),
        )
    })
    .collect();
    assert_eq!(
        limits,
        vec![
            (CommandVerb::Info, "info", 10, 64 * 1024),
            (CommandVerb::OpenList, "list", 45, 4 * 1024 * 1024),
            (CommandVerb::ExactList, "list", 30, 4 * 1024 * 1024),
            (CommandVerb::Detail, "detail", 20, 1024 * 1024),
            (CommandVerb::Create, "create", 60, 64 * 1024),
        ]
    );
    assert_eq!(CommandVerb::Detail.timeout(), Duration::from_secs(20));
}

#[test]
fn only_these_variables_are_passed_through() {
    assert_eq!(
        INHERITED_VARS,
        [
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
        ]
    );
    assert_eq!(
        INHERITED_WINDOWS_VARS,
        [
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
        ]
    );
}

fn args_of(command: &std::process::Command) -> Vec<String> {
    command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn call_for<'a>(program: &'a Path, env: &'a CommandEnv) -> CommandCall<'a> {
    CommandCall::new(
        program,
        CommandVerb::Detail,
        None,
        Path::new("/work-sources/fixture"),
        env,
    )
}

#[test]
fn a_program_gets_only_the_verb_and_json_flag() {
    let env = CommandEnv::default();
    let program = Path::new("/opt/fixture/source");
    let command = command_for(&call_for(program, &env)).expect("a plain program runs");
    assert_eq!(command.get_program(), program.as_os_str());
    assert_eq!(args_of(&command), ["detail", "--json"]);
    assert_eq!(
        command.get_current_dir(),
        Some(Path::new("/work-sources/fixture"))
    );
}

#[test]
fn a_powershell_script_runs_through_powershell_with_fixed_flags() {
    let env = CommandEnv::default();
    for script in ["/opt/fixture/source.ps1", "/opt/fixture/source.PS1"] {
        let command = command_for(&call_for(Path::new(script), &env)).expect("a .ps1 runs");
        let program = Path::new(command.get_program());
        let stem = program
            .file_stem()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase);
        assert!(
            matches!(stem.as_deref(), Some("pwsh" | "powershell")),
            "{program:?}"
        );
        assert_eq!(
            args_of(&command),
            [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                script,
                "detail",
                "--json",
            ]
        );
    }
}

#[test]
fn a_batch_file_is_refused_even_with_trailing_dots_or_spaces() {
    let env = CommandEnv::default();
    for name in [
        "run.bat",
        "run.cmd",
        "RUN.CMD",
        "run.cmd.",
        "run.bat ",
        "run.cmd. .",
        "run.bat..",
    ] {
        let program = Path::new("/opt/fixture").join(name);
        assert!(
            matches!(
                command_for(&call_for(&program, &env)),
                Err(CommandFailure::BatchFile(_))
            ),
            "{name:?}"
        );
    }
    let program = Path::new("/opt/fixture/run.cmdline");
    assert!(command_for(&call_for(program, &env)).is_ok());
}
