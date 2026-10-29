use std::{
    env,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

pub mod portable;
pub mod powershell;
pub mod tool_path;

#[cfg(not(target_os = "windows"))]
const PRODUCT_DIR_UNIX: &str = "ghostex";
#[cfg(target_os = "windows")]
const PRODUCT_DIR_NATIVE: &str = "Ghostex";
const LEGACY_MIGRATION_MARKER: &str = "legacy-storage-v4.complete";
const LEGACY_MIGRATION_SKIPPED_FILE: &str = "legacy-storage-v4.skipped";
const PREVIOUS_LEGACY_MIGRATION_MARKERS: &[&str] =
    &["legacy-storage-v2.complete", "legacy-storage-v3.complete"];

/// CDXC:ServerDaemon 2026-09-13 WHY:
/// Isolated app and zmx launchd jobs share a namespace outside the production prefix.
/// This keeps either app's Quit and BG Service action from stopping the other instance.
/// SEE-ALSO: apps/desktop/src/app/helpers/board_gxserver/gxserver_health_and_daemon.rs, server/src/zmx/launch.rs.
pub fn launchd_label_prefix(local_api_port: u16) -> String {
    if nonempty_env_path("GHOSTEX_HOME").is_some() {
        format!("com.madda.ghostex-isolated.{local_api_port}.")
    } else {
        "com.madda.ghostex.".to_string()
    }
}

/// True for a profile kept apart from the user's normal Ghostex: an explicit
/// `GHOSTEX_HOME`, or a Windows portable copy. Such a profile never migrates the
/// user's legacy storage into itself and never takes over user-wide things such
/// as the public CLI wrappers.
pub fn uses_separate_profile() -> bool {
    nonempty_env_path("GHOSTEX_HOME").is_some() || portable::current().is_some()
}

/// The Windows on-demand component store (the web runtime, the Code editor): `GhostexData\components`
/// beside `%LOCALAPPDATA%\Ghostex` (see `component_store_root` in
/// apps/desktop/src/component_store/asset_download.rs for why), or a portable copy's own data folder.
pub fn windows_component_store_dir() -> Option<PathBuf> {
    if let Some(copy) = portable::current() {
        return Some(copy.data_dir.join("components"));
    }
    nonempty_env_path("LOCALAPPDATA").map(|root| root.join("GhostexData/components"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GhostexPaths {
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub home_dir: PathBuf,
    pub legacy_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl GhostexPaths {
    /// Resolve the per-user Ghostex directories.
    ///
    /// `GHOSTEX_HOME` remains an explicit compatibility override for isolated
    /// development/test profiles. When set, it intentionally keeps the old
    /// single-root shape instead of mixing that profile with platform stores.
    /// A Windows portable copy (see `portable`) uses the same shape inside its
    /// own `GhostexData` folder.
    pub fn resolve() -> Self {
        let home_dir = user_home_dir();
        if let Some(root) = nonempty_env_path("GHOSTEX_HOME") {
            return Self::unified(home_dir, root);
        }
        if let Some(copy) = portable::current() {
            return Self::unified(home_dir, copy.data_dir.clone());
        }

        Self::platform_defaults(home_dir)
    }

    /// Resolve production directories and perform the idempotent legacy
    /// migration before returning them to a runtime consumer. Legacy items
    /// that cannot be moved stay where they are; see `migrate_legacy_layout`.
    pub fn resolve_and_migrate() -> Self {
        let paths = Self::resolve();
        paths.migrate_legacy_layout();
        paths
    }

    /// Preserve the historical explicit-home behavior used by isolated daemon
    /// runs without consulting or mutating the real user's platform stores.
    pub fn for_explicit_home(home_dir: PathBuf) -> Self {
        let legacy_dir = home_dir.join(".ghostex");
        Self::unified(home_dir, legacy_dir)
    }

    pub fn sidebar_settings_file(&self) -> PathBuf {
        self.config_dir.join("native-sidebar-settings.json")
    }

    pub fn extensions_dir(&self) -> PathBuf {
        self.data_dir.join("extensions")
    }

    pub fn extensions_store_file(&self) -> PathBuf {
        self.extensions_dir().join("extensions-store.json")
    }

    /// The Ghostty config Ghostex's own terminals load (Ghostty syntax), kept apart from the
    /// Ghostty app's config. A `themes/` folder beside it holds custom terminal themes.
    pub fn terminal_ghostty_config_file(&self) -> PathBuf {
        self.config_dir.join("ghostty").join("config.ghostty")
    }

    /// Written once the one-time copy out of the Ghostty app's config has run.
    pub fn terminal_ghostty_config_migration_marker_file(&self) -> PathBuf {
        self.state_dir.join("terminal-ghostty-config-migrated")
    }

    pub fn gxserver_config_dir(&self) -> PathBuf {
        self.config_dir.join("gxserver")
    }

    pub fn gxserver_state_dir(&self) -> PathBuf {
        self.state_dir.join("gxserver")
    }

    pub fn gxserver_data_dir(&self) -> PathBuf {
        self.data_dir.join("gxserver")
    }

    /// CDXC:PlatformSupport 2026-09-28 WHY:
    /// Windows local starts can install the app outside Program Files (`GHOSTEX_INSTALL_DIR`), and the managed CLI under Data/gxserver cannot tell where the app lives from its own path. The desktop app records its folder here on every gxserver start; readers try it before Program Files.
    /// SEE-ALSO: apps/desktop/src/windows_terminal_backend/native_package.rs, server/src/ghostex_cli/editors/prompt.rs, server/src/ghostex_cli/launchers.rs, apps/desktop/src/app/helpers/remote/windows.rs (the PowerShell readers hardcode `gxserver/windows-app-dir`).
    pub fn windows_app_dir_file(&self) -> PathBuf {
        self.gxserver_data_dir().join("windows-app-dir")
    }

    pub fn clients_dir(&self) -> PathBuf {
        self.config_dir.join("clients")
    }

    pub fn hooks_dir(&self) -> PathBuf {
        self.data_dir.join("hooks")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.data_dir.join("i")
    }

    pub fn attachments_dir(&self) -> PathBuf {
        self.data_dir.join("f")
    }

    pub fn icons_dir(&self) -> PathBuf {
        self.data_dir.join("icons")
    }

    pub fn source_runtime_dir(&self) -> PathBuf {
        self.data_dir.join("source-runtime")
    }

    pub fn code_server_runtime_dir(&self) -> PathBuf {
        self.data_dir.join("code-server-runtime-gpui")
    }

    pub fn cef_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("cef")
    }

    pub fn migration_marker(&self) -> PathBuf {
        self.state_dir
            .join("migrations")
            .join(LEGACY_MIGRATION_MARKER)
    }

    /// CDXC:ServerDaemon 2026-10-16 WHY:
    /// GitHub issue #231: one legacy item the user could not move (a folder left owned by root after a `sudo` run, a file with an immutable flag) failed the whole migration, and both the `ghostex` CLI and gxserver refused to start. Each item is now moved on its own; an item that cannot be moved stays where it is and is reported, the rest of the migration goes on, and the caller logs one warning. Skipped items are retried on the next start (once the user fixes the permissions they move), but a warning is only reported again when the list of skipped items changed.
    pub fn migrate_legacy_layout(&self) -> LegacyMigrationReport {
        if uses_separate_profile() || self.migration_marker().exists() {
            return LegacyMigrationReport::default();
        }

        let mut skipped = Vec::new();
        #[cfg(target_os = "macos")]
        self.migrate_legacy_macos_layout(&mut skipped);

        self.migrate_legacy_dot_directory(&mut skipped);
        self.finish_migration(skipped)
    }

    /// Confirms every folder Ghostex writes to exists and accepts new files.
    pub fn check_writable(&self) -> Result<(), StorageNotWritable> {
        for directory in [
            &self.config_dir,
            &self.state_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.logs_dir,
            &self.runtime_dir,
        ] {
            ensure_writable_directory(directory)?;
        }
        Ok(())
    }

    fn migrate_legacy_dot_directory(&self, skipped: &mut Vec<SkippedLegacyItem>) {
        let legacy_type = match fs::symlink_metadata(&self.legacy_dir) {
            Ok(metadata) => Some(metadata.file_type()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                skip(skipped, &self.legacy_dir, "read it", error);
                return;
            }
        };
        let previous_marker_exists = PREVIOUS_LEGACY_MIGRATION_MARKERS
            .iter()
            .any(|marker| self.state_dir.join("migrations").join(marker).is_file());
        if legacy_type.is_some_and(|file_type| !file_type.is_dir()) {
            move_if_missing(
                skipped,
                &self.legacy_dir,
                &self.data_dir.join("legacy/legacy-dot-ghostex"),
            );
            return;
        }
        if legacy_type.is_none() && !previous_marker_exists {
            return;
        }

        for directory in [
            &self.config_dir,
            &self.state_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.logs_dir,
            &self.runtime_dir,
        ] {
            create_directory(skipped, directory);
        }

        if legacy_type.is_some() {
            // Settings are migrated first so the first updated launch reads the
            // user's existing preferences rather than creating defaults over them.
            move_if_missing(
                skipped,
                &self.legacy_dir.join("state/native-sidebar-settings.json"),
                &self.sidebar_settings_file(),
            );
            move_if_missing(
                skipped,
                &self.legacy_dir.join("editor-window-frame.json"),
                &self.state_dir.join("editor-window-frame.json"),
            );

            migrate_directory_contents(
                skipped,
                &self.legacy_dir.join("clients"),
                &self.clients_dir(),
            );
            migrate_directory_contents(skipped, &self.legacy_dir.join("logs"), &self.logs_dir);
            migrate_directory_contents(skipped, &self.legacy_dir.join("state"), &self.state_dir);
            migrate_gxserver(skipped, &self.legacy_dir.join("gxserver"), self);
            #[cfg(unix)]
            ensure_legacy_gxserver_links(skipped, self);

            for (name, destination) in [
                ("hooks", self.hooks_dir()),
                ("i", self.images_dir()),
                ("f", self.attachments_dir()),
                ("icons", self.icons_dir()),
                ("source-runtime", self.source_runtime_dir()),
                ("code-server-runtime-gpui", self.code_server_runtime_dir()),
                ("chats", self.data_dir.join("chats")),
                ("cli", self.state_dir.join("cli")),
                (
                    "remote-attach-carriers",
                    self.state_dir.join("remote-attach-carriers"),
                ),
                ("zehn", self.cache_dir.join("zehn")),
                ("cef", self.cef_cache_dir()),
            ] {
                migrate_directory_contents(skipped, &self.legacy_dir.join(name), &destination);
            }

            // Unknown legacy entries are retained under Data/legacy rather than
            // discarded. This makes the migration forward-compatible with older
            // builds that may have created a top-level directory no longer known to
            // the current app.
            if let Ok(entries) = fs::read_dir(&self.legacy_dir) {
                for entry in entries.flatten() {
                    let source = entry.path();
                    let name = entry.file_name();
                    if legacy_compatibility_entry(name.as_os_str()) {
                        continue;
                    }
                    let destination = self.data_dir.join("legacy").join(name);
                    move_if_missing(skipped, &source, &destination);
                }
            }
        }

        #[cfg(unix)]
        for (name, destination) in [
            ("i", self.images_dir()),
            ("f", self.attachments_dir()),
            ("chats", self.data_dir.join("chats")),
            ("icons", self.icons_dir()),
            ("logs", self.logs_dir.clone()),
        ] {
            ensure_legacy_directory_link(skipped, &self.legacy_dir.join(name), &destination);
        }

        #[cfg(unix)]
        if legacy_type.is_none() {
            ensure_legacy_gxserver_links(skipped, self);
        }
    }

    #[cfg(target_os = "macos")]
    fn migrate_legacy_macos_layout(&self, skipped: &mut Vec<SkippedLegacyItem>) {
        let application_support = self
            .home_dir
            .join("Library/Application Support")
            .join("Ghostex");
        let legacy_cache = self.home_dir.join("Library/Caches/Ghostex");
        let legacy_logs = self.home_dir.join("Library/Logs/Ghostex");
        let archive_root = self.data_dir.join("legacy/macos-platform-storage");

        for (name, source, destination) in [
            (
                "Config",
                application_support.join("Config"),
                self.config_dir.clone(),
            ),
            (
                "Data",
                application_support.join("Data"),
                self.data_dir.clone(),
            ),
            (
                "State",
                application_support.join("State"),
                self.state_dir.clone(),
            ),
            (
                "Runtime",
                application_support.join("Runtime"),
                self.runtime_dir.clone(),
            ),
            (
                "components",
                application_support.join("components"),
                self.data_dir.join("components"),
            ),
            (
                "on-demand",
                application_support.join("on-demand"),
                self.data_dir.join("on-demand"),
            ),
            ("Cache", legacy_cache.clone(), self.cache_dir.clone()),
            ("Logs", legacy_logs.clone(), self.logs_dir.clone()),
        ] {
            migrate_directory_contents(skipped, &source, &destination);
            move_if_missing(skipped, &source, &archive_root.join(name));
        }

        remove_empty_directory(&application_support);
        move_if_missing(
            skipped,
            &application_support,
            &archive_root.join("ApplicationSupport"),
        );
    }
    fn unified(home_dir: PathBuf, root: PathBuf) -> Self {
        Self {
            cache_dir: root.join("cache"),
            config_dir: root.clone(),
            data_dir: root.clone(),
            home_dir,
            legacy_dir: root.clone(),
            logs_dir: root.join("logs"),
            runtime_dir: root.join("runtime"),
            state_dir: root.join("state"),
        }
    }

    fn platform_defaults(home_dir: PathBuf) -> Self {
        let legacy_dir = home_dir.join(".ghostex");

        #[cfg(target_os = "windows")]
        {
            let roaming =
                nonempty_env_path("APPDATA").unwrap_or_else(|| home_dir.join("AppData/Roaming"));
            let local = nonempty_env_path("LOCALAPPDATA")
                .unwrap_or_else(|| home_dir.join("AppData/Local"))
                .join(PRODUCT_DIR_NATIVE);
            return Self {
                cache_dir: local.join("Cache"),
                config_dir: roaming.join(PRODUCT_DIR_NATIVE),
                data_dir: local.join("Data"),
                home_dir,
                legacy_dir,
                logs_dir: local.join("Logs"),
                runtime_dir: local.join("Runtime"),
                state_dir: local.join("State"),
            };
        }

        #[cfg(not(target_os = "windows"))]
        {
            let config_base = xdg_base("XDG_CONFIG_HOME", &home_dir, ".config");
            let state_base = xdg_base("XDG_STATE_HOME", &home_dir, ".local/state");
            let data_base = xdg_base("XDG_DATA_HOME", &home_dir, ".local/share");
            let cache_base = xdg_base("XDG_CACHE_HOME", &home_dir, ".cache");
            let state_dir = state_base.join(PRODUCT_DIR_UNIX);
            let runtime_dir = nonempty_env_path("XDG_RUNTIME_DIR")
                .map(|base| base.join(PRODUCT_DIR_UNIX))
                .unwrap_or_else(|| state_dir.join("runtime"));
            Self {
                cache_dir: cache_base.join(PRODUCT_DIR_UNIX),
                config_dir: config_base.join(PRODUCT_DIR_UNIX),
                data_dir: data_base.join(PRODUCT_DIR_UNIX),
                home_dir,
                legacy_dir,
                logs_dir: state_dir.join("logs"),
                runtime_dir,
                state_dir,
            }
        }
    }

    fn skipped_items_file(&self) -> PathBuf {
        self.state_dir
            .join("migrations")
            .join(LEGACY_MIGRATION_SKIPPED_FILE)
    }

    fn finish_migration(&self, mut skipped: Vec<SkippedLegacyItem>) -> LegacyMigrationReport {
        if skipped.is_empty() {
            match self.write_migration_marker() {
                Ok(()) => {
                    let _ = fs::remove_file(self.skipped_items_file());
                    return LegacyMigrationReport::default();
                }
                Err(error) => skip(
                    &mut skipped,
                    &self.migration_marker(),
                    "write the migration marker",
                    error,
                ),
            }
        }
        let mut lines = skipped.iter().map(ToString::to_string).collect::<Vec<_>>();
        lines.sort();
        let listing = format!("{}\n", lines.join("\n"));
        let skipped_file = self.skipped_items_file();
        let changed = fs::read_to_string(&skipped_file).ok().as_deref() != Some(listing.as_str());
        if changed {
            if let Some(parent) = skipped_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&skipped_file, listing);
        }
        LegacyMigrationReport {
            skipped,
            skipped_changed: changed,
        }
    }

    fn write_migration_marker(&self) -> io::Result<()> {
        let marker = self.migration_marker();
        if let Some(parent) = marker.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(marker, b"Ghostex XDG storage migration v4\n")
    }
}

/// One legacy storage item the migration could not move. It stays where it was.
#[derive(Debug)]
pub struct SkippedLegacyItem {
    pub path: PathBuf,
    pub action: String,
    pub error: io::Error,
}

impl std::fmt::Display for SkippedLegacyItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: could not {}: {}",
            self.path.display(),
            self.action,
            self.error
        )
    }
}

