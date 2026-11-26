use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionChatMessageSource {
    Composer,
    AutomaticQueue,
    AutomaticRecovery,
    AccountSwitch(uuid::Uuid),
    ManualQueue,
    /// Ghostex's own notes to a coordinator, sent as a message of their own right after the user's
    /// (`send_ghostex_notes`).
    GhostexNote,
}

/*
CDXC:SessionChat 2026-08-21:
THE internal chat-message send. `/api/sendSessionChatMessage` is one caller;
the prompt queue ("Send now" and the scheduler) is the other, which is why it
lives here instead of inside the HTTP handler. Everything a chat send needs
travels with it — the per-session send mutex in session_chat_send/queue.rs, the
answerable-picker refusal, the terminal-input clear, the delivery watchdog and
the option re-detect — so a queued prompt is indistinguishable from one the user
typed and can never interleave with a Delayed Send.
Returns the number of text bytes handed to zmx.
*/
pub(crate) async fn send_session_chat_message_internal(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    source: SessionChatMessageSource,
) -> std::result::Result<usize, DomainStateError> {
    send_session_chat_message_with_draft(
        state,
        project_id,
        session_id,
        text,
        image_paths,
        source,
        None,
        false,
    )
    .await
}

/// The pause after an interrupt and send's Escape, so the agent ends its turn before the message
/// is typed instead of taking it into the turn it is stopping.
const INTERRUPT_BEFORE_SEND_SETTLE_MS: u64 = 1_000;
/// How much longer a send keeps watching the input box for a paste the first check missed.
const LATE_PASTE_WATCH_MS: u64 = 6_000;
/// The paste check of the one clear-and-retype attempt.
pub(super) const PASTE_RETRY_WATCH_MS: u64 = 6_000;

/// CDXC:SessionChat 2026-10-04 WHY:
/// "The terminal did not accept the pasted message" is decided before Return is written, so nothing was submitted, and under load it is usually a paste that lands late, not one that was lost: on Windows a 561-byte message gets a 2-second check, and a coordinator's sends failed right after a gxserver restart and while a release build ran, then went through unchanged a minute later. The diagnostics of the 07:03 failure show the first retry here (a fixed 1.5 s pause, then the whole send again) pressing Ctrl+C on the first paste, which had arrived by then, and its second paste missing the same 2-second window. So the send first keeps watching the input box and submits the first paste when it shows up; only when it never does is the box cleared (the send's own verified clear) and the message typed once more, with a longer check. Not for image or slash-command sends, whose steps do more than type text.
fn paste_not_accepted(error: &crate::session_chat_send::SessionChatSendError) -> bool {
    error.failure == crate::session_chat_send::SessionChatSendFailure::Write
        && error.message == crate::session_chat_send::SESSION_CHAT_PASTE_NOT_ACCEPTED
}

/// Which step of a refused send failed, for the delivery card.
fn send_write_failure(
    error: &crate::session_chat_send::SessionChatSendError,
) -> crate::session_chat_watchdog::SendWriteFailure {
    use crate::session_chat_watchdog::SendWriteFailure;
    if paste_not_accepted(error) {
        SendWriteFailure::PasteNotShown
    } else if error.message == crate::session_chat_send::SESSION_CHAT_PASTE_DOUBLED {
        SendWriteFailure::PasteDoubled
    } else if crate::session_chat_send_submit::is_not_submitted_failure(&error.message) {
        SendWriteFailure::NotSubmitted
    } else if error.message == crate::session_chat_send::SESSION_CHAT_BUSY_AGENT_STALLED {
        SendWriteFailure::AgentStalled
    } else {
        SendWriteFailure::TerminalUnresponsive
    }
}

/// The send's steps from its paste check on (the check, Return, the submit check), with the
/// check watching longer and without the settle the paste no longer needs.
fn late_paste_steps(
    steps: &[crate::session_chat_send::SessionChatSendStep],
) -> Option<Vec<crate::session_chat_send::SessionChatSendStep>> {
    use crate::session_chat_send::SessionChatSendStep;
    let start = steps
        .iter()
        .position(|step| matches!(step, SessionChatSendStep::VerifyPasteLanded { .. }))?;
    let mut late = steps[start..].to_vec();
    if let SessionChatSendStep::VerifyPasteLanded {
        settle_ms,
        timeout_ms,
        ..
    } = &mut late[0]
    {
        *settle_ms = 0;
        *timeout_ms = LATE_PASTE_WATCH_MS;
    }
    Some(late)
}

pub(super) fn with_paste_watch_of_at_least(
    mut steps: Vec<crate::session_chat_send::SessionChatSendStep>,
    watch_ms: u64,
) -> Vec<crate::session_chat_send::SessionChatSendStep> {
    for step in &mut steps {
        if let crate::session_chat_send::SessionChatSendStep::VerifyPasteLanded {
            timeout_ms, ..
        } = step
        {
            *timeout_ms = (*timeout_ms).max(watch_ms);
        }
    }
    steps
}

/// The retype's steps with `KeepSinglePaste` after its paste check, for an agent whose input box
/// the send reads and whose verified clear sends no keys to an empty box.
pub(super) fn with_single_paste_guard(
    mut steps: Vec<crate::session_chat_send::SessionChatSendStep>,
    agent: Option<&str>,
) -> Vec<crate::session_chat_send::SessionChatSendStep> {
    use crate::session_chat_send::SessionChatSendStep;
    let Some(agent) = crate::agents::identity::normalize_agent_id(agent)
        .filter(|agent| crate::session_chat_send::keeps_single_paste(agent))
    else {
        return steps;
    };
    let check = steps
        .iter()
        .enumerate()
        .find_map(|(index, step)| match step {
            SessionChatSendStep::VerifyPasteLanded { text, .. } => Some((index, text.clone())),
            _ => None,
        });
    if let Some((index, text)) = check {
        steps.insert(
            index + 1,
            SessionChatSendStep::KeepSinglePaste { agent, text },
        );
    }
    steps
}

/// CDXC:SessionChat 2026-10-05 WHY:
/// The clear-and-retype attempt only runs when two paste checks found no message in the input box, so nothing was submitted; but an agent that takes a paste without the Return (or a screen read that missed a submitted turn) would get the message twice from it. The retype first asks the agent's transcript whether a user turn since this send already carries the message, and settles the send as delivered when it does.
pub(super) async fn paste_already_recorded(session: &Value, text: &str, since_ms: i64) -> bool {
    let needles = crate::coordinators::delivery_needles(text);
    let session = session.clone();
    tokio::task::spawn_blocking(move || {
        crate::coordinators::transcript_records_message(
            &session,
            &needles,
            since_ms,
            &mut Default::default(),
        )
    })
    .await
    .ok()
    .flatten()
        == Some(true)
}

/// The notes that go under a message to a coordinator: the playbook note with the guide version it
/// is about, and the drift reminder or compaction note (coordinators/drift.rs) with the record to
/// keep once the message is delivered.
struct CoordinatorNotes {
    playbook: Option<(String, String)>,
    drift: Option<crate::coordinators::DriftDelivery>,
    /// Post-compaction standing role reminder for any session on an agent whose hooks cannot
    /// inject SessionStart context (agents/standing_role.rs).
    standing_role: Option<crate::agents::standing_role::StandingRoleDelivery>,
    /// The questions still waiting on the user, by title (`open_questions_note`).
    open_questions: Option<String>,
    /// The questions the user dismissed since the agent's last note, as the note names them, and
    /// the note (`crate::session_chat_dismissed`). Any session, not only a coordinator.
    dismissed: Option<(Vec<String>, String)>,
}

impl CoordinatorNotes {
    /// Whether a note asks the agent to do something: the playbook or compaction note ("run
    /// `ghostex coordinator guide`"), a standing-role post-compaction note, or a drift "Reminder:".
    /// The open questions list and the dismissed questions only inform, so they never wake an idle
    /// agent (`send_ghostex_notes`).
    ///
    /// CDXC:Coordinators 2026-11-18 DECISION:
    /// User picked "A + C" (Q3887) after "i dont like that we tell the agent to respond with ok in
    /// some cases": a message that needs nothing from an agent should not wake it at all (A), and
    /// the chat hides a bare "ok" (C, gx-chat-core `session::appended_notes`). Notes that only
    /// inform are held while the agent is idle and go with its next message, the user's or a
    /// thread's report, in the same separate system message; notes that ask for action go at once.
    fn asks_action(&self) -> bool {
        self.playbook.is_some() || self.drift.is_some() || self.standing_role.is_some()
    }

    /// Every note, in the order they go: the playbook note, the drift reminder, the standing role
    /// compaction note, the open questions, the dismissed questions.
    fn all(&self) -> Vec<&str> {
        self.playbook
            .iter()
            .map(|(note, _)| note.as_str())
            .chain(
                self.drift
                    .iter()
                    .flat_map(|drift| drift.notes.iter().map(String::as_str)),
            )
            .chain(self.standing_role.as_ref().map(|role| role.note.as_str()))
            .chain(self.open_questions.as_deref())
            .chain(self.dismissed.as_ref().map(|(_, note)| note.as_str()))
            .collect()
    }
}

