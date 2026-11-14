//! The reverse of a snapshot: the rows a machine holds, written back out as one
//! `PresentationSnapshot`.
//!
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! The last-seen copy of an offline remote machine is stored as a whole
//! `GxserverPresentationSnapshot`, the same shape the stream delivers, so the store has to be able
//! to hand one back. That is a decoding question rather than a serializing one: the value the old
//! sidebar writes is what the old sidebar reads, and both are still live, so a copy that dropped a
//! field or reordered an array would be a different payload for the same key and the two writers
//! would flip the row between them on every publish.
//!
//! **What makes the two bytewise equal is the ARRAY ORDER, and it is the daemon's own.** Projects
//! and groups keep the `(sortKey, id)` order [`super::loaded`] already sorts them into; sessions
//! are written in `(projectId, groupId, sortKey, sessionId)` order, which is the key sequence
//! `orderPresentationSessions` (`packages/shared/gxserver-presentation-cache.ts`) re-sorts the
//! TypeScript cache into after every delta. Byte order rather than `localeCompare`, for the reason
//! on `sort_projects`: the daemon orders by byte order and the store follows the daemon.
//!
//! **What is NOT written, on purpose.** The local overlays (a pending sleep, a local hide, a
//! manual reorder's predicted row) are this client's guesses about a machine, not the machine, and
//! a copy read back next run must start from what the daemon said. Rows a snapshot could not parse
//! (`Rows::skipped`) are not written back either: the store does not hold them.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/remote_last_seen.rs (the only caller today),
//! packages/gx-core/src/presentation_store/apply.rs (`seed_last_seen`, which reads one back in).

use ghostex_gx_protocol::{PresentationSession, PresentationSnapshot};
use serde_json::{Number, Value};

use super::store::MachinePresentation;

/// A snapshot as the stored copy holds it: JSON with every object's keys in byte order.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// The order is not cosmetic and it is not `serde_json`'s to decide. gxserver builds its frames
/// with `json!` into a `BTreeMap`, so every object reaches the wire in byte order, and
/// `JSON.stringify` of the parsed object keeps it, which is the order the old sidebar's copy of
/// this key is written in. A Rust struct serializes in DECLARATION order, and the desktop binary
/// turns `serde_json`'s map into an `IndexMap` (gpui pulls in the `preserve_order` feature), so
/// nothing about the default output is the daemon's order. Sorting here makes the two writers
/// produce the same bytes for the same rows, which is what lets the write be skipped instead of
/// flipping the row and its `updatedAt` on every publish.
pub fn snapshot_storage_json(snapshot: &PresentationSnapshot) -> String {
    let value = serde_json::to_value(snapshot).unwrap_or(Value::Null);
    json_stringify(&value)
}

/// The value written the way `JSON.stringify` writes it, with every object's keys in byte order.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// `serde_json`'s own `to_string` is NOT `JSON.stringify` and the difference is not cosmetic: it
/// writes an f64 through `ryu`, so a `sidebarOrder` of zero reaches the row as `0.0` where
/// `JSON.stringify` writes `0`. That one character is why the "both writers store the same bytes"
/// claim was false and why every publish rewrote the row instead of being skipped as unchanged (a
/// real stored row of 43,012 characters came back as 43,048, first difference at a `sidebarOrder`).
/// So the text is emitted here rather than by `serde_json`, and numbers follow ECMAScript
/// `Number::toString`, which differs from `ryu` in four places: an integral value has no `.0`,
/// negative zero prints as `0`, the exponent form starts at 1e21 rather than 1e16 and stops at
/// 1e-7 rather than 1e-6.
pub fn json_stringify(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => write_number(out, number),
        Value::String(text) => write_string(out, text),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                write_value(out, &map[key]);
            }
            out.push('}');
        }
    }
}

/// One number, as `JSON.stringify` writes it.
///
/// Every number goes through `f64` on purpose, integers included: the copy this has to match was
/// produced by `JSON.parse` of the daemon's frame followed by `JSON.stringify`, and `JSON.parse`
/// has no integer type. Below 2^53 that changes nothing, because an `i64` of that size is exact as
/// an `f64` and prints as the same digits; above it, printing what JavaScript would print is the
/// point.
fn write_number(out: &mut String, number: &Number) {
    match number.as_f64() {
        Some(value) => out.push_str(&js_number(value)),
        // Unreachable without `serde_json`'s `arbitrary_precision`, which nothing here enables.
        None => out.push_str(&number.to_string()),
    }
}

