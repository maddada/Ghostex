//! A ticket's details, which someone waits for: asked of the workspace's command (`detail`) on the
//! caller's own thread, in a process slot the background workers leave free (`SlotUse::Awaited`),
//! with the same checks as a background read. The store's writer lock and the placement lock are
//! held from the plan check until the command starts, and the answer is kept only while the plan
//! still holds. Callers asking for the same ticket at the same time share one call
//! (`single_flight`), and the details page's own cache keeps the answer for a while
//! (`cached_detail`).

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use serde_json::{json, Value};

use crate::logging::GxserverLogger;
use crate::paths::GxserverPaths;

use super::*;

/// What a details read answers when the command's setup or the project's workspace changed while
/// it read.
pub(crate) const DETAIL_CHANGED_MEANWHILE: &str =
    "The ticket command changed while Ghostex was reading the ticket; try again.";

/// What callers waiting on a details read get when it ended without an answer.
const DETAIL_UNANSWERED: &str = "Ghostex couldn't read the ticket; try again.";

/// One ticket's details, for a project placed in the plan's workspace.
pub(crate) struct DetailRead {
    pub(crate) plan: JobPlan,
    pub(crate) id: String,
    pub(crate) project: [String; 1],
    pub(crate) paths: GxserverPaths,
}

/// Prints no ticket id, like the other request types of a ticket command.
impl std::fmt::Debug for DetailRead {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DetailRead")
            .field("plan", &self.plan)
            .finish_non_exhaustive()
    }
}

impl PlannedCall for DetailRead {
    fn plan(&self) -> &JobPlan {
        &self.plan
    }

    fn projects(&self) -> &[String] {
        &self.project
    }

    fn paths(&self) -> &GxserverPaths {
        &self.paths
    }
}

impl DetailRead {
    /// The details cache's key: the ticket at the workspace's generation, so a changed setup never
    /// shows an answer read before it.
    pub(crate) fn cache_key(&self) -> String {
        format!(
            "cmd:{}:{}:{}:{}",
            self.plan.source.as_str(),
            self.plan.workspace_id,
            self.plan.generation,
            self.id
        )
    }
}

/// Asks the command for the ticket's details, for the details page (`ticket_detail_value`), once a
/// process slot is free, with `canStart: true`: an answer reaches only a caller that can read the
/// ticket (`read_detail`, `start_branch`). A found ticket also refreshes what its card shows and its last title. A
/// failure is logged once, without the command's own words, and answered with the runner's masked
/// message; `name` is the source's name.
pub(crate) fn run_detail(read: &DetailRead, name: &str) -> Result<Value, String> {
    run_detail_in(read, name, slots().take(SlotUse::Awaited))
}

/// `run_detail` in a slot the caller already holds.
fn run_detail_in(read: &DetailRead, name: &str, _slot: SlotGuard<'_>) -> Result<Value, String> {
    let Some(PreparedRun {
        program,
        format,
        env,
        work_dir,
        writer,
    }) = prepare(read)
    else {
        return Err(DETAIL_CHANGED_MEANWHILE.to_string());
    };
    #[cfg(test)]
    before_spawn(&read.plan.workspace_id);
    let input = detail_input(&read.id);
    let call = CommandCall::new(&program, CommandVerb::Detail, Some(&input), &work_dir, &env);
    let started = Instant::now();
    let answer = command_detail_after_spawn(&call, format, &[], move || drop(writer));
    let duration = started.elapsed();
    let plan = &read.plan;
    // Only the ticket asked for, compared as the format reads ids (the answer's is canonical
    // already): an answer for another one counts as no answer, and nothing of it is kept.
    let asked = format.parse_ref(&read.id);
    let ticket = answer
        .as_ref()
        .ok()
        .filter(|detail| asked.as_deref() == Some(detail.ticket.id.as_str()))
        .map(|detail| ticket_of_command(plan.source, detail.ticket.clone()));
    let committed = commit_if_current(read, || {
        if let Some(ticket) = &ticket {
            let ids = [ticket.ticket.id.clone()];
            remember_exact_read(plan, &ids, Some((std::slice::from_ref(ticket), &[])));
            keep_titles(&read.paths, plan, std::slice::from_ref(ticket), &ids);
        }
    });
    if !committed {
        return Err(DETAIL_CHANGED_MEANWHILE.to_string());
    }
    match (answer, ticket) {
        (Ok(detail), Some(ticket)) => {
            let mut fields = detail.fields;
            fields.insert("canStart".to_string(), json!(true));
            Ok(ticket_detail_value(TicketDetail {
                ticket: &ticket.ticket,
                title: ticket.title.clone(),
                url: ticket.url.clone(),
                description: detail.description,
                status: ticket.status.clone(),
                state: ticket.state.clone(),
                fields,
                comment_count: detail.comment_count,
                comments: detail.comments,
            }))
        }
        (Err(failure), _) => {
            log_command_failure(
                &GxserverLogger::new(read.paths.clone()),
                CommandVerb::Detail,
                &failure,
                duration,
                &env.shown(&plan.workspace_id),
                Some(&env.shown(plan.source.as_str())),
            );
            Err(failure.message(&env.shown(name)))
        }
        (Ok(_), None) => Err(DETAIL_UNANSWERED.to_string()),
    }
}

