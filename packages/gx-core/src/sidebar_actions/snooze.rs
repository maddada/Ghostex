//! Snooze and unsnooze: the wake time, the call, and the sleep that follows an accepted snooze.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! This is the only sidebar action whose answer depends on the CLOCK, and it depends on it twice:
//! on the instant (`Date.now()`) and on the local calendar, because "Tomorrow" and "Next week"
//! mean 09:00 local time on a later day and not a fixed offset. The crate reads neither, so the
//! host hands both over as data in [`SnoozeClock`], including the local UTC offset that will be in
//! force AT each candidate morning. That last part is what makes the port exact rather than nearly
//! right: JavaScript's `setHours(9, 0, 0, 0)` converts the target local time with the offset of
//! the TARGET day, so a snooze set the evening before a daylight-saving change and built with
//! today's offset would wake an hour early or an hour late.
//!
//! Three separate payloads live here because they are one feature:
//!
//! - `sessionAction: snooze` is the MENU row. It calls nothing: it resolves the preset to an
//!   absolute wake time and posts up to two ordinary commands, the tag first and the snooze
//!   second, exactly as `runNativeSessionAction` does.
//! - `snoozeSession` is the call, `/api/snoozeSession`, and an accepted one is followed by an
//!   ordinary sleep through the path that already owns sleeping.
//! - `unsnoozeSession` is the other call, `/api/unsnoozeSession`, and nothing follows it.
//!
//! **Neither call patches anything.** The deleted `runSessionLifecycleCommand` said so in its own comment and
//! it is a rule rather than an omission: gxserver owns `snoozedUntil`, emits the delta itself, and
//! enforces guards (a wake time in the past is refused) that the client must not pre-empt. So
//! there is no overlay here and no echo guard to write; the one optimistic value in the whole
//! feature is the sleep, and that belongs to `lifecycle.rs`.
//!
//! SEE-ALSO: packages/shared/session-snooze.ts (`resolveSessionSnoozeWakeTime`,
//! `isSidebarSessionSnoozed`), apps/desktop/src/app/gx_store/sidebar_snooze.rs. Ported from the
//! sidebar page's session actions (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/session-actions.ts`) and `snoozeSession` /
//! `runSessionLifecycleCommand` in the deleted `gxserver-runtime/sessions-and-focus.ts`.

use serde_json::{json, Map, Value};

use crate::keys::SessionKey;
use crate::sidebar_view::text::civil_from_days;
use crate::sidebar_view::SidebarView;

use super::plan::ToastLevel;
use super::resolve::{drawn_row, text_field};

const HOUR_MS: i64 = 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * HOUR_MS;
/// `MORNING_WAKE_HOUR`.
const MORNING_WAKE_HOUR_MS: i64 = 9 * HOUR_MS;

/// `SESSION_SNOOZE_PRESETS`, in the order the submenu offers them.
pub const SESSION_SNOOZE_PRESETS: [&str; 4] = ["oneHour", "threeHours", "tomorrow", "nextWeek"];

/// The description every lifecycle-command failure toast carries, on either machine.
pub(super) const LIFECYCLE_FAILURE_DESCRIPTION: &str =
    "gxserver refused the change. The session may be working or waiting on you.";

/// The clock facts the wake-time rule needs, read by the host because this crate reads no clock
/// and knows no time zone.
///
/// `morning_offset_ms[d]` is the local UTC offset in force at 09:00 local time `d` local days from
/// today, which is the offset JavaScript's `setHours` would use for that target. Index 0 is today
/// and is never read by the presets below; the two that need one read 1 (Tomorrow) through 7 (Next
/// week when today is a Monday).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnoozeClock {
    /// `Date.now()`.
    pub now_ms: i64,
    /// `-new Date().getTimezoneOffset() * 60_000`: the local offset at `now_ms`, which is what
    /// decides today's local date and today's local weekday.
    pub offset_ms: i64,
    pub morning_offset_ms: [i64; 8],
}

impl SnoozeClock {
    /// A clock in a zone with one fixed offset, which is every zone without daylight saving.
    pub fn fixed(now_ms: i64, offset_ms: i64) -> Self {
        Self {
            now_ms,
            offset_ms,
            morning_offset_ms: [offset_ms; 8],
        }
    }

    /// Today's local date, as days since 1970-01-01. `new Date().getDate()` reads the same thing
    /// through the offset in force now.
    fn local_day(&self) -> i64 {
        self.now_ms
            .saturating_add(self.offset_ms)
            .div_euclid(DAY_MS)
    }

    /// `new Date().getDay()`: 0 is Sunday. 1970-01-01 was a Thursday.
    fn local_weekday(&self) -> i64 {
        (self.local_day() + 4).rem_euclid(7)
    }

    /// `atMorning(date)` for a date `days` local days from today.
    fn local_morning_ms(&self, days: i64) -> i64 {
        let offset = self
            .morning_offset_ms
            .get(days.clamp(0, 7) as usize)
            .copied()
            .unwrap_or(self.offset_ms);
        (self.local_day() + days) * DAY_MS + MORNING_WAKE_HOUR_MS - offset
    }
}

