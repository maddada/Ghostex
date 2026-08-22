use std::collections::{HashMap, HashSet};

use anyhow::{bail, ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

use super::slug::*;
use super::types::*;

const PORTLESS_STATE_ID: &str = "global";
const MAX_STABLE_KEY_LEN: usize = 160;

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

struct PortlessStateStorageRow {
    enabled: i64,
    protocol: String,
    setup_ownership: String,
    setup_status: String,
    runtime_status: String,
    created_at: String,
    updated_at: String,
}

struct RawProjectBackfillRow {
    project_id: String,
    name: String,
    path: Option<String>,
    worktree_json: String,
}

#[derive(Clone)]
pub(crate) struct PortlessProjectBackfillRow {
    pub(crate) project_id: String,
    pub(crate) name: String,
    pub(crate) path: Option<String>,
    pub(crate) worktree: Option<PortlessWorktreeBackfillMetadata>,
}

#[derive(Clone)]
pub(crate) struct PortlessWorktreeBackfillMetadata {
    pub(crate) parent_project_id: String,
    pub(crate) name: Option<String>,
    pub(crate) branch: Option<String>,
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

pub(crate) fn validate_stable_key(field: &str, value: &str) -> Result<()> {
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

pub(crate) fn validate_slug(field: &str, value: &str) -> Result<()> {
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

