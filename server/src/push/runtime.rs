use std::{
    collections::VecDeque,
    sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::JoinHandle,
    time::Duration,
};

use serde_json::json;

use crate::{
    domain::DomainRepository,
    logging::{GxserverLogInput, GxserverLogger, LogLevel},
    paths::GxserverPaths,
    session_approval::session_has_question,
    storage::open_gxserver_database,
};

use super::{
    expo::{ExpoClient, ReceiptState, ReceiptsOutcome, SendOutcome},
    limiter::prune_budgets,
    message::{
        alert_message, silent_message, AgentStartEvent, AlertEvent, AlertKind, ALERT_TTL_MS,
        SILENT_TTL_MS,
    },
    store::{token_fingerprint, PendingReceipt, PushRegistrationStore, PushStoreData},
};

/// Alert jobs (one per event and phone) waiting to be sent, retried or fanned out.
pub(crate) const ALERT_QUEUE_CAPACITY: usize = 64;
/// The waits before the first and the second retry; there is no third.
const RETRY_DELAYS_MS: [i64; 2] = [1_000, 2_000];
pub(crate) const RECEIPT_DELAY_MS: i64 = 15 * 60_000;
pub(crate) const RECEIPT_KEEP_MS: i64 = 24 * 3_600_000;
const RECEIPT_BATCH: usize = 100;

/// Wall-clock milliseconds; tests drive their own.
pub(crate) trait PushClock: Send + Sync {
    fn now_ms(&self) -> i64;
}

pub(crate) struct SystemClock;

impl PushClock for SystemClock {
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

/// One alert for one phone. The phone is named by its computer id and the fingerprint of the
/// token the job was made for, so a rotation or an unregistration ends it.
struct AlertJob {
    event: Arc<AlertEvent>,
    machine_id: String,
    token_hash: String,
    sends: usize,
    not_before: i64,
    expires_at: i64,
}

struct SilentJob {
    session_id: String,
    machine_id: String,
    token_hash: String,
    sends: usize,
    not_before: i64,
    expires_at: i64,
}

#[derive(Default)]
struct Queue {
    events: VecDeque<AlertEvent>,
    start: Option<AgentStartEvent>,
    jobs: Vec<AlertJob>,
    silent_jobs: Vec<SilentJob>,
    stopping: bool,
    poked: bool,
    idle: bool,
    overflowing: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    idle_changed: Condvar,
    paths: GxserverPaths,
    clock: Arc<dyn PushClock>,
    client: ExpoClient,
    logger: Arc<GxserverLogger>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn warn(&self, event: &str, code: &str) {
        let _ = self.logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: event.to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({ "code": code })),
        });
    }
}

/// CDXC:Notifications 2026-10-31 WHY:
/// One worker thread sends every push, so the hook that committed a feed row only queues it and never waits on the network. Alerts go before start messages, at most 64 alert jobs wait (a full queue drops the newest with one warning per overflow), and start messages coalesce into one pending wake per phone, so they never take an alert's place.
/// Each send checks the phone again first: an unregistered or rotated token, a switched-off kind or a spent budget ends the job. A message expires an hour (alerts) or a minute (start messages) after its event; up to two retries on rate limits, server errors or no answer wait 1 s and then 2 s, or longer if Expo or the phone's budget asks, never past that expiry, and resend the same feed row id. A request already handed to Expo cannot be taken back.
/// Receipts are read about 15 minutes after a send and kept 24 hours in the private store, so they are checked after a restart too; a dead token is deleted only while it is still the phone's current one. When the store cannot be read or written during a receipt reading, the next one waits another 15 minutes instead of asking Expo again at once. A stop drops the queued messages (the feed keeps the rows, and nothing is replayed later) and waits at most for the request in flight, which times out after 5 s.
pub(crate) struct PushRuntime {
    shared: Arc<Shared>,
    worker: Mutex<Option<(JoinHandle<()>, mpsc::Receiver<()>)>>,
}

