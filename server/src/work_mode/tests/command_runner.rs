//! A command ticket source run for real: `command_source.sh` answers each verb the way its
//! `mode` credential says, and these tests check what the runner hands the command (argv, env,
//! stdin, folder) and what it makes of each answer, failure and misbehaving command.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::*;

const TOKEN: &str = "tok-fixture-4821";

struct Fixture {
    dir: tempfile::TempDir,
    program: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let program = dir.path().join("source.sh");
        std::fs::write(&program, include_str!("command_source.sh")).expect("write fixture");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { dir, program }
    }

    /// An environment with no credentials at all; the fixture reads its mode from a file.
    fn without_credentials(&self, mode: &str) -> CommandEnv {
        std::fs::write(self.dir.path().join("mode.txt"), mode).expect("write mode");
        CommandEnv {
            credentials: Vec::new(),
            ..env("unused")
        }
    }

    fn recorded(&self, file: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(file)).unwrap_or_default()
    }

    fn call<'a>(
        &'a self,
        verb: CommandVerb,
        input: Option<&'a Value>,
        env: &'a CommandEnv,
    ) -> CommandCall<'a> {
        call_of(&self.program, self.dir.path(), verb, input, env)
    }
}

fn call_of<'a>(
    program: &'a Path,
    work_dir: &'a Path,
    verb: CommandVerb,
    input: Option<&'a Value>,
    env: &'a CommandEnv,
) -> CommandCall<'a> {
    CommandCall::new(program, verb, input, work_dir, env)
}

fn env(mode: &str) -> CommandEnv {
    CommandEnv {
        source_id: Some("fixture".to_string()),
        workspace_id: "ws-cmd".to_string(),
        project: Some(app_project()),
        credentials: vec![
            credential("mode", mode, false),
            credential("token", TOKEN, true),
        ],
    }
}

fn app_project() -> CommandProject {
    CommandProject {
        id: "PCmdApp".to_string(),
        name: "app".to_string(),
        path: Some("/gx-char/cmd/app".to_string()),
        repo: Some("acme/app".to_string()),
    }
}

#[test]
fn a_good_source_answers_every_verb() {
    let fixture = Fixture::new();
    let env = env("good");

    let info = command_info(&fixture.call(CommandVerb::Info, None, &env)).expect("info");
    assert_eq!(info.id, "fixture");
    assert_eq!(info.name, "Fixture");
    assert_eq!(info.id_format, IdFormat::KeyNumber);
    assert_eq!(info.id_keys, vec!["ACM", "OPS"]);
    assert_eq!(
        info.credentials
            .iter()
            .map(|field| (field.name.as_str(), field.secret, field.required))
            .collect::<Vec<_>>(),
        vec![("mode", false, false), ("token", true, true)]
    );
    assert!(info.can_create);
    assert!(info
        .icon
        .as_ref()
        .is_some_and(|icon| icon.as_ref().is_ok_and(|svg| svg.contains("currentColor"))));
    assert_eq!(fixture.recorded("argv.txt"), "info\n--json\n");

    let projects = [app_project()];
    let input = list_input(&projects, None);
    let list = command_list(
        &fixture.call(CommandVerb::OpenList, Some(&input), &env),
        IdFormat::KeyNumber,
        &projects,
    )
    .expect("list");
    assert_eq!(list.tickets.len(), 1);
    let ticket = &list.tickets[0];
    assert_eq!(ticket.id, "ACM-12");
    assert_eq!(ticket.status.group, "progress");
    assert_eq!(ticket.project_id.as_deref(), Some("PCmdApp"));
    assert_eq!(ticket.team.as_deref(), Some("ACM"));
    assert_eq!(
        serde_json::from_str::<Value>(&fixture.recorded("stdin.txt")).unwrap(),
        input
    );

    let input = detail_input("ACM-12");
    let detail = command_detail(
        &fixture.call(CommandVerb::Detail, Some(&input), &env),
        IdFormat::KeyNumber,
        &projects,
    )
    .expect("detail");
    assert_eq!(detail.ticket.title, "Spec the contract");
    assert_eq!(detail.description.as_deref(), Some("Body"));
    assert_eq!(detail.comment_count, 1);
    assert!(!detail.fields.contains_key("secretField"));
    // The id arrives on stdin, never in argv.
    assert_eq!(fixture.recorded("argv.txt"), "detail\n--json\n");
    assert_eq!(fixture.recorded("stdin.txt"), r#"{"id":"ACM-12"}"#);

    let extra = serde_json::Map::new();
    let draft = TicketDraft {
        title: "New one",
        description: None,
        assign_to_me: true,
        extra: &extra,
    };
    let input = create_input(&draft, Some(&app_project()));
    let created = command_create(
        &fixture.call(CommandVerb::Create, Some(&input), &env),
        IdFormat::KeyNumber,
    )
    .expect("create");
    assert_eq!(created.id, "ACM-41");
    assert_eq!(
        serde_json::from_str::<Value>(&fixture.recorded("stdin.txt")).unwrap()["assignToMe"],
        json!(true)
    );
}

#[test]
fn the_command_gets_only_the_allowlisted_environment() {
    assert!(
        std::env::var_os("CARGO_MANIFEST_DIR").is_some(),
        "cargo sets it for the test process, so leaving it out is the runner's doing"
    );
    let fixture = Fixture::new();
    command_info(&fixture.call(CommandVerb::Info, None, &env("good"))).expect("info");
    let recorded = fixture.recorded("env.txt");
    let vars: Vec<&str> = recorded
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name))
        .collect();
    for wanted in [
        "PATH",
        "NO_COLOR",
        "GHOSTEX_WORKSPACE_ID",
        "GHOSTEX_TICKET_SOURCE_ID",
        "GHOSTEX_PROJECT_ID",
        "GHOSTEX_PROJECT_REPO",
        "GHOSTEX_CREDENTIAL_TOKEN",
    ] {
        assert!(vars.contains(&wanted), "{wanted} is missing from {vars:?}");
    }
    assert!(recorded.contains(&format!("GHOSTEX_CREDENTIAL_TOKEN={TOKEN}\n")));
    assert!(!vars.contains(&"CARGO_MANIFEST_DIR"), "{vars:?}");
}

