//! The JavaScript string, number and date behaviour Quick Access depends on, as the app runtime's
//! QuickJS engine had it.
//!
//! CDXC:AppModal 2026-09-25 WHY:
//! The TypeScript controller ran inside QuickJS, not V8: `localeCompare` there ignores its options
//! and compares NFC code points, and `Intl` does not exist (`day-labels.ts` formats by hand for that
//! reason). The ported model reproduces QuickJS, because that is what the user saw.

use crate::sidebar_view::text::{is_js_whitespace, js_trim, utf16_len};

/// The hotkey label convention: compact glyphs on macOS, words elsewhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HotkeyPlatform {
    #[default]
    Mac,
    Windows,
    Linux,
}

/// What the model needs from the host's clock and calendar. The core reads neither itself.
pub trait QuickAccessClock {
    /// Epoch milliseconds now (`Date.now()`).
    fn now_ms(&self) -> i64;
    /// The local UTC offset in milliseconds in force at `ms`, which is what `new Date(ms)`'s local
    /// fields use.
    fn utc_offset_ms_at(&self, ms: i64) -> i64;
}

/// A clock frozen at one instant and one offset, for replays and the web build's first cut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FixedClock {
    pub now_ms: i64,
    pub utc_offset_ms: i64,
}

impl QuickAccessClock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.now_ms
    }
    fn utc_offset_ms_at(&self, _ms: i64) -> i64 {
        self.utc_offset_ms
    }
}

/// QuickJS `a.localeCompare(b)`: NFC code point order, options ignored. The strings compared here
/// (titles, names, ISO stamps, ids) arrive already composed, so the normalization is the identity.
pub(crate) fn locale_compare(left: &str, right: &str) -> std::cmp::Ordering {
    left.chars().cmp(right.chars())
}

/// `value.toLowerCase()`.
pub(crate) fn js_lower(value: &str) -> String {
    value.to_lowercase()
}

