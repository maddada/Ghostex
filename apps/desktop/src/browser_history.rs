use std::{
    collections::HashMap,
    sync::{OnceLock, mpsc},
    time::{SystemTime, UNIX_EPOCH},
};

use futures::channel::oneshot;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) struct HistoryPage {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) favicon_url: Option<String>,
    pub(crate) remote_machine_id: Option<String>,
}

enum Command {
    Record {
        key: (u64, u64),
        page: HistoryPage,
        navigation: bool,
    },
    Import(Vec<HistoryPage>),
    Query {
        project_id: Option<String>,
        query: String,
        before: Option<(i64, i64)>,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Get {
        id: i64,
        reply: oneshot::Sender<Result<Option<HistoryPage>, String>>,
    },
}

/// CDXC:Browser 2026-09-09 DECISION:
/// User: save Browser history across projects and restarts, including page titles and favicons for the history popup.
/// A separate visit store survives tab closure and back/forward-list truncation; the single database worker preserves navigation/metadata order without disk I/O on the UI thread.
fn sender() -> &'static mpsc::Sender<Command> {
    static SENDER: OnceLock<mpsc::Sender<Command>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("browser-history".into())
            .spawn(move || run_worker(receiver))
            .expect("start browser history worker");
        sender
    })
}

fn run_worker(receiver: mpsc::Receiver<Command>) {
    let database = open_database().map_err(|error| error.to_string());
    let mut current_visits = HashMap::<(u64, u64), (i64, String)>::new();
    for command in receiver {
        match command {
            Command::Record {
                key,
                page,
                navigation,
            } => {
                let result = database.as_ref().map_err(Clone::clone).and_then(|db| {
                    record_visit(db, &mut current_visits, key, page, navigation)
                        .map_err(|error| error.to_string())
                });
                if let Err(error) = result {
                    eprintln!("Could not save browser history: {error}");
                }
            }
            Command::Import(pages) => {
                let result = database
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|db| import_pages(db, pages).map_err(|error| error.to_string()));
                if let Err(error) = result {
                    eprintln!("Could not import browser history: {error}");
                }
            }
            Command::Query {
                project_id,
                query,
                before,
                reply,
            } => {
                let result = database.as_ref().map_err(Clone::clone).and_then(|db| {
                    query_visits(db, project_id, query, before).map_err(|error| error.to_string())
                });
                let _ = reply.send(result);
            }
            Command::Get { id, reply } => {
                let result = database
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|db| get_visit(db, id).map_err(|error| error.to_string()));
                let _ = reply.send(result);
            }
        }
    }
}

fn record_visit(
    db: &Connection,
    current_visits: &mut HashMap<(u64, u64), (i64, String)>,
    key: (u64, u64),
    page: HistoryPage,
    navigation: bool,
) -> rusqlite::Result<()> {
    let current = current_visits.get(&key).filter(|(_, url)| *url == page.url);
    if let Some((id, _)) = current {
        db.execute(
            "UPDATE visits SET title = CASE WHEN ?1 = '' THEN title ELSE ?1 END,
                favicon_url = COALESCE(?2, favicon_url),
                project_name = CASE WHEN ?3 = '' THEN project_name ELSE ?3 END WHERE id = ?4",
            params![page.title, page.favicon_url, page.project_name, id],
        )?;
    } else if navigation {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        insert_visit(db, &page, timestamp)?;
        current_visits.insert(key, (db.last_insert_rowid(), page.url));
    }
    Ok(())
}

fn get_visit(db: &Connection, id: i64) -> rusqlite::Result<Option<HistoryPage>> {
    db.query_row(
        "SELECT project_id, project_name, url, title, favicon_url, remote_machine_id FROM visits WHERE id = ?1",
        [id],
        |row| Ok(HistoryPage {
            project_id: row.get(0)?, project_name: row.get(1)?, url: row.get(2)?,
            title: row.get(3)?, favicon_url: row.get(4)?, remote_machine_id: row.get(5)?,
        }),
    ).optional()
}

