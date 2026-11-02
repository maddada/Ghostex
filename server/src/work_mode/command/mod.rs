//! A command ticket source: a program the user points a workspace at, which gxserver runs like
//! `gh` to learn the source's name, icon and id format (`info`), list its tickets (`list`), read
//! one (`detail`) and create one (`create`). Split by concern: running the program
//! (`runner.rs`), the JSON it reads and answers (`contract.rs`), what of that answer may show a
//! credential (`answer_guard.rs`), its icon (`icon.rs`), the path the user typed (`path.rs`) and
//! what this computer keeps about each workspace's command (`store/`) and the log line for a
//! failed one (`failure_log.rs`).

mod answer_guard;
mod contract;
mod failure_log;
mod icon;
mod path;
mod runner;
mod store;

pub(crate) use answer_guard::*;
pub(crate) use contract::*;
pub(crate) use failure_log::*;
pub(crate) use icon::*;
pub(crate) use path::*;
pub(crate) use runner::*;
pub(crate) use store::*;

use super::*;
