//! Sessions on the other computers this one is connected to (Settings > Remote > Remote machines,
//! and the computers that connected to this one): their open sessions and projects for the voice
//! agent, and every tool request about one of them, sent there through this gxserver's relay.
//!
//! CDXC:VoiceMode 2026-11-03 WHY:
//! Voice mode read sessions from this computer's database only, so a call knew nothing of the sessions the Unified sidebar shows from other computers ("it only seems aware of local sessions"). Their rows come from that computer's own `/api/listSessions` and `/api/listProjects`, and every request goes through `/api/computers/<serverId>/relay`, the path `ghostex --computer` takes, so each computer's Agent access switch and `PEER_ALLOWED_ENDPOINTS` decide exactly as they do for agents. A tool that needs an endpoint outside that list (chat drafts, question forms, coordinator threads) says so in a sentence instead of trying.
//! SEE-ALSO: server/src/connected_computers/ (registry, relay, scope), apps/desktop/src/app/voice_agent/focus.rs (shows a session of another computer a tool named), apps/desktop/src/app/gx_store/renderer_commands/perform.rs (`readFocusedSession` names another computer's session).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::loopback;
use super::sessions::{self, ProjectInfo, SessionInfo};
use crate::connected_computers::ConnectedComputer;
use crate::server::AppState;

/// A list request waits at most this long for another computer.
const LIST_TIMEOUT: Duration = Duration::from_secs(6);
/// Which of another computer's sessions are threads (`hierarchy`) is waited for at most this long
/// once its sessions and projects came; without an answer by then its sessions come as they are,
/// with their threads unknown (`Elsewhere::threads_unknown`).
pub(super) const THREADS_TIMEOUT: Duration = Duration::from_secs(2);
/// A computer's lists are reused this long, so a tool call right after the two-second
/// announcement poll does not ask again.
const FRESH_FOR: Duration = Duration::from_millis(1_500);
/// A computer that did not answer is not asked again for this long, so a dead tunnel does not
/// hold up every tool call.
const FAILED_FOR: Duration = Duration::from_secs(15);

/// The connected computer a session or project is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Computer {
    pub(super) server_id: String,
    pub(super) name: String,
}

/// A connected computer whose sessions could not be read, and why, in words the model can say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Unavailable {
    pub(super) computer: String,
    pub(super) reason: String,
}

impl Unavailable {
    pub(super) fn to_tool_value(&self) -> Value {
        json!({"computer": self.computer, "reason": self.reason})
    }
}

/// What a reader needs to know of another computer's sessions besides the rows.
///
/// CDXC:VoiceMode 2026-11-27 WHY: which sessions are threads takes one more request per computer
/// (`hierarchy`), and a computer slow to answer it held up every reader of the shared list for up
/// to `LIST_TIMEOUT` more, a project lookup or a focused call's start included, though their
/// sessions and projects had come. Only the readers that tell threads apart ask it, and only for
/// `THREADS_TIMEOUT`; the others take what came, threads too when they came already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Threads {
    /// Which are a coordinator's threads: the call's list, the announcements, `list_sessions`,
    /// and matching the user's words (a top-level session wins over a thread of the same name).
    Wanted,
    /// Nothing more: projects, and the lookups by ref of a focused call, a GBot and the active
    /// session.
    NotNeeded,
}

/// What the other computers hold.
#[derive(Default)]
pub(super) struct Elsewhere {
    pub(super) sessions: Vec<SessionInfo>,
    pub(super) projects: Vec<ProjectInfo>,
    pub(super) unavailable: Vec<Unavailable>,
    /// With `Threads::Wanted`, the computers, by name, whose sessions came without which of them
    /// are threads: each is listed as a session of its own, a thread or not.
    pub(super) threads_unknown: Vec<String>,
}

/// One computer's answers.
#[derive(Clone)]
struct Listing {
    computer: Computer,
    /// Its open sessions, waiting ones first, threads not marked.
    sessions: Vec<SessionInfo>,
    projects: Vec<ProjectInfo>,
    /// Its presentation snapshot (`hierarchy`): `None` until asked, `Some(None)` when it did not
    /// come in time.
    threads: Option<Option<Value>>,
}

impl Listing {
    /// Its sessions, each coordinator's threads marked and right after it when its snapshot came.
    fn sessions(&self) -> Vec<SessionInfo> {
        match &self.threads {
            Some(Some(snapshot)) => with_threads(&self.computer, self.sessions.clone(), snapshot),
            _ => self.sessions.clone(),
        }
    }

    fn threads_known(&self) -> bool {
        matches!(self.threads, Some(Some(_)))
    }
}