#[test]
fn only_allowlisted_variables_are_inherited() {
    let env = CommandEnv {
        workspace_id: "ws-cmd".to_string(),
        ..CommandEnv::default()
    };
    let inherited = |name: &str| match name {
        "HOME" => Some("/home/me".into()),
        "GH_TOKEN" | "AWS_SECRET_ACCESS_KEY" | "GHOSTEX_SESSION_ID" | "ZMX_SESSION" => {
            Some("leak".into())
        }
        _ => None,
    };
    let names: Vec<String> = env
        .vars(inherited)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        vec!["HOME", "PATH", "NO_COLOR", "GHOSTEX_WORKSPACE_ID"]
    );
}

#[test]
fn a_failure_with_an_error_object_says_its_message_with_credentials_masked() {
    let fixture = Fixture::new();
    let failure = run_command(&fixture.call(CommandVerb::Info, None, &env("error-json")))
        .expect_err("it exited 1");
    assert!(matches!(
        &failure,
        CommandFailure::Reported { code: ErrorCode::Auth, message } if message == "The token •••• was rejected."
    ));
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: The token •••• was rejected. Check its credentials in Settings."
    );
}

#[test]
fn a_failure_without_an_error_object_says_its_last_stderr_line_or_its_status() {
    let fixture = Fixture::new();
    let bare = fixture.without_credentials("error-stderr");
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &bare)).expect_err("it exited 3");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: can't reach the server with"
    );
    let failure = run_command(&fixture.call(CommandVerb::Info, None, &env("error-silent")))
        .expect_err("it exited 4");
    assert_eq!(failure.message("Fixture"), "Fixture: exited with status 4");
}

#[test]
fn stderr_is_hidden_when_a_credential_is_set() {
    let fixture = Fixture::new();
    let input = detail_input("ACM-12");
    let failure =
        run_command(&fixture.call(CommandVerb::Detail, Some(&input), &env("error-stderr")))
            .expect_err("it exited 3");
    let message = failure.message("Fixture");
    assert!(
        message.starts_with("Fixture: detail exited with status 3; its error output (")
            && message.ends_with(" bytes) is hidden because it could show a credential."),
        "{message}"
    );
    assert!(
        !format!("{message} {failure:?}").contains("4821"),
        "{failure:?}"
    );
}

