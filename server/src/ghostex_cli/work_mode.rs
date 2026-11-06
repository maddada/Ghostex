//! `ghostex issues` (Issues Workflow; `ghostex work-mode` is its old name and still works) and
//! `ghostex link-session` (crate::work_mode): the Linear key, the primary tracker, creating a
//! ticket, starting work on a ticket, the Clean up answer, and a session's hand-set links.

use std::io::{BufRead, IsTerminal, Write};

use serde_json::{json, Map, Value};

use super::actions::with_resolved_gxserver_session_params;
use super::args::parse_args;
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

/// `ghostex issues status|linear-key|source|tracker|create-ticket|start|cleanup …`. `on|off` are left from
/// the retired per-project switch: they change nothing and print what the project has.
pub(super) fn work_mode_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let subcommand = parsed.rest.first().map(String::as_str).unwrap_or("status");
    match subcommand {
        "on" | "off" => {
            let mut params = project_selector(flags);
            params.insert("enabled".to_string(), json!(subcommand == "on"));
            let result =
                rpc::call_gxserver_rpc("/api/setProjectWorkMode", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        "status" => {
            let result = rpc::call_gxserver_rpc("/api/readWorkModeStatus", &json!({}), flags)?;
            print_json(&result);
            Ok(())
        }
        "linear-key" => {
            let mut params = Map::new();
            if let Some(project_id) = flags.string_value("projectId") {
                params.insert("projectId".to_string(), json!(project_id));
            }
            if !flags.truthy("clear") {
                params.insert("apiKey".to_string(), json!(read_secret_line()?));
            }
            let result =
                rpc::call_gxserver_rpc("/api/setLinearApiKey", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        "create-ticket" => create_ticket(&parsed.rest[1..], flags),
        "tracker" => tracker_command(parsed.rest.get(1).map(String::as_str), flags),
        "source" => super::work_source::work_source_command(&parsed.rest[1..], flags),
        "start" => {
            let ticket = parsed
                .rest
                .get(1)
                .map(String::as_str)
                .or_else(|| flags.string_value("pr"))
                // `start --cloud SPX-1245`: the parser gave the ticket to the flag.
                .or_else(|| flags.string_value("cloud"))
                .ok_or_else(|| {
                    CliError::Other("Pass the ticket: ghostex issues start SPX-1245 (or #218, or --pr 412).".to_string())
                })?;
            let ticket = if flags.contains("pr") && !ticket.contains("/pull/") {
                format!("pr:{}", ticket.trim_start_matches('#'))
            } else {
                ticket.to_string()
            };
            let result = start_work(&ticket, flags.string_value("source"), flags)?;
            print_json(&result);
            Ok(())
        }
        "cleanup" => answer_cleanup(&parsed.rest[1..], flags),
        other => Err(CliError::Other(format!(
            "Unknown issues command \"{other}\". Use status, linear-key, source, tracker, create-ticket, start or cleanup."
        ))),
    }
}

/// `ghostex issues tracker [linear|github|<command id>] [--workspace W | --project-id id | --path p]`:
/// the workspace's primary tracker (Linear tickets & projects, GitHub issues & projects, or its
/// ticket command), or sets it; gxserver says which values it takes. A team workspace's choice is
/// the team's, which only its owners can change, and it can't be a ticket command.
fn tracker_command(value: Option<&str>, flags: &super::args::Flags) -> CliResult<()> {
    let params = match flags.string_value("workspace") {
        Some(workspace) => {
            let mut params = Map::new();
            params.insert("workspaceId".to_string(), json!(workspace));
            params
        }
        None => project_selector(flags),
    };
    let current = rpc::call_gxserver_rpc(
        "/api/readWorkTracker",
        &Value::Object(params.clone()),
        &with_default_timeout(flags, "30000"),
    )?;
    let Some(value) = value else {
        print_json(&current);
        return Ok(());
    };
    let result = rpc::call_gxserver_rpc(
        "/api/setWorkTracker",
        &set_tracker_params(&current, value),
        &with_default_timeout(flags, "30000"),
    )?;
    print_json(&result);
    Ok(())
}

/// `/api/setWorkTracker`'s request: the workspace `/api/readWorkTracker` resolved, and the value as
/// typed, which gxserver checks (`linear`, `github`, or the workspace's ticket command id).
fn set_tracker_params(current: &Value, value: &str) -> Value {
    json!({
        "workspaceId": current.get("workspaceId").cloned().unwrap_or(Value::Null),
        "tracker": value,
    })
}

/// `ghostex issues create-ticket --title T [--description D] [--source id] [--team-id id]
/// [--linear-project-id id] [--no-assign] [--start [--agent id] [--model m] [--effort e]]`. In a
/// workspace whose primary tracker is GitHub it creates a GitHub issue in the project's repo
/// instead (the Linear flags are ignored); where it is a ticket command, or `--source` names the
/// workspace's command, one of that command's tickets (`create_command_ticket`).
fn create_ticket(rest: &[String], flags: &super::args::Flags) -> CliResult<()> {
    let title = flags
        .string_value("title")
        .map(str::to_string)
        .or_else(|| (!rest.is_empty()).then(|| rest.join(" ")))
        .ok_or_else(|| CliError::Other("Pass --title \"…\".".to_string()))?;
    let mut params = project_selector(flags);
    let tracker = rpc::call_gxserver_rpc(
        "/api/readWorkTracker",
        &Value::Object(params.clone()),
        &with_default_timeout(flags, "30000"),
    )?;
    let command = command_source(
        flags.string_value("source"),
        tracker.get("tracker").and_then(Value::as_str),
    );
    if let Some(source) = command {
        return create_command_ticket(&source, &title, params, flags);
    }
    if tracker.get("tracker").and_then(Value::as_str) == Some("github") {
        params.insert("title".to_string(), json!(title));
        if let Some(value) = flags.string_value("description") {
            params.insert("description".to_string(), json!(value));
        }
        params.insert("assignToMe".to_string(), json!(!flags.truthy("noAssign")));
        let created = rpc::call_gxserver_rpc(
            "/api/createGithubIssue",
            &Value::Object(params),
            &with_default_timeout(flags, "60000"),
        )?;
        if !flags.truthy("start") {
            print_json(&created);
            return Ok(());
        }
        let number = created
            .get("number")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                CliError::Other("GitHub did not say which issue it created.".to_string())
            })?;
        let started = start_work(&number.to_string(), Some("github"), flags)?;
        print_json(&json!({ "ticket": created, "started": started }));
        return Ok(());
    }
    params.insert("title".to_string(), json!(title));
    for (flag, key) in [
        ("description", "description"),
        ("teamId", "teamId"),
        ("linearProjectId", "linearProjectId"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    params.insert("assignToMe".to_string(), json!(!flags.truthy("noAssign")));
    let created = rpc::call_gxserver_rpc(
        "/api/createLinearIssue",
        &Value::Object(params),
        &with_default_timeout(flags, "30000"),
    )?;
    if !flags.truthy("start") {
        print_json(&created);
        return Ok(());
    }
    let identifier = created
        .get("identifier")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::Other("Linear did not say which ticket it created.".to_string())
        })?;
    let started = start_work(identifier, Some("linear"), flags)?;
    print_json(&json!({ "ticket": created, "started": started }));
    Ok(())
}

/// How long the CLI waits for `/api/createWorkTicket`: past the longest a create may take there
/// (`CREATE_WRITER_WAIT`, its 60 s, `CREATE_CLEANUP`), so it never gives up while a create it sent
/// can still be running.
const CREATE_WORK_TICKET_TIMEOUT_MS: u64 = 90_000;

/// One of the ticket command `source`'s tickets, through `/api/createWorkTicket`, which runs its
/// `create` once. A create that ran and failed answers its words, which this prints as an error;
/// `--start` starts work only on a ticket that answered with its id, never after a timeout.
fn create_command_ticket(
    source: &str,
    title: &str,
    mut params: Map<String, Value>,
    flags: &super::args::Flags,
) -> CliResult<()> {
    if let Some(refused) = linear_flag_refusal(|flag| flags.string_value(flag).is_some()) {
        return Err(CliError::Other(refused));
    }
    params.insert("source".to_string(), json!(source));
    params.insert("title".to_string(), json!(title));
    if let Some(value) = flags.string_value("description") {
        params.insert("description".to_string(), json!(value));
    }
    params.insert("assignToMe".to_string(), json!(!flags.truthy("noAssign")));
    let created = rpc::call_gxserver_rpc(
        "/api/createWorkTicket",
        &Value::Object(params),
        &with_default_timeout(flags, &CREATE_WORK_TICKET_TIMEOUT_MS.to_string()),
    )?;
    let id = created_ticket_id(&created)?;
    if !flags.truthy("start") {
        print_json(&created);
        return Ok(());
    }
    let started = start_work(&id, Some(source), flags)?;
    print_json(&json!({ "ticket": created, "started": started }));
    Ok(())
}

/// The ticket command a create goes to: `--source`, else the workspace's tracker when that is
/// one (neither Linear nor GitHub).
fn command_source(source: Option<&str>, tracker: Option<&str>) -> Option<String> {
    source
        .or_else(|| tracker.filter(|tracker| !matches!(*tracker, "linear" | "github")))
        .map(str::to_string)
}

/// Why a ticket command's create is refused when it is given a Linear flag: the command picks
/// where its tickets go, so the flag is refused, not ignored. Fixed words, never `--source`,
/// which gxserver hasn't checked yet and could be a value the workspace stores.
fn linear_flag_refusal(given: impl Fn(&str) -> bool) -> Option<String> {
    [
        ("teamId", "--team-id"),
        ("linearProjectId", "--linear-project-id"),
    ]
    .into_iter()
    .find(|(flag, _)| given(flag))
    .map(|(_, written)| format!("A ticket command picks where its tickets go; drop {written}."))
}

/// The id of the ticket `/api/createWorkTicket` answered with, else its failure's words.
fn created_ticket_id(created: &Value) -> CliResult<String> {
    if let Some(error) = created.get("error").and_then(Value::as_str) {
        return Err(CliError::Other(error.to_string()));
    }
    created
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            CliError::Other("The ticket command did not say which ticket it created.".to_string())
        })
}