/// Each computer's last answer, by this gxserver's and that computer's server ids.
type Cache = HashMap<(String, String), (Instant, Result<Listing, String>)>;

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Drops what was read of `server_id`, after a tool changed its sessions (a new agent there).
pub(super) fn forget(state: &AppState, server_id: &str) {
    cache().remove(&(state.metadata.server_id.clone(), server_id.to_string()));
}

/// The open sessions and projects of every connected computer, read in parallel. One that
/// cannot be read is named in `unavailable` and leaves the rest alone.
pub(super) async fn other_computers(state: &Arc<AppState>, threads: Threads) -> Elsewhere {
    let computers = crate::connected_computers::list();
    let reads = computers
        .iter()
        .map(|computer| listing(state, computer, threads));
    let mut elsewhere = Elsewhere::default();
    for (computer, outcome) in computers
        .iter()
        .zip(futures_util::future::join_all(reads).await)
    {
        match outcome {
            Ok(listing) => {
                if threads == Threads::Wanted && !listing.threads_known() {
                    elsewhere.threads_unknown.push(computer.name.clone());
                }
                elsewhere.sessions.extend(listing.sessions());
                elsewhere.projects.extend(listing.projects);
            }
            Err(reason) => elsewhere.unavailable.push(Unavailable {
                computer: computer.name.clone(),
                reason,
            }),
        }
    }
    // Each computer's rows come waiting first with its threads under their orchestrator
    // (`Listing::sessions`); `sessions::merge` sorts them in with this computer's.
    elsewhere
}

async fn listing(
    state: &Arc<AppState>,
    computer: &ConnectedComputer,
    threads: Threads,
) -> Result<Listing, String> {
    if let Some(waiting) = computer.waiting_message() {
        return Err(waiting);
    }
    if !computer.agent_access {
        return Err(crate::connected_computers::access_off_message(computer));
    }
    let key = (state.metadata.server_id.clone(), computer.server_id.clone());
    let cached = cache().get(&key).and_then(|(at, outcome)| {
        let keep = if outcome.is_ok() {
            FRESH_FOR
        } else {
            FAILED_FOR
        };
        (at.elapsed() < keep).then(|| (*at, outcome.clone()))
    });
    let (at, outcome) = match cached {
        Some(cached) => cached,
        None => {
            let outcome = read(state, computer).await;
            let at = Instant::now();
            cache().insert(key.clone(), (at, outcome.clone()));
            (at, outcome)
        }
    };
    let mut listing = outcome?;
    if threads == Threads::Wanted && listing.threads.is_none() {
        listing.threads = Some(hierarchy(state, &listing.computer, &listing.sessions).await);
        // Kept with the answers it is about, unless they were read again meanwhile.
        if let Some((cached_at, Ok(cached))) = cache().get_mut(&key) {
            if *cached_at == at {
                cached.threads = listing.threads.clone();
            }
        }
    }
    Ok(listing)
}