/// ECMAScript `Number::toString` at radix 10 (ECMA-262, "Number::toString"), which is what
/// `JSON.stringify` emits for a finite number.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// The digits come from `serde_json`'s own formatter (`ryu`) and NOT from Rust's `{}` or `{:e}`,
/// which was the first cut and was wrong 68 times in 400,044 random values: all three emit the
/// shortest digits that round-trip, but where two candidates are exactly equidistant `ryu` and V8
/// both round the last digit to even while Rust's rounds away from zero, so `{:e}` writes
/// `1658206780088562.3` where JavaScript writes `1658206780088562.2`. What this function adds on
/// top of `ryu` is only where ECMAScript puts the decimal point.
pub(crate) fn js_number(value: f64) -> String {
    // `JSON.stringify` writes a non-finite number as `null`; `serde_json` cannot hold one in a
    // `Value` at all, so this is the shape of the answer rather than a case that arises.
    if !value.is_finite() {
        return "null".to_string();
    }
    // Covers negative zero, which ECMAScript prints as `0`.
    if value == 0.0 {
        return "0".to_string();
    }
    let negative = value < 0.0;
    let shortest = Number::from_f64(value.abs())
        .map(|number| number.to_string())
        .unwrap_or_else(|| value.abs().to_string());
    let (digits, point) = shortest_parts(&shortest);
    let digits = digits.as_str();
    // The spec's `k` and `n`: the value is `digits` (k of them) read as `0.digits * 10^n`.
    let count = digits.len() as i32;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if count <= point && point <= 21 {
        out.push_str(digits);
        out.extend(std::iter::repeat_n('0', (point - count) as usize));
    } else if 0 < point && point <= 21 {
        out.push_str(&digits[..point as usize]);
        out.push('.');
        out.push_str(&digits[point as usize..]);
    } else if -6 < point && point <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-point) as usize));
        out.push_str(digits);
    } else {
        out.push_str(&digits[..1]);
        if count > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if point > 0 { '+' } else { '-' });
        out.push_str(&(point - 1).abs().to_string());
    }
    out
}

/// `serde_json`'s rendering of a positive finite f64 read as the spec's `digits` and `n`, where the
/// value is `0.digits * 10^n`.
///
/// Both of that rendering's forms are handled: `123.45`, which always carries a decimal point, and
/// `1.2345e+21`, whose exponent always carries its sign.
fn shortest_parts(shortest: &str) -> (String, i32) {
    let (mantissa, exponent) = match shortest.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().unwrap_or(0)),
        None => (shortest, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = String::with_capacity(whole.len() + fraction.len());
    digits.push_str(whole);
    digits.push_str(fraction);
    let trimmed = digits.trim_start_matches('0');
    let trailing = trimmed.len() - trimmed.trim_end_matches('0').len();
    let trimmed = trimmed.trim_end_matches('0');
    if trimmed.is_empty() {
        return ("0".to_string(), 1);
    }
    // `value = int(trimmed) * 10^(trailing + exponent - fraction.len())`, and the spec writes that
    // as `s * 10^(n - k)` with `s = int(trimmed)` and `k = trimmed.len()`.
    let point = trimmed.len() as i32 + trailing as i32 + exponent - fraction.len() as i32;
    (trimmed.to_string(), point)
}

/// One string, as `JSON.stringify` writes it.
///
/// CDXC:RemoteMachines 2026-09-21 WHY:
/// The two writers agree here and this spells out why, because "they probably escape the same" is
/// what the number formatting was assumed to be. Both escape exactly `"`, `\` and the C0 controls,
/// both spell the five short escapes the same way and both leave everything else alone, the
/// forward slash and U+2028/U+2029 included (well-formed `JSON.stringify` escapes only LONE
/// SURROGATES, which a Rust `String` cannot hold: a frame carrying one is refused by the parser
/// long before this, which is a different answer from JavaScript's but not a different payload).
fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{9}' => out.push_str("\\t"),
            '\u{a}' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\u{d}' => out.push_str("\\r"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

impl MachinePresentation {
    /// The machine's daemon rows and side state as one snapshot, or `None` while it is not loaded.
    ///
    /// Round-trips: a snapshot fed in through `apply_snapshot` or `seed_last_seen` and asked back
    /// for here serializes to the same bytes, which the `last-seen transition` pass of
    /// `sidebar_replay` asserts against the recording's own snapshot.
    pub fn to_snapshot(&self) -> Option<PresentationSnapshot> {
        let loaded = self.loaded()?;
        let mut sessions: Vec<PresentationSession> = loaded.server_sessions().cloned().collect();
        sessions.sort_by(|left, right| session_order(left).cmp(&session_order(right)));
        Some(PresentationSnapshot {
            revision: loaded.revision,
            generated_at: loaded.generated_at.clone(),
            projects: loaded.projects().to_vec().into(),
            groups: loaded.groups().to_vec().into(),
            sessions: sessions.into(),
            capabilities: loaded.capabilities.clone(),
            auto_settle_after_days: loaded.auto_settle_after_days.clone(),
            workspace_groups: self.side_state().workspace_groups.clone(),
            sidebar_project_collections: self.side_state().project_collections.clone(),
            sidebar_spaces: self.side_state().spaces.clone(),
            sidebar_spaces_enabled: self.side_state().spaces_enabled,
            sidebar_workspaces: self.side_state().workspaces.clone(),
            custom_session_tags: self.side_state().custom_session_tags.clone(),
            // Not kept by the store yet: sidebar extensions are drawn from phase 2
            // (docs/2026-11-14/sidebar-extensions/). gxserver sends the field only while a
            // sidebar extension is on, so recorded snapshots without one still round-trip.
            extension_panels: None,
        })
    }
}

/// `orderPresentationSessions`' key sequence.
fn session_order(session: &PresentationSession) -> (&str, &str, &str, &str) {
    (
        session.project_id.as_str(),
        session.group_id.as_str(),
        session.sort_key.as_str(),
        session.session_id.as_str(),
    )
}
