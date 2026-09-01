use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use super::types::*;

const TAILCAT_STATE_ID: &str = "global";

pub struct TailcatRepository<'a> {
    db: &'a Connection,
}

impl<'a> TailcatRepository<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    pub fn read_state(&self) -> Result<Option<TailcatStateRecord>> {
        let row = self
            .db
            .query_row(
                r#"
                SELECT enabled, portsCsv, allowedClientKeysCsv, updatedAt
                FROM tailcat_state
                WHERE stateId = ?1
                "#,
                params![TAILCAT_STATE_ID],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .with_context(|| "read tailcat state")?;
        Ok(
            row.map(|(enabled, ports, keys, updated_at)| TailcatStateRecord {
                state: TailcatState {
                    enabled: enabled != 0,
                    ports: parse_ports_csv(&ports),
                    allowed_client_keys: parse_keys_csv(&keys),
                },
                updated_at,
            }),
        )
    }

    pub fn upsert_state(&self, state: TailcatState) -> Result<TailcatStateRecord> {
        let updated_at = now_iso();
        let ports = ports_csv(&state.ports);
        let keys = state.allowed_client_keys.join(",");
        let updated = self
            .db
            .execute(
                r#"
                UPDATE tailcat_state
                SET enabled = ?1,
                    portsCsv = ?2,
                    allowedClientKeysCsv = ?3,
                    updatedAt = ?4
                WHERE stateId = ?5
                "#,
                params![
                    i64::from(state.enabled),
                    ports,
                    keys,
                    updated_at,
                    TAILCAT_STATE_ID,
                ],
            )
            .with_context(|| "update tailcat state")?;
        if updated == 0 {
            self.db
                .execute(
                    r#"
                    INSERT INTO tailcat_state (
                      stateId,
                      enabled,
                      portsCsv,
                      allowedClientKeysCsv,
                      createdAt,
                      updatedAt
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                    "#,
                    params![
                        TAILCAT_STATE_ID,
                        i64::from(state.enabled),
                        ports,
                        keys,
                        updated_at,
                    ],
                )
                .with_context(|| "insert tailcat state")?;
        }
        self.read_state()?
            .with_context(|| "tailcat state missing after upsert")
    }
}

pub fn read_tailcat_state_or_default(repository: &TailcatRepository<'_>) -> Result<TailcatState> {
    Ok(repository
        .read_state()?
        .map(|record| record.state)
        .unwrap_or_else(default_tailcat_state))
}

/// Ports are stored as CSV rather than a child table: the set is tiny, always
/// read and written whole, and the sidecar's own command line is CSV too.
pub fn ports_csv(ports: &[u16]) -> String {
    ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_ports_csv(value: &str) -> Vec<u16> {
    normalize_tailcat_ports(
        value
            .split(',')
            .filter_map(|part| part.trim().parse::<u16>().ok())
            .collect(),
    )
}

fn parse_keys_csv(value: &str) -> Vec<String> {
    normalize_tailcat_client_keys(
        value
            .split(',')
            .map(|part| part.trim().to_string())
            .collect(),
    )
}

/// Deduplicated and sorted so an unchanged set can never look like a change and
/// bounce the sidecar.
pub fn normalize_tailcat_ports(mut ports: Vec<u16>) -> Vec<u16> {
    ports.retain(|port| *port > 0);
    ports.sort_unstable();
    ports.dedup();
    ports
}

pub fn normalize_tailcat_client_keys(mut keys: Vec<String>) -> Vec<String> {
    keys.retain(|key| !key.trim().is_empty());
    for key in &mut keys {
        *key = key.trim().to_string();
    }
    keys.sort();
    keys.dedup();
    keys
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