#[test]
fn a_json_escaped_credential_in_stdout_or_stderr_never_shows() {
    let fixture = Fixture::new();
    for (mode, verb) in [
        ("escaped-stdout", CommandVerb::Info),
        ("escaped-shape", CommandVerb::OpenList),
        ("escaped-stderr", CommandVerb::Info),
    ] {
        let env = env_with(&[("mode", mode), ("token", "a\"b\\c")]);
        let input = list_input(&[], None);
        let call = fixture.call(
            verb,
            Some(&input).filter(|_| verb != CommandVerb::Info),
            &env,
        );
        let failure = match verb {
            CommandVerb::OpenList => command_list(&call, IdFormat::KeyNumber, &[]).expect_err(mode),
            _ => run_command(&call).expect_err(mode),
        };
        let shown = format!("{} {failure:?}", failure.message("Fixture"));
        for spelling in [r#"a\"b\\c"#, r#"\u0061\u0022"#, "u0061", "a\"b\\c"] {
            assert!(!shown.contains(spelling), "{mode}: {shown}");
        }
        if let CommandFailure::Unreadable { stdout, .. } = &failure {
            assert!(
                stdout.ends_with(" bytes, hidden because they could show a credential)"),
                "{mode}: {stdout}"
            );
        }
    }
}

#[test]
fn an_answer_that_is_not_json_is_unreadable() {
    let fixture = Fixture::new();
    let env = fixture.without_credentials("invalid-json");
    let failure = run_command(&fixture.call(CommandVerb::Info, None, &env)).expect_err("not JSON");
    let CommandFailure::Unreadable { what, stdout } = &failure else {
        panic!("{failure:?}");
    };
    assert!(what.starts_with("invalid JSON"), "{what}");
    assert_eq!(stdout, "tickets: none\n");
}

#[test]
fn a_ticket_in_a_state_group_of_the_code_host_is_skipped_and_counted() {
    let fixture = Fixture::new();
    let input = list_input(&[], None);
    let list = command_list(
        &fixture.call(CommandVerb::OpenList, Some(&input), &env("bad-group")),
        IdFormat::KeyNumber,
        &[],
    )
    .expect("list");
    assert_eq!(
        list.tickets
            .iter()
            .map(|ticket| ticket.id.as_str())
            .collect::<Vec<_>>(),
        vec!["ACM-12"]
    );
    assert_eq!(
        list.errors("Fixture"),
        vec!["Fixture: skipped 1 ticket it sent in an unknown shape"]
    );
}

#[test]
fn an_answer_past_the_verbs_cap_fails() {
    let fixture = Fixture::new();
    let failure = run_command(&fixture.call(CommandVerb::Info, None, &env("oversized")))
        .expect_err("past 64 KB");
    assert!(matches!(failure, CommandFailure::TooLarge(cap) if cap == 64 * 1024));
    assert_eq!(
        failure.message("Fixture"),
        "Fixture answered more than 64 KB."
    );
    assert_eq!(
        CommandFailure::TooLarge(CommandVerb::OpenList.stdout_cap()).message("Fixture"),
        "Fixture answered more than 4 MB."
    );
}

#[test]
fn a_command_that_hangs_is_stopped_at_its_time_limit() {
    let fixture = Fixture::new();
    let env = env("hang");
    let input = detail_input("ACM-12");
    let started = Instant::now();
    let failure = run_command(
        &fixture
            .call(CommandVerb::Detail, Some(&input), &env)
            .with_timeout(Duration::from_secs(1)),
    )
    .expect_err("it hangs");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(
        failure.message("Fixture"),
        "Fixture did not answer within 1 s."
    );
    let create = CommandFailure::TimedOut {
        verb: CommandVerb::Create,
        limit: CommandVerb::Create.timeout(),
    };
    assert_eq!(
        create.message("Fixture"),
        "Fixture did not answer within 60 s. The ticket may have been created, so check Fixture before trying again."
    );
}

#[test]
fn an_answer_a_left_behind_child_still_holds_open_is_not_read() {
    let fixture = Fixture::new();
    let ids = vec!["ACM-9".to_string()];
    let input = list_input(&[app_project()], Some(&ids));
    let failure = command_list(
        &fixture
            .call(
                CommandVerb::ExactList,
                Some(&input),
                &env("answer-held-open"),
            )
            .with_timeout(Duration::from_secs(1)),
        IdFormat::KeyNumber,
        &[],
    )
    .expect_err("its answer never ended");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture did not answer within 1 s."
    );
}

