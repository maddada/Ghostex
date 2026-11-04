//! Ticket command calls the background asks for: one bounded queue, drained by
//! `BACKGROUND_WORKERS` threads started on first use, so the card pass and the Work list's feed
//! lock only queue and never wait for a command, and the threads never grow with the jobs.
//!
//! A job carries what it was planned against (`JobPlan`). Its worker checks that again under the
//! store's writer lock, copies the command's path and credentials, and holds the lock until the
//! command has started (`run_bounded_after_spawn`), never while it waits for the answer; then it
//! takes the lock again to keep the result, only if the plan still holds. So a clear, a credential
//! change, a Switch, a team join or a workspace deletion either waits for the start or makes the
//! job drop what it read: an old path or credential never runs after the change, and an old
//! answer never lands in a newer setup.
//!
//! CDXC:WorkMode 2026-11-02 WHY:
//! A command may take its whole time limit (30 s for an exact read). Run inline, it would hold the card pass's serial loop over every source, and `refresh_work_feeds`' one lock, for that long. So the background never runs one inline: it queues the call and moves on, and the card shows the answer on a later snapshot.

use std::collections::{HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use crate::logging::GxserverLogger;
use crate::paths::GxserverPaths;

use super::*;

pub(crate) const BACKGROUND_WORKERS: usize = 3;
/// Commands running at once. The workers hold at most `BACKGROUND_WORKERS`, so a call someone
/// waits for (a ticket's details) always finds one.
pub(crate) const PROCESS_SLOTS: usize = 4;
pub(crate) const QUEUE_CAPACITY: usize = 64;
/// Exact reads one card pass queues per workspace's command, each of at most `MAX_EXACT_IDS`.
pub(crate) const MAX_COMMAND_CALLS_PER_PASS: usize = 4;

/// An exact read the card pass asks for: tickets of one workspace's command.
pub(crate) struct ExactRead {
    pub(crate) plan: JobPlan,
    pub(crate) ids: Vec<String>,
    /// The projects whose sessions named `ids`, each placed in the plan's workspace when the pass
    /// queued it.
    pub(crate) projects: Vec<String>,
    pub(crate) paths: GxserverPaths,
}

/// The ids' count only, like the other request types of a ticket command.
impl std::fmt::Debug for ExactRead {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExactRead")
            .field("plan", &self.plan)
            .field("ids", &self.ids.len())
            .field("projects", &self.projects.len())
            .finish()
    }
}

/// What a call was planned against and for, which `prepare` and `commit_if_current` check again.
pub(crate) trait PlannedCall {
    fn plan(&self) -> &JobPlan;
    /// The projects it reads for, each placed in the plan's workspace when it was planned.
    fn projects(&self) -> &[String];
    fn paths(&self) -> &GxserverPaths;
}

impl PlannedCall for ExactRead {
    fn plan(&self) -> &JobPlan {
        &self.plan
    }

    fn projects(&self) -> &[String] {
        &self.projects
    }

    fn paths(&self) -> &GxserverPaths {
        &self.paths
    }
}

impl ExactRead {
    fn key(&self) -> JobKey {
        let mut ids = self.ids.clone();
        ids.sort();
        JobKey {
            plan: self.plan.clone(),
            verb: CommandVerb::ExactList,
            ids,
        }
    }
}

/// What a worker runs: a card's exact read, or the hourly `info` check (`info_check.rs`).
#[derive(Debug)]
pub(crate) enum CommandJob {
    ExactRead(ExactRead),
    Info(InfoCheck),
}

impl From<ExactRead> for CommandJob {
    fn from(read: ExactRead) -> Self {
        Self::ExactRead(read)
    }
}

impl From<InfoCheck> for CommandJob {
    fn from(check: InfoCheck) -> Self {
        Self::Info(check)
    }
}

impl CommandJob {
    fn plan(&self) -> &JobPlan {
        match self {
            Self::ExactRead(read) => &read.plan,
            Self::Info(check) => &check.plan,
        }
    }

