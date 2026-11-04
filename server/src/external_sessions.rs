//! CDXC:Sessions 2026-09-14 DECISION:
//! User: import ZCode conversations as well as Claude and Codex conversations from outside Ghostex, and let users continue them from Quick Access Sessions.
//! This extends the 2026-09-08 Claude and Codex decision to ZCode.

use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, GxserverLogger, LogLevel},
    paths::GxserverPaths,
};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static SCANNED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

mod hermes;
pub(crate) use hermes::HERMES_INJECTED_PREFIXES;
mod scan_cache;
use scan_cache::ScanCache;
#[cfg(test)]
mod tests;

#[derive(Deserialize, Serialize)]
struct Conversation {
    agent: String,
    id: String,
    cwd: String,
    title: String,
    path: PathBuf,
    agent_home: PathBuf,
    updated: String,
}

fn error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError::corrupt_state(format!("Could not discover external sessions: {error}"))
}

/// Makes the next discovery scan again. Bot sync calls it after adding a bot project: a scan that
/// ran before the project existed skipped that bot's conversations and marked the store scanned.
pub(crate) fn rescan_on_next_discovery(paths: &GxserverPaths) {
    if let Some(scanned) = SCANNED.get() {
        if let Ok(mut scanned) = scanned.lock() {
            scanned.remove(&paths.state_db_file);
        }
    }
}

/// CDXC:Sessions 2026-10-29 WHY:
/// Importing outside conversations is a side step of listing Ghostex's own closed sessions, so a store that cannot be read must not take that list down: the failure is logged and the import runs again on the next request (a failed discovery never marks the store scanned). Before this, one unreadable ZCode database emptied the Closed list and broke restoring from it.
pub(crate) fn discover_before_listing(
    db: &Connection,
    server_id: &str,
    paths: &GxserverPaths,
    refresh: bool,
) {
    if let Err(failure) = discover(db, server_id, paths, refresh) {
        let _ = GxserverLogger::new(paths.clone()).log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "externalSessions.discoveryFailed".to_string(),
            server_id: Some(server_id.to_string()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(failure.to_string()),
            details: None,
        });
    }
}

/// Receipts survive removing/restoring a history row, so deleted conversations
/// do not return on the next launch. Discovery never starts an agent process.
pub(crate) fn discover(
    db: &Connection,
    server_id: &str,
    paths: &GxserverPaths,
    refresh: bool,
) -> Result<(), DomainStateError> {
    let home = paths
        .isolated_agent_home_dir
        .as_ref()
        .unwrap_or(&paths.home_dir);
    let mut scanned = SCANNED
        .get_or_init(Default::default)
        .lock()
        .map_err(error)?;
    if !refresh && scanned.contains(&paths.state_db_file) {
        return Ok(());
    }
    let mut cache = ScanCache::load(db)?;
    let mut conversations = scan(home, paths.isolated_agent_home_dir.is_none(), &mut cache)?;
    conversations.extend(hermes::read_bot_conversations(db, server_id, paths)?);
    db.execute_batch("CREATE TABLE IF NOT EXISTS external_session_receipts (agent TEXT NOT NULL, conversationId TEXT NOT NULL, PRIMARY KEY(agent, conversationId))").map_err(error)?;
    let receipts = db
        .prepare("SELECT agent, conversationId FROM external_session_receipts")
        .map_err(error)?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(error)?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(error)?;
    let conversations: Vec<_> = conversations
        .into_iter()
        .filter(|conversation| {
            !receipts.contains(&(conversation.agent.clone(), conversation.id.clone()))
        })
        .collect();
    if conversations.is_empty() && !cache.has_changes() {
        scanned.insert(paths.state_db_file.clone());
        return Ok(());
    }
    if !conversations.is_empty() {
        reconcile_pending_codex_forks(&DomainRepository::new(db, server_id), home)?;
    }
    let transaction =
        Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(error)?;
    if conversations.is_empty() {
        cache.save(&transaction)?;
        transaction.commit().map_err(error)?;
        scanned.insert(paths.state_db_file.clone());
        return Ok(());
    }
    let repository = DomainRepository::new(&transaction, server_id);
    // Only the conversation ids every row records are needed, so no row is hydrated.
    let known: HashSet<String> = repository
        .list_recorded_agent_session_ids()?
        .iter()
        .map(|id| id.to_lowercase())
        .collect();
    // A project is only matched by path and handed on by id, so its session history is not read.
    let mut projects: HashMap<String, Value> = repository
        .list_projects_without_session_history()?
        .into_iter()
        .filter_map(|p| Some((project_key(p.get("path")?.as_str()?), p)))
        .collect();
    for conversation in conversations {
        // CDXC:Bots 2026-09-26 WHY:
        // A Hermes conversation belongs to its bot, the project at its profile folder.
        // Bot sync owns creating that project; an ordinary project made here would stop the folder from ever becoming a bot.
        let is_hermes = conversation.agent == hermes::HERMES_AGENT;
        let project_key = if is_hermes {
            project_key(&conversation.agent_home.to_string_lossy())
        } else {
            project_key(&conversation.cwd)
        };
        if is_hermes && !projects.contains_key(&project_key) {
            continue;
        }
        let inserted = transaction.execute("INSERT OR IGNORE INTO external_session_receipts (agent, conversationId) VALUES (?1, ?2)", (&conversation.agent, &conversation.id)).map_err(error)?;
        if inserted == 0 || known.contains(&conversation.id) {
            continue;
        }
        let project = match projects.get(&project_key) {
            Some(project) => project.clone(),
            None => {
                let project = repository.create_project(json!({
                    "name": Path::new(&conversation.cwd).file_name().and_then(|s| s.to_str()).unwrap_or(&conversation.cwd),
                    "path": conversation.cwd,
                    "isRecentProject": true,
                }).as_object().unwrap())?;
                projects.insert(project_key, project.clone());
                project
            }
        };
        repository.import_external_session(json!({
            "projectId": project["projectId"], "kind": "terminal", "surface": "workspace",
            "agentId": conversation.agent, "title": conversation.title, "cwd": conversation.cwd,
            "lifecycleState": "stopped", "lastActiveAt": conversation.updated,
            "providerState": {"lifecycleState": "missing", "probedAt": conversation.updated},
            "runtimeSettings": {
                "agentSessionId": conversation.id, "agentSessionPath": conversation.path,
                "externalSession": true, "externalAgentHome": conversation.agent_home, "titleSource": "user", "agentActivity": "idle"
            }
        }).as_object().unwrap())?;
    }
    cache.save(&transaction)?;
    transaction.commit().map_err(error)?;
    scanned.insert(paths.state_db_file.clone());
    Ok(())
}