impl PushRuntime {
    pub(crate) fn start(paths: GxserverPaths, logger: Arc<GxserverLogger>) -> Self {
        Self::start_with(paths, logger, Arc::new(SystemClock), ExpoClient::new())
    }

    pub(crate) fn start_with(
        paths: GxserverPaths,
        logger: Arc<GxserverLogger>,
        clock: Arc<dyn PushClock>,
        client: ExpoClient,
    ) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            idle_changed: Condvar::new(),
            paths,
            clock,
            client,
            logger,
        });
        let (done_tx, done_rx) = mpsc::channel();
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("gxserver-push".into())
            .spawn(move || {
                run(&worker_shared);
                let _ = done_tx.send(());
            })
            .ok()
            .map(|handle| (handle, done_rx));
        Self {
            shared,
            worker: Mutex::new(worker),
        }
    }

    /// Queues a committed feed row for every phone whose switch for its kind is on.
    pub(crate) fn enqueue_alert(&self, event: AlertEvent) {
        if !event.is_valid() {
            self.shared.warn("pushEventRejected", "invalidIds");
            return;
        }
        let mut queue = self.shared.lock();
        if queue.stopping {
            return;
        }
        if queue.events.len() + queue.jobs.len() >= ALERT_QUEUE_CAPACITY {
            note_overflow(&self.shared, &mut queue);
            return;
        }
        queue.events.push_back(event);
        poke(&self.shared, &mut queue);
    }

    /// Queues the background-only start message; a newer start replaces one not yet fanned out.
    pub(crate) fn enqueue_agent_start(&self, event: AgentStartEvent) {
        if !super::registration::is_printable_id(
            &event.session_id,
            super::registration::SESSION_ID_MAX,
        ) {
            self.shared.warn("pushEventRejected", "invalidIds");
            return;
        }
        let mut queue = self.shared.lock();
        if queue.stopping {
            return;
        }
        queue.start = Some(event);
        poke(&self.shared, &mut queue);
    }

    /// Drops everything queued and waits up to `wait` for the worker to finish its request in
    /// flight. Returns whether it finished.
    pub(crate) fn stop(&self, wait: Duration) -> bool {
        {
            let mut queue = self.shared.lock();
            queue.stopping = true;
            queue.events.clear();
            queue.start = None;
            queue.jobs.clear();
            queue.silent_jobs.clear();
            self.shared.wake.notify_all();
        }
        let Some((handle, done)) = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        else {
            return true;
        };
        match done.recv_timeout(wait) {
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            _ => {
                let _ = handle.join();
                true
            }
        }
    }

    /// Makes the worker look at the clock again.
    #[cfg(test)]
    pub(crate) fn wake(&self) {
        let mut queue = self.shared.lock();
        poke(&self.shared, &mut queue);
    }

    /// Waits until the worker has nothing due at the current time.
    #[cfg(test)]
    pub(crate) fn wait_until_idle(&self, timeout: Duration) -> bool {
        let queue = self.shared.lock();
        let (queue, result) = self
            .shared
            .idle_changed
            .wait_timeout_while(queue, timeout, |queue| !queue.idle)
            .unwrap_or_else(PoisonError::into_inner);
        drop(queue);
        !result.timed_out()
    }
}

impl Drop for PushRuntime {
    fn drop(&mut self) {
        let mut queue = self.shared.lock();
        queue.stopping = true;
        self.shared.wake.notify_all();
    }
}

fn poke(shared: &Shared, queue: &mut Queue) {
    queue.poked = true;
    queue.idle = false;
    shared.wake.notify_all();
}

fn note_overflow(shared: &Shared, queue: &mut Queue) {
    if !queue.overflowing {
        queue.overflowing = true;
        shared.warn("pushQueueFull", "alertDropped");
    }
}

