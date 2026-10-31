//! A command ticket source: a program the user points a workspace at, which gxserver runs like
//! `gh` to learn the source's name, icon and id format (`info`), list its tickets (`list`), read
//! one (`detail`) and create one (`create`). Split by concern: running the program
//! (`runner.rs`), the JSON it reads and answers (`contract.rs`) and its icon (`icon.rs`).

mod contract;
mod icon;
mod runner;

pub(crate) use contract::*;
pub(crate) use icon::*;
pub(crate) use runner::*;

use super::*;
