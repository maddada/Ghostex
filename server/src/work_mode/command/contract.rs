//! The JSON a command ticket source reads and answers, checked against the contract's rules
//! and caps. Unknown fields are ignored; a missing optional field and `null` both mean "not
//! known"; a field over its cap is cut, a link with another scheme is dropped. A field that is
//! required and unreadable fails the whole answer (`info`, `detail`, `create`) or skips one row
//! (`list`). Each `Err` names what could not be read, for "<Name> answered something Ghostex
//! can't read (<what>)".

use serde_json::{json, Map, Value};

use super::*;

/// Ids a command may not call itself: the built-in sources' link kinds and words a typed
/// reference could mean.
pub(crate) const RESERVED_SOURCE_IDS: [&str; 8] = [
    "linear",
    "github",
    "issue",
    "pr",
    "pullrequest",
    "ticket",
    "ghostex",
    "command",
];

/// The Work page groups a ticket's state may be in. GitHub's `closed` and the PR groups (`draft`,
/// `open`, `merged`) belong to the code host.
const STATE_GROUPS: [&str; 6] = ["backlog", "todo", "progress", "review", "done", "canceled"];

pub(crate) const MAX_LISTED_TICKETS: usize = 200;
pub(crate) const MAX_EXACT_IDS: usize = 50;
const MAX_ID_KEYS: usize = 200;
const MAX_CREDENTIAL_FIELDS: usize = 8;
const MAX_LABELS: usize = 20;
const MAX_PULL_REQUESTS: usize = 20;
/// A ticket id; a longer text is not one.
const MAX_ID_CHARS: usize = 64;
/// A state's name and its raw state word.
const MAX_STATE_CHARS: usize = 40;
/// An RFC 3339 time: `2026-10-28T09:12:00.123456789+05:30` is 35 bytes. A longer text is dropped
/// before it is parsed, since the format allows a fraction of a second of any length.
const MAX_TIME_BYTES: usize = 40;
/// What an error message from the command is cut to.
pub(crate) const MAX_MESSAGE_CHARS: usize = 300;
/// Create ticket's title, as for GitHub issues.
const MAX_CREATE_TITLE_CHARS: usize = 256;

/// How the source's ticket ids look, which decides how gxserver matches them in branches and
/// typed references without asking the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdFormat {
    /// `ACM-12`: a key from `info.ids.keys`, a dash and a number, matched like Linear's.
    KeyNumber,
    /// `218`: a bare number, matched like a GitHub issue.
    Number,
}

impl IdFormat {
    /// The canonical form of a typed id (`acm-12` → `ACM-12`, `#218` → `218`); `None` when the
    /// text is not an id of this format.
    pub(crate) fn parse_ref(self, text: &str) -> Option<String> {
        if text.trim().chars().count() > MAX_ID_CHARS {
            return None;
        }
        match self {
            Self::KeyNumber => normalize_linear_identifier(text),
            Self::Number => work_source(WorkSourceId::Github).parse_ref(text),
        }
    }
}

/// What `info` says about the source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandInfo {
    pub(crate) id: String,
    pub(crate) name: String,
    /// The icon rebuilt from the allowlist (`sanitize_icon`), or why it was dropped. `None` when
    /// the command sent none.
    pub(crate) icon: Option<Result<String, String>>,
    pub(crate) id_format: IdFormat,
    /// The keys `KeyNumber` ids start with, uppercase.
    pub(crate) id_keys: Vec<String>,
    pub(crate) credentials: Vec<CredentialField>,
    pub(crate) can_create: bool,
}

/// One credential the source asks for in Settings.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CredentialField {
    /// Its env name is `GHOSTEX_CREDENTIAL_<NAME>`.
    pub(crate) name: String,
    pub(crate) label: String,
    /// A secret is never returned to any client.
    pub(crate) secret: bool,
    pub(crate) required: bool,
    pub(crate) placeholder: Option<String>,
}

