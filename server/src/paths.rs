use std::{env, path::PathBuf};

use ghostex_paths::GhostexPaths;

#[derive(Clone, Debug)]
pub struct GxserverPaths {
    pub app_cache_dir: PathBuf,
    pub app_config_dir: PathBuf,
    pub app_data_dir: PathBuf,
    pub app_state_dir: PathBuf,
    pub auth_dir: PathBuf,
    pub auth_token_file: PathBuf,
    pub config_file: PathBuf,
    pub extensions_dir: PathBuf,
    pub extensions_store_file: PathBuf,
    pub home_dir: PathBuf,
    pub identity_file: PathBuf,
    pub isolated_agent_home_dir: Option<PathBuf>,
    pub logs_dir: PathBuf,
    pub log_file: PathBuf,
    pub migrations_dir: PathBuf,
    pub root_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub runtime_metadata_file: PathBuf,
    pub state_db_file: PathBuf,
    pub tailcat_dir: PathBuf,
    pub zmx_dir: PathBuf,
}

impl GxserverPaths {
    /// CDXC:ServerDaemon 2026-09-13 WHY:
    /// Hook installation already isolates provider configuration for explicit profiles.
    /// Skill installation and startup refresh must use the same root to avoid rewriting the main user's agent configuration.
    pub fn agent_config_home_dir(&self) -> &std::path::Path {
        self.isolated_agent_home_dir
            .as_deref()
            .unwrap_or(&self.home_dir)
    }
}

/*
CDXC:ServerDaemon 2026-06-14-20:37:
The Rust daemon must use the shared Ghostex XDG/GHOSTEX_HOME path contract: daemon state stays in the resolved state directory, while support-bundle-safe JSONL diagnostics stay in the resolved logs directory.
*/
pub fn get_gxserver_paths(home_dir: Option<PathBuf>) -> GxserverPaths {
    let isolated_agent_home_dir = home_dir.clone().or_else(|| {
        env::var_os("GHOSTEX_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    });
    let storage = home_dir
        .map(GhostexPaths::for_explicit_home)
        .unwrap_or_else(GhostexPaths::resolve);
    let home_dir = storage.home_dir.clone();
    let root_dir = storage.gxserver_state_dir();
    let config_file = storage.gxserver_config_dir().join("config.json");
    let auth_dir = root_dir.join("auth");
    let logs_dir = storage.logs_dir.clone();
    let migrations_dir = root_dir.join("migrations");
    let runtime_dir = root_dir.join("runtime");
    // CDXC:RemotePairing 2026-09-01: the tailcat server key and its derived address
    // blob are daemon-owned state, so they live beside the other gxserver
    // state directories rather than in a tailcat-default home location.
    let tailcat_dir = root_dir.join("tailcat");
    let zmx_dir = root_dir.join("zmx");
    let extensions_dir = storage.extensions_dir();
    let extensions_store_file = storage.extensions_store_file();

    GxserverPaths {
        app_cache_dir: storage.cache_dir,
        app_config_dir: storage.config_dir.clone(),
        app_data_dir: storage.data_dir,
        app_state_dir: storage.state_dir,
        auth_token_file: auth_dir.join("token"),
        auth_dir,
        config_file,
        extensions_dir,
        extensions_store_file,
        home_dir,
        identity_file: root_dir.join("identity.json"),
        isolated_agent_home_dir,
        log_file: logs_dir.join("gxserver.jsonl"),
        logs_dir,
        migrations_dir,
        runtime_metadata_file: runtime_dir.join("server.json"),
        runtime_dir,
        state_db_file: root_dir.join("state.db"),
        tailcat_dir,
        zmx_dir,
        root_dir,
    }
}

/// Moves legacy storage into the current layout and returns the one warning to
/// log, if any. It never fails: items that cannot be moved stay where they are.
/// When the current storage itself refuses writes, the warning says exactly
/// that (path and fix) instead of listing every item it could not move there.
/// Ghostex's own folders are made private first (`GhostexPaths::keep_private`).
pub fn migrate_legacy_storage() -> Option<String> {
    let paths = GhostexPaths::resolve();
    paths.keep_private();
    let report = paths.migrate_legacy_layout();
    if report.skipped.is_empty() {
        return None;
    }
    if let Err(error) = paths.check_writable() {
        return Some(error.to_string());
    }
    report.warning()
}

/// Every folder the gxserver daemon writes to must accept new files.
pub fn check_storage_writable() -> Result<(), ghostex_paths::StorageNotWritable> {
    GhostexPaths::resolve().check_writable()
}