    fn verb(&self) -> CommandVerb {
        match self {
            Self::ExactRead(_) => CommandVerb::ExactList,
            Self::Info(_) => CommandVerb::Info,
        }
    }

    fn key(&self) -> JobKey {
        match self {
            Self::ExactRead(read) => read.key(),
            Self::Info(check) => JobKey {
                plan: check.plan.clone(),
                verb: CommandVerb::Info,
                ids: Vec::new(),
            },
        }
    }
}

/// A job already queued or running is not queued again.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct JobKey {
    plan: JobPlan,
    verb: CommandVerb,
    ids: Vec<String>,
}

/// What queueing a job did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum QueueOutcome {
    Queued,
    /// The same job is queued or running.
    Duplicate,
    /// The queue holds `QUEUE_CAPACITY` jobs; the next pass asks again.
    Full,
    /// Its key is backing off (`backs_off`).
    BackingOff,
}

#[derive(Default)]
struct QueueState {
    jobs: VecDeque<CommandJob>,
    /// Queued or running.
    keys: HashSet<JobKey>,
}

pub(crate) struct JobQueue {
    capacity: usize,
    state: Mutex<QueueState>,
    ready: Condvar,
}

impl JobQueue {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::new(QueueState::default()),
            ready: Condvar::new(),
        }
    }

    fn state(&self) -> MutexGuard<'_, QueueState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Returns at once.
    pub(crate) fn push(&self, job: impl Into<CommandJob>) -> QueueOutcome {
        let job = job.into();
        if backing_off(job.plan(), job.verb()) {
            return QueueOutcome::BackingOff;
        }
        let key = job.key();
        let mut state = self.state();
        if state.keys.contains(&key) {
            return QueueOutcome::Duplicate;
        }
        if state.jobs.len() >= self.capacity {
            return QueueOutcome::Full;
        }
        state.keys.insert(key);
        state.jobs.push_back(job);
        self.ready.notify_one();
        QueueOutcome::Queued
    }

    /// The next job, once there is one, and its key, held as running until it is dropped.
    fn take(&self) -> (CommandJob, RunningKey<'_>) {
        let mut state = self.state();
        loop {
            if let Some(job) = state.jobs.pop_front() {
                let key = job.key();
                return (job, RunningKey { queue: self, key });
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.state().jobs.len()
    }
}

/// A job's key, held as running until the job ends, on every return and on a panic too.
struct RunningKey<'q> {
    queue: &'q JobQueue,
    key: JobKey,
}

impl Drop for RunningKey<'_> {
    fn drop(&mut self) {
        self.queue.state().keys.remove(&self.key);
    }
}

/// The queue, its workers started on first use. A worker that can't start is logged once, in
/// `paths`' log: with none, the queue fills and its jobs never run.
fn queue(paths: &GxserverPaths) -> &'static JobQueue {
    static QUEUE: OnceLock<JobQueue> = OnceLock::new();
    static WORKERS: OnceLock<()> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| JobQueue::new(QUEUE_CAPACITY));
    WORKERS.get_or_init(|| {
        let unstarted = (0..BACKGROUND_WORKERS)
            .filter(|_| {
                std::thread::Builder::new()
                    .name("work-source".to_string())
                    .spawn(move || drain(queue, run_command_job))
                    .is_err()
            })
            .count();
        if unstarted > 0 {
            log_workers_unstarted(paths, unstarted);
        }
    });
    queue
}

/// One error line in fixed words when background workers couldn't start.
fn log_workers_unstarted(paths: &GxserverPaths, unstarted: usize) {
    let _ = GxserverLogger::new(paths.clone()).log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Error,
        event: "work_source_workers_unstarted".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(
            "Ghostex couldn't start a ticket command worker thread; cards may not show new ticket reads until gxserver restarts."
                .to_string(),
        ),
        details: Some(serde_json::json!({ "unstarted": unstarted, "workers": BACKGROUND_WORKERS })),
    });
}

/// Queues an exact read for a worker; returns at once.
pub(crate) fn queue_exact_read(job: ExactRead) -> QueueOutcome {
    queue(&job.paths).push(job)
}