#[test]
fn a_command_that_reads_its_input_late_still_gets_it() {
    let fixture = Fixture::new();
    let ids = vec!["ACM-9".to_string()];
    let input = list_input(&[app_project()], Some(&ids));
    let list = command_list(
        &fixture.call(CommandVerb::ExactList, Some(&input), &env("late-stdin")),
        IdFormat::KeyNumber,
        &[],
    )
    .expect("list");
    assert_eq!(list.not_found, vec!["ACM-9"]);
    assert_eq!(
        serde_json::from_str::<Value>(&fixture.recorded("stdin.txt")).unwrap(),
        input
    );
}

#[test]
fn a_bad_icon_is_dropped_and_the_source_keeps_its_name() {
    let fixture = Fixture::new();
    let env = fixture.without_credentials("bad-icon");
    let info = command_info(&fixture.call(CommandVerb::Info, None, &env)).expect("info");
    assert_eq!(info.name, "Fixture");
    assert_eq!(
        info.icon,
        Some(Err("it contains a <script> element".to_string()))
    );
}

#[test]
fn a_refused_icon_says_only_that_once_a_credential_is_set() {
    let fixture = Fixture::new();
    let refused =
        "it was refused; the reason is hidden because it could show a credential".to_string();
    // Longer than an icon error's 40-character quote, so a mask could never find what it keeps.
    let long_token = "tok-fixture-0123456789012345678901234567890123456789";
    for (mode, token) in [
        ("bad-icon", TOKEN),
        ("icon-token", TOKEN),
        ("icon-token", long_token),
        ("icon-token-tag", TOKEN),
    ] {
        let env = CommandEnv {
            credentials: vec![
                credential("mode", mode, false),
                credential("token", token, true),
            ],
            ..env(mode)
        };
        let info = command_info(&fixture.call(CommandVerb::Info, None, &env)).expect("info");
        assert_eq!(info.icon, Some(Err(refused.clone())), "{mode}");
    }
    // A good icon is kept with a credential set.
    let info = command_info(&fixture.call(CommandVerb::Info, None, &env("good"))).expect("info");
    assert!(matches!(info.icon, Some(Ok(_))), "{:?}", info.icon);
}

#[test]
fn a_missing_unrunnable_or_batch_program_says_so_without_running() {
    let dir = tempfile::tempdir().expect("tempdir");
    let env = env("good");
    let failure_of = |name: &str| {
        let program = dir.path().join(name);
        run_command(&call_of(
            &program,
            dir.path(),
            CommandVerb::Info,
            None,
            &env,
        ))
        .expect_err("it can't run")
        .message("Fixture")
    };
    let path_of = |name: &str| dir.path().join(name).display().to_string();

    assert_eq!(
        failure_of("missing"),
        format!("Fixture: can't find {}", path_of("missing"))
    );

    std::fs::write(dir.path().join("plain"), "#!/bin/sh\necho '{}'\n").unwrap();
    assert_eq!(
        failure_of("plain"),
        format!("Fixture: {} isn't executable", path_of("plain"))
    );

    for name in ["source.cmd", "source.BAT"] {
        std::fs::write(dir.path().join(name), "echo {}").unwrap();
        std::fs::set_permissions(
            dir.path().join(name),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert!(
            failure_of(name).contains("doesn't run .bat or .cmd files"),
            "{name}"
        );
    }
}

#[test]
fn the_work_folder_is_private_and_per_source() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let shared = command_work_dir(&paths, None).expect("shared folder");
    let own = command_work_dir(&paths, Some("fixture")).expect("own folder");
    assert_eq!(own, shared.join("fixture"));
    assert_eq!(
        std::fs::metadata(&own).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn a_command_env_never_prints_a_credential() {
    let printed = format!("{:?}", env("good"));
    assert!(!printed.contains(TOKEN), "{printed}");
    assert!(printed.contains("token"), "{printed}");
}

/// Every credential secret.
fn env_with(credentials: &[(&str, &str)]) -> CommandEnv {
    CommandEnv {
        credentials: credentials
            .iter()
            .map(|(name, value)| credential(name, value, true))
            .collect(),
        ..env("unused")
    }
}

fn credential(name: &str, value: &str, secret: bool) -> CommandCredential {
    CommandCredential {
        name: name.to_string(),
        value: value.to_string(),
        secret,
    }
}

#[test]
fn every_credential_value_is_masked_wherever_it_appears() {
    let long = "L".repeat(400);
    let env = env_with(&[
        ("pin", "abc"),
        ("token", &long),
        ("prefix", "secret"),
        ("whole", "secret-123"),
    ]);
    // Short, inside another word, and a value inside another value.
    assert_eq!(env.redact("PIN abc was rejected"), "PIN •••• was rejected");
    assert_eq!(env.redact("fabcd"), "f••••d");
    assert_eq!(env.redact("key secret-123 and secret"), "key •••• and ••••");
    // Long: masked before the cut, so no part of it survives.
    let shown = env.shown(&format!("token {long} rejected"));
    assert_eq!(shown, "token •••• rejected");
    // Across the cut.
    let shown = env.shown(&format!("{}{long}", "x".repeat(290)));
    assert!(!shown.contains('L'), "{shown}");
}

#[test]
fn a_text_holding_a_one_or_two_character_credential_is_withheld() {
    for value in ["7", "ab"] {
        let env = env_with(&[("pin", value)]);
        assert_eq!(
            env.redact(&format!("PIN {value} was rejected")),
            "(message hidden: it could show a credential)",
            "{value}"
        );
        assert_eq!(env.redact("nothing here"), "nothing here", "{value}");
    }
    assert_eq!(env_with(&[("pin", "")]).redact("empty"), "empty");
}

#[test]
fn a_short_credential_in_an_error_object_never_shows() {
    let fixture = Fixture::new();
    let env = CommandEnv {
        credentials: vec![
            credential("mode", "error-pin", false),
            credential("pin", "abc", true),
        ],
        ..env("unused")
    };
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &env)).expect_err("it exited 1");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: PIN •••• was rejected. Check its credentials in Settings."
    );
    assert!(!format!("{failure:?}").contains("abc"), "{failure:?}");
}

