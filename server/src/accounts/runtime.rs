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
/// Makes the next accounts read discover again, for callers without an `AccountRuntime`.
pub(crate) fn invalidate_snapshot() {
    SNAPSHOT
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .fetched_at = None;
}
#[derive(Default)]
pub(crate) struct AccountRuntime {
    pub setup_jobs: super::setup::SetupJobs,
    mutations: Mutex<()>,
    pub history: Arc<super::history::HistoryRuntime>,
    pub recovery: Arc<super::recovery::RecoverySignals>,
    poll_gate: Mutex<()>,
}
impl AccountRuntime {
    /// GitHub #410 WHY:
    /// `mutations` serialises account changes but guards no data: every holder re-reads the registry from SQLite under it. A panic while it was held (a login thread calling `tokio::spawn` without a runtime) poisoned it, and every later account operation and chat send then failed with "poisoned lock" until gxserver restarted, so a poisoned gate is taken as it stands.
    pub fn mutation_gate(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutations.lock().unwrap_or_else(|e| e.into_inner())
    }
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
        self.discover_all(home)
    }
    /// Refreshes unless the cached snapshot started after `since`, so a reading already in flight when a reset was claimed cannot stand in for the post-claim one.
    pub fn refresh_since(&self, home: &Path, since: Instant) -> Snapshot {
        let _gate = self.poll_gate.lock().unwrap_or_else(|e| e.into_inner());
        let cached = self.snapshot();
        if cached.fetched_at.is_some_and(|t| t > since) {
            return cached;
        }
        self.discover_all(home)
    }
    fn discover_all(&self, home: &Path) -> Snapshot {
        let started = Instant::now();
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
        next.fetched_at = Some(started);
        *SNAPSHOT.write().unwrap_or_else(|e| e.into_inner()) = next.clone();
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GitHub #410: a panic while the gate was held must not block every later account operation.
    #[test]
    fn a_poisoned_mutation_gate_is_still_taken() {
        let runtime = Arc::new(AccountRuntime::default());
        let poisoner = Arc::clone(&runtime);
        let _ = std::thread::spawn(move || {
            let _gate = poisoner.mutation_gate();
            panic!("a holder failed");
        })
        .join();
        assert!(runtime.mutations.is_poisoned());
        drop(runtime.mutation_gate());
    }
}
