//! Where a Save to Markdown file lands and what it is called, ported from
//! `packages/shared/session-chat-presentation/save-markdown.ts`.
//!
//! Every rule here is a path rule, so the strings it produces are part of the feature: the folder
//! it offers, the numbered stem it suggests, and the four refusals it writes back into the sheet.

use crate::extras::agent_tasks::js_trim;

/// `localDateDirectory`: today's date in the user's own timezone, `YYYY-MM-DD`.
///
/// The TypeScript read `new Date()` and its local `getFullYear`/`getMonth`/`getDate`. The core
/// reads no clock and knows no timezone, so both arrive on [`crate::ChatContext`].
pub fn local_date_directory(now_ms: f64, utc_offset_minutes: i32) -> String {
    let local_ms = now_ms + f64::from(utc_offset_minutes) * 60_000.0;
    let days = (local_ms / 86_400_000.0).floor() as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `normalizedMarkdownStem`: the typed name without its `.md` and without the space around it.
pub fn normalized_markdown_stem(value: &str) -> String {
    let trimmed = js_trim(value);
    let without_extension = strip_suffix_ci(trimmed, ".md").unwrap_or(trimmed);
    js_trim(without_extension).to_string()
}

/// `normalizedFolderPath`: each segment trimmed, the slashes kept.
pub fn normalized_folder_path(value: &str) -> String {
    js_trim(value)
        .split('/')
        .map(js_trim)
        .collect::<Vec<_>>()
        .join("/")
}

/// `folderPathError`: the refusal to show under the folder field, or `None`.
pub fn folder_path_error(value: &str) -> Option<String> {
    let path = normalized_folder_path(value);
    if path.is_empty() {
        return Some("Enter a folder name.".to_string());
    }
    if path.encode_utf16().count() > 240 {
        return Some("Use a folder path of 240 characters or fewer.".to_string());
    }
    for segment in path.split('/') {
        if segment.is_empty() {
            return Some("Enter a folder name between each slash.".to_string());
        }
        if segment == "." || segment == ".." || ends_with_period_or_space(segment) {
            return Some(
                "Folder names cannot be a period or end with a period or space.".to_string(),
            );
        }
        if segment.chars().any(is_forbidden_path_character) {
            return Some(
                "A folder name contains a character that cannot be used in a path.".to_string(),
            );
        }
        if is_reserved_device_name(segment) {
            return Some("A folder name is reserved by the operating system.".to_string());
        }
    }
    None
}

/// `markdownStemError`: the refusal to show under the file name field, or `None`.
pub fn markdown_stem_error(value: &str) -> Option<String> {
    let stem = normalized_markdown_stem(value);
    if stem.is_empty() {
        return Some("Enter a file name.".to_string());
    }
    if stem.encode_utf16().count() > 120 {
        return Some("Use a file name of 120 characters or fewer.".to_string());
    }
    if stem.chars().any(is_forbidden_stem_character) {
        return Some(
            "The file name contains a character that cannot be used in a path.".to_string(),
        );
    }
    if stem == "." || stem == ".." || ends_with_period_or_space(&stem) {
        return Some("Enter a file name without a trailing period or space.".to_string());
    }
    // The file-name rule also refuses a reserved name carrying an extension (`nul.txt`).
    let head = stem.split_once('.').map(|(head, _)| head).unwrap_or(&stem);
    if is_reserved_device_name(head) {
        return Some("That file name is reserved by the operating system.".to_string());
    }
    None
}

/// `suggestedMarkdownStem`: the session's title, numbered past whatever is already in the folder.
pub fn suggested_markdown_stem(
    session_title: &str,
    folder_path: &str,
    existing_paths: &[String],
) -> String {
    let base = session_markdown_base(session_title);
    let suffix_separator = if base.contains('-') { "-" } else { " " };
    let prefix = format!("docs/{}/", normalized_folder_path(folder_path));
    let prefix_lower = prefix.to_lowercase();
    let mut highest_suffix: i64 = 0;
    for path in existing_paths {
        if !path.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }
        // The TypeScript slices by the prefix's own length, which is only correct when the two
        // agree; they always do, because the comparison above is case-insensitive over the same
        // characters.
        let Some(name) = char_slice_from(path, prefix.len()) else {
            continue;
        };
        if name.contains('/') {
            continue;
        }
        if let Some(suffix) = numbered_suffix(name, &base, suffix_separator) {
            highest_suffix = highest_suffix.max(suffix);
        }
    }
    format!("{base}{suffix_separator}{}", highest_suffix + 1)
}

