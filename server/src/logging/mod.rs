//! gxserver structured JSONL logging, split by concern. Every submodule is glob re-exported
//! here, so `crate::logging::*` paths are unchanged.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;

mod log_files;
mod logger;
mod query;
mod redaction;
#[cfg(test)]
mod tests;

use log_files::*;
pub use logger::*;
pub use query::*;
use redaction::*;

/// What the JSONL writer would persist for `entry`, for tests outside this module that check a
/// line's fields survive redaction.
#[cfg(test)]
pub(crate) fn normalize_log_entry_for_tests(entry: GxserverLogInput) -> Value {
    normalize_log_entry(entry)
}
