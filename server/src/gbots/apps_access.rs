//! Which GBots may only read the user's connected apps (`gbots_bots.appsReadOnly`, migration
//! 0068). Connectors asks here for every call an agent makes, and so does the launch that hands an
//! agent its workspace's `gh` and `gws` sign-ins.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (M8): the day watch reads the user's mail and calendar and never sends, posts, creates, edits or deletes anything in their apps.
//! CDXC:GBots 2026-11-09 WHY:
//! A card asking before a write is not enough here: Chief of Staff may answer the prompts of the GBots it added, so the refusal is made in gxserver, where neither the GBot nor Chief of Staff can approve it, and such a GBot's chats get no `gh`/`gws` sign-in to write with either.
//! SEE-ALSO: server/src/connectors/agent/calls.rs (the refusal), server/src/connectors/cli/launch.rs (no sign-ins), server/src/gbots/day_watch/ (the GBot that is marked).

use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::DomainStateError;

/// Whether the GBot `bot_id` only reads the user's apps.
pub(crate) fn apps_read_only(db: &Connection, bot_id: &str) -> bool {
    db.query_row(
        "SELECT appsReadOnly FROM gbots_bots WHERE botId = ?1",
        [bot_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .is_some_and(|flag| flag != 0)
}

pub(super) fn set_apps_read_only(
    db: &Connection,
    bot_id: &str,
    read_only: bool,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_bots SET appsReadOnly = ?2 WHERE botId = ?1",
        rusqlite::params![bot_id, i64::from(read_only)],
    )
    .map(|_| ())
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    })
}

/// Whether the agents of `project` (a GBot's project) may only read the user's apps.
pub(crate) fn project_reads_apps_only(db: &Connection, project: &Value) -> bool {
    super::project::gbots_bot_id(project).is_some_and(|bot_id| apps_read_only(db, bot_id))
}

/// What an agent of such a GBot is told when a call would change something.
pub(crate) fn read_only_refusal(named: &str) -> String {
    format!(
        "This GBot only reads your apps, so {named} didn't run. Ghostex never lets it send, post, create, change or delete anything there; say in your report what the user could do instead."
    )
}
