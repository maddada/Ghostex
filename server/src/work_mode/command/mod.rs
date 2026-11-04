//! A command ticket source: a program the user points a workspace at, which gxserver runs like
//! `gh` to learn the source's name, icon and id format (`info`), list its tickets (`list`), read
//! one (`detail`) and create one (`create`). Split by concern: running the program
//! (`runner.rs`), the JSON it reads and answers (`contract.rs`), what of that answer may show a
//! credential (`answer_guard.rs`), its icon (`icon.rs`), the path the user typed (`path.rs`) and
//! what this computer keeps about each workspace's command (`store/`), the log line for a
//! failed one (`failure_log.rs`), and the background reads for the cards: their queue and workers
//! (`jobs.rs`), what they remember (`caches.rs`), the tickets' last titles (`titles.rs`) and each
//! project's workspace (`project_workspaces.rs`).

mod answer_guard;
mod caches;
mod contract;
mod detail;
mod failure_log;
mod icon;
mod jobs;
mod path;
mod project_workspaces;
mod runner;
mod store;
mod titles;

pub(crate) use answer_guard::*;
pub(crate) use caches::*;
pub(crate) use contract::*;
pub(crate) use detail::*;
pub(crate) use failure_log::*;
pub(crate) use icon::*;
pub(crate) use jobs::*;
pub(crate) use path::*;
pub(crate) use project_workspaces::*;
pub(crate) use runner::*;
pub(crate) use store::*;
pub(crate) use titles::*;

use super::*;