/// `resolveSessionSnoozeWakeTime`, as an instant.
///
/// `None` is a preset this rule has no case for. JavaScript's `switch` has none either and returns
/// `undefined`, on which `.toISOString()` throws, so the payload is refused rather than answered.
pub fn snooze_wake_ms(preset: &str, clock: &SnoozeClock) -> Option<i64> {
    match preset {
        "oneHour" => Some(clock.now_ms.saturating_add(HOUR_MS)),
        "threeHours" => Some(clock.now_ms.saturating_add(3 * HOUR_MS)),
        "tomorrow" => Some(clock.local_morning_ms(1)),
        // `(8 - day) % 7 || 7`: the next Monday, and a full week when today is one.
        "nextWeek" => {
            let days = match (8 - clock.local_weekday()) % 7 {
                0 => 7,
                days => days,
            };
            Some(clock.local_morning_ms(days))
        }
        _ => None,
    }
}

/// `Date.prototype.toISOString`: `YYYY-MM-DDTHH:mm:ss.sssZ`, and the six-digit expanded year form
/// for a year outside 0000..=9999, which is what the daemon is handed.
pub fn iso_string_from_ms(ms: i64) -> String {
    let days = ms.div_euclid(DAY_MS);
    let time = ms.rem_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    let year_text = match (0..=9_999).contains(&year) {
        true => format!("{year:04}"),
        false => match year < 0 {
            true => format!("-{:06}", -year),
            false => format!("+{year:06}"),
        },
    };
    format!(
        "{year_text}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        time / HOUR_MS,
        (time % HOUR_MS) / 60_000,
        (time % 60_000) / 1_000,
        time % 1_000,
    )
}

/// What a `sessionAction: snooze` posts, in order.
///
/// An empty list is a real answer: the row is drawn, the command names neither a tag nor a preset,
/// and doing nothing is what `runNativeSessionAction` does with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnoozeAction {
    pub messages: Vec<Value>,
}

impl SnoozeAction {
    pub fn to_json(&self) -> Value {
        Value::Array(self.messages.clone())
    }
}

/// The menu row, resolved into the commands it posts.
///
/// The row is looked up in the DRAWN list, which is where `runNativeSessionAction` looks
/// (`sidebarStore.getState().sessionsById`), and a row that is not there stops the whole action
/// before the switch.
///
/// Refused, with the reason at each refusal:
///
/// - a `sessionAction` that is not `snooze` belongs to `modals.rs` (rename, note) or to the old
///   runtime (`firstMessage`, `delayedSend`), each already refused there with its own reason;
/// - a preset this rule has no case for, because JavaScript throws a `TypeError` inside
///   `.toISOString()` on it rather than posting anything, and no menu can build one: reproducing a
///   throw is not a behaviour worth porting, so the payload stays the old runtime's.
pub fn plan_snooze_action(
    view: &SidebarView,
    message: &Value,
    clock: &SnoozeClock,
) -> Option<SnoozeAction> {
    if text_field(message, "type")? != "sessionAction" || text_field(message, "action")? != "snooze"
    {
        return None;
    }
    let sidebar_session_id = text_field(message, "sessionId")?;
    drawn_row(view, sidebar_session_id)?;
    let mut messages: Vec<Value> = Vec::new();
    // `if (command.sessionTag !== undefined)`: the tag rides ahead of the snooze, and an explicit
    // `null` is the clear. Absence is the third state and posts nothing at all.
    if let Some(session_tag) = message.get("sessionTag") {
        messages.push(json!({
            "type": "setSessionTag",
            "sessionId": sidebar_session_id,
            "sessionTag": session_tag,
        }));
    }
    // `if (command.preset)`: an empty string is falsy and posts nothing.
    if let Some(preset) = text_field(message, "preset").filter(|preset| !preset.is_empty()) {
        let wake_ms = snooze_wake_ms(preset, clock)?;
        messages.push(json!({
            "type": "snoozeSession",
            "sessionId": sidebar_session_id,
            "snoozedUntil": iso_string_from_ms(wake_ms),
        }));
    }
    Some(SnoozeAction { messages })
}

/// Which of the two `runSessionLifecycleCommand` endpoints the sidebar can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnoozeCall {
    Snooze,
    Unsnooze,
}

impl SnoozeCall {
    pub fn rpc_path(self) -> &'static str {
        match self {
            Self::Snooze => "/api/snoozeSession",
            Self::Unsnooze => "/api/unsnoozeSession",
        }
    }

    /// `SESSION_LIFECYCLE_FAILURE_TITLES[path]`. Unsnooze is called "Wake failed" because that is
    /// what the user asked for.
    pub fn failure_title(self) -> &'static str {
        match self {
            Self::Snooze => "Snooze failed",
            Self::Unsnooze => "Wake failed",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Snooze => "snooze",
            Self::Unsnooze => "unsnooze",
        }
    }
}

/// What the host must call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnoozeRequest {
    pub session: SessionKey,
    pub call: SnoozeCall,
    pub rpc_path: &'static str,
    pub rpc_params: Value,
}