/// Queues the hourly `info` check for a worker; returns at once.
pub(crate) fn queue_info_check(job: InfoCheck) -> QueueOutcome {
    queue(&job.paths).push(job)
}

fn run_command_job(job: &CommandJob) {
    match job {
        CommandJob::ExactRead(read) => run_exact_read(read),
        CommandJob::Info(check) => run_info_check(check),
    }
}

/// A worker: runs each job `queue` hands it, for good. A panic ends only that job: its slot, its
/// writer lock and its key are guards, and the panic hook has already reported it.
pub(crate) fn drain(queue: &JobQueue, run: impl Fn(&CommandJob)) {
    loop {
        let (job, running) = queue.take();
        let _ = catch_unwind(AssertUnwindSafe(|| run(&job)));
        drop(running);
    }
}

/// Which share of the process slots a call may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotUse {
    /// All but one.
    Background,
    /// Any, the reserved one too: a call someone waits for.
    Awaited,
}

pub(crate) struct Slots {
    limit: usize,
    held: Mutex<usize>,
    freed: Condvar,
}

impl Slots {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            held: Mutex::new(0),
            freed: Condvar::new(),
        }
    }

    /// Waits for a slot `slot_use` may take.
    pub(crate) fn take(&self, slot_use: SlotUse) -> SlotGuard<'_> {
        let limit = match slot_use {
            SlotUse::Background => self.limit - 1,
            SlotUse::Awaited => self.limit,
        };
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *held >= limit {
            held = self
                .freed
                .wait(held)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *held += 1;
        SlotGuard { slots: self }
    }

    /// Takes a slot only when one is free now.
    #[cfg(test)]
    pub(crate) fn try_take(&self, slot_use: SlotUse) -> Option<SlotGuard<'_>> {
        let limit = match slot_use {
            SlotUse::Background => self.limit - 1,
            SlotUse::Awaited => self.limit,
        };
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (*held < limit).then(|| {
            *held += 1;
            SlotGuard { slots: self }
        })
    }
}

pub(crate) struct SlotGuard<'s> {
    slots: &'s Slots,
}

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        let mut held = self
            .slots
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *held -= 1;
        self.slots.freed.notify_all();
    }
}

pub(super) fn slots() -> &'static Slots {
    static SLOTS: OnceLock<Slots> = OnceLock::new();
    SLOTS.get_or_init(|| Slots::new(PROCESS_SLOTS))
}

/// What a worker runs: copied out of the store and the credentials file in one step, under the
/// store's writer lock, which it holds until the command starts.
pub(crate) struct PreparedRun {
    pub(crate) program: PathBuf,
    pub(crate) format: IdFormat,
    pub(crate) env: CommandEnv,
    pub(crate) work_dir: PathBuf,
    /// The store's writer lock and the placement lock, both released once the command starts.
    pub(super) writer: (
        MutexGuard<'static, ()>,
        crate::domain::repository::project::PlacementGuard,
    ),
}

/// The paths through its mask and its environment's own `Debug`, which prints no credential.
impl std::fmt::Debug for PreparedRun {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedRun")
            .field(
                "program",
                &self.env.shown(&self.program.display().to_string()),
            )
            .field("format", &self.format)
            .field("env", &self.env)
            .field(
                "work_dir",
                &self.env.shown(&self.work_dir.display().to_string()),
            )
            .finish()
    }
}

/// Whether the job's plan still holds: the workspace at its generation, still pinning its source,
/// not in a team, and every project the job reads for still placed in it, as the database says
/// (`project_placed_in`). The caller holds the store's writer lock and the placement lock
/// (`hold_project_placement`), which every change to a project's placement takes.
fn plan_holds(job: &impl PlannedCall, db: &rusqlite::Connection) -> bool {
    let plan = job.plan();
    work_source_generation(&plan.workspace_id) == plan.generation
        && workspace_command(&plan.workspace_id)
            .is_some_and(|command| command.source_id == plan.source.as_str())
        && !crate::team_sync::workspace_has_team(job.paths(), &plan.workspace_id)
        && job
            .projects()
            .iter()
            .all(|project_id| project_placed_in(db, project_id, &plan.workspace_id))
}

