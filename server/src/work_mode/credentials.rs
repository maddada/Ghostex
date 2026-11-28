//! The private credentials file: the Linear API keys work mode reads Linear with (one shared key,
//! the default workspace's, an optional key per workspace, and an optional key per project that
//! overrides both), and crate::team_sync's connections.
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! gxserver has no OS credential store it can use on every platform (its own auth token is a
//! private file too), so the keys live in one private file next to that token: the auth folder is
//! 0700 and the file 0600 on macOS and Linux. They are never written to the settings file, never
//! published, and never returned by any endpoint.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: the Linear key is set once and shared by the projects, and a single project can override
//! it in its own settings.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;

const CREDENTIALS_FILE_NAME: &str = "work-mode-credentials.json";
/// Where the retired ticket commands kept their credentials. The next write of the file drops it,
/// so no secret of a feature that is gone stays on disk.
///
/// CDXC:WorkMode 2026-11-28 DECISION:
/// The repository owner, on custom ticket commands: "Simplify/remove all that work we did for custom ticket commands please it was bad and we still didn't use it for anything." Their saved setup is ignored (a workspace whose tracker was a command goes back to the default tracker), and their credentials are dropped here rather than kept unread.
const RETIRED_SOURCE_CREDENTIALS_KEY: &str = "workspaceSourceCredentials";
/// `teamSync.<workspaceId>`: crate::team_sync's connections.
pub(crate) const TEAM_CONNECTIONS_KEY: &str = "teamSync";

fn credentials_path(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join(CREDENTIALS_FILE_NAME)
}

/// The whole private credentials file; crate::team_sync keeps its connections in it too.
pub(crate) fn read_credentials(paths: &GxserverPaths) -> Map<String, Value> {
    fs::read_to_string(credentials_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

/// The whole file, read for a change: a missing file is empty, and a file that is there but can't
/// be read, isn't a JSON object, or holds a container a change writes into that isn't one
/// (`misshapen_container`), is an error, so a change never writes over what it couldn't read
/// (the Linear keys and team connections in it).
pub(crate) fn read_credentials_strictly(
    paths: &GxserverPaths,
) -> std::io::Result<Map<String, Value>> {
    let unreadable = |detail: String| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{CREDENTIALS_FILE_NAME} can't be read ({detail}); fix it or remove it, and try again"),
        )
    };
    let text = match fs::read_to_string(credentials_path(paths)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(unreadable(error.to_string())),
    };
    let file = match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(file)) => file,
        Ok(_) => return Err(unreadable("it isn't a JSON object".to_string())),
        Err(error) => return Err(unreadable(error.to_string())),
    };
    match misshapen_container(&file) {
        Some(problem) => Err(unreadable(problem)),
        None => Ok(file),
    }
}

/// What is wrong with the first container a change writes into that isn't a JSON object: a change
/// would take it for empty and replace it. It names the container's level in fixed words, never a
/// key read from the file, which in a damaged file can be any text, a credential among them. Each
/// team connection is replaced whole, so only `teamSync` itself is checked; keys Ghostex doesn't
/// know are left alone.
fn misshapen_container(file: &Map<String, Value>) -> Option<String> {
    let maps = [
        "projectLinearApiKeys",
        "workspaceLinearApiKeys",
        TEAM_CONNECTIONS_KEY,
    ];
    maps.into_iter()
        .find(|key| file.get(*key).is_some_and(|value| !value.is_object()))
        .map(|key| format!("{key} isn't a JSON object"))
}

/// Changes the file under one lock across read, change, write and rename: every writer goes
/// through here, so two writers at once never lose one's change. A file it can't read is left as
/// it is (`read_credentials_strictly`). Readers need no lock, since the rename makes each read see
/// a whole file.
pub(crate) fn mutate_credentials<T>(
    paths: &GxserverPaths,
    change: impl FnOnce(&mut Map<String, Value>) -> T,
) -> std::io::Result<T> {
    let _guard = credentials_lock();
    let mut credentials = read_credentials_strictly(paths)?;
    let changed = change(&mut credentials);
    write_credentials(paths, &credentials)?;
    Ok(changed)
}

/// The one lock every writer of the file holds across read, change, write and rename.
fn credentials_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_credentials(
    paths: &GxserverPaths,
    credentials: &Map<String, Value>,
) -> std::io::Result<()> {
    fs::create_dir_all(&paths.auth_dir)?;
    let mut credentials = credentials.clone();
    credentials.remove(RETIRED_SOURCE_CREDENTIALS_KEY);
    let path = credentials_path(paths);
    let temp = path.with_extension("json.tmp");
    let _ = fs::remove_file(&temp);
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(Value::Object(credentials).to_string().as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temp, &path)
}

