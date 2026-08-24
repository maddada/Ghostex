use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};

use crate::paths::GxserverPaths;
use crate::storage::open_gxserver_database;

use super::admin::*;
use super::launchd::*;
use super::repository::*;
use super::types::*;

pub(crate) const PORTLESS_ROUTES_FILE: &str = "routes.json";
pub(crate) const PORTLESS_ROUTES_LOCK: &str = "routes.lock";
const PORTLESS_FILE_MODE: u32 = 0o644;
const PORTLESS_DIR_MODE: u32 = 0o755;
const PORTLESS_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const PORTLESS_LOCK_RETRY_DELAY: Duration = Duration::from_millis(10);
const PORTLESS_STALE_LOCK_AGE: Duration = Duration::from_secs(10);
const PORTLESS_PRIMARY_ROUTE_PORT_PREFERENCE: &[u16] = &[3000, 5173, 5174, 8080, 8000];
const PORTLESS_SERVICE_REACHABILITY_TIMEOUT: Duration = Duration::from_millis(250);
pub(crate) static PORTLESS_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

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

pub(crate) fn apply_portless_background_sync_policy(
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

pub(crate) fn is_portless_disabled_state(state: Option<&PortlessState>) -> bool {
    state
        .map(|state| !state.enabled || state.setup_status == PortlessSetupStatus::Disabled)
        .unwrap_or(false)
}

pub(crate) fn probe_portless_proxy_reachable(port: u16) -> bool {
    let addr = ([127, 0, 0, 1], port).into();
    TcpStream::connect_timeout(&addr, PORTLESS_SERVICE_REACHABILITY_TIMEOUT).is_ok()
}

#[derive(Clone, Copy)]
pub(crate) struct PortlessRouteSyncOptions {
    pub(crate) lock_timeout: Duration,
    pub(crate) lock_retry_delay: Duration,
    pub(crate) stale_lock_age: Duration,
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

pub(crate) fn sync_portless_routes_with_options(
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

pub(crate) fn validate_portless_routes(routes: &[PortlessRoute]) -> Result<()> {
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

pub(crate) fn portless_base_domain_for_listener(
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

pub(crate) fn primary_portless_route_target_index(
    targets: &[PortlessRouteTarget],
) -> Option<usize> {
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