/// Whether the project is in `workspace_id` now, read from the database (its row, a worktree's
/// parent, the workspaces document), not from the map the projection reads, which a change
/// replaces only after it is written.
fn project_placed_in(db: &rusqlite::Connection, project_id: &str, workspace_id: &str) -> bool {
    let repository = crate::domain::DomainRepository::new(db, "");
    let (Ok(Some(project)), Ok(workspaces)) = (
        repository.get_project(project_id),
        crate::workspaces::read_sidebar_workspaces(db),
    ) else {
        return false;
    };
    let parent: Vec<serde_json::Value> = project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(serde_json::Value::as_str)
        .and_then(|parent_id| repository.get_project(parent_id).ok().flatten())
        .into_iter()
        .collect();
    crate::workspaces::project_workspace_id(&workspaces, &project, &parent) == workspace_id
}

/// Takes the store's writer lock and, while the plan holds and every project the job reads for is
/// still placed in its workspace, copies what the job runs with: the command's path, its
/// credentials, and the mask of everything the workspace stores (`stored_values`, read strictly),
/// which masks the answer without being passed to the command. `None` when the plan no longer
/// holds or what the workspace stores can't be read: then nothing runs.
pub(crate) fn prepare(job: &impl PlannedCall) -> Option<PreparedRun> {
    let (plan, paths) = (job.plan(), job.paths());
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    let writer = hold_work_source_writer().ok()?;
    let placement = crate::domain::repository::project::hold_project_placement().ok()?;
    if !plan_holds(job, &db) {
        return None;
    }
    let command = workspace_command(&plan.workspace_id)?;
    let stored = stored_values(&db, paths, &plan.workspace_id).ok()?;
    let work_dir = command_work_dir(paths, Some(&command.source_id)).ok()?;
    Some(PreparedRun {
        program: command.resolved_path.clone(),
        format: command.info.id_format,
        env: command_env_of(paths, &plan.workspace_id, Some(&command)).masking_too(&stored.mask),
        work_dir,
        writer: (writer, placement),
    })
}

/// Under the store's writer lock again: runs `effects` (caches, back-off, titles) only while the
/// job's plan still holds, its projects' placement included, and says whether it did. A writer
/// that changes the setup, or moves one of its projects, waits for them.
pub(crate) fn commit_if_current(job: &impl PlannedCall, effects: impl FnOnce()) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(job.paths()) else {
        return false;
    };
    let Ok(_writer) = hold_work_source_writer() else {
        return false;
    };
    // Released before `effects`, which need only the store's writer lock.
    let holds = match crate::domain::repository::project::hold_project_placement() {
        Ok(_placement) => plan_holds(job, &db),
        Err(_) => false,
    };
    if !holds {
        return false;
    }
    effects();
    true
}

/// Runs one exact read and keeps what it answered for the cards: each ticket, its title, and the
/// back-off a failure asks for. A failure is logged once, without the command's own words.
pub(crate) fn run_exact_read(job: &ExactRead) {
    // Checked again here: a failure may have started a back-off while the job waited.
    if backing_off(&job.plan, CommandVerb::ExactList) {
        return;
    }
    let _slot = slots().take(SlotUse::Background);
    let Some(PreparedRun {
        program,
        format,
        env,
        work_dir,
        writer,
    }) = prepare(job)
    else {
        return;
    };
    #[cfg(test)]
    before_spawn(&job.plan.workspace_id);
    let input = list_input(&[], Some(&job.ids));
    let call = CommandCall::new(
        &program,
        CommandVerb::ExactList,
        Some(&input),
        &work_dir,
        &env,
    );
    let started = Instant::now();
    let answer = command_list_after_spawn(&call, format, &[], move || drop(writer));
    let duration = started.elapsed();
    let plan = &job.plan;
    let committed = commit_if_current(job, || match &answer {
        Ok(list) => {
            let tickets: Vec<Ticket> = list
                .tickets
                .iter()
                .filter(|ticket| job.ids.contains(&ticket.id))
                .map(|ticket| ticket_of_command(plan.source, ticket.clone()))
                .collect();
            remember_exact_read(plan, &job.ids, Some((&tickets, &list.not_found)));
            keep_titles(&job.paths, plan, &tickets, &job.ids);
        }
        Err(failure) => {
            remember_exact_read(plan, &job.ids, None);
            if backs_off(failure) {
                back_off(plan, CommandVerb::ExactList);
            }
        }
    });
    if let (true, Err(failure)) = (committed, &answer) {
        log_command_failure(
            &GxserverLogger::new(job.paths.clone()),
            CommandVerb::ExactList,
            failure,
            duration,
            &env.shown(&plan.workspace_id),
            Some(&env.shown(plan.source.as_str())),
        );
    }
}