/// CDXC:SessionFork 2026-09-23 WHY:
/// Codex writes its fork rollout before the managed row adopts the new conversation id. Importing that file first creates a second owner. Resolve the exact terminal process's open rollout before classifying external history; the presentation probe's cached pre-fork identity is too old for this ownership decision.
fn reconcile_pending_codex_forks(
    repository: &DomainRepository<'_>,
    home: &Path,
) -> Result<(), DomainStateError> {
    let is_pending_fork = |session: &Value| {
        session["lifecycleState"] == "running"
            && session["runtimeSettings"]["agentName"] == "codex"
            && session["runtimeSettings"]["externalSession"] != true
            && session["runtimeSettings"]["forkedFromSessionId"]
                .as_str()
                .is_some()
            && session["runtimeSettings"]["agentSessionId"]
                .as_str()
                .is_none_or(|id| id.trim().is_empty())
    };
    // `is_pending_fork` only accepts running rows.
    let pending = repository
        .list_sessions_with_lifecycle_state("running")?
        .into_iter()
        .filter(is_pending_fork)
        .collect::<Vec<_>>();
    let names = pending
        .iter()
        .filter_map(|session| session["zmxName"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let identities =
        crate::zmx::read_zmx_session_process_identities(&names, home).map_err(|failure| {
            error(format!(
                "Could not resolve managed fork ownership: {failure:?}"
            ))
        })?;
    for session in pending {
        let Some(identity) = session["zmxName"]
            .as_str()
            .and_then(|name| identities.get(name))
        else {
            continue;
        };
        if identity.agent_id.as_deref() != Some("codex") || identity.agent_session_path.is_none() {
            continue;
        }
        let (Some(project_id), Some(session_id)) =
            (session["projectId"].as_str(), session["sessionId"].as_str())
        else {
            continue;
        };
        let Some(current) = repository.get_session(project_id, session_id)? else {
            continue;
        };
        if !is_pending_fork(&current)
            || current["agentId"] != session["agentId"]
            || current["zmxName"] != session["zmxName"]
            || crate::agents::draft_agent_switch_in_progress(project_id, session_id)
        {
            continue;
        }
        crate::agents::apply_live_process_session_identity(
            repository,
            &current,
            project_id,
            session_id,
            identity.agent_id.clone(),
            identity.agent_session_id.clone(),
            identity.agent_session_path.clone(),
        )?;
    }
    Ok(())
}

fn project_key(path: &str) -> String {
    fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.trim_end_matches('/').to_string())
}

fn directories(path: &Path) -> Vec<PathBuf> {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_type().ok().filter(|t| t.is_dir()).map(|_| e.path()))
        .collect()
}

