//! The Skills section of a session whose agent is not Claude: the skills the `$` picker offers it
//! (`session_chat_skills.rs`, which mirrors each CLI's own skill folders), with each SKILL.md's
//! description.
//!
//! CDXC:SessionDetails 2026-10-25 WHY:
//! Only Claude's transcript says which skills it listed to the model and which the session used. Other agents record neither, so their card lists what is installed where the agent loads skills from, the same list the `$` picker shows, and has no "used in this session" part.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

/// How long one agent's list for one folder is reused (Grok's comes from `grok inspect`).
const FRESH_FOR: Duration = Duration::from_secs(60);
/// The head of a SKILL.md read for its front matter.
const FRONT_MATTER_BYTES: usize = 8 * 1024;
const MAX_DESCRIPTION_CHARS: usize = 400;

type Cache = Mutex<HashMap<(String, Option<PathBuf>), (Instant, Option<Value>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// The `description:` of a SKILL.md's front matter, a folded (`>-`, `|`) value joined into one line.
fn description(skill_file: &Path) -> Option<String> {
    let bytes = std::fs::read(skill_file).ok()?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(FRONT_MATTER_BYTES)]).into_owned();
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut lines = lines.take_while(|line| line.trim() != "---").peekable();
    while let Some(line) = lines.next() {
        let Some(value) = line.strip_prefix("description:") else {
            continue;
        };
        let value = value.trim();
        let text = if value.is_empty() || matches!(value, ">" | ">-" | "|" | "|-") {
            let mut parts = Vec::new();
            while let Some(next) = lines.next_if(|next| next.starts_with([' ', '\t'])) {
                parts.push(next.trim());
            }
            parts.join(" ")
        } else {
            value.trim_matches(['"', '\'']).to_string()
        };
        let text: String = text.chars().take(MAX_DESCRIPTION_CHARS).collect();
        return (!text.is_empty()).then_some(text);
    }
    None
}

fn read(paths: &GxserverPaths, agent: &str, folder: Option<&Path>) -> Option<Value> {
    let listed = crate::session_chat_skills::read_session_chat_skills(paths, agent, folder, None);
    let available: Vec<Value> = listed["skills"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|skill| {
            let name = skill["name"].as_str()?;
            let path = skill["skillFilePath"].as_str();
            let scope = match skill["sourceKind"].as_str() {
                Some("repository") => "project",
                Some("pluginCache") => "plugin",
                _ => "user",
            };
            let description = skill["description"]
                .as_str()
                .map(str::to_string)
                .or_else(|| path.and_then(|path| description(Path::new(path))));
            Some(json!({
                "name": name,
                "description": description,
                "tokens": Value::Null,
                "scope": scope,
                "path": path,
            }))
        })
        .collect();
    (!available.is_empty()).then(|| {
        json!({
            "tracksUse": false,
            "used": [],
            "available": available,
        })
    })
}

/// `{tracksUse: false, used: [], available}` for `agent` in `folder`, or `None` when it has no
/// skills. Blocking; cached a minute.
pub(super) fn picker_skills(
    paths: &GxserverPaths,
    agent: &str,
    folder: Option<&Path>,
) -> Option<Value> {
    let key = (agent.to_string(), folder.map(Path::to_path_buf));
    if let Some((_, value)) = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
        .filter(|(at, _)| at.elapsed() < FRESH_FOR)
    {
        return value;
    }
    let value = read(paths, agent, folder);
    if let Ok(mut cache) = cache().lock() {
        cache.retain(|_, (at, _)| at.elapsed() < FRESH_FOR);
        cache.insert(key, (Instant::now(), value.clone()));
    }
    value
}
