//! The image folder (`<data dir>/i`): screenshots agents save there for their reports, and the
//! images pasted into chat messages and board cards.

use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use super::files::{is_older, prune_tree, remove_file, Since, Tally, DAY};

/// CDXC:ServerDaemon 2026-11-03 DECISION:
/// User chose: screenshots in the image folder are deleted 60 days after they were written, except folders named for something kept on purpose (`release-*`, `youtube-*`).
const SCREENSHOT_MAX_AGE: Duration = Duration::from_secs(60 * DAY.as_secs());

/// Folders under `i/` whose names start with one of these are never cleaned.
const KEPT_FOLDER_PREFIXES: &[&str] = &["release-", "youtube-"];

/// Queued chat prompt attachments, which `session_chat_queue_attachments` owns and sweeps.
const QUEUED_ATTACHMENTS_DIR: &str = "queued";

pub(crate) fn prune_images(data_dir: &Path, now: SystemTime) -> Tally {
    let dir = data_dir.join("i");
    let mut tally = Tally::default();
    let Ok(entries) = fs::read_dir(&dir) else {
        return tally;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            if name == QUEUED_ATTACHMENTS_DIR
                || KEPT_FOLDER_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            {
                continue;
            }
            tally.add(prune_tree(&path, Since::Modified, SCREENSHOT_MAX_AGE, now));
            let _ = fs::remove_dir(&path);
        } else if metadata.is_file()
            && !is_pasted_image_name(&name)
            && is_older(&metadata, Since::Modified, SCREENSHOT_MAX_AGE, now)
        {
            remove_file(&path, &metadata, &mut tally);
        }
    }
    tally
}

/// CDXC:ServerDaemon 2026-11-03 WHY:
/// An image pasted into a chat message or a board card is saved straight into `i/` as `<epoch ms>.<ext>` (or `<epoch ms>-<n>.<ext>`; `session_chat_files.rs`, `project_board_images.rs`), and the message or card still shows it however old it is, so those names are not screenshots and are kept.
fn is_pasted_image_name(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if extension.is_empty() || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return false;
    }
    let (millis, suffix) = match stem.split_once('-') {
        Some((millis, suffix)) => (millis, Some(suffix)),
        None => (stem, None),
    };
    millis.len() == 13
        && millis.bytes().all(|byte| byte.is_ascii_digit())
        && suffix.is_none_or(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}
