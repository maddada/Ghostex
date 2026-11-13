use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::UNIX_EPOCH,
};

use rand::TryRng;
use serde_json::json;

/// The largest prepared page gxserver stores, local images included.
pub(crate) const VISUAL_PAGE_MAX_BYTES: usize = 8 * 1024 * 1024;
/// Titles are card labels; anything longer is cut.
pub(super) const VISUAL_PAGE_TITLE_MAX_CHARS: usize = 200;
const VISUAL_PAGE_SESSION_REF_MAX_CHARS: usize = 200;
const VISUAL_PAGES_DIR_NAME: &str = "visual-pages";
/// Held while a page's record is read and rewritten, so two answers sent at once cannot both
/// pass the "not answered yet" check.
static RECORD_WRITE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn visual_pages_dir(gxserver_state_dir: &Path) -> PathBuf {
    gxserver_state_dir.join(VISUAL_PAGES_DIR_NAME)
}

/// CDXC:ServerDaemon 2026-11-03 DECISION:
/// User chose: a published page is deleted 90 days after it was published, with its record and its card picture; its card in an old chat then no longer opens.
const PAGE_MAX_AGE: std::time::Duration =
    std::time::Duration::from_secs(90 * crate::storage_cleanup::DAY.as_secs());

/// Deletes every page published more than [`PAGE_MAX_AGE`] ago (`<id>.html`, `<id>.json`,
/// `<id>.png`), measured from the page file, which is written once when the page is published.
pub(crate) fn prune_old_pages(
    gxserver_state_dir: &Path,
    now: std::time::SystemTime,
) -> crate::storage_cleanup::Tally {
    use crate::storage_cleanup::{is_older, remove_file, Since, Tally};
    let dir = visual_pages_dir(gxserver_state_dir);
    let mut tally = Tally::default();
    let Ok(entries) = fs::read_dir(&dir) else {
        return tally;
    };
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".html").filter(|id| is_page_id(id)) else {
            continue;
        };
        let page = entry.path();
        let old = fs::metadata(&page)
            .is_ok_and(|metadata| is_older(&metadata, Since::Modified, PAGE_MAX_AGE, now));
        if !old {
            continue;
        }
        for extension in ["html", "json", "png"] {
            let path = dir.join(format!("{id}.{extension}"));
            if let Ok(metadata) = fs::metadata(&path) {
                remove_file(&path, &metadata, &mut tally);
            }
        }
    }
    tally
}

/// 128 random bits from the operating system's generator, as 32 lowercase hex characters.
pub(super) fn new_page_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system's random generator failed");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn is_page_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn clip_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Writes `<id>.html` and its `<id>.json` record and returns the new id. `source` is the HTML file
/// the page was prepared from, whose path and modification time the record keeps for Refresh.
pub(super) fn write_page(
    gxserver_state_dir: &Path,
    title: &str,
    html: &str,
    session_ref: Option<&str>,
    source: Option<&Path>,
) -> std::io::Result<String> {
    let dir = visual_pages_dir(gxserver_state_dir);
    fs::create_dir_all(&dir)?;
    let id = new_page_id();
    // A page published from now on may be pictured for its card before anyone opens it; pages
    // published before had no such mark and never are (`thumbnail.rs`).
    let mut record = json!({
        "title": title,
        "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "pictureInBackground": true,
        "sessionRef": session_ref
            .map(|value| clip_chars(value, VISUAL_PAGE_SESSION_REF_MAX_CHARS)),
    });
    if let Some(source) = source {
        record["sourcePath"] = json!(source.to_string_lossy());
        record["sourceModifiedMs"] = json!(source_modified_ms(source));
    }
    fs::write(dir.join(format!("{id}.html")), html)?;
    fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_vec_pretty(&record).unwrap_or_default(),
    )?;
    Ok(id)
}

/// When the file at `path` was last changed, in milliseconds since the epoch.
pub(super) fn source_modified_ms(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let millis = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    u64::try_from(millis).ok()
}

/// Replaces a published page's HTML with its file prepared again (`source.rs`), keeping its id,
/// and notes the file's time on the record. The page's picture is marked out of date so the
/// desktop takes a new one the next time it shows the page.
pub(super) fn replace_page_html(
    gxserver_state_dir: &Path,
    id: &str,
    html: &str,
    source_modified_ms: Option<u64>,
) -> Result<(), StoreAnswersError> {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut record = read_record(gxserver_state_dir, id).ok_or(StoreAnswersError::NotFound)?;
    let path = visual_pages_dir(gxserver_state_dir).join(format!("{id}.html"));
    let temporary = path.with_extension("html.tmp");
    fs::write(&temporary, html)
        .and_then(|()| fs::rename(&temporary, &path))
        .map_err(StoreAnswersError::Io)?;
    record["sourceModifiedMs"] = json!(source_modified_ms);
    record["refreshedAt"] =
        json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    record["thumbnailOutdated"] = json!(true);
    write_record(gxserver_state_dir, id, &record).map_err(StoreAnswersError::Io)
}