/// A Ghostex project as the command sees it in `list` and `create`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandProject {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: Option<String>,
    /// `owner/repo` of its `origin`, lowercase.
    pub(crate) repo: Option<String>,
}

impl CommandProject {
    fn value(&self) -> Value {
        json!({ "id": self.id, "name": self.name, "path": self.path, "repo": self.repo })
    }
}

/// One ticket of a `list` answer, or the ticket part of a `detail` answer.
#[derive(Clone, Debug)]
pub(crate) struct CommandTicket {
    /// Canonical for the source's `IdFormat`.
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) status: WorkItemStatus,
    /// The source's raw state word (`state.type`, else `state.name`).
    pub(crate) state: String,
    pub(crate) updated_at: Option<String>,
    pub(crate) assignee: Option<WorkItemPerson>,
    pub(crate) labels: Vec<String>,
    pub(crate) cycle: Option<String>,
    pub(crate) branch_name: Option<String>,
    /// Only GitHub PR URLs.
    pub(crate) pull_request_urls: Vec<String>,
    pub(crate) team: Option<String>,
    /// The request's project it belongs to: its `projectId` when that is one of them, else the
    /// one whose repo its `repo` names.
    pub(crate) project_id: Option<String>,
}

/// A `list` answer.
#[derive(Debug, Default)]
pub(crate) struct CommandList {
    pub(crate) tickets: Vec<CommandTicket>,
    /// The asked-for ids the source says do not exist (an exact read).
    pub(crate) not_found: Vec<String>,
    /// Rows left out because their id, title or state could not be read.
    pub(crate) skipped: usize,
    /// Whether rows past `MAX_LISTED_TICKETS` were left out.
    pub(crate) truncated: bool,
}

impl CommandList {
    /// What the Work page says about the rows it does not show.
    pub(crate) fn errors(&self, name: &str) -> Vec<String> {
        let mut errors = Vec::new();
        if self.truncated {
            errors.push(format!(
                "{name}: showed the first {MAX_LISTED_TICKETS} tickets"
            ));
        }
        if self.skipped > 0 {
            let tickets = if self.skipped == 1 {
                "ticket"
            } else {
                "tickets"
            };
            errors.push(format!(
                "{name}: skipped {} {tickets} it sent in an unknown shape",
                self.skipped
            ));
        }
        errors
    }
}

/// A `detail` answer, in the shapes the details page shows (`TicketDetail`).
#[derive(Debug)]
pub(crate) struct CommandDetail {
    pub(crate) ticket: CommandTicket,
    pub(crate) description: Option<String>,
    /// Oldest first: the newest `MAX_COMMENTS`.
    pub(crate) comments: Vec<Value>,
    pub(crate) comment_count: usize,
    /// Only the allowlisted fields: nothing else the command sends reaches a client.
    pub(crate) fields: Map<String, Value>,
}

/// A `create` answer.
#[derive(Debug, PartialEq)]
pub(crate) struct CommandCreated {
    pub(crate) id: String,
    pub(crate) url: Option<String>,
    pub(crate) branch_name: Option<String>,
}

/// The `code` of the error object a failing command prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorCode {
    /// Its credentials were rejected.
    Auth,
    /// The ticket does not exist.
    NotFound,
    RateLimited,
    /// Create ticket's draft was refused; the message says why.
    Invalid,
    Other,
}

/// `{"error": {"code", "message"}}`, the error a failing command may print instead of a
/// message on stderr. The message is exactly as sent, untrimmed: the runner masks credentials in
/// it before it trims and cuts it, so a credential sent with spaces around it is still found.
pub(crate) fn reported_error(answer: &Value) -> Option<(ErrorCode, String)> {
    let error = answer.get("error")?.as_object()?;
    text(error, "message")?;
    let message = as_sent(error, "message");
    let code = match text(error, "code") {
        Some("auth") => ErrorCode::Auth,
        Some("notFound") => ErrorCode::NotFound,
        Some("rateLimited") => ErrorCode::RateLimited,
        Some("invalid") => ErrorCode::Invalid,
        _ => ErrorCode::Other,
    };
    Some((code, message.to_string()))
}

