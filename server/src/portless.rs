use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    paths::GxserverPaths,
    platform::{resources, shell::command_shell},
    storage::open_gxserver_database,
    toolchain::require_bundled_zmx,
};

const PORTLESS_STATE_ID: &str = "global";
const MAX_STABLE_KEY_LEN: usize = 160;
const MAX_HOST_LABEL_LEN: usize = 63;
const STABLE_SUFFIX_HEX_LENGTHS: &[usize] = &[8, 10, 12, 16, 24, 32, 48];
const PORTLESS_ROUTES_FILE: &str = "routes.json";
const PORTLESS_ROUTES_LOCK: &str = "routes.lock";
const PORTLESS_FILE_MODE: u32 = 0o644;
const PORTLESS_DIR_MODE: u32 = 0o755;
const PORTLESS_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const PORTLESS_LOCK_RETRY_DELAY: Duration = Duration::from_millis(10);
const PORTLESS_STALE_LOCK_AGE: Duration = Duration::from_secs(10);
const PORTLESS_LISTENER_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
const PORTLESS_LISTENER_SNAPSHOT_STDOUT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const PORTLESS_LISTENER_SNAPSHOT_STDERR_LIMIT_BYTES: usize = 64 * 1024;
const PORTLESS_PRIMARY_ROUTE_PORT_PREFERENCE: &[u16] = &[3000, 5173, 5174, 8080, 8000];
const PORTLESS_SERVICE_LABEL: &str = "sh.portless.proxy";
const PORTLESS_SERVICE_PLIST_PATH: &str = "/Library/LaunchDaemons/sh.portless.proxy.plist";
const PORTLESS_SERVICE_TLD: &str = "localhost";
const PORTLESS_SERVICE_REACHABILITY_TIMEOUT: Duration = Duration::from_millis(250);
static PORTLESS_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/*
CDXC:PortlessPersistence 2026-06-22-22:41:
gxserver-rs owns Portless durable metadata in SQLite during the first local macOS integration. Metadata APIs stay database-only; Phase 6 adds an explicit route-sync API for the separate Ghostex-managed Portless state directory.

CDXC:PortlessPersistence 2026-06-22-22:41:
Persist project and worktree slugs separately from project display names, worktree display names, paths, branches, and terminal content. Phase 4 accepts explicit slugs only; slug generation, backfill, collision suffixing, and rename-derived changes are Phase 5 work.

CDXC:PortlessPersistence 2026-06-22-22:41:
Setup and runtime state is metadata-only and enum-like. Keep it limited to enabled/protocol/ownership/status values so persistence cannot store project names, worktree names, paths, URLs, hostnames, command text, tokens, environment values, or user content.

CDXC:PortlessSlugAllocation 2026-06-22-22:49:
Generated project slugs and worktree suffixes are one-time durable metadata. ensure_* APIs must return existing rows before inspecting display names, branches, or paths so user-facing renames cannot silently change local domains.

CDXC:PortlessSlugAllocation 2026-06-22-22:49:
Worktree domain parts use the parent project's persisted project slug plus a separately persisted suffix. At first allocation, the suffix source order is stored worktree name, then the last branch segment, then a deterministic worktree-key fallback; raw branch, path, and display-name text may only influence the normalized hostname-safe label.

CDXC:PortlessSlugAllocation 2026-06-22-22:49:
Collision handling is append-only: the earliest existing or backfilled record keeps the clean label, while later records receive a deterministic stable-id suffix. Existing persisted labels are reserved and never reshuffled during later backfills.

CDXC:PortlessState 2026-06-22-23:05:
Portless active route state is mirrored directly to the resolved Ghostex Portless state directory using Portless 0.14.0's array schema with hostname, port, and pid fields. Ghostex live routes must carry the actual listener pid; pid 0 remains Portless's static-alias convention and is rejected here.

CDXC:PortlessState 2026-06-22-23:05:
The Portless package serializes empty route sets as an empty routes.json array rather than removing the file, so Ghostex cleanup writes [] to replace stale routes. The writer takes routes.lock as a directory lock, writes a same-directory temp file, flushes it, then renames over routes.json without persistent logging.

CDXC:PortlessOwnership 2026-06-22-23:15:
Phase 7 listener detection may only adopt dev servers whose listener pid is in the live process tree of a running Ghostex zmx session. Do not infer ownership from cwd, project-looking names, command text, hostnames, URLs, or stale session rows.

CDXC:PortlessOwnership 2026-06-22-23:15:
Detected listeners are temporary metadata only. A listener must appear in the current TCP listener snapshot and the current zmx-rooted process tree; when either disappears, the computed desired listener set drops it instead of preserving a fallback route.

CDXC:PortlessRouteNaming 2026-06-22-23:28:
Desired route computation is metadata-only: convert only the supplied live owned listeners to temporary PortlessRoute values, use persisted or backfilled slugs for base domains, preserve listener port and pid, and never touch routes.json or Portless state files.

CDXC:PortlessRouteNaming 2026-06-22-23:28:
Each project/worktree group has one primary domain selected by 3000, 5173, 5174, 8080, 8000, then the lowest remaining port. Other live listeners use p<port>.<base-domain> so multiple live servers in one group stay addressable without permanent service names.

CDXC:PortlessBackgroundSync 2026-06-22-23:40:
Phase 9 route sync is policy-driven and independent of Resources/sidebar polling. When Portless is enabled, gxserver-rs computes desired routes from the current live Ghostex-owned listener set, mirrors them only for Ghostex-owned active setup metadata, writes [] for disabled state, and skips setup-missing/failed/non-Ghostex states instead of inventing service detection.

CDXC:PortlessBackgroundSync 2026-06-22-23:40:
The current gxserver health, presentation, and sidebar contracts have no Portless status field. Keep setup-needed/setup-failed/status as an internal sync outcome until Phase 12 adds a metadata-only wire contract, rather than broadcasting an ad hoc UI payload.

CDXC:PortlessServiceDetection 2026-06-22-23:58:
Phase 10 detects the global macOS Portless service from launchd plist metadata before route sync. The classification stores only setup/runtime enums: missing means Install, standalone means takeover prompt, Ghostex config mismatch means Reconfigure, Ghostex unreachable means Retry, and active means routes may be mirrored.

CDXC:PortlessServiceDetection 2026-06-22-23:58:
Ghostex ownership requires the launchd service to use Ghostex's bundled code-server Node, bundled Portless CLI, and resolved Portless state directory. HTTPS/HTTP, standard proxy port, .localhost TLD, LAN off, wildcard off, expected Node, expected CLI, expected state dir, hosts sync off, and non-persistent launchd stdout/stderr sinks are strict reconfigure facts; first-version Ghostex must not accept LAN service config or persistent proxy output files.

CDXC:PortlessServiceDetection 2026-06-23-05:11:
Phase 10-12 verification requires old Ghostex-marked launchd plists to be treated as reconfigure-needed when they would let the root Portless service write /etc/hosts or persist proxy stdout/stderr under Ghostex support-bundle state. Service inspection therefore checks PORTLESS_SYNC_HOSTS=0 plus /dev/null launchd output paths as ownership facts, not optional diagnostics.

CDXC:PortlessProtocol 2026-06-23-00:25:
Phase 12 exposes Portless to health and presentation clients as metadata-only protocol payloads. Status, action availability, and route previews may carry enums, counts, stable project/session ids, protocol, hostnames, and ports, but never paths, command text, env values, process output, tokens, cookies, full URLs, query strings, terminal text, or file contents.

CDXC:PortlessProtocol 2026-06-23-00:25:
gxserver-rs only describes native admin actions; it does not advertise them as directly runnable because privileged setup is local-mac native-sidebar work in the first version. Non-local and remote gxserver consumers must see unavailable action booleans and can still render state and route previews without reading Portless files.

CDXC:PortlessLogging 2026-06-23-04:45:
Phase 17 Portless operational logs are support diagnostics, not route or service dumps. Persist only structured counts, booleans, enum states, protocol, setup/runtime state, fixed error codes, and durations through the gxserver logger; never put project/worktree names, paths, full URLs, hostnames, command text, env values, tokens, secrets, stdout, or stderr into Portless log payloads.
*/
pub struct PortlessRepository<'a> {
    db: &'a Connection,
}

