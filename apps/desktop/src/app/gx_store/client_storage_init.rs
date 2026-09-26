//! Opens the client-storage database once, at process start, before anything reads it.
//!
//! CDXC:Settings 2026-09-25 WHY:
//! The tables, the browser-era import and the catalog migrations used to run only inside the
//! QuickJS runtime (`ServiceRuntime::new` and `initializeClientStorage`), so the Rust storage doors
//! read a file the runtime had to create first and a fresh install without the runtime had no
//! tables at all. They run here now, synchronously and before the window opens, the way the
//! runtime's own start used to block its first frame. A failure is written unconditionally and the
//! app goes on: every door reports its own read or write failure, and the next start retries,
//! because every step rolled back.
//!
//! SEE-ALSO: packages/client-storage-native/src/storage_init.rs, apps/desktop/src/main.rs (the caller).

use serde_json::json;

use crate::support_logs::{self, GpuiSupportLog};

/// Creates the tables, imports the browser-era CEF profile once, and runs the migrations.
pub(crate) fn initialize_client_storage_at_start() {
    let profile = std::env::var_os("GHOSTEX_GPUI_CEF_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::shared_settings::ghostex_storage_paths().cef_cache_dir())
        .join("Default");
    let now_ms = super::host::now_ms() as i64;
    match ghostex_client_storage::initialize_client_storage(
        &super::sidebar_ui_storage::client_storage_path(),
        Some(&profile),
        now_ms,
    ) {
        Ok(report) => {
            if report.browser_import_ran
                || report.preferences_migrated > 0
                || report.legacy_rows_migrated > 0
                || report.legacy_rows_unmigrated > 0
            {
                support_logs::append(
                    GpuiSupportLog::HostLifecycle,
                    "gpui.clientStorage.migrated",
                    json!({
                        "browserImport": report.browser_import_ran,
                        "preferencesMigrated": report.preferences_migrated,
                        "legacyRowsMigrated": report.legacy_rows_migrated,
                        "legacyRowsUnmigrated": report.legacy_rows_unmigrated,
                    }),
                );
            }
        }
        Err(error) => support_logs::append(
            GpuiSupportLog::CrashReports,
            "gpui.clientStorage.startFailed",
            json!({ "error": error.to_string().lines().next().unwrap_or_default() }),
        ),
    }
}