fn run(shared: &Shared) {
    let mut next_receipt_due = PushRegistrationStore::open(&shared.paths)
        .read()
        .ok()
        .and_then(|data| earliest_receipt(&data));
    loop {
        let (events, start) = {
            let mut queue = shared.lock();
            loop {
                if queue.stopping {
                    return;
                }
                let now = shared.clock.now_ms();
                let due = queue.poked
                    || !queue.events.is_empty()
                    || queue.start.is_some()
                    || queue.jobs.iter().any(|job| job.not_before <= now)
                    || queue.silent_jobs.iter().any(|job| job.not_before <= now)
                    || next_receipt_due.is_some_and(|due| due <= now);
                if due {
                    queue.poked = false;
                    queue.idle = false;
                    break;
                }
                let next = queue
                    .jobs
                    .iter()
                    .map(|job| job.not_before)
                    .chain(queue.silent_jobs.iter().map(|job| job.not_before))
                    .chain(next_receipt_due)
                    .min();
                queue.idle = true;
                shared.idle_changed.notify_all();
                queue = match next {
                    Some(at) => {
                        let wait = Duration::from_millis((at - now).max(1) as u64);
                        shared
                            .wake
                            .wait_timeout(queue, wait)
                            .unwrap_or_else(PoisonError::into_inner)
                            .0
                    }
                    None => shared
                        .wake
                        .wait(queue)
                        .unwrap_or_else(PoisonError::into_inner),
                };
            }
            (std::mem::take(&mut queue.events), queue.start.take())
        };
        if !events.is_empty() || start.is_some() {
            fan_out(shared, events, start);
        }
        while let Some(job) = take_due_alert(shared) {
            send_alert(shared, job, &mut next_receipt_due);
        }
        while let Some(job) = take_due_silent(shared) {
            send_silent(shared, job, &mut next_receipt_due);
        }
        if next_receipt_due.is_some_and(|due| due <= shared.clock.now_ms()) {
            read_receipts(shared, &mut next_receipt_due);
        }
    }
}

/// Turns events into one job per phone that wants them.
fn fan_out(shared: &Shared, events: VecDeque<AlertEvent>, start: Option<AgentStartEvent>) {
    let now = shared.clock.now_ms();
    let data = match PushRegistrationStore::open(&shared.paths).read() {
        Ok(data) => data,
        Err(_) => {
            shared.warn("pushStoreUnreadable", "fanOut");
            return;
        }
    };
    let mut jobs = Vec::new();
    for event in events {
        let expires_at = event.occurred_at_ms.min(now) + ALERT_TTL_MS;
        if expires_at <= now {
            continue;
        }
        let event = Arc::new(event);
        for registration in &data.registrations {
            if event.kind.enabled(&registration.kinds) {
                jobs.push(AlertJob {
                    event: event.clone(),
                    machine_id: registration.machine_id.clone(),
                    token_hash: token_fingerprint(&registration.token),
                    sends: 0,
                    not_before: now,
                    expires_at,
                });
            }
        }
    }
    let silent_jobs: Vec<SilentJob> = start
        .map(|start| {
            let expires_at = start.occurred_at_ms.min(now) + SILENT_TTL_MS;
            data.registrations
                .iter()
                .filter(|_| expires_at > now)
                .map(|registration| SilentJob {
                    session_id: start.session_id.clone(),
                    machine_id: registration.machine_id.clone(),
                    token_hash: token_fingerprint(&registration.token),
                    sends: 0,
                    not_before: now,
                    expires_at,
                })
                .collect()
        })
        .unwrap_or_default();
    let mut queue = shared.lock();
    if queue.stopping {
        return;
    }
    for job in jobs {
        if queue.events.len() + queue.jobs.len() >= ALERT_QUEUE_CAPACITY {
            note_overflow(shared, &mut queue);
            break;
        }
        queue.jobs.push(job);
    }
    for job in silent_jobs {
        queue
            .silent_jobs
            .retain(|pending| pending.machine_id != job.machine_id);
        queue.silent_jobs.push(job);
    }
}