impl<'a> PortlessRepository<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    pub fn backfill_domain_identities(&self) -> Result<PortlessDomainIdentities> {
        let project_rows = self.list_registered_project_rows()?;
        self.backfill_project_slugs_for_rows(&project_rows)?;
        self.backfill_worktree_slugs_for_rows(&project_rows)?;
        self.domain_identities_for_rows(&project_rows)
    }

    pub fn ensure_project_slug(&self, project_id: &str) -> Result<PortlessProjectSlug> {
        validate_stable_key("projectId", project_id)?;
        if let Some(existing) = self.read_project_slug(project_id)? {
            return Ok(existing);
        }

        let project_rows = self.list_registered_project_rows()?;
        let row = project_rows
            .iter()
            .find(|row| row.project_id == project_id)
            .with_context(|| "Portless project slug requested for an unknown project row")?;
        ensure!(
            row.worktree.is_none(),
            "Portless worktree project rows use ensure_worktree_slug."
        );
        self.backfill_project_slugs_for_rows(&project_rows)?;
        self.read_project_slug(project_id)?
            .with_context(|| "Portless project slug missing after allocation")
    }

    pub fn ensure_worktree_slug(
        &self,
        worktree_project_id: &str,
    ) -> Result<PortlessWorktreeDomainParts> {
        validate_stable_key("worktreeProjectId", worktree_project_id)?;
        if let Some(existing) = self.read_worktree_domain_parts(worktree_project_id)? {
            return Ok(existing);
        }

        let project_rows = self.list_registered_project_rows()?;
        let row = project_rows
            .iter()
            .find(|row| row.project_id == worktree_project_id)
            .with_context(|| "Portless worktree slug requested for an unknown project row")?;
        ensure!(
            row.worktree.is_some(),
            "Portless project rows use ensure_project_slug."
        );
        self.backfill_project_slugs_for_rows(&project_rows)?;
        self.backfill_worktree_slugs_for_rows(&project_rows)?;
        self.read_worktree_domain_parts(worktree_project_id)?
            .with_context(|| "Portless worktree slug missing after allocation")
    }

    pub fn list_project_slugs(&self) -> Result<Vec<PortlessProjectSlug>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT projectId, projectSlug, createdAt, updatedAt
                FROM portless_domain_identities
                WHERE identityScope = 'project'
                ORDER BY projectId ASC
                "#,
            )
            .with_context(|| "prepare Portless project slug list")?;
        let rows = statement
            .query_map([], project_slug_from_row)
            .with_context(|| "query Portless project slug list")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .with_context(|| "read Portless project slug list")?;
        Ok(rows)
    }

    pub fn list_worktree_slugs(&self) -> Result<Vec<PortlessWorktreeSlug>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT projectId, worktreeKey, worktreeSlug, createdAt, updatedAt
                FROM portless_domain_identities
                WHERE identityScope = 'worktree'
                ORDER BY projectId ASC, worktreeKey ASC
                "#,
            )
            .with_context(|| "prepare Portless worktree slug list")?;
        let rows = statement
            .query_map([], worktree_slug_from_row)
            .with_context(|| "query Portless worktree slug list")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .with_context(|| "read Portless worktree slug list")?;
        Ok(rows)
    }

    pub fn read_worktree_domain_parts(
        &self,
        worktree_project_id: &str,
    ) -> Result<Option<PortlessWorktreeDomainParts>> {
        validate_stable_key("worktreeProjectId", worktree_project_id)?;
        let Some(row) = self.read_registered_project_row(worktree_project_id)? else {
            return Ok(None);
        };
        let Some(worktree) = row.worktree.as_ref() else {
            return Ok(None);
        };
        let worktree_key = stable_worktree_key(&row);
        let Some(worktree_slug) =
            self.read_worktree_slug(&worktree.parent_project_id, &worktree_key)?
        else {
            return Ok(None);
        };
        let Some(project_slug) = self.read_project_slug(&worktree.parent_project_id)? else {
            return Ok(None);
        };
        Ok(Some(PortlessWorktreeDomainParts {
            parent_project_id: worktree.parent_project_id.clone(),
            project_slug: project_slug.slug,
            worktree_project_id: row.project_id,
            worktree_key: worktree_slug.worktree_key,
            worktree_slug: worktree_slug.slug,
            created_at: worktree_slug.created_at,
            updated_at: worktree_slug.updated_at,
        }))
    }

    pub fn upsert_project_slug(&self, project_id: &str, slug: &str) -> Result<PortlessProjectSlug> {
        validate_stable_key("projectId", project_id)?;
        validate_slug("projectSlug", slug)?;

        let updated_at = now_iso();
        let updated = self
            .db
            .execute(
                r#"
                UPDATE portless_domain_identities
                SET projectSlug = ?2,
                    updatedAt = ?3
                WHERE identityScope = 'project'
                  AND projectId = ?1
                "#,
                params![project_id, slug, updated_at],
            )
            .with_context(|| "update Portless project slug")?;
        if updated == 0 {
            self.db
                .execute(
                    r#"
                    INSERT INTO portless_domain_identities (
                      identityScope,
                      projectId,
                      projectSlug,
                      createdAt,
                      updatedAt
                    )
                    VALUES ('project', ?1, ?2, ?3, ?3)
                    "#,
                    params![project_id, slug, updated_at],
                )
                .with_context(|| "insert Portless project slug")?;
        }

        self.read_project_slug(project_id)?
            .with_context(|| "Portless project slug missing after upsert")
    }

    pub fn read_project_slug(&self, project_id: &str) -> Result<Option<PortlessProjectSlug>> {
        validate_stable_key("projectId", project_id)?;
        self.db
            .query_row(
                r#"
                SELECT projectId, projectSlug, createdAt, updatedAt
                FROM portless_domain_identities
                WHERE identityScope = 'project'
                  AND projectId = ?1
                "#,
                params![project_id],
                project_slug_from_row,
            )
            .optional()
            .with_context(|| "read Portless project slug")
    }

    pub fn upsert_worktree_slug(
        &self,
        project_id: &str,
        worktree_key: &str,
        slug: &str,
    ) -> Result<PortlessWorktreeSlug> {
        validate_stable_key("projectId", project_id)?;
        validate_stable_key("worktreeKey", worktree_key)?;
        validate_slug("worktreeSlug", slug)?;

        let updated_at = now_iso();
        let updated = self
            .db
            .execute(
                r#"
                UPDATE portless_domain_identities
                SET worktreeSlug = ?3,
                    updatedAt = ?4
                WHERE identityScope = 'worktree'
                  AND projectId = ?1
                  AND worktreeKey = ?2
                "#,
                params![project_id, worktree_key, slug, updated_at],
            )
            .with_context(|| "update Portless worktree slug")?;
        if updated == 0 {
            self.db
                .execute(
                    r#"
                    INSERT INTO portless_domain_identities (
                      identityScope,
                      projectId,
                      worktreeKey,
                      worktreeSlug,
                      createdAt,
                      updatedAt
                    )
                    VALUES ('worktree', ?1, ?2, ?3, ?4, ?4)
                    "#,
                    params![project_id, worktree_key, slug, updated_at],
                )
                .with_context(|| "insert Portless worktree slug")?;
        }

        self.read_worktree_slug(project_id, worktree_key)?
            .with_context(|| "Portless worktree slug missing after upsert")
    }

    pub fn read_worktree_slug(
        &self,
        project_id: &str,
        worktree_key: &str,
    ) -> Result<Option<PortlessWorktreeSlug>> {
        validate_stable_key("projectId", project_id)?;
        validate_stable_key("worktreeKey", worktree_key)?;
        self.db
            .query_row(
                r#"
                SELECT projectId, worktreeKey, worktreeSlug, createdAt, updatedAt
                FROM portless_domain_identities
                WHERE identityScope = 'worktree'
                  AND projectId = ?1
                  AND worktreeKey = ?2
                "#,
                params![project_id, worktree_key],
                worktree_slug_from_row,
            )
            .optional()
            .with_context(|| "read Portless worktree slug")
    }

    pub fn upsert_state(&self, state: PortlessState) -> Result<PortlessStateRecord> {
        let updated_at = now_iso();
        let updated = self
            .db
            .execute(
                r#"
                UPDATE portless_state
                SET enabled = ?1,
                    protocol = ?2,
                    setupOwnership = ?3,
                    setupStatus = ?4,
                    runtimeStatus = ?5,
                    updatedAt = ?6
                WHERE stateId = ?7
                "#,
                params![
                    bool_to_sql(state.enabled),
                    state.protocol.as_str(),
                    state.setup_ownership.as_str(),
                    state.setup_status.as_str(),
                    state.runtime_status.as_str(),
                    updated_at,
                    PORTLESS_STATE_ID,
                ],
            )
            .with_context(|| "update Portless state")?;
        if updated == 0 {
            self.db
                .execute(
                    r#"
                    INSERT INTO portless_state (
                      stateId,
                      enabled,
                      protocol,
                      setupOwnership,
                      setupStatus,
                      runtimeStatus,
                      createdAt,
                      updatedAt
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                    "#,
                    params![
                        PORTLESS_STATE_ID,
                        bool_to_sql(state.enabled),
                        state.protocol.as_str(),
                        state.setup_ownership.as_str(),
                        state.setup_status.as_str(),
                        state.runtime_status.as_str(),
                        updated_at,
                    ],
                )
                .with_context(|| "insert Portless state")?;
        }

        self.read_state()?
            .with_context(|| "Portless state missing after upsert")
    }

    pub fn read_state(&self) -> Result<Option<PortlessStateRecord>> {
        let row = self
            .db
            .query_row(
                r#"
                SELECT
                  enabled,
                  protocol,
                  setupOwnership,
                  setupStatus,
                  runtimeStatus,
                  createdAt,
                  updatedAt
                FROM portless_state
                WHERE stateId = ?1
                "#,
                params![PORTLESS_STATE_ID],
                |row| {
                    Ok(PortlessStateStorageRow {
                        enabled: row.get(0)?,
                        protocol: row.get(1)?,
                        setup_ownership: row.get(2)?,
                        setup_status: row.get(3)?,
                        runtime_status: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .with_context(|| "read Portless state")?;
        row.map(portless_state_from_storage).transpose()
    }

    fn list_registered_project_rows(&self) -> Result<Vec<PortlessProjectBackfillRow>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT projectId, name, path, worktreeJson
                FROM projects
                ORDER BY createdAt ASC, projectId ASC
                "#,
            )
            .with_context(|| "prepare registered projects for Portless backfill")?;
        let rows = statement
            .query_map([], |row| {
                Ok(RawProjectBackfillRow {
                    project_id: row.get("projectId")?,
                    name: row.get("name")?,
                    path: row.get("path")?,
                    worktree_json: row.get("worktreeJson")?,
                })
            })
            .with_context(|| "query registered projects for Portless backfill")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .with_context(|| "read registered projects for Portless backfill")?;
        rows.into_iter()
            .map(PortlessProjectBackfillRow::from_raw)
            .collect()
    }

    fn read_registered_project_row(
        &self,
        project_id: &str,
    ) -> Result<Option<PortlessProjectBackfillRow>> {
        validate_stable_key("projectId", project_id)?;
        let row = self
            .db
            .query_row(
                r#"
                SELECT projectId, name, path, worktreeJson
                FROM projects
                WHERE projectId = ?1
                "#,
                params![project_id],
                |row| {
                    Ok(RawProjectBackfillRow {
                        project_id: row.get("projectId")?,
                        name: row.get("name")?,
                        path: row.get("path")?,
                        worktree_json: row.get("worktreeJson")?,
                    })
                },
            )
            .optional()
            .with_context(|| "read registered project for Portless identity")?;
        row.map(PortlessProjectBackfillRow::from_raw).transpose()
    }

    fn backfill_project_slugs_for_rows(&self, rows: &[PortlessProjectBackfillRow]) -> Result<()> {
        let existing = self.list_project_slugs()?;
        let mut existing_by_project_id = HashMap::new();
        let mut reserved_slugs = HashSet::new();
        for record in existing {
            reserved_slugs.insert(record.slug.clone());
            existing_by_project_id.insert(record.project_id, record.slug);
        }

        for row in rows.iter().filter(|row| row.worktree.is_none()) {
            if existing_by_project_id.contains_key(&row.project_id) {
                continue;
            }
            let base_slug = project_base_slug(row);
            let slug = allocate_slug(&reserved_slugs, &base_slug, "project", &row.project_id)?;
            self.insert_project_slug_once(&row.project_id, &slug)?;
            reserved_slugs.insert(slug.clone());
            existing_by_project_id.insert(row.project_id.clone(), slug);
        }
        Ok(())
    }

    fn backfill_worktree_slugs_for_rows(&self, rows: &[PortlessProjectBackfillRow]) -> Result<()> {
        let project_slugs = self.list_project_slugs()?;
        let project_slug_by_id = project_slugs
            .into_iter()
            .map(|record| (record.project_id, record.slug))
            .collect::<HashMap<_, _>>();
        let known_project_ids = rows
            .iter()
            .map(|row| row.project_id.as_str())
            .collect::<HashSet<_>>();
        let existing = self.list_worktree_slugs()?;
        let mut existing_by_identity = HashSet::new();
        let mut reserved_by_parent = HashMap::<String, HashSet<String>>::new();
        for record in existing {
            reserved_by_parent
                .entry(record.project_id.clone())
                .or_default()
                .insert(record.slug.clone());
            existing_by_identity.insert((record.project_id, record.worktree_key));
        }

        for row in rows.iter().filter(|row| row.worktree.is_some()) {
            let worktree = row
                .worktree
                .as_ref()
                .with_context(|| "Portless worktree metadata missing during backfill")?;
            validate_stable_key("parentProjectId", &worktree.parent_project_id)?;
            ensure!(
                known_project_ids.contains(worktree.parent_project_id.as_str()),
                "Portless worktree parent project is not registered."
            );
            ensure!(
                project_slug_by_id.contains_key(&worktree.parent_project_id),
                "Portless worktree parent project slug is missing."
            );
            let worktree_key = stable_worktree_key(row);
            validate_stable_key("worktreeKey", &worktree_key)?;
            let identity_key = (worktree.parent_project_id.clone(), worktree_key.clone());
            if existing_by_identity.contains(&identity_key) {
                continue;
            }
            let base_slug = worktree_base_slug(worktree, &worktree_key);
            let reserved_slugs = reserved_by_parent
                .entry(worktree.parent_project_id.clone())
                .or_default();
            let suffix_stable_id = format!("{}\0{}", worktree.parent_project_id, worktree_key);
            let slug = allocate_slug(reserved_slugs, &base_slug, "worktree", &suffix_stable_id)?;
            self.insert_worktree_slug_once(&worktree.parent_project_id, &worktree_key, &slug)?;
            reserved_slugs.insert(slug);
            existing_by_identity.insert(identity_key);
        }
        Ok(())
    }

    fn domain_identities_for_rows(
        &self,
        rows: &[PortlessProjectBackfillRow],
    ) -> Result<PortlessDomainIdentities> {
        let mut projects = Vec::new();
        for row in rows.iter().filter(|row| row.worktree.is_none()) {
            if let Some(record) = self.read_project_slug(&row.project_id)? {
                projects.push(record);
            }
        }
        let mut worktrees = Vec::new();
        for row in rows.iter().filter(|row| row.worktree.is_some()) {
            if let Some(record) = self.read_worktree_domain_parts(&row.project_id)? {
                worktrees.push(record);
            }
        }
        Ok(PortlessDomainIdentities {
            projects,
            worktrees,
        })
    }

    fn insert_project_slug_once(
        &self,
        project_id: &str,
        slug: &str,
    ) -> Result<PortlessProjectSlug> {
        validate_stable_key("projectId", project_id)?;
        validate_slug("projectSlug", slug)?;
        let updated_at = now_iso();
        self.db
            .execute(
                r#"
                INSERT INTO portless_domain_identities (
                  identityScope,
                  projectId,
                  projectSlug,
                  createdAt,
                  updatedAt
                )
                VALUES ('project', ?1, ?2, ?3, ?3)
                "#,
                params![project_id, slug, updated_at],
            )
            .with_context(|| "insert generated Portless project slug")?;
        self.read_project_slug(project_id)?
            .with_context(|| "generated Portless project slug missing after insert")
    }

    fn insert_worktree_slug_once(
        &self,
        project_id: &str,
        worktree_key: &str,
        slug: &str,
    ) -> Result<PortlessWorktreeSlug> {
        validate_stable_key("projectId", project_id)?;
        validate_stable_key("worktreeKey", worktree_key)?;
        validate_slug("worktreeSlug", slug)?;
        let updated_at = now_iso();
        self.db
            .execute(
                r#"
                INSERT INTO portless_domain_identities (
                  identityScope,
                  projectId,
                  worktreeKey,
                  worktreeSlug,
                  createdAt,
                  updatedAt
                )
                VALUES ('worktree', ?1, ?2, ?3, ?4, ?4)
                "#,
                params![project_id, worktree_key, slug, updated_at],
            )
            .with_context(|| "insert generated Portless worktree slug")?;
        self.read_worktree_slug(project_id, worktree_key)?
            .with_context(|| "generated Portless worktree slug missing after insert")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PortlessRoute {
    pub hostname: String,
    pub port: u16,
    pub pid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessOwnedListener {
    pub project_id: String,
    pub session_id: String,
    pub zmx_name: String,
    pub worktree_parent_project_id: Option<String>,
    pub port: u16,
    pub pid: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessBackgroundRouteAction {
    ClearMirroredRoutes,
    MirrorDesiredRoutes,
    SkipRouteFileWrite,
}

impl PortlessBackgroundRouteAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::ClearMirroredRoutes => "clearMirroredRoutes",
            Self::MirrorDesiredRoutes => "mirrorDesiredRoutes",
            Self::SkipRouteFileWrite => "skipRouteFileWrite",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessBackgroundStatus {
    Disabled,
    SetupActive,
    SetupFailed,
    SetupNeeded,
    SetupUnknown,
}

impl PortlessBackgroundStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::SetupActive => "setupActive",
            Self::SetupFailed => "setupFailed",
            Self::SetupNeeded => "setupNeeded",
            Self::SetupUnknown => "setupUnknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessBackgroundSyncOutcome {
    pub action: PortlessBackgroundRouteAction,
    pub desired_route_count: usize,
    pub live_listener_count: usize,
    pub status: PortlessBackgroundStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessServiceClassification {
    Missing,
    GhostexActive,
    GhostexConfigMismatch,
    GhostexFailed,
    Standalone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortlessServiceReachability {
    pub manager_running: Option<bool>,
    pub proxy_reachable: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessServiceInspection {
    pub classification: PortlessServiceClassification,
    pub mismatch_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessLogErrorCode {
    BackgroundSyncFailed,
    BackgroundSyncTaskJoinFailed,
    StateUpdateDatabaseUnavailable,
    StateUpdateFailed,
}

impl PortlessLogErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::BackgroundSyncFailed => "backgroundSyncFailed",
            Self::BackgroundSyncTaskJoinFailed => "backgroundSyncTaskJoinFailed",
            Self::StateUpdateDatabaseUnavailable => "stateUpdateDatabaseUnavailable",
            Self::StateUpdateFailed => "stateUpdateFailed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessStatusPayload {
    pub actions: PortlessAdminActionSet,
    pub enabled: bool,
    pub protocol: PortlessProtocol,
    pub runtime_status: PortlessRuntimeStatus,
    pub setup_ownership: PortlessSetupOwnership,
    pub setup_status: PortlessSetupStatus,
    pub source_status: PortlessPayloadSourceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/*
CDXC:PortlessFailureUX 2026-06-23-04:28:
Phase 16 makes Portless recovery state daemon-owned: protocol changes, admin
success/failure, retry, disable, and explicit service removal are persisted as
enum metadata so React can recover without reading Portless files or inventing
local fallback state.
*/
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PortlessStateUpdate {
    SetEnabled {
        enabled: bool,
    },
    SetProtocol {
        protocol: PortlessProtocol,
    },
    RecordAdminResult {
        action: PortlessAdminResultAction,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol: Option<PortlessProtocol>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessAdminResultAction {
    Install,
    Reconfigure,
    Remove,
    Retry,
}

impl PortlessAdminResultAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Reconfigure => "reconfigure",
            Self::Remove => "remove",
            Self::Retry => "retry",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessAdminActionSet {
    pub install: PortlessAdminActionAvailability,
    pub reconfigure: PortlessAdminActionAvailability,
    pub remove: PortlessAdminActionAvailability,
    pub retry: PortlessAdminActionAvailability,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessAdminActionAvailability {
    pub available: bool,
    pub local_mac_only: bool,
    pub recommended: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<PortlessAdminActionUnavailableReason>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessAdminActionUnavailableReason {
    NativeAdminBridgeRequired,
    NotRecommended,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessPresentationPayload {
    pub assigned_domains: Vec<PortlessAssignedDomain>,
    pub live_listener_count: usize,
    pub route_preview_status: PortlessRoutePreviewStatus,
    pub route_previews: Vec<PortlessRoutePreview>,
    pub status: PortlessStatusPayload,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessAssignedDomain {
    pub hostname: String,
    pub kind: PortlessAssignedDomainKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_project_id: Option<String>,
    pub project_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessAssignedDomainKind {
    Project,
    Worktree,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessRoutePreview {
    pub hostname: String,
    pub kind: PortlessRoutePreviewKind,
    pub port: u16,
    pub project_id: String,
    pub protocol: PortlessProtocol,
    pub session_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessRoutePreviewKind {
    Additional,
    Primary,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessRoutePreviewStatus {
    Current,
    Disabled,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessPayloadSourceStatus {
    Current,
    Missing,
    Unavailable,
}

pub fn read_portless_status_payload_for_paths(paths: &GxserverPaths) -> PortlessStatusPayload {
    match open_gxserver_database(paths) {
        Ok(db) => read_portless_status_payload(&db),
        Err(_) => unavailable_portless_status_payload(),
    }
}

pub fn read_portless_status_payload(db: &Connection) -> PortlessStatusPayload {
    match PortlessRepository::new(db).read_state() {
        Ok(record) => {
            let source_status = if record.is_some() {
                PortlessPayloadSourceStatus::Current
            } else {
                PortlessPayloadSourceStatus::Missing
            };
            portless_status_payload_from_record(record, source_status)
        }
        Err(_) => unavailable_portless_status_payload(),
    }
}

pub fn unavailable_portless_status_payload() -> PortlessStatusPayload {
    portless_status_payload_from_record(None, PortlessPayloadSourceStatus::Unavailable)
}

pub fn apply_portless_state_update(
    paths: &GxserverPaths,
    db: &Connection,
    update: PortlessStateUpdate,
) -> Result<PortlessStateRecord> {
    let repository = PortlessRepository::new(db);
    match update {
        PortlessStateUpdate::SetEnabled { enabled } => {
            apply_portless_enabled_update(paths, &repository, enabled)
        }
        PortlessStateUpdate::SetProtocol { protocol } => {
            apply_portless_protocol_update(&repository, protocol)
        }
        PortlessStateUpdate::RecordAdminResult {
            action,
            ok,
            protocol,
        } => apply_portless_admin_result_update(paths, &repository, action, ok, protocol),
    }
}

fn apply_portless_enabled_update(
    paths: &GxserverPaths,
    repository: &PortlessRepository<'_>,
    enabled: bool,
) -> Result<PortlessStateRecord> {
    let mut state = read_portless_state_or_default(repository)?;
    state.enabled = enabled;
    if enabled {
        if state.setup_status == PortlessSetupStatus::Disabled {
            state.setup_status = match state.setup_ownership {
                PortlessSetupOwnership::Unknown => PortlessSetupStatus::Unknown,
                PortlessSetupOwnership::Missing
                | PortlessSetupOwnership::Ghostex
                | PortlessSetupOwnership::Standalone => PortlessSetupStatus::Needed,
            };
            state.runtime_status = PortlessRuntimeStatus::Unknown;
        }
        return repository.upsert_state(state);
    }

    state.setup_status = PortlessSetupStatus::Disabled;
    state.runtime_status = PortlessRuntimeStatus::Inactive;
    let record = repository.upsert_state(state)?;
    sync_portless_routes(paths, &[])?;
    Ok(record)
}

fn apply_portless_protocol_update(
    repository: &PortlessRepository<'_>,
    protocol: PortlessProtocol,
) -> Result<PortlessStateRecord> {
    let mut state = read_portless_state_or_default(repository)?;
    state.protocol = protocol;
    if state.enabled
        && state.setup_status != PortlessSetupStatus::Disabled
        && is_portless_installed_setup_ownership(state.setup_ownership)
    {
        state.setup_status = PortlessSetupStatus::Needed;
        state.runtime_status = PortlessRuntimeStatus::Inactive;
    }
    repository.upsert_state(state)
}

fn apply_portless_admin_result_update(
    paths: &GxserverPaths,
    repository: &PortlessRepository<'_>,
    action: PortlessAdminResultAction,
    ok: bool,
    protocol: Option<PortlessProtocol>,
) -> Result<PortlessStateRecord> {
    let mut state = read_portless_state_or_default(repository)?;
    if let Some(protocol) = protocol {
        state.protocol = protocol;
    }

    match (action, ok) {
        (PortlessAdminResultAction::Remove, true) => {
            state.setup_ownership = PortlessSetupOwnership::Missing;
            state.setup_status = if state.enabled {
                PortlessSetupStatus::Needed
            } else {
                PortlessSetupStatus::Disabled
            };
            state.runtime_status = PortlessRuntimeStatus::Inactive;
            let record = repository.upsert_state(state)?;
            sync_portless_routes(paths, &[])?;
            Ok(record)
        }
        (PortlessAdminResultAction::Remove, false) => repository.upsert_state(state),
        (_, true) => {
            state.enabled = true;
            state.setup_ownership = PortlessSetupOwnership::Ghostex;
            state.setup_status = PortlessSetupStatus::Active;
            state.runtime_status = PortlessRuntimeStatus::Active;
            repository.upsert_state(state)
        }
        (_, false) => {
            state.enabled = true;
            state.setup_ownership = PortlessSetupOwnership::Ghostex;
            state.setup_status = PortlessSetupStatus::Failed;
            state.runtime_status = PortlessRuntimeStatus::Failed;
            repository.upsert_state(state)
        }
    }
}

pub fn log_portless_background_sync_outcome(
    logger: &crate::logging::GxserverLogger,
    outcome: &PortlessBackgroundSyncOutcome,
    duration_ms: u128,
) {
    let _ = logger.log_routine(
        crate::logging::DiagnosticLogScenario::Portless,
        crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Info,
            event: "portless.backgroundSync".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: Some(duration_ms),
            error: None,
            details: Some(json!({
                "action": outcome.action.as_str(),
                "desiredRouteCount": outcome.desired_route_count,
                "liveListenerCount": outcome.live_listener_count,
                "routeCount": outcome.desired_route_count,
                "status": outcome.status.as_str(),
            })),
        },
    );
}

pub fn log_portless_background_sync_failure(
    logger: &crate::logging::GxserverLogger,
    error_code: PortlessLogErrorCode,
    duration_ms: u128,
) {
    log_portless_failure(
        logger,
        "portless.backgroundSyncFailed",
        error_code,
        duration_ms,
        None,
    );
}

pub fn log_portless_state_update_success(
    logger: &crate::logging::GxserverLogger,
    update: &PortlessStateUpdate,
    record: &PortlessStateRecord,
    duration_ms: u128,
) {
    let mut details = portless_state_update_log_details(update);
    if let Some(object) = details.as_object_mut() {
        object.insert("enabled".to_string(), json!(record.state.enabled));
        object.insert(
            "protocol".to_string(),
            json!(record.state.protocol.as_str()),
        );
        object.insert(
            "runtimeStatus".to_string(),
            json!(record.state.runtime_status.as_str()),
        );
        object.insert(
            "setupOwnership".to_string(),
            json!(record.state.setup_ownership.as_str()),
        );
        object.insert(
            "setupStatus".to_string(),
            json!(record.state.setup_status.as_str()),
        );
    }
    let _ = logger.log_routine(
        crate::logging::DiagnosticLogScenario::Portless,
        crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Info,
            event: "portless.stateUpdate".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: Some(duration_ms),
            error: None,
            details: Some(details),
        },
    );
}

pub fn log_portless_state_update_failure(
    logger: &crate::logging::GxserverLogger,
    update: &PortlessStateUpdate,
    error_code: PortlessLogErrorCode,
    duration_ms: u128,
) {
    log_portless_failure(
        logger,
        "portless.stateUpdateFailed",
        error_code,
        duration_ms,
        Some(portless_state_update_log_details(update)),
    );
}

fn log_portless_failure(
    logger: &crate::logging::GxserverLogger,
    event: &str,
    error_code: PortlessLogErrorCode,
    duration_ms: u128,
    details: Option<Value>,
) {
    let mut details = details.unwrap_or_else(|| json!({}));
    if let Some(object) = details.as_object_mut() {
        object.insert("errorCode".to_string(), json!(error_code.as_str()));
    }
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: Some(duration_ms),
        error: Some(error_code.as_str().to_string()),
        details: Some(details),
    });
}

fn portless_state_update_log_details(update: &PortlessStateUpdate) -> Value {
    match update {
        PortlessStateUpdate::SetEnabled { enabled } => {
            json!({
                "enabled": *enabled,
                "updateKind": "setEnabled",
            })
        }
        PortlessStateUpdate::SetProtocol { protocol } => {
            json!({
                "protocol": protocol.as_str(),
                "updateKind": "setProtocol",
            })
        }
        PortlessStateUpdate::RecordAdminResult {
            action,
            ok,
            protocol,
        } => {
            let mut details = json!({
                "adminAction": action.as_str(),
                "ok": *ok,
                "protocolPresent": protocol.is_some(),
                "updateKind": "recordAdminResult",
            });
            if let (Some(protocol), Some(object)) = (protocol, details.as_object_mut()) {
                object.insert("protocol".to_string(), json!(protocol.as_str()));
            }
            details
        }
    }
}

pub fn read_portless_presentation_payload(db: &Connection) -> PortlessPresentationPayload {
    let status = read_portless_status_payload(db);
    let assigned_domains = read_portless_assigned_domains(db).unwrap_or_default();
    if !status.enabled || status.setup_status == PortlessSetupStatus::Disabled {
        return PortlessPresentationPayload {
            assigned_domains,
            live_listener_count: 0,
            route_preview_status: PortlessRoutePreviewStatus::Disabled,
            route_previews: Vec::new(),
            status,
        };
    }

    match compute_live_portless_owned_listeners(db).and_then(|listeners| {
        let routes = compute_desired_portless_routes(db, &listeners)?;
        Ok((listeners, routes))
    }) {
        Ok((listeners, routes)) => {
            let route_previews =
                portless_route_previews_for_desired_routes(status.protocol, &listeners, &routes);
            PortlessPresentationPayload {
                assigned_domains,
                live_listener_count: listeners.len(),
                route_preview_status: PortlessRoutePreviewStatus::Current,
                route_previews,
                status,
            }
        }
        Err(_) => PortlessPresentationPayload {
            assigned_domains,
            live_listener_count: 0,
            route_preview_status: PortlessRoutePreviewStatus::Unavailable,
            route_previews: Vec::new(),
            status,
        },
    }
}

fn read_portless_assigned_domains(db: &Connection) -> Result<Vec<PortlessAssignedDomain>> {
    /*
    CDXC:PortlessSettings 2026-06-23-04:02:
    Settings -> Projects must show assigned project/worktree domains even when
    no dev server is currently listening. Derive hostnames from persisted
    Portless slugs and expose only stable ids plus hostnames, never paths,
    names, full URLs, command text, process output, or environment values.
    */
    let identities = PortlessRepository::new(db).backfill_domain_identities()?;
    let mut domains = Vec::new();
    for project in identities.projects {
        domains.push(PortlessAssignedDomain {
            hostname: format!("{}.localhost", project.slug),
            kind: PortlessAssignedDomainKind::Project,
            parent_project_id: None,
            project_id: project.project_id,
        });
    }
    for worktree in identities.worktrees {
        domains.push(PortlessAssignedDomain {
            hostname: format!(
                "{}.{}.localhost",
                worktree.project_slug, worktree.worktree_slug
            ),
            kind: PortlessAssignedDomainKind::Worktree,
            parent_project_id: Some(worktree.parent_project_id),
            project_id: worktree.worktree_project_id,
        });
    }
    domains.sort_by(|a, b| {
        a.project_id
            .cmp(&b.project_id)
            .then_with(|| a.hostname.cmp(&b.hostname))
    });
    Ok(domains)
}

fn portless_status_payload_from_record(
    record: Option<PortlessStateRecord>,
    source_status: PortlessPayloadSourceStatus,
) -> PortlessStatusPayload {
    let (state, updated_at) = match record {
        Some(record) => (record.state, Some(record.updated_at)),
        None => (default_portless_state(), None),
    };
    PortlessStatusPayload {
        actions: portless_admin_action_set(&state),
        enabled: state.enabled,
        protocol: state.protocol,
        runtime_status: state.runtime_status,
        setup_ownership: state.setup_ownership,
        setup_status: state.setup_status,
        source_status,
        updated_at,
    }
}

fn read_portless_state_or_default(repository: &PortlessRepository<'_>) -> Result<PortlessState> {
    Ok(repository
        .read_state()?
        .map(|record| record.state)
        .unwrap_or_else(default_portless_state))
}

fn default_portless_state() -> PortlessState {
    PortlessState {
        // CDXC:PortlessSettingsDisabled 2026-07-25: Portless remains
        // implemented for later use, but gxserver must not create routes before
        // an app explicitly re-enables the currently hidden integration.
        enabled: false,
        protocol: PortlessProtocol::Https,
        setup_ownership: PortlessSetupOwnership::Unknown,
        setup_status: PortlessSetupStatus::Disabled,
        runtime_status: PortlessRuntimeStatus::Inactive,
    }
}

fn is_portless_installed_setup_ownership(ownership: PortlessSetupOwnership) -> bool {
    matches!(
        ownership,
        PortlessSetupOwnership::Ghostex | PortlessSetupOwnership::Standalone
    )
}

fn portless_admin_action_set(state: &PortlessState) -> PortlessAdminActionSet {
    let recommended = recommended_portless_admin_action(state);
    PortlessAdminActionSet {
        install: portless_admin_action_availability(
            recommended == Some(PortlessAdminActionKind::Install),
        ),
        reconfigure: portless_admin_action_availability(
            recommended == Some(PortlessAdminActionKind::Reconfigure),
        ),
        remove: portless_admin_action_availability(false),
        retry: portless_admin_action_availability(
            recommended == Some(PortlessAdminActionKind::Retry),
        ),
    }
}

fn portless_admin_action_availability(recommended: bool) -> PortlessAdminActionAvailability {
    PortlessAdminActionAvailability {
        available: false,
        local_mac_only: true,
        recommended,
        unavailable_reason: Some(if recommended {
            PortlessAdminActionUnavailableReason::NativeAdminBridgeRequired
        } else {
            PortlessAdminActionUnavailableReason::NotRecommended
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortlessAdminActionKind {
    Install,
    Reconfigure,
    Retry,
}

fn recommended_portless_admin_action(state: &PortlessState) -> Option<PortlessAdminActionKind> {
    if !state.enabled || state.setup_status == PortlessSetupStatus::Disabled {
        return None;
    }
    match (state.setup_ownership, state.setup_status) {
        (PortlessSetupOwnership::Missing, PortlessSetupStatus::Needed) => {
            Some(PortlessAdminActionKind::Install)
        }
        (PortlessSetupOwnership::Ghostex, PortlessSetupStatus::Needed) => {
            Some(PortlessAdminActionKind::Reconfigure)
        }
        (PortlessSetupOwnership::Ghostex, PortlessSetupStatus::Failed) => {
            Some(PortlessAdminActionKind::Retry)
        }
        _ => None,
    }
}

fn portless_route_previews_for_desired_routes(
    protocol: PortlessProtocol,
    listeners: &[PortlessOwnedListener],
    routes: &[PortlessRoute],
) -> Vec<PortlessRoutePreview> {
    let mut listeners_by_target = HashMap::<(u16, u32), Vec<&PortlessOwnedListener>>::new();
    for listener in listeners {
        listeners_by_target
            .entry((listener.port, listener.pid))
            .or_default()
            .push(listener);
    }

    let mut route_previews = Vec::new();
    for route in routes {
        let Some(listener) = listeners_by_target
            .get_mut(&(route.port, route.pid))
            .and_then(|candidates| candidates.pop())
        else {
            continue;
        };
        route_previews.push(PortlessRoutePreview {
            hostname: route.hostname.clone(),
            kind: portless_route_preview_kind(route),
            port: route.port,
            project_id: listener.project_id.clone(),
            protocol,
            session_id: listener.session_id.clone(),
        });
    }
    route_previews
}

fn portless_route_preview_kind(route: &PortlessRoute) -> PortlessRoutePreviewKind {
    if route.hostname.starts_with(&format!("p{}.", route.port)) {
        PortlessRoutePreviewKind::Additional
    } else {
        PortlessRoutePreviewKind::Primary
    }
}

pub fn compute_live_portless_owned_listeners(
    db: &Connection,
) -> Result<Vec<PortlessOwnedListener>> {
    let sessions = list_portless_listener_candidate_sessions(db)?;
    if sessions.is_empty() {
        return Ok(Vec::new());
    }

    let zmx = require_bundled_zmx().map_err(|_| {
        anyhow!("Ghostex bundled zmx is unavailable for Portless listener detection.")
    })?;
    let output = run_portless_listener_snapshot_command(
        &build_portless_listener_snapshot_command(&zmx.executable_path),
    )?;
    if output.stdout_truncated {
        bail!("Portless listener snapshot output exceeded the safety limit.");
    }
    if output.exit_code != 0 {
        return Ok(Vec::new());
    }
    let snapshot = parse_portless_listener_snapshot_sections(&output.stdout);
    Ok(compute_portless_owned_listeners_for_sessions(
        &sessions,
        &snapshot.zmx_list_output,
        &snapshot.ps_output,
        &snapshot.listener_output,
    ))
}

pub fn compute_portless_owned_listeners_from_snapshot(
    db: &Connection,
    zmx_list_output: &str,
    ps_output: &str,
    listener_output: &str,
) -> Result<Vec<PortlessOwnedListener>> {
    let sessions = list_portless_listener_candidate_sessions(db)?;
    Ok(compute_portless_owned_listeners_for_sessions(
        &sessions,
        zmx_list_output,
        ps_output,
        listener_output,
    ))
}

pub fn compute_desired_portless_routes(
    db: &Connection,
    listeners: &[PortlessOwnedListener],
) -> Result<Vec<PortlessRoute>> {
    let repository = PortlessRepository::new(db);
    let mut groups = BTreeMap::<String, Vec<PortlessRouteTarget>>::new();

    for listener in listeners {
        ensure!(
            listener.pid > 0,
            "Portless desired routes must preserve a nonzero live listener pid."
        );
        let base_domain = portless_base_domain_for_listener(&repository, listener)?;
        groups
            .entry(base_domain)
            .or_default()
            .push(PortlessRouteTarget {
                port: listener.port,
                pid: listener.pid,
            });
    }

    let mut routes = Vec::new();
    for (base_domain, mut targets) in groups {
        targets.sort_by(|left, right| {
            left.port
                .cmp(&right.port)
                .then_with(|| left.pid.cmp(&right.pid))
        });
        let primary_index = primary_portless_route_target_index(&targets)
            .with_context(|| "Portless route group must contain at least one listener")?;
        let primary = targets.remove(primary_index);
        routes.push(PortlessRoute {
            hostname: base_domain.clone(),
            port: primary.port,
            pid: primary.pid,
        });
        for target in targets {
            routes.push(PortlessRoute {
                hostname: format!("p{}.{}", target.port, base_domain),
                port: target.port,
                pid: target.pid,
            });
        }
    }

    validate_portless_routes(&routes)?;
    Ok(routes)
}

pub fn ensure_portless_state_dir(paths: &GxserverPaths) -> Result<()> {
    ensure!(
        paths.portless_state_dir.starts_with(&paths.root_dir),
        "Portless state directory must stay under the gxserver root."
    );
    ensure_portless_state_dir_path(&paths.portless_state_dir)
}

pub fn sync_portless_routes(paths: &GxserverPaths, desired_routes: &[PortlessRoute]) -> Result<()> {
    sync_portless_routes_with_options(paths, desired_routes, PortlessRouteSyncOptions::default())
}

pub fn run_portless_background_sync_once(
    paths: &GxserverPaths,
) -> Result<PortlessBackgroundSyncOutcome> {
    let db = open_gxserver_database(paths)?;
    let repository = PortlessRepository::new(&db);
    let state = Some(refresh_portless_service_state_for_repository(paths, &repository)?.state);

    let (live_listener_count, desired_routes) =
        if should_compute_desired_portless_routes(state.as_ref()) {
            let listeners = compute_live_portless_owned_listeners(&db)?;
            let routes = compute_desired_portless_routes(&db, &listeners)?;
            (listeners.len(), routes)
        } else {
            (0, Vec::new())
        };

    apply_portless_background_sync_policy(
        paths,
        state.as_ref(),
        &desired_routes,
        live_listener_count,
    )
}

pub fn refresh_portless_service_state(paths: &GxserverPaths) -> Result<PortlessStateRecord> {
    let db = open_gxserver_database(paths)?;
    let repository = PortlessRepository::new(&db);
    refresh_portless_service_state_for_repository(paths, &repository)
}

fn refresh_portless_service_state_for_repository(
    paths: &GxserverPaths,
    repository: &PortlessRepository<'_>,
) -> Result<PortlessStateRecord> {
    let existing = repository.read_state()?.map(|record| record.state);
    let protocol = existing
        .as_ref()
        .map(|state| state.protocol)
        .unwrap_or(PortlessProtocol::Https);
    let expectation = expected_portless_service_config(paths, protocol);
    let inspection = inspect_installed_portless_service(&expectation)?;
    let state = portless_state_for_service_inspection(existing.as_ref(), protocol, &inspection);
    repository.upsert_state(state)
}

fn apply_portless_background_sync_policy(
    paths: &GxserverPaths,
    state: Option<&PortlessState>,
    desired_routes: &[PortlessRoute],
    live_listener_count: usize,
) -> Result<PortlessBackgroundSyncOutcome> {
    let action = portless_background_route_action(state);
    match action {
        PortlessBackgroundRouteAction::MirrorDesiredRoutes => {
            sync_portless_routes(paths, desired_routes)?;
        }
        PortlessBackgroundRouteAction::ClearMirroredRoutes => {
            sync_portless_routes(paths, &[])?;
        }
        PortlessBackgroundRouteAction::SkipRouteFileWrite => {}
    }

    Ok(PortlessBackgroundSyncOutcome {
        action,
        desired_route_count: desired_routes.len(),
        live_listener_count,
        status: portless_background_status(state),
    })
}

fn should_compute_desired_portless_routes(state: Option<&PortlessState>) -> bool {
    !is_portless_disabled_state(state)
}

fn portless_background_route_action(
    state: Option<&PortlessState>,
) -> PortlessBackgroundRouteAction {
    if is_portless_disabled_state(state) {
        return PortlessBackgroundRouteAction::ClearMirroredRoutes;
    }

    let Some(state) = state else {
        return PortlessBackgroundRouteAction::SkipRouteFileWrite;
    };
    if state.setup_ownership == PortlessSetupOwnership::Ghostex
        && state.setup_status == PortlessSetupStatus::Active
    {
        PortlessBackgroundRouteAction::MirrorDesiredRoutes
    } else {
        PortlessBackgroundRouteAction::SkipRouteFileWrite
    }
}

fn portless_background_status(state: Option<&PortlessState>) -> PortlessBackgroundStatus {
    if is_portless_disabled_state(state) {
        return PortlessBackgroundStatus::Disabled;
    }

    let Some(state) = state else {
        return PortlessBackgroundStatus::SetupUnknown;
    };
    match (state.setup_ownership, state.setup_status) {
        (PortlessSetupOwnership::Ghostex, PortlessSetupStatus::Active) => {
            PortlessBackgroundStatus::SetupActive
        }
        (_, PortlessSetupStatus::Failed) => PortlessBackgroundStatus::SetupFailed,
        (
            PortlessSetupOwnership::Missing | PortlessSetupOwnership::Standalone,
            PortlessSetupStatus::Needed,
        ) => PortlessBackgroundStatus::SetupNeeded,
        (_, PortlessSetupStatus::Needed) => PortlessBackgroundStatus::SetupNeeded,
        _ => PortlessBackgroundStatus::SetupUnknown,
    }
}

fn is_portless_disabled_state(state: Option<&PortlessState>) -> bool {
    state
        .map(|state| !state.enabled || state.setup_status == PortlessSetupStatus::Disabled)
        .unwrap_or(false)
}

fn inspect_installed_portless_service(
    expectation: &PortlessServiceExpectation,
) -> Result<PortlessServiceInspection> {
    let plist = read_installed_portless_service_plist()?;
    let reachability = plist.as_ref().map(|_| PortlessServiceReachability {
        manager_running: None,
        proxy_reachable: Some(probe_portless_proxy_reachable(expectation.proxy_port)),
    });
    inspect_portless_service_from_plist_text(
        plist.as_deref(),
        expectation,
        reachability.unwrap_or_default(),
    )
}

fn inspect_portless_service_from_plist_text(
    plist_text: Option<&str>,
    expectation: &PortlessServiceExpectation,
    reachability: PortlessServiceReachability,
) -> Result<PortlessServiceInspection> {
    let Some(plist_text) = plist_text else {
        return Ok(PortlessServiceInspection {
            classification: PortlessServiceClassification::Missing,
            mismatch_count: 0,
        });
    };
    let plist = parse_portless_launchd_plist(plist_text)?;
    Ok(classify_portless_launchd_service(
        &plist,
        expectation,
        reachability,
    ))
}

#[cfg(target_os = "macos")]
fn read_installed_portless_service_plist() -> Result<Option<String>> {
    match fs::read_to_string(PORTLESS_SERVICE_PLIST_PATH) {
        Ok(plist) => Ok(Some(plist)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| "read installed Portless launchd plist"),
    }
}

#[cfg(not(target_os = "macos"))]
fn read_installed_portless_service_plist() -> Result<Option<String>> {
    Ok(None)
}

fn classify_portless_launchd_service(
    plist: &PortlessLaunchdPlist,
    expectation: &PortlessServiceExpectation,
    reachability: PortlessServiceReachability,
) -> PortlessServiceInspection {
    let node_matches = plist
        .program_arguments
        .first()
        .map(|arg| {
            path_value_matches_any(arg, &expectation.expected_node_paths, &expectation.home_dir)
        })
        .unwrap_or(false);
    let cli_matches = plist
        .program_arguments
        .get(1)
        .map(|arg| {
            path_value_matches_any(arg, &expectation.expected_cli_paths, &expectation.home_dir)
        })
        .unwrap_or(false);
    let state_dir_matches = plist
        .environment
        .get("PORTLESS_STATE_DIR")
        .map(|value| {
            normalize_path_value_for_comparison(value, &expectation.home_dir)
                == expectation.expected_state_dir
        })
        .unwrap_or(false);
    let ghostex_marked = node_matches || cli_matches || state_dir_matches;
    if !ghostex_marked {
        return PortlessServiceInspection {
            classification: PortlessServiceClassification::Standalone,
            mismatch_count: 0,
        };
    }

    let mut mismatch_count = 0_usize;
    mismatch_count += (plist.label.as_deref() != Some(PORTLESS_SERVICE_LABEL)) as usize;
    mismatch_count += (!portless_program_has_proxy_start(&plist.program_arguments)) as usize;
    mismatch_count += (!node_matches) as usize;
    mismatch_count += (!cli_matches) as usize;
    mismatch_count += (!state_dir_matches) as usize;
    mismatch_count +=
        (!portless_env_port_matches(&plist.environment, expectation.proxy_port)) as usize;
    mismatch_count +=
        (!portless_env_protocol_matches(&plist.environment, expectation.protocol)) as usize;
    mismatch_count +=
        (!portless_env_tld_matches(&plist.environment, PORTLESS_SERVICE_TLD)) as usize;
    mismatch_count += (!portless_env_lan_matches(&plist.environment, false)) as usize;
    mismatch_count += (!portless_env_wildcard_matches(&plist.environment, false)) as usize;
    mismatch_count += (!portless_env_sync_hosts_matches(&plist.environment, false)) as usize;
    mismatch_count +=
        (!portless_launchd_output_path_matches(plist.standard_out_path.as_deref())) as usize;
    mismatch_count +=
        (!portless_launchd_output_path_matches(plist.standard_error_path.as_deref())) as usize;
    mismatch_count +=
        (!portless_args_port_matches(&plist.program_arguments, expectation.proxy_port)) as usize;
    mismatch_count +=
        (!portless_args_protocol_matches(&plist.program_arguments, expectation.protocol)) as usize;
    mismatch_count +=
        (!portless_args_tld_matches(&plist.program_arguments, PORTLESS_SERVICE_TLD)) as usize;
    mismatch_count += (!portless_args_lan_matches(&plist.program_arguments, false)) as usize;
    mismatch_count += (!portless_args_wildcard_matches(&plist.program_arguments, false)) as usize;

    if mismatch_count > 0 {
        return PortlessServiceInspection {
            classification: PortlessServiceClassification::GhostexConfigMismatch,
            mismatch_count,
        };
    }

    if reachability.manager_running == Some(false) || reachability.proxy_reachable == Some(false) {
        return PortlessServiceInspection {
            classification: PortlessServiceClassification::GhostexFailed,
            mismatch_count,
        };
    }

    PortlessServiceInspection {
        classification: PortlessServiceClassification::GhostexActive,
        mismatch_count,
    }
}

fn portless_state_for_service_inspection(
    existing: Option<&PortlessState>,
    protocol: PortlessProtocol,
    inspection: &PortlessServiceInspection,
) -> PortlessState {
    let enabled = existing.map(|state| state.enabled).unwrap_or(true);
    let disabled = is_portless_disabled_state(existing);
    let (setup_ownership, mut setup_status, runtime_status) = match inspection.classification {
        PortlessServiceClassification::Missing => (
            PortlessSetupOwnership::Missing,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        ),
        PortlessServiceClassification::Standalone => (
            PortlessSetupOwnership::Standalone,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        ),
        PortlessServiceClassification::GhostexConfigMismatch => (
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        ),
        PortlessServiceClassification::GhostexFailed => (
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Failed,
            PortlessRuntimeStatus::Failed,
        ),
        PortlessServiceClassification::GhostexActive => (
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Active,
            PortlessRuntimeStatus::Active,
        ),
    };
    if disabled {
        setup_status = PortlessSetupStatus::Disabled;
    }
    PortlessState {
        enabled,
        protocol,
        setup_ownership,
        setup_status,
        runtime_status,
    }
}

fn expected_portless_service_config(
    paths: &GxserverPaths,
    protocol: PortlessProtocol,
) -> PortlessServiceExpectation {
    let home_dir = paths.home_dir.clone();
    let expected_node_paths =
        normalize_and_dedupe_paths(expected_portless_node_candidates(), &home_dir);
    let expected_cli_paths =
        normalize_and_dedupe_paths(expected_portless_cli_candidates(), &home_dir);
    PortlessServiceExpectation {
        home_dir,
        expected_node_paths,
        expected_cli_paths,
        expected_state_dir: normalize_path_for_comparison(&paths.portless_state_dir),
        protocol,
        proxy_port: portless_service_port_for_protocol(protocol),
    }
}

fn expected_portless_node_candidates() -> Vec<PathBuf> {
    resources::code_server_node_candidates()
}

fn expected_portless_cli_candidates() -> Vec<PathBuf> {
    resources::portless_cli_candidates()
}

fn normalize_and_dedupe_paths(paths: Vec<PathBuf>, home_dir: &Path) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for path in paths {
        let normalized = normalize_path_value_for_comparison(&path.to_string_lossy(), home_dir);
        if seen.insert(normalized.clone()) {
            output.push(normalized);
        }
    }
    output
}

fn portless_service_port_for_protocol(protocol: PortlessProtocol) -> u16 {
    match protocol {
        PortlessProtocol::Https => 443,
        PortlessProtocol::Http => 80,
    }
}

fn parse_portless_launchd_plist(plist_text: &str) -> Result<PortlessLaunchdPlist> {
    let label = parse_plist_string_for_key(plist_text, "Label")?;
    let program_arguments =
        parse_plist_string_array_for_key(plist_text, "ProgramArguments")?.unwrap_or_default();
    let environment =
        parse_plist_string_dict_for_key(plist_text, "EnvironmentVariables")?.unwrap_or_default();
    let standard_out_path = parse_plist_string_for_key(plist_text, "StandardOutPath")?;
    let standard_error_path = parse_plist_string_for_key(plist_text, "StandardErrorPath")?;
    Ok(PortlessLaunchdPlist {
        label,
        program_arguments,
        environment,
        standard_out_path,
        standard_error_path,
    })
}

fn parse_plist_string_for_key(plist_text: &str, key: &str) -> Result<Option<String>> {
    let Some(after_key) = find_plist_key_end(plist_text, key)? else {
        return Ok(None);
    };
    let Some(block) = xml_element_block(&plist_text[after_key..], "string") else {
        return Ok(None);
    };
    xml_unescape(block).map(Some)
}

fn parse_plist_string_array_for_key(plist_text: &str, key: &str) -> Result<Option<Vec<String>>> {
    let Some(after_key) = find_plist_key_end(plist_text, key)? else {
        return Ok(None);
    };
    let Some(block) = xml_element_block(&plist_text[after_key..], "array") else {
        return Ok(None);
    };
    parse_xml_string_elements(block).map(Some)
}

fn parse_plist_string_dict_for_key(
    plist_text: &str,
    key: &str,
) -> Result<Option<BTreeMap<String, String>>> {
    let Some(after_key) = find_plist_key_end(plist_text, key)? else {
        return Ok(None);
    };
    let Some(block) = xml_element_block(&plist_text[after_key..], "dict") else {
        return Ok(None);
    };
    parse_xml_key_string_dict(block).map(Some)
}

fn find_plist_key_end(plist_text: &str, wanted_key: &str) -> Result<Option<usize>> {
    let mut offset = 0_usize;
    while let Some(start) = plist_text[offset..].find("<key>") {
        let key_start = offset + start + "<key>".len();
        let Some(end) = plist_text[key_start..].find("</key>") else {
            return Ok(None);
        };
        let key_end = key_start + end;
        let key = xml_unescape(&plist_text[key_start..key_end])?;
        let after_key = key_end + "</key>".len();
        if key == wanted_key {
            return Ok(Some(after_key));
        }
        offset = after_key;
    }
    Ok(None)
}

fn xml_element_block<'a>(input: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = input.find(&open)? + open.len();
    let end = input[start..].find(&close)?;
    Some(&input[start..start + end])
}

fn parse_xml_string_elements(block: &str) -> Result<Vec<String>> {
    let mut values = Vec::new();
    let mut offset = 0_usize;
    while let Some(start) = block[offset..].find("<string>") {
        let value_start = offset + start + "<string>".len();
        let Some(end) = block[value_start..].find("</string>") else {
            break;
        };
        let value_end = value_start + end;
        values.push(xml_unescape(&block[value_start..value_end])?);
        offset = value_end + "</string>".len();
    }
    Ok(values)
}

fn parse_xml_key_string_dict(block: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    let mut offset = 0_usize;
    while let Some(start) = block[offset..].find("<key>") {
        let key_start = offset + start + "<key>".len();
        let Some(key_end_offset) = block[key_start..].find("</key>") else {
            break;
        };
        let key_end = key_start + key_end_offset;
        let key = xml_unescape(&block[key_start..key_end])?;
        let after_key = key_end + "</key>".len();
        let Some(value_block) = xml_element_block(&block[after_key..], "string") else {
            offset = after_key;
            continue;
        };
        values.insert(key, xml_unescape(value_block)?);
        offset = after_key
            + block[after_key..]
                .find("</string>")
                .map(|end| end + "</string>".len())
                .unwrap_or(0);
    }
    Ok(values)
}

fn xml_unescape(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut remaining = value;
    while let Some(entity_start) = remaining.find('&') {
        output.push_str(&remaining[..entity_start]);
        let entity_tail = &remaining[entity_start + 1..];
        let Some(entity_end) = entity_tail.find(';') else {
            bail!("Invalid XML entity in Portless launchd plist.");
        };
        let entity = &entity_tail[..entity_end];
        let replacement = match entity {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            _ => bail!("Unsupported XML entity in Portless launchd plist."),
        };
        output.push_str(replacement);
        remaining = &entity_tail[entity_end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn portless_program_has_proxy_start(program_arguments: &[String]) -> bool {
    portless_proxy_start_args(program_arguments).is_some()
}

fn portless_proxy_start_args(program_arguments: &[String]) -> Option<&[String]> {
    program_arguments
        .windows(2)
        .position(|window| window[0] == "proxy" && window[1] == "start")
        .map(|index| &program_arguments[index + 2..])
}

fn portless_env_port_matches(environment: &BTreeMap<String, String>, expected_port: u16) -> bool {
    environment
        .get("PORTLESS_PORT")
        .and_then(|value| parse_portless_port_value(value))
        == Some(expected_port)
}

fn portless_env_protocol_matches(
    environment: &BTreeMap<String, String>,
    expected_protocol: PortlessProtocol,
) -> bool {
    environment
        .get("PORTLESS_HTTPS")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_protocol == PortlessProtocol::Https)
}

fn portless_env_tld_matches(environment: &BTreeMap<String, String>, expected_tld: &str) -> bool {
    environment
        .get("PORTLESS_TLD")
        .map(|value| value.trim().eq_ignore_ascii_case(expected_tld))
        .unwrap_or(expected_tld == PORTLESS_SERVICE_TLD)
}

fn portless_env_lan_matches(environment: &BTreeMap<String, String>, expected_lan: bool) -> bool {
    let lan_matches = environment
        .get("PORTLESS_LAN")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_lan);
    let lan_ip_absent = environment
        .get("PORTLESS_LAN_IP")
        .map(|value| value.trim().is_empty())
        .unwrap_or(true);
    lan_matches && (expected_lan || lan_ip_absent)
}

fn portless_env_wildcard_matches(
    environment: &BTreeMap<String, String>,
    expected_wildcard: bool,
) -> bool {
    environment
        .get("PORTLESS_WILDCARD")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_wildcard)
}

fn portless_env_sync_hosts_matches(
    environment: &BTreeMap<String, String>,
    expected_sync_hosts: bool,
) -> bool {
    environment
        .get("PORTLESS_SYNC_HOSTS")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_sync_hosts)
}

fn portless_launchd_output_path_matches(path: Option<&str>) -> bool {
    path.map(str::trim) == Some("/dev/null")
}

fn portless_args_port_matches(program_arguments: &[String], expected_port: u16) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    portless_arg_value(args, "--port", Some("-p")).and_then(parse_portless_port_value)
        == Some(expected_port)
}

fn portless_args_protocol_matches(
    program_arguments: &[String],
    expected_protocol: PortlessProtocol,
) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    if portless_args_contain(args, "--cert")
        || portless_args_contain(args, "--key")
        || portless_args_contain(args, "--no-tls") && portless_args_contain(args, "--https")
    {
        return false;
    }
    match expected_protocol {
        PortlessProtocol::Https => portless_args_contain(args, "--https"),
        PortlessProtocol::Http => portless_args_contain(args, "--no-tls"),
    }
}

fn portless_args_tld_matches(program_arguments: &[String], expected_tld: &str) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    if portless_args_contain(args, "--lan") || portless_args_contain(args, "--ip") {
        return false;
    }
    portless_arg_value(args, "--tld", None)
        .map(|value| value.trim().eq_ignore_ascii_case(expected_tld))
        .unwrap_or(expected_tld == PORTLESS_SERVICE_TLD)
}

fn portless_args_lan_matches(program_arguments: &[String], expected_lan: bool) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    let lan_enabled = portless_args_contain(args, "--lan") || portless_args_contain(args, "--ip");
    lan_enabled == expected_lan
}

fn portless_args_wildcard_matches(program_arguments: &[String], expected_wildcard: bool) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    portless_args_contain(args, "--wildcard") == expected_wildcard
}

fn portless_args_contain(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| {
        arg == flag
            || arg
                .strip_prefix(flag)
                .is_some_and(|tail| tail.starts_with('='))
    })
}

fn portless_arg_value<'a>(
    args: &'a [String],
    long_flag: &str,
    short_flag: Option<&str>,
) -> Option<&'a str> {
    for (index, arg) in args.iter().enumerate() {
        if arg == long_flag || short_flag.is_some_and(|flag| arg == flag) {
            return args.get(index + 1).map(String::as_str);
        }
        if let Some(value) = arg.strip_prefix(&format!("{long_flag}=")) {
            return Some(value);
        }
    }
    None
}

fn parse_portless_bool_value(value: &str) -> Option<bool> {
    match value.trim() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

fn parse_portless_port_value(value: &str) -> Option<u16> {
    let port = value.trim().parse::<u16>().ok()?;
    (port > 0).then_some(port)
}

fn path_value_matches_any(value: &str, expected_paths: &[String], home_dir: &Path) -> bool {
    let normalized = normalize_path_value_for_comparison(value, home_dir);
    expected_paths
        .iter()
        .any(|expected| expected.as_str() == normalized)
}

fn normalize_path_value_for_comparison(value: &str, home_dir: &Path) -> String {
    let trimmed = value.trim();
    let path = if trimmed == "~" {
        home_dir.to_path_buf()
    } else if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        home_dir.join(rest)
    } else {
        PathBuf::from(trimmed)
    };
    normalize_path_for_comparison(&path)
}

fn normalize_path_for_comparison(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized.to_string_lossy().to_string()
}

fn probe_portless_proxy_reachable(port: u16) -> bool {
    let addr = ([127, 0, 0, 1], port).into();
    TcpStream::connect_timeout(&addr, PORTLESS_SERVICE_REACHABILITY_TIMEOUT).is_ok()
}

#[derive(Clone, Copy)]
struct PortlessRouteSyncOptions {
    lock_timeout: Duration,
    lock_retry_delay: Duration,
    stale_lock_age: Duration,
}

impl Default for PortlessRouteSyncOptions {
    fn default() -> Self {
        Self {
            lock_timeout: PORTLESS_LOCK_TIMEOUT,
            lock_retry_delay: PORTLESS_LOCK_RETRY_DELAY,
            stale_lock_age: PORTLESS_STALE_LOCK_AGE,
        }
    }
}

fn sync_portless_routes_with_options(
    paths: &GxserverPaths,
    desired_routes: &[PortlessRoute],
    options: PortlessRouteSyncOptions,
) -> Result<()> {
    validate_portless_routes(desired_routes)?;
    ensure_portless_state_dir(paths)?;
    let _lock = acquire_portless_routes_lock(&paths.portless_state_dir, options)?;
    write_portless_routes_json(&paths.portless_state_dir, desired_routes)
}

fn ensure_portless_state_dir_path(state_dir: &Path) -> Result<()> {
    fs::create_dir_all(state_dir).with_context(|| "create Portless state directory")?;
    set_portless_dir_mode(state_dir)?;
    ensure_current_user_owns_path(state_dir)?;
    ensure_directory_is_writable(state_dir)?;
    Ok(())
}

fn validate_portless_routes(routes: &[PortlessRoute]) -> Result<()> {
    let mut hostnames = HashSet::new();
    for route in routes {
        validate_portless_hostname(&route.hostname)?;
        ensure!(route.port > 0, "Portless route port must be 1-65535.");
        ensure!(
            route.pid > 0,
            "Portless live routes must use a nonzero pid."
        );
        ensure!(
            hostnames.insert(route.hostname.as_str()),
            "Portless route hostnames must be unique."
        );
    }
    Ok(())
}

fn portless_base_domain_for_listener(
    repository: &PortlessRepository<'_>,
    listener: &PortlessOwnedListener,
) -> Result<String> {
    if let Some(expected_parent_project_id) = listener.worktree_parent_project_id.as_deref() {
        let parts = repository.ensure_worktree_slug(&listener.project_id)?;
        ensure!(
            parts.parent_project_id == expected_parent_project_id,
            "Portless worktree listener parent metadata must match the registered worktree."
        );
        return Ok(format!(
            "{}.{}.localhost",
            parts.project_slug, parts.worktree_slug
        ));
    }

    let project = repository.ensure_project_slug(&listener.project_id)?;
    Ok(format!("{}.localhost", project.slug))
}

fn primary_portless_route_target_index(targets: &[PortlessRouteTarget]) -> Option<usize> {
    for preferred_port in PORTLESS_PRIMARY_ROUTE_PORT_PREFERENCE {
        if let Some(index) = targets
            .iter()
            .position(|target| target.port == *preferred_port)
        {
            return Some(index);
        }
    }
    targets
        .iter()
        .enumerate()
        .min_by_key(|(_, target)| (target.port, target.pid))
        .map(|(index, _)| index)
}

fn validate_portless_hostname(hostname: &str) -> Result<()> {
    ensure!(!hostname.is_empty(), "Portless route hostname is required.");
    ensure!(
        !hostname.contains("://") && !hostname.contains('/') && !hostname.contains(':'),
        "Portless route hostname must not be a URL."
    );
    ensure!(
        hostname.ends_with(".localhost") && hostname != "localhost",
        "Portless route hostname must be a .localhost subdomain."
    );

    let name = hostname
        .strip_suffix(".localhost")
        .with_context(|| "Portless route hostname must use the localhost TLD")?;
    ensure!(
        !name.is_empty() && !name.contains(".."),
        "Portless route hostname labels must be nonempty."
    );
    for label in name.split('.') {
        validate_slug("hostnameLabel", label)?;
    }
    Ok(())
}

struct PortlessRoutesLock {
    lock_path: PathBuf,
}

impl Drop for PortlessRoutesLock {
    fn drop(&mut self) {
        let _ = remove_lock_path(&self.lock_path);
    }
}

fn acquire_portless_routes_lock(
    state_dir: &Path,
    options: PortlessRouteSyncOptions,
) -> Result<PortlessRoutesLock> {
    let lock_path = state_dir.join(PORTLESS_ROUTES_LOCK);
    let deadline = Instant::now() + options.lock_timeout;
    loop {
        match fs::create_dir(&lock_path) {
            Ok(()) => return Ok(PortlessRoutesLock { lock_path }),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if remove_stale_routes_lock(&lock_path, options.stale_lock_age)? {
                    continue;
                }
                let now = Instant::now();
                if now >= deadline {
                    bail!("Timed out acquiring Portless routes lock.");
                }
                let remaining = deadline.saturating_duration_since(now);
                thread::sleep(options.lock_retry_delay.min(remaining));
            }
            Err(error) => return Err(error).with_context(|| "create Portless routes lock"),
        }
    }
}

fn remove_stale_routes_lock(lock_path: &Path, stale_lock_age: Duration) -> Result<bool> {
    let metadata = match fs::metadata(lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error).with_context(|| "read Portless routes lock metadata"),
    };
    let Ok(modified_at) = metadata.modified() else {
        return Ok(false);
    };
    let Ok(age) = modified_at.elapsed() else {
        return Ok(false);
    };
    if age < stale_lock_age {
        return Ok(false);
    }
    remove_lock_path(lock_path).with_context(|| "remove stale Portless routes lock")?;
    Ok(true)
}

fn write_portless_routes_json(state_dir: &Path, routes: &[PortlessRoute]) -> Result<()> {
    let routes_path = state_dir.join(PORTLESS_ROUTES_FILE);
    let (temp_path, mut temp_file) = create_unique_temp_file(state_dir)?;
    let result = (|| -> Result<()> {
        let bytes =
            serde_json::to_vec_pretty(routes).with_context(|| "serialize Portless routes")?;
        temp_file
            .write_all(&bytes)
            .with_context(|| "write temporary Portless routes file")?;
        temp_file
            .sync_all()
            .with_context(|| "flush temporary Portless routes file")?;
        drop(temp_file);
        fs::rename(&temp_path, &routes_path).with_context(|| "replace Portless routes file")?;
        sync_directory_if_supported(state_dir);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn create_unique_temp_file(state_dir: &Path) -> Result<(PathBuf, File)> {
    for _ in 0..32 {
        let counter = PORTLESS_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = state_dir.join(format!(".routes.json.tmp.{}.{}", process::id(), counter));
        match create_new_user_file(&temp_path) {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| "create temporary Portless routes file")
            }
        }
    }
    bail!("Unable to create a unique temporary Portless routes file.")
}

fn create_new_user_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PORTLESS_FILE_MODE);
    }
    options.open(path)
}

fn ensure_directory_is_writable(state_dir: &Path) -> Result<()> {
    let probe_path = state_dir.join(format!(
        ".gxserver-portless-write-check.{}.{}",
        process::id(),
        PORTLESS_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file =
            create_new_user_file(&probe_path).with_context(|| "create Portless write probe")?;
        file.write_all(b"")
            .with_context(|| "write Portless write probe")?;
        Ok(())
    })();
    let _ = fs::remove_file(&probe_path);
    result
}

fn remove_lock_path(lock_path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(lock_path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(lock_path),
        Ok(_) => fs::remove_file(lock_path),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sync_directory_if_supported(state_dir: &Path) {
    let _ = File::open(state_dir).and_then(|directory| directory.sync_all());
}

#[cfg(unix)]
fn set_portless_dir_mode(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(PORTLESS_DIR_MODE))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_portless_dir_mode(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn ensure_current_user_owns_path(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::metadata(path).with_context(|| "read Portless state directory metadata")?;
    let current_uid = unsafe { libc::geteuid() };
    ensure!(
        metadata.uid() == current_uid,
        "Portless state directory must be owned by the current gxserver user."
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_current_user_owns_path(_path: &Path) -> Result<()> {
    Ok(())
}

fn list_portless_listener_candidate_sessions(
    db: &Connection,
) -> Result<Vec<PortlessListenerCandidateSession>> {
    let mut statement = db
        .prepare(
            r#"
            SELECT
              sessions.projectId,
              sessions.sessionId,
              sessions.zmxName,
              sessions.lifecycleState,
              sessions.launchSettingsJson,
              sessions.runtimeSettingsJson,
              projects.worktreeJson
            FROM sessions
            INNER JOIN projects ON projects.projectId = sessions.projectId
            ORDER BY sessions.projectId ASC, sessions.sessionId ASC
            "#,
        )
        .with_context(|| "prepare Portless listener candidate sessions")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawPortlessListenerCandidateSession {
                project_id: row.get("projectId")?,
                session_id: row.get("sessionId")?,
                zmx_name: row.get("zmxName")?,
                lifecycle_state: row.get("lifecycleState")?,
                launch_settings_json: row.get("launchSettingsJson")?,
                runtime_settings_json: row.get("runtimeSettingsJson")?,
                worktree_json: row.get("worktreeJson")?,
            })
        })
        .with_context(|| "query Portless listener candidate sessions")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .with_context(|| "read Portless listener candidate sessions")?;

    rows.into_iter()
        .filter_map(
            |row| match PortlessListenerCandidateSession::from_raw(row) {
                Ok(Some(session)) => Some(Ok(session)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            },
        )
        .collect()
}

fn compute_portless_owned_listeners_for_sessions(
    sessions: &[PortlessListenerCandidateSession],
    zmx_list_output: &str,
    ps_output: &str,
    listener_output: &str,
) -> Vec<PortlessOwnedListener> {
    if sessions.is_empty() {
        return Vec::new();
    }

    let session_names = sessions
        .iter()
        .map(|session| session.zmx_name.clone())
        .collect::<Vec<_>>();
    let root_pids_by_zmx_name = parse_portless_zmx_root_pids(zmx_list_output, &session_names);
    let process_rows = parse_portless_process_rows(ps_output);
    let live_pids = process_rows
        .iter()
        .map(|process| process.pid)
        .collect::<HashSet<_>>();
    let children_by_parent_pid = group_portless_processes_by_parent_pid(&process_rows);
    let mut owner_by_pid = HashMap::<i64, PortlessProcessOwner>::new();

    for (session_index, session) in sessions.iter().enumerate() {
        let Some(root_pid) = root_pids_by_zmx_name.get(&session.zmx_name).copied() else {
            continue;
        };
        if !live_pids.contains(&root_pid) {
            continue;
        }
        for (pid, depth) in collect_portless_process_tree_pids(root_pid, &children_by_parent_pid) {
            owner_by_pid
                .entry(pid)
                .and_modify(|owner| {
                    if depth < owner.depth {
                        owner.session_index = session_index;
                        owner.depth = depth;
                    }
                })
                .or_insert(PortlessProcessOwner {
                    depth,
                    session_index,
                });
        }
    }

    let mut seen = HashSet::<(u32, u16)>::new();
    let mut owned = Vec::new();
    for listener in parse_portless_tcp_listener_rows(listener_output) {
        if !seen.insert((listener.pid, listener.port)) {
            continue;
        }
        let Some(owner) = owner_by_pid.get(&(listener.pid as i64)) else {
            continue;
        };
        let session = &sessions[owner.session_index];
        owned.push(PortlessOwnedListener {
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            zmx_name: session.zmx_name.clone(),
            worktree_parent_project_id: session.worktree_parent_project_id.clone(),
            port: listener.port,
            pid: listener.pid,
        });
    }

    owned.sort_by(|left, right| {
        left.project_id
            .cmp(&right.project_id)
            .then_with(|| left.session_id.cmp(&right.session_id))
            .then_with(|| left.port.cmp(&right.port))
            .then_with(|| left.pid.cmp(&right.pid))
    });
    owned
}

fn build_portless_listener_snapshot_command(zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
printf '%s\n' '__GHOSTEX_ZMX_LIST__'
"$zmx_bin" list
printf '%s\n' '__GHOSTEX_PS__'
ps -axo pid=,ppid=,command=
printf '%s\n' '__GHOSTEX_LSOF_LISTEN__'
if [ -x /usr/sbin/lsof ]; then
  /usr/sbin/lsof -nP -iTCP -sTCP:LISTEN -F pcn 2>/dev/null || true
elif [ -x /usr/bin/lsof ]; then
  /usr/bin/lsof -nP -iTCP -sTCP:LISTEN -F pcn 2>/dev/null || true
elif [ -x /usr/sbin/ss ]; then
  /usr/sbin/ss -H -ltnp 2>/dev/null || true
elif [ -x /usr/bin/ss ]; then
  /usr/bin/ss -H -ltnp 2>/dev/null || true
fi
"#,
        portless_shell_quote(zmx_executable_path)
    )
    .trim()
    .to_string()
}

fn parse_portless_listener_snapshot_sections(stdout: &str) -> PortlessListenerSnapshotSections {
    let zmx_marker = "__GHOSTEX_ZMX_LIST__";
    let ps_marker = "__GHOSTEX_PS__";
    let listener_marker = "__GHOSTEX_LSOF_LISTEN__";
    let Some(zmx_index) = stdout.find(zmx_marker) else {
        return PortlessListenerSnapshotSections::default();
    };
    let Some(ps_index) = stdout.find(ps_marker) else {
        return PortlessListenerSnapshotSections::default();
    };
    let Some(listener_index) = stdout.find(listener_marker) else {
        return PortlessListenerSnapshotSections::default();
    };
    if ps_index <= zmx_index || listener_index <= ps_index {
        return PortlessListenerSnapshotSections::default();
    }
    PortlessListenerSnapshotSections {
        listener_output: stdout[listener_index + listener_marker.len()..]
            .trim()
            .to_string(),
        ps_output: stdout[ps_index + ps_marker.len()..listener_index]
            .trim()
            .to_string(),
        zmx_list_output: stdout[zmx_index + zmx_marker.len()..ps_index]
            .trim()
            .to_string(),
    }
}

fn run_portless_listener_snapshot_command(script: &str) -> Result<PortlessSnapshotCommandOutput> {
    let shell = command_shell();
    let mut child = Command::new(&shell.executable)
        .args(shell.script_args(script))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| "start Portless listener snapshot")?;
    let stdout = child
        .stdout
        .take()
        .with_context(|| "open Portless listener snapshot stdout")?;
    let stderr = child
        .stderr
        .take()
        .with_context(|| "open Portless listener snapshot stderr")?;
    let terminate = Arc::new(AtomicBool::new(false));
    let stdout_terminate = terminate.clone();
    let stderr_terminate = terminate.clone();
    let stdout_thread = thread::spawn(move || {
        read_portless_capped_output(
            stdout,
            PORTLESS_LISTENER_SNAPSHOT_STDOUT_LIMIT_BYTES,
            stdout_terminate,
        )
    });
    let stderr_thread = thread::spawn(move || {
        read_portless_capped_output(
            stderr,
            PORTLESS_LISTENER_SNAPSHOT_STDERR_LIMIT_BYTES,
            stderr_terminate,
        )
    });

    let started = Instant::now();
    let mut timed_out = false;
    let mut terminate_started: Option<Instant> = None;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| "wait for Portless listener snapshot")?
        {
            let (stdout, stdout_truncated) = stdout_thread
                .join()
                .map_err(|_| anyhow!("Portless listener snapshot stdout reader failed."))?;
            let (_, stderr_truncated) = stderr_thread
                .join()
                .map_err(|_| anyhow!("Portless listener snapshot stderr reader failed."))?;
            let mut exit_code = status.code().unwrap_or(1);
            if timed_out {
                exit_code = 124;
            } else if stdout_truncated || stderr_truncated {
                exit_code = 125;
            }
            return Ok(PortlessSnapshotCommandOutput {
                exit_code,
                stdout: String::from_utf8_lossy(&stdout).trim().to_string(),
                stdout_truncated,
            });
        }

        let should_terminate = terminate.load(Ordering::SeqCst)
            || started.elapsed() >= PORTLESS_LISTENER_SNAPSHOT_TIMEOUT;
        if should_terminate {
            timed_out = timed_out || started.elapsed() >= PORTLESS_LISTENER_SNAPSHOT_TIMEOUT;
            if terminate_started.is_none() {
                terminate_started = Some(Instant::now());
                signal_portless_snapshot_child(&mut child, false);
            } else if terminate_started
                .map(|instant| instant.elapsed() >= Duration::from_millis(1_000))
                .unwrap_or(false)
            {
                signal_portless_snapshot_child(&mut child, true);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_portless_capped_output<R: Read>(
    mut reader: R,
    limit: usize,
    terminate: Arc<AtomicBool>,
) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        if read > remaining {
            if remaining > 0 {
                output.extend_from_slice(&buffer[..remaining]);
            }
            truncated = true;
            terminate.store(true, Ordering::SeqCst);
            break;
        }
        output.extend_from_slice(&buffer[..read]);
    }
    (output, truncated)
}

fn signal_portless_snapshot_child(child: &mut std::process::Child, force: bool) {
    #[cfg(unix)]
    unsafe {
        libc::kill(
            child.id() as i32,
            if force { libc::SIGKILL } else { libc::SIGTERM },
        );
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn parse_portless_zmx_root_pids(
    zmx_list_output: &str,
    session_names: &[String],
) -> HashMap<String, i64> {
    let wanted = session_names.iter().cloned().collect::<HashSet<_>>();
    let mut root_pids = HashMap::new();
    for line in zmx_list_output.lines() {
        let Some(name) = parse_portless_zmx_list_name(line) else {
            continue;
        };
        if !wanted.contains(&name) {
            continue;
        }
        if let Some(pid) = parse_portless_zmx_list_pid(line) {
            root_pids.insert(name, pid);
        }
    }
    root_pids
}

fn parse_portless_zmx_list_name(line: &str) -> Option<String> {
    for part in line.split_whitespace() {
        let Some(value) = part
            .strip_prefix("name=")
            .or_else(|| part.strip_prefix("→name="))
        else {
            continue;
        };
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_portless_zmx_list_pid(line: &str) -> Option<i64> {
    for part in line.split_whitespace() {
        let Some(value) = part.strip_prefix("pid=") else {
            continue;
        };
        let pid = value.parse::<i64>().ok()?;
        if pid > 0 {
            return Some(pid);
        }
    }
    None
}

fn parse_portless_process_rows(ps_output: &str) -> Vec<PortlessProcessRow> {
    let mut rows = Vec::new();
    for line in ps_output.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(ppid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        rows.push(PortlessProcessRow { pid, ppid });
    }
    rows
}

fn group_portless_processes_by_parent_pid(
    processes: &[PortlessProcessRow],
) -> HashMap<i64, Vec<i64>> {
    let mut grouped = HashMap::<i64, Vec<i64>>::new();
    for process_row in processes {
        grouped
            .entry(process_row.ppid)
            .or_default()
            .push(process_row.pid);
    }
    grouped
}

fn collect_portless_process_tree_pids(
    root_pid: i64,
    children_by_parent_pid: &HashMap<i64, Vec<i64>>,
) -> Vec<(i64, usize)> {
    let mut collected = Vec::new();
    let mut queue = std::collections::VecDeque::from([(root_pid, 0_usize)]);
    let mut seen = HashSet::new();
    while let Some((pid, depth)) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        collected.push((pid, depth));
        if let Some(children) = children_by_parent_pid.get(&pid) {
            for child in children {
                queue.push_back((*child, depth + 1));
            }
        }
    }
    collected
}

fn parse_portless_tcp_listener_rows(listener_output: &str) -> Vec<PortlessTcpListenerRow> {
    let mut listeners = Vec::new();
    let mut current_pid: Option<u32> = None;
    for raw_line in listener_output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(field) = line.chars().next() else {
            continue;
        };
        if !matches!(field, 'p' | 'n') {
            if let Some(row) = parse_portless_ss_listener_row(line) {
                listeners.push(row);
            }
            continue;
        }
        let value = &line[field.len_utf8()..];
        match field {
            'p' => current_pid = parse_positive_u32(value),
            'n' => {
                let Some(pid) = current_pid else {
                    continue;
                };
                let Some(port) = parse_portless_tcp_listener_port(value) else {
                    continue;
                };
                listeners.push(PortlessTcpListenerRow { pid, port });
            }
            _ => {}
        }
    }
    listeners
}

fn parse_portless_ss_listener_row(line: &str) -> Option<PortlessTcpListenerRow> {
    let pid = parse_portless_ss_listener_pid(line)?;
    let before_process = line.split(" users:").next().unwrap_or(line);
    let port = before_process
        .split_whitespace()
        .filter_map(parse_portless_tcp_listener_port)
        .next()?;
    Some(PortlessTcpListenerRow { pid, port })
}

fn parse_portless_ss_listener_pid(line: &str) -> Option<u32> {
    let value = line.split("pid=").nth(1)?;
    let digits = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    parse_positive_u32(&digits)
}

fn parse_portless_tcp_listener_port(endpoint: &str) -> Option<u16> {
    let endpoint = endpoint
        .trim()
        .strip_prefix("TCP ")
        .unwrap_or_else(|| endpoint.trim())
        .split(" (")
        .next()
        .unwrap_or("")
        .trim();
    if endpoint.is_empty() {
        return None;
    }

    let raw_port = if endpoint.starts_with('[') {
        let host_end = endpoint.find("]:")?;
        &endpoint[host_end + 2..]
    } else {
        let separator_index = endpoint.rfind(':')?;
        &endpoint[separator_index + 1..]
    };
    let port = raw_port.trim().parse::<u16>().ok()?;
    (port > 0).then_some(port)
}

fn parse_positive_u32(value: &str) -> Option<u32> {
    let parsed = value.trim().parse::<u64>().ok()?;
    if parsed == 0 || parsed > u32::MAX as u64 {
        return None;
    }
    Some(parsed as u32)
}

fn parse_worktree_parent_project_id_for_listener(
    project_id: &str,
    worktree_json: &str,
) -> Result<Option<String>> {
    let value: Value = serde_json::from_str(worktree_json).with_context(|| {
        format!("parse Portless listener worktree metadata for project {project_id}")
    })?;
    let Some(worktree) = value.as_object() else {
        return Ok(None);
    };
    let Some(parent_project_id) = trimmed_json_string(worktree.get("parentProjectId")) else {
        return Ok(None);
    };
    validate_stable_key("parentProjectId", &parent_project_id)?;
    Ok(Some(parent_project_id))
}

fn read_settings_text(settings_json: &str, key: &str) -> Option<String> {
    serde_json::from_str::<Value>(settings_json)
        .ok()
        .and_then(|value| {
            value
                .as_object()
                .and_then(|settings| settings.get(key))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn is_portless_listener_eligible_session(
    lifecycle_state: &str,
    launch_settings_json: &str,
    runtime_settings_json: &str,
) -> bool {
    lifecycle_state == "running"
        && read_settings_text(runtime_settings_json, "sessionPersistenceProvider").as_deref()
            == Some("zmx")
        && read_settings_text(launch_settings_json, "surface").as_deref() != Some("commands")
        && read_settings_text(runtime_settings_json, "surface").as_deref() != Some("commands")
}

fn portless_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessDomainIdentities {
    pub projects: Vec<PortlessProjectSlug>,
    pub worktrees: Vec<PortlessWorktreeDomainParts>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessWorktreeDomainParts {
    pub parent_project_id: String,
    pub project_slug: String,
    pub worktree_project_id: String,
    pub worktree_key: String,
    pub worktree_slug: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessProjectSlug {
    pub project_id: String,
    pub slug: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessWorktreeSlug {
    pub project_id: String,
    pub worktree_key: String,
    pub slug: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessState {
    pub enabled: bool,
    pub protocol: PortlessProtocol,
    pub setup_ownership: PortlessSetupOwnership,
    pub setup_status: PortlessSetupStatus,
    pub runtime_status: PortlessRuntimeStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessStateRecord {
    pub state: PortlessState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessProtocol {
    Https,
    Http,
}

impl PortlessProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
        }
    }

    fn from_storage(value: &str) -> Result<Self> {
        match value {
            "https" => Ok(Self::Https),
            "http" => Ok(Self::Http),
            _ => bail!("Invalid Portless protocol metadata value."),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessSetupOwnership {
    Unknown,
    Missing,
    Ghostex,
    Standalone,
}

impl PortlessSetupOwnership {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Missing => "missing",
            Self::Ghostex => "ghostex",
            Self::Standalone => "standalone",
        }
    }

    fn from_storage(value: &str) -> Result<Self> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "missing" => Ok(Self::Missing),
            "ghostex" => Ok(Self::Ghostex),
            "standalone" => Ok(Self::Standalone),
            _ => bail!("Invalid Portless setup ownership metadata value."),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessSetupStatus {
    Unknown,
    Needed,
    Active,
    Failed,
    Disabled,
    Postponed,
}

impl PortlessSetupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Needed => "needed",
            Self::Active => "active",
            Self::Failed => "failed",
            Self::Disabled => "disabled",
            Self::Postponed => "postponed",
        }
    }

    fn from_storage(value: &str) -> Result<Self> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "needed" => Ok(Self::Needed),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            "disabled" => Ok(Self::Disabled),
            "postponed" => Ok(Self::Postponed),
            _ => bail!("Invalid Portless setup status metadata value."),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessRuntimeStatus {
    Unknown,
    Inactive,
    Active,
    Failed,
}

impl PortlessRuntimeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Inactive => "inactive",
            Self::Active => "active",
            Self::Failed => "failed",
        }
    }

    fn from_storage(value: &str) -> Result<Self> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "inactive" => Ok(Self::Inactive),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            _ => bail!("Invalid Portless runtime status metadata value."),
        }
    }
}

struct PortlessStateStorageRow {
    enabled: i64,
    protocol: String,
    setup_ownership: String,
    setup_status: String,
    runtime_status: String,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug)]
struct PortlessServiceExpectation {
    home_dir: PathBuf,
    expected_node_paths: Vec<String>,
    expected_cli_paths: Vec<String>,
    expected_state_dir: String,
    protocol: PortlessProtocol,
    proxy_port: u16,
}

#[derive(Clone, Debug)]
struct PortlessLaunchdPlist {
    label: Option<String>,
    program_arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    standard_out_path: Option<String>,
    standard_error_path: Option<String>,
}

#[derive(Default)]
struct PortlessListenerSnapshotSections {
    listener_output: String,
    ps_output: String,
    zmx_list_output: String,
}

struct PortlessSnapshotCommandOutput {
    exit_code: i32,
    stdout: String,
    stdout_truncated: bool,
}

struct RawPortlessListenerCandidateSession {
    project_id: String,
    session_id: String,
    zmx_name: String,
    lifecycle_state: String,
    launch_settings_json: String,
    runtime_settings_json: String,
    worktree_json: String,
}

#[derive(Clone, Debug)]
struct PortlessListenerCandidateSession {
    project_id: String,
    session_id: String,
    zmx_name: String,
    worktree_parent_project_id: Option<String>,
}

impl PortlessListenerCandidateSession {
    fn from_raw(row: RawPortlessListenerCandidateSession) -> Result<Option<Self>> {
        if !is_portless_listener_eligible_session(
            &row.lifecycle_state,
            &row.launch_settings_json,
            &row.runtime_settings_json,
        ) {
            return Ok(None);
        }
        let zmx_name = row.zmx_name.trim();
        if zmx_name.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            project_id: row.project_id.clone(),
            session_id: row.session_id,
            zmx_name: zmx_name.to_string(),
            worktree_parent_project_id: parse_worktree_parent_project_id_for_listener(
                &row.project_id,
                &row.worktree_json,
            )?,
        }))
    }
}

#[derive(Clone, Debug)]
struct PortlessProcessRow {
    pid: i64,
    ppid: i64,
}

#[derive(Clone, Debug)]
struct PortlessProcessOwner {
    depth: usize,
    session_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PortlessTcpListenerRow {
    pid: u32,
    port: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PortlessRouteTarget {
    port: u16,
    pid: u32,
}

struct RawProjectBackfillRow {
    project_id: String,
    name: String,
    path: Option<String>,
    worktree_json: String,
}

#[derive(Clone)]
struct PortlessProjectBackfillRow {
    project_id: String,
    name: String,
    path: Option<String>,
    worktree: Option<PortlessWorktreeBackfillMetadata>,
}

#[derive(Clone)]
struct PortlessWorktreeBackfillMetadata {
    parent_project_id: String,
    name: Option<String>,
    branch: Option<String>,
}

impl PortlessProjectBackfillRow {
    fn from_raw(row: RawProjectBackfillRow) -> Result<Self> {
        let worktree = parse_worktree_backfill_metadata(&row.project_id, &row.worktree_json)?;
        Ok(Self {
            project_id: row.project_id,
            name: row.name,
            path: row.path,
            worktree,
        })
    }
}

fn project_slug_from_row(row: &Row<'_>) -> rusqlite::Result<PortlessProjectSlug> {
    Ok(PortlessProjectSlug {
        project_id: row.get("projectId")?,
        slug: row.get("projectSlug")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
    })
}

fn worktree_slug_from_row(row: &Row<'_>) -> rusqlite::Result<PortlessWorktreeSlug> {
    Ok(PortlessWorktreeSlug {
        project_id: row.get("projectId")?,
        worktree_key: row.get("worktreeKey")?,
        slug: row.get("worktreeSlug")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
    })
}

fn parse_worktree_backfill_metadata(
    project_id: &str,
    worktree_json: &str,
) -> Result<Option<PortlessWorktreeBackfillMetadata>> {
    let value: Value = serde_json::from_str(worktree_json)
        .with_context(|| format!("parse Portless worktree metadata for project {project_id}"))?;
    let Some(worktree) = value.as_object() else {
        return Ok(None);
    };
    let Some(parent_project_id) = trimmed_json_string(worktree.get("parentProjectId")) else {
        return Ok(None);
    };
    validate_stable_key("parentProjectId", &parent_project_id)?;
    Ok(Some(PortlessWorktreeBackfillMetadata {
        parent_project_id,
        name: trimmed_json_string(worktree.get("name")),
        branch: trimmed_json_string(worktree.get("branch")),
    }))
}

fn trimmed_json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn stable_worktree_key(row: &PortlessProjectBackfillRow) -> String {
    row.project_id.clone()
}

fn project_base_slug(row: &PortlessProjectBackfillRow) -> String {
    hostname_safe_slug(&row.name)
        .or_else(|| {
            row.path
                .as_deref()
                .and_then(path_basename)
                .and_then(hostname_safe_slug)
        })
        .unwrap_or_else(|| deterministic_fallback_slug("project", &row.project_id))
}

fn worktree_base_slug(worktree: &PortlessWorktreeBackfillMetadata, worktree_key: &str) -> String {
    worktree
        .name
        .as_deref()
        .and_then(hostname_safe_slug)
        .or_else(|| {
            worktree
                .branch
                .as_deref()
                .and_then(branch_last_segment)
                .and_then(hostname_safe_slug)
        })
        .unwrap_or_else(|| deterministic_fallback_slug("wt", worktree_key))
}

fn deterministic_fallback_slug(prefix: &str, stable_id: &str) -> String {
    append_slug_suffix(prefix, &stable_hex_suffix("fallback", stable_id, 10))
}

fn allocate_slug(
    reserved_slugs: &HashSet<String>,
    base_slug: &str,
    namespace: &str,
    stable_id: &str,
) -> Result<String> {
    validate_slug("baseSlug", base_slug)?;
    if !reserved_slugs.contains(base_slug) {
        return Ok(base_slug.to_string());
    }
    for length in STABLE_SUFFIX_HEX_LENGTHS {
        let suffix = stable_hex_suffix(namespace, stable_id, *length);
        let candidate = append_slug_suffix(base_slug, &suffix);
        validate_slug("candidateSlug", &candidate)?;
        if !reserved_slugs.contains(&candidate) {
            return Ok(candidate);
        }
    }
    for attempt in 1..=1024 {
        let suffix = stable_hex_suffix(namespace, &format!("{stable_id}\0{attempt}"), 32);
        let candidate = append_slug_suffix(base_slug, &suffix);
        validate_slug("candidateSlug", &candidate)?;
        if !reserved_slugs.contains(&candidate) {
            return Ok(candidate);
        }
    }
    bail!("Unable to allocate a stable Portless slug.")
}

fn append_slug_suffix(base_slug: &str, suffix: &str) -> String {
    let max_base_len = MAX_HOST_LABEL_LEN.saturating_sub(suffix.len() + 1);
    let base = truncate_slug_label(base_slug, max_base_len);
    format!("{base}-{suffix}")
}

fn stable_hex_suffix(namespace: &str, stable_id: &str, hex_len: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    hasher.update(stable_id.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex.truncate(hex_len);
    hex
}

fn hostname_safe_slug(input: &str) -> Option<String> {
    let mut output = String::new();
    let mut last_was_hyphen = false;
    for byte in input.trim().bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => {
                output.push(byte as char);
                last_was_hyphen = false;
            }
            b'A'..=b'Z' => {
                output.push(byte.to_ascii_lowercase() as char);
                last_was_hyphen = false;
            }
            _ => {
                if !output.is_empty() && !last_was_hyphen {
                    output.push('-');
                    last_was_hyphen = true;
                }
            }
        }
    }
    let label = truncate_slug_label(&output, MAX_HOST_LABEL_LEN);
    (!label.is_empty()).then_some(label)
}

fn truncate_slug_label(input: &str, max_len: usize) -> String {
    let mut value = input.trim_matches('-').to_string();
    if value.len() > max_len {
        value.truncate(max_len);
        value = value.trim_matches('-').to_string();
    }
    value
}

fn path_basename(path: &str) -> Option<&str> {
    let trimmed = path.trim();
    let without_trailing_separator = trimmed.trim_end_matches(&['/', '\\'][..]);
    let candidate = if without_trailing_separator.is_empty() {
        trimmed
    } else {
        without_trailing_separator
    };
    candidate
        .rsplit(&['/', '\\'][..])
        .find(|segment| !segment.trim().is_empty())
}

fn branch_last_segment(branch: &str) -> Option<&str> {
    let trimmed = branch.trim().trim_matches('/');
    let without_refs = trimmed
        .strip_prefix("refs/heads/")
        .unwrap_or(trimmed)
        .trim_matches('/');
    without_refs
        .rsplit('/')
        .find(|segment| !segment.trim().is_empty())
}

fn portless_state_from_storage(row: PortlessStateStorageRow) -> Result<PortlessStateRecord> {
    let enabled = match row.enabled {
        0 => false,
        1 => true,
        _ => bail!("Invalid Portless enabled metadata value."),
    };
    Ok(PortlessStateRecord {
        state: PortlessState {
            enabled,
            protocol: PortlessProtocol::from_storage(&row.protocol)?,
            setup_ownership: PortlessSetupOwnership::from_storage(&row.setup_ownership)?,
            setup_status: PortlessSetupStatus::from_storage(&row.setup_status)?,
            runtime_status: PortlessRuntimeStatus::from_storage(&row.runtime_status)?,
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn validate_stable_key(field: &str, value: &str) -> Result<()> {
    ensure!(!value.is_empty(), "{field} is required.");
    ensure!(
        value.len() <= MAX_STABLE_KEY_LEN,
        "{field} is too long for Portless metadata."
    );
    ensure!(
        !value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || ch == '/' || ch == '\\'),
        "{field} must be a stable id, not a display name or path."
    );
    Ok(())
}

fn validate_slug(field: &str, value: &str) -> Result<()> {
    ensure!(!value.is_empty(), "{field} is required.");
    ensure!(
        value.len() <= MAX_HOST_LABEL_LEN,
        "{field} is too long for a local hostname label."
    );
    ensure!(
        value
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-')),
        "{field} must be an explicit hostname-safe slug."
    );
    ensure!(
        !value.starts_with('-') && !value.ends_with('-'),
        "{field} must not start or end with a hyphen."
    );
    Ok(())
}

fn bool_to_sql(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };

    #[test]
    fn project_slug_create_read_update_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "P1main", "First Display Name");
        let repository = PortlessRepository::new(&db);

        let created = repository
            .upsert_project_slug("P1main", "ghostex")
            .expect("create project slug");
        assert_eq!(created.project_id, "P1main");
        assert_eq!(created.slug, "ghostex");
        assert_eq!(
            repository
                .read_project_slug("P1main")
                .expect("read project slug")
                .map(|record| record.slug),
            Some("ghostex".to_string())
        );

        db.execute(
            "UPDATE projects SET name = ?2, updatedAt = ?3 WHERE projectId = ?1",
            params!["P1main", "Renamed Display Name", "2026-06-22T18:41:00.000Z"],
        )
        .expect("rename display name");
        assert_eq!(
            repository
                .read_project_slug("P1main")
                .expect("read after display rename")
                .map(|record| record.slug),
            Some("ghostex".to_string())
        );

        let updated = repository
            .upsert_project_slug("P1main", "ghostex-app")
            .expect("update project slug");
        assert_eq!(updated.slug, "ghostex-app");
        assert_eq!(
            repository
                .read_project_slug("P1main")
                .expect("read updated project slug")
                .map(|record| record.slug),
            Some("ghostex-app".to_string())
        );
    }

    #[test]
    fn worktree_slug_create_read_update_is_separate_from_display_names() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "P2main", "Main Display Name");
        let repository = PortlessRepository::new(&db);

        let created = repository
            .upsert_worktree_slug("P2main", "P2wtfix", "fix-ui")
            .expect("create worktree slug");
        assert_eq!(created.project_id, "P2main");
        assert_eq!(created.worktree_key, "P2wtfix");
        assert_eq!(created.slug, "fix-ui");

        db.execute(
            "UPDATE projects SET name = ?2, updatedAt = ?3 WHERE projectId = ?1",
            params![
                "P2main",
                "Completely Different Display Name",
                "2026-06-22T18:42:00.000Z"
            ],
        )
        .expect("rename project display name");
        assert_eq!(
            repository
                .read_worktree_slug("P2main", "P2wtfix")
                .expect("read worktree slug")
                .map(|record| record.slug),
            Some("fix-ui".to_string())
        );

        let updated = repository
            .upsert_worktree_slug("P2main", "P2wtfix", "fix-ui-2")
            .expect("update worktree slug");
        assert_eq!(updated.slug, "fix-ui-2");
        assert_eq!(
            repository
                .read_worktree_slug("P2main", "P2wtfix")
                .expect("read updated worktree slug")
                .map(|record| record.slug),
            Some("fix-ui-2".to_string())
        );
    }

    #[test]
    fn ensure_project_slug_persists_first_assignment_across_project_renames() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "P1main",
            "First Display Name",
            Some("/tmp/first-display-name"),
            "2026-06-22T18:41:00.000Z",
        );
        let repository = PortlessRepository::new(&db);

        let created = repository
            .ensure_project_slug("P1main")
            .expect("ensure project slug");
        assert_eq!(created.slug, "first-display-name");

        db.execute(
            "UPDATE projects SET name = ?2, path = ?3, updatedAt = ?4 WHERE projectId = ?1",
            params![
                "P1main",
                "Renamed Project",
                "/tmp/renamed-project",
                "2026-06-22T18:45:00.000Z"
            ],
        )
        .expect("rename project");
        assert_eq!(
            repository
                .ensure_project_slug("P1main")
                .expect("ensure after rename")
                .slug,
            "first-display-name"
        );
        assert_eq!(
            repository
                .backfill_domain_identities()
                .expect("repeat backfill")
                .projects
                .into_iter()
                .find(|project| project.project_id == "P1main")
                .map(|project| project.slug),
            Some("first-display-name".to_string())
        );
    }

    #[test]
    fn project_slug_uses_path_basename_when_visible_identity_has_no_label() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Ppath",
            "!!!",
            Some("/Users/person/dev/Path Fallback App"),
            "2026-06-22T18:41:00.000Z",
        );
        insert_project_with_path(&db, "Pempty", "!!!", None, "2026-06-22T18:42:00.000Z");
        let repository = PortlessRepository::new(&db);

        assert_eq!(
            repository
                .ensure_project_slug("Ppath")
                .expect("ensure project slug")
                .slug,
            "path-fallback-app"
        );
        let fallback = repository
            .ensure_project_slug("Pempty")
            .expect("ensure project fallback slug")
            .slug;
        assert!(fallback.starts_with("project-"));
        assert_eq!(
            repository
                .ensure_project_slug("Pempty")
                .expect("ensure project fallback slug again")
                .slug,
            fallback
        );
    }

    #[test]
    fn project_slug_collisions_keep_first_clean_slug_and_stable_later_suffixes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Pfirst",
            "Ghostex",
            Some("/tmp/first"),
            "2026-06-22T18:41:00.000Z",
        );
        insert_project_with_path(
            &db,
            "Psecond",
            "Ghostex",
            Some("/tmp/second"),
            "2026-06-22T18:42:00.000Z",
        );
        let repository = PortlessRepository::new(&db);

        let first_backfill = repository
            .backfill_domain_identities()
            .expect("first backfill");
        let first_slug = project_slug(&first_backfill, "Pfirst");
        let second_slug = project_slug(&first_backfill, "Psecond");
        assert_eq!(first_slug, "ghostex");
        assert!(second_slug.starts_with("ghostex-"));
        assert_ne!(first_slug, second_slug);

        db.execute(
            "UPDATE projects SET name = ?2, updatedAt = ?3 WHERE projectId = ?1",
            params!["Pfirst", "Renamed Away", "2026-06-22T18:45:00.000Z"],
        )
        .expect("rename first project");
        let second_backfill = repository
            .backfill_domain_identities()
            .expect("second backfill");
        assert_eq!(project_slug(&second_backfill, "Pfirst"), "ghostex");
        assert_eq!(project_slug(&second_backfill, "Psecond"), second_slug);
        assert_eq!(
            sorted_project_slug_pairs(&first_backfill),
            sorted_project_slug_pairs(&second_backfill)
        );
    }

    #[test]
    fn worktree_suffix_persists_across_worktree_name_and_branch_changes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Pparent",
            "Ghostex",
            Some("/tmp/ghostex"),
            "2026-06-22T18:41:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtfix",
            "Pparent",
            "Worktree Display",
            "Fix UI",
            "feature/fix-ui",
            "2026-06-22T18:42:00.000Z",
        );
        let repository = PortlessRepository::new(&db);

        let created = repository
            .ensure_worktree_slug("Pwtfix")
            .expect("ensure worktree slug");
        assert_eq!(created.parent_project_id, "Pparent");
        assert_eq!(created.project_slug, "ghostex");
        assert_eq!(created.worktree_key, "Pwtfix");
        assert_eq!(created.worktree_slug, "fix-ui");

        update_worktree_metadata(
            &db,
            "Pwtfix",
            "Pparent",
            "Renamed Worktree",
            "feature/renamed-worktree",
        );
        let after_rename = repository
            .ensure_worktree_slug("Pwtfix")
            .expect("ensure after rename");
        assert_eq!(after_rename.worktree_slug, "fix-ui");
        assert_eq!(after_rename.project_slug, "ghostex");
    }

    #[test]
    fn worktree_suffix_uses_name_first_then_branch_last_segment() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Pparent",
            "Ghostex",
            Some("/tmp/ghostex"),
            "2026-06-22T18:41:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtname",
            "Pparent",
            "Name Project",
            "Release Prep",
            "feature/ignored-branch",
            "2026-06-22T18:42:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtbranch",
            "Pparent",
            "Branch Project",
            "!!!",
            "refs/heads/feature/fix/login",
            "2026-06-22T18:43:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtfallback",
            "Pparent",
            "Fallback Project",
            "!!!",
            "///",
            "2026-06-22T18:44:00.000Z",
        );
        let repository = PortlessRepository::new(&db);

        let identities = repository
            .backfill_domain_identities()
            .expect("backfill identities");
        assert_eq!(worktree_slug(&identities, "Pwtname"), "release-prep");
        assert_eq!(worktree_slug(&identities, "Pwtbranch"), "login");
        assert!(worktree_slug(&identities, "Pwtfallback").starts_with("wt-"));
    }

    #[test]
    fn worktree_suffix_collisions_are_stable_per_parent_without_reshuffling() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Pparent",
            "Ghostex",
            Some("/tmp/ghostex"),
            "2026-06-22T18:41:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtfirst",
            "Pparent",
            "First Worktree",
            "Fix UI",
            "feature/fix-ui-a",
            "2026-06-22T18:42:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtsecond",
            "Pparent",
            "Second Worktree",
            "Fix UI",
            "feature/fix-ui-b",
            "2026-06-22T18:43:00.000Z",
        );
        let repository = PortlessRepository::new(&db);

        let first_backfill = repository
            .backfill_domain_identities()
            .expect("first backfill");
        let first_suffix = worktree_slug(&first_backfill, "Pwtfirst");
        let second_suffix = worktree_slug(&first_backfill, "Pwtsecond");
        assert_eq!(first_suffix, "fix-ui");
        assert!(second_suffix.starts_with("fix-ui-"));
        assert_ne!(first_suffix, second_suffix);

        update_worktree_metadata(&db, "Pwtfirst", "Pparent", "Other Name", "feature/other");
        let second_backfill = repository
            .backfill_domain_identities()
            .expect("second backfill");
        assert_eq!(worktree_slug(&second_backfill, "Pwtfirst"), "fix-ui");
        assert_eq!(worktree_slug(&second_backfill, "Pwtsecond"), second_suffix);
        assert_eq!(
            sorted_worktree_slug_pairs(&first_backfill),
            sorted_worktree_slug_pairs(&second_backfill)
        );
    }

    #[test]
    fn setup_runtime_state_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = PortlessRepository::new(&db);

        assert_eq!(repository.read_state().expect("empty state"), None);

        let created = repository
            .upsert_state(PortlessState {
                enabled: true,
                protocol: PortlessProtocol::Https,
                setup_ownership: PortlessSetupOwnership::Missing,
                setup_status: PortlessSetupStatus::Needed,
                runtime_status: PortlessRuntimeStatus::Inactive,
            })
            .expect("create state");
        assert_eq!(
            created.state,
            PortlessState {
                enabled: true,
                protocol: PortlessProtocol::Https,
                setup_ownership: PortlessSetupOwnership::Missing,
                setup_status: PortlessSetupStatus::Needed,
                runtime_status: PortlessRuntimeStatus::Inactive,
            }
        );

        let updated = repository
            .upsert_state(PortlessState {
                enabled: false,
                protocol: PortlessProtocol::Http,
                setup_ownership: PortlessSetupOwnership::Ghostex,
                setup_status: PortlessSetupStatus::Disabled,
                runtime_status: PortlessRuntimeStatus::Active,
            })
            .expect("update state");
        assert_eq!(
            updated.state,
            PortlessState {
                enabled: false,
                protocol: PortlessProtocol::Http,
                setup_ownership: PortlessSetupOwnership::Ghostex,
                setup_status: PortlessSetupStatus::Disabled,
                runtime_status: PortlessRuntimeStatus::Active,
            }
        );
        assert_eq!(
            repository
                .read_state()
                .expect("read state")
                .map(|record| record.state),
            Some(updated.state)
        );
    }

    #[test]
    fn state_update_protocol_change_marks_installed_service_for_reconfigure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = PortlessRepository::new(&db);
        repository
            .upsert_state(portless_state(
                true,
                PortlessSetupOwnership::Ghostex,
                PortlessSetupStatus::Active,
                PortlessRuntimeStatus::Active,
            ))
            .expect("active state");

        let updated = apply_portless_state_update(
            &paths,
            &db,
            PortlessStateUpdate::SetProtocol {
                protocol: PortlessProtocol::Http,
            },
        )
        .expect("protocol update");

        assert_eq!(updated.state.protocol, PortlessProtocol::Http);
        assert_eq!(
            updated.state.setup_ownership,
            PortlessSetupOwnership::Ghostex
        );
        assert_eq!(updated.state.setup_status, PortlessSetupStatus::Needed);
        assert_eq!(
            updated.state.runtime_status,
            PortlessRuntimeStatus::Inactive
        );
        assert!(updated.state.enabled);
    }

    #[test]
    fn state_update_admin_failure_keeps_portless_enabled_and_failed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");

        let updated = apply_portless_state_update(
            &paths,
            &db,
            PortlessStateUpdate::RecordAdminResult {
                action: PortlessAdminResultAction::Install,
                ok: false,
                protocol: Some(PortlessProtocol::Https),
            },
        )
        .expect("failed admin result");

        assert!(updated.state.enabled);
        assert_eq!(updated.state.protocol, PortlessProtocol::Https);
        assert_eq!(
            updated.state.setup_ownership,
            PortlessSetupOwnership::Ghostex
        );
        assert_eq!(updated.state.setup_status, PortlessSetupStatus::Failed);
        assert_eq!(updated.state.runtime_status, PortlessRuntimeStatus::Failed);
        assert_eq!(
            recommended_portless_admin_action(&updated.state),
            Some(PortlessAdminActionKind::Retry)
        );
    }

    #[test]
    fn state_update_retry_success_recovers_failed_setup() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = PortlessRepository::new(&db);
        repository
            .upsert_state(portless_state(
                true,
                PortlessSetupOwnership::Ghostex,
                PortlessSetupStatus::Failed,
                PortlessRuntimeStatus::Failed,
            ))
            .expect("failed state");

        let updated = apply_portless_state_update(
            &paths,
            &db,
            PortlessStateUpdate::RecordAdminResult {
                action: PortlessAdminResultAction::Retry,
                ok: true,
                protocol: Some(PortlessProtocol::Http),
            },
        )
        .expect("retry admin result");

        assert!(updated.state.enabled);
        assert_eq!(updated.state.protocol, PortlessProtocol::Http);
        assert_eq!(
            updated.state.setup_ownership,
            PortlessSetupOwnership::Ghostex
        );
        assert_eq!(updated.state.setup_status, PortlessSetupStatus::Active);
        assert_eq!(updated.state.runtime_status, PortlessRuntimeStatus::Active);
    }

    #[test]
    fn state_update_disable_clears_routes_without_removing_service() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = PortlessRepository::new(&db);
        repository
            .upsert_state(portless_state(
                true,
                PortlessSetupOwnership::Ghostex,
                PortlessSetupStatus::Active,
                PortlessRuntimeStatus::Active,
            ))
            .expect("active state");
        sync_portless_routes(&paths, &[route("clear-on-disable.localhost", 3000, 42)])
            .expect("seed route");

        let updated = apply_portless_state_update(
            &paths,
            &db,
            PortlessStateUpdate::SetEnabled { enabled: false },
        )
        .expect("disable update");

        assert!(!updated.state.enabled);
        assert_eq!(
            updated.state.setup_ownership,
            PortlessSetupOwnership::Ghostex
        );
        assert_eq!(updated.state.setup_status, PortlessSetupStatus::Disabled);
        assert_routes_file(&paths, &[]);
    }

    #[test]
    fn state_update_explicit_remove_service_is_separate_from_disable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let repository = PortlessRepository::new(&db);
        repository
            .upsert_state(portless_state(
                true,
                PortlessSetupOwnership::Ghostex,
                PortlessSetupStatus::Active,
                PortlessRuntimeStatus::Active,
            ))
            .expect("active state");
        sync_portless_routes(&paths, &[route("clear-on-remove.localhost", 5173, 77)])
            .expect("seed route");

        let updated = apply_portless_state_update(
            &paths,
            &db,
            PortlessStateUpdate::RecordAdminResult {
                action: PortlessAdminResultAction::Remove,
                ok: true,
                protocol: None,
            },
        )
        .expect("remove admin result");

        assert!(updated.state.enabled);
        assert_eq!(
            updated.state.setup_ownership,
            PortlessSetupOwnership::Missing
        );
        assert_eq!(updated.state.setup_status, PortlessSetupStatus::Needed);
        assert_eq!(
            updated.state.runtime_status,
            PortlessRuntimeStatus::Inactive
        );
        assert_routes_file(&paths, &[]);
    }

    #[test]
    fn persistence_apis_do_not_create_or_require_portless_state_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let portless_state_dir = paths.portless_state_dir.clone();
        assert!(!portless_state_dir.exists());

        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "P3main", "Display Name");
        insert_worktree_project(
            &db,
            "P3wt",
            "P3main",
            "Worktree Project",
            "Feature A",
            "feature/a",
            "2026-06-22T18:42:00.000Z",
        );
        let repository = PortlessRepository::new(&db);
        repository
            .upsert_project_slug("P3main", "metadata-only")
            .expect("project slug");
        repository
            .upsert_worktree_slug("P3main", "P3wt", "feature-a")
            .expect("worktree slug");
        repository
            .upsert_state(PortlessState {
                enabled: true,
                protocol: PortlessProtocol::Https,
                setup_ownership: PortlessSetupOwnership::Unknown,
                setup_status: PortlessSetupStatus::Unknown,
                runtime_status: PortlessRuntimeStatus::Unknown,
            })
            .expect("state");
        repository
            .backfill_domain_identities()
            .expect("backfill identities");
        repository
            .ensure_project_slug("P3main")
            .expect("ensure project slug");
        repository
            .ensure_worktree_slug("P3wt")
            .expect("ensure worktree slug");

        assert!(!portless_state_dir.exists());
        assert!(repository
            .read_project_slug("P3main")
            .expect("read project slug")
            .is_some());
        assert!(repository
            .read_worktree_slug("P3main", "P3wt")
            .expect("read worktree slug")
            .is_some());
        assert!(repository.read_state().expect("read state").is_some());
        assert!(!portless_state_dir.exists());
    }

    #[test]
    fn path_computation_includes_ghostex_managed_portless_state_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

        assert_eq!(
            paths.portless_state_dir,
            temp.path()
                .join(".ghostex")
                .join("gxserver")
                .join("portless")
        );
        assert!(paths.portless_state_dir.starts_with(&paths.root_dir));
    }

    #[test]
    fn ensure_portless_state_dir_creates_writable_directory_under_gxserver_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

        ensure_portless_state_dir(&paths).expect("ensure Portless state dir");

        assert!(paths.portless_state_dir.starts_with(&paths.root_dir));
        assert!(fs::metadata(&paths.portless_state_dir)
            .expect("Portless state dir metadata")
            .is_dir());
        let probe = paths.portless_state_dir.join("probe");
        fs::write(&probe, b"ok").expect("write probe");
        assert_eq!(fs::read(&probe).expect("read probe"), b"ok");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&paths.portless_state_dir)
                    .expect("Portless state dir metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
    }

    #[test]
    fn route_sync_uses_routes_lock_and_blocks_when_lock_is_active() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
        fs::create_dir(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK)).expect("lock dir");

        let result = sync_portless_routes_with_options(
            &paths,
            &[route("blocked.localhost", 5173, 42)],
            PortlessRouteSyncOptions {
                lock_timeout: Duration::from_millis(25),
                lock_retry_delay: Duration::from_millis(5),
                stale_lock_age: Duration::from_secs(3600),
            },
        );

        assert!(result.is_err());
        assert!(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK).exists());
        assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
        fs::remove_dir_all(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK))
            .expect("remove test lock");
    }

    #[test]
    fn route_sync_removes_deterministically_stale_routes_lock() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
        fs::create_dir(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK)).expect("lock dir");

        sync_portless_routes_with_options(
            &paths,
            &[route("fresh.localhost", 3000, 77)],
            PortlessRouteSyncOptions {
                lock_timeout: Duration::from_millis(100),
                lock_retry_delay: Duration::from_millis(1),
                stale_lock_age: Duration::ZERO,
            },
        )
        .expect("sync after stale lock removal");

        assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK).exists());
        assert_routes_file(&paths, &[route("fresh.localhost", 3000, 77)]);
    }

    #[test]
    fn route_sync_replaces_stale_routes_with_exact_desired_set() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
        fs::write(
            paths.portless_state_dir.join(PORTLESS_ROUTES_FILE),
            serde_json::to_string_pretty(&vec![
                route("old.localhost", 3000, 11),
                route("keep.localhost", 5173, 12),
            ])
            .expect("serialize old routes"),
        )
        .expect("write old routes");

        sync_portless_routes(
            &paths,
            &[
                route("keep.localhost", 5173, 12),
                route("new.keep.localhost", 8080, 13),
            ],
        )
        .expect("sync routes");

        assert_routes_file(
            &paths,
            &[
                route("keep.localhost", 5173, 12),
                route("new.keep.localhost", 8080, 13),
            ],
        );
        assert_no_portless_temp_artifacts(&paths);
    }

    #[test]
    fn route_sync_writes_valid_json_atomically_and_cleans_temp_artifacts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
        fs::write(
            paths.portless_state_dir.join(PORTLESS_ROUTES_FILE),
            r#"[{"hostname":"previous.localhost","port":3000,"pid":21}]"#,
        )
        .expect("write previous routes");

        sync_portless_routes(&paths, &[route("atomic.localhost", 5174, 22)]).expect("sync routes");

        let text = fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
            .expect("read routes file");
        let parsed: Vec<PortlessRoute> = serde_json::from_str(&text).expect("valid routes json");
        assert_eq!(parsed, vec![route("atomic.localhost", 5174, 22)]);
        assert!(!text.contains("previous.localhost"));
        assert_no_portless_temp_artifacts(&paths);
    }

    #[test]
    fn route_sync_empty_desired_routes_leaves_empty_valid_routes_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

        sync_portless_routes(&paths, &[route("clear-me.localhost", 3000, 31)])
            .expect("initial sync");
        sync_portless_routes(&paths, &[]).expect("empty sync");

        let routes_path = paths.portless_state_dir.join(PORTLESS_ROUTES_FILE);
        assert!(routes_path.exists());
        assert_eq!(
            serde_json::from_str::<Vec<PortlessRoute>>(
                &fs::read_to_string(routes_path).expect("read empty routes")
            )
            .expect("parse empty routes"),
            Vec::<PortlessRoute>::new()
        );
        assert_no_portless_temp_artifacts(&paths);
    }

    #[test]
    fn background_sync_policy_clears_routes_when_disabled_regardless_of_setup_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        sync_portless_routes(&paths, &[route("stale.localhost", 3000, 61)])
            .expect("initial stale route");
        let disabled_active_state = portless_state(
            false,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Active,
            PortlessRuntimeStatus::Active,
        );

        let outcome = apply_portless_background_sync_policy(
            &paths,
            Some(&disabled_active_state),
            &[route("desired.localhost", 5173, 62)],
            1,
        )
        .expect("apply disabled policy");

        assert_eq!(
            outcome.action,
            PortlessBackgroundRouteAction::ClearMirroredRoutes
        );
        assert_eq!(outcome.status, PortlessBackgroundStatus::Disabled);
        assert_eq!(outcome.desired_route_count, 1);
        assert_routes_file(&paths, &[]);
    }

    #[test]
    fn background_sync_policy_skips_setup_missing_without_writing_desired_routes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let setup_missing_state = portless_state(
            true,
            PortlessSetupOwnership::Missing,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        );

        let outcome = apply_portless_background_sync_policy(
            &paths,
            Some(&setup_missing_state),
            &[route("missing.localhost", 5173, 71)],
            1,
        )
        .expect("apply missing policy");

        assert_eq!(
            outcome.action,
            PortlessBackgroundRouteAction::SkipRouteFileWrite
        );
        assert_eq!(outcome.status, PortlessBackgroundStatus::SetupNeeded);
        assert_eq!(outcome.desired_route_count, 1);
        assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
    }

    #[test]
    fn background_sync_policy_skips_failed_setup_without_writing_desired_routes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let failed_state = portless_state(
            true,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Failed,
            PortlessRuntimeStatus::Failed,
        );

        let outcome = apply_portless_background_sync_policy(
            &paths,
            Some(&failed_state),
            &[route("failed.localhost", 8080, 81)],
            1,
        )
        .expect("apply failed policy");

        assert_eq!(
            outcome.action,
            PortlessBackgroundRouteAction::SkipRouteFileWrite
        );
        assert_eq!(outcome.status, PortlessBackgroundStatus::SetupFailed);
        assert_eq!(outcome.desired_route_count, 1);
        assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
    }

    #[test]
    fn background_sync_policy_skips_non_ghostex_setup_without_writing_desired_routes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let standalone_state = portless_state(
            true,
            PortlessSetupOwnership::Standalone,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        );

        let outcome = apply_portless_background_sync_policy(
            &paths,
            Some(&standalone_state),
            &[route("standalone.localhost", 3000, 91)],
            1,
        )
        .expect("apply standalone policy");

        assert_eq!(
            outcome.action,
            PortlessBackgroundRouteAction::SkipRouteFileWrite
        );
        assert_eq!(outcome.status, PortlessBackgroundStatus::SetupNeeded);
        assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
    }

    #[test]
    fn background_sync_policy_mirrors_active_ghostex_routes_and_removes_stale_empty_set() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        sync_portless_routes(&paths, &[route("old.localhost", 3000, 101)])
            .expect("initial stale route");
        let active_state = portless_state(
            true,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Active,
            PortlessRuntimeStatus::Active,
        );

        let mirrored = apply_portless_background_sync_policy(
            &paths,
            Some(&active_state),
            &[
                route("current.localhost", 5173, 102),
                route("p8080.current.localhost", 8080, 103),
            ],
            2,
        )
        .expect("mirror active routes");

        assert_eq!(
            mirrored.action,
            PortlessBackgroundRouteAction::MirrorDesiredRoutes
        );
        assert_eq!(mirrored.status, PortlessBackgroundStatus::SetupActive);
        assert_routes_file(
            &paths,
            &[
                route("current.localhost", 5173, 102),
                route("p8080.current.localhost", 8080, 103),
            ],
        );

        let emptied = apply_portless_background_sync_policy(&paths, Some(&active_state), &[], 0)
            .expect("mirror empty live route set");

        assert_eq!(
            emptied.action,
            PortlessBackgroundRouteAction::MirrorDesiredRoutes
        );
        assert_eq!(emptied.desired_route_count, 0);
        assert_routes_file(&paths, &[]);
    }

    #[test]
    fn background_sync_once_clears_disabled_state_without_live_listener_snapshot() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        PortlessRepository::new(&db)
            .upsert_state(portless_state(
                false,
                PortlessSetupOwnership::Missing,
                PortlessSetupStatus::Needed,
                PortlessRuntimeStatus::Inactive,
            ))
            .expect("disabled state");
        sync_portless_routes(&paths, &[route("disabled-stale.localhost", 3000, 111)])
            .expect("initial stale route");

        let outcome = run_portless_background_sync_once(&paths).expect("one-shot sync");

        assert_eq!(
            outcome.action,
            PortlessBackgroundRouteAction::ClearMirroredRoutes
        );
        assert_eq!(outcome.status, PortlessBackgroundStatus::Disabled);
        assert_eq!(outcome.live_listener_count, 0);
        assert_eq!(outcome.desired_route_count, 0);
        assert_routes_file(&paths, &[]);
    }

    #[test]
    fn portless_routine_operational_logs_are_debug_gated_while_warnings_persist() {
        let disabled_temp = tempfile::tempdir().expect("disabled tempdir");
        let disabled_paths = get_gxserver_paths(Some(disabled_temp.path().to_path_buf()));
        let disabled_logger = crate::logging::GxserverLogger::new(disabled_paths.clone());
        let outcome = PortlessBackgroundSyncOutcome {
            action: PortlessBackgroundRouteAction::MirrorDesiredRoutes,
            desired_route_count: 2,
            live_listener_count: 1,
            status: PortlessBackgroundStatus::SetupActive,
        };

        log_portless_background_sync_outcome(&disabled_logger, &outcome, 11);
        assert!(!disabled_paths.log_file.exists());

        log_portless_background_sync_failure(
            &disabled_logger,
            PortlessLogErrorCode::BackgroundSyncFailed,
            12,
        );
        let warning_text = fs::read_to_string(&disabled_paths.log_file).expect("read warning log");
        assert!(warning_text.contains("portless.backgroundSyncFailed"));
        assert!(warning_text.contains("backgroundSyncFailed"));
        assert!(!warning_text.contains("portless.backgroundSync\""));

        let enabled_temp = tempfile::tempdir().expect("enabled tempdir");
        let enabled_paths = get_gxserver_paths(Some(enabled_temp.path().to_path_buf()));
        enable_debugging_mode_for_test(&enabled_paths);
        let enabled_logger = crate::logging::GxserverLogger::new(enabled_paths.clone());
        log_portless_background_sync_outcome(&enabled_logger, &outcome, 13);

        let debug_text = fs::read_to_string(&enabled_paths.log_file).expect("read debug log");
        assert!(debug_text.contains("portless.backgroundSync"));
        assert!(debug_text.contains("\"routeCount\":2"));
        assert!(debug_text.contains("\"liveListenerCount\":1"));
        assert_portless_log_text_has_no_forbidden_raw_values(&debug_text);
    }

    #[test]
    fn portless_state_update_logs_do_not_persist_forbidden_raw_values() {
        /*
        CDXC:PortlessLogging 2026-06-23-04:45:
        Phase 17 tests must prove Portless persisted diagnostics do not carry raw project/worktree names, paths, full URLs, hostnames, command text, env values, tokens, secrets, stdout, or stderr. The log helper accepts only enum/count/boolean/protocol state, so the test scans both success and warning entries for those forbidden values and field names.
        */
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        enable_debugging_mode_for_test(&paths);
        let logger = crate::logging::GxserverLogger::new(paths.clone());
        let update = PortlessStateUpdate::RecordAdminResult {
            action: PortlessAdminResultAction::Reconfigure,
            ok: false,
            protocol: Some(PortlessProtocol::Http),
        };
        let record = PortlessStateRecord {
            state: portless_state(
                true,
                PortlessSetupOwnership::Ghostex,
                PortlessSetupStatus::Failed,
                PortlessRuntimeStatus::Failed,
            ),
            created_at: "2026-06-23T00:45:00.000Z".to_string(),
            updated_at: "2026-06-23T00:45:01.000Z".to_string(),
        };

        log_portless_state_update_success(&logger, &update, &record, 21);
        log_portless_state_update_failure(
            &logger,
            &update,
            PortlessLogErrorCode::StateUpdateFailed,
            22,
        );

        let text = fs::read_to_string(&paths.log_file).expect("read Portless log");
        assert!(text.contains("portless.stateUpdate"));
        assert!(text.contains("portless.stateUpdateFailed"));
        assert!(text.contains("\"protocol\":\"http\""));
        assert!(text.contains("\"setupStatus\":\"failed\""));
        assert!(text.contains("\"errorCode\":\"stateUpdateFailed\""));
        assert_portless_log_text_has_no_forbidden_raw_values(&text);
        for forbidden_field in [
            "hostname",
            "path",
            "url",
            "command",
            "env",
            "token",
            "secret",
            "stdout",
            "stderr",
            "projectName",
            "worktreeName",
        ] {
            assert!(
                !text.contains(forbidden_field),
                "Portless log included forbidden field {forbidden_field}: {text}"
            );
        }
    }

    #[test]
    fn service_inspection_classifies_missing_as_setup_needed_install_state() {
        let expectation =
            service_expectation(Path::new("/Users/ghostex-user"), PortlessProtocol::Https);

        let inspection = inspect_portless_service_from_plist_text(
            None,
            &expectation,
            PortlessServiceReachability::default(),
        )
        .expect("inspect missing service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::Missing
        );
        assert!(state.enabled);
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Missing);
        assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
    }

    #[test]
    fn service_inspection_accepts_escaped_ghostex_plist_as_active() {
        let home = Path::new("/Users/ghostex-user");
        let expectation = service_expectation(home, PortlessProtocol::Https);
        let plist = service_plist(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            "~/.ghostex/gxserver/portless",
            443,
            true,
            false,
            false,
            None,
            &["--foreground", "--port", "443", "--https", "--skip-trust"],
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(true),
            },
        )
        .expect("inspect Ghostex service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection,
            PortlessServiceInspection {
                classification: PortlessServiceClassification::GhostexActive,
                mismatch_count: 0,
            }
        );
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
        assert_eq!(state.setup_status, PortlessSetupStatus::Active);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Active);
    }

    #[test]
    fn service_inspection_accepts_http_config_when_protocol_setting_is_http() {
        let home = Path::new("/Users/ghostex-user");
        let expectation = service_expectation(home, PortlessProtocol::Http);
        let plist = service_plist(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            "/Users/ghostex-user/.ghostex/gxserver/portless",
            80,
            false,
            false,
            false,
            None,
            &["--foreground", "--port", "80", "--no-tls", "--skip-trust"],
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(true),
            },
        )
        .expect("inspect HTTP Ghostex service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::GhostexActive
        );
        assert_eq!(inspection.mismatch_count, 0);
        assert_eq!(state.protocol, PortlessProtocol::Http);
        assert_eq!(state.setup_status, PortlessSetupStatus::Active);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Active);
    }

    #[test]
    fn service_inspection_classifies_standalone_service_separately_from_reconfigure() {
        let expectation =
            service_expectation(Path::new("/Users/ghostex-user"), PortlessProtocol::Https);
        let plist = service_plist(
            "/usr/local/bin/node",
            "/usr/local/lib/node_modules/portless/dist/cli.js",
            "/Users/ghostex-user/.portless",
            443,
            true,
            false,
            false,
            None,
            &["--foreground", "--port", "443", "--https", "--skip-trust"],
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(true),
            },
        )
        .expect("inspect standalone service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::Standalone
        );
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Standalone);
        assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
    }

    #[test]
    fn service_inspection_classifies_ghostex_config_mismatch_as_reconfigure_needed() {
        let home = Path::new("/Users/ghostex-user");
        let expectation = service_expectation(home, PortlessProtocol::Https);
        let plist = service_plist_with_lan_ip(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            "/Users/ghostex-user/.ghostex/gxserver/portless",
            80,
            false,
            true,
            true,
            Some("local"),
            Some("192.168.1.42"),
            &[
                "--foreground",
                "--port",
                "80",
                "--no-tls",
                "--lan",
                "--wildcard",
            ],
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(true),
            },
        )
        .expect("inspect mismatch service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::GhostexConfigMismatch
        );
        assert!(inspection.mismatch_count >= 6);
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
        assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
    }

    #[test]
    fn service_inspection_requires_sync_hosts_disabled_for_ghostex_service() {
        let home = Path::new("/Users/ghostex-user");
        let expectation = service_expectation(home, PortlessProtocol::Https);
        let plist = service_plist(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            "/Users/ghostex-user/.ghostex/gxserver/portless",
            443,
            true,
            false,
            false,
            None,
            &["--foreground", "--port", "443", "--https", "--skip-trust"],
        )
        .replace(
            "    <key>PORTLESS_SYNC_HOSTS</key>\n    <string>0</string>\n",
            "",
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(true),
            },
        )
        .expect("inspect sync-hosts mismatch service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::GhostexConfigMismatch
        );
        assert_eq!(inspection.mismatch_count, 1);
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
        assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
    }

    #[test]
    fn service_inspection_rejects_persistent_launchd_output_paths() {
        let home = Path::new("/Users/ghostex-user");
        let expectation = service_expectation(home, PortlessProtocol::Https);
        let plist = service_plist(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            "/Users/ghostex-user/.ghostex/gxserver/portless",
            443,
            true,
            false,
            false,
            None,
            &["--foreground", "--port", "443", "--https", "--skip-trust"],
        )
        .replace(
            "  <key>StandardOutPath</key>\n  <string>/dev/null</string>",
            "  <key>StandardOutPath</key>\n  <string>/Users/ghostex-user/.ghostex/gxserver/portless/service.log</string>",
        )
        .replace(
            "  <key>StandardErrorPath</key>\n  <string>/dev/null</string>",
            "  <key>StandardErrorPath</key>\n  <string>/Users/ghostex-user/.ghostex/gxserver/portless/service.log</string>",
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(true),
            },
        )
        .expect("inspect persistent output mismatch service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::GhostexConfigMismatch
        );
        assert_eq!(inspection.mismatch_count, 2);
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
        assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
    }

    #[test]
    fn service_inspection_treats_moved_ghostex_binary_as_reconfigure_not_standalone() {
        let home = Path::new("/Users/ghostex-user");
        let expectation = service_expectation(home, PortlessProtocol::Https);
        let plist = service_plist(
            "/Applications/Old Ghostex.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Old Ghostex.app/Contents/Resources/Web/portless/dist/cli.js",
            "/Users/ghostex-user/.ghostex/gxserver/portless",
            443,
            true,
            false,
            false,
            None,
            &["--foreground", "--port", "443", "--https", "--skip-trust"],
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(true),
            },
        )
        .expect("inspect moved Ghostex service");

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::GhostexConfigMismatch
        );
        assert!(inspection.mismatch_count >= 2);
    }

    #[test]
    fn service_inspection_classifies_unreachable_ghostex_service_as_failed_retry_state() {
        let home = Path::new("/Users/ghostex-user");
        let expectation = service_expectation(home, PortlessProtocol::Https);
        let plist = service_plist(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            "/Users/ghostex-user/.ghostex/gxserver/portless",
            443,
            true,
            false,
            false,
            None,
            &["--foreground", "--port", "443", "--https", "--skip-trust"],
        );

        let inspection = inspect_portless_service_from_plist_text(
            Some(&plist),
            &expectation,
            PortlessServiceReachability {
                manager_running: Some(true),
                proxy_reachable: Some(false),
            },
        )
        .expect("inspect failed service");
        let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

        assert_eq!(
            inspection.classification,
            PortlessServiceClassification::GhostexFailed
        );
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
        assert_eq!(state.setup_status, PortlessSetupStatus::Failed);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Failed);
    }

    #[test]
    fn service_detection_preserves_existing_disabled_state_without_reenabling() {
        let expectation =
            service_expectation(Path::new("/Users/ghostex-user"), PortlessProtocol::Https);
        let existing = portless_state(
            false,
            PortlessSetupOwnership::Missing,
            PortlessSetupStatus::Disabled,
            PortlessRuntimeStatus::Inactive,
        );
        let inspection = PortlessServiceInspection {
            classification: PortlessServiceClassification::GhostexActive,
            mismatch_count: 0,
        };

        let state = portless_state_for_service_inspection(
            Some(&existing),
            expectation.protocol,
            &inspection,
        );

        assert!(!state.enabled);
        assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
        assert_eq!(state.setup_status, PortlessSetupStatus::Disabled);
        assert_eq!(state.runtime_status, PortlessRuntimeStatus::Active);
    }

    #[test]
    fn route_sync_validation_rejects_pid_zero_invalid_hosts_and_invalid_ports() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        sync_portless_routes(&paths, &[route("valid.localhost", 3000, 41)]).expect("initial sync");
        let original = fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
            .expect("read original routes");

        let invalid_routes = [
            route("pid-zero.localhost", 3000, 0),
            route("port-zero.localhost", 0, 42),
            route("", 3000, 42),
            route("https://raw-url.localhost", 3000, 42),
            route("raw-url.localhost/path", 3000, 42),
            route("localhost", 3000, 42),
            route("bad..label.localhost", 3000, 42),
            route("-bad.localhost", 3000, 42),
            route("bad-.localhost", 3000, 42),
            route("BadUpper.localhost", 3000, 42),
            route("wrong.test", 3000, 42),
        ];

        for invalid in invalid_routes {
            assert!(
                sync_portless_routes(&paths, &[invalid]).is_err(),
                "invalid route should be rejected"
            );
            assert_eq!(
                fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
                    .expect("read routes after rejected sync"),
                original
            );
        }
    }

    #[test]
    fn desired_routes_single_project_server_uses_project_base_domain() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Psingle", "Single App");
        PortlessRepository::new(&db)
            .upsert_project_slug("Psingle", "ghostex")
            .expect("project slug");

        let routes = compute_desired_portless_routes(
            &db,
            &[owned_listener(
                "Psingle",
                "Gdev",
                "S90-Psingle-Gdev",
                None,
                8080,
                81,
            )],
        )
        .expect("desired routes");

        assert_eq!(routes, vec![route("ghostex.localhost", 8080, 81)]);
    }

    #[test]
    fn desired_routes_choose_project_primary_by_port_preference_and_extra_domains() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pmulti", "Multi App");
        PortlessRepository::new(&db)
            .upsert_project_slug("Pmulti", "ghostex")
            .expect("project slug");

        let routes = compute_desired_portless_routes(
            &db,
            &[
                owned_listener("Pmulti", "G8080", "S90-Pmulti-G8080", None, 8080, 80),
                owned_listener("Pmulti", "Glow", "S90-Pmulti-Glow", None, 4000, 40),
                owned_listener("Pmulti", "G5173", "S90-Pmulti-G5173", None, 5173, 51),
            ],
        )
        .expect("desired routes");

        assert_eq!(
            routes,
            vec![
                route("ghostex.localhost", 5173, 51),
                route("p4000.ghostex.localhost", 4000, 40),
                route("p8080.ghostex.localhost", 8080, 80),
            ]
        );
    }

    #[test]
    fn desired_routes_primary_falls_back_to_lowest_port_without_preferred_ports() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pfallback", "Fallback App");
        PortlessRepository::new(&db)
            .upsert_project_slug("Pfallback", "ghostex")
            .expect("project slug");

        let routes = compute_desired_portless_routes(
            &db,
            &[
                owned_listener("Pfallback", "G9000", "S90-Pfallback-G9000", None, 9000, 90),
                owned_listener("Pfallback", "G4242", "S90-Pfallback-G4242", None, 4242, 42),
            ],
        )
        .expect("desired routes");

        assert_eq!(
            routes,
            vec![
                route("ghostex.localhost", 4242, 42),
                route("p9000.ghostex.localhost", 9000, 90),
            ]
        );
    }

    #[test]
    fn desired_routes_worktree_listener_uses_project_and_worktree_base_domain() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pparent", "Parent App");
        insert_worktree_project(
            &db,
            "Pwtfix",
            "Pparent",
            "Worktree App",
            "Fix UI",
            "feature/fix-ui",
            "2026-06-22T18:42:00.000Z",
        );
        let repository = PortlessRepository::new(&db);
        repository
            .upsert_project_slug("Pparent", "ghostex")
            .expect("project slug");
        repository
            .upsert_worktree_slug("Pparent", "Pwtfix", "fix-ui")
            .expect("worktree slug");

        let routes = compute_desired_portless_routes(
            &db,
            &[owned_listener(
                "Pwtfix",
                "Gdev",
                "S90-Pwtfix-Gdev",
                Some("Pparent"),
                8080,
                88,
            )],
        )
        .expect("desired routes");

        assert_eq!(routes, vec![route("ghostex.fix-ui.localhost", 8080, 88)]);
    }

    #[test]
    fn desired_routes_worktree_extra_routes_use_port_prefixed_base_domain() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pparent", "Parent App");
        insert_worktree_project(
            &db,
            "Pwtfix",
            "Pparent",
            "Worktree App",
            "Fix UI",
            "feature/fix-ui",
            "2026-06-22T18:42:00.000Z",
        );
        let repository = PortlessRepository::new(&db);
        repository
            .upsert_project_slug("Pparent", "ghostex")
            .expect("project slug");
        repository
            .upsert_worktree_slug("Pparent", "Pwtfix", "fix-ui")
            .expect("worktree slug");

        let routes = compute_desired_portless_routes(
            &db,
            &[
                owned_listener(
                    "Pwtfix",
                    "G8787",
                    "S90-Pwtfix-G8787",
                    Some("Pparent"),
                    8787,
                    87,
                ),
                owned_listener(
                    "Pwtfix",
                    "G5173",
                    "S90-Pwtfix-G5173",
                    Some("Pparent"),
                    5173,
                    51,
                ),
                owned_listener(
                    "Pwtfix",
                    "G3000",
                    "S90-Pwtfix-G3000",
                    Some("Pparent"),
                    3000,
                    30,
                ),
            ],
        )
        .expect("desired routes");

        assert_eq!(
            routes,
            vec![
                route("ghostex.fix-ui.localhost", 3000, 30),
                route("p5173.ghostex.fix-ui.localhost", 5173, 51),
                route("p8787.ghostex.fix-ui.localhost", 8787, 87),
            ]
        );
    }

    #[test]
    fn desired_routes_are_temporary_and_tied_to_live_listener_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Ptemp", "Temporary App");
        PortlessRepository::new(&db)
            .upsert_project_slug("Ptemp", "ghostex")
            .expect("project slug");
        let primary_listener = owned_listener("Ptemp", "G3000", "S90-Ptemp-G3000", None, 3000, 30);
        let extra_listener = owned_listener("Ptemp", "G5173", "S90-Ptemp-G5173", None, 5173, 51);

        let with_extra = compute_desired_portless_routes(
            &db,
            &[primary_listener.clone(), extra_listener.clone()],
        )
        .expect("desired routes with extra");
        let without_extra = compute_desired_portless_routes(&db, &[primary_listener])
            .expect("desired routes after removal");

        assert_eq!(
            with_extra,
            vec![
                route("ghostex.localhost", 3000, 30),
                route("p5173.ghostex.localhost", 5173, 51),
            ]
        );
        assert_eq!(without_extra, vec![route("ghostex.localhost", 3000, 30)]);
    }

    #[test]
    fn protocol_status_payload_serializes_only_metadata_and_local_native_action_requirements() {
        /*
        CDXC:PortlessProtocol 2026-06-23-00:25:
        Phase 12 status payloads must tell React which Portless setup action is relevant while keeping all privileged actions unavailable in gxserver metadata. The native sidebar may enable them only for local Mac admin bridge execution.
        */
        let payload = portless_status_payload_from_record(
            Some(PortlessStateRecord {
                state: PortlessState {
                    enabled: true,
                    protocol: PortlessProtocol::Https,
                    setup_ownership: PortlessSetupOwnership::Missing,
                    setup_status: PortlessSetupStatus::Needed,
                    runtime_status: PortlessRuntimeStatus::Inactive,
                },
                created_at: "2026-06-23T00:25:00.000Z".to_string(),
                updated_at: "2026-06-23T00:25:00.000Z".to_string(),
            }),
            PortlessPayloadSourceStatus::Current,
        );

        let value = serde_json::to_value(&payload).expect("serialize status payload");
        assert_eq!(value["enabled"], true);
        assert_eq!(value["protocol"], "https");
        assert_eq!(value["setupOwnership"], "missing");
        assert_eq!(value["setupStatus"], "needed");
        assert_eq!(value["runtimeStatus"], "inactive");
        assert_eq!(value["sourceStatus"], "current");
        assert_eq!(value["actions"]["install"]["recommended"], true);
        assert_eq!(value["actions"]["install"]["available"], false);
        assert_eq!(value["actions"]["install"]["localMacOnly"], true);
        assert_eq!(
            value["actions"]["install"]["unavailableReason"],
            "nativeAdminBridgeRequired"
        );
        assert_eq!(value["actions"]["reconfigure"]["recommended"], false);
        assert_eq!(value["actions"]["reconfigure"]["available"], false);

        let text = value.to_string();
        for disallowed in [
            "stdout",
            "stderr",
            "token",
            "cookie",
            "env",
            "commandText",
            "filePath",
            "http://",
            "https://",
            "/tmp/",
        ] {
            assert!(
                !text.contains(disallowed),
                "Portless status payload exposed disallowed field/value {disallowed}"
            );
        }
    }

    #[test]
    fn protocol_route_previews_join_desired_routes_to_stable_ids_without_pids_or_urls() {
        /*
        CDXC:PortlessProtocol 2026-06-23-00:25:
        Route previews are presentation metadata, not Portless file contents. Carry protocol, hostname, port, stable project/session ids, and primary/additional kind so UI can render links later without full URLs, pids, raw paths, command text, or process output.
        */
        let listeners = vec![
            owned_listener(
                "Ppreview",
                "Gprimary",
                "S90-Ppreview-Gprimary",
                None,
                3000,
                30,
            ),
            owned_listener(
                "Ppreview",
                "Gadditional",
                "S90-Ppreview-Gadditional",
                None,
                5173,
                51,
            ),
        ];
        let previews = portless_route_previews_for_desired_routes(
            PortlessProtocol::Https,
            &listeners,
            &[
                route("ghostex.localhost", 3000, 30),
                route("p5173.ghostex.localhost", 5173, 51),
            ],
        );

        let value = serde_json::to_value(&previews).expect("serialize route previews");
        assert_eq!(value[0]["hostname"], "ghostex.localhost");
        assert_eq!(value[0]["kind"], "primary");
        assert_eq!(value[0]["port"], 3000);
        assert_eq!(value[0]["projectId"], "Ppreview");
        assert_eq!(value[0]["protocol"], "https");
        assert_eq!(value[0]["sessionId"], "Gprimary");
        assert_eq!(value[1]["hostname"], "p5173.ghostex.localhost");
        assert_eq!(value[1]["kind"], "additional");
        assert_eq!(value[1]["port"], 5173);
        assert_eq!(value[1]["sessionId"], "Gadditional");

        let text = value.to_string();
        for disallowed in [
            "pid",
            "stdout",
            "stderr",
            "token",
            "cookie",
            "env",
            "commandText",
            "http://",
            "https://",
            "/tmp/",
        ] {
            assert!(
                !text.contains(disallowed),
                "Portless route preview exposed disallowed field/value {disallowed}"
            );
        }
    }

    #[test]
    fn presentation_payload_includes_assigned_domains_without_live_listeners() {
        /*
        CDXC:PortlessSettings 2026-06-23-04:02:
        Assigned domains are persisted slug metadata, not a live listener view.
        The Settings UI needs these hostnames for stopped projects/worktrees
        while route previews remain limited to currently detected dev servers.
        */
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Passigned",
            "Assigned Project",
            Some("/tmp/assigned-project"),
            "2026-06-22T18:42:00.000Z",
        );
        insert_worktree_project(
            &db,
            "PassignedWt",
            "Passigned",
            "Assigned Worktree",
            "Feature UI",
            "feature/ui",
            "2026-06-22T18:43:00.000Z",
        );

        let payload = read_portless_presentation_payload(&db);

        assert_eq!(payload.assigned_domains.len(), 2);
        let project = payload
            .assigned_domains
            .iter()
            .find(|domain| domain.project_id == "Passigned")
            .expect("project assigned domain");
        assert_eq!(project.kind, PortlessAssignedDomainKind::Project);
        assert_eq!(project.parent_project_id, None);
        assert!(project.hostname.ends_with(".localhost"));

        let worktree = payload
            .assigned_domains
            .iter()
            .find(|domain| domain.project_id == "PassignedWt")
            .expect("worktree assigned domain");
        assert_eq!(worktree.kind, PortlessAssignedDomainKind::Worktree);
        assert_eq!(worktree.parent_project_id.as_deref(), Some("Passigned"));
        assert!(worktree.hostname.ends_with(".localhost"));
        assert!(payload.route_previews.is_empty());

        let text = serde_json::to_value(&payload.assigned_domains)
            .expect("serialize assigned domains")
            .to_string();
        for disallowed in [
            "stdout",
            "stderr",
            "token",
            "cookie",
            "env",
            "commandText",
            "filePath",
            "http://",
            "https://",
            "/tmp/",
        ] {
            assert!(
                !text.contains(disallowed),
                "Portless assigned domain exposed disallowed field/value {disallowed}"
            );
        }
    }

    #[test]
    fn presentation_payload_marks_disabled_without_running_listener_detection() {
        /*
        CDXC:PortlessProtocol 2026-06-23-00:25:
        Disabled Portless should render as explicit presentation metadata with no route previews and without probing live listeners, because disabled setup is a user-visible state rather than a listener-discovery failure.
        */
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        PortlessRepository::new(&db)
            .upsert_state(PortlessState {
                enabled: false,
                protocol: PortlessProtocol::Https,
                setup_ownership: PortlessSetupOwnership::Ghostex,
                setup_status: PortlessSetupStatus::Disabled,
                runtime_status: PortlessRuntimeStatus::Inactive,
            })
            .expect("disabled state");

        let payload = read_portless_presentation_payload(&db);

        assert_eq!(
            payload.route_preview_status,
            PortlessRoutePreviewStatus::Disabled
        );
        assert_eq!(payload.live_listener_count, 0);
        assert!(payload.route_previews.is_empty());
        assert_eq!(payload.status.setup_status, PortlessSetupStatus::Disabled);
    }

    #[test]
    fn desired_routes_backfill_missing_slugs_with_stable_allocator() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Pparent",
            "Ghostex",
            Some("/tmp/ghostex"),
            "2026-06-22T18:41:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtfix",
            "Pparent",
            "Worktree App",
            "Fix UI",
            "feature/fix-ui",
            "2026-06-22T18:42:00.000Z",
        );
        let listener = owned_listener(
            "Pwtfix",
            "Gdev",
            "S90-Pwtfix-Gdev",
            Some("Pparent"),
            5173,
            73,
        );

        let first = compute_desired_portless_routes(&db, std::slice::from_ref(&listener))
            .expect("first desired routes");
        let repository = PortlessRepository::new(&db);
        assert_eq!(
            repository
                .read_project_slug("Pparent")
                .expect("read project slug")
                .map(|record| record.slug),
            Some("ghostex".to_string())
        );
        assert_eq!(
            repository
                .read_worktree_slug("Pparent", "Pwtfix")
                .expect("read worktree slug")
                .map(|record| record.slug),
            Some("fix-ui".to_string())
        );

        db.execute(
            "UPDATE projects SET name = ?2, path = ?3, updatedAt = ?4 WHERE projectId = ?1",
            params![
                "Pparent",
                "Renamed Parent",
                "/tmp/renamed-parent",
                "2026-06-22T18:45:00.000Z"
            ],
        )
        .expect("rename parent project");
        update_worktree_metadata(
            &db,
            "Pwtfix",
            "Pparent",
            "Renamed Worktree",
            "feature/renamed-worktree",
        );
        let second =
            compute_desired_portless_routes(&db, &[listener]).expect("second desired routes");

        assert_eq!(first, vec![route("ghostex.fix-ui.localhost", 5173, 73)]);
        assert_eq!(second, first);
        assert!(!paths.portless_state_dir.exists());
    }

    #[test]
    fn owned_listener_detection_maps_manual_dev_server_to_running_session() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pmanual", "Manual App");
        let zmx_name = "S90-Pmanual-Gmanual";
        insert_session_row(&db, "Pmanual", "Gmanual", zmx_name, "running", "zmx");

        let listeners = compute_portless_owned_listeners_from_snapshot(
            &db,
            "name=S90-Pmanual-Gmanual\tpid=100\tclients=1\tcreated=1\tstart_dir=/private",
            r#"
100 1 /bundle/zmx run S90-Pmanual-Gmanual
101 100 -zsh
220 101 npm run dev
221 220 node vite
"#
            .trim(),
            r#"
p221
cnode
n*:5173
"#
            .trim(),
        )
        .expect("owned listeners");

        assert_eq!(
            listeners,
            vec![owned_listener(
                "Pmanual", "Gmanual", zmx_name, None, 5173, 221
            )]
        );
    }

    #[test]
    fn owned_listener_detection_ignores_external_project_looking_listener() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pexternal", "External App");
        insert_session_row(
            &db,
            "Pexternal",
            "Gterminal",
            "S90-Pexternal-Gterminal",
            "running",
            "zmx",
        );

        let listeners = compute_portless_owned_listeners_from_snapshot(
            &db,
            "name=S90-Pexternal-Gterminal\tpid=100\tclients=1\tcreated=1\tstart_dir=/private",
            r#"
100 1 /bundle/zmx run S90-Pexternal-Gterminal
101 100 -zsh
700 1 /Applications/Visual Studio Code.app/Contents/MacOS/Electron /tmp/Pexternal
701 700 node /tmp/Pexternal/node_modules/vite/bin/vite.js
"#
            .trim(),
            r#"
p701
cPexternal-dev
n127.0.0.1:5173
"#
            .trim(),
        )
        .expect("owned listeners");

        assert_eq!(listeners, Vec::<PortlessOwnedListener>::new());
    }

    #[test]
    fn owned_listener_detection_ignores_sleeping_stopped_and_missing_sessions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pstale", "Stale Sessions");
        insert_session_row(
            &db,
            "Pstale",
            "Gsleep",
            "S90-Pstale-Gsleep",
            "sleeping",
            "zmx",
        );
        insert_session_row(&db, "Pstale", "Gstop", "S90-Pstale-Gstop", "stopped", "zmx");
        insert_session_row(&db, "Pstale", "Gmiss", "S90-Pstale-Gmiss", "missing", "zmx");

        let listeners = compute_portless_owned_listeners_from_snapshot(
            &db,
            r#"
name=S90-Pstale-Gsleep pid=200 clients=1
name=S90-Pstale-Gstop pid=300 clients=1
name=S90-Pstale-Gmiss pid=400 clients=1
"#
            .trim(),
            r#"
200 1 -zsh
201 200 node stale-sleep
300 1 -zsh
301 300 node stale-stop
400 1 -zsh
401 400 node stale-missing
"#
            .trim(),
            r#"
p201
n*:3000
p301
n*:5173
p401
n*:8080
"#
            .trim(),
        )
        .expect("owned listeners");

        assert_eq!(listeners, Vec::<PortlessOwnedListener>::new());
    }

    #[test]
    fn owned_listener_detection_drops_exited_listener_absent_from_current_tree() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "Pexit", "Exit App");
        let zmx_name = "S90-Pexit-Gterm";
        insert_session_row(&db, "Pexit", "Gterm", zmx_name, "running", "zmx");
        let zmx_list = "name=S90-Pexit-Gterm pid=500 clients=1";
        let listener_output = r#"
