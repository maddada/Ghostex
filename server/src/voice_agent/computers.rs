//! Sessions on the other computers this one is connected to (Settings > Remote > Remote machines,
//! and the computers that connected to this one): their open sessions and projects for the voice
//! agent, and every tool request about one of them, sent there through this gxserver's relay.
//!
//! CDXC:VoiceMode 2026-11-03 WHY:
//! Voice mode read sessions from this computer's database only, so a call knew nothing of the sessions the Unified sidebar shows from other computers ("it only seems aware of local sessions"). Their rows come from that computer's own `/api/listSessions` and `/api/listProjects`, and every request goes through `/api/computers/<serverId>/relay`, the path `ghostex --computer` takes, so each computer's Agent access switch and `PEER_ALLOWED_ENDPOINTS` decide exactly as they do for agents. A tool that needs an endpoint outside that list (chat drafts, question forms, coordinator threads) says so in a sentence instead of trying.
//! SEE-ALSO: server/src/connected_computers/ (registry, relay, scope), apps/desktop/src/app/voice_agent/focus.rs (shows a session of another computer a tool named), apps/desktop/src/app/gx_store/renderer_commands/perform.rs (`readFocusedSession` names another computer's session).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
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
    /// are threads: a session known as a thread from that computer's last answer stays one while
    /// it is open (`known`); any other is listed as a session of its own, a thread or not.
    pub(super) threads_unknown: Vec<String>,
}

/// One computer's answers.
#[derive(Clone)]
struct Listing {
    computer: Computer,
    /// Its open sessions, waiting ones first, threads not marked.
    sessions: Vec<SessionInfo>,
    projects: Vec<ProjectInfo>,
    /// When its sessions and projects were asked.
    read_at: Instant,
    /// Which sessions are threads, from its presentation snapshot (`hierarchy`), of the sessions
    /// above: `None` until asked, `Some(None)` when it did not come in time.
    threads: Option<Option<Roles>>,
    /// When the answer in `threads` was asked.
    threads_asked: Option<Instant>,
    /// Without an answer to that, what its last answer said of the sessions still open (`known`).
    last_known: Option<Roles>,
}

impl Listing {
    /// Its sessions, each coordinator's threads marked and right after it: as its snapshot says,
    /// or as its last one said when this one did not come.
    fn sessions(&self) -> Vec<SessionInfo> {
        match (&self.threads, &self.last_known) {
            (Some(Some(roles)), _) | (_, Some(roles)) => roles.nest(self.sessions.clone()),
            _ => self.sessions.clone(),
        }
    }

    fn threads_known(&self) -> bool {
        matches!(self.threads, Some(Some(_)))
    }
}

/// Which of a computer's sessions are coordinators and which are their threads, by global ref.
#[derive(Clone, Debug, Default)]
struct Roles {
    /// Each thread's coordinator, and whether the coordinator marked it done.
    parents: HashMap<String, (String, bool)>,
    coordinators: HashSet<String>,
}

impl Roles {
    /// The coordinator fields of `computer`'s presentation snapshot (`hierarchy`).
    fn of(computer: &Computer, snapshot: &Value) -> Self {
        let global = |project_id: &str, session_id: &str| {
            crate::ids::create_global_session_ref(&computer.server_id, project_id, session_id)
        };
        let mut roles = Self::default();
        for session in snapshot
            .pointer("/snapshot/sessions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let field = |key: &str| session.get(key).and_then(Value::as_str);
            let (Some(project_id), Some(session_id)) = (field("projectId"), field("sessionId"))
            else {
                continue;
            };
            let reference = global(project_id, session_id);
            if let (Some(project_id), Some(session_id)) =
                (field("coordinatorProjectId"), field("coordinatorSessionId"))
            {
                let done = field("coordinatorThreadState") == Some("done");
                roles
                    .parents
                    .insert(reference.clone(), (global(project_id, session_id), done));
            }
            if field("coordinatorRole") == Some("coordinator") {
                roles.coordinators.insert(reference);
            }
        }
        roles
    }

    /// `sessions` with each coordinator's threads marked and right after it (`sessions::nest`: a
    /// thread whose coordinator is not open is a session of its own).
    fn nest(&self, sessions: Vec<SessionInfo>) -> Vec<SessionInfo> {
        sessions::nest(
            sessions,
            |reference| self.parents.get(reference).cloned(),
            |reference| self.coordinators.contains(reference),
        )
    }

