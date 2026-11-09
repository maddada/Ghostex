use crate::ghostex_cli::{
    args::Flags,
    rpc::{CliError, CliResult},
};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Delivery {
    Normal,
    Interrupt,
    Queue,
}

pub(super) struct Arguments {
    pub command: String,
    pub positional: Vec<String>,
    pub body_file: Option<String>,
    pub task: Option<String>,
    pub title: Option<String>,
    pub project_id: Option<String>,
    /// `--path <folder>`: create adds the folder as a project and starts the session there.
    pub path: Option<String>,
    pub delivery: Delivery,
    /// `--request-id`: the send's id, reused by a caller that retries it.
    pub request_id: Option<String>,
    pub all: bool,
    /// `--allow-hidden`: create an agent that is turned off in Settings > Agents.
    pub allow_hidden: bool,
    /// `--standalone`: an orchestrator's create leaves the new session unlinked (not its thread).
    pub standalone: bool,
    /// `--account`: the account number, email or id switch-account moves the session to.
    pub account: Option<String>,
    /// `--force`: switch-account interrupts a running turn and sends "." after the switch.
    pub force: bool,
    pub json: bool,
    pub help: bool,
    pub flags: Flags,
}

pub(super) fn parse(args: &[String]) -> CliResult<Arguments> {
    let mut parsed = Arguments {
        command: String::new(),
        positional: Vec::new(),
        body_file: None,
        task: None,
        title: None,
        project_id: None,
        path: None,
        delivery: Delivery::Normal,
        request_id: None,
        all: false,
        allow_hidden: false,
        standalone: false,
        account: None,
        force: false,
        json: false,
        help: args.is_empty(),
        flags: Flags::default(),
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => {
                parsed.positional.extend(args.cloned());
                break;
            }
            "-h" | "--help" => parsed.help = true,
            "--json" => parsed.json = true,
            "--all" => parsed.all = true,
            "--allow-hidden" => parsed.allow_hidden = true,
            "--standalone" => parsed.standalone = true,
            "--force" => parsed.force = true,
            "--interrupt" | "--queue" => {
                if parsed.delivery != Delivery::Normal {
                    return Err(CliError::Other(
                        "Choose only one delivery flag: --interrupt or --queue.".into(),
                    ));
                }
                parsed.delivery = if arg == "--interrupt" {
                    Delivery::Interrupt
                } else {
                    Delivery::Queue
                };
            }
            "--body-file" | "--server" | "--computer" | "--task" | "--title" | "--project-id"
            | "--path" | "--request-id" | "--account" => {
                let value = args
                    .next()
                    .filter(|value| !value.starts_with("--") && !value.is_empty())
                    .ok_or_else(|| CliError::Other(format!("{arg} requires a value.")))?;
                let slot = match arg.as_str() {
                    "--body-file" => Some(&mut parsed.body_file),
                    "--task" => Some(&mut parsed.task),
                    "--title" => Some(&mut parsed.title),
                    "--project-id" => Some(&mut parsed.project_id),
                    "--path" => Some(&mut parsed.path),
                    "--request-id" => Some(&mut parsed.request_id),
                    "--account" => Some(&mut parsed.account),
                    _ => None,
                };
                if let Some(slot) = slot {
                    if slot.replace(value.clone()).is_some() {
                        return Err(CliError::Other(format!("Provide {arg} only once.")));
                    }
                } else if arg == "--computer" {
                    parsed.flags.insert_text("computer", value);
                } else {
                    parsed.flags.insert_text("server", value);
                }
            }
            value if value.starts_with('-') => {
                return Err(CliError::Other(format!(
                    "Unknown option: {value}. See ghostex agents --help."
                )))
            }
            value if parsed.command.is_empty() => parsed.command = value.to_owned(),
            value => parsed.positional.push(value.to_owned()),
        }
    }
    if parsed.help {
        return Ok(parsed);
    }
    if parsed.request_id.is_some() && parsed.command != "send" {
        return Err(CliError::Other(
            "--request-id is only available for send.".into(),
        ));
    }
    if (parsed.account.is_some() || parsed.force) && parsed.command != "switch-account" {
        return Err(CliError::Other(
            "--account and --force are only available for switch-account.".into(),
        ));
    }
    if parsed
        .request_id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty() || id.chars().count() > 128)
    {
        return Err(CliError::Other(
            "--request-id must be 1 to 128 characters.".into(),
        ));
    }
    if parsed.command != "create"
        && (parsed.task.is_some()
            || parsed.title.is_some()
            || parsed.project_id.is_some()
            || parsed.path.is_some()
            || parsed.allow_hidden
            || parsed.standalone)
    {
        return Err(CliError::Other(
            "--task, --title, --project-id, --path, --allow-hidden, and --standalone are only available for create."
                .into(),
        ));
    }
    match parsed.command.as_str() {
        "whoami" | "list" | "types" => {
            if !parsed.positional.is_empty()
                || parsed.body_file.is_some()
                || parsed.delivery != Delivery::Normal
            {
                return Err(CliError::Other(format!(
                    "{} does not accept message arguments.",
                    parsed.command
                )));
            }
            let elsewhere = parsed.flags.contains("server") || parsed.flags.contains("computer");
            if parsed.command == "whoami" && (parsed.all || elsewhere) {
                return Err(CliError::Other(
                    "whoami identifies the caller; --all, --server and --computer are not accepted."
                        .into(),
                ));
            }
            if parsed.command == "list" && elsewhere && !parsed.all {
                return Err(CliError::Other(
                    "Use list --all --computer <name> to list another computer's agents.".into(),
                ));
            }
            if parsed.command == "types" && parsed.all {
                return Err(CliError::Other("--all is only available for list.".into()));
            }
        }
        "send" => {
            if parsed.all {
                return Err(CliError::Other("--all is only available for list.".into()));
            }
            let count = if parsed.body_file.is_some() { 1 } else { 2 };
            if parsed.positional.len() != count {
                return Err(CliError::Other("Usage: ghostex agents send <session-ref> <text> OR <session-ref> --body-file <path>. Quote inline text as one argument.".into()));
            }
        }
        "create" | "close" => {
            if parsed.positional.len() != 1 || parsed.all || parsed.delivery != Delivery::Normal {
                return Err(CliError::Other(format!(
                    "Usage: ghostex agents {} <{}>. See ghostex agents --help.",
                    parsed.command,
                    if parsed.command == "create" {
                        "agent-id"
                    } else {
                        "session-ref"
                    }
                )));
            }
            if parsed.command == "close" && parsed.body_file.is_some() {
                return Err(CliError::Other("close does not accept a message.".into()));
            }
            if parsed.command == "create" && parsed.body_file.is_some() && parsed.task.is_some() {
                return Err(CliError::Other(
                    "Choose --task or --body-file, not both.".into(),
                ));
            }
            if parsed.command == "create"
                && parsed.flags.contains("server")
                && parsed.project_id.is_none()
            {
                return Err(CliError::Other(
                    "Creating on --server requires --project-id.".into(),
                ));
            }
            if parsed.command == "create"
                && parsed.flags.contains("computer")
                && parsed.project_id.is_none()
            {
                return Err(CliError::Other(
                    "Creating on another computer needs --project-id: one of its projects (ghostex sessions --computer <name> lists them with their ids).".into(),
                ));
            }
        }
        "switch-account" | "reload" => {
            if parsed.positional.len() != 1
                || parsed.all
                || parsed.body_file.is_some()
                || parsed.delivery != Delivery::Normal
            {
                return Err(CliError::Other(format!(
                    "Usage: ghostex agents {}. See ghostex agents --help.",
                    if parsed.command == "reload" {
                        "reload <session-ref>"
                    } else {
                        "switch-account <session-ref> --account <number|email> [--force]"
                    }
                )));
            }
            if parsed.command == "switch-account" && parsed.account.is_none() {
                return Err(CliError::Other(
                    "switch-account requires --account <number|email>. Run ghostex accounts to see them.".into(),
                ));
            }
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown agents command: {other}. See ghostex agents --help."
            )))
        }
    }
    Ok(parsed)
}