fn take_due_alert(shared: &Shared) -> Option<AlertJob> {
    let mut queue = shared.lock();
    let now = shared.clock.now_ms();
    if queue.stopping {
        return None;
    }
    if queue.jobs.is_empty() && queue.events.is_empty() {
        queue.overflowing = false;
    }
    let index = queue.jobs.iter().position(|job| job.not_before <= now)?;
    Some(queue.jobs.remove(index))
}

fn take_due_silent(shared: &Shared) -> Option<SilentJob> {
    let mut queue = shared.lock();
    let now = shared.clock.now_ms();
    if queue.stopping {
        return None;
    }
    let index = queue
        .silent_jobs
        .iter()
        .position(|job| job.not_before <= now)?;
    Some(queue.silent_jobs.remove(index))
}

/// The phone's current token when the job still applies to it, after charging its budget.
enum Admission {
    Send(String),
    Later(i64),
    Drop,
}

fn send_alert(shared: &Shared, mut job: AlertJob, next_receipt_due: &mut Option<i64>) {
    let now = shared.clock.now_ms();
    if now >= job.expires_at {
        return;
    }
    let admission = charge_budget(shared, &job.machine_id, &job.token_hash, |data| {
        let registration = data.registration(&job.machine_id)?;
        if !job.event.kind.enabled(&registration.kinds) {
            return None;
        }
        let token = registration.token.clone();
        let first = job.sends == 0;
        let budget = data.budgets.entry(job.machine_id.clone()).or_default();
        if first && !budget.session_free(&job.event.session_id, now) {
            return None;
        }
        let slot = budget.alert_slot_at(now);
        if slot > now {
            return Some(if first || slot >= job.expires_at {
                Admission::Drop
            } else {
                Admission::Later(slot)
            });
        }
        budget.record_alert(now, first.then_some(job.event.session_id.as_str()));
        Some(Admission::Send(token))
    });
    let token = match admission {
        Admission::Send(token) => token,
        Admission::Later(at) => {
            job.not_before = at;
            requeue(shared, |queue| queue.jobs.push(job));
            return;
        }
        Admission::Drop => return,
    };
    let ttl_secs = remaining_secs(job.expires_at, now);
    let has_question = alert_has_question(shared, &job.event);
    let Some(message) = alert_message(&token, &job.machine_id, &job.event, has_question, ttl_secs)
    else {
        shared.warn("pushMessageTooLarge", "alert");
        return;
    };
    drop(token);
    let outcome = shared.client.send(&message);
    drop(message);
    job.sends += 1;
    if let Some(at) = after_send(
        shared,
        outcome,
        &job.machine_id,
        &job.token_hash,
        job.sends,
        job.expires_at,
        next_receipt_due,
    ) {
        job.not_before = at;
        requeue(shared, |queue| queue.jobs.push(job));
    }
}

fn alert_has_question(shared: &Shared, event: &AlertEvent) -> bool {
    if event.kind != AlertKind::NeedsInput {
        return false;
    }
    let session = open_gxserver_database(&shared.paths).ok().and_then(|db| {
        DomainRepository::new(&db, "")
            .get_session(&event.project_id, &event.session_id)
            .ok()
    });
    match session {
        Some(session) => session.is_some_and(|session| session_has_question(&session)),
        None => {
            shared.warn("pushSessionUnreadable", "alert");
            false
        }
    }
}

