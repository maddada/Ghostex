//! Antigravity's own list of trusted folders (`trustedWorkspaces` in
//! `~/.gemini/antigravity-cli/settings.json`). Answering agy's "Do you trust the contents of this
//! project?" with yes saves the folder there. Where Ghostex answers it only for its own run and must
//! not leave the folder trusted for every later agy session, it takes that one entry out again.
//!
//! WHY: the model list read (agent_model_pickers.rs) answered yes in each server's scratch folder,
//! and every throwaway test server has its own, so the user's agy list had collected 297 dead
//! `/private/tmp/gxts-*/cache/model-picker-scratch` entries; and Ask Ghostex AI's own chat at the home
//! folder would leave `~` trusted for every agy session started there (review of 9b7b299f8).
//!
//! The entry is cut out of the file's text, so everything else in it (its order, spacing and other
//! settings) stays byte for byte as agy wrote it, and the result is checked to read as the same
//! settings without that one entry before it is saved.

use std::path::{Path, PathBuf};

use serde_json::Value;

const LIST_KEY: &str = "trustedWorkspaces";

/// Antigravity's settings file.
pub(crate) fn agy_settings_file(home: &Path) -> PathBuf {
    home.join(".gemini")
        .join("antigravity-cli")
        .join("settings.json")
}

/// How agy writes a folder in its list: its real path.
pub(crate) fn agy_folder_key(folder: &Path) -> String {
    std::fs::canonicalize(folder)
        .unwrap_or_else(|_| folder.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn list(settings: &Value) -> Vec<&str> {
    settings
        .get(LIST_KEY)
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Whether agy trusts `folder_key` now (`false` when there is no settings file).
pub(crate) fn agy_trusts(home: &Path, folder_key: &str) -> bool {
    std::fs::read_to_string(agy_settings_file(home))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .is_some_and(|settings| list(&settings).contains(&folder_key))
}

/// The byte spans of the list's string entries, in order, and the span inside its brackets.
fn entry_spans(text: &str) -> Option<(Vec<(usize, usize)>, (usize, usize))> {
    let key = format!("\"{LIST_KEY}\"");
    let after_key = text.find(&key)? + key.len();
    let rest = text[after_key..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let open = text.len() - rest.len();
    if !rest.starts_with('[') {
        return None;
    }
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b']' => return Some((spans, (open + 1, index))),
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                spans.push((start, index + 1));
            }
            b',' | b' ' | b'\t' | b'\r' | b'\n' => {}
            // Anything but strings in the list: leave the file alone.
            _ => return None,
        }
        index += 1;
    }
    None
}

/// `text` without the first `folder_key` entry of the list, or `None` when it is not there or the
/// text can't be cut safely.
fn without_entry(text: &str, folder_key: &str) -> Option<String> {
    let (spans, (inside_start, inside_end)) = entry_spans(text)?;
    let position = spans.iter().position(|(start, end)| {
        serde_json::from_str::<String>(&text[*start..*end])
            .ok()
            .as_deref()
            == Some(folder_key)
    })?;
    let (start, end) = spans[position];
    let (cut_start, cut_end) = if let Some((next_start, _)) = spans.get(position + 1) {
        // Up to the next entry: this entry, its comma and the line break and indent after it.
        (start, *next_start)
    } else if position > 0 {
        // The last entry: from the end of the one before it, taking that one's comma.
        (spans[position - 1].1, end)
    } else {
        (inside_start, inside_end)
    };
    let mut next = String::with_capacity(text.len());
    next.push_str(&text[..cut_start]);
    next.push_str(&text[cut_end..]);
    // The same settings, less that one entry, or nothing is written.
    let mut expected: Value = serde_json::from_str(text).ok()?;
    let entries = expected.get_mut(LIST_KEY)?.as_array_mut()?;
    let index = entries
        .iter()
        .position(|entry| entry.as_str() == Some(folder_key))?;
    entries.remove(index);
    (serde_json::from_str::<Value>(&next).ok()? == expected).then_some(next)
}

/// Takes `folder_key` out of agy's trusted list, leaving the rest of the file exactly as it was.
/// Answers whether it was there and is gone now.
pub(crate) fn agy_forget(home: &Path, folder_key: &str) -> bool {
    let file = agy_settings_file(home);
    let Ok(text) = std::fs::read_to_string(&file) else {
        return false;
    };
    let Some(next) = without_entry(&text, folder_key) else {
        return false;
    };
    let staged = file.with_extension("json.ghostex-tmp");
    let permissions = std::fs::metadata(&file).ok().map(|meta| meta.permissions());
    if std::fs::write(&staged, next).is_err() {
        return false;
    }
    if let Some(permissions) = permissions {
        let _ = std::fs::set_permissions(&staged, permissions);
    }
    std::fs::rename(&staged, &file).is_ok()
}