/// Starts an agent on a ticket (`SPX-1245`, `#218` / `218` for a GitHub issue, `pr:412` or a PR
/// link for a pull request) in a worktree on the ticket's branch, linked to it, with nothing sent
/// to the agent. With `--cloud [--provider claude-code] [--prompt-file path]` it starts a cloud
/// session on the ticket's branch instead (`/api/startCloudWork`, the Work page's Start in cloud),
/// whose task is the file's text or the one Ghostex drafts from the ticket.
fn start_work(ticket: &str, source: Option<&str>, flags: &super::args::Flags) -> CliResult<Value> {
    let mut params = project_selector(flags);
    params.extend(start_ticket_params(ticket, source));
    if flags.truthy("cloud") {
        params.insert(
            "provider".to_string(),
            json!(flags.string_value("provider").unwrap_or("claude-code")),
        );
        if let Some(path) = flags.string_value("promptFile") {
            let text = std::fs::read_to_string(path)
                .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")))?;
            params.insert("prompt".to_string(), json!(text));
        }
        // The cloud start alone may take up to three minutes (crate::cloud_runner).
        return rpc::call_gxserver_rpc(
            "/api/startCloudWork",
            &Value::Object(params),
            &with_default_timeout(flags, "240000"),
        );
    }
    for (flag, key) in [
        ("agent", "agentId"),
        ("model", "model"),
        ("effort", "effort"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    rpc::call_gxserver_rpc(
        "/api/startWorkOnTicket",
        &Value::Object(params),
        &with_default_timeout(flags, "180000"),
    )
}

/// What names the ticket in a start request: a PR, or a ticket ID with `source` only when one was
/// given (`--source <id>`, or the tracker a ticket was just created in). Without it gxserver reads
/// the ID in the project's workspace, its ticket command first when that is the tracker.
fn start_ticket_params(ticket: &str, source: Option<&str>) -> serde_json::Map<String, Value> {
    let mut params = serde_json::Map::new();
    let ticket = ticket.trim();
    if let Some(number) = ticket.strip_prefix("pr:") {
        params.insert("pullRequest".to_string(), json!(number));
    } else if ticket.contains("/pull/") {
        params.insert("pullRequest".to_string(), json!(ticket));
    } else {
        if let Some(source) = source {
            params.insert("source".to_string(), json!(source));
        }
        params.insert("ticketId".to_string(), json!(ticket));
    }
    params
}

/// `ghostex issues cleanup <session> clean-up|keep`: answers the Clean up / Keep offer a
/// session gets when its linked PR is merged, as the card's chips do (`/api/answerWorkCleanup`).
fn answer_cleanup(rest: &[String], flags: &super::args::Flags) -> CliResult<()> {
    let (selector, answer) =
        match rest {
            [answer] => (None, answer),
            [selector, answer] => (Some(selector), answer),
            _ => return Err(CliError::Other(
                "Pass the session and the answer: ghostex issues cleanup <session> clean-up|keep."
                    .to_string(),
            )),
        };
    let answer = match answer.trim().to_ascii_lowercase().as_str() {
        "clean-up" | "cleanup" => "cleanUp",
        "keep" => "keep",
        _ => {
            return Err(CliError::Other(
                "The answer is clean-up or keep.".to_string(),
            ))
        }
    };
    let mut payload = Map::new();
    if let Some(selector) = flags
        .string_value("sessionId")
        .map(str::to_string)
        .or_else(|| selector.cloned())
    {
        payload.insert("sessionId".to_string(), json!(selector));
    }
    let mut params = with_resolved_gxserver_session_params(&Value::Object(payload), flags)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    params.insert("answer".to_string(), json!(answer));
    // Cleaning up removes the session's worktree, which can outlast the CLI's 15s default.
    let result = rpc::call_gxserver_rpc(
        "/api/answerWorkCleanup",
        &Value::Object(params),
        &with_default_timeout(flags, "60000"),
    )?;
    print_json(&result);
    Ok(())
}

/// Linear calls take a few seconds, and starting work fetches the branch, cuts the worktree, runs
/// the project's setup command and starts the agent: longer than the CLI's 15s default.
pub(super) fn with_default_timeout(
    flags: &super::args::Flags,
    timeout_ms: &str,
) -> super::args::Flags {
    let mut flags = flags.clone();
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", timeout_ms);
    }
    flags
}

/// `ghostex link-session <selector> [--pr N|URL|none] [--linear SPX-1,SPX-2|none]
/// [--issue N|none] [--ticket ID:TICKET,ID:TICKET|ID:none] [--linear-project NAME|none]
/// [--github-project OWNER/NUMBER|none] [--auto]`, or
/// `ghostex link-session <selector> --candidates
/// pullRequest|linearIssue|linearProject|githubIssue|githubProject [--query text]` to list what the
/// Link to picker suggests.
pub(super) fn link_session_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let mut payload = Map::new();
    if let Some(selector) = flags
        .string_value("sessionId")
        .map(str::to_string)
        .or_else(|| parsed.rest.first().cloned())
    {
        payload.insert("sessionId".to_string(), json!(selector));
    }
    let mut params = with_resolved_gxserver_session_params(&Value::Object(payload), flags)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    // CDXC:WorkMode 2026-10-09 SEE-ALSO: the phone's Link to picker (apps/mobile/app/src/screens/sessions-screen/work-link-picker.tsx) reads the same suggestions the desktop picker does through this flag, and saves through the flags below.
    if let Some(kind) = flags.string_value("candidates") {
        params.insert("kind".to_string(), json!(kind));
        if let Some(query) = flags.string_value("query") {
            params.insert("query".to_string(), json!(query));
        }
        let result = rpc::call_gxserver_rpc(
            "/api/listWorkLinkCandidates",
            &Value::Object(params),
            &with_default_timeout(flags, "30000"),
        )?;
        print_json(&result);
        return Ok(());
    }
    if flags.truthy("auto") {
        params.insert("clear".to_string(), json!(true));
    }
    for (flag, key) in [
        ("pr", "pullRequest"),
        ("linear", "linearIssues"),
        ("issue", "githubIssues"),
        ("linearProject", "linearProject"),
        ("githubProject", "githubProject"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            // `none` unlinks. The other kinds read it themselves; a Linear project is a name, so
            // the server takes the empty string for "explicitly none" (what the desktop's Unlink sends).
            let value = if key == "linearProject" && value.trim().eq_ignore_ascii_case("none") {
                ""
            } else {
                value
            };
            params.insert(key.to_string(), json!(value));
        }
    }
    if let Some(value) = flags.string_value("ticket") {
        params.insert("tickets".to_string(), ticket_links_param(value)?);
    }
    if !params.contains_key("clear")
        && ![
            "pullRequest",
            "linearIssues",
            "githubIssues",
            "tickets",
            "linearProject",
            "githubProject",
        ]
        .iter()
        .any(|key| params.contains_key(*key))
    {
        return Err(CliError::Other(
            "Pass --pr, --linear, --issue, --ticket, --linear-project, --github-project or --auto."
                .to_string(),
        ));
    }
    let result = rpc::call_gxserver_rpc("/api/setSessionWorkLinks", &Value::Object(params), flags)?;
    print_json(&result);
    Ok(())
}