/// `value.replace(/\s+/g, ' ')`, without trimming.
pub(crate) fn collapse_runs(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_run = false;
    for character in value.chars() {
        if is_js_whitespace(character) {
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

/// `value.split(/\s+/)` over a string that `collapse_runs` already produced, then `filter(Boolean)`.
pub(crate) fn split_words(value: &str) -> Vec<&str> {
    value
        .split(is_js_whitespace)
        .filter(|word| !word.is_empty())
        .collect()
}

/// `normalizeHotkeyText` (packages/shared/ghostex-hotkeys.ts).
pub(crate) fn normalize_hotkey_text(value: &str) -> String {
    let lowered = js_lower(js_trim(value));
    let replaced = lowered
        .replace('⌘', "cmd")
        .replace("command", "cmd")
        .replace('⌥', "alt")
        .replace("option", "alt")
        .replace('⌃', "ctrl")
        .replace("control", "ctrl")
        .replace('⇧', "shift");
    let replaced = replace_word(&replaced, "mod", "cmd");
    collapse_runs(&replaced)
        .split(' ')
        .map(normalize_hotkey_chord_text)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `value.replace(/\bword\b/g, with)`.
fn replace_word(value: &str, word: &str, with: &str) -> String {
    let is_word = |character: Option<char>| {
        character.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
    };
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    let mut previous: Option<char> = None;
    while let Some(index) = rest.find(word) {
        let before = rest[..index].chars().next_back().or(previous);
        let after = rest[index + word.len()..].chars().next();
        out.push_str(&rest[..index]);
        if !is_word(before) && !is_word(after) {
            out.push_str(with);
        } else {
            out.push_str(word);
        }
        previous = word.chars().next_back();
        rest = &rest[index + word.len()..];
    }
    out.push_str(rest);
    out
}

/// `normalizeHotkeyChordText`.
fn normalize_hotkey_chord_text(chord: &str) -> String {
    let mut parts: Vec<String> = chord
        .split('+')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    let Some(key) = parts.last().cloned() else {
        return chord.to_string();
    };
    let has = |parts: &[String], name: &str| parts.iter().any(|part| part == name);
    let last = parts.len() - 1;
    if has(&parts, "alt") && key == "ß" {
        parts[last] = "s".to_string();
    }
    if has(&parts, "shift") {
        let shifted_digit = match key.as_str() {
            "!" => Some("1"),
            "@" => Some("2"),
            "#" => Some("3"),
            "$" => Some("4"),
            "%" => Some("5"),
            "^" => Some("6"),
            "&" => Some("7"),
            "*" => Some("8"),
            "(" => Some("9"),
            ")" => Some("0"),
            _ => None,
        };
        if let Some(digit) = shifted_digit {
            parts[last] = digit.to_string();
        }
        let shifted_symbol = match key.as_str() {
            "{" => Some("["),
            "}" => Some("]"),
            _ => None,
        };
        if let Some(symbol) = shifted_symbol {
            parts[last] = symbol.to_string();
        }
    }
    parts.join("+")
}

/// `formatSidebarHotkeyLabel` (packages/shared/hotkey-label.ts).
pub(crate) fn format_hotkey_label(hotkey: &str, platform: HotkeyPlatform) -> String {
    normalize_hotkey_text(hotkey)
        .split(' ')
        .map(|chord| format_hotkey_chord(chord, platform))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_hotkey_chord(chord: &str, platform: HotkeyPlatform) -> String {
    let parts: Vec<&str> = chord.split('+').collect();
    let has_primary = parts.contains(&"cmd");
    let separator = if platform == HotkeyPlatform::Mac {
        ""
    } else {
        "+"
    };
    let formatted: Vec<String> = parts
        .iter()
        .map(|part| format_hotkey_part(part, platform, has_primary))
        .collect();
    formatted
        .iter()
        .enumerate()
        .filter(|(index, part)| *index == 0 || formatted[index - 1] != **part)
        .map(|(_, part)| part.as_str())
        .collect::<Vec<_>>()
        .join(separator)
}

fn format_hotkey_part(part: &str, platform: HotkeyPlatform, has_primary: bool) -> String {
    if platform != HotkeyPlatform::Mac {
        match part {
            "cmd" => return "Ctrl".to_string(),
            "ctrl" => return if has_primary { "Alt" } else { "Ctrl" }.to_string(),
            "alt" => return "Alt".to_string(),
            "shift" => return "Shift".to_string(),
            _ => {}
        }
    }
    match part {
        "cmd" => "⌘".to_string(),
        "ctrl" => "⌃".to_string(),
        "alt" => "⌥".to_string(),
        "shift" => "⇧".to_string(),
        "up" => "↑".to_string(),
        "right" => "→".to_string(),
        "down" => "↓".to_string(),
        "left" => "←".to_string(),
        "tab" => "Tab".to_string(),
        "enter" => "Enter".to_string(),
        _ => {
            let is_function_key = part.len() >= 2
                && part.starts_with('f')
                && part[1..].bytes().all(|byte| byte.is_ascii_digit());
            if is_function_key || utf16_len(part) == 1 {
                part.to_uppercase()
            } else {
                part.to_string()
            }
        }
    }
}

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `quickAccessDayLabel` (day-labels.ts): "Friday, September 18, 2026" in local time.
pub(crate) fn day_label(timestamp_ms: i64, clock: &dyn QuickAccessClock) -> String {
    let local = timestamp_ms + clock.utc_offset_ms_at(timestamp_ms);
    let days = local.div_euclid(86_400_000);
    let (year, month, day) = crate::sidebar_view::text::civil_from_days(days);
    let weekday = (days + 4).rem_euclid(7) as usize;
    format!(
        "{}, {} {}, {}",
        WEEKDAYS[weekday],
        MONTHS[(month - 1) as usize],
        day,
        year
    )
}

/// `Date.parse(value)` when it is a finite number, else `None` (the callers read `NaN` as 0).
pub(crate) fn parse_timestamp(value: Option<&str>) -> Option<i64> {
    crate::sidebar_view::text::parse_iso_ms(value?)
}

/// `formatRelativeTime(isoDate, { allowJustNow, nowMs })` as `(value, suffix)`.
///
/// An unparsable date is `NaN` in JavaScript and falls through every comparison to the day branch,
/// which is what the runtime printed (`NaNd`), so that is reproduced rather than tidied.
pub(crate) fn relative_time(
    iso_date: &str,
    allow_just_now: bool,
    now_ms: i64,
) -> (String, Option<&'static str>) {
    let Some(at) = crate::sidebar_view::text::parse_iso_ms(iso_date) else {
        return ("NaNd".to_string(), Some("ago"));
    };
    let diff = (now_ms - at).max(0);
    let seconds = diff / 1000;
    if allow_just_now && seconds < 5 {
        return ("just now".to_string(), None);
    }
    if seconds < 60 {
        return (format!("{seconds}s"), Some("ago"));
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return (format!("{minutes}m"), Some("ago"));
    }
    let hours = minutes / 60;
    if hours < 24 {
        return (format!("{hours}h"), Some("ago"));
    }
    (format!("{}d", hours / 24), Some("ago"))
}

/// `formatFileSize` (native-quick-access/sessions.ts) for a known size. The value is a dyadic
/// fraction of the byte count, so JavaScript's `toFixed(1)` and `Math.round` (both round half up on
/// the exact value) are done in integers rather than through Rust's float formatting, which rounds
/// half to even.
pub(crate) fn format_file_size(size_bytes: i64) -> String {
    if size_bytes < 1_024 {
        return format!("{size_bytes} B");
    }
    let units = ["KB", "MB", "GB"];
    let mut divisor: i128 = 1_024;
    let mut unit_index = 0;
    while i128::from(size_bytes) >= divisor * 1_024 && unit_index < units.len() - 1 {
        divisor *= 1_024;
        unit_index += 1;
    }
    let bytes = i128::from(size_bytes);
    // `value < 10` with value = bytes / divisor.
    let text = if bytes < 10 * divisor {
        let tenths = (bytes * 20 + divisor) / (2 * divisor);
        format!("{}.{}", tenths / 10, tenths % 10)
    } else {
        format!("{}", (bytes * 2 + divisor) / (2 * divisor))
    };
    format!("{text} {}", units[unit_index])
}

/// `/\p{Cf}/u`: Unicode format characters, the invisible ones a pasted title can carry.
pub(crate) fn is_format_character(character: char) -> bool {
    matches!(
        character,
        '\u{AD}'
            | '\u{600}'..='\u{605}'
            | '\u{61C}'
            | '\u{6DD}'
            | '\u{70F}'
            | '\u{890}'..='\u{891}'
            | '\u{8E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

/// `/[\p{L}\p{N}]/u`.
pub(crate) fn is_letter_or_number(character: char) -> bool {
    character.is_alphanumeric()
}
