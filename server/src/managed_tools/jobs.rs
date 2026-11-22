//! Install jobs: one at a time across every tool and agent CLI, with their output kept for the
//! page that started them (and any page opened later).

use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

/// CDXC:ManagedTools 2026-09-29 WHY:
/// Tool installs, agent CLI installs and account helper installs share one lock: they write the same npm prefix, uv tool folder, shell profile and user PATH, and an agent install that needs Node waits for a Node install already running instead of starting a second one.
pub(crate) static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Where a job's progress lines go.
#[derive(Clone)]
pub(crate) struct Log(Arc<dyn Fn(String) + Send + Sync>);

impl Log {
    pub(crate) fn new(sink: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self(Arc::new(sink))
    }

    pub(crate) fn line(&self, text: &str) {
        (self.0)(format!("{text}\n"));
    }

    pub(crate) fn chunk(&self, text: String) {
        (self.0)(text);
    }

    /// A sink for the command runner, which takes an owned closure.
    pub(crate) fn sink(&self) -> impl Fn(String) + Clone + Send + 'static {
        let inner = self.0.clone();
        move |text| inner(text)
    }
}

#[derive(Clone)]
pub(crate) struct Job {
    pub id: String,
    pub operation: String,
    pub status: &'static str,
    pub output: String,
    pub error: Option<String>,
    pub finished_at: Option<String>,
}

impl Job {
    pub(crate) fn view(&self) -> Value {
        json!({
            "id": self.id,
            "operation": self.operation,
            "status": self.status,
            "output": self.output,
            "error": self.error,
            "finishedAt": self.finished_at,
        })
    }

    pub(crate) fn active(&self) -> bool {
        matches!(self.status, "queued" | "running")
    }
}

static JOBS: LazyLock<Mutex<HashMap<String, Job>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn get(key: &str) -> Option<Job> {
    JOBS.lock().ok()?.get(key).cloned()
}

/// Registers a queued job for `key`, or refuses when one is still running.
pub(crate) fn begin(key: &str, operation: &str) -> Result<String, String> {
    let mut jobs = JOBS.lock().map_err(|error| error.to_string())?;
    if jobs.get(key).is_some_and(Job::active) {
        return Err("This is already being installed or changed. Wait for it to finish.".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    jobs.insert(
        key.to_string(),
        Job {
            id: id.clone(),
            operation: operation.to_string(),
            status: "queued",
            output: String::new(),
            error: None,
            finished_at: None,
        },
    );
    Ok(id)
}

pub(crate) fn set_status(key: &str, status: &'static str) {
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(job) = jobs.get_mut(key) {
            job.status = status;
        }
    }
}

pub(crate) fn finish(key: &str, result: &Result<(), String>) {
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(job) = jobs.get_mut(key) {
            job.status = if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            };
            job.error = result.as_ref().err().cloned();
            job.finished_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }
}

pub(crate) fn log_for(key: &str) -> Log {
    let key = key.to_string();
    Log::new(move |chunk| append(&key, &chunk))
}

fn append(key: &str, chunk: &str) {
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(job) = jobs.get_mut(key) {
            job.output.push_str(chunk);
            if job.output.len() > 64 * 1024 {
                let mut start = job.output.len() - 64 * 1024;
                while !job.output.is_char_boundary(start) {
                    start += 1;
                }
                job.output.drain(..start);
            }
        }
    }
}
