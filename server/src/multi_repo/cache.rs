//! A short-lived cache of [`super::discover::discover`] answers, so a burst of requests (a
//! Changes refresh, the diff counts, the tray) reads the disk once, and presentation can publish
//! `repos` from memory alone. The background pass (`refresh`) re-reads every project folder each
//! minute and reports the ones whose repos changed, so a repo cloned into a folder shows up
//! without a restart.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::RepoSet;

/// How long an answer is reused on the request path.
const FRESH_FOR: Duration = Duration::from_secs(5);

type Entry = (Instant, Option<Arc<RepoSet>>);

fn cache() -> &'static Mutex<HashMap<String, Entry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

fn store(path: &str, set: Option<Arc<RepoSet>>) {
    if let Ok(mut cache) = cache().lock() {
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key(path), (Instant::now(), set));
    }
}

/// The repos `path` covers, from the cache when fresh, else read from disk now.
pub fn repo_set(path: &str) -> Option<Arc<RepoSet>> {
    let path = key(path);
    if path.is_empty() {
        return None;
    }
    if let Ok(cache) = cache().lock() {
        if let Some((at, set)) = cache.get(&path) {
            if at.elapsed() < FRESH_FOR {
                return set.clone();
            }
        }
    }
    let set = super::discover::discover(Path::new(&path)).map(Arc::new);
    store(&path, set.clone());
    set
}

/// The cached answer for `path`, however old, without reading the disk.
pub fn cached_repo_set(path: &str) -> Option<Arc<RepoSet>> {
    cache().lock().ok()?.get(&key(path))?.1.clone()
}

/// Whether `path` has a cached answer (with or without repos).
pub fn is_cached(path: &str) -> bool {
    cache()
        .lock()
        .map(|cache| cache.contains_key(&key(path)))
        .unwrap_or(true)
}

/// Forgets `path`, so the next read goes to the disk (after making or removing a session folder).
pub fn forget(path: &str) {
    if let Ok(mut cache) = cache().lock() {
        cache.remove(&key(path));
    }
}

/// Re-reads every path and returns the ones whose repos changed since the last answer.
/// Blocking; for the background pass.
pub fn refresh(paths: &[String]) -> Vec<String> {
    let mut changed = Vec::new();
    for path in paths {
        let before = cached_repo_set(path);
        let had = is_cached(path);
        let after = super::discover::discover(Path::new(&key(path))).map(Arc::new);
        let publishes = |set: &Option<Arc<RepoSet>>| {
            set.as_ref()
                .filter(|set| set.is_multi())
                .map(|set| set.repos.clone())
        };
        if had && publishes(&before) != publishes(&after) {
            changed.push(path.clone());
        }
        store(path, after);
    }
    changed
}

/// Reads `path` once when it has no answer yet (a project's first sighting), so it reaches the
/// sidebar with its repos already on it.
pub fn ensure(path: &str) {
    if !is_cached(path) {
        let _ = repo_set(path);
    }
}