/// The notes that go under this message to an orchestrator (CDXC:Coordinators 2026-11-05 in
/// coordinators/playbook.rs, CDXC:Coordinators 2026-11-13 in coordinators/drift.rs), and to any
/// session its post-compaction standing role note (agents/standing_role.rs) and the questions the
/// user dismissed since its last note (session_chat_dismissed.rs).
/// Only messages someone sent (the chat box, thread reports, `ghostex agents send`, queued rows)
/// carry them, never a slash or shell command, whose text the agent runs instead of reading.
fn coordinator_playbook_note(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    source: SessionChatMessageSource,
) -> Option<CoordinatorNotes> {
    if !matches!(
        source,
        SessionChatMessageSource::Composer
            | SessionChatMessageSource::AutomaticQueue
            | SessionChatMessageSource::ManualQueue
    ) {
        return None;
    }
    let first = text.trim_start().chars().next()?;
    if first == '/' || first == '!' {
        return None;
    }
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    let key = (project_id.to_string(), session_id.to_string());
    let dismissed = {
        let names = crate::session_chat_dismissed::untold(&db, project_id, session_id);
        ghostex_gx_chat_core::session::appended_notes::dismissed_note(&names)
            .map(|note| (names, note))
    };
    let playbook = crate::coordinators::playbook_update_note(&db, &repository, &key)
        .ok()
        .flatten();
    let session = repository
        .get_session(project_id, session_id)
        .ok()
        .flatten();
    let drift = session.as_ref().and_then(|session| {
        crate::coordinators::coordinator_drift_delivery(&db, &state.paths, &key, session)
    });
    let coordinator_compaction_noted = drift.as_ref().is_some_and(|drift| {
        drift.notes.iter().any(|note| {
            note == ghostex_gx_chat_core::session::appended_notes::CONTEXT_COMPACTED_NOTE
        })
    });
    let standing_role = session.as_ref().and_then(|session| {
        crate::agents::standing_role::standing_role_compaction_delivery(
            &db,
            &key,
            session,
            coordinator_compaction_noted,
        )
    });
    let open_questions = session
        .as_ref()
        .and_then(|session| open_questions_note(state, &db, &key, session, text));
    (playbook.is_some()
        || drift.is_some()
        || standing_role.is_some()
        || open_questions.is_some()
        || dismissed.is_some())
    .then_some(CoordinatorNotes {
        playbook,
        drift,
        standing_role,
        open_questions,
        dismissed,
    })
}

/// The note under a user's message to a coordinator that names the questions still waiting on the
/// user, by title with the tags the agent knows them by: its work items' open asks ("W7 Q12") and
/// the forms and pages its replies asked ("Q12 to Q14"), leaving out the asks this message answers
/// by tag ("W7 Q12: …", settled on delivery by `settle_work_replies`). `None` for a session that
/// is not a coordinator, or nothing open. A thread's report carries it too, answering nothing.
/// Blocking.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User, on issue #627, picked "Tell the coordinator what's still open": an answer typed in the
/// normal chat box left its ask under Needs you because the coordinator did not connect the message
/// to it. The words of the note and how the chat hides it are gx-chat-core's
/// (`session::appended_notes::open_questions_note`).
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User, of two messages in a row carrying the same list: "we already told the agent about this
/// no need to spam him like this". The note goes only when the list differs from the last one the
/// agent was sent (`OPEN_QUESTIONS_SENT`, recorded on delivery): a question added or one closed.
/// And of "Which of these is what you meant?" (Q985), a form from weeks before that the dock no
/// longer showed: a form or page put off For later, removed, or asked more than
/// `OPEN_QUESTIONS_MAX_AGE` ago is not listed; a work item's ask always is, as the dock shows it.
fn open_questions_note(
    state: &AppState,
    db: &rusqlite::Connection,
    key: &(String, String),
    session: &Value,
    text: &str,
) -> Option<String> {
    crate::coordinators::read_coordinator(db, &key.0, &key.1)
        .ok()
        .flatten()?;
    // A message from another agent (a thread's report) answers nothing, whatever it quotes; it
    // still carries the list held back from an idle coordinator (`send_ghostex_notes`).
    let from_agent = text
        .lines()
        .any(|line| line.trim() == crate::coordinators::AGENT_MESSAGE_OPENER);
    let answered: Vec<(i64, String)> = if from_agent {
        Vec::new()
    } else {
        crate::coordinators::chat_replies(text)
            .into_iter()
            .map(|(number, tag, _)| (number, tag))
            .collect()
    };
    let mut open: Vec<(String, String)> = Vec::new();
    for item in crate::coordinators::list_work_items(db, key).unwrap_or_default() {
        if item.finished_at.is_some() || item.dropped_at.is_some() {
            continue;
        }
        for ask in item.asks.iter().filter(|ask| ask.is_open()) {
            if answered
                .iter()
                .any(|(number, tag)| *number == item.number && *tag == ask.tag)
            {
                continue;
            }
            let tags = if ask.tag.is_empty() {
                item.reference()
            } else {
                format!("{} {}", item.reference(), ask.tag)
            };
            open.push((ask.text.clone(), tags));
        }
    }
    use ghostex_gx_chat_core::transcript::open_questions::AskerKind;
    let put_off: Vec<String> = crate::session_chat_open_questions::later_ids(session)
        .into_iter()
        .chain(crate::session_chat_open_questions::removed_ids(session))
        .collect();
    // The forms and pages this message answers (`<ghostex-page-answers page="…">`): the
    // transcript does not hold the message yet, so the ledger still reads them as open.
    let answered_pages: Vec<String> = if from_agent {
        Vec::new()
    } else {
        ghostex_gx_chat_core::transcript::page_answers::split_all_page_answers(text)
            .0
            .into_iter()
            .map(|answers| answers.page_id)
            .collect()
    };
    let oldest = chrono::Utc::now().timestamp_millis() - OPEN_QUESTIONS_MAX_AGE_MS;
    for asker in crate::session_chat_open_questions::open_askers(state, &key.0, &key.1) {
        let tags = asker.waiting_tags();
        if tags.is_empty()
            || answered_pages.contains(&asker.id)
            || put_off.contains(&asker.id)
            || asker.asked_at().is_some_and(|at| at < oldest)
        {
            continue;
        }
        let title = match (&asker.kind, &asker.form) {
            (AskerKind::Form, Some(form)) if form.questions.len() == 1 => {
                form.questions[0].title.clone()
            }
            (AskerKind::Form, Some(form)) => form.title.clone(),
            (AskerKind::Page, _) => {
                crate::visual_pages::page_questions(&state.paths.root_dir, &asker.id)
                    .map(|(title, _)| title)
                    .unwrap_or_default()
            }
            _ => String::new(),
        };
        open.push((
            title,
            ghostex_gx_chat_core::transcript::open_questions::tag_range(&tags),
        ));
    }
    let note = ghostex_gx_chat_core::session::appended_notes::open_questions_note(&open);
    let sent = read_open_questions_sent(db, key);
    if note.is_none() && sent.is_some() {
        // Nothing open now: the next list, whatever it is, is news again.
        write_open_questions_sent(db, key, None);
    }
    note.filter(|note| sent.as_deref() != Some(note.as_str()))
}

/// A form or page asked longer ago than this is not listed in the open questions note.
const OPEN_QUESTIONS_MAX_AGE_MS: i64 = 3 * 24 * 60 * 60 * 1000;

/// The last open questions note a coordinator was sent, by session, in gxserver's `metadata`.
const OPEN_QUESTIONS_SENT: &str = "coordinatorOpenQuestionsSent";

fn open_questions_sent_key(key: &(String, String)) -> String {
    format!("{OPEN_QUESTIONS_SENT}:{}:{}", key.0, key.1)
}

fn read_open_questions_sent(db: &rusqlite::Connection, key: &(String, String)) -> Option<String> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [open_questions_sent_key(key)],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn write_open_questions_sent(
    db: &rusqlite::Connection,
    key: &(String, String),
    note: Option<&str>,
) {
    let metadata_key = open_questions_sent_key(key);
    let _ = match note {
        Some(note) => db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
            rusqlite::params![metadata_key, note, chrono::Utc::now().to_rfc3339()],
        ),
        None => db.execute("DELETE FROM metadata WHERE key = ?1", [metadata_key]),
    };
}

fn record_coordinator_playbook_note(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    notes: Option<&CoordinatorNotes>,
) {
    let Some(notes) = notes else {
        return;
    };
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let key = (project_id.to_string(), session_id.to_string());
        if let Some((_, version)) = &notes.playbook {
            let _ = crate::coordinators::record_playbook_update_note(&db, &key, version);
        }
        if let Some(drift) = &notes.drift {
            crate::coordinators::record_coordinator_drift(&db, &key, drift);
        }
        if let Some(standing_role) = &notes.standing_role {
            crate::agents::standing_role::record_standing_role_compaction(
                &db,
                &key,
                &standing_role.compaction,
            );
        }
        if let Some(note) = &notes.open_questions {
            write_open_questions_sent(&db, &key, Some(note));
        }
        if let Some((names, _)) = &notes.dismissed {
            crate::session_chat_dismissed::forget(&db, project_id, session_id, names);
        }
    }
}

