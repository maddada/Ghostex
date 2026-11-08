//! Automations, split by concern: runtime and scheduler, the HTTP endpoint, run launch,
//! run results, definition normalization, SQLite records, launch targets and the
//! schedule math (whose file also holds the module's unit tests).
mod definitions;
mod endpoint;
mod launch;
mod records;
mod run_results;
mod runtime;
mod schedule;
mod schedule_zone;
mod targets;
mod values;

use definitions::*;
pub use endpoint::*;
use launch::*;
use records::*;
use run_results::*;
pub use runtime::*;
use schedule::*;
use targets::*;
use values::*;