#[derive(Debug, Default)]
pub struct LegacyMigrationReport {
    pub skipped: Vec<SkippedLegacyItem>,
    /// False when an earlier start already reported exactly these items.
    pub skipped_changed: bool,
}

impl LegacyMigrationReport {
    /// The warning to log, only when items were skipped and the list changed since the last report.
    pub fn warning(&self) -> Option<String> {
        if self.skipped.is_empty() || !self.skipped_changed {
            return None;
        }
        const SHOWN: usize = 10;
        let mut text = format!(
            "Ghostex left {} item(s) of its old storage where they are because they could not be moved. Ghostex keeps working and tries again on its next start.",
            self.skipped.len()
        );
        for item in self.skipped.iter().take(SHOWN) {
            text.push_str(&format!("\n  - {item}"));
        }
        if self.skipped.len() > SHOWN {
            text.push_str(&format!("\n  - and {} more", self.skipped.len() - SHOWN));
        }
        #[cfg(unix)]
        text.push_str("\nIf one of these paths belongs to another user (for example after running a tool with sudo), give it back with: sudo chown -R \"$(id -un)\" \"<path>\"");
        Some(text)
    }
}

/// A folder Ghostex must write to (its settings, state, logs) refuses new files.
#[derive(Debug)]
pub struct StorageNotWritable {
    pub path: PathBuf,
    pub error: io::Error,
}

