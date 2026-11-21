use crate::ghostex_cli::{
    args::Flags,
    rpc::{CliError, CliResult},
};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Delivery {
    Normal,
    Interrupt,
    Queue,
    /// `--with-next`: held, and given to the recipient with the next message it gets.
    WithNext,
}

/// `--if-none`: what `send --main-coordinator` does when the project has no main coordinator.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum IfNone {
    /// Start the project's main coordinator and send it the message.
    Create,
    /// Start a regular session there with the message as its task; it reports to the caller.
    Session,
}

#[derive(Clone)]
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
    /// `send --main-coordinator`: the recipient is the main coordinator of `--project-id` or
    /// `--path` (on `--computer`), not a session ref (main_coordinator.rs).
    pub main_coordinator: bool,
    pub if_none: Option<IfNone>,
    /// `send --as-user`: the user's own terminal (or Grok Bot on this computer) sends as the user;
    /// refused inside any Ghostex session (identity.rs `sender`).
    pub as_user: bool,
    /// `--lead <session-ref|self>`: create and link make the session report to that lead
    /// (agents/lead_links in gxserver).
    pub lead: Option<String>,
    /// `--role <line>`: standing role line set by create or link (agents/standing_role).
    pub role: Option<String>,
    /// `--session <session-ref>`: target session for `agents role set|show|clear`.
    pub session: Option<String>,
    /// `create --model <model>` and `--effort <level>`: the new session's model and effort, as
    /// `ghostex create-agent` takes them (GitHub #700: a linked session on a chosen model in one
    /// step).
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `wait-reply --contains <text>`: wait for a reply holding this text (any case).
    pub contains: Option<String>,
    /// `wait-reply --timeout <seconds>`.
    pub timeout: Option<String>,
    /// `wait-reply --after <message-id|now>`: the reply must come after this row.
    pub after: Option<String>,
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
        main_coordinator: false,
        if_none: None,
        as_user: false,
        lead: None,
        role: None,
        session: None,
        model: None,
        effort: None,
        contains: None,
        timeout: None,
        after: None,
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
            "--main-coordinator" => parsed.main_coordinator = true,
            "--as-user" => parsed.as_user = true,
            "--if-none" => {
                let value = args.next().map(String::as_str).unwrap_or_default();
                parsed.if_none = Some(match value {
                    "create" => IfNone::Create,
                    "session" => IfNone::Session,
                    _ => {
                        return Err(CliError::Other(
                            "--if-none takes create (start the project's main coordinator) or session (start a regular session there that reports to you).".into(),
                        ))
                    }
                });
            }
            "--force" => parsed.force = true,
            "--interrupt" | "--queue" | "--with-next" => {
                if parsed.delivery != Delivery::Normal {
                    return Err(CliError::Other(
                        "Choose only one delivery flag: --interrupt, --queue or --with-next."
                            .into(),
                    ));
                }
                parsed.delivery = match arg.as_str() {
                    "--interrupt" => Delivery::Interrupt,
                    "--queue" => Delivery::Queue,
                    _ => Delivery::WithNext,
                };
            }
            "--body-file" | "--server" | "--computer" | "--task" | "--title" | "--project-id"
            | "--path" | "--request-id" | "--account" | "--lead" | "--role" | "--session"
            | "--contains" | "--timeout" | "--after" | "--model" | "--effort" => {
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
                    "--lead" => Some(&mut parsed.lead),
                    "--role" => Some(&mut parsed.role),
                    "--session" => Some(&mut parsed.session),
                    "--contains" => Some(&mut parsed.contains),
                    "--timeout" => Some(&mut parsed.timeout),
                    "--after" => Some(&mut parsed.after),
                    "--model" => Some(&mut parsed.model),
                    "--effort" => Some(&mut parsed.effort),
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
    if parsed.lead.is_some()
        && !matches!(parsed.command.as_str(), "create" | "link")
        && !(parsed.command == "unlink" && parsed.all)
    {
        return Err(CliError::Other(
            "--lead is only available for create, link and unlink --all.".into(),
        ));
    }
    if parsed.lead.is_some() && parsed.standalone {
        return Err(CliError::Other(
            "Choose --lead or --standalone, not both.".into(),
        ));
    }
    if parsed.lead.is_some() && parsed.flags.contains("computer") {
        return Err(CliError::Other(
            "Lead links work on this computer only: --lead can't go with --computer.".into(),
        ));
    }
    if parsed.role.is_some() && !matches!(parsed.command.as_str(), "create" | "link") {
        return Err(CliError::Other(
            "--role is only available for create and link. Use `ghostex agents role set \"<line>\"` to set a standing role directly.".into(),
        ));
    }
    if parsed.session.is_some() && parsed.command != "role" {
        return Err(CliError::Other(
            "--session is only available for role.".into(),
        ));
    }
    if (parsed.contains.is_some() || parsed.timeout.is_some() || parsed.after.is_some())
        && parsed.command != "wait-reply"
    {
        return Err(CliError::Other(
            "--contains, --timeout and --after are only available for wait-reply.".into(),
        ));
    }
    if parsed.as_user && parsed.command != "send" {
        return Err(CliError::Other(
            "--as-user is only available for send.".into(),
        ));
    }
    if (parsed.main_coordinator || parsed.if_none.is_some()) && parsed.command != "send" {
        return Err(CliError::Other(
            "--main-coordinator and --if-none are only available for send.".into(),
        ));
    }
    if parsed.if_none.is_some() && !parsed.main_coordinator {
        return Err(CliError::Other(
            "--if-none goes with --main-coordinator.".into(),
        ));
    }
    // --if-none starts a session with the message as its first request, which cannot wait for a
    // next one.
    if parsed.if_none.is_some() && parsed.delivery == Delivery::WithNext {
        return Err(CliError::Other(
            "--with-next can't go with --if-none: a session started for the message gets it as its first request. Send it without --with-next.".into(),
        ));
    }
    let main_send = parsed.command == "send" && parsed.main_coordinator;
    if main_send && (parsed.task.is_some() || parsed.allow_hidden || parsed.standalone) {
        return Err(CliError::Other(
            "send --main-coordinator takes the message as text or --body-file, plus --project-id or --path, --computer, --title and --if-none.".into(),
        ));
    }
    if parsed.command != "create"
        && !main_send
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
    if (parsed.model.is_some() || parsed.effort.is_some()) && parsed.command != "create" {
        return Err(CliError::Other(
            "--model and --effort are only available for create.".into(),
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
        "held" | "unhold" => {
            if parsed.body_file.is_some() || parsed.delivery != Delivery::Normal {
                return Err(CliError::Other(format!(
                    "{} does not accept a message or a delivery flag.",
                    parsed.command
                )));
            }
            if parsed.command == "held" && (parsed.positional.len() > 1 || parsed.all) {
                return Err(CliError::Other(
                    "Usage: ghostex agents held [<session-ref>].".into(),
                ));
            }
            if parsed.command == "unhold"
                && (parsed.positional.is_empty() || (parsed.positional.len() == 1) != parsed.all)
            {
                return Err(CliError::Other(
                    "Usage: ghostex agents unhold <session-ref> <request-id>... (or --all for every one you sent).".into(),
                ));
            }
        }
        "send" => {
            if parsed.all {
                return Err(CliError::Other("--all is only available for list.".into()));
            }
            if parsed.main_coordinator {
                let count = if parsed.body_file.is_some() { 0 } else { 1 };
                if parsed.positional.len() != count {
                    return Err(CliError::Other("Usage: ghostex agents send --main-coordinator --project-id <id>|--path <folder> [--computer <name>] [--if-none create|session] <text> (or --body-file <path>). Quote inline text as one argument.".into()));
                }
                return Ok(parsed);
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
        "link" | "unlink" | "links" => {
            if parsed.body_file.is_some() || parsed.delivery != Delivery::Normal {
                return Err(CliError::Other(format!(
                    "{} does not accept a message or a delivery flag. See ghostex agents link --help.",
                    parsed.command
                )));
            }
            let usage = match parsed.command.as_str() {
                "link" if parsed.positional.len() != 1 || parsed.lead.is_none() || parsed.all => {
                    Some("ghostex agents link <session-ref> --lead <session-ref|self>")
                }
                "unlink" if (parsed.positional.len() == 1) == parsed.all
                    || parsed.positional.len() > 1 =>
                {
                    Some("ghostex agents unlink <session-ref> (or --all for every session linked to you)")
                }
                "links" if !parsed.positional.is_empty() => Some("ghostex agents links [--all] [--json]"),
                _ => None,
            };
            if let Some(usage) = usage {
                return Err(CliError::Other(format!(
                    "Usage: {usage}. See ghostex agents link --help."
                )));
            }
        }
        "role" => {
            if parsed.all || parsed.body_file.is_some() || parsed.delivery != Delivery::Normal {
                return Err(CliError::Other(
                    "role does not accept --all, --body-file or a delivery flag. See ghostex agents role --help.".into(),
                ));
            }
            let valid = match parsed.positional.first().map(String::as_str) {
                Some("set") => parsed.positional.len() == 2,
                Some("show" | "clear") => parsed.positional.len() == 1,
                _ => false,
            };
            if !valid {
                return Err(CliError::Other(
                    "Usage: ghostex agents role set \"<line>\" [--session <ref>] | show [--session <ref>] | clear [--session <ref>]. See ghostex agents role --help.".into(),
                ));
            }
        }
        "wait-reply" => {
            if parsed.positional.len() != 1
                || parsed.all
                || parsed.body_file.is_some()
                || parsed.delivery != Delivery::Normal
            {
                return Err(CliError::Other(
                    "Usage: ghostex agents wait-reply <session-ref> [--contains <text>] [--timeout <seconds>] [--after <message-id>|now] [--json]. See ghostex agents --help.".into(),
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
