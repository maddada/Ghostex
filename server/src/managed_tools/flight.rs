//! One check at a time per tool: a request that arrives while a tool is being checked waits for
//! that check's answer instead of starting another.

use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

/// The checks running now, by key.
pub(crate) struct Flights<K, V> {
    running: Mutex<HashMap<K, Arc<Flight<V>>>>,
}

/// One running check; every request for its key waits on the same answer.
pub(crate) struct Flight<V> {
    /// `Some(answer)` once done; `Some(None)` when the check panicked.
    answer: Mutex<Option<Option<V>>>,
    ready: Condvar,
}

impl<K, V> Flights<K, V>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Clone + Send + 'static,
{
    pub(crate) fn new() -> Self {
        Self {
            running: Mutex::new(HashMap::new()),
        }
    }

    /// The check running for `key`, or a new one running `work` on a thread of its own. The check
    /// finishes even when nobody waits for it any more, and the next request after it starts anew.
    ///
    /// CDXC:ManagedTools 2026-10-27 WHY:
    /// GitHub #336: every open of Settings > Integrations (and every Retry) started another `gh
    /// --version` while the last one still looped, and the reporter found several running at once.
    /// A request for a tool that is still being checked shares that check's answer.
    pub(crate) fn join(
        &'static self,
        key: K,
        work: impl FnOnce() -> V + Send + 'static,
    ) -> Arc<Flight<V>> {
        let Ok(mut running) = self.running.lock() else {
            return Flight::finished(None);
        };
        if let Some(flight) = running.get(&key) {
            return flight.clone();
        }
        let flight = Arc::new(Flight {
            answer: Mutex::new(None),
            ready: Condvar::new(),
        });
        running.insert(key.clone(), flight.clone());
        drop(running);
        let landing = Landing {
            flights: self,
            key,
            flight: flight.clone(),
            answer: None,
        };
        // A thread that cannot start drops the landing with its closure, which lands the flight empty.
        let _ = std::thread::Builder::new()
            .name("managed-tool-check".into())
            .spawn(move || {
                let mut landing = landing;
                landing.answer = Some(work());
            });
        flight
    }
}

impl<V: Clone> Flight<V> {
    fn finished(answer: Option<V>) -> Arc<Self> {
        Arc::new(Self {
            answer: Mutex::new(Some(answer)),
            ready: Condvar::new(),
        })
    }

    /// The answer, or `None` when it isn't there by `deadline` (or the check failed).
    pub(crate) fn wait_until(&self, deadline: Instant) -> Option<V> {
        let mut answer = self.answer.lock().ok()?;
        loop {
            if let Some(answer) = answer.as_ref() {
                return answer.clone();
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            answer = self.ready.wait_timeout(answer, left).ok()?.0;
        }
    }
}

/// Publishes the answer and frees the key when the check ends, a panic included.
struct Landing<K: Eq + Hash + Clone + Send + 'static, V: Clone + Send + 'static> {
    flights: &'static Flights<K, V>,
    key: K,
    flight: Arc<Flight<V>>,
    answer: Option<V>,
}

impl<K: Eq + Hash + Clone + Send + 'static, V: Clone + Send + 'static> Drop for Landing<K, V> {
    fn drop(&mut self) {
        if let Ok(mut running) = self.flights.running.lock() {
            if running
                .get(&self.key)
                .is_some_and(|flight| Arc::ptr_eq(flight, &self.flight))
            {
                running.remove(&self.key);
            }
        }
        if let Ok(mut answer) = self.flight.answer.lock() {
            *answer = Some(self.answer.take());
        }
        self.flight.ready.notify_all();
    }
}
