use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::{Connection, OptionalExtension};
use serde_json::value::RawValue;
use serde_json::Value;

use crate::domain::{project_from_row, project_row_from_sql, DomainResult};

const MAX_DATABASES: usize = 4;

/// Every project, parsed, as `list_projects` returns them.
pub(crate) struct ProjectSet {
    pub(crate) projects: Vec<Value>,
    by_id: HashMap<String, usize>,
    list_result: OnceLock<Option<Box<RawValue>>>,
}

impl ProjectSet {
    pub(crate) fn get(&self, project_id: &str) -> Option<&Value> {
        self.by_id
            .get(project_id)
            .map(|index| &self.projects[*index])
    }

    /// `/api/listProjects`' result, `{"projects":[...]}`, as text, written once per set.
    ///
    /// CDXC:StateSync 2026-11-10 WHY: every listProjects answer deep copied the whole set twice
    /// (the clone out of this cache, then `json!` copying it into the result) before writing the
    /// same text again: about 260 of 3,605 busy samples of gxserver while phones polled. The set
    /// lives under one `projectsRevision`, so its text does too; the endpoint sends it as written.
    pub(crate) fn list_result(&self) -> Option<&RawValue> {
        self.list_result
            .get_or_init(|| {
                #[derive(serde::Serialize)]
                struct ListResult<'a> {
                    projects: &'a [Value],
                }
                serde_json::value::to_raw_value(&ListResult {
                    projects: &self.projects,
                })
                .ok()
            })
            .as_deref()
    }
}

struct CachedProjects {
    database: String,
    revision: String,
    set: Arc<ProjectSet>,
}

fn cache() -> &'static Mutex<VecDeque<CachedProjects>> {
    static CACHE: OnceLock<Mutex<VecDeque<CachedProjects>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// The parsed projects of `db` while no project row changed since they were read.
///
/// CDXC:StateSync 2026-11-08 WHY:
/// The projects table changes a few times a day, but the coordinator tick, settings sync, the
/// session links and git passes and the lifecycle sweeps each parsed all of it (318 projects,
/// about 830 KB of JSON columns on the profiled Mac, mostly their custom agents), or one project
/// per session: about 2.5% of a core with nothing happening. `projectsRevision` moves with every
/// project write (migration 0063's triggers), so the parsed set is kept under it, read in the same
/// SQLite snapshot as the revision. A caller inside a transaction may see its own uncommitted
/// writes, so it reads as before and never fills the cache.
///
/// With `build` false a stale or missing set is not rebuilt: a single-project read is one row,
/// and a loop that writes projects and reads each back must not parse every project each time.
/// `None` means read the rows directly (no file-backed database, a transaction, a registry
/// without the revision, or a row that does not parse, whose error the direct read reports).
pub(crate) fn cached_projects(db: &Connection, build: bool) -> Option<Arc<ProjectSet>> {
    let database = db.path().filter(|path| !path.is_empty())?;
    if !db.is_autocommit() {
        return None;
    }
    let transaction = db.unchecked_transaction().ok()?;
    let revision: String = transaction
        .query_row(
            "SELECT value FROM metadata WHERE key = 'projectsRevision'",
            [],
            |row| row.get(0),
        )
        .optional()
        .ok()??;
    {
        let cache = cache().lock().ok()?;
        if let Some(entry) = cache
            .iter()
            .find(|entry| entry.database == database && entry.revision == revision)
        {
            return Some(entry.set.clone());
        }
    }
    if !build {
        return None;
    }
    let rows = {
        let mut statement = transaction
            .prepare_cached("SELECT * FROM projects ORDER BY updatedAt DESC, projectId ASC")
            .ok()?;
        let rows = statement
            .query_map([], project_row_from_sql)
            .ok()?
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        rows
    };
    transaction.commit().ok()?;
    let projects = rows
        .into_iter()
        .map(project_from_row)
        .collect::<DomainResult<Vec<_>>>()
        .ok()?;
    let by_id = projects
        .iter()
        .enumerate()
        .filter_map(|(index, project)| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(|project_id| (project_id.to_string(), index))
        })
        .collect();
    let set = Arc::new(ProjectSet {
        projects,
        by_id,
        list_result: OnceLock::new(),
    });
    if let Ok(mut cache) = cache().lock() {
        cache.retain(|entry| entry.database != database);
        if cache.len() >= MAX_DATABASES {
            cache.pop_front();
        }
        cache.push_back(CachedProjects {
            database: database.to_string(),
            revision,
            set: set.clone(),
        });
    }
    Some(set)
}

/// `/api/listProjects`' result: the cached set's text when the cache holds the set, else the
/// projects just read. Both serialize to the same `{"projects":[...]}`.
pub(crate) enum ListProjectsResult {
    Cached(Arc<ProjectSet>),
    Read(Vec<Value>),
}

impl serde::Serialize for ListProjectsResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(serde::Serialize)]
        struct ListResult<'a> {
            projects: &'a [Value],
        }
        match self {
            Self::Cached(set) => match set.list_result() {
                Some(text) => text.serialize(serializer),
                None => ListResult {
                    projects: &set.projects,
                }
                .serialize(serializer),
            },
            Self::Read(projects) => ListResult { projects }.serialize(serializer),
        }
    }
}

/// A cached project as the `*_without_session_history` readers return it.
pub(crate) fn without_session_history(project: &Value) -> Value {
    match project {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(key, _)| key.as_str() != "previousSessionHistory")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}