fn scan(
    home: &Path,
    use_environment: bool,
    cache: &mut ScanCache,
) -> Result<Vec<Conversation>, DomainStateError> {
    let mut claude_roots = vec![home.join(".claude")];
    claude_roots.extend(directories(&home.join(".claude-profiles")));
    let mut codex_roots = vec![home.join(".codex")];
    codex_roots.extend(directories(&home.join(".codex-profiles")));
    if use_environment {
        if let Some(root) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            claude_roots.push(root.into());
        }
        if let Some(root) = std::env::var_os("CODEX_HOME") {
            codex_roots.push(root.into());
        }
    }
    let mut conversations = HashMap::new();
    let mut visited = HashSet::new();
    for (agent, roots) in [("claude", claude_roots), ("codex", codex_roots)] {
        for root in roots {
            let Ok(root) = fs::canonicalize(root) else {
                continue;
            };
            if !visited.insert((agent, root.clone())) {
                continue;
            }
            let mut files = Vec::new();
            let mut titles = HashMap::new();
            if agent == "codex" {
                let index = root.join("session_index.jsonl");
                titles = cache
                    .read(&index, || read_codex_titles(&index))?
                    .unwrap_or_default();
            }
            if agent == "claude" {
                for base in ["projects", "projects2"] {
                    for project in directories(&root.join(base)) {
                        collect_files(&project, 0, &mut files);
                    }
                }
            } else {
                collect_files(&root.join("sessions"), 3, &mut files);
                collect_files(&root.join("archived_sessions"), 3, &mut files);
            }
            for path in files {
                if let Some(mut conversation) = cache.read(&path, || {
                    read_conversation(agent, path.clone(), root.clone())
                })? {
                    if let Some(title) = titles
                        .get(&conversation.id)
                        .filter(|title| !title.trim().is_empty())
                    {
                        conversation.title = title.chars().take(180).collect();
                    }
                    let key = (agent, conversation.id.clone());
                    let entry = conversations
                        .entry(key)
                        .or_insert_with(|| None::<Conversation>);
                    if entry
                        .as_ref()
                        .is_none_or(|previous| previous.updated < conversation.updated)
                    {
                        *entry = Some(conversation);
                    }
                }
            }
        }
    }
    let mut result: Vec<_> = conversations.into_values().flatten().collect();
    result.extend(
        read_zcode_conversations(&home.join(".zcode"))
            .map_err(|failure| error(format!("ZCode: {failure}")))?,
    );
    Ok(result)
}

fn read_codex_titles(path: &Path) -> io::Result<Option<HashMap<String, String>>> {
    let file = File::open(path)?;
    let mut titles = HashMap::new();
    for line in BufReader::new(file.take(64 * 1024 * 1024)).lines() {
        if let Ok(row) = serde_json::from_str::<Value>(&line?) {
            if let (Some(id), Some(title)) = (row["id"].as_str(), row["thread_name"].as_str()) {
                titles.insert(id.to_lowercase(), title.chars().take(180).collect());
            }
        }
    }
    Ok(Some(titles))
}

/// CDXC:Sessions 2026-09-14 WHY:
/// ZCode's CLI store includes headless conversations absent from its desktop task index; the index only supplies lifecycle exclusions.
/// Read SQLite on each discovery instead of using the transcript-file cache: WAL commits can leave the main database's size and mtime unchanged.
fn read_zcode_conversations(agent_home: &Path) -> io::Result<Vec<Conversation>> {
    let db_path = agent_home.join("cli").join("db").join("db.sqlite");
    let Some(connection) = open_optional_zcode_database(&db_path)? else {
        return Ok(Vec::new());
    };
    let excluded = read_zcode_excluded_ids(&agent_home.join("v2").join("tasks-index.sqlite"))?;
    let mut statement = connection
        .prepare(
            "SELECT id, directory, title, time_updated FROM session \
             WHERE task_type = 'interactive' AND parent_id IS NULL \
             AND time_archived IS NULL AND id NOT GLOB 'sess_subagent*'",
        )
        .map_err(io::Error::other)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(io::Error::other)?;
    let mut conversations = Vec::new();
    for row in rows {
        let (id, cwd, title, time_updated) = row.map_err(io::Error::other)?;
        if excluded.contains(&id)
            || id.trim().is_empty()
            || id.chars().any(char::is_control)
            || !Path::new(&cwd).is_absolute()
        {
            continue;
        }
        let Some(updated) = chrono::DateTime::from_timestamp_millis(time_updated) else {
            continue;
        };
        let title = title.unwrap_or_default();
        let title = title.trim();
        let display_title = if title.is_empty() {
            format!(
                "ZCode conversation {}",
                id.chars().take(18).collect::<String>()
            )
        } else {
            title.chars().take(180).collect()
        };
        conversations.push(Conversation {
            agent: "zcode".to_string(),
            id,
            cwd,
            title: display_title,
            path: db_path.clone(),
            agent_home: agent_home.to_path_buf(),
            updated: updated.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        });
    }
    Ok(conversations)
}

