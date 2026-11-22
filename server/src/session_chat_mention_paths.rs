//! Which `@path` mentions in a user's chat message name something that really exists, so the chats
//! draw only those as file and folder pills.
//!
//! CDXC:SessionChat 2026-10-28 DECISION:
//! User: "please make @ something only show the file pill thingy if it's actually a file in the project not for everything" (an `npm install -g @deepseek-ai/dsh` prompt showed `@deepseek-ai/dsh` as a folder pill). An `@…` in a message the user sent is a file or folder pill only when that path exists: relative to the session's working folder or its project's folder, `~/…` under the home folder, or an absolute path. npm scopes, `@handles`, decorators and anything else stay plain text. gxserver decides, because only it sees the disk: every user row it sends a chat (the readSessionChat answer, history pages, and the follower's snapshot and appended frames) carries `mentionPaths`, the mentions it found on disk, and gx-chat-core links nothing else (`native_markdown_with_mentions`). The desktop, the web build and the phone all read the same rows. A row from a gxserver that predates the field shows its mentions as plain text.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/file_paths.rs (`bare_file_paths`, the one rule for what is a mention), packages/gx-chat-core/src/transcript/native_markdown.rs (where the pill is made), packages/gx-protocol/src/chat.rs (`ChatMessage::mention_paths`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use ghostex_gx_chat_core::transcript::file_paths::bare_file_paths;
use ghostex_gx_chat_core::transcript::file_position::split_file_position;
use serde_json::{Map, Value};

use crate::domain::DomainRepository;
use crate::paths::GxserverPaths;
use crate::storage::open_gxserver_database;

/// How long a session's folders and a path's existence are trusted before they are read again.
const ROOTS_TTL: Duration = Duration::from_secs(60);
const EXISTS_TTL: Duration = Duration::from_secs(20);
/// Past this many remembered paths the cache starts over rather than grow without bound.
const EXISTS_CACHE_LIMIT: usize = 4_096;

struct Installed {
    paths: GxserverPaths,
    server_id: String,
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

type SessionKey = (String, String);

fn roots_cache() -> &'static Mutex<HashMap<SessionKey, (Instant, Vec<PathBuf>)>> {
    static CACHE: OnceLock<Mutex<HashMap<SessionKey, (Instant, Vec<PathBuf>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn exists_cache() -> &'static Mutex<HashMap<PathBuf, (Instant, bool)>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Instant, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Lets the frame builders, which have no `AppState`, read a session's folders. Called once when
/// gxserver starts.
pub(crate) fn install(paths: &GxserverPaths, server_id: &str) {
    let _ = INSTALLED.set(Installed {
        paths: paths.clone(),
        server_id: server_id.to_string(),
    });
}

/// The session's working folder, then its project's folder, as stored.
fn read_roots(project_id: &str, session_id: &str) -> Vec<PathBuf> {
    let Some(installed) = INSTALLED.get() else {
        return Vec::new();
    };
    let read = || -> Option<Vec<PathBuf>> {
        let db = open_gxserver_database(&installed.paths).ok()?;
        let repository = DomainRepository::new(&db, installed.server_id.as_str());
        let folder = |value: &Value, key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| Path::new(path).is_absolute())
                .map(PathBuf::from)
        };
        let session = repository.get_session(project_id, session_id).ok()??;
        let project = repository.get_project(project_id).ok().flatten();
        let mut roots = Vec::new();
        for root in [
            folder(&session, "cwd"),
            project.as_ref().and_then(|project| folder(project, "path")),
        ]
        .into_iter()
        .flatten()
        {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        Some(roots)
    };
    read().unwrap_or_default()
}

fn session_roots(project_id: &str, session_id: &str) -> Vec<PathBuf> {
    let key = (project_id.to_string(), session_id.to_string());
    if let Ok(cache) = roots_cache().lock() {
        if let Some((at, roots)) = cache.get(&key) {
            if at.elapsed() < ROOTS_TTL {
                return roots.clone();
            }
        }
    }
    let roots = read_roots(project_id, session_id);
    if let Ok(mut cache) = roots_cache().lock() {
        cache.insert(key, (Instant::now(), roots.clone()));
    }
    roots
}

fn exists(path: &Path) -> bool {
    if let Ok(cache) = exists_cache().lock() {
        if let Some((at, found)) = cache.get(path) {
            if at.elapsed() < EXISTS_TTL {
                return *found;
            }
        }
    }
    let found = path.exists();
    if let Ok(mut cache) = exists_cache().lock() {
        if cache.len() >= EXISTS_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (Instant::now(), found));
    }
    found
}

/// The places a mention may name: itself when absolute, under the home folder for `~/…`, else
/// under each root.
fn candidates(mention: &str, roots: &[PathBuf], home: Option<&Path>) -> Vec<PathBuf> {
    let (path, _) = split_file_position(mention);
    let path = path.trim_end_matches(['/', '\\']);
    if path.is_empty() {
        return Vec::new();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.map(|home| vec![home.join(rest)]).unwrap_or_default();
    }
    let as_path = Path::new(path);
    if as_path.is_absolute() {
        return vec![as_path.to_path_buf()];
    }
    roots.iter().map(|root| root.join(as_path)).collect()
}

