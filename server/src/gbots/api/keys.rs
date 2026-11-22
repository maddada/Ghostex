//! `gbots_bot_keys`: a GBot's API key, kept as its SHA-256 only. A GBot's API is on while it has a
//! key that is not revoked; "New key" revokes that row and adds another, turning it off revokes it.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::Rng as _;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::coordinators::now_iso;
use crate::domain::DomainStateError;

/// Every GBot API key starts with this, so a leaked one is easy to recognise and to search for.
pub(crate) const KEY_PREFIX: &str = "gxb_";
/// Random bytes in a key (base64url after the prefix).
const KEY_BYTES: usize = 32;
/// A key's label (who uses it), shown in GBot Settings and its chat's title.
pub(crate) const LABEL_MAX_CHARS: usize = 40;

#[derive(Clone, Debug)]
pub(crate) struct BotKey {
    pub key_id: String,
    pub bot_id: String,
    pub label: String,
    pub created_at: String,
    pub created_by: String,
    pub last_used_at: Option<String>,
    pub uses: i64,
    pub revoked_at: Option<String>,
    /// The key's chats may read the GBot's memory and the shared preferences.
    pub share_memory: bool,
}

const COLUMNS: &str =
    "keyId, botId, label, createdAt, createdBy, lastUsedAt, uses, revokedAt, shareMemory";

fn from_row(row: &Row<'_>) -> rusqlite::Result<BotKey> {
    Ok(BotKey {
        key_id: row.get(0)?,
        bot_id: row.get(1)?,
        label: row.get(2)?,
        created_at: row.get(3)?,
        created_by: row.get(4)?,
        last_used_at: row.get(5)?,
        uses: row.get(6)?,
        revoked_at: row.get(7)?,
        share_memory: row.get::<_, i64>(8)? != 0,
    })
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    }
}

pub(crate) fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.trim().as_bytes()))
}

fn mint_key() -> String {
    let mut random = [0_u8; KEY_BYTES];
    rand::rng().fill_bytes(&mut random);
    format!("{KEY_PREFIX}{}", URL_SAFE_NO_PAD.encode(random))
}

/// The bot's key that is not revoked, if its API is on.
pub(crate) fn live_key(db: &Connection, bot_id: &str) -> Result<Option<BotKey>, DomainStateError> {
    db.query_row(
        &format!(
            "SELECT {COLUMNS} FROM gbots_bot_keys WHERE botId = ?1 AND revokedAt IS NULL ORDER BY createdAt DESC LIMIT 1"
        ),
        [bot_id],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// The bot ids whose API is on (the briefs say so).
pub(crate) fn bots_with_api(db: &Connection) -> Vec<String> {
    let Ok(mut statement) =
        db.prepare("SELECT DISTINCT botId FROM gbots_bot_keys WHERE revokedAt IS NULL")
    else {
        return Vec::new();
    };
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// The live key whose hash is `hash_key(presented)`.
pub(crate) fn key_for(
    db: &Connection,
    presented: &str,
) -> Result<Option<BotKey>, DomainStateError> {
    if !presented.trim().starts_with(KEY_PREFIX) {
        return Ok(None);
    }
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_bot_keys WHERE hash = ?1 AND revokedAt IS NULL"),
        [hash_key(presented)],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// Revokes the bot's live key (if any); answers the revoked row.
pub(crate) fn revoke(db: &Connection, bot_id: &str) -> Result<Option<BotKey>, DomainStateError> {
    let live = live_key(db, bot_id)?;
    db.execute(
        "UPDATE gbots_bot_keys SET revokedAt = ?2 WHERE botId = ?1 AND revokedAt IS NULL",
        params![bot_id, now_iso()],
    )
    .map_err(sql)?;
    Ok(live)
}

/// Adds a key for the bot (revoke the old one first); answers the row and the key itself, which
/// is never stored and never shown again.
pub(crate) fn add_key(
    db: &Connection,
    bot_id: &str,
    label: &str,
    by: &str,
    share_memory: bool,
) -> Result<(BotKey, String), DomainStateError> {
    let key = mint_key();
    let row = BotKey {
        key_id: super::super::record::new_id("k_"),
        bot_id: bot_id.to_string(),
        label: label.trim().chars().take(LABEL_MAX_CHARS).collect(),
        created_at: now_iso(),
        created_by: by.to_string(),
        last_used_at: None,
        uses: 0,
        revoked_at: None,
        share_memory,
    };
    db.execute(
        "INSERT INTO gbots_bot_keys (keyId, botId, hash, label, createdAt, createdBy, uses, shareMemory) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        params![row.key_id, row.bot_id, hash_key(&key), row.label, row.created_at, row.created_by, share_memory as i64],
    )
    .map_err(sql)?;
    Ok((row, key))
}

/// Lets the bot's live key's chats read its memory, or stops them.
pub(crate) fn set_share_memory(
    db: &Connection,
    bot_id: &str,
    on: bool,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_bot_keys SET shareMemory = ?2 WHERE botId = ?1 AND revokedAt IS NULL",
        params![bot_id, on as i64],
    )
    .map_err(sql)?;
    Ok(())
}

/// The key a chat opened as `api:<keyId>` belongs to (revoked or not).
pub(crate) fn key_by_id(db: &Connection, key_id: &str) -> Option<BotKey> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_bot_keys WHERE keyId = ?1"),
        [key_id],
        from_row,
    )
    .ok()
}

/// Notes one use of the key.
pub(crate) fn touch(db: &Connection, key_id: &str) {
    let _ = db.execute(
        "UPDATE gbots_bot_keys SET lastUsedAt = ?2, uses = uses + 1 WHERE keyId = ?1",
        params![key_id, now_iso()],
    );
}

/// A key as `/api/gbots {op: "api"}` answers it (never the key or its hash).
pub(crate) fn key_summary(key: &BotKey) -> Value {
    json!({
        "keyId": key.key_id,
        "label": key.label,
        "createdAt": key.created_at,
        "createdBy": key.created_by,
        "lastUsedAt": key.last_used_at,
        "uses": key.uses,
        "revokedAt": key.revoked_at,
        "shareMemory": key.share_memory,
    })
}
