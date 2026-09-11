use super::{helpers, model::*};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
/// CDXC:AgentProviders 2026-09-11 WHY:
/// The discovered-usage snapshot is process-wide rather than a field of AccountRuntime because new-session launch (`launch::apply_new_session`) runs from automations, board start-work, worktree ops, and drafts, which hold only a database handle and no AppState, yet must pick the account with the most limit remaining.
static SNAPSHOT: RwLock<Snapshot> = RwLock::new(Snapshot {
    accounts: Vec::new(),
    errors: BTreeMap::new(),
    fetched_at: None,
});
pub(crate) fn current_snapshot() -> Snapshot {
    SNAPSHOT.read().unwrap_or_else(|e| e.into_inner()).clone()
}
#[derive(Default)]
pub(crate) struct AccountRuntime {
    pub setup_jobs: super::setup::SetupJobs,
    pub mutations: Mutex<()>,
    pub history: Arc<super::history::HistoryRuntime>,
    poll_gate: Mutex<()>,
}
impl AccountRuntime {
    pub fn invalidate(&self) {
        SNAPSHOT
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .fetched_at = None;
    }
    pub fn snapshot(&self) -> Snapshot {
        current_snapshot()
    }
    pub fn refresh(&self, home: &Path, force: bool) -> Snapshot {
        let cached = self.snapshot();
        let age = if force { 15 } else { 120 };
        if cached
            .fetched_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(age))
        {
            return cached;
        }
        let _gate = self.poll_gate.lock().unwrap_or_else(|e| e.into_inner());
        let cached = self.snapshot();
        if cached
            .fetched_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(age))
        {
            return cached;
        }
        let mut next = Snapshot::default();
        std::thread::scope(|scope| {
            let tasks: Vec<_> = [Provider::Claude, Provider::Codex]
                .into_iter()
                .map(|provider| {
                    (
                        provider,
                        scope.spawn(move || helpers::discover(home, provider)),
                    )
                })
                .collect();
            for (provider, task) in tasks {
                match task.join() {
                    Ok(Ok(rows)) => next.accounts.extend(rows),
                    Ok(Err(error)) => {
                        next.errors.insert(provider, error);
                    }
                    Err(_) => {
                        next.errors
                            .insert(provider, "Account discovery did not complete.".into());
                    }
                }
            }
        });
        next.fetched_at = Some(Instant::now());
        *SNAPSHOT.write().unwrap_or_else(|e| e.into_inner()) = next.clone();
        next
    }
}
