//! Every gxserver lib test runs with Ghostex's folders in a fresh temporary home.
//!
//! CDXC:Tests 2026-11-09 WHY: about sixty places resolve Ghostex's folders on their own
//! (`get_gxserver_paths(None)`, `GhostexPaths::resolve()`), so a test that reached one read or
//! wrote this Mac's real state: the user's saved Autopilot (answering approvals) wrote every
//! approval row already read and failed the OMP wait and push tests, and test runs left files in
//! the real `gxserver/open-questions` folder. Before any test starts, this points `GHOSTEX_HOME`
//! (the isolated-profile variable `GhostexPaths::resolve` reads first) at a new folder of its own,
//! so none of them can reach the real one. Folders under the real home directory that are not
//! Ghostex's (an agent's own `~/.claude`, `~/.codex`) are not covered.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static HOME: OnceLock<PathBuf> = OnceLock::new();

/// This test process's temporary Ghostex home, removed when the process exits.
pub(crate) fn path() -> &'static Path {
    HOME.get_or_init(|| {
        std::env::temp_dir().join(format!("gxserver-lib-tests-{}", std::process::id()))
    })
}

extern "C" fn isolate_ghostex_home() {
    let home = path();
    if std::fs::create_dir_all(home).is_ok() {
        // Runs before main, while the process has one thread.
        std::env::set_var("GHOSTEX_HOME", home);
        // CDXC:Tests 2026-11-26 WHY: the home was never removed, and a runner that starts one process per test left one folder per test (4,328 of them, 36 MB, in the user's temp folder; leak audit row 13). A static never drops, so the folder goes when the process exits instead, whether its tests passed or failed.
        // SAFETY: `remove_home` is a plain `extern "C" fn` with no arguments, as `atexit` requires.
        unsafe {
            libc::atexit(remove_home);
        }
    }
}

extern "C" fn remove_home() {
    let _ = std::fs::remove_dir_all(path());
}

/// Run by the loader before main, as a C++ static constructor would be.
#[used]
#[cfg_attr(target_vendor = "apple", link_section = "__DATA,__mod_init_func")]
#[cfg_attr(
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    link_section = ".init_array"
)]
#[cfg_attr(windows, link_section = ".CRT$XCU")]
static ISOLATE_GHOSTEX_HOME: extern "C" fn() = isolate_ghostex_home;