#[test]
fn a_credential_across_the_message_cut_never_shows() {
    let fixture = Fixture::new();
    let failure = run_command(&fixture.call(CommandVerb::Info, None, &env("error-long")))
        .expect_err("it exited 1");
    let message = failure.message("Fixture");
    assert!(message.ends_with("x••••"), "{message}");
    assert!(!message.contains("tok-"), "{message}");
}

#[test]
fn credentials_that_overlap_are_both_masked_in_either_order() {
    for credentials in [
        [("first", "abcXYZ"), ("second", "XYZdefgh")],
        [("first", "XYZdefgh"), ("second", "abcXYZ")],
    ] {
        let env = env_with(&credentials);
        assert_eq!(env.redact("key abcXYZdefgh end"), "key •••• end");
        assert_eq!(
            env.redact("XYZdefghabcXYZ"),
            "••••",
            "touching values merge"
        );
    }
    let env = env_with(&[("first", "defghabc"), ("second", "XYZdefgh")]);
    assert_eq!(env.redact("XYZdefghabc"), "••••");
    // A value that overlaps itself.
    assert_eq!(env_with(&[("pin", "aXa")]).redact("aXaXa!"), "••••!");
}

#[test]
fn a_stderr_tail_starting_inside_overlapping_credentials_never_shows_them() {
    let fixture = Fixture::new();
    let env = env_with(&[
        ("mode", "stderr-straddle"),
        ("first", "abcXYZ"),
        ("second", "XYZdefgh"),
    ]);
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &env)).expect_err("it exited 5");
    let shown = format!("{} {failure:?}", failure.message("Fixture"));
    assert!(
        shown.contains("its error output (more than 8 KB) is hidden"),
        "{shown}"
    );
    for part in ["YZ", "def", "fgh"] {
        assert!(!shown.contains(part), "{shown}");
    }
}

#[test]
fn a_credential_sent_with_spaces_around_it_never_shows() {
    let fixture = Fixture::new();
    // An error object whose message is the credential, spaces and all.
    let env = env_with(&[("mode", "error-padded"), ("token", " padded-secret ")]);
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &env)).expect_err("it exited 1");
    let shown = format!("{} {failure:?}", failure.message("Fixture"));
    assert!(!shown.contains("padded-secret"), "{shown}");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: ••••. Check its credentials in Settings."
    );
    // A contract complaint that quotes the field the credential was sent in.
    let env = env_with(&[("mode", "info-padded"), ("token", " Padded-Secret ")]);
    let failure = command_info(&fixture.call(CommandVerb::Info, None, &env)).expect_err("bad id");
    let shown = format!("{} {failure:?}", failure.message("Fixture"));
    assert!(!shown.contains("Padded-Secret"), "{shown}");
    // An id is structural: one that contains a credential is refused before its form is checked.
    assert!(shown.contains("(id contains a credential)"), "{shown}");
}

