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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePairedDeviceRecord {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub ssh_key_fingerprint: String,
    pub tailcat_client_key: Option<String>,
    pub paired_at: String,
    pub last_seen_at: Option<String>,
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
                  id, name, platform, sshKeyFingerprint, tailcatClientKey, pairedAt, lastSeenAt
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    device.id,
                    device.name,
                    device.platform,
                    device.ssh_key_fingerprint,
                    device.tailcat_client_key,
                    device.paired_at,
                    device.last_seen_at,
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
                SELECT id, name, platform, sshKeyFingerprint, tailcatClientKey, pairedAt, lastSeenAt
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

    pub fn find_device(&self, id: &str) -> Result<Option<RemotePairedDeviceRecord>> {
        self.db
            .query_row(
                r#"
                SELECT id, name, platform, sshKeyFingerprint, tailcatClientKey, pairedAt, lastSeenAt
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
    })
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
