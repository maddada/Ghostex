//! Automations, split by concern: runtime and scheduler, the HTTP endpoint, run launch,
//! run results, definition normalization, SQLite records, launch targets and the
//! schedule math (whose file also holds the module's unit tests).
mod cron;
mod definitions;
mod endpoint;
mod launch;
mod precheck;
mod records;
mod routine_tick;
mod routines_api;
mod run_results;
mod runtime;
mod schedule;
mod schedule_label;
mod schedule_zone;
mod targets;
mod values;

use definitions::*;
pub use endpoint::*;
use launch::*;
use precheck::run_precheck;
pub(crate) use precheck::{normalize_precheck, precheck_sentence, CALENDAR_TOOL, MAIL_TOOL};
use records::*;
use routine_tick::*;
pub(crate) use routine_tick::{routine_check_state, ROUTINE_MIN_GAP_MS};
pub(crate) use routines_api::*;
use run_results::*;
pub use runtime::*;
use schedule::*;
pub(crate) use schedule_label::{next_run_label, schedule_sentence};
use targets::*;
use values::*;
