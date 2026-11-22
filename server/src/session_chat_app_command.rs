/*
CDXC:SessionChat 2026-08-23:
Slash commands GHOSTEX types into the agent, not the user.

Several flows write a command straight into the session's pty without the chat
composer ever being involved: provider-specific first-prompt auto-title jobs,
the rename modal's "Generate Name" stage `/rename <title>` (Pi `/name`, Hermes
Agent `/title`), and forks submit a provisional `Fork: <old title>`
the same way. Chat is a transcript projection, so what it shows afterwards
depends entirely on whether the CLI happens to record the command:

  * Claude Code writes a `local_command` row for everything it intercepts, so
    the send lands in the transcript and chat already renders it.
  * Codex records NOTHING for an intercepted command. The conversation simply
    did not move, and a session that renamed itself mid-thread looked like the
    chat had dropped whatever the user was doing.

So the app records what IT sent. This is deliberately a short-lived
ACKNOWLEDGEMENT, not an archive entry: the point is "Ghostex just did this",
which stops being worth a row once the agent's own record shows up (the client
drops ours when it finds the matching transcript envelope) or once enough time
has passed that nobody is still wondering. Nothing here is persisted, so a
reload shows the transcript and only the transcript, and the two agents cannot
disagree about history.

CDXC:SessionChat 2026-09-10 WHY:
That last sentence is now true of THIS store only. Commands the user sends from
chat are archived by session_chat_local_command.rs and replayed into the
messages on every read, because the user asked for those to survive a reload;
this store still holds only the live half, including the live half of those
(`local_command`), and still expires.

The store is keyed by (project, session) and swept lazily on read, the same
shape as the terminal-notice watchdog map in session_chat_notice/watchdog_store.rs.
*/

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

/*
Long enough to cover a rename the user was not looking at (the fork rename
fires four seconds after startup, the auto-title job after a model round trip),
short enough that a row cannot outlive anyone's memory of what caused it. The
client retires it earlier whenever the agent records the command itself.
*/
const APP_COMMAND_TTL: Duration = Duration::from_secs(300);

/// A session cannot plausibly be app-renamed more often than this; the cap only
/// exists so a runaway caller cannot grow the map without bound.
const APP_COMMAND_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub struct SessionChatAppCommand {
    /// Stable within a session, so a client can key rows without re-deriving
    /// identity from the text (two `/rename` sends can carry the same title).
    pub id: String,
    /// Verbatim command text as written to the pty, e.g. `/rename Fix parser`.
    pub command: String,
    /// Resolved session title. Bare `/rename` commands receive this once the
    /// agent publishes the generated title in its own metadata.
    pub title: Option<String>,
    pub output: Option<String>,
    /// The parsed goal cell behind a Codex `/goal` command's output.
    pub goal: Option<crate::session_chat_codex_goal::SessionChatCodexGoal>,
    /*
    CDXC:SessionChat 2026-09-10 WHY:
    The row is the live half of a command the USER sent from chat, which is
    archived in session_chat_local_command.rs and replays from there on the next
    read. Clients render this one as the same two rows the archive produces, so
    the look does not change under the reader when the archive takes over, and
    the archived id travels with it so the settled output lands on the same row.
    */
    pub local_command: bool,
    /// CDXC:SessionTitles 2026-09-30 WHY: a `/rename` carrying a name the user typed (sidebar context menu Rename, the rename modal) must not read "Ghostex auto named this session" in chat; only the generated and fork renames are Ghostex's own naming.
    pub user_rename: bool,
    durable_id: Option<String>,
    /// Whose screen the baseline was captured from, for the output diff.
    screen_agent: Option<String>,
    screen_baseline: Option<String>,
    /// RFC3339 millis, for display ordering only.
    pub sent_at: String,
    title_metadata_baseline: Option<(String, String)>,
    title_metadata_baseline_captured: bool,
    recorded: Instant,
}

impl SessionChatAppCommand {
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".to_string(), json!(self.id));
        map.insert("command".to_string(), json!(self.command));
        if let Some(output) = self.output.as_deref() {
            map.insert("output".to_string(), json!(output));
        }
        if let Some(goal) = self.goal.as_ref() {
            map.insert("goal".to_string(), goal.to_value());
        }
        if let Some(title) = self.title.as_deref() {
            map.insert("title".to_string(), json!(title));
        }
        if self.local_command {
            map.insert("archiveId".to_string(), json!(self.durable_id));
            map.insert("localCommand".to_string(), json!(true));
        }
        if self.user_rename {
            map.insert("userRename".to_string(), json!(true));
        }
        map.insert("sentAt".to_string(), json!(self.sent_at));
        Value::Object(map)
    }
}