    /// Only what it says of `open` sessions.
    fn retain_open(&mut self, open: &[SessionInfo]) {
        let open: HashSet<String> = open.iter().map(SessionInfo::reference).collect();
        self.parents.retain(|reference, _| open.contains(reference));
        self.coordinators.retain(|reference| open.contains(reference));
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

/// Which sessions are threads as each computer last said it, by the same key as `cache`, kept for
/// its sessions that are still open.
///
/// CDXC:VoiceMode 2026-11-27 WHY: when a connected computer did not say which sessions are threads
/// (a slow or failed `readPresentationSnapshot`), its sessions came flat, so a thread it had named
/// before reached a call about all sessions as a session of its own: its next turn end was
/// announced, though threads are never announced there. A session it named as a thread stays one
/// while it and its coordinator are open, until an answer says otherwise; this outlives the
/// `FRESH_FOR` cache, whose entries are read again. A thread that opened since the last answer, or
/// on a computer that never answered, cannot be told from a session of its own and is still
/// listed and announced as one (Help says so).
///
/// CDXC:VoiceMode 2026-11-27 WHY: two readers can ask the same computer at once, and a slow answer
/// that still named a thread arrived after a newer one that confirmed its unlink, replaced it, and
/// the next outage made that open session a thread again. Answers are ordered by when they were
/// asked: an older one replaces neither this nor the cached answer, and a list read before the kept
/// answer was asked does not trim it. An older gxserver ignores `onlySessions` and names closed
/// pinned threads too (64 links kept for one open thread in the review's replay), so an answer is
/// trimmed to the sessions its list holds open before it is kept.
type Known = HashMap<(String, String), Remembered>;

/// A computer's last threads answer, and when it was asked.
struct Remembered {
    asked: Instant,
    roles: Roles,
}

fn known() -> std::sync::MutexGuard<'static, Known> {
    static KNOWN: LazyLock<Mutex<Known>> = LazyLock::new(Default::default);
    KNOWN
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
    if threads == Threads::Wanted {
        // A computer no longer connected has no sessions to keep roles for.
        known().retain(|(here, there), _| {
            *here != state.metadata.server_id
                || computers.iter().any(|computer| computer.server_id == *there)
        });
    }
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
    if threads == Threads::Wanted {
        if listing.threads.is_none() {
            let asked = Instant::now();
            let snapshot = hierarchy(state, &listing.computer, &listing.sessions).await;
            listing.threads = Some(snapshot.map(|snapshot| {
                let mut roles = Roles::of(&listing.computer, &snapshot);
                roles.retain_open(&listing.sessions);
                roles
            }));
            listing.threads_asked = Some(asked);
            // Kept with the answers it is about, unless they were read again meanwhile or a
            // newer threads answer is kept with them.
            if let Some((cached_at, Ok(cached))) = cache().get_mut(&key) {
                if *cached_at == at
                    && cached
                        .threads_asked
                        .map_or(true, |kept| kept < asked)
                {
                    cached.threads = listing.threads.clone();
                    cached.threads_asked = Some(asked);
                }
            }
        }
        match (&listing.threads, listing.threads_asked) {
            (Some(Some(roles)), Some(asked)) => {
                let mut known = known();
                if known.get(&key).map_or(true, |kept| kept.asked < asked) {
                    known.insert(
                        key,
                        Remembered {
                            asked,
                            roles: roles.clone(),
                        },
                    );
                }
            }
            _ => {
                if let Some(kept) = known().get_mut(&key) {
                    if listing.read_at > kept.asked {
                        kept.roles.retain_open(&listing.sessions);
                    }
                    listing.last_known = Some(kept.roles.clone());
                }
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
    let read_at = Instant::now();
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
        (Ok(rows), Ok(projects)) => Ok(Listing {
            read_at,
            ..read_listing(&owner, &rows, &projects)
        }),
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
/// stay top-level, except those its last answer named (`known`), and the readers that want them
/// say so (`Elsewhere::threads_unknown`).
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
/// marked with `computer`, waiting ones first; which are threads is not asked yet. `read_at` is
/// now; `read` sets when they were asked.
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
        read_at: Instant::now(),
        threads: None,
        threads_asked: None,
        last_known: None,
    }
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
    known().clear();
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
