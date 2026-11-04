//! Reading and saving the shared settings file (`native-sidebar-settings.json`).

use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value};

use crate::durable_file::{remove_stale_temporary_files, retry_while_busy, write_durably};

/// The last good copy is refreshed at most this often.
const BACKUP_INTERVAL: Duration = Duration::from_secs(60 * 60);
const MAX_CORRUPT_COPIES: usize = 5;
/// An editor that saves in place empties the file first; a read caught in that moment is not damage.
const IN_PLACE_SAVE_GRACE: Duration = Duration::from_millis(100);

#[derive(Debug, Default)]
pub struct SettingsRead {
    /// `None` when there is no settings file: a first run, or a damaged file with no good copy to
    /// restore, which has been set aside.
    pub object: Option<Map<String, Value>>,
    /// The bytes `object` was parsed from.
    pub bytes: Vec<u8>,
    /// Damage found and what was done about it, for the caller's error log. Empty almost always.
    pub problems: Vec<String>,
}

/// Reads the settings file, repairing it first when it exists but is not a JSON object.
///
/// An error means the system refused the read, so the settings are unknown: the caller must not
/// treat that as empty, and must not save anything built from it. A missing file is not an error.
///
/// CDXC:Settings 2026-10-31 WHY:
/// After the power-loss wipe the app read the damaged file as an empty object, as if it were a first run, and the next save wrote those defaults over everything. A damaged file is now copied to `<file>.corrupt-<time>` (the newest five are kept) and replaced by `<file>.bak`. Restoring the last good copy is crash recovery, not a fallback hiding a bug of ours: the cause is outside the app, and it costs at most the last hour of changes (see [`save`]). With no good copy the file is set aside and Ghostex starts on defaults, so nothing is overwritten. A missing file is deliberately not restored, because deleting it is how a person starts over.
/// SEE-ALSO: packages/paths/src/durable_file.rs.
pub fn read(path: &Path) -> io::Result<SettingsRead> {
    let bytes = match retry_while_busy(|| fs::read(path)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(SettingsRead::default()),
        Err(error) => return Err(error),
    };
    match parse_object(&bytes) {
        Some(object) => Ok(SettingsRead {
            object: Some(object),
            bytes,
            problems: Vec::new(),
        }),
        None => repair(path),
    }
}

