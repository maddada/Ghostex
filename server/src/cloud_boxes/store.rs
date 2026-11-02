//! The registry tables (migration 0049): provider rows with their sealed keys, box rows, project
//! snapshots and this month's spend. Each row is its wire object as JSON; secrets are sealed with
//! the vault and live only in the `*Sealed` columns. Every function blocks.

use ghostex_gx_protocol::cloud_box::{
    CloudBox, CloudProvider, CloudProviderId, CloudSnapshot, CloudSpendLine,
};
use rusqlite::{params, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::storage::{open_gxserver_database, GxserverDatabase};

fn open(paths: &GxserverPaths) -> Result<GxserverDatabase, DomainStateError> {
    open_gxserver_database(paths)
        .map_err(|error| super::params::internal(format!("SQLite gxserver state error: {error}")))
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    super::params::internal(format!("Cloud Boxes registry error: {error}"))
}

fn to_json<T: Serialize>(value: &T) -> Result<String, DomainStateError> {
    serde_json::to_string(value).map_err(|error| super::params::internal(error.to_string()))
}

fn from_json<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}

fn now() -> String {
    crate::domain::params::now_iso()
}

pub(super) fn seal(paths: &GxserverPaths, plain: &str) -> Result<String, DomainStateError> {
    crate::vault::seal(paths, plain).map_err(super::params::internal)
}

pub(super) fn unseal(paths: &GxserverPaths, sealed: &str) -> Result<String, DomainStateError> {
    crate::vault::unseal(paths, sealed).map_err(|error| match error {
        crate::vault::UnsealError::Unreadable => super::params::not_connected(
            "The saved provider key can't be opened on this computer. Connect the provider again.",
        ),
        crate::vault::UnsealError::Vault(message) => super::params::internal(message),
    })
}

// ---- Providers -----------------------------------------------------------------------------

/// A connected provider: its row and its sealed credentials.
#[derive(Clone)]
pub(super) struct StoredProvider {
    pub(super) row: CloudProvider,
    pub(super) key_sealed: Option<String>,
    pub(super) refresh_sealed: Option<String>,
    /// The id of a key Ghostex minted (to revoke it on disconnect).
    pub(super) key_id: Option<String>,
}

