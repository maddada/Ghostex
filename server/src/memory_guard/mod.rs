//! The memory guard: when this computer is about to run out of memory, gxserver steps in on the
//! agent session using the most. The runtime that ticks and acts is
//! server/src/server/memory_guard_runtime.rs; this module holds what it reads and decides.
//!
//! - `pressure.rs`: the system's memory pressure level (macOS kernel level, Linux PSI and
//!   MemAvailable).
//! - `processes.rs` (+ `processes_macos.rs`, `processes_linux.rs`): this user's processes, each
//!   session's process tree, how much memory each process uses, and the signals that stop them.
//! - `usage.rs`: one session's usage (the agent, each command it started) and the step to take.
//! - `words.rs`: what the chat card, the notification and the agent are told.
//! - `card.rs`: the chat card, and the record a stopped session keeps (its red dot and card).
//!
//! CDXC:MemoryGuard 2026-11-14 DECISION:
//! User, on a post where another app "notices when your threads are causing too much memory pressure and kills them to prevent your system from OOMing": "i want this on by default too", and the approach they picked on the design page (Q3098 D, staged): at the kernel's critical memory pressure level (Q3099), held for 10 seconds, Ghostex stops the heaviest command an agent session started (a build, a test run, a dev server), or restarts the agent on the same conversation when the agent process itself is the heavy one, and only if pressure stays critical 30 seconds later stops the whole session, which the user can resume. Only sessions of at least 2 GB are touched, never anything outside Ghostex's own agent sessions. The user sees a card in that chat and a notification whenever the guard stops something (Q3100), and both switches sync to all the user's computers, even while Settings > Sync > Agent defaults is off, with each computer guarding itself (Q3101, Q3122; gx-core `ALWAYS_SYNCED_KEYS`). On telling the agent: "keep in mind u can send with interruption in case the agent is in the middle of something if u need to make the message appear right away in his transcript", so the note about a stopped command goes out with an interrupt.
//! SEE-ALSO: packages/settings-catalog/src/general/system.rs (the switches), apps/desktop/src/app/window/settings_modal/tabs/general/memory_guard.rs (their rows), server/src/session_chat_notice/watchdog_store.rs (the card's lifetime and priority), server/src/push/message.rs (`AlertKind::OutOfMemory`).

mod card;
mod pressure;
mod processes;
#[cfg(target_os = "linux")]
mod processes_linux;
#[cfg(target_os = "macos")]
mod processes_macos;
mod usage;
mod words;

pub(crate) use card::{
    blocked_by_memory_guard, card, card_from_record, stop_record, stopped_parts,
    BLOCKED_ALERTED_BY, STOP_RECORD,
};
pub(crate) use pressure::{read_pressure_level, PressureLevel};
pub(crate) use processes::{
    alive_members, signal_processes, snapshot_user_processes, supported, Member, StopSignal,
};
pub(crate) use usage::{min_session_bytes, session_usage, ChildGroup, SessionUsage, Step};
pub(crate) use words::*;

/// The settings switch (on by default): the guard acts only while it is on.
pub(crate) const ENABLED_SETTING: &str = "memoryGuardEnabled";
/// Whether the guard also posts a notification (the bell and the phone) when it stops something.
pub(crate) const NOTIFY_SETTING: &str = "memoryGuardNotify";
/// The kind of the chat card the guard leaves in the chat it acted on.
pub(crate) const NOTICE_KIND: &str = "memoryGuard";
/// The card's Resume button on a session the guard stopped.
pub(crate) const ACTION_RESUME: &str = "memoryGuardResume";

/// Whether the guard is on in this computer's settings file (absent file or key: on).
pub(crate) fn setting_on(settings: Option<&serde_json::Value>, key: &str) -> bool {
    settings
        .and_then(|settings| settings.get(key))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

/// `/api/readMemoryGuard`: the two switches as this computer has them, for the phone's Settings ›
/// Connection › Memory, which changes them through `updateSettingsPatch` like any settings write
/// (apps/mobile/app/src/screens/settings/MemoryGuardRows.tsx).
pub(crate) fn read_endpoint_value(paths: &crate::paths::GxserverPaths) -> serde_json::Value {
    let settings = crate::shared_settings_file::read_shared_settings(paths);
    serde_json::json!({
        "enabled": setting_on(settings.as_ref(), ENABLED_SETTING),
        "notify": setting_on(settings.as_ref(), NOTIFY_SETTING),
    })
}