/// What a person's Start answers when no read of the ticket is running to wait for and no process
/// slot is free now: nothing ran.
pub(crate) fn start_busy(name: &str) -> String {
    format!("{name} is busy answering another request; try again in a moment.")
}

/// The ticket's details for a person's Start: the answer of a read of the same ticket already
/// running, which needs no slot; else a read in a process slot free now, else `start_busy` at
/// once. Which of the three is settled in one step (`single_flight_with`), and a Start that can
/// neither join nor take a slot starts no shared read, so no other caller ever gets its "busy".
/// It never waits for a slot, and never answers from the details cache, so a failure kept there
/// doesn't stop the person; a found answer goes into it for the details page.
pub(crate) fn start_detail(read: &DetailRead, name: &str) -> Result<Value, String> {
    let key = read.cache_key();
    let answer = single_flight_with(
        &key,
        || slots().try_take(SlotUse::Awaited),
        |slot| run_detail_in(read, name, slot),
    )
    .unwrap_or_else(|| {
        // A test pauses here, with every slot taken and no shared read started.
        #[cfg(test)]
        before_spawn(&read.plan.workspace_id);
        Err(start_busy(name))
    });
    if !detail_still_planned(read) {
        return Err(DETAIL_CHANGED_MEANWHILE.to_string());
    }
    if let Ok(found) = &answer {
        let _ = cached_detail(key, true, || Ok(found.clone()));
    }
    answer
}

/// Whether `read`'s plan still holds, its project's placement included, under the store's writer
/// lock and the placement lock: for a caller whose answer came from the cache or from another
/// caller's call, which checked only its own project.
pub(crate) fn detail_still_planned(read: &DetailRead) -> bool {
    commit_if_current(read, || {})
}

/// A read some caller runs while others wait for its answer.
#[derive(Default)]
struct Flight {
    answer: Mutex<Option<Result<Value, String>>>,
    landed: Condvar,
}

fn flights() -> MutexGuard<'static, HashMap<String, Arc<Flight>>> {
    static FLIGHTS: OnceLock<Mutex<HashMap<String, Arc<Flight>>>> = OnceLock::new();
    FLIGHTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The first caller's flight: on every return, and on a panic too, it leaves the map and wakes
/// every caller waiting on it, with `DETAIL_UNANSWERED` when it ended without an answer.
struct Landing<'f> {
    key: &'f str,
    flight: &'f Flight,
}

impl Drop for Landing<'_> {
    fn drop(&mut self) {
        flights().remove(self.key);
        let mut answer = self
            .flight
            .answer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        answer.get_or_insert_with(|| Err(DETAIL_UNANSWERED.to_string()));
        self.flight.landed.notify_all();
    }
}

/// Runs `read` for the first caller asking for `key`; a caller asking for it meanwhile waits for
/// that answer instead of running its own.
pub(crate) fn single_flight(
    key: &str,
    read: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    single_flight_with(key, || Some(()), |()| read())
        .unwrap_or_else(|| Err(DETAIL_UNANSWERED.to_string()))
}

/// `single_flight` for a first caller that runs `read` only with what `lead` gives it, asked under
/// the same lock that decides who is first: `None`, with nothing started, when no read of `key` is
/// running and `lead` gives nothing.
pub(crate) fn single_flight_with<G>(
    key: &str,
    lead: impl FnOnce() -> Option<G>,
    read: impl FnOnce(G) -> Result<Value, String>,
) -> Option<Result<Value, String>> {
    let (flight, held) = {
        let mut flights = flights();
        match flights.get(key) {
            Some(flight) => (Arc::clone(flight), None),
            None => {
                let held = lead()?;
                let flight = Arc::new(Flight::default());
                flights.insert(key.to_string(), Arc::clone(&flight));
                (flight, Some(held))
            }
        }
    };
    let Some(held) = held else {
        let mut answer = flight
            .answer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while answer.is_none() {
            answer = flight
                .landed
                .wait(answer)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        return Some(
            answer
                .clone()
                .unwrap_or_else(|| Err(DETAIL_UNANSWERED.to_string())),
        );
    };
    let landing = Landing {
        key,
        flight: &flight,
    };
    let answer = read(held);
    *flight
        .answer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(answer.clone());
    drop(landing);
    Some(answer)
}

/// How many callers are waiting on a flight for `key` besides its first: none when there is no
/// flight.
#[cfg(test)]
pub(crate) fn single_flight_waiting(key: &str) -> usize {
    flights()
        .get(key)
        .map_or(0, |flight| Arc::strong_count(flight).saturating_sub(2))
}