impl SnoozeRequest {
    pub fn to_json(&self) -> Value {
        json!({
            "call": self.call.as_str(),
            "rpc": { "path": self.rpc_path, "params": self.rpc_params },
        })
    }
}

/// What the host does once the call has come back, or has not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnoozeFollowUp {
    /// The ordinary sleep, through the path that owns its call, its declined leg and its overlay.
    /// A snoozed session is always asleep (`CDXC:Sessions 2026-09-12 DECISION`), and the sleep
    /// follows the snooze rather than leading it, so a refused snooze leaves the session running.
    Sleep { session: SessionKey },
    /// The refusal the user sees. It names no session, no path and no daemon body.
    Toast {
        level: ToastLevel,
        title: &'static str,
        description: &'static str,
    },
}

impl SnoozeFollowUp {
    pub fn to_json(&self) -> Value {
        match self {
            Self::Sleep { session } => json!({
                "follow": "sleep",
                "session": session.to_sidebar_session_id(),
            }),
            Self::Toast {
                level,
                title,
                description,
            } => json!({
                "follow": "toast",
                "level": level.as_str(),
                "title": title,
                "hasDescription": !description.is_empty(),
            }),
        }
    }
}

/// The `snoozeSession` and `unsnoozeSession` payloads, or `None` when this file does not own one.
///
/// Refused, with the reason at each refusal: a browser row (`gpui-browser:`) is an app tab with no
/// daemon session behind it. A REMOTE row is answered by `remote.rs`, from the same request this
/// builds (`snooze_request`), because `runSessionLifecycleCommand` sends the same body down that
/// machine's tunnel. The Quick Automations row is deliberately NOT refused: unlike
/// `setSessionSleeping`, `runSessionLifecycleCommand` has no early return for it and really does
/// call the daemon, and the sleep that follows an accepted snooze is the one that returns early.
/// A row the store does not hold is not refused either, for the same reason as the flags path: the
/// TypeScript parses the id and calls, and a port that checked would silently do nothing where the
/// shipped code still asks.
pub fn plan_snooze_request(message: &Value) -> Option<SnoozeRequest> {
    snooze_request(message).filter(|request| request.session.machine.is_local())
}

/// The call for either machine: `runSessionLifecycleCommand` builds one body and only the route
/// differs, so the body is built once.
pub(super) fn snooze_request(message: &Value) -> Option<SnoozeRequest> {
    let call = match text_field(message, "type")? {
        "snoozeSession" => SnoozeCall::Snooze,
        "unsnoozeSession" => SnoozeCall::Unsnooze,
        _ => return None,
    };
    let sidebar_session_id = text_field(message, "sessionId")?;
    if sidebar_session_id.starts_with("gpui-browser:") {
        return None;
    }
    let session = SessionKey::parse_sidebar_session_id(sidebar_session_id)?;
    let mut params = Map::new();
    if call == SnoozeCall::Snooze {
        // Absent, not null. The TypeScript spreads `{ snoozedUntil }` and `JSON.stringify` drops an
        // `undefined`, so a payload without the field sends no key rather than a null the daemon
        // would have to interpret. Same class as the cleared tag and the missing agent icon.
        if let Some(snoozed_until) = text_field(message, "snoozedUntil") {
            params.insert(
                "snoozedUntil".to_string(),
                Value::String(snoozed_until.to_string()),
            );
        }
    }
    params.insert(
        "projectId".to_string(),
        Value::String(session.project_id.clone()),
    );
    params.insert(
        "sessionId".to_string(),
        Value::String(session.session_id.clone()),
    );
    Some(SnoozeRequest {
        rpc_path: call.rpc_path(),
        rpc_params: Value::Object(params),
        call,
        session,
    })
}

/// What to do with the answer.
pub fn apply_snooze_answer(request: &SnoozeRequest, accepted: bool) -> Vec<SnoozeFollowUp> {
    if !accepted {
        return vec![SnoozeFollowUp::Toast {
            level: ToastLevel::Warning,
            title: request.call.failure_title(),
            description: LIFECYCLE_FAILURE_DESCRIPTION,
        }];
    }
    match request.call {
        SnoozeCall::Snooze => vec![SnoozeFollowUp::Sleep {
            session: request.session.clone(),
        }],
        SnoozeCall::Unsnooze => Vec::new(),
    }
}

/// Whether this payload is one of the two calls, without building anything.
pub fn owns_snooze_message(message: &Value) -> bool {
    matches!(
        text_field(message, "type"),
        Some("snoozeSession" | "unsnoozeSession")
    )
}

/// Whether this payload is the menu row, without building anything.
pub fn owns_snooze_action(command: &Value) -> bool {
    text_field(command, "type") == Some("sessionAction")
        && text_field(command, "action") == Some("snooze")
}

/// Every message type the two calls answer, for a host that wants to count a decline by name.
pub const SNOOZE_MESSAGE_TYPES: [&str; 2] = ["snoozeSession", "unsnoozeSession"];