/// Saves `bytes` as the settings file through [`write_durably`], returning problems for the
/// caller's error log. It refuses when the current file cannot be read, and keeps a copy of a
/// current file that does not parse, so nothing is ever written over a file it could not check.
///
/// CDXC:Settings 2026-10-31 WHY:
/// `<file>.bak` holds the file as it was on disk before this save, already proven to parse, and is refreshed at most hourly. Mirroring every save would let one bad save destroy the only copy straight away, and would double the flushes of a burst of saves such as stepping the zoom.
pub fn save(path: &Path, bytes: &[u8]) -> io::Result<Vec<String>> {
    let _lock = lock(path);
    let mut problems = Vec::new();
    match retry_while_busy(|| fs::read(path)) {
        Ok(previous) if parse_object(&previous).is_some() => {
            refresh_backup(path, &previous, &mut problems)
        }
        Ok(previous) => {
            let copy = keep_corrupt_copy(path, &previous)?;
            problems.push(format!(
                "{} was damaged ({} bytes) when a save arrived; kept it as {}.",
                file_name(path),
                previous.len(),
                file_name(&copy)
            ));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    write_durably(path, bytes)?;
    Ok(problems)
}

fn refresh_backup(path: &Path, previous: &[u8], problems: &mut Vec<String>) {
    let backup = sibling(path, ".bak");
    let due = fs::metadata(&backup)
        .and_then(|metadata| metadata.modified())
        .map_or(true, |modified| {
            modified
                .elapsed()
                .map_or(true, |age| age >= BACKUP_INTERVAL)
        });
    if !due {
        return;
    }
    if let Err(error) = write_durably(&backup, previous) {
        problems.push(format!("could not refresh {}: {error}", file_name(&backup)));
    }
    remove_stale_temporary_files(path, BACKUP_INTERVAL);
}

fn repair(path: &Path) -> io::Result<SettingsRead> {
    std::thread::sleep(IN_PLACE_SAVE_GRACE);
    let _lock = lock(path);
    // Another thread or process may have finished the same repair while this one waited.
    let bytes = match retry_while_busy(|| fs::read(path)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(SettingsRead::default()),
        Err(error) => return Err(error),
    };
    if let Some(object) = parse_object(&bytes) {
        return Ok(SettingsRead {
            object: Some(object),
            bytes,
            problems: Vec::new(),
        });
    }
    let copy = keep_corrupt_copy(path, &bytes)?;
    let backup = sibling(path, ".bak");
    let good_copy = fs::read(&backup).ok().and_then(|backup_bytes| {
        let object = parse_object(&backup_bytes)?;
        Some((object, backup_bytes))
    });
    let Some((object, backup_bytes)) = good_copy else {
        retry_while_busy(|| fs::remove_file(path))?;
        return Ok(SettingsRead {
            object: None,
            bytes: Vec::new(),
            problems: vec![format!(
                "{} was damaged ({} bytes) and {} holds no good copy; kept the damaged file as {} and started on defaults.",
                file_name(path),
                bytes.len(),
                file_name(&backup),
                file_name(&copy)
            )],
        });
    };
    write_durably(path, &backup_bytes)?;
    let problems = vec![format!(
        "{} was damaged ({} bytes); kept it as {} and restored {} ({} settings).",
        file_name(path),
        bytes.len(),
        file_name(&copy),
        file_name(&backup),
        object.len()
    )];
    Ok(SettingsRead {
        object: Some(object),
        bytes: backup_bytes,
        problems,
    })
}

fn parse_object(bytes: &[u8]) -> Option<Map<String, Value>> {
    // Windows PowerShell's `Set-Content -Encoding UTF8` and old Notepad save a byte order mark.
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match serde_json::from_slice(bytes).ok()? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

/// Copies damaged bytes to `<file>.corrupt-<time>`, reusing the newest copy when it already holds
/// these bytes so a repair that keeps failing cannot fill the folder, and keeps the newest few.
fn keep_corrupt_copy(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    let mut copies = corrupt_copies(path);
    if let Some(newest) = copies.last() {
        if fs::read(newest).is_ok_and(|existing| existing == bytes) {
            return Ok(newest.clone());
        }
    }
    let stamp = utc_stamp(SystemTime::now());
    let mut copy = sibling(path, &format!(".corrupt-{stamp}"));
    let mut attempt = 1;
    while copy.exists() {
        copy = sibling(path, &format!(".corrupt-{stamp}-{attempt}"));
        attempt += 1;
    }
    write_durably(&copy, bytes)?;
    copies.push(copy.clone());
    let excess = copies.len().saturating_sub(MAX_CORRUPT_COPIES);
    for old in copies.drain(..excess) {
        let _ = fs::remove_file(old);
    }
    Ok(copy)
}

/// The existing corrupt copies, oldest first: their time stamps sort as text.
fn corrupt_copies(path: &Path) -> Vec<PathBuf> {
    let prefix = format!("{}.corrupt-", file_name(path));
    let mut copies: Vec<PathBuf> = path
        .parent()
        .and_then(|parent| fs::read_dir(parent).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect();
    copies.sort();
    copies
}

/// Serializes saves and repairs between threads and processes, because gxserver, the CLI and the
/// app can all find the same damage at once. Released when the file is closed, so a crash cannot
/// leave it stuck. Best effort: a filesystem without locks must not stop settings being saved.
fn lock(path: &Path) -> Option<fs::File> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(sibling(path, ".lock"))
        .ok()?;
    file.lock().ok()?;
    Some(file)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `20261031-131335`: UTC, and usable in a file name on every platform.
fn utc_stamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        second_of_day / 3_600,
        second_of_day % 3_600 / 60,
        second_of_day % 60
    )
}