#[test]
fn a_credential_stored_with_spaces_is_masked_where_it_is_echoed_without_them() {
    let fixture = Fixture::new();
    let env = env_with(&[("mode", "error-token"), ("token", "  stored-padded  ")]);
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &env)).expect_err("it exited 1");
    let shown = format!("{} {failure:?}", failure.message("Fixture"));
    assert!(!shown.contains("stored-padded"), "{shown}");
    assert!(shown.contains("••••"), "{shown}");
}

#[test]
fn blank_error_output_without_a_credential_says_it_was_blank() {
    let fixture = Fixture::new();
    let env = fixture.without_credentials("blank-stderr");
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &env)).expect_err("it exited 3");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: info exited with status 3; its error output was blank."
    );
    let env = env_with(&[("mode", "blank-stderr"), ("token", TOKEN)]);
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &env)).expect_err("it exited 3");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: info exited with status 3; its error output was blank."
    );
}

#[test]
fn hidden_output_is_counted_in_the_bytes_the_command_wrote() {
    let fixture = Fixture::new();
    let failure = run_command(&fixture.call(CommandVerb::Info, None, &env("not-utf8")))
        .expect_err("not UTF-8");
    let CommandFailure::Unreadable { stdout, .. } = &failure else {
        panic!("{failure:?}");
    };
    assert_eq!(
        stdout,
        "(3 bytes, hidden because they could show a credential)"
    );

    let failure = run_command(&fixture.call(CommandVerb::Info, None, &env("stderr-exact")))
        .expect_err("it exited 7");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: info exited with status 7; its error output (8192 bytes) is hidden because it could show a credential."
    );
}

/// What the runner had as accepted: `token` set and declared not secret.
fn env_with_open_token(mode: &str) -> CommandEnv {
    CommandEnv {
        credentials: vec![
            credential("mode", mode, false),
            credential("token", TOKEN, false),
        ],
        ..env("unused")
    }
}

#[test]
fn a_field_turning_secret_is_masked_in_that_same_info() {
    let fixture = Fixture::new();
    let env = env_with_open_token("flip-secret");
    let info = command_info(&fixture.call(CommandVerb::Info, None, &env)).expect("info");
    assert_eq!(info.name, "Fixture ••••");
    assert!(info
        .credentials
        .iter()
        .any(|field| field.name == "token" && field.secret));
}

#[test]
fn a_field_declared_secret_in_an_unacceptable_info_is_still_masked() {
    let fixture = Fixture::new();
    for mode in ["flip-secret-invalid", "flip-secret-bad-label"] {
        let env = env_with_open_token(mode);
        let failure = command_info(&fixture.call(CommandVerb::Info, None, &env))
            .expect_err("the contract refuses it");
        let shown = format!("{} {failure:?}", failure.message("Fixture"));
        assert!(!shown.contains(TOKEN), "{mode}: {shown}");
        assert!(
            shown.contains("hidden because they could show a credential"),
            "{mode}: {shown}"
        );
    }
    // Stdout that isn't JSON declares nothing: the accepted flags apply, and with no secret set
    // the excerpt is shown.
    let env = env_with_open_token("invalid-json");
    let failure = command_info(&fixture.call(CommandVerb::Info, None, &env)).expect_err("no JSON");
    assert!(
        format!("{failure:?}").contains("tickets: none"),
        "{failure:?}"
    );
}

#[test]
fn a_non_secret_credential_leaves_stderr_shown_and_a_secret_hides_it() {
    let fixture = Fixture::new();
    let shown = env_with_open_token("error-stderr");
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &shown)).expect_err("it exited 3");
    assert_eq!(
        failure.message("Fixture"),
        format!("Fixture: can't reach the server with {TOKEN}")
    );
    let failure =
        run_command(&fixture.call(CommandVerb::Info, None, &env("error-stderr"))).expect_err("3");
    assert!(
        failure
            .message("Fixture")
            .ends_with("is hidden because it could show a credential."),
        "{}",
        failure.message("Fixture")
    );
}