/// One lock per session, held by a chat message send from its screen read to its last retype.
fn session_send_lock(project_id: &str, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: std::sync::OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        std::sync::OnceLock::new();
    let Ok(mut locks) = LOCKS.get_or_init(Default::default).lock() else {
        return Arc::new(tokio::sync::Mutex::new(()));
    };
    // Locks nobody holds or waits on are dropped, so the map stays as small as the live sessions.
    if locks.len() > 256 {
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
    }
    locks
        .entry(format!("{project_id}|{session_id}"))
        .or_default()
        .clone()
}

/// CDXC:SessionChat 2026-11-08 WHY:
/// GitHub #505/#506 (Windows 11, a coordinator): "a user message I type often fails with 'Your
/// message didn't arrive. The agent got a different message instead and answered that' … almost
/// every time a thread report is delivered to the coordinator at about the same moment as my
/// message. The report is typed into the terminal first; my text is left unsent in the input
/// box." The per-session job queue (session_chat_send/queue.rs) keeps one job's clear, paste and
/// Return together, but one message is several jobs: the main one, then on a slow screen the late
/// paste watch, the clear-and-retype and the heal's repaint-and-retype, each queued only after the
/// one before failed. A report's send and the user's send took turns between those jobs, so a
/// retype could clear or submit the other's text, and each read its screen and sampled the
/// transcript for its delivery check before the other had typed anything, so the other's row read
/// as "a different message instead". A whole send now holds this lock from its screen read to its
/// last retype: the user's message and the report go out one after the other, in the order they
/// arrived, and each reads the screen and the transcript as the previous one left them.
/// SEE-ALSO: session_chat_watchdog/delivery_match.rs (a message Ghostex typed is never "a
/// different message").
pub(crate) async fn send_session_chat_message_with_draft(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    source: SessionChatMessageSource,
    draft_version: Option<&crate::session_chat_draft_versions::DraftVersion>,
    interrupt: bool,
) -> std::result::Result<usize, DomainStateError> {
    let lock = session_send_lock(project_id, session_id);
    let _owned = lock.lock().await;
    // Read before the message goes, so the open questions it answers by tag are left out.
    let notes = coordinator_playbook_note(state, project_id, session_id, text, source);
    // What `agents send --with-next` held for this session goes with this message: inside it for
    // Claude Code, as messages of their own right after it for other agents (`send_held_with_next`).
    let held = if carries_held_notes(text, source) {
        crate::session_chat_send::take_held_with_next(state, project_id, session_id)
    } else {
        Vec::new()
    };
    // Claude Code gets both inside this message's `<ghostex-system>` block: a second submit typed
    // while it works is queued and, after a reply with no tool call, run as a turn of its own.
    let inside = (!held.is_empty() || notes.is_some())
        && takes_held_notes_inside(state, project_id, session_id);
    let riders: Vec<String> = if inside {
        notes
            .iter()
            .flat_map(CoordinatorNotes::all)
            .map(str::to_string)
            .chain(crate::session_chat_send::held_notes_block(&held))
            .collect()
    } else {
        Vec::new()
    };
    let sent = send_session_chat_message_locked(
        state,
        project_id,
        session_id,
        text,
        image_paths,
        source,
        draft_version,
        interrupt,
        &riders,
    )
    .await;
    if sent.is_ok() {
        settle_work_replies(state, project_id, session_id, text, image_paths, source);
        // Ask Ghostex AI's welcome card stops offering a prompt once it was sent.
        crate::chief::remember_asked(&state.paths, project_id, session_id, text);
        if inside {
            // They went in with the message: recorded as sent, like a note delivered on its own.
            record_coordinator_playbook_note(state, project_id, session_id, notes.as_ref());
        } else {
            send_held_with_next(state, project_id, session_id, held, source).await;
            if let Some(notes) = notes {
                send_ghostex_notes(state, project_id, session_id, &notes).await;
            }
        }
    } else {
        crate::session_chat_send::put_back_held_with_next(state, project_id, session_id, &held);
    }
    sent
}

/// Whether `text` carries what `agents send --with-next` held: a message someone sent (the chat
/// box, another session's message, a queued row), never a slash or shell command.
fn carries_held_notes(text: &str, source: SessionChatMessageSource) -> bool {
    matches!(
        source,
        SessionChatMessageSource::Composer
            | SessionChatMessageSource::AutomaticQueue
            | SessionChatMessageSource::ManualQueue
    ) && !matches!(text.trim_start().chars().next(), Some('/' | '!'))
}

/// Whether the held messages go inside the releasing message: Claude Code queues a message typed
/// while it works and, when its reply ends without a tool call, runs it as a turn of its own
/// (GitHub #677: one message and two held notes made three turns and three replies). Codex and
/// the others take a message typed mid-turn into the running turn, and show each note as the
/// agent message it is.
fn takes_held_notes_inside(state: &AppState, project_id: &str, session_id: &str) -> bool {
    open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| {
            crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str())
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        })
        .is_some_and(|session| {
            session_chat_agent_for_session(&session).as_deref() == Some("claude")
        })
}

/// The messages `agents send --with-next` held for this session, each typed as a message of its
/// own right after the one just sent, under the same send lock (session_chat_send/with_next.rs).
/// One that does not go is held again for the next.
async fn send_held_with_next(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    held: Vec<Value>,
    source: SessionChatMessageSource,
) {
    for (index, note) in held.iter().enumerate() {
        let message = crate::session_chat_send::held_note_message(note);
        let sent = Box::pin(send_session_chat_message_locked(
            state,
            project_id,
            session_id,
            &message,
            &[],
            source,
            None,
            false,
            &[],
        ))
        .await;
        if sent.is_err() {
            crate::session_chat_send::put_back_held_with_next(
                state,
                project_id,
                session_id,
                &held[index..],
            );
            return;
        }
    }
}

/// Ghostex's notes for a coordinator (the playbook note, the drift reminder, the compaction note,
/// the open questions), and for any session the questions the user dismissed since its last note,
/// as a message of their own, one `<ghostex-system>` block, typed right after
/// the user's message under the same send lock, so nothing else gets between them. The user's
/// message starts the turn at once and the note reaches the agent while it works: Claude Code and
/// Codex take a message submitted mid-turn into the running turn. Recorded as sent only once
/// delivered, so a note that did not go is offered again with the next message.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User (Q3557): the notes may show in the terminal but never in the chat view, and go "as their
/// own separate message, not inside the user's message". Before, they rode inside the user's
/// message (2026-11-05 to 2026-11-16); the chat drops a row that holds only the block
/// (gx-chat-core `session::appended_notes::is_system_only`).
///
/// CDXC:Coordinators 2026-11-20 DECISION:
/// Not for Claude Code any more. Typed while it works, Claude Code queues this second submit and,
/// when its reply ends without a tool call, runs it as a turn of its own, the same extra turn
/// GitHub #677 found for held `--with-next` notes. The user said yes to the follow-up ("should
/// ride in the next message's <ghostex-system> block (or the same paste) instead of a separate
/// submit, so it never costs an extra turn"): for Claude Code the notes go in the user's message's
/// own `<ghostex-system>` block, still kept out of every chat's view (`send_session_chat_message_with_draft`),
/// and this separate message stays for the other agents, which take it into the running turn.
async fn send_ghostex_notes(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    notes: &CoordinatorNotes,
) {
    let all = notes.all();
    let Some(message) = ghostex_gx_chat_core::session::appended_notes::system_message(&all) else {
        return;
    };
    if !notes.asks_action() && !agent_still_working(state, project_id, session_id).await {
        // Not recorded as sent, so the next message the agent gets (the user's or a thread's
        // report) carries it again, rebuilt as things then stand.
        return;
    }
    let sent = Box::pin(send_session_chat_message_locked(
        state,
        project_id,
        session_id,
        &message,
        &[],
        SessionChatMessageSource::GhostexNote,
        None,
        false,
        &[],
    ))
    .await;
    match sent {
        Ok(_) => record_coordinator_playbook_note(state, project_id, session_id, Some(notes)),
        Err(error) => {
            let _ = state.logger.log(crate::logging::GxserverLogInput {
                level: crate::logging::LogLevel::Warn,
                event: "coordinatorNotesNotSent".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.message),
                details: Some(json!({ "projectId": project_id, "sessionId": session_id })),
            });
        }
    }
}

/// How long `agent_still_working` waits for the message just sent to show as a running turn.
const NOTE_WORKING_WAIT: Duration = Duration::from_secs(5);

