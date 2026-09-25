//! The two locale-formatted stamps the host owes the core, in the form desktop users see today.
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! `new Date(ms).toLocaleString()` is the only locale call in the whole chat brain, in two places:
//! the Codex context panel's "Started" row and the account panel's "Next attempt" recovery line.
//! The core cannot read a locale, so it takes the rendering from
//! [`ChatContext::formatted_times`](ghostex_gx_chat_core::ChatContext) and falls back to its own
//! `en-US` form.
//!
//! **That fallback is not what the desktop printed.** The chat brain ran in QuickJS, which is built
//! with no `Intl` and no locale data at all (`typeof Intl === 'undefined'` inside the runtime), so
//! its `toLocaleString()` ignores the user's locale AND V8's `en-US` shape and always writes a
//! zero-padded `MM/DD/YYYY, hh:mm:ss AM/PM` in the machine's local timezone. Measured against
//! `rquickjs` 0.12, the engine the app embedded until 2026-09-25 (this format is kept so the text
//! did not change when the brain moved to Rust):
//!
//! | stamp (UTC) | QuickJS, `TZ=UTC` |
//! |---|---|
//! | 2026-01-02T00:05:09Z | `01/02/2026, 12:05:09 AM` |
//! | 2026-01-02T12:00:00Z | `01/02/2026, 12:00:00 PM` |
//! | 2026-01-02T13:07:03Z | `01/02/2026, 01:07:03 PM` |
//!
//! The crate's fallback writes `1/2/2026, 12:05:09 AM` for the first of those, and applies no
//! offset at all on the account line. So the host supplies both, in the shape above, and the
//! switch to the Rust brain changes neither string. Whether this form should become the user's
//! real locale is a separate, visible product change; it is not this milestone's to make.

use ghostex_gx_chat_core::{ChatState, FormattedTime, FormattedTimeStyle};
use serde_json::Value;

/// The stamps this turn's context carries, read out of the state the last turn left.
///
/// Both stamps are ISO-8601 strings on state the core already holds, and both are parsed with the
/// core's own parser so the millisecond the host keys its entry by is the millisecond the core
/// looks it up by. Anything absent or unparseable is simply left out and falls back.
pub(super) fn formatted_times(state: &ChatState, utc_offset_minutes: i32) -> Vec<FormattedTime> {
    let mut times = Vec::new();
    // `value_started_at` in `menus/context/codex.rs`, which reads Codex's statusline payload off
    // the merged options and parses it with `date_parse`.
    let started_at = state
        .session
        .selected_options
        .as_ref()
        .and_then(|options| options.get("codexStatus"))
        .and_then(|codex| codex.get("startedAt"))
        .and_then(Value::as_str)
        .and_then(ghostex_gx_chat_core::menus::context::time::date_parse)
        .map(|millis| millis.round() as i64);
    if let Some(stamp_ms) = started_at {
        times.push(FormattedTime {
            style: FormattedTimeStyle::ContextStartedAt,
            stamp_ms,
            text: quickjs_locale_string(stamp_ms, utc_offset_minutes),
        });
    }
    // `locale_date_time` in `menus/native_accounts.rs`, which reads the recovery line off the
    // accounts answer and parses it with `parse_iso_millis`.
    let next_attempt = state
        .menus
        .accounts
        .as_ref()
        .and_then(|accounts| accounts.get("session"))
        .and_then(|session| session.get("recovery"))
        .and_then(|recovery| recovery.get("nextAttemptAt"))
        .and_then(Value::as_str)
        .and_then(ghostex_gx_chat_core::menus::time::parse_iso_millis);
    if let Some(stamp_ms) = next_attempt {
        times.push(FormattedTime {
            style: FormattedTimeStyle::AccountDateTime,
            stamp_ms,
            text: quickjs_locale_string(stamp_ms, utc_offset_minutes),
        });
    }
    times
}

/// `new Date(ms).toLocaleString()` as QuickJS with no locale data writes it:
/// `MM/DD/YYYY, hh:mm:ss AM/PM`, every field zero padded to two digits, in local time.
pub(super) fn quickjs_locale_string(epoch_ms: i64, utc_offset_minutes: i32) -> String {
    let local = epoch_ms + i64::from(utc_offset_minutes) * 60_000;
    let days = local.div_euclid(86_400_000);
    let time_of_day = local.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let hour24 = time_of_day / 3_600_000;
    let minute = (time_of_day / 60_000) % 60;
    let second = (time_of_day / 1_000) % 60;
    let suffix = if hour24 < 12 { "AM" } else { "PM" };
    let hour12 = match hour24 % 12 {
        0 => 12,
        hour => hour,
    };
    format!("{month:02}/{day:02}/{year}, {hour12:02}:{minute:02}:{second:02} {suffix}")
}

/// `new Date(ms).toISOString()`: `YYYY-MM-DDTHH:MM:SS.sssZ`.
///
/// The sent history sorts on this string with `localeCompare`, so the zero padding and the three
/// fractional digits are part of the record rather than a formatting choice.
pub(super) fn iso_from_millis(epoch_ms: i64) -> String {
    let days = epoch_ms.div_euclid(86_400_000);
    let time_of_day = epoch_ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3_600_000;
    let minute = (time_of_day / 60_000) % 60;
    let second = (time_of_day / 1_000) % 60;
    let millis = time_of_day % 1_000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days`: the civil date of a day count since the epoch.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = month_position + if month_position < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}
