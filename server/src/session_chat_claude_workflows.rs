//! CDXC:SessionStatus 2026-09-25 WHY:
//! A Claude dynamic workflow starts its agents without Agent tool calls. The parent records one `local_workflow` launch (task id plus run id) and each agent writes `subagents/workflows/<runId>/agent-<id>.jsonl` beside a `meta.json` holding its label. Claude's footer folds the run into one "4/6 agents done" row, so the Subagents card read nothing but the lead's own Agent children. Read the run folder instead; one run id can be launched again (a resumed workflow), and each launch gets a new task id.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::session_chat::{read_transcript_file_version, MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES};
use crate::session_chat_fleet_transcript::timestamp;

#[derive(Clone)]
pub(crate) struct WorkflowRun {
    pub name: String,
    /// The latest launch of this run id.
    pub launched_at: i64,
    /// Set by the terminal notification for the latest launch; a relaunch clears it.
    pub finished_at: Option<i64>,
}

#[derive(Clone, Default)]
pub(crate) struct WorkflowRuns {
    runs: HashMap<String, WorkflowRun>,
    run_by_task: HashMap<String, String>,
}

pub(crate) struct WorkflowAgent {
    pub id: String,
    pub path: PathBuf,
    pub run: WorkflowRun,
    pub label: Option<String>,
    pub launched_at: Option<i64>,
}

fn text(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

impl WorkflowRuns {
    /// Records the parent's `local_workflow` launch acknowledgement.
    pub(crate) fn launch(&mut self, result: &Value, at: i64) {
        if text(result.get("taskType")) != Some("local_workflow")
            || text(result.get("status")) != Some("async_launched")
        {
            return;
        }
        let (Some(task), Some(run)) = (text(result.get("taskId")), text(result.get("runId")))
        else {
            return;
        };
        if !safe_id(run) {
            return;
        }
        self.run_by_task.insert(task.to_string(), run.to_string());
        self.runs.insert(
            run.to_string(),
            WorkflowRun {
                name: text(result.get("workflowName")).unwrap_or(run).to_string(),
                launched_at: at,
                finished_at: None,
            },
        );
    }

    /// True when `task` is a workflow launch, whose terminal notification ends the run.
    pub(crate) fn finish(&mut self, task: &str, at: i64) -> bool {
        let Some(run) = self
            .run_by_task
            .get(task)
            .and_then(|run| self.runs.get_mut(run))
        else {
            return false;
        };
        // An older launch's late notification must not end a relaunch of the same run.
        if at >= run.launched_at {
            run.finished_at = Some(at);
        }
        true
    }

    /// The agents of every run that is still going. A finished run cannot list anything:
    /// its agents are over and the footer row that kept them visible is gone.
    pub(crate) fn running_agents(&self, subagents: &Path) -> io::Result<Vec<WorkflowAgent>> {
        let mut agents = Vec::new();
        for (run_id, run) in &self.runs {
            if run.finished_at.is_some() {
                continue;
            }
            let files = match fs::read_dir(subagents.join("workflows").join(run_id)) {
                Ok(files) => files,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            for entry in files {
                let path = entry?.path();
                let Some(id) = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix("agent-"))
                    .and_then(|name| name.strip_suffix(".jsonl"))
                    .filter(|id| safe_id(id))
                else {
                    continue;
                };
                let label = match File::open(path.with_extension("meta.json")) {
                    Ok(file) => {
                        let mut bytes = Vec::new();
                        file.take(64 * 1024).read_to_end(&mut bytes)?;
                        serde_json::from_slice::<Value>(&bytes)
                            .ok()
                            .and_then(|meta| text(meta.get("description")).map(str::to_string))
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e),
                };
                agents.push(WorkflowAgent {
                    id: id.to_string(),
                    launched_at: first_record_time(&path)?,
                    path,
                    run: run.clone(),
                    label,
                });
            }
        }
        Ok(agents)
    }
}

/// A workflow agent has no launch record in the parent; its transcript's first record is its start.
fn first_record_time(path: &Path) -> io::Result<Option<i64>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (String, i64)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let identity = read_transcript_file_version(path)?.identity;
    if let Some(at) = cache.lock().ok().and_then(|cache| {
        cache
            .get(path)
            .filter(|(known, _)| *known == identity)
            .map(|(_, at)| *at)
    }) {
        return Ok(Some(at));
    }
    let mut line = Vec::new();
    BufReader::new(File::open(path)?)
        .take(MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES as u64)
        .read_until(b'\n', &mut line)?;
    // A first record still being written has no time yet.
    let Some(at) = (line.last() == Some(&b'\n'))
        .then(|| serde_json::from_slice::<Value>(&line).ok())
        .flatten()
        .and_then(|record| timestamp(&record))
    else {
        return Ok(None);
    };
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (identity, at));
    }
    Ok(Some(at))
}

/// A workflow agent's transcript, which lives in its run's folder rather than beside the Agent children.
pub(crate) fn find_workflow_transcript(subagents: &Path, id: &str) -> io::Result<Option<PathBuf>> {
    if !safe_id(id) {
        return Ok(None);
    }
    let runs = match fs::read_dir(subagents.join("workflows")) {
        Ok(runs) => runs,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    for run in runs {
        let path = run?.path().join(format!("agent-{id}.jsonl"));
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