pub(super) fn read_provider(
    paths: &GxserverPaths,
    provider: &CloudProviderId,
) -> Result<Option<StoredProvider>, DomainStateError> {
    let db = open(paths)?;
    db.query_row(
        "SELECT rowJson, keySealed, refreshSealed, keyId FROM cloud_providers WHERE provider = ?1",
        [provider.as_str()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )
    .optional()
    .map_err(sql)
    .map(|found| {
        found.and_then(|(json, key_sealed, refresh_sealed, key_id)| {
            Some(StoredProvider {
                row: from_json(&json)?,
                key_sealed,
                refresh_sealed,
                key_id,
            })
        })
    })
}

pub(super) fn save_provider(
    paths: &GxserverPaths,
    stored: &StoredProvider,
) -> Result<(), DomainStateError> {
    let db = open(paths)?;
    db.execute(
        "INSERT INTO cloud_providers (provider, rowJson, keySealed, refreshSealed, keyId, updatedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(provider) DO UPDATE SET rowJson = excluded.rowJson,
           keySealed = excluded.keySealed, refreshSealed = excluded.refreshSealed,
           keyId = excluded.keyId, updatedAt = excluded.updatedAt",
        params![
            stored.row.id.as_str(),
            to_json(&stored.row)?,
            stored.key_sealed,
            stored.refresh_sealed,
            stored.key_id,
            now()
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// Replaces only the visible row (state, plan, error), keeping the credentials.
pub(super) fn update_provider_row(
    paths: &GxserverPaths,
    row: &CloudProvider,
) -> Result<(), DomainStateError> {
    let db = open(paths)?;
    db.execute(
        "UPDATE cloud_providers SET rowJson = ?2, updatedAt = ?3 WHERE provider = ?1",
        params![row.id.as_str(), to_json(row)?, now()],
    )
    .map_err(sql)?;
    Ok(())
}

pub(super) fn delete_provider(
    paths: &GxserverPaths,
    provider: &CloudProviderId,
) -> Result<(), DomainStateError> {
    let db = open(paths)?;
    db.execute(
        "DELETE FROM cloud_providers WHERE provider = ?1",
        [provider.as_str()],
    )
    .map_err(sql)?;
    Ok(())
}

// ---- Boxes ---------------------------------------------------------------------------------

/// A box row and what only home gxserver keeps about it.
#[derive(Clone)]
pub(super) struct StoredBox {
    pub(super) row: CloudBox,
    pub(super) idempotency_key: Option<String>,
    /// The box gxserver's bearer token from its boot, sealed.
    pub(super) token_sealed: Option<String>,
}

fn box_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(String, Option<String>, Option<String>)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn stored_box(
    (json, idempotency_key, token_sealed): (String, Option<String>, Option<String>),
) -> Option<StoredBox> {
    Some(StoredBox {
        row: from_json(&json)?,
        idempotency_key,
        token_sealed,
    })
}

pub(super) fn list_boxes(paths: &GxserverPaths) -> Result<Vec<StoredBox>, DomainStateError> {
    let db = open(paths)?;
    let mut statement = db
        .prepare_cached(
            "SELECT rowJson, idempotencyKey, tokenSealed FROM cloud_boxes ORDER BY boxId",
        )
        .map_err(sql)?;
    let rows = statement
        .query_map([], box_from_sql)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows.into_iter().filter_map(stored_box).collect())
}

pub(super) fn get_box(
    paths: &GxserverPaths,
    box_id: &str,
) -> Result<Option<StoredBox>, DomainStateError> {
    let db = open(paths)?;
    db.query_row(
        "SELECT rowJson, idempotencyKey, tokenSealed FROM cloud_boxes WHERE boxId = ?1",
        [box_id],
        box_from_sql,
    )
    .optional()
    .map_err(sql)
    .map(|found| found.and_then(stored_box))
}

pub(super) fn find_box_by_idempotency_key(
    paths: &GxserverPaths,
    key: &str,
) -> Result<Option<StoredBox>, DomainStateError> {
    let db = open(paths)?;
    db.query_row(
        "SELECT rowJson, idempotencyKey, tokenSealed FROM cloud_boxes WHERE idempotencyKey = ?1 AND state != 'deleted'",
        [key],
        box_from_sql,
    )
    .optional()
    .map_err(sql)
    .map(|found| found.and_then(stored_box))
}

/// Writes a box row (the progress card is never stored: it lives in the registry overlay).
pub(super) fn save_box(paths: &GxserverPaths, stored: &StoredBox) -> Result<(), DomainStateError> {
    let mut row = stored.row.clone();
    row.progress = None;
    let db = open(paths)?;
    db.execute(
        "INSERT INTO cloud_boxes (boxId, provider, providerBoxId, state, homeProjectId, idempotencyKey,
           deleteAfter, tokenSealed, rowJson, updatedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(boxId) DO UPDATE SET provider = excluded.provider,
           providerBoxId = excluded.providerBoxId, state = excluded.state,
           homeProjectId = excluded.homeProjectId, idempotencyKey = excluded.idempotencyKey,
           deleteAfter = excluded.deleteAfter, tokenSealed = excluded.tokenSealed,
           rowJson = excluded.rowJson, updatedAt = excluded.updatedAt",
        params![
            row.box_id,
            row.provider.as_str(),
            row.provider_box_id,
            row.state.as_str(),
            row.home_project_id,
            stored.idempotency_key,
            row.delete_after,
            stored.token_sealed,
            to_json(&row)?,
            now()
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// Drops `deleted` rows older than `before` (ISO-8601), so the history stays bounded.
pub(super) fn prune_deleted_boxes(
    paths: &GxserverPaths,
    before: &str,
) -> Result<(), DomainStateError> {
    let db = open(paths)?;
    db.execute(
        "DELETE FROM cloud_boxes WHERE state = 'deleted' AND updatedAt < ?1",
        [before],
    )
    .map_err(sql)?;
    Ok(())
}

// ---- Snapshots -----------------------------------------------------------------------------

pub(super) fn list_snapshots(
    paths: &GxserverPaths,
) -> Result<Vec<CloudSnapshot>, DomainStateError> {
    let db = open(paths)?;
    let mut statement = db
        .prepare_cached("SELECT rowJson FROM cloud_snapshots ORDER BY updatedAt DESC")
        .map_err(sql)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows.iter().filter_map(|json| from_json(json)).collect())
}

pub(super) fn get_snapshot(
    paths: &GxserverPaths,
    snapshot_id: &str,
) -> Result<Option<CloudSnapshot>, DomainStateError> {
    let db = open(paths)?;
    db.query_row(
        "SELECT rowJson FROM cloud_snapshots WHERE snapshotId = ?1",
        [snapshot_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(sql)
    .map(|found| found.and_then(|json| from_json(&json)))
}

pub(super) fn save_snapshot(
    paths: &GxserverPaths,
    snapshot: &CloudSnapshot,
) -> Result<(), DomainStateError> {
    let db = open(paths)?;
    db.execute(
        "INSERT INTO cloud_snapshots (snapshotId, provider, projectKey, isDefault, expiresAt, rowJson, updatedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(snapshotId) DO UPDATE SET provider = excluded.provider,
           projectKey = excluded.projectKey, isDefault = excluded.isDefault,
           expiresAt = excluded.expiresAt, rowJson = excluded.rowJson, updatedAt = excluded.updatedAt",
        params![
            snapshot.snapshot_id,
            snapshot.provider.as_str(),
            snapshot.project_key,
            snapshot.is_default as i64,
            snapshot.expires_at,
            to_json(snapshot)?,
            now()
        ],
    )
    .map_err(sql)?;
    Ok(())
}

pub(super) fn delete_snapshot(
    paths: &GxserverPaths,
    snapshot_id: &str,
) -> Result<(), DomainStateError> {
    let db = open(paths)?;
    db.execute(
        "DELETE FROM cloud_snapshots WHERE snapshotId = ?1",
        [snapshot_id],
    )
    .map_err(sql)?;
    Ok(())
}

// ---- Spend ---------------------------------------------------------------------------------

pub(super) fn read_spend(
    paths: &GxserverPaths,
    month: &str,
) -> Result<Vec<CloudSpendLine>, DomainStateError> {
    let db = open(paths)?;
    let mut statement = db
        .prepare_cached("SELECT rowJson FROM cloud_spend WHERE month = ?1 ORDER BY provider")
        .map_err(sql)?;
    let rows = statement
        .query_map([month], |row| row.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows.iter().filter_map(|json| from_json(json)).collect())
}

pub(super) fn save_spend(
    paths: &GxserverPaths,
    month: &str,
    line: &CloudSpendLine,
) -> Result<(), DomainStateError> {
    let db = open(paths)?;
    db.execute(
        "INSERT INTO cloud_spend (month, provider, rowJson, readAt) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(month, provider) DO UPDATE SET rowJson = excluded.rowJson, readAt = excluded.readAt",
        params![month, line.provider.as_str(), to_json(line)?, now()],
    )
    .map_err(sql)?;
    Ok(())
}