fn open_optional_zcode_database(path: &Path) -> io::Result<Option<Connection>> {
    match fs::metadata(path) {
        Err(failure) if failure.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => {
            result?;
        }
    }
    ghostex_find::agent_store::open_agent_store(path)
        .map(Some)
        .map_err(|failure| io::Error::other(format!("{}: {failure}", path.display())))
}

/// CDXC:Sessions 2026-09-14 WHY:
/// Only an absent desktop index means no exclusions; ignoring query or row errors can permanently import running or deleted conversations.
fn read_zcode_excluded_ids(index_path: &Path) -> io::Result<HashSet<String>> {
    let Some(connection) = open_optional_zcode_database(index_path)? else {
        return Ok(HashSet::new());
    };
    let mut statement = connection.prepare(
        "SELECT task_id FROM tasks WHERE deleted = 1 OR archived = 1 OR task_status = 'running'",
    ).map_err(io::Error::other)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(io::Error::other)?;
    rows.collect::<Result<HashSet<_>, _>>()
        .map_err(io::Error::other)
}

fn collect_files(root: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() && depth > 0 {
            collect_files(&path, depth - 1, files);
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
            files.push(path);
        }
    }
}

fn read_conversation(
    agent: &'static str,
    path: PathBuf,
    agent_home: PathBuf,
) -> io::Result<Option<Conversation>> {
    let mut file = File::open(&path)?;
    let metadata = file.metadata()?;
    let updated: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
    let mut id = String::new();
    let mut cwd = String::new();
    let mut title = String::new();
    // Read a bounded prefix for identity and first prompt, then a bounded tail
    // for user-assigned titles. Transcript bodies can be hundreds of MB.
    let mut prefix = Vec::new();
    (&mut file).take(256 * 1024).read_to_end(&mut prefix)?;
    let mut tail = Vec::new();
    if metadata.len() > 256 * 1024 {
        file.seek(SeekFrom::Start(metadata.len().saturating_sub(64 * 1024)))?;
        (&mut file).take(64 * 1024).read_to_end(&mut tail)?;
    }
    for data in [&prefix, &tail] {
        for line in BufReader::new(data.as_slice())
            .lines()
            .map_while(Result::ok)
        {
            let Ok(row) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if row.get("isSidechain").and_then(Value::as_bool) == Some(true) {
                return Ok(None);
            }
            if agent == "codex" && row["type"] == "session_meta" {
                let payload = &row["payload"];
                if payload["source"].get("subagent").is_some() || payload["source"] == "subagent" {
                    return Ok(None);
                }
                id = payload["id"].as_str().unwrap_or_default().to_lowercase();
                cwd = payload["cwd"].as_str().unwrap_or_default().to_string();
            } else if agent == "claude" {
                if id.is_empty() {
                    id = row["sessionId"].as_str().unwrap_or_default().to_lowercase();
                }
                if cwd.is_empty() {
                    cwd = row["cwd"].as_str().unwrap_or_default().to_string();
                }
                if let Some(custom) = row["customTitle"].as_str().filter(|s| !s.trim().is_empty()) {
                    title = custom.to_string();
                }
            }
            if title.is_empty() {
                let message = if agent == "claude" {
                    &row["message"]
                } else {
                    &row["payload"]
                };
                if message["role"] == "user" && row["isMeta"] != true {
                    let text = message["content"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            message["content"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|part| part["text"].as_str())
                                .collect::<Vec<_>>()
                                .join(" ")
                        });
                    if !text.trim_start().starts_with(['<', '#']) {
                        title = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    }
                }
            }
        }
    }
    if id.len() != 36
        || !id.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
        || cwd.is_empty()
        || !Path::new(&cwd).is_absolute()
    {
        return Ok(None);
    }
    if title.is_empty() {
        title = format!(
            "{} conversation {}",
            if agent == "claude" { "Claude" } else { "Codex" },
            &id[..8]
        );
    }
    Ok(Some(Conversation {
        agent: agent.to_string(),
        id,
        cwd,
        title: title.chars().take(180).collect(),
        path,
        agent_home,
        updated: updated.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    }))
}
