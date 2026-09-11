use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{self, FileType, Metadata},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone)]
pub struct Entry {
    path: PathBuf,
    name: OsString,
    metadata: Option<Metadata>,
    file_type: Option<FileType>,
}

impl Entry {
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
    pub fn file_name(&self) -> OsString {
        self.name.clone()
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        self.metadata
            .clone()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
    pub fn file_type(&self) -> io::Result<FileType> {
        self.file_type
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

struct Snapshot {
    modified: Option<SystemTime>,
    created: Instant,
    entries: Vec<Entry>,
    scanned: usize,
}

type Slot = Arc<Mutex<Option<Snapshot>>>;
static CACHE: OnceLock<Mutex<HashMap<PathBuf, Slot>>> = OnceLock::new();
const CACHE_DIRECTORIES: usize = 128;
const CACHE_DIRECTORY_ENTRIES: usize = 512;
const CACHE_AGE: Duration = Duration::from_secs(30);

/// CDXC:Docs 2026-09-11 WHY:
/// Sorting used to stat each child on every comparison, multiplying filesystem work by the number of sort comparisons.
/// Desktop and server share captured metadata and short-lived directory snapshots; a per-directory lock combines simultaneous scans without serializing unrelated projects.
pub fn children(
    directory: &Path,
    scanned: &mut usize,
    limit: usize,
    limit_error: fn() -> String,
) -> Result<Vec<Entry>, String> {
    let cache = CACHE.get_or_init(Default::default);
    let slot = {
        let mut cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !cache.contains_key(directory) && cache.len() >= CACHE_DIRECTORIES {
            cache.retain(|_, slot| Arc::strong_count(slot) > 1);
            if cache.len() >= CACHE_DIRECTORIES {
                // Active requests retain their slots; this request simply has no reusable slot.
                Arc::new(Mutex::new(None))
            } else {
                cache.entry(directory.to_path_buf()).or_default().clone()
            }
        } else {
            cache.entry(directory.to_path_buf()).or_default().clone()
        }
    };
    let mut snapshot = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let modified = fs::metadata(directory)
        .map_err(|_| "Could not list project files.".to_string())?
        .modified()
        .ok();
    if let Some(cached) = snapshot
        .as_ref()
        .filter(|cached| cached.modified == modified && cached.created.elapsed() < CACHE_AGE)
    {
        if cached.scanned > limit.saturating_sub(*scanned) {
            return Err(limit_error());
        }
        *scanned += cached.scanned;
        return Ok(cached.entries.clone());
    }
    let before = *scanned;
    let mut entries = Vec::new();
    for child in fs::read_dir(directory).map_err(|_| "Could not list project files.".to_string())? {
        if *scanned >= limit {
            return Err(limit_error());
        }
        *scanned += 1;
        if let Ok(child) = child {
            entries.push(Entry {
                path: child.path(),
                name: child.file_name(),
                metadata: child.metadata().ok(),
                file_type: child.file_type().ok(),
            });
        }
    }
    if entries.len() <= CACHE_DIRECTORY_ENTRIES {
        *snapshot = Some(Snapshot {
            modified,
            created: Instant::now(),
            scanned: *scanned - before,
            entries: entries.clone(),
        });
    } else {
        *snapshot = None;
    }
    Ok(entries)
}

pub fn invalidate() {
    if let Some(cache) = CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

/// Invalidating both sides of a mutation also drops scans that raced its write.
pub struct MutationGuard;
impl MutationGuard {
    pub fn new() -> Self {
        invalidate();
        Self
    }
}
impl Drop for MutationGuard {
    fn drop(&mut self) {
        invalidate();
    }
}