/// `--ticket`'s `tickets`: each ticket command's id to the ticket IDs it sets, from
/// `fixture:ACM-14,fixture:ACM-15`; `fixture:none` sets none, as `--linear none` does. gxserver
/// checks each id.
fn ticket_links_param(value: &str) -> CliResult<Value> {
    let refused = || {
        CliError::Other(
            "--ticket takes <command id>:<ticket ID>, like fixture:ACM-14, or <command id>:none."
                .to_string(),
        )
    };
    let mut set: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    let mut none: std::collections::BTreeSet<&str> = Default::default();
    for part in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let (id, ticket) = part.split_once(':').ok_or_else(refused)?;
        let (id, ticket) = (id.trim(), ticket.trim());
        if id.is_empty() || ticket.is_empty() {
            return Err(refused());
        }
        if ticket.eq_ignore_ascii_case("none") {
            none.insert(id);
        } else {
            set.entry(id).or_default().push(ticket);
        }
    }
    // `none` stands alone: beside ticket IDs of the same command, it can't say which was meant.
    if (set.is_empty() && none.is_empty()) || none.iter().any(|id| set.contains_key(id)) {
        return Err(refused());
    }
    let mut tickets = Map::new();
    for (id, ids) in set {
        tickets.insert(id.to_string(), json!(ids));
    }
    for id in none {
        tickets.insert(id.to_string(), json!([]));
    }
    Ok(Value::Object(tickets))
}

