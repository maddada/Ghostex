//! `ghostex session-groups` and the verbs that create, rename and delete a project's session groups
//! and move a session into or out of one.
//!
//! CDXC:Sessions 2026-10-25 WHY: the Ghostex app owns the session groups document and gxserver keeps only its write-through copy, so these verbs ask the running app (the `sessionGroups` renderer command), the way `ghostex settings set` asks it to save a setting; an edit written straight to gxserver's copy was lost whenever the app had a push of its own outstanding. With the app not running, `session-groups` lists gxserver's copy and the edits fail.
//! SEE-ALSO: packages/gx-core/src/workspace_groups/verb_edits.rs, apps/desktop/src/app/gx_store/renderer_commands/perform.rs.

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::events::NO_RENDERER_MESSAGE;
use crate::ghostex_cli::args::{parse_args, FlagValue, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{self, call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::selector;

/// The sidebar's id for a user-made group: `gpui-wsg:<project id>:<group id>`.
const SIDEBAR_SUBGROUP_PREFIX: &str = "gpui-wsg:";
/// The sidebar's id for a project's own list: `combined-project:<project id>`.
const SIDEBAR_PROJECT_GROUP_PREFIX: &str = "combined-project:";
/// gxserver's id for a project's own list, the `groupId` every listed session carries (and the one
/// the phone's Create Terminal sends): `<project id>:active`.
const PROJECT_GROUP_SUFFIX: &str = ":active";
/// The groups document's key prefix for another computer's project (gx-core `keys.rs`).
const REMOTE_PROJECT_PREFIX: &str = "remote:";
/// The app's answer for a session its sidebar does not list
/// (`RendererCommandError::NoMatchingSession` in packages/gx-core/src/renderer_commands/errors.rs).
const SESSION_NOT_LISTED_MESSAGE: &str = "No matching session was found.";
/// How long a just-created session may take to reach the app's sidebar, and how often to ask.
const CREATED_SESSION_LISTING_WAIT: Duration = Duration::from_secs(10);
const CREATED_SESSION_LISTING_POLL: Duration = Duration::from_millis(250);
/// The CLI's time limit for one request when `--timeout` gives none (`request_gxserver_envelope`).
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_millis(15_000);

/// Where a `--group-id` points: a project, and the user-made group inside it unless the id named the
/// project's own list.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct GroupTarget {
    pub project_id: Option<String>,
    pub group_id: Option<String>,
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

/// `--group-id` as given: absent, or an id. A `--group-id` with no value or a blank one is refused
/// rather than read as no group, which would put the session in the project's own list.
pub(crate) fn group_id_arg(value: Option<&Value>) -> CliResult<Option<&str>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if !id.trim().is_empty() => Ok(Some(id)),
        _ => Err(CliError::Other(
            "--group-id needs a session group id (\"ghostex session-groups\" lists them)."
                .to_string(),
        )),
    }
}

fn not_a_group_id(raw: &str) -> CliError {
    CliError::Other(format!("\"{raw}\" is not a session group id."))
}

/// Reads a `--group-id` the way `ghostex session-groups` prints it (`group-2`, with the project
/// from `--project-id`), the way the sidebar names it (`gpui-wsg:<project>:group-2`,
/// `combined-project:<project>`), or the way `ghostex sessions --json` does (`<project>:active`). A
/// group of one project with `--project-id` naming another is refused, because a session only joins
/// a group of its own project.
pub(crate) fn resolve_group_target(
    group_id: Option<&str>,
    project_id: Option<&str>,
) -> CliResult<GroupTarget> {
    let project_id = non_empty(project_id);
    let Some(raw) = non_empty(group_id) else {
        return Ok(GroupTarget {
            project_id,
            group_id: None,
        });
    };
    let (named_project, group) = if let Some(rest) = raw.strip_prefix(SIDEBAR_SUBGROUP_PREFIX) {
        let Some((project, group)) = rest
            .split_once(':')
            .filter(|(project, group)| !project.is_empty() && !group.is_empty())
        else {
            return Err(not_a_group_id(&raw));
        };
        (Some(decode(project)), Some(group.to_string()))
    } else if let Some(project) = raw.strip_prefix(SIDEBAR_PROJECT_GROUP_PREFIX) {
        let Some(project) = non_empty(Some(&decode(project))) else {
            return Err(not_a_group_id(&raw));
        };
        (Some(project), None)
    } else if let Some(project) = raw
        .strip_suffix(PROJECT_GROUP_SUFFIX)
        .filter(|project| !project.is_empty() && !project.contains(':'))
    {
        (Some(project.to_string()), None)
    } else {
        (None, Some(raw))
    };
    if let (Some(named), Some(given)) = (&named_project, &project_id) {
        if named != given {
            return Err(CliError::Other(format!(
                "--group-id names a group of project {named}, not {given}. A session only joins a group of its own project."
            )));
        }
    }
    Ok(GroupTarget {
        project_id: named_project.or(project_id),
        group_id: group,
    })
}