impl StorageNotWritable {
    pub fn new(path: &Path, error: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            error,
        }
    }
}

impl std::fmt::Display for StorageNotWritable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Ghostex can't write to {}: {}. Ghostex keeps its settings, sessions and logs there and can't start without it.",
            self.path.display(),
            self.error
        )?;
        // The deepest folder that exists is the one refusing the write.
        let blocking = self
            .path
            .ancestors()
            .find(|path| fs::symlink_metadata(path).is_ok())
            .unwrap_or(&self.path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let owner = fs::symlink_metadata(blocking).map(|metadata| metadata.uid());
            let user = fs::metadata(user_home_dir()).map(|metadata| metadata.uid());
            if let (Ok(owner), Ok(user)) = (owner, user) {
                if owner != user {
                    write!(
                        formatter,
                        " {} belongs to another user (uid {owner}), usually after running a tool with sudo.",
                        blocking.display()
                    )?;
                }
            }
            write!(
                formatter,
                " Fix it with: sudo chown -R \"$(id -un)\" \"{}\"",
                blocking.display()
            )
        }
        #[cfg(not(unix))]
        write!(
            formatter,
            " Check that your account can write to {}.",
            blocking.display()
        )
    }
}

impl std::error::Error for StorageNotWritable {}

/// Creates `directory` if needed and proves a new file can be written in it.
pub fn ensure_writable_directory(directory: &Path) -> Result<(), StorageNotWritable> {
    fs::create_dir_all(directory).map_err(|error| StorageNotWritable::new(directory, error))?;
    let probe = directory.join(format!(".ghostex-write-check-{}", std::process::id()));
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
        .map_err(|error| StorageNotWritable::new(directory, error))?;
    let _ = fs::remove_file(probe);
    Ok(())
}

