use super::{project_key, Conversation};
use crate::{
    bot_projects::{bots_enabled, discover_bot_profiles},
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, GxserverLogger, LogLevel},
    paths::GxserverPaths,
    presentation::util::normalize_spaces,
    session_chat_hermes::{hermes_home, is_safe_hermes_session_id, open_read_only_state_db},
};
use rusqlite::Connection;
use std::{collections::HashSet, path::Path};

pub(super) const HERMES_AGENT: &str = "hermes-agent";

/// CDXC:Bots 2026-09-27 DECISION:
/// User: Hermes sessions started outside Ghostex are listed in Quick Access Sessions under their bot, read from the root store and every profile's: CLI, TUI and Hermes app chats, and Discord and Telegram chats too ("import both").
/// Supersedes 2026-09-26, which kept Discord and other gateway sessions out; cron and other automated sessions still stay out.
///
/// CDXC:Bots 2026-09-27 WHY:
/// Only bots that already have a project are read: bot sync owns creating them, and a conversation with nowhere to go would stay unreceipted and send every refresh through the import transaction.
/// Nothing is read while Bots is off, so Ghostex behaves as before.
/// A row's cwd only says where the terminal reopens, and it can name a folder that is gone (a moved checkout, a previous user account), so a cwd that is not a directory reopens in the profile folder rather than dropping the conversation.
/// A store that cannot be read is logged and adds nothing this scan instead of failing the whole discovery; nothing is receipted, so its rows arrive on a later scan.
/// Sources are an allow-list: Hermes' platforms are open-ended (plugins add them) and include automated ones (webhook, API server, relay), so only the chat apps the user named are listed.
/// A session nobody wrote in is not a conversation and is not listed.
/// A Discord or Telegram chat reopens as a terminal resume; what is said there never reaches the chat app.
/// Bots dispatch work to each other in Discord threads, and a thread only bots wrote in is no more a conversation than a scripted run, so a Discord or Telegram chat is listed once a person wrote in it.
/// Hermes keeps no author flag, only the author's display name as a `[<name>] ` tag on each message of a shared thread (after any reply quote or channel context), and an untagged message is the session starter's.
/// A message whose author is one of this install's bots does not count, nor do the rows Hermes injects itself.
/// A bot whose chat-app name differs from its profile name reads as a person, so its threads show rather than hide.
/// Scripted one-shot runs (`hermes chat -q`, `hermes -z`) are not conversations either.
/// Newer Hermes saves them as `oneshot`, but older ones carry a local label (`cli`, or the `tui`/`desktop` label they inherited), and no stored field tells them apart from a chat: end reason, message count, title and cwd all overlap.
/// So a local row that started before the install's first `oneshot` row is listed only when a person wrote two different messages in it.
/// A single-query run has exactly one; the user rows Hermes injects itself (bracketed notices and compaction summaries, the tool-limit request, the prompt re-sent after compaction) do not count.
/// Rows after that first `oneshot` row trust the source, so a one-question chat still shows.
/// The first one is taken across every store read: one Hermes install writes them all, and a profile that never runs a one-shot would otherwise hide its one-question chats for good.
pub(super) fn read_bot_conversations(
    db: &Connection,
    server_id: &str,
    paths: &GxserverPaths,
) -> Result<Vec<Conversation>, DomainStateError> {
    if !bots_enabled(paths) {
        return Ok(Vec::new());
    }
    let hermes_home = paths
        .isolated_agent_home_dir
        .as_ref()
        .map_or_else(hermes_home, |home| home.join(".hermes"));
    let projects: HashSet<String> = DomainRepository::new(db, server_id)
        .list_projects()?
        .iter()
        .filter_map(|project| Some(project_key(project.get("path")?.as_str()?)))
        .collect();
    let bots = discover_bot_profiles(&hermes_home);
    let stores: Vec<(&Path, Connection)> = bots
        .iter()
        .filter(|bot| projects.contains(&project_key(&bot.path.to_string_lossy())))
        .filter_map(|bot| {
            let store = open_hermes_store(&bot.path).unwrap_or_else(|failure| {
                log_unreadable_store(paths, failure);
                None
            });
            store.map(|store| (bot.path.as_path(), store))
        })
        .collect();
    let oneshot_since = stores
        .iter()
        .filter_map(|(_, store)| {
            store
                .query_row(
                    "SELECT min(started_at) FROM sessions WHERE source = 'oneshot'",
                    [],
                    |row| row.get::<_, Option<f64>>(0),
                )
                .ok()
                .flatten()
        })
        .reduce(f64::min);
    let bot_names: HashSet<&str> = bots.iter().map(|bot| bot.name.as_str()).collect();
    Ok(stores
        .iter()
        .flat_map(|(profile_home, store)| {
            read_conversations(store, profile_home, oneshot_since, &bot_names).unwrap_or_else(
                |failure| {
                    log_unreadable_store(paths, failure.to_string());
                    Vec::new()
                },
            )
        })
        .collect())
}

