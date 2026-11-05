//! Creating a ticket with a workspace's ticket command (`/api/createWorkTicket`), which someone
//! waits for. A `create` must never run twice for one request, since the first may have made the
//! ticket even when it timed out: so it is never queued, never retried, one runs at a time per
//! workspace and command, and after a timeout creates there stay refused until the block expires.
//! It never starts late either: its command starts only within `CREATE_WRITER_WAIT` of the
//! request's arrival (its time queued for a worker counts), checked once more right before the
//! spawn; it waits for the store's writer lock no longer than that and takes a process slot only
//! when one is free then, else it refuses having run nothing. Once started, the runner ends it at
//! its time limit.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::logging::GxserverLogger;
use crate::paths::GxserverPaths;

use super::*;

/// How long after its request arrived a create may still start: past it, it refuses having run
/// nothing, whatever it waited on (a worker, the store's writer lock).
pub(crate) const CREATE_WRITER_WAIT: Duration = Duration::from_secs(2);

/// What ending a timed-out create's process tree and answering may take past its time limit.
pub(crate) const CREATE_CLEANUP: Duration = Duration::from_secs(5);

/// How long creates in a workspace and command stay refused after one timed out.
pub(crate) const CREATE_BLOCK: Duration = BACK_OFF;

/// Why a created ticket can't be shown: its id holds a value the workspace stores. The ticket
/// exists, so the words say so; they quote nothing the command sent.
pub(crate) const CREATED_ID_SPELLS_CREDENTIAL: &str =
    "created a ticket, but its ID contains the value of a credential this workspace keeps, which Ghostex can't hide; open it to find the ticket.";

/// One create, for the project it was asked from.
pub(crate) struct CreateCall {
    pub(crate) plan: JobPlan,
    pub(crate) project: [String; 1],
    pub(crate) paths: GxserverPaths,
}

impl PlannedCall for CreateCall {
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

/// What a create that ran answers: the ticket it made, or the command's failure, masked.
#[derive(Debug)]
pub(crate) enum CreateOutcome {
    Created(CommandCreated),
    Failed {
        error: String,
        /// The command's output, only for an answer Ghostex couldn't read and only while no
        /// credential is set (`CommandFailure::Unreadable`).
        detail: Option<String>,
        /// It timed out, so the command may have made the ticket.
        may_have_been_created: bool,
    },
}

/// The workspace and command a create runs for.
type CreateKey = (String, String);

fn creating() -> MutexGuard<'static, HashSet<CreateKey>> {
    static CREATING: OnceLock<Mutex<HashSet<CreateKey>>> = OnceLock::new();
    CREATING
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn blocks() -> MutexGuard<'static, HashMap<CreateKey, Instant>> {
    static BLOCKS: OnceLock<Mutex<HashMap<CreateKey, Instant>>> = OnceLock::new();
    BLOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The one create running for its key, until dropped (on every return, and on a panic too).
struct Creating(CreateKey);

impl Creating {
    fn take(key: &CreateKey) -> Option<Self> {
        creating().insert(key.clone()).then(|| Self(key.clone()))
    }
}

impl Drop for Creating {
    fn drop(&mut self) {
        creating().remove(&self.0);
    }
}

/// Whether creates for `key` are refused: one timed out less than `CREATE_BLOCK` ago. Keyed
/// without the workspace's generation, since a new setup doesn't say whether that create made a
/// ticket; nothing but time clears it.
fn create_blocked(key: &CreateKey) -> bool {
    let mut blocks = blocks();
    let now = Instant::now();
    blocks.retain(|_, until| *until > now);
    blocks.contains_key(key)
}

/// Runs one create for `call` with `input` (`create_input`), ended at `timeout`, only if it can
/// start by `admit_by` (the request's arrival plus `CREATE_WRITER_WAIT`). `name` is the source's
/// name, already through the command's mask. Every refusal is in Ghostex's own words and runs
/// nothing, except a created id that holds a stored value, whose words say the ticket was made.
pub(crate) fn run_create(
    call: &CreateCall,
    name: &str,
    input: &Value,
    timeout: Duration,
    admit_by: Instant,
) -> Result<CreateOutcome, String> {
    let key = (
        call.plan.workspace_id.clone(),
        call.plan.source.as_str().to_string(),
    );
    let _creating = Creating::take(&key)
        .ok_or_else(|| format!("{name} is still creating a ticket; wait for it to answer."))?;
    if create_blocked(&key) {
        return Err(format!(
            "{name} didn't answer a create a moment ago, so it may have made the ticket; check {name} before creating again."
        ));
    }
    let busy = || {
        format!(
            "{name} is busy answering another request; nothing was created. Try again in a moment."
        )
    };
    // The writer first, then a slot only if one is free now: a create never holds a slot while it
    // waits. Neither is waited for past `admit_by`, and the command doesn't start after it.
    let left = admit_by.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(busy());
    }
    let writer = hold_work_source_writer_within(left).ok_or_else(busy)?;
    let _slot = slots().try_take(SlotUse::Awaited).ok_or_else(busy)?;
    let (
        PreparedRun {
            program,
            format,
            env,
            work_dir,
            writer,
        },
        stored,
    ) = prepare_holding(call, writer).map_err(|unprepared| match unprepared {
        Unprepared::Changed => format!("{name}'s setup changed while Ghostex got ready to create the ticket; nothing was created. Try again."),
        Unprepared::Storage(text) => format!("{text} Nothing was created."),
    })?;
    #[cfg(test)]
    before_spawn(&call.plan.workspace_id);
    // Checked again right before the command starts: taking the slot and preparing can run past
    // it. Returning drops the slot and the writer lock.
    if Instant::now() >= admit_by {
        return Err(busy());
    }
    let command_call =
        CommandCall::new(&program, CommandVerb::Create, Some(input), &work_dir, &env)
            .with_timeout(timeout);
    let started = Instant::now();
    let answer = command_create_after_spawn(&command_call, format, move || drop(writer));
    let duration = started.elapsed();
    match answer {
        Ok(CreateAnswer::HiddenId) => Err(format!("{name} {CREATED_ID_SPELLS_CREDENTIAL}")),
        Ok(CreateAnswer::Created(mut created)) => {
            if stored.found_in(&created.id) {
                return Err(format!("{name} {CREATED_ID_SPELLS_CREDENTIAL}"));
            }
            created.branch_name = created
                .branch_name
                .filter(|branch| !stored.found_in(branch));
            Ok(CreateOutcome::Created(created))
        }
        Err(failure) => {
            log_command_failure(
                &GxserverLogger::new(call.paths.clone()),
                CommandVerb::Create,
                &failure,
                duration,
                &env.shown(&call.plan.workspace_id),
                Some(&env.shown(call.plan.source.as_str())),
            );
            let may_have_been_created = matches!(failure, CommandFailure::TimedOut { .. });
            if may_have_been_created {
                blocks().insert(key, Instant::now() + CREATE_BLOCK);
            }
            Ok(CreateOutcome::Failed {
                error: failure.message(name),
                detail: match &failure {
                    CommandFailure::Unreadable { stdout, .. } => Some(stdout.clone()),
                    _ => None,
                },
                may_have_been_created,
            })
        }
    }
}