/// Creates the parent folder of `file` if needed and proves `file` can be opened for appending.
pub fn ensure_writable_file(file: &Path) -> Result<(), StorageNotWritable> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|error| StorageNotWritable::new(parent, error))?;
    }
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .map(|_| ())
        .map_err(|error| StorageNotWritable::new(file, error))
}

fn skip(skipped: &mut Vec<SkippedLegacyItem>, path: &Path, action: &str, error: io::Error) {
    skipped.push(SkippedLegacyItem {
        path: path.to_path_buf(),
        action: action.to_string(),
        error,
    });
}

fn create_directory(skipped: &mut Vec<SkippedLegacyItem>, directory: &Path) -> bool {
    match fs::create_dir_all(directory) {
        Ok(()) => true,
        Err(error) => {
            skip(skipped, directory, "create this folder", error);
            false
        }
    }
}

fn migrate_gxserver(skipped: &mut Vec<SkippedLegacyItem>, source: &Path, paths: &GhostexPaths) {
    if !fs::symlink_metadata(source).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return;
    }
    let state_dir = paths.gxserver_state_dir();
    #[cfg(unix)]
    if matches!(
        fs::symlink_metadata(&state_dir),
        Err(ref error) if error.kind() == io::ErrorKind::NotFound
    ) && state_dir
        .parent()
        .is_none_or(|parent| create_directory(skipped, parent))
    {
        match fs::rename(source, &state_dir) {
            Ok(()) => {
                // Move the complete SQLite directory in one operation before
                // recreating the old name. Open Unix file descriptors and
                // locks keep referring to the same inodes, while subsequent
                // opens immediately resolve through the compatibility link.
                if let Err(error) = std::os::unix::fs::symlink(&state_dir, source) {
                    skip(
                        skipped,
                        source,
                        &format!("link to {}", state_dir.display()),
                        error,
                    );
                }
            }
            Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {}
            Err(error) => {
                skip(
                    skipped,
                    source,
                    &format!("move to {}", state_dir.display()),
                    error,
                );
                return;
            }
        }
    }
    let config_dir = paths.gxserver_config_dir();
    move_if_missing(
        skipped,
        &source.join("config.json"),
        &config_dir.join("config.json"),
    );

    let data_dir = paths.gxserver_data_dir();
    for name in ["package", "releases", "windows-app-runtime.sha256"] {
        move_if_missing(skipped, &source.join(name), &data_dir.join(name));
    }

    migrate_directory_contents(skipped, source, &state_dir);
}

fn migrate_directory_contents(
    skipped: &mut Vec<SkippedLegacyItem>,
    source: &Path,
    destination: &Path,
) {
    let source_type = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata.file_type(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => return skip(skipped, source, "read it", error),
    };
    if !source_type.is_dir() || !create_directory(skipped, destination) {
        return;
    }
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) => return skip(skipped, source, "list this folder", error),
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                move_if_missing(skipped, &entry.path(), &destination.join(entry.file_name()))
            }
            Err(error) => skip(skipped, source, "list this folder", error),
        }
    }
    remove_empty_directory(source);
}

fn move_if_missing(skipped: &mut Vec<SkippedLegacyItem>, source: &Path, destination: &Path) {
    let source_type = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata.file_type(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => return skip(skipped, source, "read it", error),
    };
    let destination_type = match fs::symlink_metadata(destination) {
        Ok(metadata) => Some(metadata.file_type()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return skip(skipped, destination, "read it", error),
    };
    if let Some(destination_type) = destination_type {
        if source_type.is_dir() && destination_type.is_dir() {
            migrate_directory_contents(skipped, source, destination);
        }
        return;
    }
    if destination
        .parent()
        .is_some_and(|parent| !create_directory(skipped, parent))
    {
        return;
    }
    match fs::rename(source, destination) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            if let Err(error) = copy_recursively(source, destination) {
                // Drop the partial copy (the destination did not exist before)
                // so a later start retries from the untouched source.
                let _ = if source_type.is_dir() {
                    fs::remove_dir_all(destination)
                } else {
                    fs::remove_file(destination)
                };
                return skip(
                    skipped,
                    source,
                    &format!("copy to {}", destination.display()),
                    error,
                );
            }
            let removed = if source_type.is_dir() {
                fs::remove_dir_all(source)
            } else {
                fs::remove_file(source)
            };
            if let Err(error) = removed {
                skip(
                    skipped,
                    source,
                    &format!("remove it after copying it to {}", destination.display()),
                    error,
                );
            }
        }
        Err(error) => skip(
            skipped,
            source,
            &format!("move to {}", destination.display()),
            error,
        ),
    }
}

#[cfg(unix)]
fn ensure_legacy_directory_link(
    skipped: &mut Vec<SkippedLegacyItem>,
    link: &Path,
    destination: &Path,
) {
    if fs::symlink_metadata(link).is_ok() || !create_directory(skipped, destination) {
        return;
    }
    if link
        .parent()
        .is_some_and(|parent| !create_directory(skipped, parent))
    {
        return;
    }
    if let Err(error) = std::os::unix::fs::symlink(destination, link) {
        skip(
            skipped,
            link,
            &format!("link to {}", destination.display()),
            error,
        );
    }
}

