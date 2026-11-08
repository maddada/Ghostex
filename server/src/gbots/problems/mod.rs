//! GBot problems and escalations, reported to Chief of Staff.
//!
//! - [`store`]: the `gbots_problems` table (migration 0067).
//! - [`watch`]: the watch over chats nobody supervises (could not start, failed, stalled, waiting
//!   on an answer) and the routine-failure hook; it closes rows once the trouble is over.
//! - [`report`]: what Chief of Staff hears and how (its chat, or a notification), and the mutes.
//! - [`ops`]: `/api/gbots` `problems`, `problemRetry`, `problemRebrief`, `problemClose`, `flag`.

mod ops;
mod report;
mod store;
mod watch;

pub(crate) use ops::{handles, retry, run};
pub(crate) use watch::{chat_did_not_start, routine_run_settled, start_gbots_problem_watch};
