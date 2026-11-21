//! Main Hub (with Chief of Staff) on for every computer that never chose: once per computer,
//! gxserver turns `mainHubHidden` to false unless something shows the user turned it off.
//!
//! CDXC:Chief 2026-11-21 DECISION:
//! User: "let's please make the main hub project and the chief of staff always enabled for everyone who installs ghostex ... chief of staff is basically the entry point to ghostex". Main Hub is on by default (`mainHubHidden` default false, packages/settings-catalog/src/data/defaults.rs); anyone who never set the switch gets it on, anyone who turned it off keeps it off, and the user can still turn it off in Settings > Extensions (agents still can't turn it on). Supersedes the 2026-11-08 "off by default" decision.
//!
//! WHY: the settings file stores every default, so since 11.2.0 almost every file holds
//! `mainHubHidden: true` whether or not the person ever touched it. Main Hub was off by default
//! all that time, so turning it off means it was on first, and what it leaves behind tells the
//! two apart ([`decide`]). A file with no `mainHubHidden` (a new install) already reads the new
//! default. The pass runs until it has settled once (the `mainHubDefaultOn` row in gxserver's
//! metadata), so a later turn-off is never undone.
//! SEE-ALSO: server/src/chief/sweep.rs (runs it), packages/settings-catalog/src/data/official_extensions.rs (the entry).

use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use super::record::ChiefRecord;

/// The metadata row that says the pass has settled on this computer, with what it decided.
const SETTLED_KEY: &str = "mainHubDefaultOn";

/// When 11.2.0, the first release with Main Hub off by default, was published. A Chief of Staff
/// session started before then may have been started while Chief of Staff was on by default
/// (11.1.0), so it says nothing about a choice.
const OFF_BY_DEFAULT_SINCE: &str = "2026-11-08T17:58:59Z";

/// What the pass does on a computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Main Hub is on already (a new install, or the user turned it on): nothing to do.
    AlreadyOn,
    /// The user turned it off: it stays off.
    KeepOff(&'static str),
    /// Off only because it used to be off by default: turn it on.
    TurnOn,
}

impl Decision {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::AlreadyOn => "alreadyOn",
            Self::KeepOff(_) => "keptOff",
            Self::TurnOn => "turnedOn",
        }
    }
}

fn bool_at(settings: Option<&Value>, key: &str) -> Option<bool> {
    settings?.get(key)?.as_bool()
}

/// Whether Main Hub is off on this computer only because it used to be off by default.
///
/// Main Hub off (`mainHubHidden: true`) is the user's own choice when any of these shows it was on
/// before:
/// 1. `chiefHidden: true`: they turned Chief of Staff off in 11.1.0, when it was on by default.
/// 2. `chiefConfirmNewChat: false` or `chiefShowOnAllSpaces: true`: they changed a Main Hub
///    setting, which shows only while Main Hub is on.
/// 3. A Chief of Staff session started (or a New Chat or Change Agent) on or after 11.2.0's
///    release: gxserver refuses to start one while Main Hub is off.
///
/// Otherwise it was never turned on, so it was never turned off either. The one case this cannot
/// see is turning it on and off again without ever opening Chief of Staff or touching its settings.
pub(crate) fn decide(settings: Option<&Value>, record: Option<&ChiefRecord>) -> Decision {
    if bool_at(settings, "mainHubHidden") != Some(true) {
        return Decision::AlreadyOn;
    }
    if bool_at(settings, "chiefHidden") == Some(true) {
        return Decision::KeepOff("Chief of Staff was turned off in 11.1");
    }
    if bool_at(settings, "chiefConfirmNewChat") == Some(false)
        || bool_at(settings, "chiefShowOnAllSpaces") == Some(true)
    {
        return Decision::KeepOff("a Main Hub setting was changed while it was on");
    }
    let opened_since_off_by_default = record.is_some_and(|record| {
        chrono::DateTime::parse_from_rfc3339(&record.created_at)
            .ok()
            .zip(chrono::DateTime::parse_from_rfc3339(OFF_BY_DEFAULT_SINCE).ok())
            // A date that can't be read can't prove anything either way: keep it off.
            .is_none_or(|(created, since)| created >= since)
    });
    if opened_since_off_by_default {
        return Decision::KeepOff("Chief of Staff was opened while Main Hub was off by default");
    }
    Decision::TurnOn
}

/// Whether the pass has settled on this computer.
pub(crate) fn settled(db: &Connection) -> bool {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [SETTLED_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

/// Records that the pass settled, and how.
pub(crate) fn settle(db: &Connection, decision: Decision) -> rusqlite::Result<()> {
    let value = match decision {
        Decision::KeepOff(reason) => format!("{}: {reason}", decision.word()),
        _ => decision.word().to_string(),
    };
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![SETTLED_KEY, value, crate::coordinators::now_iso()],
    )
    .map(|_| ())
}