type AppCommandStore = Mutex<HashMap<(String, String), Vec<SessionChatAppCommand>>>;

fn store() -> &'static AppCommandStore {
    static STORE: OnceLock<AppCommandStore> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Answered side questions kept live per session (see `prune`).
const SIDE_QUESTION_LIMIT: usize = 20;

/// CDXC:SessionChat 2026-09-27 WHY: a client keeps the rows a snapshot gave it and adds live rows after, so an answered `/btw` asked since its last snapshot would vanish when its live row expired, although the user decided side answers stay in the chat for good. Answered side questions are exempt from the expiry and the general cap and keep their own cap instead; the archive holds them across restarts.
fn prune(rows: &mut Vec<SessionChatAppCommand>, now: Instant) {
    let kept_side_question =
        |row: &SessionChatAppCommand| is_side_question(&row.command) && row.output.is_some();
    rows.retain(|row| {
        kept_side_question(row) || now.duration_since(row.recorded) < APP_COMMAND_TTL
    });
    let side_questions = rows.iter().filter(|row| kept_side_question(row)).count();
    if side_questions > SIDE_QUESTION_LIMIT {
        let mut surplus = side_questions - SIDE_QUESTION_LIMIT;
        rows.retain(|row| {
            if surplus > 0 && kept_side_question(row) {
                surplus -= 1;
                return false;
            }
            true
        });
    }
    let others = rows.iter().filter(|row| !kept_side_question(row)).count();
    if others > APP_COMMAND_LIMIT {
        let mut surplus = others - APP_COMMAND_LIMIT;
        rows.retain(|row| {
            if surplus > 0 && !kept_side_question(row) {
                surplus -= 1;
                return false;
            }
            true
        });
    }
}

/*
Call this at the point the command STRING is built, next to the dispatch that
writes it — not from inside the zmx send path. Ghostex also writes Ctrl+U/Ctrl+Y
draft-kill bytes and bare `\r` submits through that same path, and none of those
are commands the user needs told about.
*/
pub fn record_session_chat_app_command(project_id: &str, session_id: &str, command: &str) {
    record_session_chat_app_command_inner(project_id, session_id, command, None, false, false);
}

/// Shows something Ghostex does for the session on its own, with its result so far, as a row in the
/// chat (Codex's "Update now", which Ghostex runs outside the terminal: session_chat_codex_update.rs).
/// Pass the returned id back to replace that row's output as the work goes on.
pub(crate) fn show_session_chat_app_command_output(
    project_id: &str,
    session_id: &str,
    id: Option<&str>,
    command: &str,
    output: &str,
) -> String {
    let now = Instant::now();
    let sent_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let Ok(mut guard) = store().lock() else {
        return String::new();
    };
    let rows = guard
        .entry((project_id.to_string(), session_id.to_string()))
        .or_default();
    if let Some(row) = id.and_then(|id| rows.iter_mut().find(|row| row.id == id)) {
        row.output = Some(output.to_string());
        row.recorded = now;
        return row.id.clone();
    }
    let id = format!("{sent_at}-{}", rows.len());
    rows.push(SessionChatAppCommand {
        id: id.clone(),
        command: command.to_string(),
        title: None,
        output: Some(output.to_string()),
        goal: None,
        local_command: false,
        user_rename: false,
        durable_id: None,
        screen_agent: None,
        screen_baseline: None,
        sent_at,
        title_metadata_baseline: None,
        title_metadata_baseline_captured: false,
        recorded: now,
    });
    prune(rows, now);
    id
}

/// Record a rename whose title the user typed, so chat does not call it an auto name.
pub fn record_session_chat_user_rename_command(project_id: &str, session_id: &str, command: &str) {
    record_session_chat_app_command_inner(project_id, session_id, command, None, false, true);
}

/// Record a bare rename together with the title record visible before dispatch.
pub fn record_session_chat_app_command_with_title_metadata_baseline(
    project_id: &str,
    session_id: &str,
    command: &str,
    title_metadata_baseline: Option<(String, String)>,
) {
    record_session_chat_app_command_inner(
        project_id,
        session_id,
        command,
        title_metadata_baseline,
        true,
        false,
    );
}