/// Another computer's sessions and projects, without which sessions are threads.
async fn read(state: &Arc<AppState>, computer: &ConnectedComputer) -> Result<Listing, String> {
    let owner = Computer {
        server_id: computer.server_id.clone(),
        name: computer.name.clone(),
    };
    let (rows, projects) = tokio::join!(
        call(
            state,
            Some(&owner),
            "/api/listSessions",
            json!({"includeStopped": false}),
            LIST_TIMEOUT,
        ),
        call(
            state,
            Some(&owner),
            "/api/listProjects",
            json!({}),
            LIST_TIMEOUT
        ),
    );
    match (rows, projects) {
        (Ok(rows), Ok(projects)) => Ok(read_listing(&owner, &rows, &projects)),
        // This computer's relay did not answer in time: the tunnel to it hangs.
        (Err(error), _) | (_, Err(error)) if error.starts_with(loopback::NO_ANSWER) => {
            Err(format!("{} didn't answer in time.", computer.name))
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

/// Which of another computer's open sessions are coordinators and which are their threads: the
/// coordinator fields of its presentation snapshot for just those sessions (`onlySessions`),
/// waited for at most `THREADS_TIMEOUT`. `None` when it did not answer in time; its threads then
/// stay top-level, and the readers that want them say so (`Elsewhere::threads_unknown`).
///
/// CDXC:VoiceMode 2026-11-26 WHY: the voice agent left a coordinator's threads out of the call's
/// session list and its announcements only on this computer, because which sessions are threads
/// is in each computer's own database, which `/api/listSessions` does not answer. That computer's
/// presentation snapshot carries it in the fields every client nests threads by
/// (coordinators/presentation.rs), on an endpoint other computers may already read
/// (`PEER_ALLOWED_ENDPOINTS`); `onlySessions` keeps the answer to the listed sessions, as the
/// remote coordinator runtime asks it (server/coordinator_remote_runtime.rs).
async fn hierarchy(
    state: &Arc<AppState>,
    computer: &Computer,
    sessions: &[SessionInfo],
) -> Option<Value> {
    if sessions.is_empty() {
        // No session, so no thread: nothing to ask.
        return Some(Value::Null);
    }
    let keys: Vec<Value> = sessions
        .iter()
        .map(|session| json!({"projectId": session.project_id, "sessionId": session.session_id}))
        .collect();
    call(
        state,
        Some(computer),
        "/api/readPresentationSnapshot",
        json!({ "onlySessions": keys }),
        THREADS_TIMEOUT,
    )
    .await
    .ok()
}

/// Another computer's `listSessions` and `listProjects` answers as the voice agent's rows, each
/// marked with `computer`, waiting ones first; which are threads is not asked yet.
fn read_listing(computer: &Computer, rows: &Value, projects: &Value) -> Listing {
    let projects: Vec<ProjectInfo> = projects
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(sessions::project_info)
        .map(|mut project| {
            project.computer = Some(computer.clone());
            project
        })
        .collect();
    let now = crate::presentation::now_iso();
    let rows = rows
        .get("sessions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| sessions::session_info(row, &projects, &now))
        .map(|mut session| {
            session.computer = Some(computer.clone());
            session
        })
        .collect();
    Listing {
        computer: computer.clone(),
        sessions: sessions::sorted(rows),
        projects,
        threads: None,
    }
}

/// `sessions` of `computer` with each coordinator's threads marked and right after it, as
/// `snapshot` (its `readPresentationSnapshot` answer, `hierarchy`) names them.
fn with_threads(
    computer: &Computer,
    sessions: Vec<SessionInfo>,
    snapshot: &Value,
) -> Vec<SessionInfo> {
    // Each session's coordinator fields, by its global ref (the one `SessionInfo::reference`
    // gives a session on another computer).
    let global = |project_id: &str, session_id: &str| {
        crate::ids::create_global_session_ref(&computer.server_id, project_id, session_id)
    };
    let roles: HashMap<String, &Value> = snapshot
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|session| {
            Some((
                global(
                    session.get("projectId")?.as_str()?,
                    session.get("sessionId")?.as_str()?,
                ),
                session,
            ))
        })
        .collect();
    let field = |reference: &str, key: &str| {
        roles
            .get(reference)
            .and_then(|session| session.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    sessions::nest(
        sessions,
        |reference| {
            let project_id = field(reference, "coordinatorProjectId")?;
            let session_id = field(reference, "coordinatorSessionId")?;
            let done = field(reference, "coordinatorThreadState").as_deref() == Some("done");
            Some((global(&project_id, &session_id), done))
        },
        |reference| field(reference, "coordinatorRole").as_deref() == Some("coordinator"),
    )
}

/// `error` (no session matched the user's words) with the connected computers that could not be
/// read, so "the Linux one" on a computer whose Agent access is off is answered with why.
pub(super) fn with_unreachable(error: String, unavailable: &[Unavailable]) -> String {
    if unavailable.is_empty() || !error.starts_with(super::tools::NO_MATCH) {
        return error;
    }
    let reasons = unavailable
        .iter()
        .map(|computer| computer.reason.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{error} Sessions on another computer can't be reached right now: {reasons}")
}

/// The route a request about something on `computer` takes: this gxserver's own, or the relay to
/// that computer. An endpoint another computer does not open to agents is refused here, before
/// anything is sent; the relay and that computer's gxserver refuse it again on their own.
pub(super) fn route(computer: Option<&Computer>, path: &str) -> Result<String, String> {
    let Some(computer) = computer else {
        return Ok(path.to_string());
    };
    if !crate::connected_computers::is_peer_allowed(path) {
        return Err(not_on_another_computer(&computer.name));
    }
    Ok(format!(
        "{}{path}",
        crate::connected_computers::relay_base_url("", &computer.server_id)
    ))
}

/// What the model hears when a tool can't do its job on another computer.
pub(super) fn not_on_another_computer(name: &str) -> String {
    format!("That can't be done on {name} from here: on another computer you can read sessions, send prompts, answer approvals and question cards, stop a turn and start agents, but not more. The user can do it in Ghostex on {name}.")
}

/// POSTs `params` to `path` on `computer` (this computer when `None`) and answers the result, or
/// the error the route or that computer gave.
pub(super) async fn call(
    state: &Arc<AppState>,
    computer: Option<&Computer>,
    path: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let routed = route(computer, path)?;
    loopback::call(state, &routed, params, timeout).await
}

#[cfg(test)]
pub(super) fn reset_for_tests() {
    cache().clear();
}

#[cfg(test)]
pub(super) fn read_listing_for_tests(
    computer: &Computer,
    rows: &Value,
    projects: &Value,
) -> (Vec<SessionInfo>, Vec<ProjectInfo>) {
    let listing = read_listing(computer, rows, projects);
    (listing.sessions, listing.projects)
}