/// Why the app did not answer: none was connected, or one was and did not answer in time (gxserver
/// gives both the `dependencyUnavailable` code; only the message tells them apart).
#[derive(PartialEq, Eq)]
enum NoAnswer {
    NotRunning,
    TimedOut,
}

fn no_answer(error: &CliError) -> Option<NoAnswer> {
    let CliError::Rpc { response, .. } = error else {
        return None;
    };
    let unavailable = ["error", "code"]
        .iter()
        .any(|field| response.get(*field).and_then(Value::as_str) == Some("dependencyUnavailable"));
    if !unavailable {
        return None;
    }
    Some(
        match response.get("message").and_then(Value::as_str) == Some(NO_RENDERER_MESSAGE) {
            true => NoAnswer::NotRunning,
            false => NoAnswer::TimedOut,
        },
    )
}

/// Asks the Ghostex app for one group verb. `Err` is a failure to ask; the app's own refusal comes
/// back as `{ ok: false, error }`.
fn ask_app_raw(payload: Map<String, Value>, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc(
        "/api/dispatchRendererCommand",
        &json!({ "action": "sessionGroups", "payload": payload }),
        flags,
    )
}

/// "No app" and "no answer" said plainly, for the op asked (`list`, `check`, or an edit). A late
/// answer may still have made an edit, so that one never says to retry; `list` and `check` change
/// nothing and can simply be asked again.
fn explain_no_answer(error: CliError, op: &str) -> CliError {
    let edits = !matches!(op, "list" | "check");
    match no_answer(&error) {
        Some(NoAnswer::NotRunning) => CliError::Other(
            "Session groups are edited by the Ghostex app, and it is not running on this computer. Open Ghostex and retry. On a computer without the app (a server another Ghostex connects to), use Move to Group in the sidebar of the Ghostex that connects to it."
                .to_string(),
        ),
        Some(NoAnswer::TimedOut) if edits => CliError::Other(
            "The Ghostex app did not answer in time, so the edit may or may not have been made. Check with \"ghostex session-groups\" before you retry."
                .to_string(),
        ),
        Some(NoAnswer::TimedOut) => CliError::Other(
            "The Ghostex app did not answer in time. Nothing was changed; try again in a moment."
                .to_string(),
        ),
        None => error,
    }
}

/// [`ask_app_raw`] with [`explain_no_answer`].
fn ask_app(payload: Map<String, Value>, flags: &Flags) -> CliResult<Value> {
    let op = payload
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    ask_app_raw(payload, flags).map_err(|error| explain_no_answer(error, &op))
}

fn request(op: &str, project_id: Option<&str>, group_id: Option<&str>) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("op".to_string(), json!(op));
    if let Some(project_id) = project_id {
        payload.insert("projectId".to_string(), json!(project_id));
    }
    if let Some(group_id) = group_id {
        payload.insert("groupId".to_string(), json!(group_id));
    }
    payload
}

fn answer_error(answer: &Value) -> String {
    answer
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("the Ghostex app refused the edit.")
        .to_string()
}

