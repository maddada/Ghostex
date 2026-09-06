use super::{helpers, model::*};
use std::{
    path::Path,
    sync::{Mutex, RwLock},
    time::{Duration, Instant},
};
#[derive(Default)]
pub(crate) struct AccountRuntime {
    pub mutations: Mutex<()>,
    poll_gate: Mutex<()>,
    snapshot: RwLock<Snapshot>,
}
impl AccountRuntime {
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn refresh(&self, home: &Path, force: bool) -> Snapshot {
        let _gate = self.poll_gate.lock().unwrap_or_else(|e| e.into_inner());
        let cached = self.snapshot();
        let age = if force { 15 } else { 120 };
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
        *self.snapshot.write().unwrap_or_else(|e| e.into_inner()) = next.clone();
        next
    }
}
