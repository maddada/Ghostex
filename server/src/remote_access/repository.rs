use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

const REMOTE_PAIRING_SECRET_STATE_ID: &str = "global";

/*
CDXC:RemotePairing 2026-09-03:
The pairing secret is stored only as a SHA-256 hash: the plaintext lives in
the QR code (and in the process that minted it) and nowhere on disk, so a
copied state database cannot pair a device. One row, keyed `global`, because
one secret is live at a time; `/api/pairDevice` (M2) consumes it by deleting
the row and the next status poll mints a fresh one.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePairingSecretRecord {
    pub secret_hash: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemotePairedDeviceRecord {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub ssh_key_fingerprint: String,
    pub tailcat_client_key: Option<String>,
    /// SHA-256 of the phone's own API token; `None` for a phone paired before device tokens.
    pub api_token_hash: Option<String>,
    pub paired_at: String,
    pub last_seen_at: Option<String>,
    /// `paired` (pairing v2, with a device key) or `legacy` (paired before 11.0.1: no access
    /// left, listed so the user knows to pair it again).
    pub status: PairedDeviceStatus,
    /// The device key its token is bound to (pairing v2); `None` on a legacy row.
    pub device_key: Option<StoredDeviceKey>,
    /// When the device token lapses unless the device key renews it (RFC 3339).
    pub api_token_expires_at: Option<String>,
    /// The phone's approver key, enrolled when its pairing was approved.
    pub approver_key_id: Option<String>,
    /// A legacy row whose unrestricted SSH key line is already gone.
    pub legacy_key_line_removed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PairedDeviceStatus {
    #[default]
    Paired,
    Legacy,
}

impl PairedDeviceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paired => "paired",
            Self::Legacy => "legacy",
        }
    }

    fn from_db(value: &str) -> Self {
        if value == "paired" {
            Self::Paired
        } else {
            Self::Legacy
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredDeviceKey {
    pub algorithm: String,
    /// base64 standard, as the phone sent it.
    pub public_key: String,
    /// `crate::security::approver_key_id` of the key bytes.
    pub key_id: String,
}

pub struct RemotePairingRepository<'a> {
    db: &'a Connection,
}

impl<'a> RemotePairingRepository<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    pub fn read_secret(&self) -> Result<Option<RemotePairingSecretRecord>> {
        self.db
            .query_row(
                r#"
                SELECT secretHash, expiresAt, createdAt
                FROM remote_pairing_secret
                WHERE stateId = ?1
                "#,
                params![REMOTE_PAIRING_SECRET_STATE_ID],
                |row| {
                    Ok(RemotePairingSecretRecord {
                        secret_hash: row.get(0)?,
                        expires_at: row.get(1)?,
                        created_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .with_context(|| "read remote pairing secret")
    }

    pub fn replace_secret(&self, secret_hash: &str, expires_at: &str) -> Result<()> {
        let created_at = now_iso();
        self.db
            .execute(
                r#"
                INSERT INTO remote_pairing_secret (stateId, secretHash, expiresAt, createdAt)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(stateId) DO UPDATE SET
                  secretHash = excluded.secretHash,
                  expiresAt = excluded.expiresAt,
                  createdAt = excluded.createdAt
                "#,
                params![
                    REMOTE_PAIRING_SECRET_STATE_ID,
                    secret_hash,
                    expires_at,
                    created_at
                ],
            )
            .with_context(|| "replace remote pairing secret")?;
        Ok(())
    }

    /// Returns the stored record when `secret_hash` matches the live secret
    /// and it has not expired at `now_iso`. Used by `/api/pairDevice` (M2).
    pub fn find_valid_secret(
        &self,
        secret_hash: &str,
        now_iso: &str,
    ) -> Result<Option<RemotePairingSecretRecord>> {
        Ok(self.read_secret()?.filter(|record| {
            record.secret_hash == secret_hash && record.expires_at.as_str() > now_iso
        }))
    }

    /// Single use: deletes the secret row so a second registration with the
    /// same code fails. Returns whether a matching row existed.
    pub fn consume_secret(&self, secret_hash: &str) -> Result<bool> {
        let deleted = self
            .db
            .execute(
                "DELETE FROM remote_pairing_secret WHERE stateId = ?1 AND secretHash = ?2",
                params![REMOTE_PAIRING_SECRET_STATE_ID, secret_hash],
            )
            .with_context(|| "consume remote pairing secret")?;
        Ok(deleted > 0)
    }

    /// Validate-and-consume in one statement: the row is deleted only when
    /// the hash matches and it has not expired at `now_iso`, so two
    /// concurrent registrations with the same code cannot both win. Returns
    /// whether this call took the secret.
    pub fn consume_valid_secret(&self, secret_hash: &str, now_iso: &str) -> Result<bool> {
        let deleted = self
            .db
            .execute(
                r#"
                DELETE FROM remote_pairing_secret
                WHERE stateId = ?1 AND secretHash = ?2 AND expiresAt > ?3
                "#,
                params![REMOTE_PAIRING_SECRET_STATE_ID, secret_hash, now_iso],
            )
            .with_context(|| "consume valid remote pairing secret")?;
        Ok(deleted > 0)
    }

    pub fn insert_device(&self, device: &RemotePairedDeviceRecord) -> Result<()> {
        self.db
            .execute(
                r#"
                INSERT INTO remote_paired_devices (
                  id, name, platform, sshKeyFingerprint, tailcatClientKey, apiTokenHash, pairedAt,
                  lastSeenAt, status, deviceKeyAlgorithm, deviceKeyPublic, deviceKeyId,
                  apiTokenExpiresAt, approverKeyId, legacyKeyLineRemoved
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                "#,
                params![
                    device.id,
                    device.name,
                    device.platform,
                    device.ssh_key_fingerprint,
                    device.tailcat_client_key,
                    device.api_token_hash,
                    device.paired_at,
                    device.last_seen_at,
                    device.status.as_str(),
                    device.device_key.as_ref().map(|key| key.algorithm.as_str()),
                    device
                        .device_key
                        .as_ref()
                        .map(|key| key.public_key.as_str()),
                    device.device_key.as_ref().map(|key| key.key_id.as_str()),
                    device.api_token_expires_at,
                    device.approver_key_id,
                    device.legacy_key_line_removed,
                ],
            )
            .with_context(|| "insert remote paired device")?;
        Ok(())
    }

    pub fn list_devices(&self) -> Result<Vec<RemotePairedDeviceRecord>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT id, name, platform, sshKeyFingerprint, tailcatClientKey, pairedAt, lastSeenAt,
                  apiTokenHash, status, deviceKeyAlgorithm, deviceKeyPublic, deviceKeyId,
                  apiTokenExpiresAt, approverKeyId, legacyKeyLineRemoved
                FROM remote_paired_devices
                ORDER BY pairedAt DESC, id ASC
                "#,
            )
            .with_context(|| "prepare remote paired devices list")?;
        let rows = statement
            .query_map([], read_device_row)
            .with_context(|| "list remote paired devices")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| "read remote paired device rows")
    }

    /// Every device's name, newest first, reading only columns every schema since pairing has, so
    /// it works on a database its migrations haven't reached yet (the 11.0.1 notice reads it
    /// before migration 0062 runs, `security_upgrade.rs`).
    pub fn list_device_names(&self) -> Result<Vec<String>> {
        let mut statement = self
            .db
            .prepare("SELECT name FROM remote_paired_devices ORDER BY pairedAt DESC, id ASC")
            .with_context(|| "prepare paired device names")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .with_context(|| "list paired device names")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| "read paired device names")
    }

    pub fn find_device(&self, id: &str) -> Result<Option<RemotePairedDeviceRecord>> {
        self.db
            .query_row(
                r#"
                SELECT id, name, platform, sshKeyFingerprint, tailcatClientKey, pairedAt, lastSeenAt,
                  apiTokenHash, status, deviceKeyAlgorithm, deviceKeyPublic, deviceKeyId,
                  apiTokenExpiresAt, approverKeyId, legacyKeyLineRemoved
                FROM remote_paired_devices
                WHERE id = ?1
                "#,
                params![id],
                read_device_row,
            )
            .optional()
            .with_context(|| "find remote paired device")
    }

    /// Deletes the row and returns it so the caller can undo the side effects
    /// it carries (the `authorized_keys` line, the allow-listed client key).
    pub fn remove_device(&self, id: &str) -> Result<Option<RemotePairedDeviceRecord>> {
        let Some(device) = self.find_device(id)? else {
            return Ok(None);
        };
        self.db
            .execute(
                "DELETE FROM remote_paired_devices WHERE id = ?1",
                params![id],
            )
            .with_context(|| "remove remote paired device")?;
        Ok(Some(device))
    }

    /// Every paired (v2) phone's API token: its hash, its device id and when it lapses. Legacy
    /// rows never carry a working token.
    pub fn list_api_tokens(&self) -> Result<Vec<(String, String, Option<String>)>> {
        let mut statement = self
            .db
            .prepare(
                "SELECT apiTokenHash, id, apiTokenExpiresAt FROM remote_paired_devices
                 WHERE apiTokenHash IS NOT NULL AND status = 'paired'",
            )
            .with_context(|| "prepare paired device token list")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .with_context(|| "list paired device tokens")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| "read paired device tokens")
    }

    /// Replaces a paired phone's token (after its device key signed a renewal).
    pub fn replace_api_token(&self, id: &str, token_hash: &str, expires_at: &str) -> Result<bool> {
        let updated = self
            .db
            .execute(
                "UPDATE remote_paired_devices SET apiTokenHash = ?2, apiTokenExpiresAt = ?3
                 WHERE id = ?1 AND status = 'paired'",
                params![id, token_hash, expires_at],
            )
            .with_context(|| "replace paired device token")?;
        Ok(updated > 0)
    }

    /// Marks a legacy row's SSH key line as gone.
    pub fn mark_legacy_key_line_removed(&self, id: &str) -> Result<()> {
        self.db
            .execute(
                "UPDATE remote_paired_devices SET legacyKeyLineRemoved = 1 WHERE id = ?1",
                params![id],
            )
            .with_context(|| "mark legacy key line removed")?;
        Ok(())
    }

    /// The second Easy Connect key a paired device registered for this computer, if any
    /// (`remote_paired_device_link_keys`, migration 0072).
    pub fn device_link_key(&self, device_id: &str) -> Result<Option<String>> {
        self.db
            .query_row(
                "SELECT clientKey FROM remote_paired_device_link_keys WHERE deviceId = ?1",
                params![device_id],
                |row| row.get(0),
            )
            .optional()
            .with_context(|| "read paired device link key")
    }

    /// Sets a paired device's link key, replacing the one it had.
    pub fn set_device_link_key(&self, device_id: &str, client_key: &str) -> Result<()> {
        self.db
            .execute(
                "INSERT INTO remote_paired_device_link_keys (deviceId, clientKey, updatedAt)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(deviceId) DO UPDATE SET clientKey = excluded.clientKey,
                   updatedAt = excluded.updatedAt",
                params![device_id, client_key, now_iso()],
            )
            .with_context(|| "save paired device link key")?;
        Ok(())
    }

    /// Drops a device's link key and returns it.
    pub fn remove_device_link_key(&self, device_id: &str) -> Result<Option<String>> {
        let key = self.device_link_key(device_id)?;
        self.db
            .execute(
                "DELETE FROM remote_paired_device_link_keys WHERE deviceId = ?1",
                params![device_id],
            )
            .with_context(|| "remove paired device link key")?;
        Ok(key)
    }

    /// A paired device's key for `purpose` (`remote_paired_device_purpose_keys`, migration 0073).
    pub fn device_purpose_key(&self, device_id: &str, purpose: &str) -> Result<Option<String>> {
        self.db
            .query_row(
                "SELECT clientKey FROM remote_paired_device_purpose_keys WHERE deviceId = ?1 AND purpose = ?2",
                params![device_id, purpose],
                |row| row.get(0),
            )
            .optional()
            .with_context(|| "read paired device purpose key")
    }

    /// Sets a paired device's key for `purpose`, replacing the one it had.
    pub fn set_device_purpose_key(
        &self,
        device_id: &str,
        purpose: &str,
        client_key: &str,
    ) -> Result<()> {
        self.db
            .execute(
                "INSERT INTO remote_paired_device_purpose_keys (deviceId, purpose, clientKey, updatedAt)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(deviceId, purpose) DO UPDATE SET clientKey = excluded.clientKey,
                   updatedAt = excluded.updatedAt",
                params![device_id, purpose, client_key, now_iso()],
            )
            .with_context(|| "save paired device purpose key")?;
        Ok(())
    }

    /// Every purpose key of a device, as `(purpose, key)`.
    pub fn device_purpose_keys(&self, device_id: &str) -> Result<Vec<(String, String)>> {
        let mut statement = self
            .db
            .prepare("SELECT purpose, clientKey FROM remote_paired_device_purpose_keys WHERE deviceId = ?1")
            .with_context(|| "prepare paired device purpose keys")?;
        let rows = statement
            .query_map(params![device_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .with_context(|| "list paired device purpose keys")?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| "read paired device purpose keys")
    }

    /// Drops a device's key for `purpose`.
    pub fn remove_device_purpose_key(&self, device_id: &str, purpose: &str) -> Result<()> {
        self.db
            .execute(
                "DELETE FROM remote_paired_device_purpose_keys WHERE deviceId = ?1 AND purpose = ?2",
                params![device_id, purpose],
            )
            .with_context(|| "remove paired device purpose key")?;
        Ok(())
    }

    /// Drops every purpose key of a device and returns them.
    pub fn remove_device_purpose_keys(&self, device_id: &str) -> Result<Vec<(String, String)>> {
        let keys = self.device_purpose_keys(device_id)?;
        self.db
            .execute(
                "DELETE FROM remote_paired_device_purpose_keys WHERE deviceId = ?1",
                params![device_id],
            )
            .with_context(|| "remove paired device purpose keys")?;
        Ok(keys)
    }

    /// Whether any remaining device's link key is `client_key`.
    /// The entries that hold `client_key` as their link key or one of their purpose keys.
    pub fn device_ids_with_link_or_purpose_key(&self, client_key: &str) -> Result<Vec<String>> {
        let mut statement = self
            .db
            .prepare(
                "SELECT deviceId FROM remote_paired_device_link_keys WHERE clientKey = ?1
                 UNION SELECT deviceId FROM remote_paired_device_purpose_keys WHERE clientKey = ?1",
            )
            .with_context(|| "prepare the entries holding a key")?;
        let ids = statement
            .query_map(params![client_key], |row| row.get::<_, String>(0))
            .with_context(|| "read the entries holding a key")?
            .collect::<rusqlite::Result<Vec<String>>>()
            .with_context(|| "read the entries holding a key")?;
        Ok(ids)
    }

    pub fn link_key_in_use(&self, client_key: &str) -> Result<bool> {
        let count: i64 = self
            .db
            .query_row(
                "SELECT (SELECT COUNT(*) FROM remote_paired_device_link_keys l
                   JOIN remote_paired_devices d ON d.id = l.deviceId WHERE l.clientKey = ?1)
                 + (SELECT COUNT(*) FROM remote_paired_device_purpose_keys p
                   JOIN remote_paired_devices d ON d.id = p.deviceId WHERE p.clientKey = ?1)",
                params![client_key],
                |row| row.get(0),
            )
            .with_context(|| "count paired device link keys")?;
        Ok(count > 0)
    }

    pub fn touch_device_seen(&self, id: &str) -> Result<Option<RemotePairedDeviceRecord>> {
        self.db
            .execute(
                "UPDATE remote_paired_devices SET lastSeenAt = ?2 WHERE id = ?1",
                params![id, now_iso()],
            )
            .with_context(|| "update remote paired device lastSeenAt")?;
        self.find_device(id)
    }
}

fn read_device_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemotePairedDeviceRecord> {
    Ok(RemotePairedDeviceRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        platform: row.get(2)?,
        ssh_key_fingerprint: row.get(3)?,
        tailcat_client_key: row.get(4)?,
        paired_at: row.get(5)?,
        last_seen_at: row.get(6)?,
        api_token_hash: row.get(7)?,
        status: PairedDeviceStatus::from_db(&row.get::<_, String>(8)?),
        device_key: match (
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
        ) {
            (Some(algorithm), Some(public_key), Some(key_id)) => Some(StoredDeviceKey {
                algorithm,
                public_key,
                key_id,
            }),
            _ => None,
        },
        api_token_expires_at: row.get(12)?,
        approver_key_id: row.get(13)?,
        legacy_key_line_removed: row.get(14)?,
    })
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
