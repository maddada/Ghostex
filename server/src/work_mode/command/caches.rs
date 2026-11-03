//! What the background reads of ticket commands remember for the cards: each ticket's last read,
//! the back-off after a failure that would only repeat, and whether each workspace's command
//! could run. `jobs.rs` writes them while it commits; the projection reads them without I/O.
//!
//! Every key holds the workspace's generation (`work_source_generation`): a change to its path,
//! pin, credentials or team bumps it, so nothing read or refused before the change applies after
//! it, and the next pass reads again at once.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use super::*;

/// How long a found ticket counts as fresh; a failure counts as long, as Linear's do.
const FOUND_TTL: Duration = Duration::from_secs(120);
/// For a session still waiting for its PR, which the ticket can bring along.
const AWAITING_PULL_REQUEST_TTL: Duration = Duration::from_secs(45);
const NOT_FOUND_TTL: Duration = Duration::from_secs(600);
/// How long a call that `backs_off` keeps its key from running.
pub(crate) const BACK_OFF: Duration = Duration::from_secs(300);
/// The tickets kept across every workspace; past it, the least recently read goes.
const MAX_CACHED_TICKETS: usize = 2_000;

/// What a job was planned against, rechecked before its command starts and before its result is
/// kept (`jobs.rs`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct JobPlan {
    pub(crate) workspace_id: String,
    pub(crate) source: CommandSourceId,
    pub(crate) generation: u64,
}

/// What the last read of one ticket answered.
#[derive(Clone, Debug)]
pub(crate) enum CachedTicket {
    Found(Ticket),
    NotFound,
    /// The read failed and no read of this generation ever found it.
    Failed,
}

struct TicketEntry {
    /// The last read; after a failure, the last time it was found, if ever (spec §5.3).
    read: CachedTicket,
    failed: bool,
    read_at: Instant,
    awaiting_pull_request: bool,
    /// Its read is due on the next pass whatever its age (its title couldn't be saved).
    expired: bool,
}

impl TicketEntry {
    fn fresh(&self, now: Instant) -> bool {
        if self.expired {
            return false;
        }
        let ttl = match (&self.read, self.failed) {
            (CachedTicket::NotFound, false) => NOT_FOUND_TTL,
            _ if self.awaiting_pull_request => AWAITING_PULL_REQUEST_TTL,
            _ => FOUND_TTL,
        };
        now.duration_since(self.read_at) < ttl
    }
}

#[derive(Default)]
struct Caches {
    tickets: HashMap<(JobPlan, String), TicketEntry>,
    back_off: HashMap<(JobPlan, CommandVerb), Instant>,
    /// Each workspace's generation, and whether its command could run then (`can_read`).
    readiness: HashMap<String, (u64, bool)>,
}