fn open_database() -> anyhow::Result<Connection> {
    let directory = &crate::shared_settings::ghostex_storage_paths().state_dir;
    std::fs::create_dir_all(directory)?;
    let db = Connection::open(directory.join("browser-history.sqlite3"))?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS visits (
            id INTEGER PRIMARY KEY, project_id TEXT NOT NULL, project_name TEXT NOT NULL,
            url TEXT NOT NULL, title TEXT NOT NULL, favicon_url TEXT, remote_machine_id TEXT, visited_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS visits_recent ON visits(visited_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS visits_project ON visits(project_id, visited_at DESC, id DESC);
        CREATE TABLE IF NOT EXISTS migrations (name TEXT PRIMARY KEY);")?;
    Ok(db)
}

fn insert_visit(db: &Connection, page: &HistoryPage, timestamp: i64) -> rusqlite::Result<usize> {
    db.execute("INSERT INTO visits(project_id, project_name, url, title, favicon_url, remote_machine_id, visited_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![page.project_id, page.project_name, page.url, page.title, page.favicon_url, page.remote_machine_id, timestamp])
}

fn import_pages(db: &Connection, pages: Vec<HistoryPage>) -> rusqlite::Result<()> {
    let transaction = db.unchecked_transaction()?;
    let imported: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM migrations WHERE name = 'tab-history')",
        [],
        |row| row.get(0),
    )?;
    if !imported {
        for page in pages {
            let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM visits WHERE project_id = ?1 AND url = ?2 AND remote_machine_id IS ?3)", params![page.project_id, page.url, page.remote_machine_id], |row| row.get(0))?;
            if !exists {
                insert_visit(&transaction, &page, 0)?;
            }
        }
        transaction.execute("INSERT INTO migrations(name) VALUES ('tab-history')", [])?;
    }
    transaction.commit()
}

fn query_visits(
    db: &Connection,
    project_id: Option<String>,
    query: String,
    before: Option<(i64, i64)>,
) -> rusqlite::Result<Value> {
    let mut statement = db.prepare("SELECT id, project_id, project_name, url, title, favicon_url, visited_at FROM visits
        WHERE (?1 IS NULL OR project_id = ?1) AND (?2 = '' OR instr(lower(title || ' ' || url || ' ' || project_name), lower(?2)) > 0)
        AND (?3 IS NULL OR visited_at < ?3 OR (visited_at = ?3 AND id < ?4))
        ORDER BY visited_at DESC, id DESC LIMIT 101")?;
    let mut rows = statement.query_map(params![project_id, query, before.map(|(time, _)| time), before.map(|(_, id)| id)], |row| Ok(json!({
        "id": row.get::<_, i64>(0)?.to_string(), "projectId": row.get::<_, String>(1)?, "projectName": row.get::<_, String>(2)?,
        "url": row.get::<_, String>(3)?, "title": row.get::<_, String>(4)?, "faviconUrl": row.get::<_, Option<String>>(5)?,
        "visitedAt": row.get::<_, i64>(6)?,
    })))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let has_more = rows.len() > 100;
    rows.truncate(100);
    Ok(json!({ "entries": rows, "hasMore": has_more }))
}

pub(crate) fn record(key: (u64, u64), page: HistoryPage, navigation: bool) {
    if sender()
        .send(Command::Record {
            key,
            page,
            navigation,
        })
        .is_err()
    {
        eprintln!("Browser history worker stopped");
    }
}

pub(crate) fn import(pages: Vec<HistoryPage>) {
    if sender().send(Command::Import(pages)).is_err() {
        eprintln!("Browser history worker stopped");
    }
}

pub(crate) async fn query(
    project_id: Option<String>,
    query: String,
    before: Option<(i64, i64)>,
) -> Result<Value, String> {
    let (reply, response) = oneshot::channel();
    sender()
        .send(Command::Query {
            project_id,
            query,
            before,
            reply,
        })
        .map_err(|e| e.to_string())?;
    response.await.map_err(|e| e.to_string())?
}

pub(crate) async fn get(id: i64) -> Result<Option<HistoryPage>, String> {
    let (reply, response) = oneshot::channel();
    sender()
        .send(Command::Get { id, reply })
        .map_err(|e| e.to_string())?;
    response.await.map_err(|e| e.to_string())?
}
