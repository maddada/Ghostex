//! A ticket command's open tickets, for the Work list of a workspace whose tracker it is: one list
//! per workspace, read by a background worker (`jobs.rs`) like a card's exact read, so
//! `refresh_work_feeds` only queues it and its lock never waits for a command. The answer is kept
//! at the workspace's generation, for the projects it was asked for, for as long as the other
//! sources' feeds, and while it is owed the list route keeps answering `refreshing: true`
//! (`command_lists_pending`).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use crate::logging::GxserverLogger;
use crate::paths::GxserverPaths;

use super::*;

/// How long an open list holds, its failure included: as long as the other sources' feeds
/// (`WORK_FEED_TTL` in feeds.rs).
const OPEN_LIST_TTL: Duration = Duration::from_secs(90);

/// One workspace's open list, as a Work list request plans it.
#[derive(Clone)]
pub(crate) struct OpenList {
    pub(crate) plan: JobPlan,
    /// The workspace's work-mode projects whose tracker the command is, which `list` is given.
    pub(crate) projects: Vec<CommandProject>,
    /// Their ids, sorted: `list` is given the projects, so its answer is kept, and its job queued,
    /// per set of projects (a Work list can ask for some of the workspace's).
    pub(crate) project_ids: Vec<String>,
    /// Why the command can't be asked right now (its program gone, a required credential unset),
    /// for the Work page's errors; `None` when it can.
    pub(crate) blocked: Option<String>,
    pub(crate) paths: GxserverPaths,
}

impl OpenList {
    pub(crate) fn new(
        plan: JobPlan,
        projects: Vec<CommandProject>,
        blocked: Option<String>,
        paths: GxserverPaths,
    ) -> Self {
        let mut project_ids: Vec<String> = projects.iter().map(|project| project.id.clone()).collect();
        project_ids.sort();
        Self {
            plan,
            projects,
            project_ids,
            blocked,
            paths,
        }
    }
}

/// Counts only, like the other request types of a ticket command.
impl std::fmt::Debug for OpenList {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenList")
            .field("plan", &self.plan)
            .field("projects", &self.projects.len())
            .field("blocked", &self.blocked.is_some())
            .finish()
    }
}

impl PlannedCall for OpenList {
    fn plan(&self) -> &JobPlan {
        &self.plan
    }

    fn projects(&self) -> &[String] {
        &self.project_ids
    }

    fn paths(&self) -> &GxserverPaths {
        &self.paths
    }
}

/// What a list answered: its tickets, or the failure's message for the Work page.
pub(crate) type OpenListAnswer = Result<Vec<CommandTicket>, String>;

struct Entry {
    answer: OpenListAnswer,
    fetched_at: Instant,
}

/// A list's plan and its sorted project ids (`OpenList::project_ids`).
type ListKey = (JobPlan, Vec<String>);

fn list_key(list: &OpenList) -> ListKey {
    (list.plan.clone(), list.project_ids.clone())
}

fn lists() -> MutexGuard<'static, HashMap<ListKey, Entry>> {
    static LISTS: OnceLock<Mutex<HashMap<ListKey, Entry>>> = OnceLock::new();
    LISTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Keeps a list's answer, in place of what an older generation of the workspace kept.
fn remember_open_list(list: &OpenList, answer: OpenListAnswer) {
    let plan = &list.plan;
    let mut lists = lists();
    lists.retain(|(kept, _), _| {
        kept.workspace_id != plan.workspace_id
            || kept.source != plan.source
            || kept.generation == plan.generation
    });
    lists.insert(
        list_key(list),
        Entry {
            answer,
            fetched_at: Instant::now(),
        },
    );
}

/// What the last list at the plan's generation, for the same projects, answered, however old.
pub(crate) fn cached_open_list(list: &OpenList) -> Option<OpenListAnswer> {
    lists().get(&list_key(list)).map(|entry| entry.answer.clone())
}

/// Whether the list should be read again: its command can run, it isn't backing off, and nothing
/// was kept at its generation for its projects, or what was is older than `OPEN_LIST_TTL`.
pub(crate) fn open_list_stale(list: &OpenList) -> bool {
    list.blocked.is_none()
        && !backing_off(&list.plan, CommandVerb::OpenList)
        && lists()
            .get(&list_key(list))
            .is_none_or(|entry| entry.fetched_at.elapsed() >= OPEN_LIST_TTL)
}

/// Whether a planned open list is still owed, whatever queueing it answered: a list the queue
/// refused as full is still stale, so the page asks again and its next refresh queues it. A list
/// whose command can't run, that is backing off, or whose failure was kept isn't owed, so the page
/// stops asking.
pub(crate) fn command_lists_pending(plan: &WorkFeedPlan) -> bool {
    plan.command_lists.iter().any(open_list_stale)
}

/// Runs one open list and keeps what it answered, and the back-off a failure asks for. A failure
/// is logged once, without the command's own words.
pub(crate) fn run_open_list(job: &OpenList) {
    // Checked again here: a failure may have started a back-off while the job waited.
    if backing_off(&job.plan, CommandVerb::OpenList) {
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
    let name = workspace_command(&job.plan.workspace_id).map_or_else(
        || job.plan.source.as_str().to_string(),
        |command| command.info.name.clone(),
    );
    let input = list_input(&job.projects, None);
    let call = CommandCall::new(
        &program,
        CommandVerb::OpenList,
        Some(&input),
        &work_dir,
        &env,
    );
    let started = Instant::now();
    let answer = command_list_after_spawn(&call, format, &job.projects, move || drop(writer));
    let duration = started.elapsed();
    let plan = &job.plan;
    let committed = commit_if_current(job, || match &answer {
        Ok(list) => remember_open_list(job, Ok(list.tickets.clone())),
        Err(failure) => {
            remember_open_list(job, Err(failure.message(&env.shown(&name))));
            if backs_off(failure) {
                back_off(plan, CommandVerb::OpenList);
            }
        }
    });
    if let (true, Err(failure)) = (committed, &answer) {
        log_command_failure(
            &GxserverLogger::new(job.paths.clone()),
            CommandVerb::OpenList,
            failure,
            duration,
            &env.shown(&plan.workspace_id),
            Some(&env.shown(plan.source.as_str())),
        );
    }
}