/// `sessionMarkdownBase`: the title reduced to something a file system accepts.
fn session_markdown_base(session_title: &str) -> String {
    let replaced: String = js_trim(session_title)
        .chars()
        .map(|character| {
            if is_forbidden_stem_character(character) {
                ' '
            } else {
                character
            }
        })
        .collect();
    let collapsed = collapse_whitespace_runs(&replaced);
    let trimmed = js_trim(trim_trailing_periods_and_spaces(&collapsed));
    let title = if trimmed.is_empty() {
        "Saved response"
    } else {
        trimmed
    };
    // `.slice(0, 110)` counts UTF-16 code units, then `trimEnd`.
    let units: Vec<u16> = title.encode_utf16().collect();
    let cut = if units.len() > 110 {
        String::from_utf16_lossy(&units[..110])
    } else {
        title.to_string()
    };
    cut.trim_end_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
        .to_string()
}

/// `^<base><separator>(\d+)\.md$`, case-insensitive; the captured number, or `None`.
fn numbered_suffix(name: &str, base: &str, separator: &str) -> Option<i64> {
    let head = format!("{base}{separator}");
    let candidate = char_slice_to(name, head.len())?;
    if candidate.to_lowercase() != head.to_lowercase() {
        return None;
    }
    let rest = char_slice_from(name, head.len())?;
    let digits = strip_suffix_ci(rest, ".md")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // A run longer than an `i64` cannot be a real suffix; the TypeScript's `parseInt` would give
    // an imprecise float, which never wins the `Math.max` against a real numbering either.
    digits.parse().ok()
}

/// `[\\/<>:"|?*\u0000-\u001f]`, the characters a file name may not carry.
fn is_forbidden_stem_character(character: char) -> bool {
    matches!(
        character,
        '\\' | '/' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
    ) || (character as u32) <= 0x1f
}

/// The same set without the slash, which separates a folder path's segments.
fn is_forbidden_path_character(character: char) -> bool {
    matches!(character, '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*')
        || (character as u32) <= 0x1f
}

/// `/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])$/i`.
fn is_reserved_device_name(segment: &str) -> bool {
    let lowered = segment.to_lowercase();
    if matches!(lowered.as_str(), "con" | "prn" | "aux" | "nul") {
        return true;
    }
    for prefix in ["com", "lpt"] {
        if let Some(digit) = lowered.strip_prefix(prefix) {
            if digit.len() == 1 && matches!(digit.as_bytes()[0], b'1'..=b'9') {
                return true;
            }
        }
    }
    false
}

/// `/[. ]$/u`.
fn ends_with_period_or_space(value: &str) -> bool {
    matches!(value.chars().next_back(), Some('.') | Some(' '))
}

/// `/[. ]+$/gu` removed.
fn trim_trailing_periods_and_spaces(value: &str) -> &str {
    value.trim_end_matches(['.', ' '])
}

/// `.replace(/\s+/gu, ' ')`.
fn collapse_whitespace_runs(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_run = false;
    for character in value.chars() {
        if character.is_whitespace() || character == '\u{feff}' {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(character);
            in_run = false;
        }
    }
    out
}

/// Howard Hinnant's `civil_from_days`, the inverse of the one in [`crate::extras::time`].
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// `String.prototype.endsWith` plus the slice, case-insensitively.
fn strip_suffix_ci<'a>(value: &'a str, suffix: &str) -> Option<&'a str> {
    let start = value.len().checked_sub(suffix.len())?;
    if !value.is_char_boundary(start) || value[start..].to_lowercase() != suffix.to_lowercase() {
        return None;
    }
    Some(&value[..start])
}

/// `value.slice(at)`, or `None` when `at` is not a character boundary.
fn char_slice_from(value: &str, at: usize) -> Option<&str> {
    (at <= value.len() && value.is_char_boundary(at)).then(|| &value[at..])
}

/// `value.slice(0, at)`, or `None` when `at` is past the end or not a character boundary.
fn char_slice_to(value: &str, at: usize) -> Option<&str> {
    (at <= value.len() && value.is_char_boundary(at)).then(|| &value[..at])
}