/// `--project-id`, else `--path`, else the current folder.
fn project_selector(flags: &super::args::Flags) -> Map<String, Value> {
    let mut params = Map::new();
    if let Some(project_id) = flags.string_value("projectId") {
        params.insert("projectId".to_string(), json!(project_id));
    } else if let Some(path) = flags.string_value("path") {
        params.insert("path".to_string(), json!(path));
    } else if let Ok(cwd) = std::env::current_dir() {
        params.insert("path".to_string(), json!(cwd.to_string_lossy()));
    }
    params
}

/// Reads the key from stdin, so it never lands in shell history or the process list.
fn read_secret_line() -> CliResult<String> {
    read_stdin_line(
        "Paste your Linear API key and press Enter: ",
        "the key",
        "No key was given. Pipe it in, or pass --clear to remove the key.".to_string(),
    )
}

/// One line from stdin, trimmed, for a value that must not sit in the shell's history (a Linear
/// key, a ticket command's credential). `prompt` asks for it when stdin is a terminal; `what`
/// names it in a read error; `missing` is the error for a blank line.
pub(crate) fn read_stdin_line(prompt: &str, what: &str, missing: String) -> CliResult<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
    }
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|error| CliError::Other(format!("Could not read {what}: {error}")))?;
    let value = line.trim().to_string();
    if value.is_empty() {
        return Err(CliError::Other(missing));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        command_source, created_ticket_id, linear_flag_refusal, set_tracker_params,
        start_ticket_params, ticket_links_param, CREATE_WORK_TICKET_TIMEOUT_MS,
    };

    #[test]
    fn a_ticket_command_id_is_sent_as_typed() {
        assert_eq!(
            set_tracker_params(
                &json!({ "workspaceId": "ws-1", "tracker": "github" }),
                "fixture"
            ),
            json!({ "workspaceId": "ws-1", "tracker": "fixture" })
        );
    }

    /// A create that ran and failed, a timeout included, gives no id, so `--start` never runs
    /// after it; a created ticket's id is what it starts on.
    #[test]
    fn create_ticket_never_starts_after_a_failed_create() {
        let timed_out = json!({
            "source": "fixture",
            "error": "Fixture did not answer within 60 s. The ticket may have been created, so check Fixture before trying again.",
            "detail": null,
            "mayHaveBeenCreated": true,
        });
        assert!(created_ticket_id(&timed_out)
            .is_err_and(|error| error.to_string().contains("may have been created")));
        assert_eq!(
            created_ticket_id(&json!({ "source": "fixture", "id": "ACM-41" })).unwrap(),
            "ACM-41"
        );
    }

    /// The CLI waits past the longest a create may take in gxserver.
    #[test]
    fn the_create_rpc_outlives_the_commands_timeout() {
        let budget = crate::work_mode::CREATE_WRITER_WAIT
            + crate::work_mode::CommandVerb::Create.timeout()
            + crate::work_mode::CREATE_CLEANUP;
        assert!(budget < std::time::Duration::from_millis(CREATE_WORK_TICKET_TIMEOUT_MS));
    }

    /// A create goes to the ticket command `--source` names, else to the tracker when it is one.
    #[test]
    fn a_create_goes_to_the_named_command_or_the_command_tracker() {
        assert_eq!(
            command_source(None, Some("fixture")).as_deref(),
            Some("fixture")
        );
        assert_eq!(command_source(None, Some("linear")), None);
        assert_eq!(command_source(None, Some("github")), None);
        assert_eq!(
            command_source(Some("fixture"), Some("linear")).as_deref(),
            Some("fixture")
        );
        assert_eq!(command_source(None, None), None);
    }

    /// Linear's own flags are refused for a ticket command's create, in fixed words.
    #[test]
    fn a_command_create_refuses_the_linear_flags() {
        assert_eq!(
            linear_flag_refusal(|flag| flag == "teamId").as_deref(),
            Some("A ticket command picks where its tickets go; drop --team-id.")
        );
        assert_eq!(
            linear_flag_refusal(|flag| flag == "linearProjectId").as_deref(),
            Some("A ticket command picks where its tickets go; drop --linear-project-id.")
        );
        assert_eq!(linear_flag_refusal(|_| false), None);
    }

    /// A ticket ID goes without a source unless one is named, so gxserver reads it in the
    /// project's workspace; a PR goes as a PR.
    #[test]
    fn start_sends_a_source_only_when_one_is_named() {
        let params = |ticket: &str, source: Option<&str>| {
            serde_json::Value::Object(start_ticket_params(ticket, source))
        };
        assert_eq!(params("ACM-12", None), json!({ "ticketId": "ACM-12" }));
        assert_eq!(params("#218", None), json!({ "ticketId": "#218" }));
        assert_eq!(
            params("ACM-12", Some("fixture")),
            json!({ "source": "fixture", "ticketId": "ACM-12" })
        );
        assert_eq!(
            params("pr:412", Some("fixture")),
            json!({ "pullRequest": "412" })
        );
    }

    /// `--ticket` sets each command's ticket IDs; `<id>:none` sets none, and can't stand beside a
    /// ticket ID of the same command.
    #[test]
    fn link_session_ticket_sets_each_commands_tickets() {
        assert_eq!(
            ticket_links_param("fixture:ACM-14, fixture:ACM-15").unwrap(),
            json!({ "fixture": ["ACM-14", "ACM-15"] })
        );
        assert_eq!(
            ticket_links_param("fixture:none").unwrap(),
            json!({ "fixture": [] })
        );
        for refused in [
            "",
            "ACM-14",
            "fixture:",
            ":ACM-14",
            "fixture:none,fixture:ACM-14",
        ] {
            assert!(ticket_links_param(refused).is_err(), "{refused}");
        }
    }
}