/// Saves the last titles of `tickets`, which a read of `ids` answered. When they can't be saved,
/// those tickets are read again on the next pass, which saves their titles then, and one line in
/// fixed words is logged.
pub(super) fn keep_titles(
    paths: &GxserverPaths,
    plan: &JobPlan,
    tickets: &[Ticket],
    ids: &[String],
) {
    let titles: Vec<(String, String)> = tickets
        .iter()
        .filter_map(|ticket| Some((ticket.ticket.id.clone(), ticket.title.clone()?)))
        .collect();
    let saved = crate::storage::open_gxserver_database(paths)
        .map_err(|_| ())
        .and_then(|db| {
            save_ticket_titles(&db, &plan.workspace_id, plan.source.as_str(), &titles)
                .map_err(|_| ())
        });
    if saved.is_err() {
        expire_command_tickets(plan, ids);
        log_titles_unsaved(paths);
    }
}

/// One error line in fixed words when the tickets' last titles couldn't be saved: the database's
/// own message can quote a title.
fn log_titles_unsaved(paths: &GxserverPaths) {
    let _ = GxserverLogger::new(paths.clone()).log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Error,
        event: "work_ticket_titles_unsaved".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(
            "Ghostex couldn't save ticket commands' last titles; it reads them again on the next pass."
                .to_string(),
        ),
        details: None,
    });
}

/// What a test makes a worker do once it has prepared a job, while it holds a slot and the store's
/// writer lock, right before the command starts.
#[cfg(test)]
pub(crate) enum BeforeSpawn {
    /// Say so on `reached`, then wait on `release`.
    Pause {
        reached: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    },
    /// Unwind, as a panic does, every time.
    Panic,
}

#[cfg(test)]
fn before_spawn_hooks() -> MutexGuard<'static, std::collections::HashMap<String, BeforeSpawn>> {
    static HOOKS: OnceLock<Mutex<std::collections::HashMap<String, BeforeSpawn>>> = OnceLock::new();
    HOOKS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Sets what a worker does before it starts the command of a job in `workspace_id`.
#[cfg(test)]
pub(crate) fn hook_before_spawn(workspace_id: &str, hook: BeforeSpawn) {
    before_spawn_hooks().insert(workspace_id.to_string(), hook);
}

#[cfg(test)]
pub(crate) fn unhook_before_spawn(workspace_id: &str) {
    before_spawn_hooks().remove(workspace_id);
}

/// What a test set for `workspace_id` (`hook_before_spawn`), at a worker's start, at a links
/// write (`with_checked_ticket_links`), and after an hourly check kept its result
/// (`recheck_work_source`).
#[cfg(test)]
pub(crate) fn before_spawn(workspace_id: &str) {
    let hook = {
        let mut hooks = before_spawn_hooks();
        match hooks.get(workspace_id) {
            Some(BeforeSpawn::Panic) => {
                drop(hooks);
                std::panic::resume_unwind(Box::new("a job unwinds before its command starts"));
            }
            Some(BeforeSpawn::Pause { .. }) => hooks.remove(workspace_id),
            None => None,
        }
    };
    if let Some(BeforeSpawn::Pause { reached, release }) = hook {
        let _ = reached.send(());
        let _ = release.recv();
    }
}
