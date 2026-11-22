//! A usage-only sign-in in progress: the job Settings polls, what it shows (the page to open, the
//! code to type, a box for a pasted code), and the provider's sign-in running on its own thread.
//!
//! CDXC:AgentProviders 2026-11-05 WHY:
//! The client opens the page the job names (`url`), not gxserver, so a sign-in started from a
//! window opens there. The job stays until the window acknowledges it or a newer one replaces it.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::http::Failure;
use super::store::{self, Credential, UsageAccount};
use super::UsageProvider;
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// What a finished sign-in returns: the account it signed in to.
pub(crate) struct SignedIn {
    pub identity: String,
    pub name: String,
    pub email: String,
    pub credential: Credential,
}

struct Job {
    view: Value,
    cancelled: bool,
    input: Option<String>,
}

type Shared = Arc<(Mutex<Job>, Condvar)>;

fn jobs() -> &'static Mutex<HashMap<String, Shared>> {
    static JOBS: std::sync::OnceLock<Mutex<HashMap<String, Shared>>> = std::sync::OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The running sign-in's handle on its job.
pub(crate) struct Progress {
    job: Shared,
}

impl Progress {
    fn change(&self, apply: impl FnOnce(&mut Value)) {
        let (job, _) = &*self.job;
        let mut job = job.lock().unwrap_or_else(|e| e.into_inner());
        apply(&mut job.view);
    }
    /// The page to sign in on; `accepts_code` shows a box for a code the browser may show.
    pub(crate) fn show_url(&self, url: &str, accepts_code: bool) {
        self.change(|view| {
            view["status"] = json!("waiting");
            view["url"] = json!(url);
            view["acceptsCode"] = json!(accepts_code);
        });
    }
    /// A device code to type on the provider's page.
    pub(crate) fn show_code(&self, url: &str, code: &str) {
        self.change(|view| {
            view["status"] = json!("waiting");
            view["url"] = json!(url);
            view["userCode"] = json!(code);
        });
    }
    pub(crate) fn note(&self, text: &str) {
        self.change(|view| view["note"] = json!(text));
    }
    pub(crate) fn saving(&self) {
        self.change(|view| {
            view["status"] = json!("saving");
            view["note"] = Value::Null;
        });
    }
    /// Waits up to `for_how_long`, waking early for a cancel or a pasted code; `true` once cancelled.
    pub(crate) fn wait(&self, for_how_long: Duration) -> bool {
        let (job, wake) = &*self.job;
        let job = job.lock().unwrap_or_else(|e| e.into_inner());
        if job.cancelled || job.input.is_some() {
            return job.cancelled;
        }
        let (job, _) = wake
            .wait_timeout(job, for_how_long)
            .unwrap_or_else(|e| e.into_inner());
        job.cancelled
    }
    pub(crate) fn take_input(&self) -> Option<String> {
        let (job, _) = &*self.job;
        job.lock().unwrap_or_else(|e| e.into_inner()).input.take()
    }
}

