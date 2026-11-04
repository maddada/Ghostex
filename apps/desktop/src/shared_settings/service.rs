use std::{
    collections::hash_map::DefaultHasher,
    env, fs,
    hash::Hasher as _,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "macos")]
use std::process;

use super::*;
use serde_json::{Map, Value};

static SHARED_SIDEBAR_SETTINGS_SERVICE: OnceLock<Mutex<SharedSidebarSettingsService>> =
    OnceLock::new();

/// Change key for the settings file used to skip re-reading it: mtime plus
/// length from `fs::metadata`. A stat is orders of magnitude cheaper than the
/// read+parse it replaces, which matters because `read_snapshot` runs on hot
/// paths (per-frame surface-host sync and per-call support-log scenario
/// gating) under the global service mutex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SharedSettingsFileIdentity {
    modified: SystemTime,
    len: u64,
}

impl SharedSettingsFileIdentity {
    fn from_path(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok()?,
            len: metadata.len(),
        })
    }
}

/// How long a stat of the settings file vouches for the cached snapshot.
///
/// CDXC:Settings 2026-10-23 WHY:
/// `read_snapshot` has about two hundred callers, several on per-frame and per-log paths, and each
/// call used to stat the file. Writes made through this service update the snapshot themselves and
/// drop the stat (`apply_observed_settings`), so only a write from another process (`ghostex
/// settings`, gxserver) waits, at most this long; the app's own settings polls run every two
/// seconds anyway.
const SETTINGS_FILE_RECHECK_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct SharedSidebarSettingsService {
    path: PathBuf,
    snapshot: SharedSidebarSettingsSnapshot,
    cached_file_identity: Option<SharedSettingsFileIdentity>,
    /// When `cached_file_identity` was last compared with the file.
    file_identity_checked_at: Option<Instant>,
    /// The system refused the last read, so it is retried at the next recheck.
    read_failed: bool,
    /// No read has succeeded yet, so `snapshot` is a placeholder that says nothing about the file.
    ///
    /// CDXC:Settings 2026-10-31 WHY:
    /// Every save here is read-modify-write over the snapshot, so a save built from a placeholder would replace the user's settings with its few keys, which is how a power-loss wipe left six keys in the file. Such a save is refused until a read succeeds.
    snapshot_is_placeholder: bool,
}