fn legacy_compatibility_entry(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some("i" | "f" | "chats" | "icons" | "logs" | "gxserver")
    )
}

#[cfg(unix)]
fn ensure_legacy_gxserver_links(skipped: &mut Vec<SkippedLegacyItem>, paths: &GhostexPaths) {
    let legacy_root = paths.legacy_dir.join("gxserver");
    let state_root = paths.gxserver_state_dir();
    if !create_directory(skipped, &state_root) {
        return;
    }

    // Keep the old gxserver root atomic. SQLite creates, deletes, and
    // recreates state.db-wal/state.db-shm beside the database path, so linking
    // those files individually can eventually split one database across the
    // legacy and XDG directories. Split config/data entries live as links
    // inside the XDG state root instead, then the whole legacy gxserver path
    // points at that state root.
    for (name, destination) in [
        ("package", paths.gxserver_data_dir().join("package")),
        ("releases", paths.gxserver_data_dir().join("releases")),
    ] {
        if create_directory(skipped, &destination) {
            ensure_gxserver_compatibility_link(
                skipped,
                paths,
                &state_root.join(name),
                &destination,
                name,
            );
        }
    }
    for (name, destination) in [
        (
            "config.json",
            paths.gxserver_config_dir().join("config.json"),
        ),
        (
            "windows-app-runtime.sha256",
            paths.gxserver_data_dir().join("windows-app-runtime.sha256"),
        ),
    ] {
        ensure_gxserver_compatibility_link(
            skipped,
            paths,
            &state_root.join(name),
            &destination,
            name,
        );
    }

    ensure_gxserver_compatibility_link(
        skipped,
        paths,
        &legacy_root,
        &state_root,
        "legacy-gxserver-root",
    );
}

#[cfg(unix)]
fn ensure_gxserver_compatibility_link(
    skipped: &mut Vec<SkippedLegacyItem>,
    paths: &GhostexPaths,
    link: &Path,
    destination: &Path,
    archive_name: &str,
) {
    match fs::symlink_metadata(link) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if fs::read_link(link).is_ok_and(|target| target == destination) {
                return;
            }
            archive_legacy_compatibility_node(skipped, paths, link, archive_name);
        }
        Ok(_) => archive_legacy_compatibility_node(skipped, paths, link, archive_name),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return skip(skipped, link, "read it", error),
    }
    // Archiving was skipped and already reported; the node stays in place.
    if fs::symlink_metadata(link).is_ok() {
        return;
    }
    if destination
        .parent()
        .is_some_and(|parent| !create_directory(skipped, parent))
        || link
            .parent()
            .is_some_and(|parent| !create_directory(skipped, parent))
    {
        return;
    }
    if let Err(error) = std::os::unix::fs::symlink(destination, link) {
        skip(
            skipped,
            link,
            &format!("link to {}", destination.display()),
            error,
        );
    }
}

#[cfg(unix)]
fn archive_legacy_compatibility_node(
    skipped: &mut Vec<SkippedLegacyItem>,
    paths: &GhostexPaths,
    source: &Path,
    archive_name: &str,
) {
    let archive_root = paths
        .data_dir
        .join("legacy/gxserver-compatibility-residuals");
    if !create_directory(skipped, &archive_root) {
        return;
    }
    let base = archive_root.join(archive_name);
    let mut destination = base.clone();
    let mut suffix = 1_u64;
    while fs::symlink_metadata(&destination).is_ok() {
        destination = archive_root.join(format!("{archive_name}.{suffix}"));
        suffix += 1;
    }
    move_if_missing(skipped, source, &destination);
}

fn copy_recursively(source: &Path, destination: &Path) -> io::Result<()> {
    let source_type = fs::symlink_metadata(source)?.file_type();
    if source_type.is_symlink() {
        let target = fs::read_link(source)?;
        #[cfg(unix)]
        return std::os::unix::fs::symlink(target, destination);
        #[cfg(windows)]
        {
            return if fs::metadata(source).is_ok_and(|metadata| metadata.is_dir()) {
                std::os::windows::fs::symlink_dir(target, destination)
            } else {
                std::os::windows::fs::symlink_file(target, destination)
            };
        }
        #[cfg(not(any(unix, windows)))]
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "copying symbolic links is unsupported on this platform",
        ));
    }
    if source_type.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_recursively(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    fs::copy(source, destination).map(|_| ())
}

/// Cleanup only: a legacy folder that cannot be removed holds no data, so failures are ignored.
fn remove_empty_directory(path: &Path) {
    let _ = fs::remove_dir(path);
}

pub fn user_home_dir() -> PathBuf {
    nonempty_env_path("HOME")
        .or_else(|| nonempty_env_path("USERPROFILE"))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(target_os = "windows"))]
fn xdg_base(variable: &str, home_dir: &Path, fallback: &str) -> PathBuf {
    nonempty_env_path(variable).unwrap_or_else(|| home_dir.join(fallback))
}

fn nonempty_env_path(variable: &str) -> Option<PathBuf> {
    env::var_os(variable).and_then(|value| absolute_nonempty_path(&value))
}