/// gxserver's copy cut to the rows `session-groups` prints, for when the app is not running. Like
/// the app's answer (`session_groups_json` in gx-core), it leaves out the groups the app keeps for
/// another computer's projects, keyed `remote:<machine>:project:<id>`.
fn copy_groups(document: &Value, project_id: Option<&str>) -> Vec<Value> {
    let Some(projects) = document.get("projects").and_then(Value::as_object) else {
        return Vec::new();
    };
    projects
        .iter()
        .filter(|(id, _)| !id.starts_with(REMOTE_PROJECT_PREFIX))
        .filter(|(id, _)| project_id.is_none_or(|wanted| wanted == id.as_str()))
        .flat_map(|(id, project)| {
            project
                .get("groups")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(move |group| {
                    json!({
                        "projectId": id,
                        "groupId": group.get("groupId"),
                        "title": group.get("title"),
                        "sessionIds": group.get("sessionIds"),
                    })
                })
        })
        .collect()
}

/// Fails unless the project holds this user-made group and it can take one more session, so a
/// create with `--group-id` is refused before it makes a session. The group can still go between
/// this check and the move, which the create then reports as its `groupError`.
pub(crate) fn require_existing_group(
    project_id: &str,
    group_id: &str,
    flags: &Flags,
) -> CliResult<()> {
    let answer = ask_app(request("check", Some(project_id), Some(group_id)), flags)?;
    if !is_failed_cli_result(&answer) {
        return Ok(());
    }
    Err(CliError::Other(format!(
        "Project {project_id} cannot take a new session in session group {group_id}: {} Run \"ghostex session-groups --project-id {project_id}\" to list its groups.",
        answer_error(&answer)
    )))
}

