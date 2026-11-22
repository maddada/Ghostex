//! The one `gbots_day_watch` row (migration 0068): the day watch's GBot and routines on this
//! computer, and when Chief of Staff offered it.

use rusqlite::{params, Connection, OptionalExtension};

use crate::coordinators::now_iso;
use crate::domain::DomainStateError;

const ROW_ID: &str = "local";

#[derive(Clone, Debug, Default)]
pub(super) struct DayWatchRecord {
    pub bot_id: Option<String>,
    pub brief_routine_id: Option<String>,
    pub watch_routine_id: Option<String>,
    pub offered_at: Option<String>,
    pub set_up_at: Option<String>,
    pub set_up_by: Option<String>,
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

pub(super) fn read(db: &Connection) -> Result<DayWatchRecord, DomainStateError> {
    db.query_row(
        "SELECT botId, briefRoutineId, watchRoutineId, offeredAt, setUpAt, setUpBy \
         FROM gbots_day_watch WHERE id = ?1",
        [ROW_ID],
        |row| {
            Ok(DayWatchRecord {
                bot_id: row.get(0)?,
                brief_routine_id: row.get(1)?,
                watch_routine_id: row.get(2)?,
                offered_at: row.get(3)?,
                set_up_at: row.get(4)?,
                set_up_by: row.get(5)?,
            })
        },
    )
    .optional()
    .map(Option::unwrap_or_default)
    .map_err(sql)
}

pub(super) fn save(db: &Connection, record: &DayWatchRecord) -> Result<(), DomainStateError> {
    db.execute(
        "INSERT INTO gbots_day_watch \
         (id, botId, briefRoutineId, watchRoutineId, offeredAt, setUpAt, setUpBy, updatedAt) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(id) DO UPDATE SET botId = excluded.botId, \
         briefRoutineId = excluded.briefRoutineId, watchRoutineId = excluded.watchRoutineId, \
         offeredAt = excluded.offeredAt, setUpAt = excluded.setUpAt, setUpBy = excluded.setUpBy, \
         updatedAt = excluded.updatedAt",
        params![
            ROW_ID,
            record.bot_id,
            record.brief_routine_id,
            record.watch_routine_id,
            record.offered_at,
            record.set_up_at,
            record.set_up_by,
            now_iso()
        ],
    )
    .map(|_| ())
    .map_err(sql)
}