fn send_silent(shared: &Shared, mut job: SilentJob, next_receipt_due: &mut Option<i64>) {
    let now = shared.clock.now_ms();
    if now >= job.expires_at {
        return;
    }
    let admission = charge_budget(shared, &job.machine_id, &job.token_hash, |data| {
        let token = data.registration(&job.machine_id)?.token.clone();
        let budget = data.budgets.entry(job.machine_id.clone()).or_default();
        if !budget.silent_slot_free(now) {
            return Some(Admission::Drop);
        }
        budget.record_silent(now);
        Some(Admission::Send(token))
    });
    let Admission::Send(token) = admission else {
        return;
    };
    let ttl_secs = remaining_secs(job.expires_at, now);
    let Some(message) = silent_message(&token, &job.machine_id, &job.session_id, ttl_secs) else {
        shared.warn("pushMessageTooLarge", "start");
        return;
    };
    drop(token);
    let outcome = shared.client.send(&message);
    drop(message);
    job.sends += 1;
    if let Some(at) = after_send(
        shared,
        outcome,
        &job.machine_id,
        &job.token_hash,
        job.sends,
        job.expires_at,
        next_receipt_due,
    ) {
        job.not_before = at;
        requeue(shared, |queue| {
            if !queue
                .silent_jobs
                .iter()
                .any(|pending| pending.machine_id == job.machine_id)
            {
                queue.silent_jobs.push(job);
            }
        });
    }
}

/// Checks the phone's registration against the job under the store lock and lets `decide`
/// charge its budget. The lock is released before any request.
fn charge_budget(
    shared: &Shared,
    machine_id: &str,
    token_hash: &str,
    decide: impl FnOnce(&mut PushStoreData) -> Option<Admission>,
) -> Admission {
    let now = shared.clock.now_ms();
    let store = PushRegistrationStore::open(&shared.paths);
    let mut data = match store.read() {
        Ok(data) => data,
        Err(_) => {
            shared.warn("pushStoreUnreadable", "send");
            return Admission::Drop;
        }
    };
    let current = data
        .registration(machine_id)
        .is_some_and(|registration| token_fingerprint(&registration.token) == token_hash);
    if !current {
        return Admission::Drop;
    }
    prune_budgets(&mut data.budgets, now);
    let admission = decide(&mut data).unwrap_or(Admission::Drop);
    if matches!(admission, Admission::Send(_)) && store.write(&data).is_err() {
        shared.warn("pushStoreUnwritable", "send");
        return Admission::Drop;
    }
    admission
}

/// Records the ticket or the dead token; returns when to retry, if at all.
fn after_send(
    shared: &Shared,
    outcome: SendOutcome,
    machine_id: &str,
    token_hash: &str,
    sends: usize,
    expires_at: i64,
    next_receipt_due: &mut Option<i64>,
) -> Option<i64> {
    let now = shared.clock.now_ms();
    match outcome {
        SendOutcome::Accepted(id) => {
            let receipt = PendingReceipt {
                id,
                machine_id: machine_id.to_string(),
                token_hash: token_hash.to_string(),
                due_at: now + RECEIPT_DELAY_MS,
                expires_at: now + RECEIPT_KEEP_MS,
            };
            let due_at = receipt.due_at;
            if update_store(shared, |data| {
                data.receipts.push(receipt);
                true
            }) {
                *next_receipt_due = Some(next_receipt_due.map_or(due_at, |due| due.min(due_at)));
            }
            None
        }
        SendOutcome::DeviceNotRegistered => {
            update_store(shared, |data| data.remove_if_token(machine_id, token_hash));
            None
        }
        SendOutcome::Retry { after, code } => {
            let Some(delay) = RETRY_DELAYS_MS.get(sends - 1) else {
                shared.warn("pushSendGaveUp", &code);
                return None;
            };
            let after_ms = after.map_or(0, |after| after.as_millis() as i64);
            let at = now + (*delay).max(after_ms);
            if at >= expires_at {
                shared.warn("pushSendExpired", &code);
                return None;
            }
            Some(at)
        }
        SendOutcome::Failed(code) => {
            shared.warn("pushSendFailed", &code);
            None
        }
    }
}

