//! gxserver SQLite storage, split by concern. Every submodule is glob re-exported here,
//! so `crate::storage::*` paths are unchanged.

use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::{
    config::write_default_config_if_missing,
    constants::GXSERVER_MIGRATION_IDS,
    paths::GxserverPaths,
    protocol::{
        LegacyMacosLogsImportStatus, LegacyMacosStateImportStatus, MigrationStateImports,
        MigrationStatus,
    },
};

mod maintenance;

pub use maintenance::hold_gxserver_database_open;

mod fair_busy;
mod legacy_backfill;
mod pool;
mod schema_migrations;
mod setup;
#[cfg(test)]
mod tests;

pub(crate) use fair_busy::set_off_worker;
use legacy_backfill::*;
pub use pool::GxserverDatabase;
use pool::{borrow_gxserver_database, open_unpooled_gxserver_database};
pub use schema_migrations::*;
pub use setup::*;