/// Puts a session this command just created in a group of its project. `Err` carries the reason
/// it was not moved.
///
/// The app answers "no matching session" for a session its sidebar does not list yet, and the
/// create's answer does not mean the app has received that row: gxserver queues the new row's
/// broadcast and hands the app this command directly. So that answer is asked again for up to
/// ten seconds, each request's answer held to what is left of them; the edit is idempotent. Any
/// other answer or failure ends the wait, since a request that did not answer may have made the
/// edit.
///
/// Each attempt goes through the CLI's ordinary request, which first checks a remote target's ssh
/// or tailcat tunnel (a probe of up to 600 ms or 3 s, longer when the tunnel has to be started
/// again; nothing for this computer) and brings it back if it closed. That time is outside the ten
/// seconds, as it is for every CLI request.
pub(crate) fn move_created_session_to_group(
    project_id: &str,
    session_id: &str,
    group_id: &str,
    flags: &Flags,
) -> Result<Value, String> {
    let per_request = flags
        .number("timeout")
        .or_else(|| flags.number("timeoutMs"))
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .map_or(DEFAULT_REQUEST_TIMEOUT, |ms| {
            Duration::from_millis(ms as u64)
        });
    let mut payload = request("move", Some(project_id), Some(group_id));
    payload.insert("sessionId".to_string(), json!(session_id));
    let mut params = json!({ "action": "sessionGroups", "payload": payload });
    let target =
        rpc::resolve_gxserver_server_target(flags, &params).map_err(|error| error.to_string())?;
    let mut attempt_flags = flags.clone();
    let deadline = Instant::now() + CREATED_SESSION_LISTING_WAIT;
    loop {
        let budget_ms = deadline
            .saturating_duration_since(Instant::now())
            .min(per_request)
            .as_millis();
        if budget_ms == 0 {
            return Err(format!(
                "the Ghostex app did not list the session in its sidebar within {} seconds.",
                CREATED_SESSION_LISTING_WAIT.as_secs()
            ));
        }
        attempt_flags.insert_text("timeout", &budget_ms.to_string());
        params["timeoutMs"] = json!(budget_ms);
        let answer = rpc::request_gxserver_rpc(
            &target,
            "/api/dispatchRendererCommand",
            &params,
            &attempt_flags,
        )
        .map_err(|error| explain_no_answer(error, "move").to_string())?;
        if !is_failed_cli_result(&answer) {
            return Ok(answer);
        }
        let error = answer_error(&answer);
        if error != SESSION_NOT_LISTED_MESSAGE {
            return Err(error);
        }
        std::thread::sleep(
            CREATED_SESSION_LISTING_POLL.min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

/// Prints the app's answer; a refusal exits 1.
fn print_answer(answer: Value) {
    let mut answer = answer;
    if let Some(object) = answer.as_object_mut() {
        object.remove("requestId");
    }
    if is_failed_cli_result(&answer) {
        crate::ghostex_cli::set_exit_code(1);
    }
    print_json(&answer);
}

fn require_project(target: &GroupTarget, verb: &str) -> CliResult<String> {
    target.project_id.clone().ok_or_else(|| {
        CliError::Other(format!(
            "{verb} needs --project-id, or a --group-id that names the project (gpui-wsg:<project>:<group>)."
        ))
    })
}

fn require_group(target: &GroupTarget, verb: &str) -> CliResult<String> {
    target.group_id.clone().ok_or_else(|| {
        CliError::Other(format!(
            "{verb} needs --group-id (\"ghostex session-groups\" lists them)."
        ))
    })
}

/// One of the session group verbs.
pub fn session_groups_command(verb: &str, args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let group_id = flags.0.get("groupId").map(FlagValue::as_json);
    let target = resolve_group_target(
        group_id_arg(group_id.as_ref())?,
        flags.string_value("projectId"),
    )?;
    // Passed as typed: the app trims a title and refuses a blank one or one with control characters.
    let title = flags.0.get("title").map(FlagValue::as_json);
    match verb {
        "session-groups" => {
            let project_id = target.project_id.as_deref();
            match ask_app_raw(request("list", project_id, None), flags) {
                Ok(answer) => print_answer(answer),
                Err(error) if no_answer(&error) == Some(NoAnswer::NotRunning) => {
                    let copy =
                        call_gxserver_rpc("/api/readWorkspaceSessionGroups", &json!({}), flags)?;
                    let groups =
                        copy_groups(copy.get("groups").unwrap_or(&Value::Null), project_id);
                    print_json(&json!({ "groups": groups, "ok": true }));
                }
                Err(error) => return Err(error),
            }
            Ok(())
        }
        "create-session-group" => {
            let project_id = require_project(&target, verb)?;
            let mut payload = request("create", Some(&project_id), None);
            if let Some(title) = title {
                payload.insert("title".to_string(), title);
            }
            print_answer(ask_app(payload, flags)?);
            Ok(())
        }
        "rename-session-group" => {
            let project_id = require_project(&target, verb)?;
            let group_id = require_group(&target, verb)?;
            let Some(title) = title else {
                return Err(CliError::Other(
                    "rename-session-group needs --title.".to_string(),
                ));
            };
            let mut payload = request("rename", Some(&project_id), Some(&group_id));
            payload.insert("title".to_string(), title);
            print_answer(ask_app(payload, flags)?);
            Ok(())
        }
        "delete-session-group" => {
            let project_id = require_project(&target, verb)?;
            let group_id = require_group(&target, verb)?;
            print_answer(ask_app(
                request("delete", Some(&project_id), Some(&group_id)),
                flags,
            )?);
            Ok(())
        }
        "move-session-to-group" | "remove-session-from-group" => {
            let Some(selector_text) = selector::session_selector_from_args(&parsed.rest, flags)
            else {
                return Err(CliError::Other(format!("{verb} needs a session selector.")));
            };
            let session = selector::resolve_cli_session_selector(&selector_text, flags)?;
            let (Some(project_id), Some(session_id)) = (
                session.get("projectId").and_then(Value::as_str),
                session.get("sessionId").and_then(Value::as_str),
            ) else {
                return Err(CliError::Other(format!(
                    "No gxserver session matched \"{selector_text}\"."
                )));
            };
            let group_id = if verb == "move-session-to-group" {
                Some(require_group(&target, verb)?)
            } else {
                None
            };
            if let Some(group_project) = target.project_id.as_deref() {
                if group_id.is_some() && group_project != project_id {
                    return Err(CliError::Other(format!(
                        "The group is in project {group_project} and the session in {project_id}. A session only joins a group of its own project."
                    )));
                }
            }
            let mut payload = request("move", Some(project_id), group_id.as_deref());
            payload.insert("sessionId".to_string(), json!(session_id));
            print_answer(ask_app(payload, flags)?);
            Ok(())
        }
        other => Err(CliError::Other(format!("Unknown command: {other}"))),
    }
}