/// A profile that has no store yet has nothing to read; a store that exists and cannot be opened
/// is a failure.
fn open_hermes_store(profile_home: &Path) -> Result<Option<Connection>, String> {
    let db_path = profile_home.join("state.db");
    if !db_path.is_file() {
        return Ok(None);
    }
    open_read_only_state_db(&db_path)
        .map(Some)
        .ok_or_else(|| "the store could not be opened read-only".to_string())
}

fn read_conversations(
    connection: &Connection,
    profile_home: &Path,
    oneshot_since: Option<f64>,
    bot_names: &HashSet<&str>,
) -> rusqlite::Result<Vec<Conversation>> {
    let mut statement = connection.prepare(
        "SELECT s.id, s.cwd, s.title, \
         CASE WHEN trim(coalesce(s.title, '')) = '' THEN \
          (SELECT substr(m.content, 1, 2000) FROM messages m \
           WHERE m.session_id = s.id AND m.role = 'user' ORDER BY m.id LIMIT 1) END, \
         COALESCE(s.last_activity_at, s.ended_at, s.started_at), \
         s.source IN ('discord', 'telegram'), s.origin_json \
         FROM sessions s WHERE s.parent_session_id IS NULL AND s.archived = 0 AND s.hidden = 0 \
         AND EXISTS (SELECT 1 FROM messages m WHERE m.session_id = s.id AND m.role = 'user') \
         AND (s.source IN ('discord', 'telegram') \
          OR s.source IN ('cli', 'tui', 'desktop') AND (s.started_at >= ?1 \
           OR (SELECT count(DISTINCT m.content) FROM messages m \
            WHERE m.session_id = s.id AND m.role = 'user' AND ltrim(m.content) NOT LIKE '[%' \
            AND m.content NOT LIKE 'You''ve reached the maximum number of tool-calling iterations%') >= 2))",
    )?;
    let rows = statement.query_map([oneshot_since], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut conversations = Vec::new();
    for row in rows {
        let (id, cwd, title, prompt, updated, messaging, origin) = row?;
        if messaging {
            let starter = origin
                .and_then(|origin| serde_json::from_str::<serde_json::Value>(&origin).ok())
                .and_then(|origin| origin["user_name"].as_str().map(str::to_string));
            if !a_person_wrote(connection, &id, starter.as_deref(), bot_names)? {
                continue;
            }
        }
        let Some(updated) = chrono::DateTime::from_timestamp_millis((updated * 1000.0) as i64)
        else {
            continue;
        };
        if !is_safe_hermes_session_id(&id) {
            continue;
        }
        let cwd = cwd
            .filter(|cwd| Path::new(cwd).is_absolute() && Path::new(cwd).is_dir())
            .unwrap_or_else(|| profile_home.to_string_lossy().into_owned());
        let title = [title, prompt]
            .into_iter()
            .flatten()
            .map(|text| normalize_spaces(&text))
            .find(|text| !text.is_empty())
            .unwrap_or_else(|| format!("Hermes conversation {id}"));
        conversations.push(Conversation {
            agent: HERMES_AGENT.to_string(),
            id,
            cwd,
            title: title.chars().take(180).collect(),
            path: profile_home.join("state.db"),
            agent_home: profile_home.to_path_buf(),
            updated: updated.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        });
    }
    Ok(conversations)
}

/// The user rows Hermes writes itself, as its `_SYNTHETIC_USER_ROW_PREFIXES` lists them. Its other
/// injected rows (the tool-limit request, recovery nudges) carry no tag and read as the starter's.
pub(crate) const HERMES_INJECTED_PREFIXES: [&str; 9] = [
    "[System:",
    "[CONTEXT",
    "[PRIOR CONTEXT",
    "[IMPORTANT: Background",
    "[Your active task list",
    "[Planning state preserved",
    "[ASYNC DELEGATION",
    "[OUT-OF-BAND",
    "Cronjob Response:",
];

/// Whether someone other than this install's bots wrote a message in a Discord or Telegram session.
fn a_person_wrote(
    connection: &Connection,
    session_id: &str,
    starter: Option<&str>,
    bot_names: &HashSet<&str>,
) -> rusqlite::Result<bool> {
    let mut statement = connection
        .prepare_cached("SELECT content FROM messages WHERE session_id = ?1 AND role = 'user'")?;
    let contents = statement.query_map([session_id], |row| row.get::<_, Option<String>>(0))?;
    for content in contents {
        let content = content?.unwrap_or_default();
        let content = content.trim_start();
        if HERMES_INJECTED_PREFIXES
            .iter()
            .any(|prefix| content.starts_with(prefix))
        {
            continue;
        }
        let author = message_author(content).or(starter);
        if !author.is_some_and(|author| bot_names.contains(author)) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The author tag of a message in a shared thread.
fn message_author(content: &str) -> Option<&str> {
    let message = match content.rsplit_once("\n[New message]\n") {
        Some((_, message)) => message,
        None if content.starts_with("[Replying to") => content
            .split_once("\"]\n\n")
            .map_or(content, |(_, message)| message),
        None => content,
    };
    let (author, _) = message.strip_prefix('[')?.split_once("] ")?;
    (!author.contains('\n')).then_some(author)
}

fn log_unreadable_store(paths: &GxserverPaths, failure: String) {
    let _ = GxserverLogger::new(paths.clone()).log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "externalSessions.hermesStoreUnreadable".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(failure),
        details: None,
    });
}

#[cfg(test)]
mod tests {
    use super::super::discover;
    use crate::{
        domain::DomainRepository,
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };
    use rusqlite::Connection;
    use serde_json::{json, Value};
    use std::{fs, path::Path};

    /// A WAL-mode Hermes store with one session per `(id, source)`, each opened in `cwd` with the
    /// prompt "hi"; `adjust` is SQL that gives single rows their difference.
    fn write_store(folder: &Path, rows: &[(&str, &str)], cwd: &Path, adjust: &str) -> Connection {
        fs::create_dir_all(folder).unwrap();
        let store = Connection::open(folder.join("state.db")).unwrap();
        store
            .execute_batch(
                "PRAGMA journal_mode = WAL; \
                 CREATE TABLE sessions (id TEXT PRIMARY KEY, source TEXT NOT NULL, parent_session_id TEXT, \
                 started_at REAL NOT NULL, ended_at REAL, cwd TEXT, title TEXT, archived INTEGER NOT NULL DEFAULT 0, \
                 last_activity_at REAL, hidden INTEGER NOT NULL DEFAULT 0, origin_json TEXT); \
                 CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, \
                 role TEXT NOT NULL, content TEXT, timestamp REAL NOT NULL);",
            )
            .unwrap();
        for (id, source) in rows {
            store
                .execute(
                    "INSERT INTO sessions (id, source, started_at, cwd) VALUES (?1, ?2, 1790413815.5, ?3)",
                    rusqlite::params![id, source, cwd.to_str()],
                )
                .unwrap();
            store
                .execute(
                    "INSERT INTO messages (session_id, role, content, timestamp) VALUES (?1, 'user', 'hi', 1790413816.0)",
                    [id],
                )
                .unwrap();
        }
        store.execute_batch(adjust).unwrap();
        store
    }

    #[test]
    fn only_top_level_conversations_of_every_profile_import_under_their_bot() {
        let temp = tempfile::tempdir().unwrap();
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).unwrap();
        let db = open_gxserver_database(&paths).unwrap();
        let hermes = temp.path().join(".hermes");
        let harry = hermes.join("profiles").join("harry");
        let work = temp.path().join("work");
        fs::create_dir_all(&work).unwrap();
        // The root store is closed, as when Hermes is not running: SQLite has removed its WAL files.
        // It has no `oneshot` row, but Harry's shows this Hermes install labels one-shots, so its
        // later one-message rows trust their source. Nobody wrote in root_empty. Harry started both
        // Discord threads: only Harry wrote in root_dispatch, and Alex replied in root_discord.
        drop(write_store(
            &hermes,
            &[
                ("root_cli", "cli"),
                ("root_child", "cli"),
                ("root_archived", "cli"),
                ("root_hidden", "cli"),
                ("root_app", "desktop"),
                ("root_tui", "tui"),
                ("root_dispatch", "discord"),
                ("root_discord", "discord"),
                ("root_telegram", "telegram"),
                ("root_empty", "telegram"),
            ],
            &work,
            "UPDATE sessions SET title = 'Root chat' WHERE id = 'root_cli'; \
             UPDATE sessions SET parent_session_id = 'root_cli' WHERE id = 'root_child'; \
             UPDATE sessions SET archived = 1 WHERE id = 'root_archived'; \
             UPDATE sessions SET hidden = 1 WHERE id = 'root_hidden'; \
             DELETE FROM messages WHERE session_id = 'root_empty'; \
             UPDATE sessions SET origin_json = '{\"user_name\": \"Harry\"}' WHERE source = 'discord'; \
             UPDATE sessions SET origin_json = '{\"user_name\": \"Alex\"}' WHERE id = 'root_telegram'; \
             UPDATE sessions SET title = 'Ship the fix' WHERE id = 'root_discord'; \
             UPDATE messages SET content = '[Harry] Dispatched: ship the fix' \
              WHERE session_id IN ('root_dispatch', 'root_discord'); \
             INSERT INTO messages (session_id, role, content, timestamp) VALUES \
              ('root_dispatch', 'user', '[CONTEXT COMPACTION — REFERENCE ONLY] Earlier turns', 1790413817.0), \
              ('root_dispatch', 'user', '[Alex] Earlier line\n\n[New message]\n[Harry] Thanks.', 1790413818.0), \
              ('root_discord', 'user', '[Replying to: \"Done.\"]\n\n[Alex] Merge it.', 1790413817.0);",
        ));
        // Harry's stays open, as under a running gateway. Its local rows predate its `oneshot` row:
        // harry_cli is a chat because a person wrote twice, harry_scripted is a one-shot run padded
        // with Hermes' own rows, harry_app a one-shot that inherited the app's label.
        let _harry_store = write_store(
            &harry,
            &[
                ("harry_cli", "cli"),
                ("harry_scripted", "cli"),
                ("harry_app", "desktop"),
                ("harry_oneshot", "oneshot"),
                ("harry_cron", "cron"),
            ],
            &work,
            "UPDATE sessions SET started_at = 1790413700.0 WHERE source IN ('cli', 'desktop'); \
             UPDATE sessions SET started_at = 1790413800.0 WHERE id = 'harry_oneshot'; \
             UPDATE sessions SET cwd = '/gone/old-account' WHERE id = 'harry_cli'; \
             UPDATE messages SET content = 'Triage   the\nboard' WHERE session_id = 'harry_cli'; \
             INSERT INTO messages (session_id, role, content, timestamp) VALUES \
              ('harry_cli', 'user', 'then close it', 1790413817.0), \
              ('harry_scripted', 'user', '[CONTEXT COMPACTION — REFERENCE ONLY] Earlier turns', 1790413817.0), \
              ('harry_scripted', 'user', 'You''ve reached the maximum number of tool-calling iterations allowed.', 1790413818.0), \
              ('harry_scripted', 'user', 'hi', 1790413819.0);",
        );
        let repository = DomainRepository::new(&db, "S1");
        for folder in [&hermes, &harry] {
            let project = json!({ "path": folder });
            repository
                .add_project_path(project.as_object().unwrap())
                .unwrap();
        }
        let canonical = |path: &Value| {
            fs::canonicalize(path.as_str().unwrap_or_default())
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let imported = || {
            let mut rows: Vec<Value> = repository
                .list_sessions(None)
                .unwrap()
                .into_iter()
                .filter(|session| session["agentId"] == "hermes-agent")
                .map(|session| {
                    let project = repository
                        .get_project(session["projectId"].as_str().unwrap())
                        .unwrap()
                        .unwrap();
                    json!({
                        "id": session["runtimeSettings"]["agentSessionId"],
                        "title": session["title"],
                        "project": canonical(&project["path"]),
                        "cwd": canonical(&session["cwd"]),
                        "home": canonical(&session["runtimeSettings"]["externalAgentHome"]),
                    })
                })
                .collect();
            rows.sort_by_key(|row| row["id"].as_str().unwrap_or_default().to_string());
            rows
        };

        discover(&db, "S1", &paths, true).unwrap();
        assert_eq!(imported(), Vec::<Value>::new(), "nothing while Bots is off");

        fs::create_dir_all(&paths.app_config_dir).unwrap();
        fs::write(
            paths.app_config_dir.join("native-sidebar-settings.json"),
            r#"{"botsHidden": false}"#,
        )
        .unwrap();
        discover(&db, "S1", &paths, true).unwrap();
        let [hermes, harry, work] = [&hermes, &harry, &work].map(|path| canonical(&json!(path)));
        assert_eq!(
            imported(),
            vec![
                json!({ "id": "harry_cli", "title": "Triage the board", "project": harry, "cwd": harry, "home": harry }),
                json!({ "id": "root_app", "title": "hi", "project": hermes, "cwd": work, "home": hermes }),
                json!({ "id": "root_cli", "title": "Root chat", "project": hermes, "cwd": work, "home": hermes }),
                json!({ "id": "root_discord", "title": "Ship the fix", "project": hermes, "cwd": work, "home": hermes }),
                json!({ "id": "root_telegram", "title": "hi", "project": hermes, "cwd": work, "home": hermes }),
                json!({ "id": "root_tui", "title": "hi", "project": hermes, "cwd": work, "home": hermes }),
            ]
        );
    }
}