pub(super) fn read_page(gxserver_state_dir: &Path, id: &str) -> Option<Vec<u8>> {
    if !is_page_id(id) {
        return None;
    }
    fs::read(visual_pages_dir(gxserver_state_dir).join(format!("{id}.html"))).ok()
}

fn record_path(gxserver_state_dir: &Path, id: &str) -> PathBuf {
    visual_pages_dir(gxserver_state_dir).join(format!("{id}.json"))
}

/// The page's `<id>.json` record: its title, the session that published it, and its answers once
/// the reader sends them.
pub(super) fn read_record(gxserver_state_dir: &Path, id: &str) -> Option<serde_json::Value> {
    if !is_page_id(id) {
        return None;
    }
    let bytes = fs::read(record_path(gxserver_state_dir, id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(super) enum StoreAnswersError {
    NotFound,
    /// The page already holds answers, sent at this time.
    AlreadyAnswered(String),
    Io(std::io::Error),
}

/// Stores the reader's answers on the page's record, once: the first submission wins, so a page
/// cannot type a stream of answers into the agent.
pub(super) fn store_answers(
    gxserver_state_dir: &Path,
    id: &str,
    answers: &serde_json::Value,
) -> Result<serde_json::Value, StoreAnswersError> {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut record = read_record(gxserver_state_dir, id).ok_or(StoreAnswersError::NotFound)?;
    if let Some(answered_at) = record.get("answeredAt").and_then(serde_json::Value::as_str) {
        return Err(StoreAnswersError::AlreadyAnswered(answered_at.to_string()));
    }
    record["answers"] = answers.clone();
    record["answeredAt"] =
        json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    // The answers replace the draft they were written from.
    if let Some(fields) = record.as_object_mut() {
        fields.remove("draft");
        fields.remove("draftEditedAt");
    }
    write_record(gxserver_state_dir, id, &record).map_err(StoreAnswersError::Io)?;
    Ok(record)
}

/// Keeps what the reader has picked and written so far on the page's record, so closing and
/// reopening the page (or a restart) brings it back; `null` forgets it. A page that was answered
/// keeps its answers instead.
///
/// `edited_at` is when the reader made the edit the draft ends with (the page's clock, in ms),
/// kept as `draftEditedAt`. A draft from an earlier edit than the one stored is not written and
/// `Ok(false)` says so: an older save that lands late (a retry, the beacon of a page that went
/// away) never replaces newer text. A draft without one (an older page script, the phone's
/// pending draft) is written as before.
pub(super) fn store_draft(
    gxserver_state_dir: &Path,
    id: &str,
    draft: &serde_json::Value,
    edited_at: Option<u64>,
) -> Result<bool, StoreAnswersError> {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut record = read_record(gxserver_state_dir, id).ok_or(StoreAnswersError::NotFound)?;
    if let Some(answered_at) = record.get("answeredAt").and_then(serde_json::Value::as_str) {
        return Err(StoreAnswersError::AlreadyAnswered(answered_at.to_string()));
    }
    let stored_at = record
        .get("draftEditedAt")
        .and_then(serde_json::Value::as_u64);
    if let (Some(edited_at), Some(stored_at)) = (edited_at, stored_at) {
        if edited_at < stored_at {
            return Ok(false);
        }
    }
    let Some(fields) = record.as_object_mut() else {
        return Err(StoreAnswersError::NotFound);
    };
    if draft.is_null() {
        fields.remove("draft");
    } else {
        fields.insert("draft".to_string(), draft.clone());
    }
    match edited_at {
        Some(edited_at) => {
            fields.insert("draftEditedAt".to_string(), json!(edited_at));
        }
        None => {
            fields.remove("draftEditedAt");
        }
    }
    write_record(gxserver_state_dir, id, &record).map_err(StoreAnswersError::Io)?;
    Ok(true)
}

/// Keeps where the reader left the page (`scroll.rs`), in pixels from its top. An unchanged
/// position is not written again.
pub(super) fn store_scroll(
    gxserver_state_dir: &Path,
    id: &str,
    y: u64,
) -> Result<(), StoreAnswersError> {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut record = read_record(gxserver_state_dir, id).ok_or(StoreAnswersError::NotFound)?;
    if record.get("scrollY").and_then(serde_json::Value::as_u64) == Some(y) {
        return Ok(());
    }
    record["scrollY"] = json!(y);
    write_record(gxserver_state_dir, id, &record).map_err(StoreAnswersError::Io)
}

/// Notes on the record that the reader closed the old-layout notice (notice.rs). Closing it again
/// keeps the first time.
pub(super) fn dismiss_layout_notice(
    gxserver_state_dir: &Path,
    id: &str,
) -> Result<(), StoreAnswersError> {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut record = read_record(gxserver_state_dir, id).ok_or(StoreAnswersError::NotFound)?;
    if record.get("layoutNoticeDismissedAt").is_some() {
        return Ok(());
    }
    record["layoutNoticeDismissedAt"] =
        json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    write_record(gxserver_state_dir, id, &record).map_err(StoreAnswersError::Io)
}

/// Marks a page just published as waiting until the reader marks it seen (`awaitsSeen`,
/// publish.rs).
pub(super) fn note_awaits_seen(
    gxserver_state_dir: &Path,
    id: &str,
) -> Result<(), StoreAnswersError> {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut record = read_record(gxserver_state_dir, id).ok_or(StoreAnswersError::NotFound)?;
    record["awaitsSeen"] = json!(true);
    write_record(gxserver_state_dir, id, &record).map_err(StoreAnswersError::Io)
}

/// Replaces the record whole (written beside it, then renamed), so a reader never sees half.
fn write_record(
    gxserver_state_dir: &Path,
    id: &str,
    record: &serde_json::Value,
) -> std::io::Result<()> {
    let path = record_path(gxserver_state_dir, id);
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(record).unwrap_or_default(),
    )
    .and_then(|()| fs::rename(&temporary, &path))
}

/// Notes on the record that a picture of the page was just stored (`thumbnail.rs`), and whether the
/// page was already answered when it was taken. Returns the updated record.
pub(super) fn note_thumbnail(
    gxserver_state_dir: &Path,
    id: &str,
    answered: bool,
) -> Result<serde_json::Value, StoreAnswersError> {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut record = read_record(gxserver_state_dir, id).ok_or(StoreAnswersError::NotFound)?;
    record["thumbnailAt"] =
        json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    record["thumbnailAnswered"] = json!(answered);
    if let Some(fields) = record.as_object_mut() {
        fields.remove("thumbnailOutdated");
    }
    write_record(gxserver_state_dir, id, &record).map_err(StoreAnswersError::Io)?;
    Ok(record)
}

/// Notes on the record how handing the answers to the agent went: queued (`queuedAt`) or why not.
pub(super) fn note_delivery(gxserver_state_dir: &Path, id: &str, error: Option<&str>) {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(mut record) = read_record(gxserver_state_dir, id) else {
        return;
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    match error {
        None => record["queuedAt"] = json!(now),
        Some(error) => record["deliveryError"] = json!(clip_chars(error, 500)),
    }
    let _ = write_record(gxserver_state_dir, id, &record);
}

/// Marks a page made by opening its file, which no session published (`file_page.rs`).
pub(super) fn mark_opened_from_file(gxserver_state_dir: &Path, id: &str) {
    let _guard = RECORD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(mut record) = read_record(gxserver_state_dir, id) else {
        return;
    };
    record[super::file_page::OPENED_FROM_FILE_KEY] = json!(true);
    let _ = write_record(gxserver_state_dir, id, &record);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    fn page() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(visual_pages_dir(dir.path())).unwrap();
        write_record(dir.path(), ID, &json!({"title": "t"})).unwrap();
        dir
    }

    fn notes(dir: &Path) -> serde_json::Value {
        read_record(dir, ID).unwrap()["draft"]["notes"].clone()
    }

    #[test]
    fn a_draft_from_an_earlier_edit_never_replaces_a_later_one() {
        let dir = page();
        assert!(matches!(
            store_draft(dir.path(), ID, &json!({"notes": "long text"}), Some(2_000)),
            Ok(true)
        ));
        // The late retry or beacon of an older edit.
        assert!(matches!(
            store_draft(dir.path(), ID, &json!({"notes": "long"}), Some(1_000)),
            Ok(false)
        ));
        assert_eq!(notes(dir.path()), "long text");
        assert_eq!(read_record(dir.path(), ID).unwrap()["draftEditedAt"], 2_000);
        // The same edit again, and a later one, are kept.
        assert!(matches!(
            store_draft(dir.path(), ID, &json!({"notes": "long text"}), Some(2_000)),
            Ok(true)
        ));
        assert!(matches!(
            store_draft(
                dir.path(),
                ID,
                &json!({"notes": "long text, more"}),
                Some(3_000)
            ),
            Ok(true)
        ));
        assert_eq!(notes(dir.path()), "long text, more");
    }

    #[test]
    fn a_draft_without_an_edit_time_is_written_and_drops_the_old_time() {
        let dir = page();
        store_draft(dir.path(), ID, &json!({"notes": "a"}), Some(5_000)).ok();
        assert!(matches!(
            store_draft(dir.path(), ID, &json!({"notes": "from the phone"}), None),
            Ok(true)
        ));
        assert_eq!(notes(dir.path()), "from the phone");
        assert!(read_record(dir.path(), ID)
            .unwrap()
            .get("draftEditedAt")
            .is_none());
        assert!(matches!(
            store_draft(dir.path(), ID, &json!({"notes": "b"}), Some(1)),
            Ok(true)
        ));
    }

    #[test]
    fn answers_replace_the_draft_and_its_edit_time() {
        let dir = page();
        store_draft(dir.path(), ID, &json!({"notes": "a"}), Some(5_000)).ok();
        store_answers(dir.path(), ID, &json!({"q": "a"})).ok();
        let record = read_record(dir.path(), ID).unwrap();
        assert!(record.get("draft").is_none());
        assert!(record.get("draftEditedAt").is_none());
        assert!(matches!(
            store_draft(dir.path(), ID, &json!({"notes": "late"}), Some(9_000)),
            Err(StoreAnswersError::AlreadyAnswered(_))
        ));
    }
}