pub(crate) fn parse_info(answer: &Value, guard: &AnswerGuard) -> Result<CommandInfo, String> {
    let info = object(answer)?;
    let id = checked_required(info, "id", guard)?;
    if !valid_source_id(id) {
        return Err(format!(
            "id \"{}\" isn't 2 to 32 lowercase letters, digits or dashes starting with a letter",
            as_sent(info, "id")
        ));
    }
    if RESERVED_SOURCE_IDS.contains(&id) {
        return Err(format!("id \"{}\" is reserved", as_sent(info, "id")));
    }
    let name = required(info, "name")?;
    if name.chars().count() > 40 {
        return Err("name is longer than 40 characters".to_string());
    }
    one_line(name, "name")?;
    let ids = info
        .get("ids")
        .and_then(Value::as_object)
        .ok_or("ids is missing")?;
    let id_format = match checked_text(ids, "format", guard)? {
        Some("key-number") => IdFormat::KeyNumber,
        Some("number") => IdFormat::Number,
        _ => return Err("ids.format isn't key-number or number".to_string()),
    };
    let id_keys = match (id_format, ids.get("keys")) {
        (IdFormat::KeyNumber, Some(Value::Array(keys))) => {
            if keys.len() > MAX_ID_KEYS {
                return Err(format!("ids.keys has more than {MAX_ID_KEYS} keys"));
            }
            keys.iter()
                .map(|key| {
                    if let Some(key) = key.as_str() {
                        guard.structural("ids.keys", key)?;
                    }
                    key.as_str()
                        .filter(|key| valid_id_key(key))
                        .map(str::to_ascii_uppercase)
                        .ok_or_else(|| match key.as_str() {
                            // The decoded text, which the runner's mask matches, never the
                            // value written out again as JSON.
                            Some(key) => format!("ids.keys has an invalid key \"{key}\""),
                            None => "ids.keys has a key that isn't text".to_string(),
                        })
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        _ => Vec::new(),
    };
    let credentials = match info.get("credentials") {
        Some(Value::Array(fields)) if fields.len() <= MAX_CREDENTIAL_FIELDS => fields
            .iter()
            .map(|field| credential_field(field, guard))
            .collect::<Result<Vec<_>, _>>()?,
        Some(Value::Array(_)) => {
            return Err(format!(
                "credentials has more than {MAX_CREDENTIAL_FIELDS} fields"
            ))
        }
        _ => return Err("credentials is missing".to_string()),
    };
    for (index, field) in credentials.iter().enumerate() {
        if credentials[..index]
            .iter()
            .any(|earlier| earlier.name == field.name)
        {
            return Err(format!("credential name \"{}\" is used twice", field.name));
        }
    }
    Ok(CommandInfo {
        id: id.to_string(),
        name: guard
            .display(as_sent(info, "name"), 40)
            .ok_or("name is missing")?,
        icon: text(info, "icon").map(|icon| match guard.droppable(icon) {
            Some(icon) => sanitize_icon(icon),
            None => Err(ICON_REFUSED.to_string()),
        }),
        id_format,
        id_keys,
        credentials,
        can_create: info
            .get("canCreate")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn credential_field(field: &Value, guard: &AnswerGuard) -> Result<CredentialField, String> {
    let field = object(field)?;
    guard.structural("a credential name", as_sent(field, "name"))?;
    let name = required(field, "name")?;
    if !valid_credential_name(name) {
        return Err(format!(
            "credential name \"{}\" isn't lowercase letters, digits or underscores",
            as_sent(field, "name")
        ));
    }
    let label = required(field, "label")?;
    one_line(label, "a credential label")?;
    let placeholder = text(field, "placeholder");
    if let Some(placeholder) = placeholder {
        one_line(placeholder, "a credential placeholder")?;
    }
    Ok(CredentialField {
        name: name.to_string(),
        label: guard
            .display(as_sent(field, "label"), 40)
            .ok_or("label is missing")?,
        secret: field.get("secret").and_then(Value::as_bool).unwrap_or(true),
        required: field
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        placeholder: placeholder.and_then(|_| guard.display(as_sent(field, "placeholder"), 80)),
    })
}

/// Which of `known` (the fields that have a value) an `info` answer declares secret, read before
/// the rest of the answer, or even the rest of each declaration, is checked: a declaration with a
/// valid name counts with its `secret` (`true` when absent or not a boolean), whatever its label
/// or placeholder say. The runner masks those fields in this very answer, so a field turning
/// secret is never shown in the complaint that refuses the answer.
pub(crate) fn credential_secrecy(answer: &Value, known: &[&str]) -> Vec<String> {
    answer
        .get("credentials")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|field| {
            let name = text(field, "name")?;
            let secret = field.get("secret").and_then(Value::as_bool).unwrap_or(true);
            (secret && valid_credential_name(name) && known.contains(&name))
                .then(|| name.to_string())
        })
        .collect()
}

fn valid_credential_name(name: &str) -> bool {
    name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `list`'s input: every work-mode project of the workspace, and the ids of an exact read.
pub(crate) fn list_input(projects: &[CommandProject], ids: Option<&[String]>) -> Value {
    let mut input =
        json!({ "projects": projects.iter().map(CommandProject::value).collect::<Vec<_>>() });
    if let Some(ids) = ids {
        input["ids"] = json!(ids);
    }
    input
}

/// `detail`'s input: the id goes on stdin, never in argv, so an id from a branch name can never
/// read as a flag.
pub(crate) fn detail_input(id: &str) -> Value {
    json!({ "id": id })
}

/// `create`'s input: the fixed draft (a source's own pickers are not forwarded) and the project
/// Create ticket was opened from.
pub(crate) fn create_input(draft: &TicketDraft, project: Option<&CommandProject>) -> Value {
    json!({
        "title": cut(draft.title.trim(), MAX_CREATE_TITLE_CHARS),
        "description": draft.description,
        "assignToMe": draft.assign_to_me,
        "project": project.map(CommandProject::value),
    })
}

pub(crate) fn parse_list(
    answer: &Value,
    format: IdFormat,
    projects: &[CommandProject],
    guard: &AnswerGuard,
) -> Result<CommandList, String> {
    let rows = answer
        .get("tickets")
        .and_then(Value::as_array)
        .ok_or("tickets is missing")?;
    let mut list = CommandList {
        truncated: rows.len() > MAX_LISTED_TICKETS,
        ..CommandList::default()
    };
    for row in rows.iter().take(MAX_LISTED_TICKETS) {
        match parse_ticket(row, format, projects, guard) {
            Ok(ticket) => list.tickets.push(ticket),
            Err(_) => list.skipped += 1,
        }
    }
    list.not_found = answer
        .get("notFound")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str().and_then(|id| guard.droppable(id)))
        .filter_map(|id| format.parse_ref(id))
        .take(MAX_EXACT_IDS)
        .collect();
    Ok(list)
}

pub(crate) fn parse_detail(
    answer: &Value,
    format: IdFormat,
    projects: &[CommandProject],
    guard: &AnswerGuard,
) -> Result<CommandDetail, String> {
    let ticket = parse_ticket(answer, format, projects, guard)?;
    let detail = object(answer)?;
    let mut comments: Vec<Value> = detail
        .get("comments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|comment| {
            Some(json!({
                "author": shown_text(comment, "author", 64, guard)?,
                "avatarUrl": kept_text(comment, "avatarUrl", guard).and_then(https_url),
                "body": shown_text(comment, "body", MAX_TEXT_CHARS, guard).unwrap_or_default(),
                "createdAt": time(comment, "createdAt", guard),
            }))
        })
        .collect();
    let comment_count = detail
        .get("commentCount")
        .and_then(Value::as_u64)
        .map(|count| count as usize)
        .unwrap_or(comments.len());
    comments.drain(..comments.len().saturating_sub(MAX_COMMENTS));
    let fields = json!({
        "updatedAt": ticket.updated_at,
        "createdAt": time(detail, "createdAt", guard),
        "branchName": ticket.branch_name,
        "priority": shown_text(detail, "priority", 40, guard),
        "assignee": ticket.assignee,
        "author": shown_text(detail, "author", 64, guard),
        "cycle": ticket.cycle,
        "labels": ticket.labels,
        "pullRequestUrls": ticket.pull_request_urls,
    });
    Ok(CommandDetail {
        description: shown_text(detail, "description", MAX_TEXT_CHARS, guard),
        comments,
        comment_count,
        fields: fields.as_object().cloned().unwrap_or_default(),
        ticket,
    })
}

pub(crate) fn parse_created(
    answer: &Value,
    format: IdFormat,
    guard: &AnswerGuard,
) -> Result<CommandCreated, String> {
    let created = object(answer)?;
    Ok(CommandCreated {
        id: canonical_id(created, format, guard)?,
        url: kept_text(created, "url", guard).and_then(web_url),
        branch_name: branch_name(created, guard),
    })
}

fn parse_ticket(
    row: &Value,
    format: IdFormat,
    projects: &[CommandProject],
    guard: &AnswerGuard,
) -> Result<CommandTicket, String> {
    let ticket = object(row)?;
    let id = canonical_id(ticket, format, guard)?;
    let state = ticket
        .get("state")
        .and_then(Value::as_object)
        .ok_or("state is missing")?;
    let group = checked_required(state, "group", guard)?;
    let group = STATE_GROUPS
        .into_iter()
        .find(|known| *known == group)
        .ok_or_else(|| {
            format!(
                "state group \"{}\" isn't one of {}",
                as_sent(state, "group"),
                STATE_GROUPS.join(", ")
            )
        })?;
    let state_name = shown_text(state, "name", MAX_STATE_CHARS, guard).ok_or("name is missing")?;
    let wanted_project = checked_text(ticket, "projectId", guard)?;
    let repo = checked_text(ticket, "repo", guard)?.map(str::to_ascii_lowercase);
    let project_id = wanted_project
        .and_then(|wanted| projects.iter().find(|project| project.id == wanted))
        .or_else(|| {
            let repo = repo.as_deref()?;
            projects
                .iter()
                .find(|project| project.repo.as_deref() == Some(repo))
        })
        .map(|project| project.id.clone());
    // A key-number id names its team; only a number source says which team in the answer.
    let team = match format {
        IdFormat::KeyNumber => id.split_once('-').map(|(team, _)| team.to_string()),
        IdFormat::Number => checked_text(ticket, "team", guard)?.map(|team| cut(team, 32)),
    };
    Ok(CommandTicket {
        title: shown_text(ticket, "title", 500, guard).ok_or("title is missing")?,
        url: kept_text(ticket, "url", guard).and_then(web_url),
        state: shown_text(state, "type", MAX_STATE_CHARS, guard)
            .unwrap_or_else(|| state_name.clone()),
        status: WorkItemStatus {
            group,
            name: state_name,
        },
        updated_at: time(ticket, "updatedAt", guard),
        assignee: ticket
            .get("assignee")
            .and_then(Value::as_object)
            .and_then(|person| {
                Some(WorkItemPerson {
                    name: shown_text(person, "name", 64, guard)?,
                    avatar_url: kept_text(person, "avatarUrl", guard).and_then(https_url),
                    is_me: person.get("isMe").and_then(Value::as_bool).unwrap_or(false),
                })
            }),
        labels: strings(ticket, "labels")
            .take(MAX_LABELS)
            .map(|label| guard.display_as_sent(label, 64))
            .collect(),
        cycle: shown_text(ticket, "cycle", 64, guard),
        branch_name: branch_name(ticket, guard),
        pull_request_urls: strings(ticket, "pullRequestUrls")
            .filter_map(|url| guard.droppable(url))
            .filter(|url| is_canonical_github_pull_request_url(url))
            .take(MAX_PULL_REQUESTS)
            .map(str::to_string)
            .collect(),
        team,
        project_id,
        id,
    })
}

/// The answer's `id`, which must already be in the format's canonical form.
fn canonical_id(
    answer: &Map<String, Value>,
    format: IdFormat,
    guard: &AnswerGuard,
) -> Result<String, String> {
    let id = checked_required(answer, "id", guard)?;
    match format.parse_ref(id) {
        Some(canonical) if canonical == id => Ok(canonical),
        _ => Err(format!(
            "id \"{}\" isn't a ticket id of its own format",
            as_sent(answer, "id")
        )),
    }
}

fn valid_source_id(id: &str) -> bool {
    (2..=32).contains(&id.len())
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_id_key(key: &str) -> bool {
    (1..=10).contains(&key.len())
        && key.starts_with(|c: char| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric())
}

fn object(value: &Value) -> Result<&Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| "it isn't a JSON object".to_string())
}

/// A non-empty string field; `null`, absent or blank are all "not known".
fn text<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// A field's text exactly as the command sent it, for a complaint or an error message. The runner
/// masks credentials in it, and a trimmed copy would hide a credential sent with spaces around
/// it from that mask.
fn as_sent<'a>(object: &'a Map<String, Value>, key: &str) -> &'a str {
    object.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    text(object, key).ok_or_else(|| format!("{key} is missing"))
}

fn strings<'a>(object: &'a Map<String, Value>, key: &str) -> impl Iterator<Item = &'a str> {
    object
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// An RFC 3339 time, as sent; dropped when it contains a secret.
fn time(object: &Map<String, Value>, key: &str, guard: &AnswerGuard) -> Option<String> {
    kept_text(object, key, guard)
        .filter(|time| time.len() <= MAX_TIME_BYTES)
        .filter(|time| chrono::DateTime::parse_from_rfc3339(time).is_ok())
        .map(str::to_string)
}

