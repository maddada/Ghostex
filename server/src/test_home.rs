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

extern "C" fn isolate_ghostex_home() {
    let home = std::env::temp_dir().join(format!("gxserver-lib-tests-{}", std::process::id()));
    if std::fs::create_dir_all(&home).is_ok() {
        // Runs before main, while the process has one thread.
        std::env::set_var("GHOSTEX_HOME", &home);
    }
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