fn caches() -> MutexGuard<'static, Caches> {
    static CACHES: OnceLock<Mutex<Caches>> = OnceLock::new();
    CACHES
        .get_or_init(|| Mutex::new(Caches::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether a failed call would only fail again if asked soon: the command can't start, didn't
/// answer in time, or said its credentials were refused or it is rate limited. Its key then
/// rests for `BACK_OFF`; any other failure is kept for the ticket's TTL.
pub(crate) fn backs_off(failure: &CommandFailure) -> bool {
    matches!(
        failure,
        CommandFailure::NotFound(_)
            | CommandFailure::NotExecutable(_)
            | CommandFailure::BatchFile(_)
            | CommandFailure::Start(_)
            | CommandFailure::TimedOut { .. }
            | CommandFailure::Reported {
                code: ErrorCode::Auth | ErrorCode::RateLimited,
                ..
            }
    )
}

pub(crate) fn back_off(plan: &JobPlan, verb: CommandVerb) {
    caches()
        .back_off
        .insert((plan.clone(), verb), Instant::now() + BACK_OFF);
}

pub(crate) fn backing_off(plan: &JobPlan, verb: CommandVerb) -> bool {
    let mut caches = caches();
    let now = Instant::now();
    caches.back_off.retain(|_, until| *until > now);
    caches.back_off.contains_key(&(plan.clone(), verb))
}

/// The card pass's answer for a workspace at a generation: whether its command can run there.
pub(crate) fn remember_command_readiness(workspace_id: &str, generation: u64, can_run: bool) {
    caches()
        .readiness
        .insert(workspace_id.to_string(), (generation, can_run));
}

/// Whether the workspace's command could run at `generation`; `None` before a card pass checked
/// that generation.
pub(crate) fn command_readiness(workspace_id: &str, generation: u64) -> Option<bool> {
    caches()
        .readiness
        .get(workspace_id)
        .filter(|(checked, _)| *checked == generation)
        .map(|(_, can_run)| *can_run)
}

/// The ticket's last read at the plan's generation; `None` when none was kept.
pub(crate) fn cached_command_ticket(plan: &JobPlan, id: &str) -> Option<CachedTicket> {
    caches()
        .tickets
        .get(&(plan.clone(), id.to_string()))
        .map(|entry| entry.read.clone())
}

/// Whether the ticket needs a read: never read at this generation, or no longer fresh.
pub(crate) fn command_ticket_due(plan: &JobPlan, id: &str, now: Instant) -> bool {
    caches()
        .tickets
        .get(&(plan.clone(), id.to_string()))
        .is_none_or(|entry| !entry.fresh(now))
}

/// Keeps what an exact read answered for each of `ids`: found, not found, or failed (`answer` is
/// `None` when the call failed; an id its tickets and `notFound` leave out failed too). A failure
/// keeps the ticket as last found. Entries of the workspace's older generations go.
pub(crate) fn remember_exact_read(
    plan: &JobPlan,
    ids: &[String],
    awaiting_pull_request: &[String],
    answer: Option<(&[Ticket], &[String])>,
) {
    let now = Instant::now();
    let mut caches = caches();
    caches.tickets.retain(|(kept, _), _| {
        kept.workspace_id != plan.workspace_id
            || kept.source != plan.source
            || kept.generation == plan.generation
    });
    for id in ids {
        let found =
            answer.and_then(|(tickets, _)| tickets.iter().find(|ticket| &ticket.ticket.id == id));
        let not_found = answer.is_some_and(|(_, not_found)| not_found.contains(id));
        let key = (plan.clone(), id.clone());
        let entry = match (found, not_found) {
            (Some(ticket), _) => TicketEntry {
                read: CachedTicket::Found(ticket.clone()),
                failed: false,
                read_at: now,
                awaiting_pull_request: false,
                expired: false,
            },
            (None, true) => TicketEntry {
                read: CachedTicket::NotFound,
                failed: false,
                read_at: now,
                awaiting_pull_request: false,
                expired: false,
            },
            (None, false) => TicketEntry {
                read: match caches.tickets.get(&key).map(|entry| &entry.read) {
                    Some(found @ CachedTicket::Found(_)) => found.clone(),
                    _ => CachedTicket::Failed,
                },
                failed: true,
                read_at: now,
                awaiting_pull_request: false,
                expired: false,
            },
        };
        caches.tickets.insert(
            key,
            TicketEntry {
                awaiting_pull_request: awaiting_pull_request.contains(id),
                ..entry
            },
        );
    }
    while caches.tickets.len() > MAX_CACHED_TICKETS {
        let Some(oldest) = caches
            .tickets
            .iter()
            .min_by_key(|(_, entry)| entry.read_at)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        caches.tickets.remove(&oldest);
    }
}

/// Makes the plan's reads of `ids` due on the next pass, keeping what they found.
pub(crate) fn expire_command_tickets(plan: &JobPlan, ids: &[String]) {
    let mut caches = caches();
    for id in ids {
        if let Some(entry) = caches.tickets.get_mut(&(plan.clone(), id.clone())) {
            entry.expired = true;
        }
    }
}

/// A ticket command's ticket as the cards and the Work list read it: its id's key is its team for
/// a key-number source (the parser reads it so), its JSON `team` for a number source.
pub(crate) fn ticket_of_command(source: CommandSourceId, ticket: CommandTicket) -> Ticket {
    Ticket {
        assigned_to_me: ticket.assignee.as_ref().is_some_and(|person| person.is_me),
        ticket: TicketRef::new(WorkSourceId::Command(source), ticket.id),
        title: Some(ticket.title),
        url: ticket.url,
        updated_at: ticket.updated_at,
        state: Some(ticket.state),
        status: ticket.status,
        assignee: ticket.assignee,
        labels: ticket.labels,
        cycle: ticket.cycle,
        branch_name: ticket.branch_name,
        project: None,
        pull_request_urls: ticket.pull_request_urls,
        team: ticket.team,
    }
}