/// A branch Start work can create a worktree on; any other name is dropped (never cut, which
/// would name another branch), so the source falls back to the name Ghostex makes.
fn branch_name(object: &Map<String, Value>, guard: &AnswerGuard) -> Option<String> {
    kept_text(object, "branchName", guard)
        .filter(|branch| crate::server::is_allowed_worktree_branch(branch))
        .map(str::to_string)
}

/// Display text: the decoded value masked whole, then trimmed and cut (`AnswerGuard::display`).
fn shown_text(
    object: &Map<String, Value>,
    key: &str,
    cap: usize,
    guard: &AnswerGuard,
) -> Option<String> {
    guard.display(object.get(key)?.as_str()?, cap)
}

/// A structural field (`text`), refused when its decoded value contains a secret.
fn checked_text<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    guard: &AnswerGuard,
) -> Result<Option<&'a str>, String> {
    guard.structural(key, as_sent(object, key))?;
    Ok(text(object, key))
}

fn checked_required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    guard: &AnswerGuard,
) -> Result<&'a str, String> {
    checked_text(object, key, guard)?.ok_or_else(|| format!("{key} is missing"))
}

/// A link (`text`), dropped when its decoded value contains a secret.
fn kept_text<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    guard: &AnswerGuard,
) -> Option<&'a str> {
    guard.droppable(as_sent(object, key))?;
    text(object, key)
}

/// Text Settings shows on one line: a newline or another control character in it is refused.
fn one_line(text: &str, what: &str) -> Result<(), String> {
    if text.chars().any(char::is_control) {
        return Err(format!(
            "{what} has a line break or another control character"
        ));
    }
    Ok(())
}

/// An `http` or `https` link: clients open these.
fn web_url(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://")) && url.len() <= MAX_URL_BYTES)
        .then(|| url.to_string())
}

fn https_url(url: &str) -> Option<String> {
    (url.to_ascii_lowercase().starts_with("https://") && url.len() <= MAX_URL_BYTES)
        .then(|| url.to_string())
}
