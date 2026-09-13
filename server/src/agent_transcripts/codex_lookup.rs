use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::resume_lookup::{codex_homes, codex_transcript_paths};

const FOUND_TTL: Duration = Duration::from_secs(10);
const MISSING_TTL: Duration = Duration::from_secs(2);
const MAX_LOOKUPS: usize = 256;
const MAX_CANDIDATES: usize = 32;

struct Lookup {
    homes: Vec<PathBuf>,
    session_id: String,
    result: Arc<Mutex<Option<LookupResult>>>,
}

struct LookupResult {
    fetched_at: Instant,
    candidates: Vec<PathBuf>,
}

/// CDXC:SessionChat 2026-09-13 WHY:
/// HTTP reads and the question watcher repeatedly searched every Codex history directory for the same identity.
/// Cache only the fallback lookup, with configured homes in the key, file validation on hits, and short negative expiry for newly created rollouts.
pub(super) fn find(session_id: &str) -> Option<PathBuf> {
    let homes = codex_homes();
    static CACHE: OnceLock<Mutex<VecDeque<Lookup>>> = OnceLock::new();
    let flight = {
        let mut cache = CACHE
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(index) = cache
            .iter()
            .position(|entry| entry.session_id == session_id && entry.homes == homes)
        {
            let entry = cache.remove(index).unwrap();
            let result = entry.result.clone();
            cache.push_back(entry);
            result
        } else {
            if cache.len() >= MAX_LOOKUPS {
                cache.pop_front();
            }
            let result = Arc::new(Mutex::new(None));
            cache.push_back(Lookup {
                homes: homes.clone(),
                session_id: session_id.to_owned(),
                result: result.clone(),
            });
            result
        }
    };
    // A slow directory scan blocks only a duplicate request for this identity.
    let mut cached = flight
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(entry) = cached.as_ref().filter(|entry| {
        entry.fetched_at.elapsed()
            < if entry.candidates.is_empty() {
                MISSING_TTL
            } else {
                FOUND_TTL
            }
    }) {
        if entry.candidates.iter().all(|path| path.is_file()) {
            return super::newest_file(entry.candidates.clone());
        }
    }
    let candidates = homes
        .iter()
        .flat_map(|home| codex_transcript_paths(home, session_id, None))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    let result = super::newest_file(candidates.clone());
    *cached = (candidates.len() <= MAX_CANDIDATES).then(|| LookupResult {
        fetched_at: Instant::now(),
        candidates,
    });
    result
}
