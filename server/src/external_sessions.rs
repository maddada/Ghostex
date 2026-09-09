//! CDXC:Sessions 2026-09-08 DECISION:
//! User: detect Claude and Codex conversations from outside Ghostex on first use and let users continue them from Quick Access Sessions.

use crate::{
    domain::{DomainRepository, DomainStateError},
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

mod scan_cache;
use scan_cache::ScanCache;

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
    let conversations = scan(home, paths.isolated_agent_home_dir.is_none(), &mut cache)?;
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
    let transaction =
        Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(error)?;
    if conversations.is_empty() {
        cache.save(&transaction)?;
        transaction.commit().map_err(error)?;
        scanned.insert(paths.state_db_file.clone());
        return Ok(());
    }
    let repository = DomainRepository::new(&transaction, server_id);
    let known: HashSet<String> = repository
        .list_sessions(None)?
        .iter()
        .flat_map(|session| {
            session
                .pointer("/runtimeSettings/agentSessionId")
                .and_then(Value::as_str)
                .into_iter()
                .chain(
                    session
                        .pointer("/runtimeSettings/previousAgentSessionIds")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                )
                .map(str::to_lowercase)
        })
        .collect();
    let mut projects: HashMap<String, Value> = repository
        .list_projects()?
        .into_iter()
        .filter_map(|p| Some((project_key(p.get("path")?.as_str()?), p)))
        .collect();
    for conversation in conversations {
        let inserted = transaction.execute("INSERT OR IGNORE INTO external_session_receipts (agent, conversationId) VALUES (?1, ?2)", (&conversation.agent, &conversation.id)).map_err(error)?;
        if inserted == 0 || known.contains(&conversation.id) {
            continue;
        }
        let project_key = project_key(&conversation.cwd);
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
    Ok(conversations.into_values().flatten().collect())
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