/// Starts a sign-in for `provider`; `account_id` signs an account already saved in again.
pub(crate) fn start(
    paths: &GxserverPaths,
    provider: UsageProvider,
    owner: &str,
    account_id: Option<String>,
) -> Result<Value, DomainStateError> {
    let id = uuid::Uuid::new_v4().to_string();
    let view = json!({
        "id": id, "owner": owner, "provider": provider, "status": "starting",
        "accountId": account_id, "createdAt": chrono::Utc::now().timestamp_millis(),
    });
    let job: Shared = Arc::new((
        Mutex::new(Job {
            view: view.clone(),
            cancelled: false,
            input: None,
        }),
        Condvar::new(),
    ));
    {
        let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
        // One sign-in per window and provider: a new one replaces the last.
        all.retain(|_, other| {
            let (other, wake) = &**other;
            let mut other = other.lock().unwrap_or_else(|e| e.into_inner());
            let replaced =
                other.view["owner"] == owner && other.view["provider"] == json!(provider);
            if replaced {
                other.cancelled = true;
                wake.notify_all();
            }
            !replaced
        });
        all.insert(id.clone(), job.clone());
    }
    let paths = paths.clone();
    std::thread::spawn(move || {
        let progress = Progress { job: job.clone() };
        let result = match provider {
            UsageProvider::Cursor => super::cursor::sign_in(&progress),
            UsageProvider::Gemini => super::gemini::sign_in(&progress),
            UsageProvider::Copilot => super::copilot::sign_in(&progress),
        }
        .and_then(|signed_in| save(&paths, provider, account_id, signed_in));
        progress.change(|view| match result {
            Ok(id) => {
                view["status"] = json!("complete");
                view["accountId"] = json!(id);
                view["url"] = Value::Null;
                view["userCode"] = Value::Null;
            }
            Err(failure) => {
                view["status"] = json!("failed");
                view["error"] = json!(failure.message());
            }
        });
    });
    Ok(view)
}

/// Saves a finished sign-in. A new account starts shown in the sidebar (CDXC:AgentProviders
/// 2026-11-03 on `new_saved_account`); signing an account in again keeps its name and settings and
/// refuses another account.
fn save(
    paths: &GxserverPaths,
    provider: UsageProvider,
    again: Option<String>,
    signed_in: SignedIn,
) -> Result<String, Failure> {
    let _lock = store::lock();
    let mut accounts = store::read(paths).map_err(|e| Failure::Unavailable(e.to_string()))?;
    let id = super::account_id(provider, &signed_in.identity);
    if let Some(again) = again {
        if again != id {
            return Err(Failure::Unavailable(
                "That's a different account. Sign in to this one, or use Add account.".into(),
            ));
        }
    }
    match accounts.iter_mut().find(|account| account.id == id) {
        Some(account) => {
            account.credential = signed_in.credential;
            if !signed_in.email.is_empty() {
                account.email = signed_in.email;
            }
        }
        None => accounts.push(UsageAccount {
            id: id.clone(),
            provider,
            identity: signed_in.identity,
            name: signed_in.name,
            email: signed_in.email,
            show_in_titlebar: true,
            user_disabled: false,
            credential: signed_in.credential,
        }),
    }
    store::write(paths, &accounts).map_err(|e| Failure::Unavailable(e.to_string()))?;
    super::forget_reading(&id);
    Ok(id)
}

fn find(id: &str, owner: &str) -> Result<Shared, DomainStateError> {
    let all = jobs().lock().unwrap_or_else(|e| e.into_inner());
    let job = all
        .get(id)
        .cloned()
        .ok_or_else(|| DomainStateError::not_found("Sign-in not found."))?;
    if job.0.lock().unwrap_or_else(|e| e.into_inner()).view["owner"] != owner {
        return Err(DomainStateError::bad_request(
            "This sign-in belongs to another window.",
        ));
    }
    Ok(job)
}

/// `usageSignInStatus`, `usageSignInInput`, `usageSignInCancel` and `usageSignInAcknowledge`.
pub(crate) fn operate(
    operation: &str,
    id: &str,
    owner: &str,
    input: Option<&str>,
) -> Result<Value, DomainStateError> {
    let shared = find(id, owner)?;
    let (job, wake) = &*shared;
    let mut job = job.lock().unwrap_or_else(|e| e.into_inner());
    match operation {
        "usageSignInInput" => {
            job.input = input
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string);
            wake.notify_all();
        }
        "usageSignInCancel" => {
            job.cancelled = true;
            wake.notify_all();
        }
        "usageSignInAcknowledge" => {
            let view = job.view.clone();
            drop(job);
            jobs().lock().unwrap_or_else(|e| e.into_inner()).remove(id);
            return Ok(view);
        }
        _ => {}
    }
    Ok(job.view.clone())
}