fn non_empty(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Which key a store or lookup is about: one project's override, one workspace's key, or the
/// shared key (which is also the default workspace's key).
#[derive(Clone, Copy, Debug)]
pub(crate) enum LinearKeyScope<'a> {
    Project(&'a str),
    Workspace(&'a str),
    Shared,
}

/// The key a project's Linear calls use: its own override, else its workspace's key, else the
/// shared key.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: the Linear key is set once per workspace and shared by its projects, and a project can
/// override it in its own settings. The default (Personal) workspace's key is the shared key, so
/// `ghostex work-mode linear-key` and the Personal row of the Workspaces page set the same key.
pub(crate) fn linear_api_key(
    paths: &GxserverPaths,
    project_id: Option<&str>,
    workspace_id: Option<&str>,
) -> Option<String> {
    WorkCredentials::new(paths).linear_api_key(project_id, workspace_id)
}

/// Where each ticket source looks up its own credentials: the private file, read on the first
/// lookup and kept for the request or pass that made it. A source that needs none never opens it.
pub(crate) struct WorkCredentials {
    paths: GxserverPaths,
    file: OnceLock<Map<String, Value>>,
}

impl WorkCredentials {
    pub(crate) fn new(paths: &GxserverPaths) -> Self {
        Self {
            paths: paths.clone(),
            file: OnceLock::new(),
        }
    }

    /// Credentials holding only one project's own Linear key (none without one), for tests that
    /// never touch the file.
    #[cfg(test)]
    pub(crate) fn with_project_linear_key(project_id: &str, api_key: Option<&str>) -> Self {
        let mut file = Map::new();
        if let Some(api_key) = api_key {
            file.insert(
                "projectLinearApiKeys".to_string(),
                json!({ project_id: api_key }),
            );
        }
        Self {
            paths: crate::paths::get_gxserver_paths(Some(PathBuf::from("/gx-char/credentials"))),
            file: OnceLock::from(file),
        }
    }

    fn file(&self) -> &Map<String, Value> {
        self.file.get_or_init(|| read_credentials(&self.paths))
    }

    /// Whether the project has a Linear key of its own or its workspace's, which the shared key
    /// alone does not make (Create Linear Ticket, `linear_create_applies`).
    pub(crate) fn has_scoped_linear_key(
        &self,
        project_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> bool {
        let credentials = self.file();
        let scoped = |field: &str, id: Option<&str>| {
            id.and_then(|id| non_empty(credentials.get(field).and_then(|keys| keys.get(id))))
                .is_some()
        };
        scoped("projectLinearApiKeys", project_id) || scoped("workspaceLinearApiKeys", workspace_id)
    }

    /// The key a project's Linear calls use: its own override, else its workspace's key, else the
    /// shared key.
    pub(crate) fn linear_api_key(
        &self,
        project_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Option<String> {
        let credentials = self.file();
        let scoped = |field: &str, id: Option<&str>| {
            id.and_then(|id| non_empty(credentials.get(field).and_then(|keys| keys.get(id))))
        };
        scoped("projectLinearApiKeys", project_id)
            .or_else(|| scoped("workspaceLinearApiKeys", workspace_id))
            .or_else(|| non_empty(credentials.get("linearApiKey")))
    }
}

/// Never prints a credential.
impl std::fmt::Debug for WorkCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("WorkCredentials { .. }")
    }
}

/// Stores (or, with `None`, removes) the shared key, one workspace's key, or one project's
/// override.
pub(crate) fn store_linear_api_key(
    paths: &GxserverPaths,
    scope: LinearKeyScope<'_>,
    api_key: Option<&str>,
) -> std::io::Result<()> {
    mutate_credentials(paths, |credentials| {
        store_linear_api_key_in(credentials, scope, api_key)
    })
}

fn store_linear_api_key_in(
    credentials: &mut Map<String, Value>,
    scope: LinearKeyScope<'_>,
    api_key: Option<&str>,
) {
    let api_key = api_key.map(str::trim).filter(|key| !key.is_empty());
    let scoped = match scope {
        LinearKeyScope::Project(id) => Some(("projectLinearApiKeys", id)),
        LinearKeyScope::Workspace(id) if id != crate::workspaces::DEFAULT_WORKSPACE_ID => {
            Some(("workspaceLinearApiKeys", id))
        }
        LinearKeyScope::Workspace(_) | LinearKeyScope::Shared => None,
    };
    match scoped {
        Some((field, id)) => {
            let mut keys = credentials
                .get(field)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            match api_key {
                Some(key) => {
                    keys.insert(id.to_string(), json!(key));
                }
                None => {
                    keys.remove(id);
                }
            }
            credentials.insert(field.to_string(), Value::Object(keys));
        }
        None => match api_key {
            Some(key) => {
                credentials.insert("linearApiKey".to_string(), json!(key));
            }
            None => {
                credentials.remove("linearApiKey");
            }
        },
    }
}

/// Which keys exist, without their values.
pub(crate) fn linear_api_key_summary(paths: &GxserverPaths) -> Value {
    let credentials = read_credentials(paths);
    let project_overrides: Vec<String> = credentials
        .get("projectLinearApiKeys")
        .and_then(Value::as_object)
        .map(|keys| {
            keys.iter()
                .filter(|(_, key)| non_empty(Some(key)).is_some())
                .map(|(project_id, _)| project_id.clone())
                .collect()
        })
        .unwrap_or_default();
    let workspaces: Vec<String> = credentials
        .get("workspaceLinearApiKeys")
        .and_then(Value::as_object)
        .map(|keys| {
            keys.iter()
                .filter(|(_, key)| non_empty(Some(key)).is_some())
                .map(|(workspace_id, _)| workspace_id.clone())
                .collect()
        })
        .unwrap_or_default();
    json!({
        "shared": non_empty(credentials.get("linearApiKey")).is_some(),
        "projectOverrides": project_overrides,
        "workspaces": workspaces,
    })
}