p521
n*:3000
"#
        .trim();

        let first = compute_portless_owned_listeners_from_snapshot(
            &db,
            zmx_list,
            r#"
500 1 /bundle/zmx run S90-Pexit-Gterm
520 500 npm run dev
521 520 node vite
"#
            .trim(),
            listener_output,
        )
        .expect("first owned listeners");
        assert_eq!(
            first,
            vec![owned_listener("Pexit", "Gterm", zmx_name, None, 3000, 521)]
        );

        let current = compute_portless_owned_listeners_from_snapshot(
            &db,
            zmx_list,
            r#"
500 1 /bundle/zmx run S90-Pexit-Gterm
520 500 npm run dev
"#
            .trim(),
            listener_output,
        )
        .expect("current owned listeners");
        assert_eq!(current, Vec::<PortlessOwnedListener>::new());
    }

    #[test]
    fn owned_listener_detection_preserves_worktree_parent_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project_with_path(
            &db,
            "Pparent",
            "Parent App",
            Some("/tmp/parent-app"),
            "2026-06-22T18:41:00.000Z",
        );
        insert_worktree_project(
            &db,
            "Pwtfix",
            "Pparent",
            "Worktree App",
            "Fix UI",
            "feature/fix-ui",
            "2026-06-22T18:42:00.000Z",
        );
        let zmx_name = "S90-Pwtfix-Gdev";
        insert_session_row(&db, "Pwtfix", "Gdev", zmx_name, "running", "zmx");

        let listeners = compute_portless_owned_listeners_from_snapshot(
            &db,
            "name=S90-Pwtfix-Gdev pid=600 clients=1",
            r#"
600 1 /bundle/zmx run S90-Pwtfix-Gdev
601 600 -zsh
602 601 npm run dev
"#
            .trim(),
            r#"
p602
n[::1]:8080
"#
            .trim(),
        )
        .expect("owned listeners");

        assert_eq!(
            listeners,
            vec![owned_listener(
                "Pwtfix",
                "Gdev",
                zmx_name,
                Some("Pparent"),
                8080,
                602
            )]
        );
    }

    #[test]
    fn listener_parser_accepts_lsof_field_rows_and_rejects_invalid_pids_and_ports() {
        let rows = parse_portless_tcp_listener_rows(
            r#"
p0
n*:3000
p42
cnode
n*:0
n*:65536
n*:5173
n[::1]:8080 (LISTEN)
p999999999999
n*:9999
p43
nTCP 127.0.0.1:8000 (LISTEN)
LISTEN 0 511 127.0.0.1:5174 0.0.0.0:* users:(("node",pid=44,fd=23))
LISTEN 0 511 [::1]:9000 [::]:* users:(("vite",pid=45,fd=24))
LISTEN 0 511 *:0 *:* users:(("bad",pid=46,fd=24))
LISTEN 0 511 *:4242 *:* users:(("bad",pid=0,fd=24))
"#
            .trim(),
        );

        assert_eq!(
            rows,
            vec![
                PortlessTcpListenerRow {
                    pid: 42,
                    port: 5173
                },
                PortlessTcpListenerRow {
                    pid: 42,
                    port: 8080
                },
                PortlessTcpListenerRow {
                    pid: 43,
                    port: 8000
                },
                PortlessTcpListenerRow {
                    pid: 44,
                    port: 5174
                },
                PortlessTcpListenerRow {
                    pid: 45,
                    port: 9000
                },
            ]
        );
    }

    fn insert_project(db: &Connection, project_id: &str, name: &str) {
        insert_project_with_path(
            db,
            project_id,
            name,
            Some(&format!("/tmp/{project_id}")),
            "2026-06-22T18:41:00.000Z",
        );
    }

    fn insert_project_with_path(
        db: &Connection,
        project_id: &str,
        name: &str,
        path: Option<&str>,
        created_at: &str,
    ) {
        db.execute(
            r#"
            INSERT INTO projects (projectId, name, path, createdAt, updatedAt)
            VALUES (?1, ?2, ?3, ?4, ?4)
            "#,
            params![project_id, name, path, created_at],
        )
        .expect("insert project");
    }

    fn insert_worktree_project(
        db: &Connection,
        project_id: &str,
        parent_project_id: &str,
        display_name: &str,
        worktree_name: &str,
        branch: &str,
        created_at: &str,
    ) {
        db.execute(
            r#"
            INSERT INTO projects (projectId, name, path, worktreeJson, createdAt, updatedAt)
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            "#,
            params![
                project_id,
                display_name,
                format!("/tmp/{project_id}"),
                worktree_json(parent_project_id, worktree_name, branch),
                created_at,
            ],
        )
        .expect("insert worktree project");
    }

    fn insert_session_row(
        db: &Connection,
        project_id: &str,
        session_id: &str,
        zmx_name: &str,
        lifecycle_state: &str,
        persistence_provider: &str,
    ) {
        db.execute(
            r#"
            INSERT INTO sessions (
              projectId,
              sessionId,
              kind,
              title,
              lifecycleState,
              providerStateJson,
              zmxName,
              launchSettingsJson,
              runtimeSettingsJson,
              completionRulesJson,
              attentionRulesJson,
              notificationRulesJson,
              worktreeJson,
              createdAt,
              updatedAt
            )
            VALUES (?1, ?2, 'terminal', ?3, ?4, ?5, ?6, '{}', ?7, '{}', '{}', '{}', '{}', ?8, ?8)
            "#,
            params![
                project_id,
                session_id,
                format!("Session {session_id}"),
                lifecycle_state,
                serde_json::json!({ "lifecycleState": "exists", "provider": "zmx" }).to_string(),
                zmx_name,
                serde_json::json!({ "sessionPersistenceProvider": persistence_provider })
                    .to_string(),
                "2026-06-22T18:43:00.000Z",
            ],
        )
        .expect("insert session");
    }

    fn update_worktree_metadata(
        db: &Connection,
        project_id: &str,
        parent_project_id: &str,
        worktree_name: &str,
        branch: &str,
    ) {
        db.execute(
            r#"
            UPDATE projects
            SET name = ?2,
                worktreeJson = ?3,
                updatedAt = ?4
            WHERE projectId = ?1
            "#,
            params![
                project_id,
                worktree_name,
                worktree_json(parent_project_id, worktree_name, branch),
                "2026-06-22T18:45:00.000Z",
            ],
        )
        .expect("update worktree metadata");
    }

    fn worktree_json(parent_project_id: &str, worktree_name: &str, branch: &str) -> String {
        serde_json::json!({
            "branch": branch,
            "createdAt": "2026-06-22T18:42:00.000Z",
            "name": worktree_name,
            "parentProjectId": parent_project_id,
            "parentProjectName": "Parent Project",
            "parentProjectPath": "/tmp/parent-project"
        })
        .to_string()
    }

    fn route(hostname: &str, port: u16, pid: u32) -> PortlessRoute {
        PortlessRoute {
            hostname: hostname.to_string(),
            port,
            pid,
        }
    }

    fn owned_listener(
        project_id: &str,
        session_id: &str,
        zmx_name: &str,
        worktree_parent_project_id: Option<&str>,
        port: u16,
        pid: u32,
    ) -> PortlessOwnedListener {
        PortlessOwnedListener {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            zmx_name: zmx_name.to_string(),
            worktree_parent_project_id: worktree_parent_project_id.map(str::to_string),
            port,
            pid,
        }
    }

    fn enable_debugging_mode_for_test(paths: &crate::paths::GxserverPaths) {
        let settings_path = paths
            .home_dir
            .join(".ghostex")
            .join("state")
            .join("native-sidebar-settings.json");
        fs::create_dir_all(settings_path.parent().expect("settings parent"))
            .expect("create settings dir");
        fs::write(
            settings_path,
            r#"{"debuggingMode":true,"diagnosticLogging":{"scenarios":{"gxserver.portless":{"enabled":true}},"version":1}}"#,
        )
        .expect("write debugging setting");
    }

    fn assert_portless_log_text_has_no_forbidden_raw_values(text: &str) {
        for forbidden in [
            "Private Project",
            "Feature Worktree",
            "/Users/person/dev/private-project",
            "https://ghostex.localhost/private?token=SECRET",
            "ghostex.localhost",
            "npm run dev",
            "PORTLESS_STATE_DIR=/Users/person/.ghostex/gxserver/portless",
            "SECRET",
            "stdout payload",
            "stderr payload",
        ] {
            assert!(
                !text.contains(forbidden),
                "Portless log leaked forbidden raw value {forbidden}: {text}"
            );
        }
    }

    fn service_expectation(
        home_dir: &Path,
        protocol: PortlessProtocol,
    ) -> PortlessServiceExpectation {
        PortlessServiceExpectation {
            home_dir: home_dir.to_path_buf(),
            expected_node_paths: vec![normalize_path_for_comparison(Path::new(
                "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            ))],
            expected_cli_paths: vec![normalize_path_for_comparison(Path::new(
                "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            ))],
            expected_state_dir: normalize_path_for_comparison(
                &home_dir.join(".ghostex").join("gxserver").join("portless"),
            ),
            protocol,
            proxy_port: portless_service_port_for_protocol(protocol),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn service_plist(
        node: &str,
        cli: &str,
        state_dir: &str,
        port: u16,
        https: bool,
        lan: bool,
        wildcard: bool,
        tld: Option<&str>,
        proxy_args: &[&str],
    ) -> String {
        service_plist_with_lan_ip(
            node, cli, state_dir, port, https, lan, wildcard, tld, None, proxy_args,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn service_plist_with_lan_ip(
        node: &str,
        cli: &str,
        state_dir: &str,
        port: u16,
        https: bool,
        lan: bool,
        wildcard: bool,
        tld: Option<&str>,
        lan_ip: Option<&str>,
        proxy_args: &[&str],
    ) -> String {
        let mut args = vec![
            node.to_string(),
            cli.to_string(),
            "proxy".to_string(),
            "start".to_string(),
        ];
        args.extend(proxy_args.iter().map(|arg| (*arg).to_string()));
        let mut env = vec![
            ("PORTLESS_STATE_DIR", state_dir.to_string()),
            ("PORTLESS_PORT", port.to_string()),
            ("PORTLESS_HTTPS", if https { "1" } else { "0" }.to_string()),
            ("PORTLESS_LAN", if lan { "1" } else { "0" }.to_string()),
            (
                "PORTLESS_WILDCARD",
                if wildcard { "1" } else { "0" }.to_string(),
            ),
            ("PORTLESS_SYNC_HOSTS", "0".to_string()),
        ];
        if let Some(tld) = tld {
            env.push(("PORTLESS_TLD", tld.to_string()));
        }
        if let Some(lan_ip) = lan_ip {
            env.push(("PORTLESS_LAN_IP", lan_ip.to_string()));
        }
        let args_xml = args
            .iter()
            .map(|arg| format!("    <string>{}</string>", test_xml_escape(arg)))
            .collect::<Vec<_>>()
            .join("\n");
        let env_xml = env
            .iter()
            .map(|(key, value)| {
                format!(
                    "    <key>{}</key>\n    <string>{}</string>",
                    test_xml_escape(key),
                    test_xml_escape(value)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{}</string>
  <key>ProgramArguments</key>
  <array>
{}
  </array>
  <key>EnvironmentVariables</key>
  <dict>
{}
  </dict>
  <key>StandardOutPath</key>
  <string>/dev/null</string>
  <key>StandardErrorPath</key>
  <string>/dev/null</string>
</dict>
</plist>
"#,
            PORTLESS_SERVICE_LABEL, args_xml, env_xml
        )
    }

    fn test_xml_escape(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn portless_state(
        enabled: bool,
        setup_ownership: PortlessSetupOwnership,
        setup_status: PortlessSetupStatus,
        runtime_status: PortlessRuntimeStatus,
    ) -> PortlessState {
        PortlessState {
            enabled,
            protocol: PortlessProtocol::Https,
            setup_ownership,
            setup_status,
            runtime_status,
        }
    }

    fn assert_routes_file(paths: &crate::paths::GxserverPaths, expected: &[PortlessRoute]) {
        let text = fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
            .expect("read routes file");
        let routes: Vec<PortlessRoute> = serde_json::from_str(&text).expect("parse routes file");
        assert_eq!(routes, expected);
    }

    fn assert_no_portless_temp_artifacts(paths: &crate::paths::GxserverPaths) {
        let names = fs::read_dir(&paths.portless_state_dir)
            .expect("read Portless state dir")
            .map(|entry| {
                entry
                    .expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert!(
            names
                .iter()
                .all(|name| !name.starts_with(".routes.json.tmp.")),
            "temporary Portless route files should be cleaned up: {names:?}"
        );
        assert!(
            !names.iter().any(|name| name == PORTLESS_ROUTES_LOCK),
            "Portless routes lock should be released"
        );
    }

    fn project_slug(identities: &PortlessDomainIdentities, project_id: &str) -> String {
        identities
            .projects
            .iter()
            .find(|project| project.project_id == project_id)
            .map(|project| project.slug.clone())
            .expect("project slug")
    }

    fn worktree_slug(identities: &PortlessDomainIdentities, worktree_project_id: &str) -> String {
        identities
            .worktrees
            .iter()
            .find(|worktree| worktree.worktree_project_id == worktree_project_id)
            .map(|worktree| worktree.worktree_slug.clone())
            .expect("worktree slug")
    }

    fn sorted_project_slug_pairs(identities: &PortlessDomainIdentities) -> Vec<(String, String)> {
        let mut pairs = identities
            .projects
            .iter()
            .map(|project| (project.project_id.clone(), project.slug.clone()))
            .collect::<Vec<_>>();
        pairs.sort();
        pairs
    }

    fn sorted_worktree_slug_pairs(identities: &PortlessDomainIdentities) -> Vec<(String, String)> {
        let mut pairs = identities
            .worktrees
            .iter()
            .map(|worktree| {
                (
                    worktree.worktree_project_id.clone(),
                    worktree.worktree_slug.clone(),
                )
            })
            .collect::<Vec<_>>();
        pairs.sort();
        pairs
    }
}