fn record_session_chat_app_command_inner(
    project_id: &str,
    session_id: &str,
    command: &str,
    title_metadata_baseline: Option<(String, String)>,
    title_metadata_baseline_captured: bool,
    user_rename: bool,
) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    let now = Instant::now();
    let sent_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let Ok(mut guard) = store().lock() else {
        return;
    };
    let rows = guard
        .entry((project_id.to_string(), session_id.to_string()))
        .or_default();
    rows.push(SessionChatAppCommand {
        id: format!("{sent_at}-{}", rows.len()),
        command: command.to_string(),
        title: app_command_title(command),
        output: None,
        goal: None,
        local_command: false,
        user_rename,
        durable_id: None,
        screen_agent: None,
        screen_baseline: None,
        sent_at,
        title_metadata_baseline,
        title_metadata_baseline_captured,
        recorded: now,
    });
    prune(rows, now);
}

fn app_command_title(command: &str) -> Option<String> {
    // Empryo's rename is the two-word `/tab rename <name>`.
    if let Some(title) = command.trim().strip_prefix("/tab rename ") {
        let title = title.trim();
        return (!title.is_empty()).then(|| title.to_string());
    }
    let mut parts = command.trim().splitn(2, char::is_whitespace);
    let command_name = parts.next()?.to_ascii_lowercase();
    if !matches!(command_name.as_str(), "/rename" | "/name" | "/title") {
        return None;
    }
    let title = parts.next()?.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// CDXC:SessionChat 2026-09-05 WHY:
/// Codex's local commands do not enter its transcript, and asynchronous commands such as /mcp repaint after their initial loading line.
/// Retain one command's screen baseline until the next send so the shared screen probe can update the same result row.
///
/// CDXC:SessionChat 2026-09-10 WHY:
/// Every agent now takes this path, not only Codex: Claude records a transcript
/// envelope for a handful of its commands and nothing at all for the rest
/// (`/rename` included), so the screen is the only place its result exists.
/// `durable_id` is the archived row in session_chat_local_command.rs that this
/// capture's settled output belongs to.
pub(crate) fn begin_local_command_output(
    project_id: &str,
    session_id: &str,
    agent: Option<&str>,
    command: &str,
    durable_id: Option<String>,
    screen: String,
) {
    let Ok(mut guard) = store().lock() else {
        return;
    };
    let rows = guard
        .entry((project_id.to_string(), session_id.to_string()))
        .or_default();
    if agent == Some("codex")
        && crate::session_chat_codex_dialog::detect_codex_dialog(&screen).is_some()
        && rows.iter().any(|row| row.screen_baseline.is_some())
    {
        return;
    }
    for row in rows.iter_mut() {
        row.screen_baseline = None;
    }
    let now = Instant::now();
    let sent_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    rows.push(SessionChatAppCommand {
        id: format!("{sent_at}-{}", rows.len()),
        command: command.to_string(),
        title: None,
        output: Some(String::new()),
        goal: None,
        local_command: durable_id.is_some(),
        user_rename: false,
        durable_id,
        screen_agent: agent.map(str::to_string),
        screen_baseline: Some(screen),
        sent_at,
        title_metadata_baseline: None,
        title_metadata_baseline_captured: false,
        recorded: now,
    });
    prune(rows, now);
}

pub(crate) fn commit_local_command(
    project_id: &str,
    session_id: &str,
    mut command: crate::session_chat_local_command::SessionChatLocalCommand,
) {
    let Ok(mut guard) = store().lock() else {
        return;
    };
    let rows = guard
        .entry((project_id.to_string(), session_id.to_string()))
        .or_default();
    if let Some(row) = rows
        .iter()
        .find(|row| row.durable_id.as_deref() == Some(&command.id))
    {
        command.output = row.output.clone();
    } else {
        rows.push(SessionChatAppCommand {
            id: command.id.clone(),
            command: command.text(),
            title: None,
            output: None,
            goal: None,
            local_command: true,
            user_rename: false,
            durable_id: Some(command.id.clone()),
            screen_agent: None,
            screen_baseline: None,
            sent_at: chrono::DateTime::from_timestamp_millis(command.sent_at_ms)
                .unwrap_or_default()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            title_metadata_baseline: None,
            title_metadata_baseline_captured: false,
            recorded: Instant::now(),
        });
        prune(rows, Instant::now());
    }
    crate::session_chat_local_command::persist_session_chat_local_command(
        project_id, session_id, &command,
    );
}

pub(crate) fn discard_local_command(project_id: &str, session_id: &str, id: &str) {
    if let Ok(mut guard) = store().lock() {
        if let Some(rows) = guard.get_mut(&(project_id.to_string(), session_id.to_string())) {
            rows.retain(|row| row.durable_id.as_deref() != Some(id));
        }
    }
}

/// Claude's `/btw`: its answer is read off the panel whole (session_chat_claude_panel.rs), so the
/// screen diff, which would capture the panel's chrome and only its visible window, stays out.
fn is_side_question(command: &str) -> bool {
    command == "/btw" || command.starts_with("/btw ")
}

/// Whether an archived or live `/btw` asked `question`. A question Claude cut short with `…` on
/// its panel matches the full text the user sent.
fn asks(args: &str, question: &str) -> bool {
    match question.strip_suffix('…') {
        Some(prefix) => !prefix.trim().is_empty() && args.starts_with(prefix.trim_end()),
        None => args == question,
    }
}

/*
CDXC:SessionChat 2026-09-27 DECISION:
User: side answers stay in the chat after Close, folded, for good, at the point the question was asked. The answer is stored as the `/btw` command's output in the slash-command archive, so it replays on every read and client like any other archived command. A `/btw` typed in the terminal is archived here when its answer is read; an earlier question the user browsed back to is never added.
*/
pub(crate) fn attach_side_answer(
    project_id: &str,
    session_id: &str,
    session: Option<&Value>,
    question: &str,
    answer: &str,
    latest: bool,
) {
    if answer.trim().is_empty() {
        return;
    }
    let mut durable = None;
    if let Ok(mut guard) = store().lock() {
        if let Some(rows) = guard.get_mut(&(project_id.to_string(), session_id.to_string())) {
            if let Some(row) = rows.iter_mut().rev().find(|row| {
                row.local_command
                    && is_side_question(&row.command)
                    && asks(row.command["/btw".len()..].trim(), question)
            }) {
                row.output = Some(answer.to_string());
                row.screen_baseline = None;
                durable = row.durable_id.clone();
            }
        }
    }
    if let Some(id) = durable {
        crate::session_chat_local_command::attach_session_chat_local_command_output(
            project_id, session_id, &id, answer,
        );
        return;
    }
    let archived =
        crate::session_chat_local_command::load_session_chat_local_commands(project_id, session_id);
    if let Some(row) = archived
        .iter()
        .rev()
        .find(|row| row.command == "/btw" && asks(&row.args, question))
    {
        crate::session_chat_local_command::attach_session_chat_local_command_output(
            project_id, session_id, &row.id, answer,
        );
        return;
    }
    if !latest || question.ends_with('…') {
        return;
    }
    let Some(mut row) = crate::session_chat_local_command::prepare_session_chat_local_command(
        &format!("/btw {question}"),
    ) else {
        return;
    };
    if let Some(session) = session {
        crate::session_chat_local_command::anchor_session_chat_local_command(&mut row, session);
    }
    row.output = Some(answer.to_string());
    crate::session_chat_local_command::persist_session_chat_local_command(
        project_id, session_id, &row,
    );
}

/// The whole question behind a `/btw` Claude cut short with `…` on its panel, from what was sent.
pub(crate) fn full_side_question(
    project_id: &str,
    session_id: &str,
    question: &str,
) -> Option<String> {
    question.strip_suffix('…')?;
    if let Ok(guard) = store().lock() {
        if let Some(rows) = guard.get(&(project_id.to_string(), session_id.to_string())) {
            if let Some(row) = rows.iter().rev().find(|row| {
                is_side_question(&row.command) && asks(row.command["/btw".len()..].trim(), question)
            }) {
                return Some(row.command["/btw".len()..].trim().to_string());
            }
        }
    }
    crate::session_chat_local_command::load_session_chat_local_commands(project_id, session_id)
        .into_iter()
        .rev()
        .find(|row| row.command == "/btw" && asks(&row.args, question))
        .map(|row| row.args)
}

pub(crate) fn stop_local_command_output(project_id: &str, session_id: &str) {
    if let Ok(mut guard) = store().lock() {
        if let Some(rows) = guard.get_mut(&(project_id.to_string(), session_id.to_string())) {
            for row in rows {
                row.screen_baseline = None;
            }
        }
    }
}

pub(crate) fn refresh_local_command_output(project_id: &str, session_id: &str, screen: &str) {
    let mut settled: Vec<(String, String)> = Vec::new();
    {
        let Ok(mut guard) = store().lock() else {
            return;
        };
        let Some(rows) = guard.get_mut(&(project_id.to_string(), session_id.to_string())) else {
            return;
        };
        prune(rows, Instant::now());
        refresh_rows(rows, screen, &mut settled);
    }
    // Outside the store lock: the archive writes to disk.
    for (durable_id, output) in settled {
        crate::session_chat_local_command::attach_session_chat_local_command_output(
            project_id,
            session_id,
            &durable_id,
            &output,
        );
    }
}

fn refresh_rows(
    rows: &mut [SessionChatAppCommand],
    screen: &str,
    settled: &mut Vec<(String, String)>,
) {
    for row in rows
        .iter_mut()
        .filter(|row| row.screen_baseline.is_some() && !is_side_question(&row.command))
    {
        let Some(output) = crate::session_chat_local_command::session_chat_local_command_output(
            row.screen_agent.as_deref(),
            &row.command,
            row.screen_baseline.as_deref().unwrap_or_default(),
            screen,
        ) else {
            continue;
        };
        /*
        CDXC:SessionChat 2026-09-10 WHY:
        Outside Codex a later capture must not shorten the result. Claude's panels
        (`/status`) close into a one-line dismissal and its notices repaint under
        the result, so "the newest diff wins" replaced a full status panel with a
        stray footer line. A longer repaint can replace a loading message or restore a clipped top.
        Codex keeps last-wins: its `/mcp` repaints in place.
        */
        if row.screen_agent.as_deref() != Some("codex")
            && row.output.as_deref().is_some_and(|current| {
                !current.is_empty() && output.chars().count() < current.chars().count()
            })
        {
            continue;
        }
        if let Some(durable_id) = row.durable_id.as_deref() {
            settled.push((durable_id.to_string(), output.clone()));
        }
        if crate::session_chat_codex_goal::command_is_codex_goal(&row.command) {
            if let Some(cell) = crate::session_chat_codex_goal::parse_codex_goal_cell(&output) {
                row.output = Some(cell.text);
                row.goal = Some(cell.goal);
                if cell.settled {
                    row.screen_baseline = None;
                }
                continue;
            }
        }
        row.output = Some(output);
    }
}

/// Attach the title emitted by an agent after Ghostex sent a bare `/rename`.
/// Explicit title commands are already complete and are never rewritten. A
/// bare command resolves only after metadata advances beyond its pre-send
/// record and title, so an earlier title cannot permanently claim the row.
pub fn resolve_latest_session_chat_app_command_title(
    project_id: &str,
    session_id: &str,
    title: &str,
    title_metadata_revision: Option<&str>,
) {
    let title = title.trim();
    let Some(title_metadata_revision) = title_metadata_revision else {
        return;
    };
    if title.is_empty() {
        return;
    }
    let Ok(mut guard) = store().lock() else {
        return;
    };
    let Some(rows) = guard.get_mut(&(project_id.to_string(), session_id.to_string())) else {
        return;
    };
    let Some(row) = rows.iter_mut().rev().find(|row| {
        row.title.is_none()
            && row.title_metadata_baseline_captured
            && row.title_metadata_baseline.as_ref().is_none_or(
                |(baseline_title, baseline_revision)| {
                    baseline_title != title && baseline_revision != title_metadata_revision
                },
            )
            && matches!(
                row.command.split_whitespace().next(),
                Some("/rename" | "/name" | "/title")
            )
    }) else {
        return;
    };
    row.title = Some(title.to_string());
}

/// Live rows for a session, oldest first. Sweeps expired entries on the way out.
pub fn session_chat_app_commands(project_id: &str, session_id: &str) -> Vec<SessionChatAppCommand> {
    let now = Instant::now();
    let Ok(mut guard) = store().lock() else {
        return Vec::new();
    };
    let key = (project_id.to_string(), session_id.to_string());
    let Some(rows) = guard.get_mut(&key) else {
        return Vec::new();
    };
    prune(rows, now);
    if rows.is_empty() {
        guard.remove(&key);
        return Vec::new();
    }
    rows.clone()
}

/*
Stamped onto read results and onto every frame that can carry live state. Unlike
`terminalNotice`, an omitted field does NOT mean "cleared": these rows retire on
their own schedule and on the client's dedupe, so a frame that has nothing to add
simply says nothing rather than racing the client into dropping a row it should
still be showing.
*/
pub fn insert_session_chat_app_commands(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    let rows = session_chat_app_commands(project_id, session_id);
    if rows.is_empty() {
        return;
    }
    frame.insert(
        "appCommands".to_string(),
        Value::Array(rows.iter().map(SessionChatAppCommand::to_value).collect()),
    );
}

/*
What the 500ms long-poll fingerprint hashes. A bare rename's resolved title can
arrive under an existing id, so the title participates in the identity. This
must stay allocation-cheap and I/O-free like every other term in that hash.
*/
pub fn session_chat_app_commands_identity(project_id: &str, session_id: &str) -> String {
    session_chat_app_commands(project_id, session_id)
        .into_iter()
        .map(|row| {
            format!(
                "{}\u{1e}{}\u{1e}{}\u{1e}{}",
                row.id,
                row.title.as_deref().unwrap_or_default(),
                row.output.as_deref().unwrap_or_default(),
                row.goal
                    .as_ref()
                    .map(|goal| goal.identity())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\u{1f}")
}
