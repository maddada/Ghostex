//! What Ask Ghostex AI's welcome card is drawn from (`coordinatorThreads.chiefWelcome` on its own
//! chat; packages/gx-chat-core/src/extras/chief_welcome.rs decides the card): whether the user has
//! a project, an agent signed in, an agent started and a phone paired, what is running and what
//! needs them, and which of the card's prompts they already sent to it.
//!
//! Chat frames carry it, so it is worked out at most every few seconds; the sign-in reads, which
//! run each agent's own status command, happen on a thread of their own every few minutes.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::domain::DomainRepository;
use crate::paths::GxserverPaths;

/// How long one reading of the state is reused.
const FRESH_FOR: Duration = Duration::from_secs(5);
/// How long one reading of the agents' sign-in is reused.
const SIGN_IN_FRESH_FOR: Duration = Duration::from_secs(5 * 60);
/// The agents whose own login counts for "Sign in to Claude or Codex" (Antigravity too, since
/// onboarding offers it beside them).
const SIGN_IN_AGENTS: &[&str] = &["claude", "codex", "antigravity"];
/// The prompts of the card the user sent to Ask Ghostex AI, in gxserver's `metadata`.
const ASKED_KEY: &str = "chiefWelcomeAsked";

fn reading() -> &'static Mutex<Option<(Instant, Value)>> {
    static READING: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();
    READING.get_or_init(Default::default)
}

/// The last sign-in reading: when it was taken (or started) and whether any agent was signed in.
fn sign_in_reading() -> &'static Mutex<Option<(Instant, bool)>> {
    static READING: OnceLock<Mutex<Option<(Instant, bool)>>> = OnceLock::new();
    READING.get_or_init(Default::default)
}

/// Whether an agent is signed in, as last read; starts a new read on its own thread once the last
/// one is older than [`SIGN_IN_FRESH_FOR`]. `false` until the first read ends.
fn signed_in_cli(home: std::path::PathBuf) -> bool {
    let mut guard = sign_in_reading()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let known = guard.map(|(_, signed_in)| signed_in).unwrap_or(false);
    if guard.is_some_and(|(at, _)| at.elapsed() < SIGN_IN_FRESH_FOR) {
        return known;
    }
    // Marked as read now so only one thread reads at a time; it keeps the last answer meanwhile.
    *guard = Some((Instant::now(), known));
    drop(guard);
    std::thread::spawn(move || {
        let signed_in = SIGN_IN_AGENTS
            .iter()
            .any(|agent| crate::agent_sign_in::read_signed_in(&home, agent) == Some(true));
        *sign_in_reading()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((Instant::now(), signed_in));
    });
    known
}

fn has_saved_account(db: &Connection) -> bool {
    use crate::accounts::model::Provider;
    crate::accounts::store::read(db).is_ok_and(|registry| {
        registry
            .accounts
            .iter()
            .any(|account| matches!(account.provider, Provider::Claude | Provider::Codex))
    })
}

fn asked(db: &Connection) -> Vec<String> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [ASKED_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
    .unwrap_or_default()
}

/// The state, read fresh.
fn read(paths: &GxserverPaths) -> Option<Value> {
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    // Only read here: the server id is used for what a repository writes.
    let repository = DomainRepository::new(&db, "");
    let record = super::read_record(&db);
    let projects = repository.list_projects().ok()?;
    let own_projects = projects
        .iter()
        .filter(|project| !super::is_home_project(project) && !super::is_help_home_project(project))
        .count();
    let sessions = repository.list_sessions(None).ok()?;
    let now = crate::coordinators::now_iso();
    let theirs = |session: &&Value| {
        let key = (
            session["projectId"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            session["sessionId"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        !record.as_ref().is_some_and(|record| record.owns(&key))
    };
    let agents: Vec<&Value> = sessions
        .iter()
        .filter(theirs)
        .filter(|session| session["kind"] == "agent")
        .collect();
    let mut running_projects = HashSet::new();
    let mut running = 0;
    let mut needs_you = 0;
    for session in &agents {
        match crate::presentation::session_attributes::presentation_activity(session, &now).as_str()
        {
            "working"
                if !crate::presentation::session_attributes::working_only_in_background(
                    session, &now,
                ) =>
            {
                running += 1;
                running_projects.insert(session["projectId"].as_str().unwrap_or_default());
            }
            "attention" => needs_you += 1,
            _ => {}
        }
    }
    let prompted = agents
        .iter()
        .any(|session| crate::agents::agent_recorded_a_prompt(session));
    let signed_in = prompted || has_saved_account(&db) || signed_in_cli(paths.home_dir.clone());
    let phone = crate::remote_access::paired_devices::list_paired_devices_json(&db)
        .ok()
        .and_then(|devices| devices.as_array().map(|devices| !devices.is_empty()))
        .unwrap_or(false);
    Some(json!({
        "projects": own_projects,
        "signedIn": signed_in,
        // A draft (New session, nothing sent yet) has not been started.
        "agentStarted": agents
            .iter()
            .any(|session| !crate::agents::session_is_draft(session)),
        "phone": phone,
        "running": running,
        "runningProjects": running_projects.len(),
        "needsYou": needs_you,
        "asked": asked(&db),
    }))
}

/// The state for Ask Ghostex AI's welcome card, at most [`FRESH_FOR`] old.
pub(crate) fn welcome_state(paths: &GxserverPaths) -> Option<Value> {
    let mut guard = reading()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((at, value)) = guard.as_ref() {
        if at.elapsed() < FRESH_FOR {
            return Some(value.clone());
        }
    }
    let value = read(paths)?;
    *guard = Some((Instant::now(), value.clone()));
    Some(value)
}

/// A message the user sent to session `project_id`/`session_id`: when that is one of Ask Ghostex
/// AI's chats and the text is one of its welcome card's prompts, the card stops offering it.
pub(crate) fn remember_asked(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) {
    let text = text.trim();
    if !ghostex_gx_chat_core::extras::chief_welcome::is_welcome_prompt(text) {
        return;
    }
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return;
    };
    let key = (project_id.to_string(), session_id.to_string());
    if !super::read_record(&db).is_some_and(|record| record.owns(&key)) {
        return;
    }
    let mut asked = asked(&db);
    if asked.iter().any(|known| known == text) {
        return;
    }
    asked.push(text.to_string());
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![
            ASKED_KEY,
            serde_json::to_string(&asked).unwrap_or_default(),
            crate::coordinators::now_iso()
        ],
    );
    *reading()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}