fn read_receipts(shared: &Shared, next_receipt_due: &mut Option<i64>) {
    let now = shared.clock.now_ms();
    let mut due_ids: Vec<String> = Vec::new();
    let mut stored = update_store(shared, |data| {
        let before = data.receipts.len();
        data.receipts.retain(|receipt| receipt.expires_at > now);
        due_ids = data
            .receipts
            .iter()
            .filter(|receipt| receipt.due_at <= now)
            .take(RECEIPT_BATCH)
            .map(|receipt| receipt.id.clone())
            .collect();
        data.receipts.len() != before
    });
    if !due_ids.is_empty() {
        let ids: Vec<&str> = due_ids.iter().map(String::as_str).collect();
        let outcome = shared.client.receipts(&ids);
        let now = shared.clock.now_ms();
        stored &= update_store(shared, |data| {
            settle_receipts(shared, data, &due_ids, outcome, now);
            true
        });
    }
    let reloaded = if stored {
        PushRegistrationStore::open(&shared.paths).read().ok()
    } else {
        None
    };
    *next_receipt_due = match reloaded {
        Some(data) => earliest_receipt(&data),
        // The store could not be read or written, so it still holds the due times this reading
        // could not settle; reading them back would ask Expo again at once, in a loop.
        None => Some(shared.clock.now_ms() + RECEIPT_DELAY_MS),
    };
}

fn settle_receipts(
    shared: &Shared,
    data: &mut PushStoreData,
    due_ids: &[String],
    outcome: ReceiptsOutcome,
    now: i64,
) {
    let mut dead: Vec<(String, String)> = Vec::new();
    match outcome {
        ReceiptsOutcome::Read(states) => data.receipts.retain_mut(|receipt| {
            if !due_ids.contains(&receipt.id) {
                return true;
            }
            match states.get(&receipt.id) {
                Some(ReceiptState::Delivered) => false,
                Some(ReceiptState::DeviceNotRegistered) => {
                    dead.push((receipt.machine_id.clone(), receipt.token_hash.clone()));
                    false
                }
                Some(ReceiptState::Failed(code)) => {
                    shared.warn("pushReceiptFailed", code);
                    false
                }
                None => {
                    receipt.due_at = now + RECEIPT_DELAY_MS;
                    receipt.due_at < receipt.expires_at
                }
            }
        }),
        ReceiptsOutcome::Retry(code) => {
            shared.warn("pushReceiptsUnavailable", &code);
            data.receipts.retain_mut(|receipt| {
                if due_ids.contains(&receipt.id) {
                    receipt.due_at = now + RECEIPT_DELAY_MS;
                }
                receipt.due_at < receipt.expires_at
            });
        }
        ReceiptsOutcome::Failed(code) => {
            shared.warn("pushReceiptsFailed", &code);
            data.receipts
                .retain(|receipt| !due_ids.contains(&receipt.id));
        }
    }
    for (machine_id, token_hash) in dead {
        data.remove_if_token(&machine_id, &token_hash);
    }
}

/// Reads, changes and (when `change` says so) writes the store; false when it could not.
fn update_store(shared: &Shared, change: impl FnOnce(&mut PushStoreData) -> bool) -> bool {
    let store = PushRegistrationStore::open(&shared.paths);
    let Ok(mut data) = store.read() else {
        shared.warn("pushStoreUnreadable", "update");
        return false;
    };
    if change(&mut data) && store.write(&data).is_err() {
        shared.warn("pushStoreUnwritable", "update");
        return false;
    }
    true
}

fn requeue(shared: &Shared, put_back: impl FnOnce(&mut Queue)) {
    let mut queue = shared.lock();
    if !queue.stopping {
        put_back(&mut queue);
    }
}

fn earliest_receipt(data: &PushStoreData) -> Option<i64> {
    data.receipts.iter().map(|receipt| receipt.due_at).min()
}

/// Whole seconds left until `expires_at`, rounded up so a message is never sent with ttl 0.
fn remaining_secs(expires_at: i64, now: i64) -> i64 {
    (expires_at - now + 999) / 1000
}
