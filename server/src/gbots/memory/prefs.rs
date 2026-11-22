//! The shared preferences, `<app data>/gbots/PREFERENCES.md`: what every GBot should know about the
//! user (how they like answers, names, time zone). Every bot's brief and generated instruction
//! files include them; only the user and Chief of Staff change them (`ghostex gbots prefs set`).

use serde_json::{json, Value};

use crate::domain::DomainStateError;
use crate::gbots::folder::{gbots_dir, read_preferences, write_atomic, PREFERENCES_FILE};
use crate::paths::GxserverPaths;

/// Lines of shared preferences that load into a bot.
pub(crate) const PREFERENCES_MAX_LINES: usize = 100;
const PREFERENCES_MAX_BYTES: usize = 16 * 1024;

pub(crate) fn show(paths: &GxserverPaths) -> Value {
    let text = read_preferences(paths);
    json!({
        "text": text,
        "lines": text.lines().count(),
        "maxLines": PREFERENCES_MAX_LINES,
        "path": gbots_dir(paths).join(PREFERENCES_FILE),
    })
}

/// Saves `text` (secrets redacted) and answers the new state with how many secrets were hidden.
pub(crate) fn set(paths: &GxserverPaths, text: &str) -> Result<Value, DomainStateError> {
    let (text, redacted) = super::redact::redact(text.trim());
    if text.lines().count() > PREFERENCES_MAX_LINES {
        return Err(DomainStateError::bad_request(format!(
            "Keep the shared preferences to {PREFERENCES_MAX_LINES} lines: every GBot reads all of them."
        )));
    }
    if text.len() > PREFERENCES_MAX_BYTES {
        return Err(DomainStateError::bad_request(format!(
            "Keep the shared preferences under {} KB: every GBot reads all of them.",
            PREFERENCES_MAX_BYTES / 1024
        )));
    }
    let body = if text.is_empty() {
        String::new()
    } else {
        format!("{text}\n")
    };
    write_atomic(&gbots_dir(paths).join(PREFERENCES_FILE), &body)
        .map_err(super::store::internal)?;
    let mut answer = show(paths);
    answer["redacted"] = json!(redacted);
    Ok(answer)
}