/// The `@` mentions of `text` that exist under `roots` (or absolutely), in the spelling
/// `bare_file_paths` gives them, which is what gx-chat-core compares with.
pub(crate) fn existing_mentions(
    text: &str,
    roots: &[PathBuf],
    home: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
) -> Vec<String> {
    let mut found = Vec::new();
    for mention in bare_file_paths(text)
        .into_iter()
        .filter(|found| found.mention)
    {
        if found.contains(&mention.path) {
            continue;
        }
        if candidates(&mention.path, roots, home)
            .iter()
            .any(|candidate| exists(candidate))
        {
            found.push(mention.path);
        }
    }
    found
}

/// The text a user row's text blocks hold, joined.
fn user_text(message: &Map<String, Value>) -> Option<String> {
    if message.get("role").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let text: Vec<&str> = message
        .get("blocks")?
        .as_array()?
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    let text = text.join("\n");
    text.contains('@').then_some(text)
}

/// Writes `mentionPaths` on every user row of `messages` that holds an `@` mention: the ones that
/// exist, possibly none. Rows without a mention are left alone.
pub(crate) fn annotate_messages(messages: &mut [Value], project_id: &str, session_id: &str) {
    let mut roots: Option<Vec<PathBuf>> = None;
    let home = INSTALLED
        .get()
        .map(|installed| installed.paths.home_dir.clone());
    for message in messages.iter_mut() {
        let Some(object) = message.as_object_mut() else {
            continue;
        };
        let Some(text) = user_text(object) else {
            continue;
        };
        if !bare_file_paths(&text).iter().any(|found| found.mention) {
            continue;
        }
        let roots = roots.get_or_insert_with(|| session_roots(project_id, session_id));
        let existing = existing_mentions(&text, roots, home.as_deref(), &exists);
        object.insert("mentionPaths".to_string(), Value::from(existing));
    }
}

/// [`annotate_messages`] on a read answer's or frame's `messages`.
pub(crate) fn annotate_result(result: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    if let Some(messages) = result.get_mut("messages").and_then(Value::as_array_mut) {
        annotate_messages(messages, project_id, session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project() -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().expect("a temp folder");
        std::fs::create_dir_all(dir.path().join("src/app")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        let roots = vec![dir.path().to_path_buf()];
        (dir, roots)
    }

    fn mentions(text: &str, roots: &[PathBuf]) -> Vec<String> {
        existing_mentions(text, roots, None, &|path: &Path| path.exists())
    }

    #[test]
    fn a_real_file_and_a_real_folder_are_mentions() {
        let (_dir, roots) = project();
        assert_eq!(
            mentions("look at @src/main.rs please", &roots),
            vec!["src/main.rs"]
        );
        assert_eq!(mentions("and @src/app", &roots), vec!["src/app"]);
        assert_eq!(
            mentions("line @src/main.rs:3", &roots),
            vec!["src/main.rs:3"]
        );
    }

    #[test]
    fn a_missing_path_an_npm_scope_a_handle_and_a_decorator_are_not() {
        let (_dir, roots) = project();
        assert!(mentions("open @src/missing.rs", &roots).is_empty());
        assert!(mentions(
            "npm install -g @deepseek-ai/dsh <- command to install deepseek harness",
            &roots
        )
        .is_empty());
        assert!(mentions("thanks @someone, see @Override", &roots).is_empty());
        assert!(mentions("mail me at someone@example.com", &roots).is_empty());
    }

    #[test]
    fn an_absolute_path_counts_when_it_exists() {
        let (dir, _) = project();
        let absolute = format!("@{}", dir.path().join("src/main.rs").display());
        assert_eq!(mentions(&absolute, &[]).len(), 1);
    }

    #[test]
    fn user_rows_with_mentions_get_the_list() {
        let (_dir, roots) = project();
        let mut rows = vec![
            json!({"role": "user", "blocks": [{"type": "text", "text": "fix @src/main.rs and @deepseek-ai/dsh"}]}),
            json!({"role": "user", "blocks": [{"type": "text", "text": "no mention here"}]}),
            json!({"role": "assistant", "blocks": [{"type": "text", "text": "@src/main.rs"}]}),
        ];
        // Seeded so the test needs no installed database.
        roots_cache().lock().unwrap().insert(
            ("P1".to_string(), "G1".to_string()),
            (Instant::now(), roots),
        );
        annotate_messages(&mut rows, "P1", "G1");
        assert_eq!(rows[0]["mentionPaths"], json!(["src/main.rs"]));
        assert!(rows[1].get("mentionPaths").is_none());
        assert!(rows[2].get("mentionPaths").is_none());
    }
}