#[test]
fn a_secret_equal_to_or_inside_any_path_id_or_name_never_prints() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Short, overlapping and three-character secrets, each also inside the texts below.
    for secrets in [
        vec!["b"],
        vec!["bi"],
        vec!["bin"],
        vec!["fixt", "xture"],
        vec!["ws-cmd"],
        vec!["acme/app"],
        vec!["token"],
    ] {
        let mut env = CommandEnv {
            source_id: Some("fixture".to_string()),
            workspace_id: "ws-cmd".to_string(),
            project: Some(app_project()),
            credentials: secrets
                .iter()
                .enumerate()
                .map(|(index, value)| credential(&format!("token{index}"), value, true))
                .collect(),
        };
        env.credentials
            .push(credential("token", "the-token-value", true));
        let missing = dir.path().join("usr/bin/fixture-token");
        let input = detail_input("ACM-12");
        let call = call_of(
            &missing,
            dir.path(),
            CommandVerb::Detail,
            Some(&input),
            &env,
        );
        let failure = run_command(&call).expect_err("not found");
        let batch = dir.path().join("bin/fixture.cmd");
        let batch_failure =
            command_for(&call_of(&batch, dir.path(), CommandVerb::Info, None, &env))
                .expect_err("refused");
        let printed = format!(
            "{} {failure:?} {} {batch_failure:?} {call:?} {env:?}",
            failure.message(&env.shown("Fixture")),
            batch_failure.message(&env.shown("Fixture")),
        );
        for secret in &secrets {
            let withheld = printed.contains("(message hidden: it could show a credential)");
            let masked_everywhere = !printed.contains(secret);
            assert!(masked_everywhere || withheld, "{secrets:?}: {printed}");
            if secret.chars().count() >= 3 {
                assert!(masked_everywhere, "{secrets:?}: {printed}");
            }
        }
        assert!(!printed.contains("the-token-value"), "{printed}");
        assert!(
            !printed.contains("ACM-12\""),
            "the input prints only as its size: {printed}"
        );
    }
}

/// A one-off program for a failing `info`: `script` is the shell after the shebang.
fn script_of(dir: &Path, script: &str) -> PathBuf {
    let program = dir.join("failing-info.sh");
    std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).expect("write script");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    program
}

#[test]
fn a_failing_info_masks_a_field_it_declares_secret_in_its_error_object() {
    let dir = tempfile::tempdir().expect("tempdir");
    let program = script_of(
        dir.path(),
        r#"printf '{"error":{"code":"auth","message":"rejected %s"},"credentials":[{"name":"token","label":"Token"}]}\n' "$GHOSTEX_CREDENTIAL_TOKEN"
exit 1"#,
    );
    let env = env_with_open_token("unused");
    let failure = command_info(&call_of(
        &program,
        dir.path(),
        CommandVerb::Info,
        None,
        &env,
    ))
    .expect_err("it exited 1");
    let shown = format!("{} {failure:?}", failure.message("Fixture"));
    assert!(!shown.contains(TOKEN), "{shown}");
    assert_eq!(
        failure.message("Fixture"),
        "Fixture: rejected ••••. Check its credentials in Settings."
    );
}

#[test]
fn a_failing_info_hides_its_stderr_once_it_declares_a_field_secret() {
    let dir = tempfile::tempdir().expect("tempdir");
    let program = script_of(
        dir.path(),
        r#"printf '{"credentials":[{"name":"token","label":"Token"}]}\n'
echo "can't reach the server with $GHOSTEX_CREDENTIAL_TOKEN" >&2
exit 3"#,
    );
    let env = env_with_open_token("unused");
    let failure = command_info(&call_of(
        &program,
        dir.path(),
        CommandVerb::Info,
        None,
        &env,
    ))
    .expect_err("it exited 3");
    let shown = format!("{} {failure:?}", failure.message("Fixture"));
    assert!(!shown.contains(TOKEN), "{shown}");
    assert!(
        failure
            .message("Fixture")
            .ends_with("is hidden because it could show a credential."),
        "{shown}"
    );
    // A `list` is not `info`: its stdout declares nothing, so the accepted flags apply.
    let input = list_input(&[], None);
    let list = run_command(&call_of(
        &program,
        dir.path(),
        CommandVerb::OpenList,
        Some(&input),
        &env,
    ))
    .expect_err("it exited 3");
    assert!(
        list.message("Fixture").contains(TOKEN),
        "{}",
        list.message("Fixture")
    );
}