/// Whether the agent is working on the message just sent: its hooks or its transcript say a turn
/// runs. Read for up to `NOTE_WORKING_WAIT`, as a hook or the transcript row can land a moment
/// after the send; a turn that has already ended (a short reply with no tool call) reads as not.
async fn agent_still_working(state: &AppState, project_id: &str, session_id: &str) -> bool {
    let started = std::time::Instant::now();
    let gate = Arc::new(Mutex::new(SessionChatTranscriptGate::default()));
    loop {
        let (check_state, project, session_key, readings) = (
            state.clone(),
            project_id.to_string(),
            session_id.to_string(),
            gate.clone(),
        );
        let working = tokio::task::spawn_blocking(move || {
            let mut params = Map::new();
            params.insert("projectId".to_string(), json!(project));
            params.insert("sessionId".to_string(), json!(session_key));
            let Ok(target) = crate::session_chat_send::resolve_session_chat_send_target(
                &check_state,
                &params,
                "noteWorking",
            ) else {
                return false;
            };
            crate::session_chat_follower::session_chat_hook_working(&target.session)
                || readings
                    .lock()
                    .is_ok_and(|mut gate| gate.is_working(&target.session))
        })
        .await
        .unwrap_or(false);
        if working {
            return true;
        }
        if started.elapsed() >= NOTE_WORKING_WAIT {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// A user's message that reached a coordinator answers its open work asks named "W7 Q12: …"
/// (coordinators/work.rs `settle_chat_replies`), and one that reached any session answers its own
/// to-dos named by Reply in chat box pills (`answering:Q61`, session_needs_you
/// `settle_chat_answers`). Not a message from another agent, which may quote such a line, and not a
/// recovery or account-switch resend of one already sent.
fn settle_work_replies(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    source: SessionChatMessageSource,
) {
    let from_user = matches!(
        source,
        SessionChatMessageSource::Composer
            | SessionChatMessageSource::AutomaticQueue
            | SessionChatMessageSource::ManualQueue
    );
    let work_replies = !crate::coordinators::chat_replies(text).is_empty();
    let todo_answers = !crate::session_needs_you::todo_pill_answers(text).is_empty();
    if !from_user
        || (!work_replies && !todo_answers)
        || text
            .lines()
            .any(|line| line.trim() == crate::coordinators::AGENT_MESSAGE_OPENER)
        // Only text one of the user's own callers sent settles an ask, never text that merely
        // lacks the agent block (session_chat_send/sender_mark.rs).
        || !crate::session_chat_send::sender_mark::take_user_send(
            &state.paths,
            project_id,
            session_id,
            text,
        )
    {
        return;
    }
    if let Ok(db) = open_gxserver_database(&state.paths) {
        if work_replies {
            let key = (project_id.to_string(), session_id.to_string());
            let _ = crate::coordinators::settle_chat_replies(&db, &key, text, image_paths.len());
        }
        if todo_answers {
            let _ = crate::session_needs_you::settle_chat_answers(
                state,
                &db,
                project_id,
                session_id,
                text,
                image_paths.len(),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_session_chat_message_locked(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    source: SessionChatMessageSource,
    draft_version: Option<&crate::session_chat_draft_versions::DraftVersion>,
    interrupt: bool,
    riders: &[String],
) -> std::result::Result<usize, DomainStateError> {
    if let Some(version) = draft_version {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: error.to_string(),
        })?;
        crate::session_chat_draft_versions::require_saved(
            &db, project_id, session_id, text, version,
        )?;
    }
    // CDXC:Changes 2026-10-25 WHY: a Changes view review comment sits in the chat box as a pill; the agent reads it as the full block (file, comment, lines as a diff). It is expanded here, after the draft check, because gxserver sends only the exact text of the saved draft revision (the pills), and here every send and queued prompt passes on its way to the agent.
    let expanded = ghostex_gx_chat_core::composer::review_comments::expand_review_comments(text);
    // A message that points at earlier ones (`[M12](message:M12)`, `#M12`, `G9jt7.M12`) carries
    // the start of each, quoted at its bottom (CDXC:SessionChat 2026-11-20 in
    // chat_message_numbers.rs). Never under a slash or shell command, which the agent runs, and
    // only when the user's own caller sent it (the app, the web build, a phone, `--as-user`): a
    // Collab guest could otherwise pull words from the owner's other chats into the prompt, and
    // another agent's `#M3` means its own chat, not this one. The questions, work items and Live
    // pages the user's own words point at (`question:`, `board:`, `page:` links) are quoted after
    // them, in a block of their own (chat_item_quotes/).
    let expanded = if expanded.trim_start().starts_with(['/', '!'])
        || !crate::session_chat_send::sender_mark::is_user_send(
            &state.paths,
            project_id,
            session_id,
            text,
        ) {
        expanded
    } else {
        let expanded = crate::chat_message_numbers::with_message_quotes(
            &state.paths,
            project_id,
            session_id,
            &expanded,
        );
        crate::chat_item_quotes::with_item_quotes(state, project_id, session_id, text, &expanded)
    };
    // Each Boost pill's prompt goes under the message (CDXC:Boosts 2026-11-20 in boosts.rs).
    // Which Boosts it carried is kept for the anonymous `boosts.used` event sent after a
    // successful send (CDXC:Telemetry 2026-11-20 in telemetry/capture.rs).
    let (boosts_built_in, boosts_custom) = crate::boosts::used_in(text);
    let expanded = crate::boosts::with_boosts(&state.paths, &expanded);
    // Ghostex's notes and the held `--with-next` messages for Claude Code, in the block every chat
    // leaves out of the user's row (`takes_held_notes_inside`; CDXC:SessionChat 2026-11-20 in
    // with_next.rs, CDXC:Coordinators 2026-11-20 at `send_ghostex_notes`).
    let expanded = if riders.is_empty() {
        expanded
    } else {
        let riders: Vec<&str> = riders.iter().map(String::as_str).collect();
        ghostex_gx_chat_core::session::appended_notes::with_system_notes(&expanded, &riders)
    };
    let text = expanded.as_str();
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    // Serialize with any in-flight Terminal <-> Native switch on this session so a send never
    // races a sleep/wake cycle or lands on a stale transport (`acp::channel::session_switch_lock`).
    // Bound the wait well under the 15s CLI send budget (`ghostex_cli/rpc.rs`) so a slow switch
    // queues the message cleanly instead of timing out the caller.
    let switch_lock = crate::acp::channel::session_switch_lock(project_id, session_id);
    let _switch_guard = match tokio::time::timeout(Duration::from_secs(5), switch_lock.lock_owned())
        .await
    {
        Ok(guard) => guard,
        Err(_) => {
            if let Ok(target) =
                resolve_session_chat_send_target(state, &params, "sendSessionChatMessage")
            {
                return hold_for_restarted_agent(state, &target, source, text, image_paths);
            }
            return Err(DomainStateError {
                code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
                message: "This chat is switching between Terminal and Native mode; the message will be sent as soon as it is ready.".to_string(),
            });
        }
    };
    let target = resolve_session_chat_send_target(state, &params, "sendSessionChatMessage")?;
    let draft_before_send = if source == SessionChatMessageSource::Composer {
        open_gxserver_database(&state.paths).ok().and_then(|db| {
            read_session_chat_queue_snapshot_with(&db, &target.project_id, &target.session_id).draft
        })
    } else {
        None
    };
    if source == SessionChatMessageSource::Composer {
        crate::accounts::recovery::user_action(state, project_id, session_id, false)?;
    }
    // A session whose Claude login was disabled moves to another account before anything is typed (accounts/disabled.rs); the continuation and recovery sends are that switch's own.
    let switch_owned = matches!(
        source,
        SessionChatMessageSource::AutomaticRecovery | SessionChatMessageSource::AccountSwitch(_)
    );
    let target = if !switch_owned
        && crate::accounts::disabled::switch_before_send(state, project_id, session_id).await?
    {
        resolve_session_chat_send_target(state, &params, "sendSessionChatMessage")?
    } else {
        target
    };
    // A draft whose Run on row picked a box has no agent to type into yet: its first message
    // creates the box (agents/draft_run_location.rs).
    if let Some(sent) = super::box_first_send::send_pending_box_first_message(
        state,
        project_id,
        session_id,
        text,
        image_paths,
        source == SessionChatMessageSource::Composer,
        draft_before_send.as_ref(),
        draft_version,
    )
    .await
    {
        return sent;
    }
    // A Claude cloud session's terminal runs Ghostex's cloud client, not an agent to type into.
    let cloud_send = if target
        .session
        .pointer("/runtimeSettings/claudeCloud")
        .is_some_and(Value::is_object)
    {
        super::cloud_send::send_claude_cloud_message(
            state,
            project_id,
            session_id,
            text,
            image_paths,
            source == SessionChatMessageSource::Composer,
            draft_before_send.as_ref(),
            draft_version,
        )
        .await
    } else {
        None
    };
    if let Some(sent) = cloud_send {
        return sent;
    }
    let agent = session_chat_agent_for_session(&target.session);
    let terminal_agent =
        crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
            .or_else(|| agent.clone());
    if agent.as_deref() == Some("opencode")
        && !crate::acp::session_is_acp(&target.session)
        && crate::acp::channel::live(&target.project_id, &target.session_id, "send").is_none()
    {
        let conversation = crate::session_chat_opencode::chat_conversation(
            state,
            &target.project_id,
            &target.session_id,
            &target.session,
        )?;
        let id = conversation.id.clone();
        let send_id = id.clone();
        let directory = conversation.directory;
        let message = text.to_string();
        let images = image_paths.to_vec();
        let session = target.session.clone();
        let message_id = draft_version.map(|version| {
            use sha2::{Digest, Sha256};
            let identity =
                json!([project_id, session_id, version.draft_id, version.revision]).to_string();
            format!(
                "msg_gx_{}",
                hex::encode(Sha256::digest(identity.as_bytes()))
            )
        });
        let archive = tokio::task::spawn_blocking(move || {
            let mut archive =
                crate::session_chat_local_command::prepare_session_chat_local_command(&message);
            if let Some(command) = archive.as_mut() {
                crate::session_chat_local_command::anchor_session_chat_local_command(
                    command, &session,
                );
            }
            let client =
                crate::session_chat_opencode::ensure_conversation(&send_id, directory.as_deref())?;
            if interrupt {
                crate::session_chat_opencode::interrupt(&send_id, None)?;
            }
            client.send(&send_id, &message, &images, message_id.as_deref())?;
            Ok::<_, DomainStateError>(archive)
        })
        .await
        .map_err(|_| crate::session_chat_opencode::error("OpenCode delivery task failed."))
        .and_then(|result| result);
        let archive = match archive {
            Ok(archive) => archive,
            Err(error) => {
                crate::telemetry::reliability_error("prompt_delivery_failed", Some("opencode"));
                return Err(error);
            }
        };
        if let Some(command) = archive {
            crate::session_chat_local_command::persist_session_chat_local_command(
                project_id, session_id, &command,
            );
        }
        crate::session_chat_opencode::invalidate(&id);
        let prompt_source = prompt_source_for_message_source(source);
        crate::telemetry::prompt_sent(&target.session, prompt_source);
        crate::telemetry::boosts_used(
            &target.session,
            prompt_source,
            boosts_built_in,
            boosts_custom,
        );
        promote_draft_session_after_send(state, project_id, session_id);
        unpark_session_after_send(state, project_id, session_id);
        crate::session_chat_returned_prompt::record_session_chat_send_submitted(
            project_id, session_id,
        );
        if let Some(version) = draft_version {
            let db = open_gxserver_database(&state.paths)
                .map_err(|e| crate::session_chat_opencode::error(e.to_string()))?;
            crate::session_chat_draft_versions::consume(&db, project_id, session_id, version)?;
            broadcast_session_chat_queue_state(state, project_id, session_id);
        } else if source == SessionChatMessageSource::Composer {
            retire_sent_session_chat_draft(
                state,
                project_id,
                session_id,
                text,
                draft_before_send.as_ref(),
            );
        }
        schedule_session_chat_option_redetect(state, project_id, session_id, Some("opencode"));
        return Ok(text.len());
    }
    // The draft was checked against the text as written; the terminal, the delivery watchdog and
    // the local-command archive see what the agent is actually handed (a Claude skill pill typed as
    // its bare `/name`, a Hermes `/rename` as `/title`).
    let drafted_text = text;
    let agent_text =
        crate::session_chat_skill_invocation::agent_skill_text(terminal_agent.as_deref(), text);
    let agent_text = crate::server::title_generation::chat_rename_as_agent_title_command(
        terminal_agent.as_deref(),
        &agent_text,
    )
    .map_or(agent_text, std::borrow::Cow::Owned);
    let agent_text =
        match crate::session_chat_send::picture_terminal_control_characters(&agent_text) {
            std::borrow::Cow::Owned(pictured) => std::borrow::Cow::Owned(pictured),
            std::borrow::Cow::Borrowed(_) => agent_text,
        };
    let text = agent_text.as_ref();
    // An ACP-mode session running inside `ghostex acp-host` takes messages directly over ACP
    // (`session/prompt`) without typing into a terminal (CDXC:AgentProviders 2026-11-22 in acp/mod.rs).
    // If a live authenticated `acp-host` channel is open while stored `nonCliMode` is out of sync,
    // reconcile the stored mode and deliver over the Native channel instead of waiting on a CLI box.
    let live_acp_channel =
        crate::acp::channel::live(&target.project_id, &target.session_id, "send");
    if crate::acp::session_is_acp(&target.session) || live_acp_channel.is_some() {
        if !crate::acp::session_is_acp(&target.session) {
            if let Some(channel) = live_acp_channel.as_ref() {
                crate::acp::channel::reconcile_live_host_mode_locked(
                    state,
                    &target.project_id,
                    &target.session_id,
                    &channel.agent,
                    "send_path",
                )
                .await;
            }
        }
        // If the Native host is not connected because the session is sleeping, resolved/closed, or
        // its host exited inside zmx, wake or restart it immediately on send (matching Terminal's
        // `starting_session_refusal` path) rather than leaving the queued row idle until backoff.
        let session_running =
            crate::presentation::effective_lifecycle_state(&target.session) == "running";
        if !session_running || live_acp_channel.is_none() {
            if let Some(refusal) =
                crate::session_chat_send_wake::wake_missing_acp_session(state, &target).await
            {
                return Err(refusal);
            }
        }
        // Keep strict FIFO order: if an earlier send is already queued for this Native session,
        // queue this normal send behind it rather than jumping ahead.
        if source == SessionChatMessageSource::Composer
            && !interrupt
            && crate::session_chat_send_wake::has_pending_startup_send(
                state,
                &target.project_id,
                &target.session_id,
            )
        {
            if crate::session_approval::session_awaits_answer(&target.session) {
                return Err(DomainStateError {
                    code: crate::session_chat_send::SESSION_CHAT_PROMPT_WAITING_CODE,
                    message: "An earlier message is waiting for your answer; this message is queued behind it.".to_string(),
                });
            }
            let generated_at = chrono::Utc::now().to_rfc3339();
            if crate::presentation::presentation_activity(&target.session, &generated_at)
                == "working"
            {
                return Err(DomainStateError {
                    code: crate::session_chat_send_wake::SESSION_CHAT_AGENT_BUSY,
                    message: "An earlier message is queued while the agent replies; this message is queued behind it.".to_string(),
                });
            }
            return Err(DomainStateError {
                code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
                message: "An earlier message is waiting for the agent to start.".to_string(),
            });
        }
        if interrupt {
            crate::session_chat_send::record_session_chat_interrupt(
                state,
                &target,
                "session-chat-interrupt-and-send",
            );
        }
        return match crate::acp::channel::send(&state.paths, &target, text, image_paths, interrupt)
            .await
        {
            Ok(_) => {
                crate::session_chat_send::clear_session_chat_interrupt(
                    state,
                    &target,
                    "session-chat-send-clear-interrupt",
                );
                crate::telemetry::prompt_sent(
                    &target.session,
                    match source {
                        SessionChatMessageSource::Composer => "chat",
                        _ => "queue",
                    },
                );
                promote_draft_session_after_send(state, &target.project_id, &target.session_id);
                unpark_session_after_send(state, &target.project_id, &target.session_id);
                crate::session_chat_returned_prompt::record_session_chat_send_submitted(
                    &target.project_id,
                    &target.session_id,
                );
                if let Some(version) = draft_version {
                    if let Ok(db) = open_gxserver_database(&state.paths) {
                        let _ = crate::session_chat_draft_versions::consume(
                            &db,
                            &target.project_id,
                            &target.session_id,
                            version,
                        );
                    }
                    broadcast_session_chat_queue_state(
                        state,
                        &target.project_id,
                        &target.session_id,
                    );
                } else if source == SessionChatMessageSource::Composer {
                    retire_sent_session_chat_draft(
                        state,
                        &target.project_id,
                        &target.session_id,
                        drafted_text,
                        draft_before_send.as_ref(),
                    );
                }
                Ok(text.len())
            }
            Err(error) => Err(error),
        };
    }
    // A Pi or OMP whose Ghostex extension has its chat channel open takes the message through Pi's
    // own API, without the screen checks and typing below (CDXC:PiChatChannel 2026-11-05).
    if matches!(terminal_agent.as_deref(), Some("pi" | "omp")) {
        if let Some(sent) = super::pi_channel_send::send_pi_channel_message(
            state,
            &target,
            text,
            image_paths,
            interrupt,
            source,
            drafted_text,
            draft_before_send.as_ref(),
            draft_version,
        )
        .await
        {
            return sent;
        }
    }
    // DeepSeek Harness takes it through Ghostex's dsh-tui plugin or dsh-tui's inject socket
    // (CDXC:DshChatChannel 2026-11-05 in dsh_chat_plugin.rs).
    if terminal_agent.as_deref() == Some("dsh") {
        if let Some(sent) = super::dsh_channel_send::send_dsh_channel_message(
            state,
            &target,
            text,
            image_paths,
            interrupt,
            source,
            drafted_text,
            draft_before_send.as_ref(),
            draft_version,
        )
        .await
        {
            return sent;
        }
    }
    // Everything below reads and types through zmx (CDXC:Zmx 2026-11-05 in send_heal.rs).
    if let Some(held) = super::send_heal::hold_while_gxserver_tools_missing(
        state,
        &target,
        source,
        drafted_text,
        image_paths,
    )
    .await
    {
        return held;
    }
    /*
    CDXC:AgentScreenDetection 2026-08-19:
    Sample where the transcript ends BEFORE the message is enqueued: everything
    written past this offset is a candidate for "the agent recorded it". Sampling
    afterwards would race the agent's own write. This is the only work the
    watchdog puts in front of a send — a path resolve plus one `metadata()`, both
    on a blocking thread — and it is skipped entirely for agents the watchdog
    does not cover.
    */
    /*
    CDXC:SessionChat 2026-08-21:
    Claude Code's resume-usage picker owns the input line when a large session
    is resumed. This send used to answer it automatically ("Resume full session
    as-is") before typing, which was wrong twice over: the summary-vs-full
    trade-off is the user's to make, and whenever the walk missed, the message's
    own trailing Enter confirmed the HIGHLIGHTED row instead — silently
    compacting the conversation the user was continuing, with nothing to show
    for it but a delivery-failed banner.

    So the send refuses instead. The same capture that proves the picker is up
    caches the notice carrying its rows, and publishing it puts the answer
    picker in front of the user on every subscribed client. Only an ANSWERABLE
    state stops a send here: the catalog's other blocking dialogs still go
    through to the delivery watchdog, which is the only thing that can explain
    them.
    */
    /*
    CDXC:SessionChat 2026-08-26:
    Terminal capture must use the concrete CLI identity, not the normalized
    transcript family. Omp shares Pi's transcript decoder but paints different
    composer and statusline chrome, so folding it to Pi here loses both screen
    readings. Agents without a concrete screen identity fall back to the
    transcript family.
    */
    let detection = SessionChatOptionDetector::new(state)
        .detect(
            &target.project_id,
            &target.session_id,
            terminal_agent.as_deref(),
            true,
        )
        .await;
    let dismiss_claude_panel = detection.composer.should_dismiss();
    if let Some(blocking) = detection.notice.as_ref().filter(|notice| {
        notice.is_answerable() && !escape_closes_claude_panel(&detection.composer, notice)
    }) {
        session_chat_terminal_notice_publisher(state, &target.project_id, &target.session_id)();
        return Err(DomainStateError {
            code: "invalidState",
            message: format!("{}. Answer it in chat before sending.", blocking.title),
        });
    }
    if source == SessionChatMessageSource::Composer {
        if let Some(starting) =
            crate::session_chat_send_wake::starting_session_refusal(state, &target, &detection)
                .await
        {
            return Err(starting);
        }
    }
    // A queued row delivered to a sleeping session wakes it and waits for the woken agent as a
    // startup send (CDXC:SessionChat 2026-10-29 in session_chat_send_wake.rs).
    if matches!(
        source,
        SessionChatMessageSource::AutomaticQueue | SessionChatMessageSource::ManualQueue
    ) {
        if let Some(refusal) =
            crate::session_chat_send_wake::queued_delivery_wake_refusal(state, &target, &detection)
                .await
        {
            if refusal.code != crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING {
                return Err(refusal);
            }
            return hold_for_restarted_agent(state, &target, source, drafted_text, image_paths);
        }
    }
    // Recheck automatic delivery against the fresh capture: the scheduler's
    // cached notice may predate a quota, authentication, or agent error.
    // Explicit Send now is a retry.
    if matches!(
        source,
        SessionChatMessageSource::AutomaticRecovery | SessionChatMessageSource::AccountSwitch(_)
    ) {
        let current = resolve_session_chat_send_target(state, &params, "automaticRecovery")?;
        let armed = current
            .session
            .pointer("/runtimeSettings/accountRecovery/status")
            .and_then(Value::as_str)
            == Some("retrying")
            && match source {
                SessionChatMessageSource::AccountSwitch(claim) => {
                    current
                        .session
                        .pointer("/runtimeSettings/accountRecovery/claim")
                        .and_then(Value::as_str)
                        == Some(claim.to_string().as_str())
                }
                _ => true,
            };
        let blocked = detection.notice.as_ref().is_some_and(|n| {
            n.blocks_queued_delivery()
                && !matches!(n.kind.as_str(), "streamError" | "usageLimit" | "agentError")
        });
        // CDXC:AgentProviders 2026-09-11 WHY:
        // A resumed CLI repaints the finished turn, and the activity detector classifies its rows ("✻ Sautéed for 2s · done", the last ⏺ message) as activity. Taking that raw reading for "still working" held the continuation dot back for as long as those rows stayed on screen. Activity counts only while the session's hooks say it is working, or while the CLI compacts, the same rule the recovery pass uses.
        let working = crate::presentation::presentation_activity(
            &current.session,
            &chrono::Utc::now().to_rfc3339(),
        ) == "working";
        let live_activity = detection.activity.as_ref().is_some_and(|activity| {
            working
                || activity.kind
                    == crate::session_chat_terminal_activity::SESSION_CHAT_ACTIVITY_COMPACTING
        });
        if !armed
            || !detection.captured
            || detection.composer.state
                != crate::session_chat_composer::SessionChatComposerState::Ready
            || detection.prompt.is_some()
            || crate::session_chat_send::transcript_pending_question_prompt(&current.session)
                .is_some()
            || live_activity
            || blocked
        {
            return Err(DomainStateError {
                code: "accountRecoveryNotReady",
                message: "Automatic recovery is waiting for the session to be ready.".into(),
            });
        }
    }
    if source == SessionChatMessageSource::AutomaticQueue {
        if let Some(notice) = detection.notice.as_ref().filter(|notice| {
            notice.blocks_queued_delivery()
                && !escape_closes_claude_panel(&detection.composer, notice)
        }) {
            return Err(DomainStateError {
                code: "invalidState",
                message: format!("{}. The queued message was not sent.", notice.title),
            });
        }
    }
    /*
    CDXC:SessionChat 2026-08-26:
    The positive gate. Everything above is "is a screen we RECOGNISE in the
    way?"; this is "did the CLI paint an input box at all?". It catches the
    states no notice rule covers — a CLI still booting, an auth screen shipped
    after our catalog, a dialog we have never seen — which are exactly the ones
    that used to eat a message and answer with a delivery-failed banner minutes
    later.

    `Unknown` FAILS OPEN and is by far the common case for unmeasured agents, so
    this can only ever refuse a send it has positive evidence about. The screen
    tail behind the verdict is not squeezed into the error (DomainStateError
    carries a code and a message and nothing else, at 169 construction sites);
    clients read it from /api/readSessionTerminalTail instead.
    */
    if let Some(held) = restart_exited_agent_before_send(
        state,
        &target,
        &detection,
        terminal_agent.as_deref(),
        source,
        drafted_text,
        image_paths,
    )
    .await
    {
        return held;
    }
    let redraw_claude_composer = detection.prompt.is_none()
        && !detection
            .notice
            .as_ref()
            .is_some_and(|notice| notice.blocks_input() || notice.is_answerable())
        && crate::session_chat_send::claude_composer_needs_redraw(
            terminal_agent.as_deref(),
            &detection.composer,
        );
    if detection
        .composer
        .blocks_message_for(terminal_agent.as_deref())
        && !dismiss_claude_panel
        && !redraw_claude_composer
    {
        return Err(DomainStateError {
            code: "composerNotReady",
            message: detection
                .composer
                .reason
                .clone()
                .unwrap_or_else(|| "The agent's input box is not accepting input yet.".to_string()),
        });
    }
    if dismiss_claude_panel {
        crate::session_chat_send_diagnostics::record_send_recovery(
            state,
            "sessionChatSendClosingClaudePanel",
            &target.project_id,
            &target.session_id,
            detection.composer.reason.as_deref().unwrap_or_default(),
            &detection.composer.screen_tail,
        );
    }
    // Ghostex's own note (`send_ghostex_notes`) is never watched: a note that did not land must not
    // come back as the user's message or raise a delivery card; it goes again with the next one.
    let ghostex_note = source == SessionChatMessageSource::GhostexNote;
    let send_probe = if ghostex_note {
        None
    } else {
        crate::session_chat_watchdog::SessionChatSendProbe::sample(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
            agent.as_deref(),
            read_runtime_text(&target.session, "agentSessionId").as_deref(),
            read_runtime_text(&target.session, "agentSessionPath").as_deref(),
            text,
        )
        .await
    };
    /*
    Chat owns the terminal composer when it sends. Discard anything already
    sitting on that hidden input line instead of turning it into a user-facing
    Saved Prompt: `build_session_chat_message_steps` starts with the measured
    agent-specific clear, verifies it where supported, and only then pastes this message.
    Terminal -> Chat view switching remains the separate, loss-safe draft
    transfer path for text the user actually wants to carry between views.
    */
    let mut steps = crate::session_chat_send::build_session_chat_message_steps(
        terminal_agent.as_deref(),
        text,
        image_paths,
        dismiss_claude_panel,
    );
    if terminal_agent.as_deref() == Some("claude") {
        crate::session_chat_send::remember_claude_sent_pictures(
            &target.project_id,
            &target.session_id,
            text,
            image_paths,
        );
    }
    /*
    CDXC:SessionChat 2026-10-11 DECISION:
    User: "I'm having to switch to terminal to press ctrl + return in order to interrupt and send a prompt. should be able to send this from the chat view." Interrupt and send stops the agent's running turn and sends the message right away. Claude Code gets its own key for it after the message (Return queues the message while a turn runs, then `chat:sendNow` stops the turn and sends the queue). Every other agent gets chat Stop's Escape (Ctrl+C for Cursor) before the message, and only while it is mid-turn, so an idle agent just receives the message.
    WHY: Claude's Ctrl+Enter only reaches it as a CSI-u sequence, so the send types the other binding of the same action, Ctrl+X Ctrl+S, as plain control bytes; verified live on Claude Code 2.1.296 through wmx (the turn stopped and the queued message was answered; with no turn running it does nothing). Escape first would make Claude hand its queued messages back to the input box, which the send's clear then erases. The Escape for the other agents is its own job, awaited before the message, because the interrupt guards end their job, which would drop the message with it.
    */
    let claude_send_now = interrupt && terminal_agent.as_deref() == Some("claude");
    if claude_send_now {
        steps.push(crate::session_chat_send::SessionChatSendStep::Write(
            crate::session_chat_send::SESSION_CHAT_CLAUDE_SEND_NOW.to_string(),
        ));
    }
    /*
    CDXC:SessionChat 2026-09-10 DECISION:
    User: a slash command sent from chat must show up in chat for every agent,
    with its output, and still be there after a reload.

    Codex keeps its curated list because its commands print into a repainting
    TUI and the list is what says which ones print at all. Every other agent
    takes any line-leading slash command: Claude records a transcript envelope
    for a handful of its own and nothing for the rest, so guessing which ones
    print would just recreate the gap this closes. The archived row is written
    after a successful send, even if the capture finds nothing; failed sends leave no history.
    Commands whose result the transcript already records are left out: their
    status pill (or compaction row) is the one result row they get.
    */
    let local_command = crate::session_chat_local_command::parse_session_chat_local_command(text)
        .filter(|(command, _)| {
            // Empryo records no command in its session log, `/effort` included.
            terminal_agent.as_deref() == Some("empryo")
                || !crate::session_chat_local_command::transcript_records_command_result(command)
        });
    let capture_local_output = if terminal_agent.as_deref() == Some("codex") {
        crate::session_chat_codex_dialog::command_has_local_output(text)
    } else {
        local_command.is_some()
    };
    let mut archive_command = local_command
        .as_ref()
        .and_then(|_| crate::session_chat_local_command::prepare_session_chat_local_command(text));
    if let Some(mut command) = archive_command.take() {
        let session = target.session.clone();
        archive_command = tokio::task::spawn_blocking(move || {
            crate::session_chat_local_command::anchor_session_chat_local_command(
                &mut command,
                &session,
            );
            command
        })
        .await
        .ok();
    }
    let durable_id = archive_command.as_ref().map(|row| row.id.clone());
    if capture_local_output {
        steps.insert(
            0,
            crate::session_chat_send::SessionChatSendStep::BeginLocalCommandOutput {
                agent: terminal_agent.clone(),
                command: text.to_string(),
                durable_id: durable_id.clone(),
            },
        );
        steps.push(crate::session_chat_send::SessionChatSendStep::FinishLocalCommandOutput);
    }
    steps.insert(
        0,
        crate::session_chat_send::SessionChatSendStep::StopLocalCommandOutput,
    );
    let mid_turn_probe = super::agent_mid_turn_probe(state, &target, terminal_agent.as_deref());
    if interrupt && !claude_send_now && mid_turn_probe.mid_turn().await {
        let mut interrupt_steps =
            crate::session_chat_send::session_chat_interrupt_escape_steps(agent.as_deref());
        interrupt_steps.push(crate::session_chat_send::SessionChatSendStep::SleepMs(
            INTERRUPT_BEFORE_SEND_SETTLE_MS,
        ));
        crate::session_chat_send::execute_session_chat_send(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
            "session-chat-interrupt",
            interrupt_steps,
        )
        .await
        .map_err(|error| DomainStateError {
            code: "sessionInputFailed",
            message: error.message,
        })?;
        crate::session_chat_send::record_session_chat_interrupt(
            state,
            &target,
            "session-chat-interrupt-and-send",
        );
    }
    steps.insert(
        0,
        crate::session_chat_send::SessionChatSendStep::WaitOutBusyAgent(mid_turn_probe.clone()),
    );
    // The user's message stays the one an Escape hands back, not the note after it.
    if !ghostex_note {
        crate::session_chat_returned_prompt::record_session_chat_send_started(
            &target.project_id,
            &target.session_id,
            text,
            image_paths,
        );
    }
    let retry_steps = (image_paths.is_empty() && !capture_local_output).then(|| steps.clone());
    let heal_steps = retry_steps.clone();
    let send_started_ms = chrono::Utc::now().timestamp_millis();
    let mut sent = crate::session_chat_send::execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-message",
        steps,
    )
    .await;
    if let (Err(error), Some(retry_steps)) = (&sent, retry_steps) {
        if paste_not_accepted(error) {
            // The screen the paste check gave up on goes into the log line below, so a late paste
            // can be told apart from a half-drawn input box (CDXC:SessionChat 2026-10-29 in
            // held_redraw.rs).
            let gave_up_on =
                crate::session_chat_send::capture_session_terminal_text(&target.zmx_name)
                    .await
                    .map(|screen| {
                        crate::session_chat_composer::composer_terminal_preview_tail(&screen)
                    })
                    .unwrap_or_default();
            if let Some(mut late_steps) = late_paste_steps(&retry_steps) {
                late_steps.insert(
                    0,
                    crate::session_chat_send::SessionChatSendStep::WaitOutBusyAgent(
                        mid_turn_probe.clone(),
                    ),
                );
                sent = crate::session_chat_send::execute_session_chat_send(
                    &target.project_id,
                    &target.session_id,
                    &target.zmx_name,
                    "session-chat-message",
                    late_steps,
                )
                .await;
                if sent.is_ok() {
                    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                        "sessionChatSendLatePasteSubmitted",
                        &target.project_id,
                        &target.session_id,
                        "The pasted message showed up after the paste check gave up; it was submitted as it was.",
                        &gave_up_on,
                    );
                }
            }
            if sent.as_ref().err().is_some_and(paste_not_accepted)
                && paste_already_recorded(&target.session, text, send_started_ms).await
            {
                crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                    "sessionChatSendPasteAlreadyRecorded",
                    &target.project_id,
                    &target.session_id,
                    "The input box never showed the pasted message, but the agent's transcript already records it; it was not typed again.",
                    &[],
                );
                sent = Ok(());
            }
            if sent.as_ref().err().is_some_and(paste_not_accepted) {
                crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
                    "sessionChatSendRetriedPaste",
                    &target.project_id,
                    &target.session_id,
                    "The terminal still did not show the pasted message; cleared the input box and typed it once more.",
                    &[],
                );
                sent = crate::session_chat_send::execute_session_chat_send(
                    &target.project_id,
                    &target.session_id,
                    &target.zmx_name,
                    "session-chat-message",
                    with_single_paste_guard(
                        with_paste_watch_of_at_least(retry_steps, PASTE_RETRY_WATCH_MS),
                        terminal_agent.as_deref(),
                    ),
                )
                .await;
            }
        }
    }
    let mut agent_restarting = false;
    let mut tools_missing = false;
    let mut heal_gave_up: Option<String> = None;
    if let (Err(error), Some(heal_steps)) = (&sent, heal_steps) {
        if heals_refused_send(error, source, terminal_agent.as_deref()) {
            match super::send_heal::heal_refused_send(
                state,
                &target,
                terminal_agent.as_deref(),
                text,
                heal_steps,
                send_started_ms,
                error.message == crate::session_chat_send::SESSION_CHAT_BUSY_AGENT_STALLED,
                crate::session_chat_send_submit::is_not_submitted_failure(&error.message),
            )
            .await
            {
                super::send_heal::SendHeal::Delivered => sent = Ok(()),
                super::send_heal::SendHeal::Restarting => agent_restarting = true,
                super::send_heal::SendHeal::ToolsMissing => tools_missing = true,
                super::send_heal::SendHeal::GaveUp(reason) => heal_gave_up = reason,
            }
        }
    }
    if agent_restarting {
        if let Some(id) = durable_id.as_deref() {
            crate::session_chat_app_command::discard_local_command(
                &target.project_id,
                &target.session_id,
                id,
            );
        }
        return hold_for_restarted_agent(state, &target, source, drafted_text, image_paths);
    }
    if tools_missing {
        if let Some(id) = durable_id.as_deref() {
            crate::session_chat_app_command::discard_local_command(
                &target.project_id,
                &target.session_id,
                id,
            );
        }
        return super::send_heal::hold_while_tools_missing(
            state,
            &target,
            source,
            drafted_text,
            image_paths,
        );
    }
    if let Err(error) = sent {
        if let Some(id) = durable_id.as_deref() {
            crate::session_chat_app_command::discard_local_command(
                &target.project_id,
                &target.session_id,
                id,
            );
        }
        /*
        CDXC:AgentScreenDetection 2026-08-19:
        The case this feature exists for — the agent CLI in this pane is dead —
        fails HERE, not at the delivery watchdog: zmx refuses the clear or paste,
        or the terminal screen proves that the paste never landed, so the user
        would otherwise see only a generic toast. When the TERMINAL is what
        refused the message (as opposed to the send being superseded or
        cancelled), escalate once with the same one-capture verdict the watchdog
        takes at its deadline. It runs as its own task so the error response is
        not made to wait for the capture, it never retries the send, and the
        response below is unchanged.
        */
        if error.terminal_refused() {
            if let Some(send_probe) = send_probe {
                crate::session_chat_watchdog::escalate_failed_session_chat_send(
                    send_probe,
                    send_write_failure(&error),
                    session_chat_terminal_notice_publisher(
                        state,
                        &target.project_id,
                        &target.session_id,
                    ),
                    session_chat_watchdog_state_reader(
                        state,
                        &target.project_id,
                        &target.session_id,
                    ),
                );
            }
        }
        // CDXC:SessionChat 2026-08-26: the in-worker wait raises
        // the same code the pre-send gate does, so a client has one case to
        // handle whichever of the two caught it.
        if error.composer_not_ready() {
            return Err(DomainStateError {
                code: "composerNotReady",
                message: error.message,
            });
        }
        if error.prompt_waiting() {
            return Err(DomainStateError {
                code: crate::session_chat_send::SESSION_CHAT_PROMPT_WAITING_CODE,
                message: error.message,
            });
        }
        // The user's own Escape stopped this send before Enter: not a failure
        // the composer should announce, and the text is still theirs.
        if error.cancelled() {
            return Err(DomainStateError {
                code: "sendCancelled",
                message: error.message,
            });
        }
        crate::telemetry::reliability_error_for_session("prompt_delivery_failed", &target.session);
        let reason = crate::session_chat_send_failure::send_failure_reason(
            &error.message,
            heal_gave_up.as_deref(),
        );
        if error.failure == crate::session_chat_send::SessionChatSendFailure::ComposerNotCleared {
            return Err(DomainStateError {
                code: "composerNotCleared",
                message: reason,
            });
        }
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: reason,
        });
    }
    crate::session_chat_send::clear_session_chat_interrupt(
        state,
        &target,
        "session-chat-send-clear-interrupt",
    );
    if let Some(command) = archive_command {
        let project_id = target.project_id.clone();
        let session_id = target.session_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            crate::session_chat_app_command::commit_local_command(
                &project_id,
                &session_id,
                command,
            );
        })
        .await;
    }
    if !ghostex_note {
        crate::session_chat_returned_prompt::record_session_chat_send_submitted(
            &target.project_id,
            &target.session_id,
        );
    }
    crate::session_chat_watchdog::note_ghostex_typed(&target.project_id, &target.session_id, text);
    // The user sent something new and the agent took it, so an older failure's card and dot are
    // dealt with; its row keeps its own reason, Retry and Remove. A row the queue sent on its own
    // says nothing about what the user has seen.
    if source == SessionChatMessageSource::Composer {
        crate::session_chat_send_failure::clear_send_failure(
            state,
            &target.project_id,
            &target.session_id,
        );
    }
    /*
    CDXC:AgentScreenDetection 2026-08-19:
    The bytes reached zmx, which says nothing about the agent having received
    them: a message typed into a login screen, a trust dialog or a shell where
    the CLI already exited is accepted and lost. The watchdog verifies delivery
    against the transcript and surfaces the terminal's own explanation when it
    cannot. It never retries and never writes to the terminal.
    */
    if let Some(send_probe) = send_probe {
        let returned_prompt_state = state.clone();
        let returned_prompt_target = crate::session_chat_send::SessionChatSendTarget {
            project_id: target.project_id.clone(),
            session_id: target.session_id.clone(),
            zmx_name: target.zmx_name.clone(),
            session: target.session.clone(),
        };
        crate::session_chat_watchdog::start_session_chat_send_watchdog(
            send_probe,
            session_chat_terminal_notice_publisher(state, &target.project_id, &target.session_id),
            session_chat_watchdog_state_reader(state, &target.project_id, &target.session_id),
            std::sync::Arc::new(move || {
                crate::session_chat_returned_prompt::schedule_session_chat_returned_prompt_detection(
                    &returned_prompt_state,
                    &returned_prompt_target,
                    "session-chat-watchdog",
                );
            }),
            exited_agent_healer(state, &target, drafted_text),
        );
    }
    /*
    CDXC:Telemetry 2026-08-26:
    Counted after the bytes reached zmx and only on the success path, so a
    refused send is not reported as a prompt. The COUNT and the resolved agent
    are all that leave; the prompt text is not in scope for the emitter and
    cannot be.
    */
    let prompt_source = prompt_source_for_message_source(source);
    crate::telemetry::prompt_sent(&target.session, prompt_source);
    crate::telemetry::boosts_used(
        &target.session,
        prompt_source,
        boosts_built_in,
        boosts_custom,
    );
    /*
    An option command changes what the statusline reports: read it back. It is
    also NOT a user prompt — Ghostex itself typed it on the user's behalf when
    they picked a model or an effort level out of the composer's dropdown.
    */
    let is_option_readback_command =
        crate::session_chat_options::is_session_chat_option_command_text(
            terminal_agent.as_deref(),
            text,
        );
    let is_activity_command = crate::session_chat_options::is_session_chat_activity_command_text(
        terminal_agent.as_deref(),
        text,
    );
    let is_option_command = is_option_readback_command || is_activity_command;
    /*
    CDXC:Drafts 2026-08-28:
    THE chat half of the draft promotion choke point. Both callers reach the
    agent through this one function — the user's composer and the prompt queue's
    scheduler — so clearing the marker here covers every Ghostex-delivered first
    prompt, and it happens only after the bytes were accepted: a send refused by
    the composer gate or by zmx returns above and leaves the row a draft.
    (The terminal-direct half lives in `agents/drafts.rs`.)

    Option commands are carved out of BOTH halves. `/model` is delivered through
    this same function, and the dropdown that sends it is the one that will host
    the draft's own Agents section — so promoting on it would let a user destroy
    their draft's agent switcher by picking a model in it. The carve-out has to
    cover the terminal churn those bytes cause as well, which is why the
    non-promoting branch re-arms the draft's launch suppression window instead
    of doing nothing: the command's spinner is then folded back to idle exactly
    like a startup spinner, and cannot promote the draft through the
    activity-based half a second later.
    */
    if is_option_command {
        suppress_draft_activity_after_app_command(state, &target.project_id, &target.session_id);
    } else {
        promote_draft_session_after_send(state, &target.project_id, &target.session_id);
        unpark_session_after_send(state, &target.project_id, &target.session_id);
    }
    if let Some(version) = draft_version {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: error.to_string(),
        })?;
        crate::session_chat_draft_versions::consume(
            &db,
            &target.project_id,
            &target.session_id,
            version,
        )?;
        crate::session_chat_draft_diagnostics::log(
            &state.logger,
            "consumed",
            &target.project_id,
            &target.session_id,
            json!({ "version": version }),
        );
        broadcast_session_chat_queue_state(state, &target.project_id, &target.session_id);
    } else if source == SessionChatMessageSource::Composer {
        retire_sent_session_chat_draft(
            state,
            &target.project_id,
            &target.session_id,
            drafted_text,
            draft_before_send.as_ref(),
        );
    }
    // Probe activity commands after sending too: Codex and Cursor need not
    // record a command in their transcript before the compaction starts.
    if is_option_readback_command || capture_local_output || is_activity_command {
        schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            terminal_agent.as_deref(),
        );
    }
    Ok(text.len())
}

fn prompt_source_for_message_source(source: SessionChatMessageSource) -> &'static str {
    match source {
        SessionChatMessageSource::Composer => "chat",
        SessionChatMessageSource::AutomaticQueue
        | SessionChatMessageSource::AutomaticRecovery
        | SessionChatMessageSource::AccountSwitch(_)
        | SessionChatMessageSource::ManualQueue => "queue",
        SessionChatMessageSource::GhostexNote => "ghostexNote",
    }
}