fn absolute_nonempty_path(value: &OsStr) -> Option<PathBuf> {
    (!value.is_empty())
        .then(|| PathBuf::from(value))
        .filter(|path| path.is_absolute())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_resolves_the_same_xdg_layout_as_linux() {
        if nonempty_env_path("GHOSTEX_HOME").is_some() {
            return;
        }
        let home = user_home_dir();
        let paths = GhostexPaths::resolve();
        assert_eq!(
            paths.config_dir,
            xdg_base("XDG_CONFIG_HOME", &home, ".config").join("ghostex")
        );
        assert_eq!(
            paths.data_dir,
            xdg_base("XDG_DATA_HOME", &home, ".local/share").join("ghostex")
        );
        assert_eq!(
            paths.state_dir,
            xdg_base("XDG_STATE_HOME", &home, ".local/state").join("ghostex")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn migrates_the_previous_macos_library_layout_without_overwriting() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let test_root = env::temp_dir().join(format!(
            "ghostex-paths-macos-migration-{}-{unique}",
            std::process::id()
        ));
        let home = test_root.join("home");
        let config_dir = home.join(".config/ghostex");
        let data_dir = home.join(".local/share/ghostex");
        let state_dir = home.join(".local/state/ghostex");
        let cache_dir = home.join(".cache/ghostex");
        let legacy_app_support = home.join("Library/Application Support/Ghostex");

        fs::create_dir_all(legacy_app_support.join("Config/gxserver")).expect("old config");
        fs::create_dir_all(legacy_app_support.join("Data/hooks")).expect("old data");
        fs::create_dir_all(legacy_app_support.join("State/gxserver")).expect("old state");
        fs::create_dir_all(legacy_app_support.join("components/cef")).expect("old components");
        fs::create_dir_all(config_dir.join("gxserver")).expect("new config");
        fs::write(
            legacy_app_support.join("Config/gxserver/config.json"),
            b"old",
        )
        .expect("old config file");
        fs::write(config_dir.join("gxserver/config.json"), b"new").expect("new config file");
        fs::write(
            legacy_app_support.join("Config/gxserver/legacy-client.json"),
            b"legacy client",
        )
        .expect("non-conflicting nested config file");
        fs::write(legacy_app_support.join("Data/hooks/notify"), b"hook").expect("old hook");
        fs::write(legacy_app_support.join("State/gxserver/state.db"), b"state")
            .expect("old state file");
        fs::write(
            legacy_app_support.join("components/cef/payload"),
            b"component",
        )
        .expect("old component file");

        let paths = GhostexPaths {
            cache_dir,
            config_dir: config_dir.clone(),
            data_dir: data_dir.clone(),
            home_dir: home.clone(),
            legacy_dir: home.join(".ghostex"),
            logs_dir: state_dir.join("logs"),
            runtime_dir: state_dir.join("runtime"),
            state_dir: state_dir.clone(),
        };
        let mut skipped = Vec::new();
        paths.migrate_legacy_macos_layout(&mut skipped);
        assert!(skipped.is_empty(), "migrate old macOS layout: {skipped:?}");

        assert_eq!(
            fs::read(config_dir.join("gxserver/config.json")).unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(config_dir.join("gxserver/legacy-client.json")).unwrap(),
            b"legacy client"
        );
        assert_eq!(fs::read(data_dir.join("hooks/notify")).unwrap(), b"hook");
        assert_eq!(
            fs::read(state_dir.join("gxserver/state.db")).unwrap(),
            b"state"
        );
        assert_eq!(
            fs::read(data_dir.join("components/cef/payload")).unwrap(),
            b"component"
        );
        assert_eq!(
            fs::read(data_dir.join("legacy/macos-platform-storage/Config/gxserver/config.json"))
                .unwrap(),
            b"old"
        );

        fs::remove_dir_all(&test_root).expect("remove temp tree");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn public_migration_combines_both_legacy_layouts_and_is_idempotent() {
        if nonempty_env_path("GHOSTEX_HOME").is_some() {
            return;
        }
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let test_root = env::temp_dir().join(format!(
            "ghostex-paths-complete-migration-{}-{unique}",
            std::process::id()
        ));
        let home = test_root.join("home");
        let config_dir = home.join(".config/ghostex");
        let data_dir = home.join(".local/share/ghostex");
        let state_dir = home.join(".local/state/ghostex");
        let cache_dir = home.join(".cache/ghostex");
        let legacy_app_support = home.join("Library/Application Support/Ghostex");
        let legacy_dir = home.join(".ghostex");

        fs::create_dir_all(legacy_app_support.join("Config")).expect("library config");
        fs::create_dir_all(legacy_app_support.join("Data")).expect("library data");
        fs::create_dir_all(legacy_app_support.join("Runtime")).expect("library runtime");
        fs::create_dir_all(home.join("Library/Caches/Ghostex")).expect("library cache");
        fs::create_dir_all(home.join("Library/Logs/Ghostex")).expect("library logs");
        fs::create_dir_all(legacy_dir.join("i")).expect("dot images");
        fs::create_dir_all(legacy_dir.join("clients")).expect("dot clients");
        fs::create_dir_all(&config_dir).expect("new config");
        fs::write(legacy_app_support.join("Config/current.json"), b"old").expect("old conflict");
        fs::write(config_dir.join("current.json"), b"new").expect("new conflict");
        fs::write(legacy_app_support.join("Data/library-data"), b"library")
            .expect("library data file");
        fs::write(legacy_app_support.join("Runtime/socket"), b"runtime").expect("runtime file");
        fs::write(home.join("Library/Caches/Ghostex/cache"), b"cache").expect("cache file");
        fs::write(home.join("Library/Logs/Ghostex/log"), b"log").expect("log file");
        fs::write(legacy_dir.join("i/old.png"), b"image").expect("dot image");
        fs::write(legacy_dir.join("clients/client.json"), b"client").expect("dot client");

        let paths = GhostexPaths {
            cache_dir: cache_dir.clone(),
            config_dir: config_dir.clone(),
            data_dir: data_dir.clone(),
            home_dir: home.clone(),
            legacy_dir: legacy_dir.clone(),
            logs_dir: state_dir.join("logs"),
            runtime_dir: state_dir.join("runtime"),
            state_dir: state_dir.clone(),
        };
        let report = paths.migrate_legacy_layout();
        assert!(
            report.skipped.is_empty(),
            "public migration: {:?}",
            report.skipped
        );

        assert_eq!(fs::read(config_dir.join("current.json")).unwrap(), b"new");
        assert_eq!(fs::read(data_dir.join("library-data")).unwrap(), b"library");
        assert_eq!(fs::read(data_dir.join("i/old.png")).unwrap(), b"image");
        assert_eq!(
            fs::read(config_dir.join("clients/client.json")).unwrap(),
            b"client"
        );
        assert_eq!(fs::read(cache_dir.join("cache")).unwrap(), b"cache");
        assert_eq!(fs::read(state_dir.join("logs/log")).unwrap(), b"log");
        assert_eq!(
            fs::read(state_dir.join("runtime/socket")).unwrap(),
            b"runtime"
        );
        assert_eq!(
            fs::read(data_dir.join("legacy/macos-platform-storage/Config/current.json")).unwrap(),
            b"old"
        );
        assert!(paths.migration_marker().is_file());
        assert_eq!(
            fs::read_link(legacy_dir.join("i")).unwrap(),
            paths.images_dir()
        );
        assert_eq!(
            fs::read_link(legacy_dir.join("gxserver")).unwrap(),
            paths.gxserver_state_dir()
        );
        assert_eq!(
            fs::read_link(legacy_dir.join("logs")).unwrap(),
            paths.logs_dir
        );

        fs::write(legacy_dir.join("after-marker"), b"leave in place")
            .expect("post-migration legacy file");
        let report = paths.migrate_legacy_layout();
        assert!(
            report.skipped.is_empty(),
            "idempotent migration: {:?}",
            report.skipped
        );
        assert_eq!(
            fs::read(legacy_dir.join("after-marker")).unwrap(),
            b"leave in place"
        );

        fs::remove_dir_all(&test_root).expect("remove temp tree");
    }

    #[cfg(unix)]
    #[test]
    fn direct_legacy_upgrade_moves_the_complete_gxserver_directory() {
        if nonempty_env_path("GHOSTEX_HOME").is_some() {
            return;
        }
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let test_root = env::temp_dir().join(format!(
            "ghostex-paths-direct-gxserver-upgrade-{}-{unique}",
            std::process::id()
        ));
        let home = test_root.join("home");
        let state_dir = home.join(".local/state/ghostex");
        let paths = GhostexPaths {
            cache_dir: home.join(".cache/ghostex"),
            config_dir: home.join(".config/ghostex"),
            data_dir: home.join(".local/share/ghostex"),
            home_dir: home.clone(),
            legacy_dir: home.join(".ghostex"),
            logs_dir: state_dir.join("logs"),
            runtime_dir: state_dir.join("runtime"),
            state_dir: state_dir.clone(),
        };
        let legacy_gxserver = paths.legacy_dir.join("gxserver");
        fs::create_dir_all(legacy_gxserver.join("package/bin")).expect("legacy package");
        fs::write(legacy_gxserver.join("state.db"), b"legacy database").expect("legacy database");
        fs::write(legacy_gxserver.join("state.db-wal"), b"legacy wal").expect("legacy wal");
        fs::write(legacy_gxserver.join("config.json"), b"legacy config").expect("legacy config");
        fs::write(
            legacy_gxserver.join("package/bin/gxserver"),
            b"legacy package",
        )
        .expect("legacy package file");

        let report = paths.migrate_legacy_layout();
        assert!(
            report.skipped.is_empty(),
            "direct migration: {:?}",
            report.skipped
        );

        assert_eq!(
            fs::read_link(&legacy_gxserver).unwrap(),
            paths.gxserver_state_dir()
        );
        assert_eq!(
            fs::read(paths.gxserver_state_dir().join("state.db")).unwrap(),
            b"legacy database"
        );
        assert_eq!(
            fs::read(paths.gxserver_state_dir().join("state.db-wal")).unwrap(),
            b"legacy wal"
        );
        assert_eq!(
            fs::read(paths.gxserver_config_dir().join("config.json")).unwrap(),
            b"legacy config"
        );
        assert_eq!(
            fs::read(paths.gxserver_data_dir().join("package/bin/gxserver")).unwrap(),
            b"legacy package"
        );
        assert_eq!(
            fs::read_link(paths.gxserver_state_dir().join("package")).unwrap(),
            paths.gxserver_data_dir().join("package")
        );

        fs::remove_dir_all(&test_root).expect("remove temp tree");
    }

    #[cfg(unix)]
    #[test]
    fn v4_repairs_v3_users_with_a_legacy_gxserver_bridge() {
        if nonempty_env_path("GHOSTEX_HOME").is_some() {
            return;
        }
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let test_root = env::temp_dir().join(format!(
            "ghostex-paths-v3-upgrade-{}-{unique}",
            std::process::id()
        ));
        let home = test_root.join("home");
        let state_dir = home.join(".local/state/ghostex");
        let paths = GhostexPaths {
            cache_dir: home.join(".cache/ghostex"),
            config_dir: home.join(".config/ghostex"),
            data_dir: home.join(".local/share/ghostex"),
            home_dir: home.clone(),
            legacy_dir: home.join(".ghostex"),
            logs_dir: state_dir.join("logs"),
            runtime_dir: state_dir.join("runtime"),
            state_dir: state_dir.clone(),
        };
        fs::create_dir_all(paths.gxserver_state_dir()).expect("migrated gxserver state");
        fs::create_dir_all(state_dir.join("migrations")).expect("migration markers");
        fs::write(
            paths.gxserver_state_dir().join("state.db"),
            b"migrated database",
        )
        .expect("migrated database");
        fs::create_dir_all(paths.legacy_dir.join("gxserver")).expect("residual legacy gxserver");
        fs::write(
            paths.legacy_dir.join("gxserver/state.db"),
            b"residual legacy database",
        )
        .expect("residual legacy database");
        fs::create_dir_all(paths.gxserver_config_dir()).expect("current gxserver config");
        fs::write(
            paths.gxserver_config_dir().join("config.json"),
            b"current config",
        )
        .expect("current config");
        std::os::unix::fs::symlink(
            home.join("unrelated-config.json"),
            paths.legacy_dir.join("gxserver/config.json"),
        )
        .expect("wrong legacy config link");
        fs::write(
            state_dir.join("migrations/legacy-storage-v3.complete"),
            b"Ghostex XDG storage migration v3\n",
        )
        .expect("v3 marker");

        let report = paths.migrate_legacy_layout();
        assert!(
            report.skipped.is_empty(),
            "v4 repair migration: {:?}",
            report.skipped
        );

        assert_eq!(
            fs::read(paths.legacy_dir.join("gxserver/state.db")).unwrap(),
            b"migrated database"
        );
        assert_eq!(
            fs::read_link(paths.legacy_dir.join("gxserver")).unwrap(),
            paths.gxserver_state_dir()
        );
        assert_eq!(
            fs::read_link(paths.gxserver_state_dir().join("config.json")).unwrap(),
            paths.gxserver_config_dir().join("config.json")
        );
        assert_eq!(
            fs::read(
                paths
                    .data_dir
                    .join("legacy/gxserver-compatibility-residuals/legacy-gxserver-root/state.db")
            )
            .unwrap(),
            b"residual legacy database"
        );
        assert_eq!(
            fs::read_link(
                paths
                    .data_dir
                    .join("legacy/gxserver-compatibility-residuals/config.json")
            )
            .unwrap(),
            home.join("unrelated-config.json")
        );
        assert!(paths.migration_marker().is_file());

        fs::remove_dir_all(&test_root).expect("remove temp tree");
    }

    #[cfg(unix)]
    #[test]
    fn v4_replaces_and_archives_a_wrong_legacy_gxserver_symlink() {
        if nonempty_env_path("GHOSTEX_HOME").is_some() {
            return;
        }
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let test_root = env::temp_dir().join(format!(
            "ghostex-paths-v3-wrong-link-{}-{unique}",
            std::process::id()
        ));
        let home = test_root.join("home");
        let state_dir = home.join(".local/state/ghostex");
        let paths = GhostexPaths {
            cache_dir: home.join(".cache/ghostex"),
            config_dir: home.join(".config/ghostex"),
            data_dir: home.join(".local/share/ghostex"),
            home_dir: home.clone(),
            legacy_dir: home.join(".ghostex"),
            logs_dir: state_dir.join("logs"),
            runtime_dir: state_dir.join("runtime"),
            state_dir: state_dir.clone(),
        };
        let unrelated = home.join("unrelated-gxserver");
        fs::create_dir_all(&unrelated).expect("unrelated directory");
        fs::write(unrelated.join("must-stay"), b"untouched").expect("unrelated file");
        fs::create_dir_all(&paths.legacy_dir).expect("legacy parent");
        std::os::unix::fs::symlink(&unrelated, paths.legacy_dir.join("gxserver"))
            .expect("wrong gxserver link");
        fs::create_dir_all(state_dir.join("migrations")).expect("migration markers");
        fs::write(
            state_dir.join("migrations/legacy-storage-v3.complete"),
            b"Ghostex XDG storage migration v3\n",
        )
        .expect("v3 marker");

        let report = paths.migrate_legacy_layout();
        assert!(
            report.skipped.is_empty(),
            "v4 repair migration: {:?}",
            report.skipped
        );

        assert_eq!(
            fs::read_link(paths.legacy_dir.join("gxserver")).unwrap(),
            paths.gxserver_state_dir()
        );
        assert_eq!(fs::read(unrelated.join("must-stay")).unwrap(), b"untouched");
        assert_eq!(
            fs::read_link(
                paths
                    .data_dir
                    .join("legacy/gxserver-compatibility-residuals/legacy-gxserver-root")
            )
            .unwrap(),
            unrelated
        );

        fs::remove_dir_all(&test_root).expect("remove temp tree");
    }

    #[test]
    fn environment_paths_must_be_nonempty_and_absolute() {
        assert_eq!(absolute_nonempty_path(OsStr::new("")), None);
        assert_eq!(absolute_nonempty_path(OsStr::new("relative/path")), None);
        assert_eq!(
            absolute_nonempty_path(OsStr::new("/absolute/path")),
            Some(PathBuf::from("/absolute/path"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn directory_merge_does_not_follow_conflicting_symlinks() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let test_root = env::temp_dir().join(format!(
            "ghostex-paths-symlink-migration-{}-{unique}",
            std::process::id()
        ));
        let source = test_root.join("source");
        let destination = test_root.join("destination");
        let external = test_root.join("external");
        fs::create_dir_all(&source).expect("source");
        fs::create_dir_all(destination.join("linked")).expect("destination collision");
        fs::create_dir_all(&external).expect("external");
        fs::write(external.join("outside"), b"untouched").expect("external file");
        std::os::unix::fs::symlink(&external, source.join("linked")).expect("legacy symlink");

        let mut skipped = Vec::new();
        migrate_directory_contents(&mut skipped, &source, &destination);
        assert!(skipped.is_empty(), "safe merge: {skipped:?}");

        assert!(fs::symlink_metadata(source.join("linked"))
            .expect("symlink remains")
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(external.join("outside")).unwrap(), b"untouched");
        assert!(!destination.join("linked/outside").exists());

        fs::remove_dir_all(&test_root).expect("remove temp tree");
    }

    #[cfg(unix)]
    #[test]
    fn one_unmovable_legacy_item_does_not_fail_the_migration() {
        use std::os::unix::fs::PermissionsExt;
        if nonempty_env_path("GHOSTEX_HOME").is_some() {
            return;
        }
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let test_root = env::temp_dir().join(format!(
            "ghostex-paths-unmovable-item-{}-{unique}",
            std::process::id()
        ));
        let home = test_root.join("home");
        let state_dir = home.join(".local/state/ghostex");
        let data_dir = home.join(".local/share/ghostex");
        let legacy_dir = home.join(".ghostex");
        fs::create_dir_all(legacy_dir.join("hooks")).expect("dot hooks");
        fs::create_dir_all(legacy_dir.join("clients")).expect("dot clients");
        fs::write(legacy_dir.join("hooks/notify"), b"hook").expect("dot hook");
        fs::write(legacy_dir.join("clients/client.json"), b"client").expect("dot client");
        // Entries cannot leave a folder the user cannot write to.
        let locked = legacy_dir.join("clients");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("lock clients");
        if fs::write(locked.join("probe"), b"").is_ok() {
            // Running as root: permissions do not block anything.
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
            fs::remove_dir_all(&test_root).expect("remove temp tree");
            return;
        }

        let paths = GhostexPaths {
            cache_dir: home.join(".cache/ghostex"),
            config_dir: home.join(".config/ghostex"),
            data_dir: data_dir.clone(),
            home_dir: home.clone(),
            legacy_dir: legacy_dir.clone(),
            logs_dir: state_dir.join("logs"),
            runtime_dir: state_dir.join("runtime"),
            state_dir: state_dir.clone(),
        };
        let report = paths.migrate_legacy_layout();

        assert_eq!(fs::read(data_dir.join("hooks/notify")).unwrap(), b"hook");
        assert_eq!(fs::read(locked.join("client.json")).unwrap(), b"client");
        assert!(report
            .skipped
            .iter()
            .any(|item| item.path == locked.join("client.json")));
        assert!(report.warning().is_some());
        assert!(!paths.migration_marker().exists());

        // The next start retries quietly while nothing changed.
        let repeated = paths.migrate_legacy_layout();
        assert!(!repeated.skipped.is_empty());
        assert!(repeated.warning().is_none());

        // Once the folder is writable again the item moves and the migration completes.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock clients");
        let fixed = paths.migrate_legacy_layout();
        assert!(fixed.skipped.is_empty(), "after fix: {:?}", fixed.skipped);
        assert_eq!(
            fs::read(home.join(".config/ghostex/clients/client.json")).unwrap(),
            b"client"
        );
        assert!(paths.migration_marker().exists());

        fs::remove_dir_all(&test_root).expect("remove temp tree");
    }
}