impl SharedSidebarSettingsService {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            snapshot: SharedSidebarSettingsSnapshot::empty(),
            cached_file_identity: None,
            file_identity_checked_at: None,
            read_failed: false,
            snapshot_is_placeholder: true,
        }
    }

    pub fn for_default_path() -> Self {
        let path = shared_sidebar_settings_path();
        maybe_import_legacy_macos_sidebar_settings(&path);
        Self::new(path)
    }

    pub fn read_snapshot(&mut self) -> SharedSidebarSettingsSnapshot {
        let now = Instant::now();
        if self.snapshot.revision > 0
            && self.file_identity_checked_at.is_some_and(|at| {
                now.saturating_duration_since(at) < SETTINGS_FILE_RECHECK_INTERVAL
            })
        {
            return self.snapshot.clone();
        }
        self.file_identity_checked_at = Some(now);
        // Stat first: when the file identity is unchanged the cached snapshot
        // is current and the read+parse is skipped entirely. The stat is taken
        // before the read so a write racing between the two only leaves a
        // stale identity behind, forcing one redundant re-read on the next
        // call instead of ever serving stale content.
        let identity = SharedSettingsFileIdentity::from_path(&self.path);
        if self.snapshot.revision > 0 && !self.read_failed && identity == self.cached_file_identity
        {
            return self.snapshot.clone();
        }
        let mut read = match read_settings_object_from_path(&self.path) {
            Ok(read) => {
                self.read_failed = false;
                self.snapshot_is_placeholder = false;
                read
            }
            Err(error) => {
                if !self.read_failed {
                    log::error!("Could not read the settings file: {error}");
                }
                self.read_failed = true;
                // A snapshot read earlier is still the truth. The first read publishes the
                // placeholder so Settings can open (see the revision comment below).
                if self.snapshot.revision == 0 {
                    self.snapshot = SharedSidebarSettingsSnapshot::with_signal(
                        Map::new(),
                        1,
                        self.snapshot.content_hash,
                    );
                }
                return self.snapshot.clone();
            }
        };
        ghostty_themes::hydrate_ghostty_theme_selections(&mut read.object);
        /*
        CDXC:Settings 2026-07-26:
        Revision is the "GPUI has observed real settings state" signal the React
        app-modal host gates on (`hasNativeSettingsHydrated = revision > 0`). A
        missing, empty, or unreadable settings file reads as empty bytes, whose
        hash equals the initial empty snapshot's hash, so revision stayed 0 and
        Settings hydrated with revision 0 — leaving the modal permanently blank
        with no way to save a first settings file from the UI. The first
        completed read must always publish revision >= 1, even when the file is
        absent and defaults are the canonical state. (Since 2026-10-31 a damaged
        file is repaired first and a refused read stays a placeholder that cannot
        be saved; only an absent file reads as empty here.)
        */
        if read.content_hash != self.snapshot.content_hash || self.snapshot.revision == 0 {
            self.snapshot = SharedSidebarSettingsSnapshot::with_signal(
                read.object,
                self.snapshot.revision.wrapping_add(1),
                read.content_hash,
            );
        }
        self.cached_file_identity = identity;
        self.snapshot.clone()
    }

    #[allow(dead_code)]
    pub fn write_json_object_payload(
        &mut self,
        payload: &str,
    ) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
        let value = serde_json::from_str::<Value>(payload)
            .map_err(|_| SharedSidebarSettingsWriteError::MalformedJson)?;
        let object = match value {
            Value::Object(object) => object,
            _ => return Err(SharedSidebarSettingsWriteError::ExpectedObject),
        };
        self.write_json_object(object)
    }

    #[allow(dead_code)]
    pub fn write_json_object(
        &mut self,
        object: Map<String, Value>,
    ) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
        if self.snapshot_is_placeholder {
            return Err(io::Error::other(
                "the settings file has not been read yet, so saving could overwrite it",
            )
            .into());
        }
        let value = Value::Object(object.clone());
        let bytes = serde_json::to_vec_pretty(&value)?;
        let existing = fs::read(&self.path).ok();

        if existing.as_deref() == Some(bytes.as_slice()) {
            let snapshot = self.apply_observed_settings(object, hash_bytes(&bytes));
            return Ok(SharedSidebarSettingsWriteResult {
                status: SharedSidebarSettingsWriteStatus::Unchanged,
                snapshot,
            });
        }

        for problem in ghostex_paths::settings_file::save(&self.path, &bytes)? {
            log::error!("Settings file: {problem}");
        }

        let snapshot = self.apply_observed_settings(object, hash_bytes(&bytes));
        Ok(SharedSidebarSettingsWriteResult {
            status: SharedSidebarSettingsWriteStatus::Changed,
            snapshot,
        })
    }

    fn apply_observed_settings(
        &mut self,
        object: Map<String, Value>,
        content_hash: u64,
    ) -> SharedSidebarSettingsSnapshot {
        // A write just went through this service, so the stat-based read cache
        // can no longer vouch for the on-disk file: mtime granularity can be
        // too coarse to distinguish a same-length rewrite. Explicitly drop the
        // cached identity so a write-then-read never serves stale data; the
        // next read_snapshot re-reads once and re-establishes the cache.
        self.cached_file_identity = None;
        self.file_identity_checked_at = None;
        if content_hash != self.snapshot.content_hash {
            self.snapshot = SharedSidebarSettingsSnapshot::with_signal(
                object,
                self.snapshot.revision.wrapping_add(1),
                content_hash,
            );
        }
        self.snapshot.clone()
    }
}

pub fn shared_sidebar_settings_snapshot() -> SharedSidebarSettingsSnapshot {
    let mut service = shared_sidebar_settings_service()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    service.read_snapshot()
}

#[allow(dead_code)]
pub fn write_shared_sidebar_settings_payload(
    payload: &str,
) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
    let mut service = shared_sidebar_settings_service()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    service.write_json_object_payload(payload)
}

pub fn write_shared_sidebar_settings_object(
    object: Map<String, Value>,
) -> Result<SharedSidebarSettingsWriteResult, SharedSidebarSettingsWriteError> {
    let mut service = shared_sidebar_settings_service()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    service.write_json_object(object)
}

fn shared_sidebar_settings_service() -> &'static Mutex<SharedSidebarSettingsService> {
    SHARED_SIDEBAR_SETTINGS_SERVICE
        .get_or_init(|| Mutex::new(SharedSidebarSettingsService::for_default_path()))
}

#[cfg(target_os = "macos")]
fn maybe_import_legacy_macos_sidebar_settings(settings_path: &Path) {
    /*
    CDXC:Settings 2026-07-12:
    Production Swift builds historically stored Settings only in WKWebView
    localStorage. Match GhostexAppStorage's one-time upgrade behavior when the
    canonical resolved sidebar settings file does not exist:
    inspect only com.madda.ghostex.host localStorage databases, choose the
    richest valid `ghostex-native-settings` object, and atomically establish
    the shared file. Never read production WK data for ~/.ghostex-dev, and
    never replace or merge an existing shared file.
    */
    if settings_path.exists() {
        return;
    }
    if env::var_os("GHOSTEX_HOME")
        .is_some_and(|value| !value.is_empty() && Path::new(&value).is_absolute())
    {
        return;
    }
    let home = &ghostex_storage_paths().home_dir;
    let webkit_root = home.join("Library/WebKit/com.madda.ghostex.host");
    let mut databases = Vec::new();
    collect_legacy_local_storage_databases(&webkit_root, &mut databases);
    let mut selected: Option<(Map<String, Value>, usize, SystemTime, PathBuf)> = None;
    for database in databases {
        let Some(object) = read_legacy_local_storage_settings_object(&database) else {
            continue;
        };
        let score = (object.len() * 1_000)
            + serde_json::to_string(&Value::Object(object.clone()))
                .map(|value| value.len())
                .unwrap_or(0);
        let modified = fs::metadata(&database)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let replace = selected
            .as_ref()
            .is_none_or(|(_, current_score, current_modified, path)| {
                score > *current_score
                    || (score == *current_score && modified > *current_modified)
                    || (score == *current_score
                        && modified == *current_modified
                        && database < *path)
            });
        if replace {
            selected = Some((object, score, modified, database));
        }
    }
    let Some((object, _, _, _)) = selected else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&Value::Object(object)) else {
        return;
    };
    if settings_path.exists() {
        return;
    }
    let _ = ghostex_paths::durable_file::write_durably(settings_path, &bytes);
}

#[cfg(not(target_os = "macos"))]
fn maybe_import_legacy_macos_sidebar_settings(_settings_path: &Path) {}

#[cfg(target_os = "macos")]
fn collect_legacy_local_storage_databases(root: &Path, databases: &mut Vec<PathBuf>) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        if databases.len() >= 128 {
            break;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path.file_name().and_then(|name| name.to_str()) == Some("localstorage.sqlite3")
            {
                databases.push(path);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn read_legacy_local_storage_settings_object(path: &Path) -> Option<Map<String, Value>> {
    let output = process::Command::new("/usr/bin/sqlite3")
        .arg("-readonly")
        .arg(path)
        .arg("select hex(value) from ItemTable where key = 'ghostex-native-settings' limit 1;")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hex = String::from_utf8(output.stdout).ok()?;
    let bytes = legacy_hex_bytes(hex.trim())?;
    legacy_settings_object_from_bytes(&bytes)
}

#[cfg(target_os = "macos")]
fn legacy_hex_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect()
}

#[cfg(target_os = "macos")]
fn legacy_settings_object_from_bytes(bytes: &[u8]) -> Option<Map<String, Value>> {
    if let Ok(value) = std::str::from_utf8(bytes)
        && let Ok(Value::Object(object)) =
            serde_json::from_str(value.trim_start_matches('\u{feff}'))
    {
        return Some(object);
    }
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let utf16 = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    let value = String::from_utf16(&utf16).ok()?;
    match serde_json::from_str(value.trim_start_matches('\u{feff}')).ok()? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

/// A damaged file comes back repaired from its last good copy; an error means the settings are
/// unknown, which must never read as empty (`ghostex_paths::settings_file::read`).
fn read_settings_object_from_path(path: &Path) -> io::Result<SharedSidebarSettingsRead> {
    let read = ghostex_paths::settings_file::read(path)?;
    for problem in &read.problems {
        log::error!("Settings file: {problem}");
    }
    Ok(SharedSidebarSettingsRead {
        object: read.object.unwrap_or_default(),
        content_hash: hash_bytes(&read.bytes),
    })
}

struct SharedSidebarSettingsRead {
    object: Map<String, Value>,
    content_hash: u64,
}

pub(crate) fn hash_settings_object(object: &Map<String, Value>) -> u64 {
    serde_json::to_vec(&Value::Object(object.clone()))
        .map(|bytes| hash_bytes(&bytes))
        .unwrap_or_else(|_| hash_bytes(&[]))
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

#[allow(dead_code)] // timestamp formatting helper kept as a pair with shared_settings_civil_from_days
fn shared_settings_iso8601_utc(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_seconds = duration.as_secs() as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = shared_settings_civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    )
}

pub(crate) fn shared_settings_civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[allow(dead_code)] // timestamp formatting helper kept as a pair with shared_settings_civil_from_days
fn shared_settings_iso8601_utc_millis_like(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 24
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'.'
        && bytes[23] == b'Z'
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19 | 23) || byte.is_ascii_digit()
        })
}
