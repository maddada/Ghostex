// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.
use std::cell::RefCell;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;
use futures::StreamExt as _;
use gpui::{
    App, AppContext as _, Bounds, InteractiveElement as _, ParentElement as _, Styled as _, Window, WindowBounds, prelude::FluentBuilder as _, px, size,
};
use gpui_component::{
    WindowExt,
    notification::Notification,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_collect_native_resource_process_tree(
    seeds: &[GpuiNativeResourceProcess],
    children_by_parent: &HashMap<u32, Vec<GpuiNativeResourceProcess>>,
) -> Vec<GpuiNativeResourceProcess> {
    gpui_collect_native_resource_process_tree_bounded(seeds, children_by_parent, &|_| false)
}

/// Walk a process tree from `seeds`, skipping every descendant that another
/// Resources row already owns. Boundary processes are neither counted nor
/// descended into, so one row can never absorb another row's subtree.
pub(crate) fn gpui_collect_native_resource_process_tree_bounded(
    seeds: &[GpuiNativeResourceProcess],
    children_by_parent: &HashMap<u32, Vec<GpuiNativeResourceProcess>>,
    is_boundary: &dyn Fn(&GpuiNativeResourceProcess) -> bool,
) -> Vec<GpuiNativeResourceProcess> {
    let mut collected = HashMap::<u32, GpuiNativeResourceProcess>::new();
    let mut queue = seeds.to_vec();
    while let Some(process) = queue.pop() {
        if collected.contains_key(&process.pid) {
            continue;
        }
        queue.extend(
            children_by_parent
                .get(&process.pid)
                .into_iter()
                .flatten()
                .filter(|child| !is_boundary(child))
                .cloned(),
        );
        collected.insert(process.pid, process);
    }
    collected.into_values().collect()
}

pub(crate) fn gpui_sum_native_resource_processes(processes: &[GpuiNativeResourceProcess]) -> (f64, f64) {
    processes.iter().fold((0.0, 0.0), |(cpu, memory), process| {
        (cpu + process.cpu, memory + process.memory_mb)
    })
}

pub(crate) fn gpui_native_resource_is_app_bundle_process(process: &GpuiNativeResourceProcess) -> bool {
    let command = process.command.to_ascii_lowercase();
    command.contains("/ghostex.app/contents/")
        || command.contains("/ghostex-dev.app/contents/")
        || (cfg!(target_os = "windows")
            && [
                "ghostex.exe",
                "ghostex-gpui.exe",
                "ghostex-gpui-cef-helper.exe",
            ]
            .iter()
            .any(|executable| command.contains(executable)))
}

/// True for the app's own executables: the Ghostex binary and its CEF helper
/// processes. These are the app itself, never a user-owned server or runtime.
pub(crate) fn gpui_native_resource_is_app_shell_process(process: &GpuiNativeResourceProcess) -> bool {
    let command = process.command.to_ascii_lowercase();
    [
        "/ghostex.app/contents/macos/",
        "/ghostex.app/contents/frameworks/",
        "/ghostex-dev.app/contents/macos/",
        "/ghostex-dev.app/contents/frameworks/",
    ]
    .iter()
    .any(|marker| command.contains(marker))
        || (cfg!(target_os = "windows")
            && [
                "ghostex.exe",
                "ghostex-gpui.exe",
                "ghostex-gpui-cef-helper.exe",
            ]
            .iter()
            .any(|executable| command.contains(executable)))
}

pub(crate) fn gpui_native_resource_is_ghostex_owned_process(process: &GpuiNativeResourceProcess) -> bool {
    let command = process.command.to_ascii_lowercase();
    gpui_native_resource_is_app_bundle_process(process)
        || command.contains("/.ghostex/")
        || command.contains("/.ghostex-dev/")
        || command.contains("ghostex_")
        || command.contains("/resources/web/bin/zmx")
}

pub(crate) fn gpui_native_resource_is_user_runtime_process(process: &GpuiNativeResourceProcess) -> bool {
    let command = process.command.to_ascii_lowercase();
    [
        "zmx",
        "codex",
        "code-server",
        "computer-use",
        "chrome-devtools",
        "devtools",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(crate) fn gpui_native_resource_is_ghostex_browser_process(process: &GpuiNativeResourceProcess) -> bool {
    let command = process.command.to_ascii_lowercase();
    (command.contains("--type=renderer")
        || command.contains("--type=gpu-process")
        || command.contains("--type=utility"))
        && (command.contains("ghostex") || command.contains("/.ghostex/cef"))
}

pub(crate) fn gpui_native_resource_process_name(process: &GpuiNativeResourceProcess) -> String {
    process
        .command
        .split_whitespace()
        .next()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("process")
        .to_string()
}

pub(crate) fn gpui_native_resource_child_rows(
    processes: &[GpuiNativeResourceProcess],
    excluded_pid: Option<u32>,
) -> Vec<GpuiNativeResourceChild> {
    processes
        .iter()
        .filter(|process| Some(process.pid) != excluded_pid)
        .take(8)
        .map(|process| GpuiNativeResourceChild {
            cpu: process.cpu,
            label: gpui_native_resource_process_name(process),
            memory_mb: process.memory_mb,
            pid: process.system_pid,
        })
        .collect()
}

pub(crate) fn format_gpui_resource_cpu(cpu: f64) -> String {
    format!("CPU {:.0}%", cpu.max(0.0))
}

pub(crate) fn format_gpui_resource_memory(memory_mb: f64) -> String {
    if memory_mb >= 1024.0 {
        format!("RAM {:.1} GB", memory_mb / 1024.0)
    } else {
        format!("RAM {:.0} MB", memory_mb.max(0.0))
    }
}

pub(crate) fn find_app_bundle_root(path: &std::path::Path) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        if ancestor
            .extension()
            .is_some_and(|extension| extension == "app")
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn file_url(path: &std::path::Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

#[cfg(target_os = "windows")]
pub(crate) fn file_url(path: &std::path::Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    format!("file:///{}", normalized.trim_start_matches('/'))
}

pub(crate) const MANAGE_FILE_LIST_MAX_ENTRIES: usize = 1_200;
pub(crate) const MANAGE_FILE_LIST_MAX_DEPTH: usize = 8;
/*
CDXC:DocsRootRecursive 2026-08-09:
Mirrors `gxserver-rs/src/project_docs.rs`: a mounted Docs directory is a notes
tree, not a repo, so it gets its own far larger bounds. They are still bounds,
and hitting one labels that mount with the cap instead of returning a tree that
silently stopped.
*/
pub(crate) const MANAGE_DOCS_TREE_MAX_ENTRIES: usize = 20_000;
pub(crate) const MANAGE_DOCS_TREE_MAX_DEPTH: usize = 12;
pub(crate) const MANAGE_FILE_PREVIEW_MAX_BYTES: u64 = 2_000_000;
pub(crate) const MANAGE_FILE_SAVE_MAX_BYTES: usize = 2_000_000;
pub(crate) const MANAGE_GIT_BASELINE_MAX_BYTES: usize = 1024 * 1024;
pub(crate) const MANAGE_REMOTE_RESOURCE_MAX_BYTES: usize = 12 * 1024 * 1024;
pub(crate) const MANAGE_SESSION_CONTEXT_MAX_BYTES: usize = 300_000;
pub(crate) static MANAGE_REMOTE_RESOURCE_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
pub(crate) const MANAGE_DOCS_RELATIVE_PATH: &str = "docs";
pub(crate) const MANAGE_BUILT_IN_DOCS_RELATIVE_PATHS: &[&str] =
    &[MANAGE_DOCS_RELATIVE_PATH, "artifacts", "ai"];
/*
CDXC:DocsRootAdditive 2026-08-09:
Mirrors `EXTRA_ROOT_MOUNT_SEGMENT` in `gxserver-rs/src/project_docs.rs`: the
reserved first path segment that addresses the mounted Docs directory. Every
other Docs path is project-relative, so one relative path can only ever mean one
root and no read, save, rename, delete, move, or reveal can resolve out of the
root it was addressed to.
*/
pub(crate) const MANAGE_DOCS_EXTRA_ROOT_MOUNT_SEGMENT: &str = ".ghostex-docs-root";
pub(crate) const MANAGE_ANNOTATIONS_SIDECAR_RELATIVE_PATH: &str = ".ghostex/manage-annotations.json";
pub(crate) const MANAGE_ROOT_ARTIFACT_FILE_EXTENSIONS: &[&str] = &[
    "excalidraw",
    "htm",
    "html",
    "markdown",
    "md",
    "mdown",
    "mkdn",
];
pub(crate) const PROJECT_BOARD_CLIPBOARD_IMAGE_MAX_BYTES: usize = 12 * 1024 * 1024;
pub(crate) const PROJECT_BOARD_IMAGE_PREVIEW_MAX_BYTES: usize = 12 * 1024 * 1024;
pub(crate) const GPUI_GXSERVER_LOCAL_API_HOST: &str = "127.0.0.1";
pub(crate) const GPUI_GXSERVER_LOCAL_API_PORT: u16 = 58_744;
pub(crate) const GPUI_GXSERVER_PRODUCT: &str = "gxserver";
pub(crate) const GPUI_GXSERVER_PROTOCOL_HEADER: &str = "x-gxserver-protocol-version";
pub(crate) const GPUI_GXSERVER_PROTOCOL_VERSION: u64 = 1;
// CDXC:PortlessSettingsDisabled 2026-07-25: Keep the complete GPUI Portless
// implementation available for later, while gating all current runtime and UI
// exposure behind one intentionally disabled product switch.
pub(crate) const GPUI_PORTLESS_APP_INTEGRATION_ENABLED: bool = false;
pub(crate) const GPUI_SIDEBAR_GXSERVER_CLIENT_ID: &str = "ghostex-gpui-sidebar";
pub(crate) const GPUI_REMOTE_GXSERVER_TOKEN_START_MARKER: &str = "__GHOSTEX_REMOTE_TOKEN_START__";
pub(crate) const GPUI_REMOTE_GXSERVER_TOKEN_END_MARKER: &str = "__GHOSTEX_REMOTE_TOKEN_END__";
pub(crate) const GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_START_MARKER: &str =
    "__GHOSTEX_REMOTE_BUILD_IDENTITY_START__";
pub(crate) const GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_END_MARKER: &str =
    "__GHOSTEX_REMOTE_BUILD_IDENTITY_END__";
pub(crate) const GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER: &str =
    "__GHOSTEX_REMOTE_GXSERVER_VERSION_START__";
pub(crate) const GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_END_MARKER: &str =
    "__GHOSTEX_REMOTE_GXSERVER_VERSION_END__";
pub(crate) const GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_MAX_LENGTH: usize = 40;
// The install-state probe's own "no gxserver here" exit code. Any other
// non-zero code means the remote login shell never ran the probe script.
pub(crate) const GPUI_REMOTE_GXSERVER_NOT_INSTALLED_EXIT_CODE: i32 = 3;
pub(crate) const GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT: Duration = Duration::from_secs(18);
pub(crate) const GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const GPUI_REMOTE_GXSERVER_ARCHIVE_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const GPUI_REMOTE_GXSERVER_UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const GPUI_REMOTE_GXSERVER_INSTALL_TIMEOUT: Duration = Duration::from_secs(45);
// gxserver advertises this in /api/health/server once it accepts the
// `code-server` prompt-editor selector on session create and attach operations.
pub(crate) const GPUI_GXSERVER_CODE_SERVER_PROMPT_EDITOR_CAPABILITY: &str = "codeServerPromptEditor";
pub(crate) const GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT: Duration = Duration::from_secs(1);
pub(crate) const GPUI_REMOTE_GXSERVER_HEALTH_DEADLINE: Duration = Duration::from_secs(7);
pub(crate) const GPUI_REMOTE_GXSERVER_WATCHDOG_INTERVAL: Duration = Duration::from_secs(15);
pub(crate) const GPUI_REMOTE_GXSERVER_WATCHDOG_FAILURE_THRESHOLD: u8 = 2;
pub(crate) const GPUI_REMOTE_GXSERVER_TUNNEL_STARTUP_DELAY: Duration = Duration::from_millis(350);
pub(crate) const GPUI_REMOTE_GXSERVER_TUNNEL_RETRY_DELAY: Duration = Duration::from_millis(200);
pub(crate) const GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MIN: u16 = 42_000;
pub(crate) const GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MAX: u16 = 58_999;
pub(crate) const GPUI_REMOTE_GXSERVER_TUNNEL_ATTEMPTS: usize = 8;
pub(crate) const GPUI_REMOTE_GXSERVER_PARAMS_MAX_BYTES: usize = 64 * 1024;
pub(crate) const GPUI_REMOTE_GXSERVER_SIDEBAR_REQUEST_TIMEOUT_MIN_MS: u64 = 1_000;
pub(crate) const GPUI_REMOTE_GXSERVER_SIDEBAR_REQUEST_TIMEOUT_MAX_MS: u64 = 130_000;
pub(crate) const GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_ATTEMPTS: usize = 4;
pub(crate) const GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_FRAME_MAX_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_READ_TIMEOUT: Duration = Duration::from_millis(900);
pub(crate) const GPUI_REMOTE_GXSERVER_PRESENTATION_HEALTH_INTERVAL: Duration = Duration::from_secs(15);
pub(crate) const GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_RECONNECT_DELAY: Duration =
    Duration::from_millis(700);
pub(crate) const GPUI_REMOTE_REPOSITORY_CLONE_POLL_INTERVAL: Duration = Duration::from_millis(700);
pub(crate) const GPUI_REMOTE_REPOSITORY_CLONE_PREVIEW_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const GPUI_REMOTE_REPOSITORY_CLONE_START_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const GPUI_REMOTE_REPOSITORY_CLONE_JOB_TIMEOUT: Duration = Duration::from_secs(15);
/*
 * CDXC:AddProject 2026-07-30:
 * Add-project timeouts. Registering a project right after a remote reconnect
 * has been measured at ~19s, which the previous 20s add timeout raced: the add
 * landed on the machine while the dialog reported failure. Adds and clone
 * starts get a full minute, and every waiter on the JS side uses the same
 * budget so neither end can give up while the other is still working.
 */
pub(crate) const GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const GPUI_ADD_PROJECT_DIALOG_BROWSE_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const GPUI_ADD_PROJECT_DIALOG_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const GPUI_ADD_PROJECT_DIALOG_LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const GPUI_ADD_PROJECT_DIALOG_JOB_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_INTERVAL: Duration = Duration::from_secs(5);
// CDXC:AddProject 2026-07-30: ~10 minutes of native follow-up for a remote clone
// whose dialog poll answer was lost, and an early exit once the tunnel stops
// answering at all.
pub(crate) const GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_POLLS: u32 = 120;
pub(crate) const GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_CONSECUTIVE_ERRORS: u32 = 3;
pub(crate) const GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID: &str = "local";
pub(crate) const GPUI_GHOSTTY_SETTINGS_DOCS_URL: &str = "https://ghostty.org/docs/config/reference";
pub(crate) const GPUI_MACOS_ACCESSIBILITY_PREFERENCES_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
pub(crate) const GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
pub(crate) const GPUI_MACOS_NOTIFICATION_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.Notifications-Settings.extension";
pub(crate) const GPUI_GHOSTEX_CLI_WRAPPER_MARKER: &str = "CDXC:CliInstall 2026-06-12-09:31";
pub(crate) const GPUI_GTE_INSTALL_ACTION_ID: &str = "installGte";
pub(crate) const GPUI_GTE_HOMEBREW_INSTALL_SCRIPT: &str = concat!(
    "if command -v brew >/dev/null 2>&1; then BREW=$(command -v brew); ",
    "elif [ -x /opt/homebrew/bin/brew ]; then BREW=/opt/homebrew/bin/brew; ",
    "elif [ -x /usr/local/bin/brew ]; then BREW=/usr/local/bin/brew; ",
    "else echo 'Homebrew was not found on PATH, /opt/homebrew/bin, or /usr/local/bin.' >&2; exit 127; fi; ",
    "\"$BREW\" install maddada/tap/gte"
);
pub(crate) const GPUI_GTE_INSTALL_SUCCESS_MESSAGE: &str = "gte installed from Homebrew.";
pub(crate) const GPUI_GTE_INSTALL_FAILURE_MESSAGE: &str =
    "gte install failed. Install Homebrew or run brew install maddada/tap/gte in a terminal.";
pub(crate) const GPUI_BUNDLED_GHOSTEX_AGENT_SKILL_NAMES: &[&str] = &[
    /*
    CDXC:CodexSessionMove 2026-06-26-13:47:
    GPUI packages the same app-bundled Codex session-move skill as the native sidebar so settings repair and install flows expose a consistent Ghostex skill set.
    */
    "ghostex-browser-use",
    "ghostex-embedded-browser-use",
    "ghostex-computer-use",
    "ghostex-agent-orchestration",
    "ghostex-fable-5.6-orchestration",
    "ghostex-find-prev-session",
    "ghostex-auto-rename-session",
    "ghostex-move-codex-session",
];

pub(crate) fn gpui_spawn_open_target_command(
    command: &str,
    base_args: &[&str],
    custom_args: &[String],
    project_path: &Path,
) -> Result<(), String> {
    let mut process = std::process::Command::new("/usr/bin/env");
    process.arg(command);
    for arg in base_args {
        process.arg(arg);
    }
    for arg in custom_args {
        process.arg(arg);
    }
    process
        .arg(project_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not launch the selected Open In target.".to_string())
}

pub(crate) struct GpuiDetectedOpenTargetAvailability {
    // Catalog-ordered, always contains "finder", never duplicates.
    pub(crate) available_target_ids: Vec<String>,
    pub(crate) resolved_commands: HashMap<String, String>,
    pub(crate) resolved_app_names: HashMap<String, String>,
}

pub(crate) const GPUI_OPEN_TARGET_DETECTION_TIMEOUT: Duration = Duration::from_secs(45);

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_keep_awake_caffeinate(
    duration_minutes: shared_settings::SharedKeepAwakeDurationMinutes,
    allow_display_sleep: bool,
) -> Result<std::process::Child, String> {
    /*
    CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
    GPUI Keep Awake must start macOS caffeinate directly with fixed argv and suppressed stdio. Use `-dis` for normal display+idle sleep prevention, `-is` when Settings allows display sleep, and add bounded `-t` seconds only for the 2-hour and 5-hour shared durations.
    */
    let mut command = std::process::Command::new("/usr/bin/caffeinate");
    command.arg(if allow_display_sleep { "-is" } else { "-dis" });
    if duration_minutes.minutes() > 0 {
        command
            .arg("-t")
            .arg((duration_minutes.minutes() * 60).to_string());
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "caffeinate spawn failed".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_open_target_app_name(app_name: &str, project_path: &Path) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("-a")
        .arg(app_name)
        .arg(project_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not launch the selected Open In target.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_spawn_open_target_app_name(_app_name: &str, _project_path: &Path) -> Result<(), String> {
    Err("Could not launch the selected Open In target.".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_os_open(target: &std::ffi::OsStr) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "macOS could not open the requested Ghostex Settings target.".to_string())
}

/// The OS file manager, named the way this platform's users name it.
#[cfg(target_os = "macos")]
pub(crate) const GPUI_FILE_MANAGER_NAME: &str = "Finder";
#[cfg(target_os = "windows")]
pub(crate) const GPUI_FILE_MANAGER_NAME: &str = "File Explorer";
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) const GPUI_FILE_MANAGER_NAME: &str = "your file manager";

#[cfg(target_os = "macos")]
pub(crate) fn gpui_reveal_path_in_finder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Select an item to reveal.".to_string());
    }
    std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| format!("Could not reveal that item in {GPUI_FILE_MANAGER_NAME}."))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_reveal_path_in_finder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Select an item to reveal.".to_string());
    }
    // Explorer's own selection syntax: one "/select,<path>" argument, no space.
    let mut selection = std::ffi::OsString::from("/select,");
    selection.push(path.as_os_str());
    std::process::Command::new("explorer.exe")
        .arg(selection)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| format!("Could not reveal that item in {GPUI_FILE_MANAGER_NAME}."))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn gpui_reveal_path_in_finder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Select an item to reveal.".to_string());
    }
    // No portable "select this item" verb here, so open the containing folder.
    let folder = if path.is_dir() {
        path
    } else {
        path.parent()
            .ok_or_else(|| "That item has no containing folder.".to_string())?
    };
    std::process::Command::new("xdg-open")
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| format!("Could not reveal that item in {GPUI_FILE_MANAGER_NAME}."))
}

/// A path's file name for toast copy; `None` when the path has no final component.
pub(crate) fn gpui_path_file_name_label(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn gpui_spawn_os_open(target: &std::ffi::OsStr) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "The OS opener is unavailable for this Ghostex Settings action.".to_string())
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_spawn_os_open(target: &std::ffi::OsStr) -> Result<(), String> {
    std::process::Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "The OS opener is unavailable for this Ghostex Settings action.".to_string())
}

pub(crate) const GPUI_AGENTS_HUB_MAX_FILE_BYTES: u64 = 128 * 1024;

#[cfg(target_os = "macos")]
pub(crate) fn gpui_native_app_shot_capture_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onNativeAppShotCaptured==='function'){{bridge.onNativeAppShotCaptured(payload);}}else{{const pending=Array.isArray(bridge.pendingNativeAppShots)?bridge.pendingNativeAppShots:[];pending.push(payload);bridge.pendingNativeAppShots=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_native_app_shot_prompt_result_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onNativeAppShotPromptResult==='function'){{bridge.onNativeAppShotPromptResult(payload);}}else{{const pending=Array.isArray(bridge.pendingNativeAppShotPromptResults)?bridge.pendingNativeAppShotPromptResults:[];pending.push(payload);bridge.pendingNativeAppShotPromptResults=pending;}}}})(); undefined;"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiMacOSNotificationAuthorizationStatus {
    Unsupported,
    Unknown,
    NotDetermined,
    Denied,
    Authorized,
    Provisional,
}

impl GpuiMacOSNotificationAuthorizationStatus {
    pub(crate) fn from_native_code(code: i32) -> Self {
        match code {
            -1 => Self::Unsupported,
            1 => Self::NotDetermined,
            2 => Self::Denied,
            3 => Self::Authorized,
            4 => Self::Provisional,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn authorization_status(self) -> &'static str {
        match self {
            Self::Unsupported => "unavailable",
            Self::Unknown => "unknown",
            Self::NotDetermined => "notDetermined",
            Self::Denied => "denied",
            Self::Authorized => "authorized",
            Self::Provisional => "provisional",
        }
    }

    pub(crate) fn available(self) -> bool {
        matches!(self, Self::Authorized | Self::Provisional)
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Authorized => "macOS allows Ghostex notification banners.",
            Self::Provisional => "macOS allows provisional Ghostex notification banners.",
            Self::NotDetermined => "macOS notification permission has not been decided yet.",
            Self::Denied => {
                "macOS is not allowing Ghostex notification banners. Use Open macOS Notification Settings to allow notifications."
            }
            Self::Unsupported => "macOS notification banners are not available in this GPUI build.",
            Self::Unknown => "GPUI could not read macOS notification permission status.",
        }
    }

    pub(crate) fn toast_level(self) -> &'static str {
        if self.available() {
            "success"
        } else {
            "warning"
        }
    }

    pub(crate) fn toast_title(self) -> &'static str {
        match self {
            Self::Authorized | Self::Provisional => "Notifications enabled",
            Self::Denied => "Notifications disabled",
            Self::NotDetermined => "Notification permission undecided",
            Self::Unsupported => "Notifications unavailable",
            Self::Unknown => "Notification status unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiMacOSNotificationDeliveryResult {
    Unsupported,
    Unknown,
    PermissionNotDetermined,
    PermissionDenied,
    Sent,
    Failed,
}

impl GpuiMacOSNotificationDeliveryResult {
    pub(crate) fn from_native_code(code: i32) -> Self {
        match code {
            -1 => Self::Unsupported,
            1 => Self::PermissionNotDetermined,
            2 => Self::PermissionDenied,
            3 => Self::Sent,
            4 => Self::Failed,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn permission_status(self) -> Option<GpuiMacOSNotificationAuthorizationStatus> {
        match self {
            Self::Unsupported => Some(GpuiMacOSNotificationAuthorizationStatus::Unsupported),
            Self::PermissionNotDetermined => {
                Some(GpuiMacOSNotificationAuthorizationStatus::NotDetermined)
            }
            Self::PermissionDenied => Some(GpuiMacOSNotificationAuthorizationStatus::Denied),
            Self::Unknown => Some(GpuiMacOSNotificationAuthorizationStatus::Unknown),
            Self::Sent | Self::Failed => None,
        }
    }
}

pub(crate) fn gpui_notification_permission_status_message(
    status: GpuiMacOSNotificationAuthorizationStatus,
) -> serde_json::Value {
    serde_json::json!({
        "authorizationStatus": status.authorization_status(),
        "available": status.available(),
        "generatedAt": gpui_status_generated_at(),
        "message": status.message(),
        "type": "notificationPermissionStatus",
    })
}

pub(crate) fn gpui_macos_notification_test_action_message(
    result: GpuiMacOSNotificationDeliveryResult,
    completion_sound_enabled: bool,
    played_sound: bool,
) -> (&'static str, bool) {
    match result {
        GpuiMacOSNotificationDeliveryResult::Sent => {
            if completion_sound_enabled && played_sound {
                (
                    "Played the completion sound and sent a macOS notification test.",
                    true,
                )
            } else if completion_sound_enabled {
                (
                    "Sent a macOS notification test. The completion sound preview reported a separate failure.",
                    true,
                )
            } else {
                ("Sent a macOS notification test.", true)
            }
        }
        GpuiMacOSNotificationDeliveryResult::PermissionDenied => {
            if completion_sound_enabled && played_sound {
                (
                    "Played the completion sound, but macOS is not allowing Ghostex notification banners. Use Open macOS Notification Settings to allow notifications.",
                    false,
                )
            } else {
                (
                    "macOS is not allowing Ghostex notification banners. Use Open macOS Notification Settings to allow notifications.",
                    false,
                )
            }
        }
        GpuiMacOSNotificationDeliveryResult::PermissionNotDetermined => (
            "macOS did not return a notification permission decision. Use the notification permission button or Open macOS Notification Settings.",
            false,
        ),
        GpuiMacOSNotificationDeliveryResult::Unsupported => (
            "macOS notification banners are not available in this GPUI build.",
            false,
        ),
        GpuiMacOSNotificationDeliveryResult::Failed => {
            ("GPUI could not send a macOS notification test.", false)
        }
        GpuiMacOSNotificationDeliveryResult::Unknown => (
            "GPUI could not determine whether the macOS notification test was sent.",
            false,
        ),
    }
}

pub(crate) fn gpui_sound_preview_status_message(ok: bool, message: &str) -> serde_json::Value {
    serde_json::json!({
        "generatedAt": gpui_status_generated_at(),
        "message": message,
        "ok": ok,
        "type": "soundPreviewStatus",
    })
}

pub(crate) fn gpui_ghostex_folder_stats_error_message(message: &str) -> serde_json::Value {
    serde_json::json!({
        "errorMessage": message,
        "folderPath": gpui_path_string(&shared_settings::ghostex_storage_paths().data_dir),
        "folders": [],
        "generatedAt": gpui_status_generated_at(),
        "totalBytes": 0,
        "type": "ghostexFolderStats",
    })
}

pub(crate) fn gpui_play_completion_sound(sound: &str) -> Result<(), String> {
    /*
    CDXC:GPUISettingsActionBridge 2026-06-24-11:59:
    GPUI sound preview may play only validated bundled completion sound filenames from the app bundle/sidebar resources or the repository media directory used by local GPUI runs. The command never accepts a path from React and falls back to an explicit unsupported status when the sound asset or platform playback command is unavailable.
    */
    let file_name = gpui_completion_sound_file_name(sound);
    let Some(sound_path) = gpui_completion_sound_path(file_name) else {
        return Err("GPUI could not find a bundled completion sound asset.".to_string());
    };
    gpui_spawn_completion_sound_player(&sound_path)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_request_macos_notification_permission() -> GpuiMacOSNotificationAuthorizationStatus {
    GpuiMacOSNotificationAuthorizationStatus::from_native_code(unsafe {
        GhostexGpuiRequestNotificationAuthorization()
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_request_macos_notification_permission() -> GpuiMacOSNotificationAuthorizationStatus {
    GpuiMacOSNotificationAuthorizationStatus::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult {
    GpuiMacOSNotificationDeliveryResult::from_native_code(unsafe {
        GhostexGpuiDeliverSettingsTestNotification()
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult {
    GpuiMacOSNotificationDeliveryResult::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_reduce_motion_enabled() -> bool {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    Read only the macOS Reduce Motion boolean from the AppKit shim. Unknown or unsupported native results intentionally mean "animation allowed" so GPUI does not add fake fallback state or persist accessibility preferences.
    */
    unsafe { GhostexGpuiAccessibilityDisplayShouldReduceMotion() == 1 }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_macos_reduce_motion_enabled() -> bool {
    false
}

pub(crate) fn gpui_macos_attention_notifications_enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("showMacOSAttentionNotifications")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_deliver_macos_session_attention_notification(
    candidate: GpuiSessionAttentionNotificationCandidate,
) -> GpuiMacOSNotificationDeliveryResult {
    let Ok(session_id) = std::ffi::CString::new(candidate.session_id.as_str()) else {
        return GpuiMacOSNotificationDeliveryResult::Failed;
    };
    let Ok(title) = std::ffi::CString::new(candidate.title.as_str()) else {
        return GpuiMacOSNotificationDeliveryResult::Failed;
    };
    let Ok(body) = std::ffi::CString::new(candidate.body.as_str()) else {
        return GpuiMacOSNotificationDeliveryResult::Failed;
    };
    let icon_data_url = match candidate.icon_data_url.as_deref() {
        Some(value) => match std::ffi::CString::new(value) {
            Ok(value) => Some(value),
            Err(_) => return GpuiMacOSNotificationDeliveryResult::Failed,
        },
        None => None,
    };
    GpuiMacOSNotificationDeliveryResult::from_native_code(unsafe {
        GhostexGpuiDeliverSessionAttentionNotification(
            session_id.as_ptr(),
            title.as_ptr(),
            body.as_ptr(),
            icon_data_url
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
        )
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_deliver_macos_session_attention_notification(
    _candidate: GpuiSessionAttentionNotificationCandidate,
) -> GpuiMacOSNotificationDeliveryResult {
    GpuiMacOSNotificationDeliveryResult::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn apply_gpui_menu_bar_status_item(state: &GpuiSidebarSessionStatusIndicatorsState) {
    if let Some(visible_state) = gpui_menu_bar_status_item_visible_state(state) {
        let (_project_owners, project_entries) = gpui_menu_bar_status_native_projects(state);
        unsafe {
            GhostexGpuiApplyMenuBarStatusItemWithProjects(
                visible_state.attention_count,
                visible_state.working_count,
                visible_state.available_count,
                project_entries.as_ptr(),
                project_entries.len(),
            );
        }
    } else {
        hide_gpui_menu_bar_status_item();
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn apply_gpui_menu_bar_status_item(_state: &GpuiSidebarSessionStatusIndicatorsState) {}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_projects(
    state: &GpuiSidebarSessionStatusIndicatorsState,
) -> (
    Vec<GpuiMenuBarStatusNativeProjectOwner>,
    Vec<GpuiMenuBarStatusNativeProjectEntry>,
) {
    let project_owners = state
        .projects
        .iter()
        .filter_map(gpui_menu_bar_status_native_project_owner)
        .collect::<Vec<_>>();
    let project_entries = project_owners
        .iter()
        .map(|project| GpuiMenuBarStatusNativeProjectEntry {
            project_id: project.project_id.as_ptr(),
            title: project.title.as_ptr(),
            sessions: project.entries.as_ptr(),
            session_count: project.entries.len(),
        })
        .collect::<Vec<_>>();
    (project_owners, project_entries)
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_app_shots_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:GPUIAppShots 2026-06-25-23:07:
    The macOS App Shots monitor is process-global, but its callback target is the live GPUI root entity only. Register and remove it with the root lifecycle so native flags monitors cannot route captures to stale windows or fallback targets.
    */
    GPUI_APP_SHOTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiAppShotsCallbackTarget { app, async_app });
    });
    let images_directory = shared_settings::ghostex_storage_paths().images_dir();
    let Ok(images_directory) = std::ffi::CString::new(gpui_path_string(&images_directory)) else {
        return;
    };
    unsafe {
        GhostexGpuiInstallAppShotsEventMonitors(images_directory.as_ptr());
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_app_shots_callback_target() {
    unsafe {
        GhostexGpuiRemoveAppShotsEventMonitors();
    }
    GPUI_APP_SHOTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_app_shots_callback_target() -> Option<GpuiAppShotsCallbackTarget> {
    GPUI_APP_SHOTS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_session_attention_notification_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:GPUISettingsNotifications 2026-06-26-06:56:
    macOS attention notification responses are process-global UserNotifications callbacks, but GPUI should route clicks only while a live root app target is registered. The callback target carries no notification content and only dispatches the copied bounded session id through the existing status/pet activation path.
    */
    GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() =
            Some(GpuiSessionAttentionNotificationCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_session_attention_notification_callback_target() {
    GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_session_attention_notification_callback_target()
-> Option<GpuiSessionAttentionNotificationCallbackTarget> {
    GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_accessibility_display_options_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    NSWorkspace accessibility display-option notifications are process-global, but Reduce Motion should update only the live GPUI root. Register the callback target with app lifecycle and carry only the current boolean preference into the pet animation gate.
    */
    GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() =
            Some(GpuiAccessibilityDisplayOptionsCallbackTarget { app, async_app });
    });
    unsafe {
        GhostexGpuiInstallAccessibilityDisplayOptionsMonitor();
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_accessibility_display_options_callback_target() {
    unsafe {
        GhostexGpuiRemoveAccessibilityDisplayOptionsMonitor();
    }
    GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_accessibility_display_options_callback_target()
-> Option<GpuiAccessibilityDisplayOptionsCallbackTarget> {
    GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_sparkle_updater_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    // Sparkle delegate callbacks are process-global main-thread calls; route
    // them only while a live GPUI root is registered, mirroring the other
    // native callback targets.
    GPUI_SPARKLE_UPDATER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiSparkleUpdaterCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_sparkle_updater_callback_target() {
    GPUI_SPARKLE_UPDATER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_sparkle_updater_callback_target() -> Option<GpuiSparkleUpdaterCallbackTarget> {
    GPUI_SPARKLE_UPDATER_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_os_integration_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_OS_INTEGRATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiOsIntegrationCallbackTarget { app, async_app });
    });
    let pending = GPUI_PENDING_OS_INTEGRATION_URLS.with(|urls| urls.borrow_mut().split_off(0));
    if !pending.is_empty() {
        queue_gpui_os_integration_urls(pending);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_os_integration_callback_target() {
    GPUI_OS_INTEGRATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_callback_target() -> Option<GpuiOsIntegrationCallbackTarget> {
    GPUI_OS_INTEGRATION_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_first_responder_callback_target(
    gpui_root_view: *mut std::ffi::c_void,
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_FIRST_RESPONDER_CALLBACK_TARGETS.with(|targets| {
        targets.borrow_mut().insert(
            gpui_root_view as usize,
            GpuiFirstResponderCallbackTarget { app, async_app },
        );
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_first_responder_callback_target(gpui_root_view: *mut std::ffi::c_void) {
    GPUI_FIRST_RESPONDER_CALLBACK_TARGETS.with(|targets| {
        targets.borrow_mut().remove(&(gpui_root_view as usize));
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_first_responder_callback_target(
    gpui_root_view: *mut std::ffi::c_void,
) -> Option<GpuiFirstResponderCallbackTarget> {
    GPUI_FIRST_RESPONDER_CALLBACK_TARGETS
        .with(|targets| targets.borrow().get(&(gpui_root_view as usize)).cloned())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_keyboard_router_target(
    gpui_root_view: *mut std::ffi::c_void,
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        targets.borrow_mut().insert(
            gpui_root_view as usize,
            GpuiKeyboardRouterCallbackTarget {
                app,
                async_app,
                owner: GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::None),
                owner_generation: 0,
                window_keyboard_id: GPUI_KEYBOARD_ROUTER_NEXT_WINDOW_ID
                    .fetch_add(1, Ordering::Relaxed),
                pressed_keys: HashMap::new(),
            },
        );
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_keyboard_router_target(gpui_root_view: *mut std::ffi::c_void) {
    let root_key = gpui_root_view as usize;
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        targets.borrow_mut().remove(&root_key);
    });
    GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
        owners
            .borrow_mut()
            .retain(|_, owner_root_key| *owner_root_key != root_key);
    });
    GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS.with(|depths| {
        depths.borrow_mut().remove(&root_key);
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn update_gpui_keyboard_router_first_responder(
    gpui_root_view: *mut std::ffi::c_void,
    first_responder: FirstResponderTarget,
) {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        let mut targets = targets.borrow_mut();
        let Some(target) = targets.get_mut(&(gpui_root_view as usize)) else {
            return;
        };
        let next_owner = match first_responder {
            FirstResponderTarget::GpuiWindow
                if matches!(target.owner, GpuiKeyboardOwner::CompositedTerminal(_)) =>
            {
                return;
            }
            _ => GpuiKeyboardOwner::FirstResponder(first_responder),
        };
        if target.owner != next_owner {
            let previous_owner = target.owner;
            target.owner = next_owner;
            target.owner_generation = target.owner_generation.wrapping_add(1);
            log_gpui_keyboard_owner_change(target, previous_owner, "firstResponderChanged");
        }
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn update_gpui_keyboard_router_composited_terminal_focus(
    gpui_root_view: *mut std::ffi::c_void,
    terminal: GpuiEngineTerminalEventTarget,
    focused: bool,
    first_responder: FirstResponderTarget,
) {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        let mut targets = targets.borrow_mut();
        let Some(target) = targets.get_mut(&(gpui_root_view as usize)) else {
            return;
        };
        if focused {
            let next_owner = GpuiKeyboardOwner::CompositedTerminal(terminal);
            if target.owner != next_owner {
                let previous_owner = target.owner;
                target.owner = next_owner;
                target.owner_generation = target.owner_generation.wrapping_add(1);
                log_gpui_keyboard_owner_change(target, previous_owner, "compositedTerminalFocused");
            }
            return;
        }
        if target.owner != GpuiKeyboardOwner::CompositedTerminal(terminal) {
            return;
        }
        let previous_owner = target.owner;
        target.owner = GpuiKeyboardOwner::FirstResponder(first_responder);
        target.owner_generation = target.owner_generation.wrapping_add(1);
        log_gpui_keyboard_owner_change(target, previous_owner, "compositedTerminalBlurred");
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn log_gpui_keyboard_owner_change(
    target: &GpuiKeyboardRouterCallbackTarget,
    previous_owner: GpuiKeyboardOwner,
    reason: &'static str,
) {
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.keyboardRouter.ownerChanged",
        serde_json::json!({
            "generation": target.owner_generation,
            "owner": format!("{:?}", target.owner),
            "previousOwner": format!("{previous_owner:?}"),
            "reason": reason,
            "windowKeyboardId": target.window_keyboard_id,
        }),
    );
}

#[cfg(target_os = "macos")]
pub(crate) fn log_gpui_keyboard_native_route(
    target: &GpuiKeyboardRouterCallbackTarget,
    native_action: &'static str,
    keycode: u32,
    route: &'static str,
    handled: bool,
    action_id: Option<&str>,
    owner: GpuiKeyboardOwner,
) {
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.keyboardRouter.nativeEventDecision",
        serde_json::json!({
            "actionId": action_id,
            "generation": target.owner_generation,
            "handled": handled,
            "keycode": keycode,
            "nativeAction": native_action,
            "owner": format!("{owner:?}"),
            "route": route,
            "windowKeyboardId": target.window_keyboard_id,
        }),
    );
}

#[cfg(target_os = "macos")]
pub(crate) fn route_gpui_native_keyboard_event(
    gpui_root_view: *mut std::ffi::c_void,
    action: std::ffi::c_int,
    keycode: u32,
    modifiers: u64,
    characters_ignoring_modifiers: &str,
    characters: &str,
) -> bool {
    const NATIVE_KEY_PRESS: std::ffi::c_int = 1;
    const NATIVE_KEY_REPEAT: std::ffi::c_int = 2;
    const NATIVE_KEY_RELEASE: std::ffi::c_int = 3;
    const SHIFT: u64 = 1 << 17;
    const CONTROL: u64 = 1 << 18;
    const OPTION: u64 = 1 << 19;
    const COMMAND: u64 = 1 << 20;
    const FUNCTION: u64 = 1 << 23;
    const TAB_KEYCODE: u32 = 48;

    let supplied_root_key = gpui_root_view as usize;
    let root_key = if action == NATIVE_KEY_PRESS {
        supplied_root_key
    } else {
        GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
            owners
                .borrow()
                .get(&keycode)
                .copied()
                .unwrap_or(supplied_root_key)
        })
    };
    if action == NATIVE_KEY_RELEASE {
        GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
            owners.borrow_mut().remove(&keycode);
        });
    }
    let routed = GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        let mut targets = targets.borrow_mut();
        let target = targets.get_mut(&root_key)?;

        if action == NATIVE_KEY_RELEASE {
            return match target.pressed_keys.remove(&keycode) {
                Some(GpuiCapturedKeyRoute::CompositedTerminalTab { owner, shift }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "compositedTerminalTab",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((
                        target.app.clone(),
                        target.async_app.clone(),
                        Some(GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                            owner,
                            action: ghostty_vt::VtKeyAction::Release,
                            shift,
                        }),
                    ))
                }
                Some(GpuiCapturedKeyRoute::CompositedTerminalBulkText { owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "compositedTerminalBulkText",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::ApplicationCommand { command, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "applicationCommand",
                        true,
                        Some(command.log_id()),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::GhostexHotkey { action_id, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "ghostexHotkey",
                        true,
                        Some(&action_id),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                None => {
                    if keycode == TAB_KEYCODE {
                        log_gpui_keyboard_native_route(
                            target,
                            "release",
                            keycode,
                            "nativeResponderPassthrough",
                            false,
                            None,
                            target.owner,
                        );
                    }
                    None
                }
            };
        }

        if action == NATIVE_KEY_REPEAT {
            return match target.pressed_keys.get(&keycode).cloned() {
                Some(GpuiCapturedKeyRoute::CompositedTerminalTab { owner, shift }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "compositedTerminalTab",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((
                        target.app.clone(),
                        target.async_app.clone(),
                        Some(GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                            owner,
                            action: ghostty_vt::VtKeyAction::Repeat,
                            shift,
                        }),
                    ))
                }
                Some(GpuiCapturedKeyRoute::CompositedTerminalBulkText { owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "compositedTerminalBulkText",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::ApplicationCommand { command, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "applicationCommand",
                        true,
                        Some(command.log_id()),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::GhostexHotkey { action_id, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "ghostexHotkey",
                        true,
                        Some(&action_id),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                None => {
                    if keycode == TAB_KEYCODE {
                        log_gpui_keyboard_native_route(
                            target,
                            "repeat",
                            keycode,
                            "nativeResponderPassthrough",
                            false,
                            None,
                            target.owner,
                        );
                    }
                    None
                }
            };
        }

        if action != NATIVE_KEY_PRESS {
            return None;
        }

        let owner = target.owner;
        let native_hotkey_text =
            gpui_native_hotkey_text(keycode, modifiers, characters_ignoring_modifiers);
        let renderer_passthrough_route = if matches!(
            owner,
            GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
                FirstResponderCefSurface::SessionChat(_)
            ))
        ) && native_hotkey_text.as_deref() == Some("cmd+f")
        {
            Some("sessionChatRendererPassthrough")
        } else if gpui_keyboard_owner_uses_docs_editor_hotkeys(owner)
            && matches!(
                native_hotkey_text.as_deref(),
                Some("cmd+f" | "cmd+alt+f" | "cmd+y")
            )
        {
            Some("docsEditorRendererPassthrough")
        } else {
            None
        };
        if let Some(route) = renderer_passthrough_route {
            log_gpui_keyboard_native_route(target, "press", keycode, route, false, None, owner);
            return None;
        }
        if let Some(command) = native_hotkey_text
            .as_deref()
            .and_then(gpui_application_keyboard_command_for_native_text)
        {
            target.pressed_keys.insert(
                keycode,
                GpuiCapturedKeyRoute::ApplicationCommand { command, owner },
            );
            GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
                owners.borrow_mut().insert(keycode, root_key);
            });
            log_gpui_keyboard_native_route(
                target,
                "press",
                keycode,
                "applicationCommand",
                true,
                Some(command.log_id()),
                owner,
            );
            return Some((
                target.app.clone(),
                target.async_app.clone(),
                Some(GpuiNativeKeyboardDispatch::ApplicationCommand(command)),
            ));
        }
        let configured_action_id = native_hotkey_text
            .and_then(|text| gpui_configured_hotkey_action_id_for_native_text(&text));
        if let Some(action_id) = configured_action_id {
            if !gpui_keyboard_owner_allows_hotkey(owner, &action_id) {
                log_gpui_keyboard_native_route(
                    target,
                    "press",
                    keycode,
                    "ownerPolicyPassthrough",
                    false,
                    Some(&action_id),
                    owner,
                );
                return None;
            }
            target.pressed_keys.insert(
                keycode,
                GpuiCapturedKeyRoute::GhostexHotkey {
                    action_id: action_id.clone(),
                    owner,
                },
            );
            GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
                owners.borrow_mut().insert(keycode, root_key);
            });
            log_gpui_keyboard_native_route(
                target,
                "press",
                keycode,
                "ghostexHotkey",
                true,
                Some(&action_id),
                owner,
            );
            return Some((
                target.app.clone(),
                target.async_app.clone(),
                Some(GpuiNativeKeyboardDispatch::GhostexHotkey { action_id, owner }),
            ));
        }

        let only_shift = modifiers & (CONTROL | OPTION | COMMAND | FUNCTION) == 0;
        let GpuiKeyboardOwner::CompositedTerminal(owner) = owner else {
            if keycode == TAB_KEYCODE {
                log_gpui_keyboard_native_route(
                    target,
                    "press",
                    keycode,
                    "nativeResponderPassthrough",
                    false,
                    None,
                    owner,
                );
            }
            return None;
        };
        /*
        CDXC:GPUICompositedTerminalBulkUnicode 2026-07-27:
        Dictation and automation tools can post one CGEvent whose Unicode
        payload contains the whole committed string. GPUI derives Keystroke
        text from that event's physical keycode, reducing a keycode-zero bulk
        event to the layout's "a", while AppKit's NSTextInputClient path would
        receive the full event.characters string. Claim only multi-scalar,
        otherwise-unmodified text for the exact focused composited-terminal
        owner and deliver it as committed text before GPUI parses the keycode.
        Hardware keys and ordinary one-scalar terminal key events remain on
        the existing GPUI/libghostty key path.
        */
        if only_shift && characters.chars().count() > 1 {
            target.pressed_keys.insert(
                keycode,
                GpuiCapturedKeyRoute::CompositedTerminalBulkText { owner },
            );
            GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
                owners.borrow_mut().insert(keycode, root_key);
            });
            log_gpui_keyboard_native_route(
                target,
                "press",
                keycode,
                "compositedTerminalBulkText",
                true,
                None,
                GpuiKeyboardOwner::CompositedTerminal(owner),
            );
            return Some((
                target.app.clone(),
                target.async_app.clone(),
                Some(GpuiNativeKeyboardDispatch::CompositedTerminalBulkText {
                    owner,
                    text: characters.to_string(),
                }),
            ));
        }
        if keycode != TAB_KEYCODE || !only_shift {
            if keycode == TAB_KEYCODE {
                log_gpui_keyboard_native_route(
                    target,
                    "press",
                    keycode,
                    "nativeResponderPassthrough",
                    false,
                    None,
                    GpuiKeyboardOwner::CompositedTerminal(owner),
                );
            }
            return None;
        }
        let shift = modifiers & SHIFT != 0;
        target.pressed_keys.insert(
            keycode,
            GpuiCapturedKeyRoute::CompositedTerminalTab { owner, shift },
        );
        GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
            owners.borrow_mut().insert(keycode, root_key);
        });
        log_gpui_keyboard_native_route(
            target,
            "press",
            keycode,
            "compositedTerminalTab",
            true,
            None,
            GpuiKeyboardOwner::CompositedTerminal(owner),
        );
        Some((
            target.app.clone(),
            target.async_app.clone(),
            Some(GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                owner,
                action: ghostty_vt::VtKeyAction::Press,
                shift,
            }),
        ))
    });
    let Some((app, mut async_app, dispatch)) = routed else {
        return false;
    };
    let Some(dispatch) = dispatch else {
        return true;
    };
    let foreground = async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| match dispatch {
                GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                    owner,
                    action,
                    shift,
                } => {
                    let _ = this.send_tab_key_to_gpui_engine_terminal(owner, action, shift, cx);
                }
                GpuiNativeKeyboardDispatch::CompositedTerminalBulkText { owner, text } => {
                    let view = this.gpui_engine_terminal_view_for_target(owner);
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.gpui.fluidVoice.bulkTextDispatch",
                        serde_json::json!({
                            "targetFound": view.is_some(),
                            "text": support_logs::temporary_fluid_voice_text_shape(&text),
                        }),
                    );
                    if let Some(view) = view {
                        view.update(cx, |view, cx| view.send_text_input(&text, cx));
                    }
                }
                GpuiNativeKeyboardDispatch::ApplicationCommand(command) => {
                    this.dispatch_window_scoped_application_keyboard_command(command, window, cx);
                }
                GpuiNativeKeyboardDispatch::GhostexHotkey { action_id, owner } => {
                    this.dispatch_window_scoped_ghostex_hotkey(&action_id, owner, window, cx);
                }
            });
        })
        .detach();
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_app_shots_c_string(ptr: *const std::ffi::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_action_c_string(ptr: *const std::ffi::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    gpui_status_bridge_id_allowed(text.as_str()).then_some(text)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_session_attention_notification_action_c_string(
    ptr: *const std::ffi::c_char,
) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    gpui_status_bridge_id_allowed(text.as_str()).then_some(text)
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_session_attention_notification_click(session_id: String) {
    let Some(target) = gpui_session_attention_notification_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                cx.activate(true);
                window.activate_window();
                this.dispatch_gpui_status_pet_activation(session_id.as_str(), cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_accessibility_display_options_changed(should_reduce_motion: bool) {
    let Some(target) = gpui_accessibility_display_options_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_pet_overlay_reduce_motion_enabled(should_reduce_motion, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_sparkle_update_available_changed(available: bool) {
    let Some(target) = gpui_sparkle_updater_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_update_available(available, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_sparkle_update_downloading_changed(downloading: bool) {
    let Some(target) = gpui_sparkle_updater_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_update_downloading(downloading, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_sparkle_update_download_progress_changed(progress: Option<f64>) {
    let Some(target) = gpui_sparkle_updater_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_update_download_progress(progress, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_app_shot_capture(capture: GpuiAppShotCapture) {
    let Some(target) = gpui_app_shots_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            /*
            CDXC:GPUIAppShots 2026-06-26-04:18:
            Native App Shots callbacks must copy capture metadata at the FFI boundary and then enqueue a foreground GPUI update without borrowing `AsyncApp` across the returned future. This keeps the C callback non-blocking while preserving the existing Rust/sidebar capture contract.
            */
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                this.handle_gpui_native_app_shot_capture(capture, window, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_app_shot_status(message: &'static str) {
    let Some(target) = gpui_app_shots_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                window.push_notification(Notification::warning(message), cx);
                this.dispatch_gpui_app_modal_toast("warning", "App Shot Failed", message, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_completion_sound_player(path: &Path) -> Result<(), String> {
    std::process::Command::new("/usr/bin/afplay")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "GPUI could not start the macOS sound preview player.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_spawn_completion_sound_player(_path: &Path) -> Result<(), String> {
    Err("GPUI sound preview is not available on this platform yet.".to_string())
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    pub(crate) fn GhostexGpuiRequestNotificationAuthorization() -> i32;
    pub(crate) fn GhostexGpuiDeliverSettingsTestNotification() -> i32;
    pub(crate) fn GhostexGpuiAccessibilityDisplayShouldReduceMotion() -> i32;
    pub(crate) fn GhostexGpuiInstallAccessibilityDisplayOptionsMonitor();
    pub(crate) fn GhostexGpuiRemoveAccessibilityDisplayOptionsMonitor();
    pub(crate) fn GhostexGpuiInstallWorkspacePowerEventsMonitor();
    pub(crate) fn GhostexGpuiRemoveWorkspacePowerEventsMonitor();
    pub(crate) fn GhostexGpuiDeliverSessionAttentionNotification(
        session_id: *const std::ffi::c_char,
        title: *const std::ffi::c_char,
        body: *const std::ffi::c_char,
        icon_data_url: *const std::ffi::c_char,
    ) -> i32;
    pub(crate) fn GhostexGpuiInstallAppShotsEventMonitors(shots_directory: *const std::ffi::c_char);
    pub(crate) fn GhostexGpuiRemoveAppShotsEventMonitors();
    pub(crate) fn GhostexGpuiSparkleUpdaterStart() -> i32;
    pub(crate) fn GhostexGpuiSparkleCheckForUpdates();
    pub(crate) fn GhostexGpuiSparkleProbeForUpdateInformation();
    pub(crate) fn GhostexGpuiShowStandardAboutPanel();
    pub(crate) fn GhostexGpuiSetLidSleepPreventionEnabled(enabled: i32, install_if_needed: i32) -> i32;
    pub(crate) fn GhostexGpuiHeartbeatLidSleepPrevention() -> i32;
    pub(crate) fn GhostexGpuiApplyMenuBarStatusItemWithProjects(
        attention_count: u64,
        working_count: u64,
        available_count: u64,
        projects: *const GpuiMenuBarStatusNativeProjectEntry,
        project_count: usize,
    );
    pub(crate) fn GhostexGpuiHideMenuBarStatusItem();
    pub(crate) fn GhostexGpuiSaveRemoteSshPassword(
        remote_machine_id: *const std::ffi::c_char,
        password_bytes: *const u8,
        password_len: usize,
    ) -> i32;
    pub(crate) fn GhostexGpuiCopyRemoteSshPassword(
        remote_machine_id: *const std::ffi::c_char,
        password_bytes: *mut u8,
        password_capacity: usize,
        password_len: *mut usize,
    ) -> i32;
    pub(crate) fn GhostexGpuiSaveRemoteGxserverToken(
        remote_machine_id: *const std::ffi::c_char,
        token_bytes: *const u8,
        token_len: usize,
    ) -> i32;
    pub(crate) fn GhostexGpuiRemoveToastPopupWindowChrome(native_view: *mut std::ffi::c_void);
    pub(crate) fn GhostexGpuiAttachToastPopupToMainWindow(
        toast_native_view: *mut std::ffi::c_void,
        main_native_view: *mut std::ffi::c_void,
    );
    pub(crate) fn GhostexGpuiPrepareTitlebarPopupWindow(native_view: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_set_lid_sleep_prevention_enabled(enabled: bool, install_if_needed: bool) -> bool {
    unsafe {
        GhostexGpuiSetLidSleepPreventionEnabled(enabled as i32, install_if_needed as i32) == 1
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_heartbeat_lid_sleep_prevention() -> bool {
    unsafe { GhostexGpuiHeartbeatLidSleepPrevention() == 1 }
}

pub(crate) fn gpui_completion_sound_path(file_name: &str) -> Option<PathBuf> {
    if !gpui_is_bundled_sound_file_name(file_name) {
        return None;
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(bundle_root) = find_app_bundle_root(&executable) {
            for directory in [
                bundle_root.join("Contents/Resources/Web/sounds"),
                bundle_root.join("Contents/Resources/sidebar/sounds"),
            ] {
                let candidate = directory.join(file_name);
                if gpui_is_file(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_sound = manifest_dir
        .parent()?
        .parent()?
        .join("media/sounds")
        .join(file_name);
    gpui_is_file(&repo_sound).then_some(repo_sound)
}

pub(crate) fn gpui_is_bundled_sound_file_name(file_name: &str) -> bool {
    file_name.ends_with(".mp3")
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && !file_name.contains("..")
}

pub(crate) fn gpui_normalize_completion_sound(sound: Option<&str>) -> &'static str {
    match sound.unwrap_or("arcade") {
        "ping" => "ping",
        "pingdouble" => "pingdouble",
        "glass" => "glass",
        "glimmer" => "glimmer",
        "shamisen" => "shamisen",
        "shamisenreverb" => "shamisenreverb",
        "arcade" => "arcade",
        "arcadeboost" => "arcadeboost",
        "confirmation-001" => "confirmation-001",
        "confirmation-002" => "confirmation-002",
        "confirmation-003" => "confirmation-003",
        "confirmation-004" => "confirmation-004",
        "notification-pop" => "notification-pop",
        "success-chime" => "success-chime",
        "high-up" => "high-up",
        "high-down" => "high-down",
        "low-three-tone" => "low-three-tone",
        "tone-1" => "tone-1",
        "three-tone-1" => "three-tone-1",
        "three-tone-2" => "three-tone-2",
        "two-tone-1" => "two-tone-1",
        "two-tone-2" => "two-tone-2",
        "power-up-5" => "power-up-5",
        "power-up-6" => "power-up-6",
        "power-up-8" => "power-up-8",
        "coin-collect" => "coin-collect",
        "phaser-up-5" => "phaser-up-5",
        "zap-two-tone" => "zap-two-tone",
        "voiceover-pack-male-mission-completed" => "voiceover-pack-male-mission-completed",
        "voiceover-pack-female-mission-completed" => "voiceover-pack-female-mission-completed",
        "voiceover-pack-male-you-win" => "voiceover-pack-male-you-win",
        "voiceover-pack-female-congratulations" => "voiceover-pack-female-congratulations",
        "flawless-victory" => "flawless-victory",
        _ => "arcade",
    }
}

pub(crate) fn gpui_completion_sound_file_name(sound: &str) -> &'static str {
    match gpui_normalize_completion_sound(Some(sound)) {
        "ping" => "ping.mp3",
        "pingdouble" => "pingdouble.mp3",
        "glass" => "glass.mp3",
        "glimmer" => "glimmer.mp3",
        "shamisen" => "shamisen.mp3",
        "shamisenreverb" => "shamisenreverb.mp3",
        "arcadeboost" => "arcadeboost.mp3",
        "confirmation-001" => "confirmation-001.mp3",
        "confirmation-002" => "confirmation-002.mp3",
        "confirmation-003" => "confirmation-003.mp3",
        "confirmation-004" => "confirmation-004.mp3",
        "notification-pop" => "notification-pop.mp3",
        "success-chime" => "success-chime.mp3",
        "high-up" => "high-up.mp3",
        "high-down" => "high-down.mp3",
        "low-three-tone" => "low-three-tone.mp3",
        "tone-1" => "tone-1.mp3",
        "three-tone-1" => "three-tone-1.mp3",
        "three-tone-2" => "three-tone-2.mp3",
        "two-tone-1" => "two-tone-1.mp3",
        "two-tone-2" => "two-tone-2.mp3",
        "power-up-5" => "power-up-5.mp3",
        "power-up-6" => "power-up-6.mp3",
        "power-up-8" => "power-up-8.mp3",
        "coin-collect" => "coin-collect.mp3",
        "phaser-up-5" => "phaser-up-5.mp3",
        "zap-two-tone" => "zap-two-tone.mp3",
        "voiceover-pack-male-mission-completed" => "voiceover-pack-male-mission-completed.mp3",
        "voiceover-pack-female-mission-completed" => "voiceover-pack-female-mission-completed.mp3",
        "voiceover-pack-male-you-win" => "voiceover-pack-male-you-win.mp3",
        "voiceover-pack-female-congratulations" => "voiceover-pack-female-congratulations.mp3",
        "flawless-victory" => "flawless-victory.mp3",
        _ => "arcade.mp3",
    }
}

pub(crate) fn gpui_status_generated_at() -> String {
    gpui_iso8601_utc(SystemTime::now())
}

pub(crate) fn gpui_iso8601_utc(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_seconds = duration.as_secs() as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = gpui_civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    )
}

pub(crate) fn gpui_civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiGhostexCliProbe {
    pub(crate) agent_orchestration_skill_path: Option<String>,
    pub(crate) browser_skill_path: Option<String>,
    pub(crate) computer_use_skill_path: Option<String>,
    pub(crate) embedded_browser_skill_path: Option<String>,
    pub(crate) fable56_orchestration_skill_path: Option<String>,
    pub(crate) find_prev_session_skill_path: Option<String>,
    pub(crate) generate_title_skill_path: Option<String>,
    pub(crate) ghostex_path: Option<String>,
    pub(crate) ghostex_usable: bool,
    pub(crate) gx_blocked_by_existing_command: bool,
    pub(crate) gx_path: Option<String>,
    pub(crate) gx_usable: bool,
    pub(crate) move_codex_session_skill_path: Option<String>,
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_ghostex_cli_probe() -> Result<GpuiGhostexCliProbe, String> {
    let status = windows_terminal_backend::ghostex_cli_status()?;
    Ok(GpuiGhostexCliProbe {
        agent_orchestration_skill_path: status.agent_orchestration_skill_path,
        browser_skill_path: status.browser_skill_path,
        computer_use_skill_path: status.computer_use_skill_path,
        embedded_browser_skill_path: status.embedded_browser_skill_path,
        fable56_orchestration_skill_path: status.fable56_orchestration_skill_path,
        find_prev_session_skill_path: status.find_prev_session_skill_path,
        generate_title_skill_path: status.generate_title_skill_path,
        ghostex_usable: status.ghostex_path.is_some(),
        ghostex_path: status.ghostex_path,
        gx_blocked_by_existing_command: status.gx_blocked_by_existing_command,
        gx_path: status.gx_path,
        gx_usable: status.gx_usable,
        move_codex_session_skill_path: status.move_codex_session_skill_path,
    })
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_ghostex_cli_probe() -> Result<GpuiGhostexCliProbe, String> {
    let home = gpui_home_dir();
    let ghostex_path = gpui_which_command("ghostex");
    let gx_path = gpui_which_command("gx");
    let ghostex_usable = ghostex_path
        .as_ref()
        .map(|path| gpui_is_probably_ghostex_command(path, "ghostex"))
        .unwrap_or(false);
    let gx_usable = gx_path
        .as_ref()
        .map(|path| gpui_is_probably_ghostex_command(path, "gx"))
        .unwrap_or(false);
    let skill_path = |name: &str| {
        let path = home.join(".agents/skills").join(name).join("SKILL.md");
        gpui_is_file(&path).then(|| gpui_path_string(&path))
    };
    Ok(GpuiGhostexCliProbe {
        agent_orchestration_skill_path: skill_path("ghostex-agent-orchestration"),
        browser_skill_path: skill_path("ghostex-browser-use"),
        computer_use_skill_path: skill_path("ghostex-computer-use"),
        embedded_browser_skill_path: skill_path("ghostex-embedded-browser-use"),
        fable56_orchestration_skill_path: skill_path("ghostex-fable-5.6-orchestration"),
        find_prev_session_skill_path: skill_path("ghostex-find-prev-session"),
        generate_title_skill_path: skill_path("ghostex-auto-rename-session"),
        ghostex_path: ghostex_path.as_ref().map(|path| gpui_path_string(path)),
        ghostex_usable,
        gx_blocked_by_existing_command: gx_path.is_some() && !gx_usable,
        gx_path: gx_path.as_ref().map(|path| gpui_path_string(path)),
        gx_usable,
        move_codex_session_skill_path: skill_path("ghostex-move-codex-session"),
    })
}

pub(crate) fn gpui_ghostex_cli_status_message(detail_override: Option<&str>) -> serde_json::Value {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:36:
    GPUI Settings must answer CLI/status refreshes with the shared React contract so integration rows stop loading. The read-only GPUI probe may inspect PATH, fixed Ghostex-owned skill paths, the app bundle/local CEF resources, and Cua Driver presence, but it must not run installers, repair commands, permission prompts, or log raw paths.
    */
    let (probe, probe_error) = match gpui_ghostex_cli_probe() {
        Ok(probe) => (probe, None),
        Err(message) => (GpuiGhostexCliProbe::default(), Some(message)),
    };
    let ghostex_usable = probe.ghostex_usable;
    let gx_usable = probe.gx_usable;
    let gx_blocked = probe.gx_blocked_by_existing_command;
    let browser_skill_installed = probe.browser_skill_path.is_some();
    let embedded_browser_skill_installed = probe.embedded_browser_skill_path.is_some();
    let computer_use_skill_installed = probe.computer_use_skill_path.is_some();
    let agent_orchestration_skill_installed = probe.agent_orchestration_skill_path.is_some();
    let fable56_orchestration_skill_installed = probe.fable56_orchestration_skill_path.is_some();
    let find_prev_session_skill_installed = probe.find_prev_session_skill_path.is_some();
    let generate_title_skill_installed = probe.generate_title_skill_path.is_some();
    let move_codex_session_skill_installed = probe.move_codex_session_skill_path.is_some();
    let cua_driver_path = gpui_cua_driver_executable_path();
    let cua_app_installed = gpui_is_dir(Path::new("/Applications/CuaDriver.app"));
    let cua_driver_installed = cua_driver_path.is_some() || cua_app_installed;
    let desktop_control_installed = cua_driver_installed && computer_use_skill_installed;
    let cua_driver_update_status = gpui_cua_driver_update_status(cua_driver_path.as_deref());
    let cua_permission_status =
        gpui_cua_driver_permission_status(cua_driver_path.as_deref(), cua_app_installed);
    let detail = detail_override
        .map(str::to_string)
        .unwrap_or_else(|| {
            let mut parts = Vec::new();
            if let Some(probe_error) = probe_error.as_ref() {
                parts.push(probe_error.clone());
            } else if ghostex_usable {
                #[cfg(target_os = "windows")]
                parts.push(
                    "Ghostex CLI is installed in the selected WSL2 distribution and matches this app's managed gxserver package."
                        .to_string(),
                );
                #[cfg(not(target_os = "windows"))]
                parts.push(
                    "Ghostex CLI was found on PATH and appears to be Ghostex-owned.".to_string(),
                );
            } else if probe.ghostex_path.is_some() {
                parts.push("A ghostex command was found on PATH, but GPUI could not prove it is the Ghostex-owned wrapper or app command.".to_string());
            } else {
                #[cfg(target_os = "windows")]
                parts.push(
                    "Ghostex CLI was not found in the selected WSL2 distribution.".to_string(),
                );
                #[cfg(not(target_os = "windows"))]
                parts.push("Ghostex CLI was not found on PATH.".to_string());
            }
            if gx_usable {
                #[cfg(target_os = "windows")]
                parts.push("The gx alias in WSL is linked to the managed Ghostex CLI.".to_string());
                #[cfg(not(target_os = "windows"))]
                parts.push("The gx alias appears to be Ghostex-owned.".to_string());
            } else if gx_blocked {
                parts.push("A gx command exists on PATH, but GPUI could not prove it belongs to Ghostex.".to_string());
            }
            parts.push(if browser_skill_installed {
                "Ghostex Browser Use skill is installed.".to_string()
            } else {
                "Ghostex Browser Use skill is not installed.".to_string()
            });
            parts.push(if computer_use_skill_installed {
                "Ghostex Computer Use skill is installed.".to_string()
            } else {
                "Ghostex Computer Use skill is not installed.".to_string()
            });
            parts.push(if embedded_browser_skill_installed {
                "Ghostex Embedded Browser Use skill is installed.".to_string()
            } else {
                "Ghostex Embedded Browser Use skill is not installed.".to_string()
            });
            parts.push(if agent_orchestration_skill_installed {
                "Ghostex Agent Orchestration skill is installed.".to_string()
            } else {
                "Ghostex Agent Orchestration skill is not installed.".to_string()
            });
            parts.push(if fable56_orchestration_skill_installed {
                "Ghostex Fable 5.6 Orchestration skill is installed.".to_string()
            } else {
                "Ghostex Fable 5.6 Orchestration skill is not installed.".to_string()
            });
            parts.push(if find_prev_session_skill_installed {
                "Ghostex Find Previous Session skill is installed.".to_string()
            } else {
                "Ghostex Find Previous Session skill is not installed.".to_string()
            });
            parts.push(if generate_title_skill_installed {
                "Ghostex Auto Rename Session skill is installed.".to_string()
            } else {
                "Ghostex Auto Rename Session skill is not installed.".to_string()
            });
            parts.push(if move_codex_session_skill_installed {
                "Ghostex Move Codex Session skill is installed.".to_string()
            } else {
                "Ghostex Move Codex Session skill is not installed.".to_string()
            });
            /*
            CDXC:GPUIDesktopControlSettings 2026-06-24-13:14:
            Desktop Control readiness in Settings requires both Cua Driver and the Ghostex Computer Use skill. GPUI status refreshes must probe Cua Driver privacy grants read-only with `prompt:false`, parse the two boolean contract fields, and keep permission detail generic instead of forwarding raw command output.
            */
            parts.push(if desktop_control_installed {
                "Desktop Control is installed.".to_string()
            } else {
                "Desktop Control is not installed yet.".to_string()
            });
            parts.push(cua_permission_status.detail.clone());
            parts.join(" ")
        });

    serde_json::json!({
        "agentOrchestrationSkillInstalled": agent_orchestration_skill_installed,
        "agentOrchestrationSkillPath": probe.agent_orchestration_skill_path,
        "browserSkillInstalled": browser_skill_installed,
        "browserSkillPath": probe.browser_skill_path,
        "computerUseSkillInstalled": computer_use_skill_installed,
        "computerUseSkillPath": probe.computer_use_skill_path,
        "embeddedBrowserSkillInstalled": embedded_browser_skill_installed,
        "embeddedBrowserSkillPath": probe.embedded_browser_skill_path,
        "cuaAppInstalled": cua_app_installed,
        "cuaDriverAccessibilityPermissionGranted": cua_permission_status.accessibility_granted,
        "cuaDriverInstalled": cua_driver_installed,
        "cuaDriverLatestVersion": cua_driver_update_status.latest_version,
        "cuaDriverManagedUpdatesSupported": cfg!(target_os = "macos"),
        "cuaDriverPermissionDetail": cua_permission_status.detail,
        "cuaDriverPath": cua_driver_path.as_ref().map(|path| gpui_path_string(path)),
        "cuaDriverScreenRecordingPermissionGranted": cua_permission_status.screen_recording_granted,
        "cuaDriverUpdateAvailable": cua_driver_update_status.update_available,
        "cuaDriverVersion": cua_driver_update_status.current_version,
        "detail": detail,
        "fable56OrchestrationSkillInstalled": fable56_orchestration_skill_installed,
        "fable56OrchestrationSkillPath": probe.fable56_orchestration_skill_path,
        "findPrevSessionSkillInstalled": find_prev_session_skill_installed,
        "findPrevSessionSkillPath": probe.find_prev_session_skill_path,
        "generatedAt": gpui_status_generated_at(),
        "generateTitleSkillInstalled": generate_title_skill_installed,
        "generateTitleSkillPath": probe.generate_title_skill_path,
        "ghostexPath": probe.ghostex_path,
        "gxBlockedByExistingCommand": gx_blocked,
        "gxPath": probe.gx_path,
        "gxUsable": gx_usable,
        "installed": ghostex_usable,
        "moveCodexSessionSkillInstalled": move_codex_session_skill_installed,
        "moveCodexSessionSkillPath": probe.move_codex_session_skill_path,
        "type": "ghostexCliStatus",
    })
}

pub(crate) fn gpui_cua_driver_executable_path() -> Option<PathBuf> {
    if let Some(path) = gpui_which_command("cua-driver") {
        return Some(path);
    }
    #[cfg(target_os = "macos")]
    {
        let app_binary = PathBuf::from("/Applications/CuaDriver.app/Contents/MacOS/cua-driver");
        if gpui_is_file(&app_binary) {
            return Some(app_binary);
        }
    }
    None
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiCuaDriverUpdateStatus {
    pub(crate) current_version: Option<String>,
    pub(crate) latest_version: Option<String>,
    pub(crate) update_available: Option<bool>,
}

pub(crate) fn gpui_cua_driver_update_status(cua_driver_path: Option<&Path>) -> GpuiCuaDriverUpdateStatus {
    let Some(cua_driver_path) = cua_driver_path else {
        return GpuiCuaDriverUpdateStatus::default();
    };
    let current_version = gpui_run_command_with_captured_output_timeout(
        cua_driver_path,
        &["--version"],
        Duration::from_secs(3),
        8 * 1024,
    )
    .ok()
    .filter(|output| output.success)
    .and_then(|output| gpui_cua_driver_version_from_text(output.stdout.as_str()));

    #[cfg(target_os = "macos")]
    {
        let Ok(output) = gpui_run_command_with_captured_output_timeout(
            cua_driver_path,
            &["check-update", "--json"],
            Duration::from_secs(15),
            64 * 1024,
        ) else {
            return GpuiCuaDriverUpdateStatus {
                current_version,
                ..GpuiCuaDriverUpdateStatus::default()
            };
        };
        let Some(payload) = gpui_cua_driver_update_payload(output.stdout.as_str()) else {
            return GpuiCuaDriverUpdateStatus {
                current_version,
                ..GpuiCuaDriverUpdateStatus::default()
            };
        };
        return GpuiCuaDriverUpdateStatus {
            current_version: current_version.or_else(|| {
                payload
                    .get("current_version")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            }),
            latest_version: payload
                .get("latest_version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            update_available: payload
                .get("update_available")
                .and_then(serde_json::Value::as_bool),
        };
    }

    #[cfg(not(target_os = "macos"))]
    GpuiCuaDriverUpdateStatus {
        current_version,
        ..GpuiCuaDriverUpdateStatus::default()
    }
}

pub(crate) fn gpui_cua_driver_update_payload(stdout: &str) -> Option<serde_json::Value> {
    // A fresh Cua install can print its telemetry notice before JSON even when
    // --json is requested. Isolate the object so the Plugins status still gets
    // an exact current/latest version on that first check.
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    (start <= end)
        .then(|| serde_json::from_str::<serde_json::Value>(&stdout[start..=end]).ok())
        .flatten()
}

pub(crate) fn gpui_cua_driver_version_from_text(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-' | '+')
            })
        })
        .find(|token| {
            token
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                && token.contains('.')
                && token.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
                })
        })
        .map(str::to_string)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCuaDriverPermissionStatus {
    pub(crate) accessibility_granted: Option<bool>,
    pub(crate) detail: String,
    pub(crate) screen_recording_granted: Option<bool>,
}

pub(crate) fn gpui_cua_driver_permission_status(
    cua_driver_path: Option<&Path>,
    cua_app_installed: bool,
) -> GpuiCuaDriverPermissionStatus {
    let Some(cua_driver_path) = cua_driver_path else {
        return GpuiCuaDriverPermissionStatus {
            accessibility_granted: None,
            detail: if cua_app_installed {
                "Cua Driver app is installed, but the cua-driver CLI was not found on PATH, so GPUI cannot run the read-only permission check."
                    .to_string()
            } else {
                "Cua Driver is not installed.".to_string()
            },
            screen_recording_granted: None,
        };
    };

    /*
    CDXC:GPUIDesktopControlSettings 2026-06-24-13:14:
    The Cua permission probe is a status refresh, not a repair action. Run only `cua-driver check_permissions {"prompt":false}` with a short timeout, parse Accessibility and Screen Recording, and discard stdout/stderr before producing user-facing copy.
    */
    match gpui_run_command_with_captured_output_timeout(
        cua_driver_path,
        &["check_permissions", r#"{"prompt":false}"#],
        Duration::from_secs(5),
        64 * 1024,
    ) {
        Ok(output) => {
            let combined = output.combined_text();
            gpui_cua_driver_permission_status_from_output(&combined, output.success)
        }
        Err(_) => GpuiCuaDriverPermissionStatus {
            accessibility_granted: None,
            detail: "Unable to check Cua Driver permissions without prompting.".to_string(),
            screen_recording_granted: None,
        },
    }
}

pub(crate) fn gpui_cua_driver_permission_status_from_output(
    output: &str,
    command_success: bool,
) -> GpuiCuaDriverPermissionStatus {
    let payload = gpui_parse_cua_permission_payload(output);
    let accessibility_granted = gpui_parse_cua_permission(payload.as_ref(), "accessibility");
    let screen_recording_granted = gpui_parse_cua_permission(payload.as_ref(), "screen_recording");
    GpuiCuaDriverPermissionStatus {
        accessibility_granted,
        detail: gpui_cua_driver_permission_detail(
            accessibility_granted,
            screen_recording_granted,
            command_success,
        ),
        screen_recording_granted,
    }
}

/*
CDXC:GPUIDesktopControlSettings 2026-08-20-11:05:
`cua-driver check_permissions` answers with a JSON object whose `accessibility`
and `screen_recording` members are booleans, so read those members instead of
scanning for prose lines that the CLI never prints.
*/
pub(crate) fn gpui_parse_cua_permission_payload(output: &str) -> Option<serde_json::Value> {
    let trimmed = output.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(value);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]).ok()
}

pub(crate) fn gpui_parse_cua_permission(payload: Option<&serde_json::Value>, key: &str) -> Option<bool> {
    payload?.get(key)?.as_bool()
}

pub(crate) fn gpui_cua_driver_permission_detail(
    accessibility_granted: Option<bool>,
    screen_recording_granted: Option<bool>,
    command_success: bool,
) -> String {
    match (accessibility_granted, screen_recording_granted) {
        (Some(true), Some(true)) => {
            "Cua Driver reports Accessibility and Screen Recording permissions are granted."
                .to_string()
        }
        (Some(false), Some(false)) => "Cua Driver permissions need attention.".to_string(),
        (Some(false), _) => "Cua Driver Accessibility permission needs attention.".to_string(),
        (_, Some(false)) => "Cua Driver Screen Recording permission needs attention.".to_string(),
        _ if command_success => {
            "Cua Driver permission check completed, but GPUI could not recognize the permission state."
                .to_string()
        }
        _ => "Unable to check Cua Driver permissions without prompting.".to_string(),
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GpuiGhostexCliSettingsAction {
    InstallGhostexCli,
    InstallBrowserControl,
    InstallBrowserUseSkill,
    InstallComputerUseSkill,
    InstallAgentOrchestrationSkill,
    InstallFable56OrchestrationSkill,
    InstallFindPrevSessionSkill,
    InstallGenerateTitleSkill,
    InstallMoveCodexSessionSkill,
    FinishDesktopControlSetup {
        driver_installed: bool,
        was_update: bool,
    },
    UninstallBundledAgentSkill(&'static str),
    UninstallBundledAgentSkills,
}

impl GpuiGhostexCliSettingsAction {
    pub(crate) fn action_id(self) -> &'static str {
        match self {
            Self::InstallGhostexCli => "installGhostexCli",
            Self::InstallBrowserControl => "installBrowserControl",
            Self::InstallBrowserUseSkill => "installBrowserUseSkill",
            Self::InstallComputerUseSkill => "installComputerUseSkill",
            Self::InstallAgentOrchestrationSkill => "installAgentOrchestrationSkill",
            Self::InstallFable56OrchestrationSkill => "installFable56OrchestrationSkill",
            Self::InstallFindPrevSessionSkill => "installFindPrevSessionSkill",
            Self::InstallGenerateTitleSkill => "installGenerateTitleSkill",
            Self::InstallMoveCodexSessionSkill => "installMoveCodexSessionSkill",
            Self::FinishDesktopControlSetup { .. } => "installCuaDriver",
            Self::UninstallBundledAgentSkill(_) => "uninstallBundledAgentSkill",
            Self::UninstallBundledAgentSkills => "uninstallBundledAgentSkills",
        }
    }

    pub(crate) fn success_toast_title(self) -> &'static str {
        match self {
            Self::InstallGhostexCli => "Ghostex CLI linked",
            Self::InstallBrowserControl => "Ghostex Embedded Browser Use installed",
            Self::InstallBrowserUseSkill => "Ghostex Browser Use installed",
            Self::InstallComputerUseSkill => "Ghostex Computer Use installed",
            Self::InstallAgentOrchestrationSkill => "Ghostex Agent Orchestration installed",
            Self::InstallFable56OrchestrationSkill => "Ghostex Fable 5.6 Orchestration installed",
            Self::InstallFindPrevSessionSkill => "Ghostex Find Previous Session installed",
            Self::InstallGenerateTitleSkill => "Ghostex Auto Rename Session installed",
            Self::InstallMoveCodexSessionSkill => "Ghostex Move Codex Session installed",
            Self::FinishDesktopControlSetup {
                was_update: true, ..
            } => "Cua Driver updated",
            Self::FinishDesktopControlSetup { .. } => "Desktop Control installed",
            Self::UninstallBundledAgentSkill(_) => "Agent skill uninstalled",
            Self::UninstallBundledAgentSkills => "Bundled agent skills uninstalled",
        }
    }

    pub(crate) fn failure_toast_title(self) -> &'static str {
        match self {
            Self::InstallGhostexCli => "Ghostex CLI repair unavailable",
            Self::InstallBrowserControl => "Ghostex Embedded Browser Use install failed",
            Self::InstallBrowserUseSkill => "Ghostex Browser Use install failed",
            Self::InstallComputerUseSkill => "Ghostex Computer Use install failed",
            Self::InstallAgentOrchestrationSkill => "Ghostex Agent Orchestration install failed",
            Self::InstallFable56OrchestrationSkill => {
                "Ghostex Fable 5.6 Orchestration install failed"
            }
            Self::InstallFindPrevSessionSkill => "Ghostex Find Previous Session install failed",
            Self::InstallGenerateTitleSkill => "Ghostex Auto Rename Session install failed",
            Self::InstallMoveCodexSessionSkill => "Ghostex Move Codex Session install failed",
            Self::FinishDesktopControlSetup {
                was_update: true, ..
            } => "Cua Driver update failed",
            Self::FinishDesktopControlSetup { .. } => "Desktop Control setup incomplete",
            Self::UninstallBundledAgentSkill(_) => "Bundled agent skill uninstall failed",
            Self::UninstallBundledAgentSkills => "Bundled agent skill uninstall failed",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiGhostexCliActionResult {
    pub(crate) action_id: &'static str,
    pub(crate) available: bool,
    pub(crate) message: String,
    pub(crate) toast_level: &'static str,
    pub(crate) toast_title: &'static str,
}

impl GpuiGhostexCliActionResult {
    pub(crate) fn success(action: GpuiGhostexCliSettingsAction, message: String) -> Self {
        Self {
            action_id: action.action_id(),
            available: true,
            message,
            toast_level: "success",
            toast_title: action.success_toast_title(),
        }
    }

    pub(crate) fn failure(action: GpuiGhostexCliSettingsAction, message: String) -> Self {
        Self {
            action_id: action.action_id(),
            available: false,
            message,
            toast_level: "warning",
            toast_title: action.failure_toast_title(),
        }
    }
}

pub(crate) fn gpui_run_ghostex_cli_settings_action(
    action: GpuiGhostexCliSettingsAction,
) -> GpuiGhostexCliActionResult {
    match action {
        GpuiGhostexCliSettingsAction::InstallGhostexCli => {
            match gpui_repair_ghostex_cli_commands() {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
        GpuiGhostexCliSettingsAction::InstallBrowserControl => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["browser", "install-skill"],
                "Ghostex Embedded Browser Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallBrowserUseSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["browser-use", "install-skill"],
                "Ghostex Browser Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallComputerUseSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["computer-use", "install-skill"],
                "Ghostex Computer Use",
            )
        }
        GpuiGhostexCliSettingsAction::InstallAgentOrchestrationSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["agent-orchestration", "install-skill"],
                "Ghostex Agent Orchestration",
            )
        }
        GpuiGhostexCliSettingsAction::InstallFable56OrchestrationSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["fable-5.6-orchestration", "install-skill"],
                "Ghostex Fable 5.6 Orchestration",
            )
        }
        GpuiGhostexCliSettingsAction::InstallFindPrevSessionSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["find-prev-session", "install-skill"],
                "Ghostex Find Previous Session",
            )
        }
        GpuiGhostexCliSettingsAction::InstallGenerateTitleSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["generate-title", "install-skill"],
                "Ghostex Auto Rename Session",
            )
        }
        GpuiGhostexCliSettingsAction::InstallMoveCodexSessionSkill => {
            gpui_install_bundled_ghostex_skill_action(
                action,
                &["move-codex-session", "install-skill"],
                "Ghostex Move Codex Session",
            )
        }
        GpuiGhostexCliSettingsAction::FinishDesktopControlSetup {
            driver_installed,
            was_update,
        } => {
            match gpui_finish_desktop_control_setup(driver_installed, was_update) {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
        GpuiGhostexCliSettingsAction::UninstallBundledAgentSkill(skill_name) => {
            match gpui_uninstall_bundled_agent_skill(skill_name) {
                Ok(true) => GpuiGhostexCliActionResult::success(
                    action,
                    "Bundled Ghostex agent skill uninstalled. You can install it again from Settings."
                        .to_string(),
                ),
                Ok(false) => GpuiGhostexCliActionResult::success(
                    action,
                    "That bundled Ghostex agent skill was already uninstalled. Current integration status was refreshed."
                        .to_string(),
                ),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
        GpuiGhostexCliSettingsAction::UninstallBundledAgentSkills => {
            match gpui_uninstall_bundled_agent_skills() {
                Ok(message) => GpuiGhostexCliActionResult::success(action, message),
                Err(message) => GpuiGhostexCliActionResult::failure(action, message),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GpuiGteInstallActionResult {
    pub(crate) available: bool,
    pub(crate) message: &'static str,
    pub(crate) toast_level: &'static str,
    pub(crate) toast_title: &'static str,
}

pub(crate) fn gpui_gte_homebrew_install_command() -> (&'static str, [&'static str; 2], Duration) {
    (
        "/bin/zsh",
        ["-lc", GPUI_GTE_HOMEBREW_INSTALL_SCRIPT],
        Duration::from_secs(5 * 60),
    )
}

pub(crate) fn gpui_install_gte_from_homebrew() -> GpuiGteInstallActionResult {
    /*
    CDXC:GtePromptEditing 2026-06-24-13:28:
    GPUI Settings must use the same fixed Homebrew resolution order and `maddada/tap/gte` install operation as the macOS Settings button, bounded to five minutes with stdout/stderr suppressed. Installing the binary is separate from selecting the promptEditorBackend, and failures must report generic copy instead of raw Homebrew output, paths, command output, URLs, tokens, or environment.
    */
    let (command, args, timeout) = gpui_gte_homebrew_install_command();
    let result = gpui_run_command_with_timeout(Path::new(command), &args, timeout);
    gpui_gte_install_result_from_command_result(result)
}

pub(crate) fn gpui_gte_install_result_from_command_result(
    result: Result<bool, String>,
) -> GpuiGteInstallActionResult {
    match result {
        Ok(true) => GpuiGteInstallActionResult {
            available: true,
            message: GPUI_GTE_INSTALL_SUCCESS_MESSAGE,
            toast_level: "success",
            toast_title: GPUI_GTE_INSTALL_SUCCESS_MESSAGE,
        },
        Ok(false) | Err(_) => GpuiGteInstallActionResult {
            available: false,
            message: GPUI_GTE_INSTALL_FAILURE_MESSAGE,
            toast_level: "warning",
            toast_title: "gte install failed",
        },
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) const GPUI_CUA_DRIVER_RELEASES_URL: &str =
    "https://github.com/trycua/cua/releases?q=cua-driver-rs&expanded=true";
pub(crate) const GPUI_CUA_DRIVER_INSTALL_COMMAND_ID: &str = "ghostex.gpui.installCuaDriver";
pub(crate) const GPUI_CUA_DRIVER_UPDATE_COMMAND_ID: &str = "ghostex.gpui.updateCuaDriver";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_INSTALL_TAB_TITLE: &str = "Install Cua Driver";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_UPDATE_TAB_TITLE: &str = "Update Cua Driver";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_INSTALL_RUNNING_MESSAGE: &str = "The official Cua Driver installer is running in a command terminal tab. Plugin status updates when it finishes.";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_UPDATE_RUNNING_MESSAGE: &str = "Cua Driver is checking for and applying the latest official update in a command terminal tab. Plugin status updates when it finishes.";

/*
CDXC:GPUIDesktopControlSettings 2026-08-09:
macOS owns the in-app Cua Driver lifecycle. A missing driver runs trycua's
official installer; an existing driver performs a fresh update check and then
uses its canonical self-updater. Windows and Linux intentionally do not run an
installer from Ghostex yet and open the Cua GitHub releases page instead.
*/
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_POSIX_INSTALL_COMMAND: &str =
    "/bin/bash -c \"$(curl -fsSL https://cua.ai/driver/install.sh)\"";
#[cfg(target_os = "macos")]
pub(crate) const GPUI_CUA_DRIVER_START_COMMAND: &str = "/usr/bin/open -n -g -a CuaDriver --args serve";

#[cfg(target_os = "macos")]
pub(crate) struct GpuiCuaDriverCommandAction {
    pub(crate) command: String,
    pub(crate) command_id: &'static str,
    pub(crate) running_message: &'static str,
    pub(crate) tab_title: &'static str,
    pub(crate) toast_title: &'static str,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_cua_driver_command_action() -> GpuiCuaDriverCommandAction {
    if let Some(cua_driver_path) = gpui_cua_driver_executable_path() {
        let executable = gpui_shell_single_quote_path(&cua_driver_path);
        GpuiCuaDriverCommandAction {
            command: format!(
                "{executable} check-update --no-cache && {executable} update --apply && {GPUI_CUA_DRIVER_START_COMMAND}"
            ),
            command_id: GPUI_CUA_DRIVER_UPDATE_COMMAND_ID,
            running_message: GPUI_CUA_DRIVER_UPDATE_RUNNING_MESSAGE,
            tab_title: GPUI_CUA_DRIVER_UPDATE_TAB_TITLE,
            toast_title: "Updating Cua Driver",
        }
    } else {
        GpuiCuaDriverCommandAction {
            command: format!(
                "{GPUI_CUA_DRIVER_POSIX_INSTALL_COMMAND} && {GPUI_CUA_DRIVER_START_COMMAND}"
            ),
            command_id: GPUI_CUA_DRIVER_INSTALL_COMMAND_ID,
            running_message: GPUI_CUA_DRIVER_INSTALL_RUNNING_MESSAGE,
            tab_title: GPUI_CUA_DRIVER_INSTALL_TAB_TITLE,
            toast_title: "Installing Cua Driver",
        }
    }
}

pub(crate) fn gpui_finish_desktop_control_setup(
    driver_installed: bool,
    was_update: bool,
) -> Result<String, String> {
    /*
    CDXC:GPUIDesktopControlSettings 2026-08-09:
    The Cua Driver installer/updater runs in a visible command-pane terminal.
    This completion step installs the bundled Ghostex Computer Use skill through
    the fixed ownership-verified Ghostex CLI helper and reports command failure
    without claiming Desktop Control is ready.
    */
    if !driver_installed {
        return Err(
            if was_update {
                "The Cua Driver update did not finish successfully. Its terminal tab shows what happened; plugin status was refreshed."
            } else {
                "The Cua Driver installer did not finish successfully. Its terminal tab shows what happened; plugin status was refreshed."
            }
            .to_string(),
        );
    }

    match gpui_install_bundled_ghostex_skill(
        &["computer-use", "install-skill"],
        "Ghostex Computer Use",
    ) {
        Ok(_) => Ok(if was_update {
            "Cua Driver is up to date. Ghostex Computer Use is ready.".to_string()
        } else {
            "Cua Driver installed. Grant macOS Accessibility and Screen Recording permissions if needed."
                .to_string()
        }),
        Err(message) => Err(format!(
            "Cua Driver {}, but Ghostex Computer Use skill could not be installed. {message}",
            if was_update { "updated" } else { "installed" }
        )),
    }
}

pub(crate) fn gpui_repair_ghostex_cli_commands() -> Result<String, String> {
    /*
    CDXC:GPUISettingsCliInstall 2026-06-24-12:56:
    CLI repair is real only when GPUI is running from a packaged app that ships the native `Contents/Resources/CLI/ghostex` binary. Development binaries must report unavailable status instead of synthesizing wrappers to a source checkout, while packaged repair writes public wrappers outside the app and replaces only marked Ghostex wrappers, app-owned CLI symlinks, or broken symlinks.
    */
    let cli_dir = gpui_bundled_ghostex_cli_resource_dir()?;
    let cli_binary_path = cli_dir.join("ghostex");
    let path_entries = gpui_cli_path_entries();
    let common_dirs = gpui_common_cli_install_dirs();
    let install_dirs = gpui_cli_install_dirs(&path_entries, &common_dirs, &cli_dir);

    let ghostex_result =
        gpui_install_ghostex_cli_command("ghostex", &cli_binary_path, &cli_dir, &install_dirs);
    if !ghostex_result.installed() {
        return match ghostex_result {
            GpuiCliCommandInstallResult::Blocked => Err(
                "A ghostex command already exists in a writable install location, but GPUI could not prove it belongs to Ghostex. No unrelated command was overwritten."
                    .to_string(),
            ),
            _ => Err(
                "Ghostex could not create a writable PATH wrapper for the ghostex command. Current CLI status was refreshed without changing files."
                    .to_string(),
            ),
        };
    }

    let gx_result =
        gpui_install_ghostex_cli_command("gx", &cli_binary_path, &cli_dir, &install_dirs);
    if gx_result == GpuiCliCommandInstallResult::Blocked {
        return Ok(
            "Ghostex CLI linked. The gx alias was not changed because another command already owns that name."
                .to_string(),
        );
    }
    if !gx_result.installed() {
        return Ok(
            "Ghostex CLI linked. The gx alias could not be linked because no writable install location was available."
                .to_string(),
        );
    }
    Ok(
        "Ghostex CLI linked. ghostex and gx now launch this GPUI app build where available."
            .to_string(),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiCliCommandInstallResult {
    Current,
    Repaired,
    Blocked,
    Unavailable,
}

impl GpuiCliCommandInstallResult {
    pub(crate) fn installed(self) -> bool {
        matches!(self, Self::Current | Self::Repaired)
    }
}

pub(crate) fn gpui_bundled_ghostex_cli_resource_dir() -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|_| {
        "Packaged Ghostex CLI resources are unavailable in this GPUI build. Current integration status was refreshed without changing files."
            .to_string()
    })?;
    let Some(bundle_root) = find_app_bundle_root(&executable) else {
        return Err(
            "Packaged Ghostex CLI resources are unavailable in this GPUI build. Current integration status was refreshed without changing files."
                .to_string(),
        );
    };
    let cli_dir = bundle_root.join("Contents/Resources/CLI");
    if gpui_is_file(&cli_dir.join("ghostex")) {
        Ok(cli_dir)
    } else {
        Err(
            "Packaged Ghostex CLI resources are unavailable in this GPUI build. Current integration status was refreshed without changing files."
                .to_string(),
        )
    }
}

pub(crate) fn gpui_install_ghostex_cli_command(
    command: &str,
    cli_binary_path: &Path,
    cli_dir: &Path,
    install_dirs: &[PathBuf],
) -> GpuiCliCommandInstallResult {
    let wrapper = gpui_ghostex_cli_wrapper_content(cli_binary_path);
    for directory in install_dirs {
        if !gpui_prepare_cli_install_directory(directory) {
            continue;
        }
        let link_path = directory.join(command);
        if gpui_path_exists_or_is_symlink(&link_path) {
            if !gpui_can_replace_existing_ghostex_command(command, &link_path, cli_dir) {
                return GpuiCliCommandInstallResult::Blocked;
            }
            if gpui_is_regular_file(&link_path)
                && fs::read_to_string(&link_path)
                    .map(|content| content == wrapper)
                    .unwrap_or(false)
            {
                let _ = gpui_set_executable_permissions(&link_path);
                gpui_clear_macos_execution_policy_xattrs(&link_path);
                return GpuiCliCommandInstallResult::Current;
            }
            if fs::remove_file(&link_path).is_err() {
                return GpuiCliCommandInstallResult::Unavailable;
            }
        }
        if gpui_write_executable_wrapper(&link_path, &wrapper).is_ok() {
            gpui_clear_macos_execution_policy_xattrs(&link_path);
            return GpuiCliCommandInstallResult::Repaired;
        }
    }
    GpuiCliCommandInstallResult::Unavailable
}

pub(crate) fn gpui_ghostex_cli_wrapper_content(cli_binary_path: &Path) -> String {
    /*
    CDXC:GhostexRustCli 2026-07-13:
    The bundled CLI is the native Rust `ghostex` binary (Contents/Resources/
    CLI/ghostex); wrappers exec it directly with no Node runtime. The wrapper
    file (rather than a symlink) is kept so macOS policy assessment does not
    execute app-bundled content directly and ownership stays marker-provable.
    */
    [
        "#!/bin/bash".to_string(),
        "set -euo pipefail".to_string(),
        format!("# {GPUI_GHOSTEX_CLI_WRAPPER_MARKER}: Public PATH commands live outside the app bundle so macOS does not directly execute app-bundled shell scripts during policy assessment."),
        format!(
            "exec {} \"$@\"",
            gpui_shell_single_quote_path(cli_binary_path)
        ),
        String::new(),
    ]
    .join("\n")
}

pub(crate) fn gpui_shell_single_quote_path(path: &Path) -> String {
    format!("'{}'", gpui_path_string(path).replace('\'', "'\\''"))
}

pub(crate) fn gpui_cli_path_entries() -> Vec<PathBuf> {
    gpui_unique_paths(
        env::var_os("PATH")
            .into_iter()
            .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
            .filter(|path| !path.as_os_str().is_empty()),
    )
}

pub(crate) fn gpui_common_cli_install_dirs() -> Vec<PathBuf> {
    gpui_unique_paths([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        gpui_home_dir().join(".local/bin"),
    ])
}

pub(crate) fn gpui_cli_install_dirs(
    path_entries: &[PathBuf],
    common_dirs: &[PathBuf],
    cli_dir: &Path,
) -> Vec<PathBuf> {
    let mut owned_dirs = Vec::new();
    for command in ["ghostex", "gx"] {
        for candidate in gpui_cli_command_path_candidates(command, path_entries, common_dirs) {
            if gpui_path_exists_or_is_symlink(&candidate)
                && gpui_is_ghostex_owned_command_path(command, &candidate, cli_dir)
            {
                if let Some(parent) = candidate.parent() {
                    owned_dirs.push(parent.to_path_buf());
                }
            }
        }
    }
    gpui_unique_paths(
        owned_dirs
            .into_iter()
            .chain(path_entries.iter().cloned())
            .chain(common_dirs.iter().cloned()),
    )
}

pub(crate) fn gpui_cli_command_path_candidates(
    command: &str,
    path_entries: &[PathBuf],
    common_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let found = gpui_which_command(command).into_iter();
    gpui_unique_paths(
        found
            .chain(path_entries.iter().map(|directory| directory.join(command)))
            .chain(common_dirs.iter().map(|directory| directory.join(command))),
    )
}

pub(crate) fn gpui_unique_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        if path.as_os_str().is_empty() {
            continue;
        }
        if seen.insert(path.clone()) {
            result.push(path);
        }
    }
    result
}

pub(crate) fn gpui_prepare_cli_install_directory(directory: &Path) -> bool {
    let user_bin = gpui_home_dir().join(".local/bin");
    if directory == user_bin.as_path() && fs::create_dir_all(directory).is_err() {
        return false;
    }
    if !gpui_is_dir(directory) {
        return false;
    }
    gpui_directory_accepts_temporary_write(directory)
}

pub(crate) fn gpui_directory_accepts_temporary_write(directory: &Path) -> bool {
    let probe_path = directory.join(format!(
        ".ghostex-gpui-cli-write-test-{}-{}",
        std::process::id(),
        system_time_epoch_millis_string(SystemTime::now())
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
    {
        Ok(_) => {
            let _ = fs::remove_file(&probe_path);
            true
        }
        Err(_) => false,
    }
}

pub(crate) fn gpui_can_replace_existing_ghostex_command(command: &str, path: &Path, cli_dir: &Path) -> bool {
    gpui_is_ghostex_owned_command_path(command, path, cli_dir) || gpui_is_broken_symlink(path)
}

pub(crate) fn gpui_is_ghostex_owned_command_path(command: &str, path: &Path, cli_dir: &Path) -> bool {
    if gpui_file_contains_ghostex_cli_wrapper_marker(path) {
        return true;
    }
    let realpath = gpui_realpath_or_self(path);
    if gpui_path_is_relative_to(&realpath, cli_dir) {
        return true;
    }
    gpui_is_ghostex_app_owned_command_realpath(command, &realpath)
}

pub(crate) fn gpui_is_ghostex_app_owned_command_realpath(command: &str, realpath: &Path) -> bool {
    let normalized = gpui_path_string(realpath).to_lowercase();
    normalized.contains(&format!("/ghostex.app/contents/resources/cli/{command}"))
        || normalized.contains(&format!(
            "/ghostex.app/contents/resources/web/cli/{command}"
        ))
        || normalized.contains("/ghostex.app/contents/resources/cli/ghostex-cli.mjs")
        || (command == "ghostex" && normalized.contains("/ghostex.app/contents/macos/ghostex"))
}

pub(crate) fn gpui_is_marked_ghostex_wrapper_file(path: &Path) -> bool {
    if !gpui_is_regular_file(path) {
        return false;
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.len() > 128 * 1024 {
        return false;
    }
    fs::read_to_string(path)
        .map(|content| gpui_marked_ghostex_wrapper_content(&content))
        .unwrap_or(false)
}

pub(crate) fn gpui_is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

pub(crate) fn gpui_path_exists_or_is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub(crate) fn gpui_is_broken_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink() && !path.exists())
        .unwrap_or(false)
}

pub(crate) fn gpui_realpath_or_self(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn gpui_write_executable_wrapper(path: &Path, content: &str) -> std::io::Result<()> {
    fs::write(path, content.as_bytes())?;
    gpui_set_executable_permissions(path)
}

pub(crate) fn gpui_set_executable_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub(crate) fn gpui_clear_macos_execution_policy_xattrs(path: &Path) {
    /*
    CDXC:GPUISettingsCliInstall 2026-06-24-12:56:
    Repaired public CLI wrappers may inherit macOS execution-policy xattrs from a previous install location. Clear only the two known assessment attributes after wrapper writes/replacements, suppress command output, and keep repair success independent from xattr removal failures.
    */
    #[cfg(target_os = "macos")]
    {
        for attribute in ["com.apple.provenance", "com.apple.quarantine"] {
            let _ = std::process::Command::new("/usr/bin/xattr")
                .arg("-d")
                .arg(attribute)
                .arg(path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

pub(crate) fn gpui_install_bundled_ghostex_skill_action(
    action: GpuiGhostexCliSettingsAction,
    args: &[&str],
    display_name: &str,
) -> GpuiGhostexCliActionResult {
    match gpui_install_bundled_ghostex_skill(args, display_name) {
        Ok(message) => GpuiGhostexCliActionResult::success(action, message),
        Err(message) => GpuiGhostexCliActionResult::failure(action, message),
    }
}

pub(crate) fn gpui_install_bundled_ghostex_skill(args: &[&str], display_name: &str) -> Result<String, String> {
    /*
    CDXC:GPUISettingsAgentSkills 2026-06-24-12:56:
    GPUI Settings installs bundled Ghostex skills by resolving the fixed `ghostex` command on PATH and running only the known `ghostex <namespace> install-skill` argv. Command text never comes from React, child stdout/stderr are suppressed, failures are reported generically, and status is refreshed from disk afterward.

    CDXC:GPUISettingsAgentSkills 2026-06-24-13:08:
    Executing a PATH `ghostex` command from Settings requires strict Ghostex ownership evidence: a repair marker plus `ghostex-cli.mjs`, or an app-owned realpath recognized by the CLI repair ownership helper. Broad read-only status strings are not sufficient for process execution.
    */
    let Some(ghostex_path) = gpui_which_command("ghostex") else {
        return Err(
            "Ghostex CLI was not found on PATH. Repair the Ghostex CLI before installing bundled skills."
                .to_string(),
        );
    };
    if !gpui_is_probably_ghostex_command(&ghostex_path, "ghostex") {
        return Err(
            "A ghostex command exists on PATH, but GPUI could not prove it belongs to Ghostex. Repair the Ghostex CLI before installing bundled skills."
                .to_string(),
        );
    }
    match gpui_run_command_with_timeout(&ghostex_path, args, Duration::from_secs(120)) {
        Ok(true) => Ok(format!("{display_name} installed.")),
        Ok(false) => Err(format!(
            "{display_name} install failed. Current integration status was refreshed."
        )),
        Err(_) => Err(format!(
            "{display_name} install could not be started. Current integration status was refreshed."
        )),
    }
}

pub(crate) fn gpui_run_command_with_timeout(
    command: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<bool, String> {
    let mut child = std::process::Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "process spawn failed".to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(false);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiCapturedCommandOutput {
    pub(crate) stderr: String,
    pub(crate) stdout: String,
    pub(crate) success: bool,
}

impl GpuiCapturedCommandOutput {
    pub(crate) fn combined_text(&self) -> String {
        if self.stderr.trim().is_empty() {
            return self.stdout.clone();
        }
        if self.stdout.trim().is_empty() {
            return self.stderr.clone();
        }
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

pub(crate) fn gpui_run_command_with_captured_output_timeout(
    command: &Path,
    args: &[&str],
    timeout: Duration,
    max_capture_bytes: usize,
) -> Result<GpuiCapturedCommandOutput, String> {
    let mut child = std::process::Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "process spawn failed".to_string())?;
    let stdout_reader = child
        .stdout
        .take()
        .map(|stream| gpui_capture_child_output_stream(stream, max_capture_bytes));
    let stderr_reader = child
        .stderr
        .take()
        .map(|stream| gpui_capture_child_output_stream(stream, max_capture_bytes));
    let started = Instant::now();
    let mut success = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                success = status.success();
                break;
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let stdout = gpui_join_captured_output(stdout_reader);
    let stderr = gpui_join_captured_output(stderr_reader);
    Ok(GpuiCapturedCommandOutput {
        stderr,
        stdout,
        success,
    })
}

pub(crate) fn gpui_capture_child_output_stream<R>(
    mut stream: R,
    max_capture_bytes: usize,
) -> std::thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut captured = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(byte_count) => {
                    let remaining = max_capture_bytes.saturating_sub(captured.len());
                    if remaining > 0 {
                        captured.extend_from_slice(&buffer[..byte_count.min(remaining)]);
                    }
                }
                Err(_) => break,
            }
        }
        captured
    })
}

pub(crate) fn gpui_join_captured_output(reader: Option<std::thread::JoinHandle<Vec<u8>>>) -> String {
    reader
        .and_then(|reader| reader.join().ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_os_integration_bundle_info() -> Option<GpuiOSIntegrationBundleInfo> {
    let executable = env::current_exe().ok()?;
    let bundle_root = find_app_bundle_root(&executable)?;
    let info_plist = fs::read_to_string(bundle_root.join("Contents/Info.plist")).ok()?;
    let bundle_identifier = gpui_plist_string_value(&info_plist, "CFBundleIdentifier")?;
    Some(GpuiOSIntegrationBundleInfo {
        bundle_identifier,
        bundle_root,
        info_plist,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_plist_string_value(plist: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let after_key = plist.split_once(&key_marker)?.1;
    let after_string = after_key.split_once("<string>")?.1;
    let value = after_string.split_once("</string>")?.0.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_has_editable_registration(info_plist: &str) -> bool {
    info_plist.contains("<key>CFBundleDocumentTypes</key>")
        && info_plist.contains("<key>CFBundleTypeRole</key>")
        && info_plist.contains("<string>Editor</string>")
        && (info_plist.contains("<string>*</string>")
            || info_plist.contains("<string>public.text</string>")
            || info_plist.contains("<string>public.source-code</string>"))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_has_script_registration(info_plist: &str) -> bool {
    info_plist.contains("<key>CFBundleDocumentTypes</key>")
        && info_plist.contains("<key>CFBundleTypeRole</key>")
        && info_plist.contains("<string>Shell</string>")
        && GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS
            .iter()
            .all(|file_extension| {
                info_plist.contains(&format!("<string>{file_extension}</string>"))
            })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_has_ghostex_url_registration(info_plist: &str) -> bool {
    info_plist.contains("<key>CFBundleURLTypes</key>")
        && info_plist.contains("<key>CFBundleURLSchemes</key>")
        && info_plist.contains("<string>ghostex</string>")
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_default_role_handlers(extensions: &[&str], role: u32) -> serde_json::Value {
    let handlers = extensions
        .iter()
        .filter_map(|file_extension| {
            let content_type = gpui_macos_content_type_for_extension(file_extension)?;
            let handler =
                unsafe { LSCopyDefaultRoleHandlerForContentType(content_type.as_ref(), role) };
            let handler = gpui_cf_string_to_string_and_release(handler)?;
            Some((
                (*file_extension).to_string(),
                serde_json::Value::String(handler),
            ))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    serde_json::Value::Object(handlers)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_default_url_scheme_handler(scheme: &str) -> Option<String> {
    let scheme = GpuiCfString::new(scheme)?;
    let handler = unsafe { LSCopyDefaultHandlerForURLScheme(scheme.as_ref()) };
    gpui_cf_string_to_string_and_release(handler)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_register_os_integration_bundle(bundle_root: &Path) -> Option<i32> {
    let path = GpuiCfString::new(&bundle_root.to_string_lossy())?;
    let url = unsafe {
        CFURLCreateWithFileSystemPath(
            std::ptr::null(),
            path.as_ref(),
            K_CF_URL_POSIX_PATH_STYLE,
            1,
        )
    };
    if url.is_null() {
        return None;
    }
    let status = unsafe { LSRegisterURL(url, 1) };
    unsafe {
        CFRelease(url);
    }
    Some(status)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_content_type_for_extension(file_extension: &str) -> Option<GpuiCfString> {
    let tag = GpuiCfString::new(file_extension)?;
    let content_type = unsafe {
        UTTypeCreatePreferredIdentifierForTag(
            kUTTagClassFilenameExtension,
            tag.as_ref(),
            std::ptr::null(),
        )
    };
    GpuiCfString::from_owned(content_type)
}

#[cfg(target_os = "macos")]
pub(crate) struct GpuiCfString(CFStringRef);

#[cfg(target_os = "macos")]
impl GpuiCfString {
    pub(crate) fn new(value: &str) -> Option<Self> {
        let c_value = std::ffi::CString::new(value).ok()?;
        let string = unsafe {
            CFStringCreateWithCString(
                std::ptr::null(),
                c_value.as_ptr(),
                K_CF_STRING_ENCODING_UTF8,
            )
        };
        Self::from_owned(string)
    }

    pub(crate) fn from_owned(value: CFStringRef) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }

    pub(crate) fn as_ref(&self) -> CFStringRef {
        self.0
    }
}

#[cfg(target_os = "macos")]
impl Drop for GpuiCfString {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_cf_string_to_string_and_release(value: CFStringRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let converted = gpui_cf_string_to_string(value);
    unsafe {
        CFRelease(value);
    }
    converted
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_cf_string_to_string(value: CFStringRef) -> Option<String> {
    let direct = unsafe { CFStringGetCStringPtr(value, K_CF_STRING_ENCODING_UTF8) };
    if !direct.is_null() {
        return unsafe { std::ffi::CStr::from_ptr(direct) }
            .to_str()
            .ok()
            .map(str::to_string);
    }

    let length = unsafe { CFStringGetLength(value) };
    let max_size = unsafe { CFStringGetMaximumSizeForEncoding(length, K_CF_STRING_ENCODING_UTF8) };
    if max_size < 0 {
        return None;
    }
    let mut buffer = vec![0i8; (max_size as usize).saturating_add(1)];
    let ok = unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        )
    } != 0;
    if !ok {
        return None;
    }
    unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .ok()
        .map(str::to_string)
}

#[cfg(target_os = "macos")]
type CFStringRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type CFURLRef = *const std::ffi::c_void;

#[cfg(target_os = "macos")]
pub(crate) const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
#[cfg(target_os = "macos")]
pub(crate) const K_CF_URL_POSIX_PATH_STYLE: isize = 0;
#[cfg(target_os = "macos")]
pub(crate) const K_LS_ROLES_EDITOR: u32 = 0x0000_0004;
#[cfg(target_os = "macos")]
pub(crate) const K_LS_ROLES_SHELL: u32 = 0x0000_0008;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    static kUTTagClassFilenameExtension: CFStringRef;

    pub(crate) fn CFRelease(cf: *const std::ffi::c_void);
    pub(crate) fn CFStringCreateWithCString(
        allocator: *const std::ffi::c_void,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;
    pub(crate) fn CFStringGetCStringPtr(the_string: CFStringRef, encoding: u32) -> *const std::ffi::c_char;
    pub(crate) fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut std::ffi::c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    pub(crate) fn CFStringGetLength(the_string: CFStringRef) -> isize;
    pub(crate) fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    pub(crate) fn CFURLCreateWithFileSystemPath(
        allocator: *const std::ffi::c_void,
        file_path: CFStringRef,
        path_style: isize,
        is_directory: u8,
    ) -> CFURLRef;
    pub(crate) fn UTTypeCreatePreferredIdentifierForTag(
        tag_class: CFStringRef,
        tag: CFStringRef,
        conforming_to_uti: CFStringRef,
    ) -> CFStringRef;
    pub(crate) fn LSCopyDefaultRoleHandlerForContentType(content_type: CFStringRef, role: u32) -> CFStringRef;
    pub(crate) fn LSSetDefaultRoleHandlerForContentType(
        content_type: CFStringRef,
        role: u32,
        handler_bundle_id: CFStringRef,
    ) -> i32;
    pub(crate) fn LSCopyDefaultHandlerForURLScheme(url_scheme: CFStringRef) -> CFStringRef;
    pub(crate) fn LSSetDefaultHandlerForURLScheme(
        url_scheme: CFStringRef,
        handler_bundle_id: CFStringRef,
    ) -> i32;
    pub(crate) fn LSRegisterURL(url: CFURLRef, update: u8) -> i32;
}

pub(crate) fn gpui_ghostex_folder_stats_message() -> serde_json::Value {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:36:
    Settings storage stats may inspect only the GPUI-resolved Ghostex home and its immediate child directories. Do not trust React-provided paths, follow symlink directories, write logs, or scan unrelated project/workspace trees.
    */
    let folder_path = shared_settings::ghostex_storage_paths().data_dir.clone();
    let folder_path_string = gpui_path_string(&folder_path);
    if !gpui_is_dir(&folder_path) {
        return serde_json::json!({
            "errorMessage": "Ghostex home is not available for GPUI folder stats.",
            "folderPath": folder_path_string,
            "folders": [],
            "generatedAt": gpui_status_generated_at(),
            "totalBytes": 0,
            "type": "ghostexFolderStats",
        });
    }

    let mut folders = fs::read_dir(&folder_path)
        .map(|children| {
            children
                .filter_map(Result::ok)
                .filter_map(|child| {
                    let metadata = fs::symlink_metadata(child.path()).ok()?;
                    if !metadata.is_dir() {
                        return None;
                    }
                    let path = child.path();
                    let name = child.file_name().to_string_lossy().to_string();
                    let size_bytes = gpui_directory_size_bytes(&path);
                    Some(serde_json::json!({
                        "name": name,
                        "path": gpui_path_string(&path),
                        "sizeBytes": size_bytes,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    folders.sort_by(|left, right| {
        let left_size = left
            .get("sizeBytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let right_size = right
            .get("sizeBytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let left_name = left
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let right_name = right
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        right_size
            .cmp(&left_size)
            .then_with(|| left_name.cmp(right_name))
    });
    let total_bytes = folders
        .iter()
        .filter_map(|folder| folder.get("sizeBytes").and_then(serde_json::Value::as_u64))
        .sum::<u64>();

    serde_json::json!({
        "folderPath": folder_path_string,
        "folders": folders,
        "generatedAt": gpui_status_generated_at(),
        "totalBytes": total_bytes,
        "type": "ghostexFolderStats",
    })
}

/// Every queue row held for this session, plus how many of them are `failed`.
/// A `failed` row waits on the user rather than on the agent, but it still
/// counts: a queue stalled behind one has stopped dead, and hiding it would make
/// that look identical to no queue. This mirrors the sidebar badge's
/// `queuedPromptCount` / `queuedPromptFailedCount` exactly — one count, one
/// meaning, on every surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSessionChatQueuedCounts {
    pub(crate) total: usize,
    pub(crate) failed: usize,
}

pub(crate) fn gpui_session_chat_queued_counts_from_result(
    value: &serde_json::Value,
) -> GpuiSessionChatQueuedCounts {
    let Some(rows) = value.get("queue").and_then(serde_json::Value::as_array) else {
        return GpuiSessionChatQueuedCounts::default();
    };
    GpuiSessionChatQueuedCounts {
        total: rows.len(),
        failed: rows
            .iter()
            .filter(|row| row.get("state").and_then(serde_json::Value::as_str) == Some("failed"))
            .count(),
    }
}

pub(crate) fn gpui_spawn_zmx_refresh_if_stale_process(
    session_name: Option<String>,
    grid_size: Option<(u16, u16)>,
    reason: &'static str,
) {
    let Some(session_name) = session_name.filter(|name| !name.trim().is_empty()) else {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.zmxPersistenceViewportRefresh.ifStale",
            serde_json::json!({
                "didRequest": false,
                "reason": reason,
                "skipReason": "missingSessionName",
            }),
        );
        return;
    };
    let Some((rows, columns)) = grid_size.filter(|(rows, columns)| *rows > 0 && *columns > 0)
    else {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.zmxPersistenceViewportRefresh.ifStale",
            serde_json::json!({
                "didRequest": false,
                "reason": reason,
                "skipReason": "invalidSurfaceSize",
            }),
        );
        return;
    };
    #[cfg(target_os = "windows")]
    {
        let Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { distribution }) =
            windows_terminal_backend::resolve_current()
        else {
            support_logs::append(
                support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.zmxPersistenceViewportRefresh.ifStale",
                serde_json::json!({
                    "didRequest": false,
                    "reason": reason,
                    "skipReason": "windowsPowerShellBackend",
                }),
            );
            return;
        };
        std::thread::spawn(move || {
            let spawned = windows_terminal_backend::spawn_zmx_refresh(
                distribution.as_str(),
                session_name.as_str(),
                rows,
                columns,
            );
            let Ok(child) = spawned else {
                gpui_record_zmx_refresh_launch_failure(rows, columns, reason);
                return;
            };
            gpui_monitor_zmx_refresh_process(child, rows, columns, reason);
        });
        return;
    }
    #[cfg(not(target_os = "windows"))]
    let Some(zmx_path) = gpui_resolve_local_zmx_binary() else {
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.zmxPersistenceViewportRefresh.ifStale",
            serde_json::json!({
                "didRequest": false,
                "reason": reason,
                "skipReason": "missingBundledZmx",
            }),
        );
        return;
    };
    #[cfg(not(target_os = "windows"))]
    std::thread::spawn(move || {
        let spawned = Command::new(&zmx_path)
            .args([
                "refresh-if-stale",
                session_name.as_str(),
                rows.to_string().as_str(),
                columns.to_string().as_str(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(child) = spawned else {
            gpui_record_zmx_refresh_launch_failure(rows, columns, reason);
            return;
        };
        gpui_monitor_zmx_refresh_process(child, rows, columns, reason);
    });
}

pub(crate) fn gpui_record_zmx_refresh_launch_failure(rows: u16, columns: u16, reason: &'static str) {
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.zmxPersistenceViewportRefresh.ifStale",
        serde_json::json!({
            "columns": columns,
            "didLaunch": false,
            "didRequest": true,
            "reason": reason,
            "rows": rows,
        }),
    );
}

pub(crate) fn gpui_monitor_zmx_refresh_process(
    mut child: std::process::Child,
    rows: u16,
    columns: u16,
    reason: &'static str,
) {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut timed_out = false;
    let mut exit_code: i32 = -1;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code().unwrap_or(-1);
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.zmxPersistenceViewportRefresh.ifStale",
        serde_json::json!({
            "columns": columns,
            "didLaunch": true,
            "didRequest": true,
            "exitCode": exit_code,
            "reason": reason,
            "rows": rows,
            "timedOut": timed_out,
        }),
    );
}

pub(crate) fn gpui_file_contains_ghostex_cli_wrapper_marker(path: &Path) -> bool {
    gpui_is_marked_ghostex_wrapper_file(path)
}

pub(crate) fn gpui_current_bundle_cli_dir_for_ownership_probe() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let bundle_root = find_app_bundle_root(&executable)?;
    let cli_dir = bundle_root.join("Contents/Resources/CLI");
    gpui_is_dir(&cli_dir).then_some(cli_dir)
}

pub(crate) fn gpui_marked_ghostex_wrapper_content(content: &str) -> bool {
    /*
    CDXC:GhostexRustCli 2026-07-13:
    New wrappers exec the bundled native `Resources/CLI/ghostex` binary; the
    legacy `ghostex-cli.mjs` form stays recognized so repair can replace
    wrappers written by pre-cutover app builds.
    */
    content.contains(GPUI_GHOSTEX_CLI_WRAPPER_MARKER)
        && (content.contains("ghostex-cli.mjs") || content.contains("/Resources/CLI/ghostex"))
}

pub(crate) fn gpui_is_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

pub(crate) fn gpui_is_dir(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

pub(crate) fn gpui_is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) fn gpui_directory_size_bytes(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .map(|children| {
            children
                .filter_map(Result::ok)
                .map(|child| gpui_directory_size_bytes(&child.path()))
                .sum()
        })
        .unwrap_or(0)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiAppModalProductState {
    pub(crate) pinned_prompts: Vec<GpuiPinnedPrompt>,
    pub(crate) scratch_pad_content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiPinnedPrompt {
    pub(crate) content: String,
    pub(crate) created_at: String,
    pub(crate) prompt_id: String,
    pub(crate) title: String,
    pub(crate) updated_at: String,
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn source_code_server_spawn_runtime(
    target: &SourceCodeServerRuntimeTarget,
    settings: &SourceCodeServerRuntimeSettings,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, String> {
    if matches!(
        target.endpoint,
        SourceCodeServerRuntimeEndpoint::Remote { .. }
    ) {
        #[cfg(target_os = "macos")]
        return source_code_server_spawn_remote_runtime(target, startup_deadline);
        #[cfg(not(target_os = "macos"))]
        return Err(
            "Remote Source runtime is available only from the macOS SSH owner.".to_string(),
        );
    }
    if source_code_server_health_check() {
        let remaining = startup_deadline.saturating_duration_since(Instant::now());
        let _ = source_code_server_wait_until_not_responsive(
            SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL.min(remaining),
        );
    }

    let repo_root = source_code_server_resolve_repo_root()?;
    let entrypoint = repo_root.join("out/node/entry.js");
    let node_path = source_code_server_resolve_node_path(&repo_root)?;
    let (user_data_dir, extensions_dir) = source_code_server_runtime_storage()?;
    if source_code_server_should_seed_default_theme(settings) {
        source_code_server_ensure_default_theme(&user_data_dir)?;
    }
    if Instant::now() >= startup_deadline {
        return Err("Source runtime startup timed out".to_string());
    }

    let mut command = Command::new(&node_path);
    command
        .arg(&entrypoint)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(&target.project_path)
        .envs(source_code_server_runtime_environment(&repo_root));
    if let Some(vscode_user_config_dir) = settings.linked_vscode_user_config_dir() {
        command
            .arg("--link-vscode-user-config")
            .arg("--vscode-user-config-dir")
            .arg(vscode_user_config_dir);
    }
    command
        .arg("--auth")
        .arg("none")
        .arg("--bind-addr")
        .arg(format!(
            "{}:{}",
            SOURCE_CODE_SERVER_EDITOR_HOST, SOURCE_CODE_SERVER_EDITOR_PORT
        ))
        .arg("--disable-telemetry")
        .arg("--disable-update-check")
        .arg("--disable-workspace-trust")
        .arg("--disable-getting-started-override")
        .arg("--ignore-last-opened")
        .arg("--app-name")
        .arg("ghostex Code")
        .arg("--user-data-dir")
        .arg(&user_data_dir)
        .arg("--extensions-dir")
        .arg(&extensions_dir);

    let started_at = Instant::now();
    let child = command
        .spawn()
        .map_err(|_| "failed to start Source runtime".to_string())?;
    let readiness = source_code_server_wait_until_responsive(
        startup_deadline.saturating_duration_since(Instant::now()),
    );
    Ok(SourceCodeServerRuntimeStartOutput {
        child,
        runtime_origin: SOURCE_CODE_SERVER_EDITOR_ORIGIN.to_string(),
        prompt_editor_ipc_ready: readiness.prompt_editor_ipc_ready,
        started_at,
        http_runtime_ready: readiness.http_runtime_ready,
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn source_code_server_spawn_runtime(
    target: &SourceCodeServerRuntimeTarget,
    settings: &SourceCodeServerRuntimeSettings,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, String> {
    /*
    CDXC:GPUISourceWindowsWsl 2026-07-26:
    Windows projects and their authoritative paths live inside the selected
    WSL2 distribution. Launch code-server there as well; a native Windows
    Node child cannot use the WSL project path or the Linux runtime payload.
    The packaged Linux runtime is activated in WSL during startup; project and
    launch parameters cross the boundary as argv values, while the fixed WSL
    script owns Linux storage, environment, and working-directory setup.
    */
    if source_code_server_health_check() {
        let remaining = startup_deadline.saturating_duration_since(Instant::now());
        let _ = source_code_server_wait_until_not_responsive(
            SOURCE_CODE_SERVER_PORT_BUSY_WAIT_INTERVAL.min(remaining),
        );
    }

    if Instant::now() >= startup_deadline {
        return Err("Source runtime startup timed out".to_string());
    }
    let bind_address = format!(
        "{}:{}",
        SOURCE_CODE_SERVER_EDITOR_HOST, SOURCE_CODE_SERVER_EDITOR_PORT
    );
    let mut command = windows_terminal_backend::source_code_server_command(
        &target.project_path,
        SOURCE_CODE_SERVER_DEFAULT_NODE_MAJOR,
        &bind_address,
        settings.link_vscode_user_config,
        settings.use_vscode_insiders_user_config,
    )?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let started_at = Instant::now();
    let child = command
        .spawn()
        .map_err(|_| "failed to start Source runtime in WSL".to_string())?;
    let readiness = source_code_server_wait_until_responsive(
        startup_deadline.saturating_duration_since(Instant::now()),
    );
    Ok(SourceCodeServerRuntimeStartOutput {
        child,
        runtime_origin: SOURCE_CODE_SERVER_EDITOR_ORIGIN.to_string(),
        prompt_editor_ipc_ready: readiness.prompt_editor_ipc_ready,
        started_at,
        http_runtime_ready: readiness.http_runtime_ready,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarNativeAppShotPromptMessage {
    pub(crate) prompt: String,
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSessionAttentionNotificationCandidate {
    pub(crate) body: String,
    pub(crate) icon_data_url: Option<String>,
    pub(crate) session_id: String,
    pub(crate) title: String,
}

#[derive(Debug, Default)]
pub(crate) struct GpuiSessionAttentionNotificationRateLimiter {
    pub(crate) global_window_count: usize,
    pub(crate) global_window_started_at: Option<Instant>,
    pub(crate) session_last_sent_at: HashMap<String, Instant>,
}

impl GpuiSessionAttentionNotificationRateLimiter {
    pub(crate) fn consume(&mut self, session_id: &str, now: Instant) -> bool {
        if self
            .session_last_sent_at
            .get(session_id)
            .is_some_and(|previous| {
                now.duration_since(*previous) < GPUI_SESSION_ATTENTION_NOTIFICATION_SESSION_COOLDOWN
            })
        {
            return false;
        }

        if self.global_window_started_at.is_none_or(|started_at| {
            now.duration_since(started_at) >= GPUI_SESSION_ATTENTION_NOTIFICATION_GLOBAL_WINDOW
        }) {
            self.global_window_started_at = Some(now);
            self.global_window_count = 0;
        }
        if self.global_window_count >= GPUI_SESSION_ATTENTION_NOTIFICATION_GLOBAL_LIMIT {
            return false;
        }

        self.global_window_count += 1;
        self.session_last_sent_at
            .insert(session_id.to_string(), now);
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiMenuBarStatusItemState {
    pub(crate) attention_count: u64,
    pub(crate) available_count: u64,
    pub(crate) working_count: u64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct GpuiMenuBarStatusNativeSessionEntry {
    pub(crate) session_id: *const std::ffi::c_char,
    pub(crate) title: *const std::ffi::c_char,
    pub(crate) last_active_at: *const std::ffi::c_char,
    pub(crate) status: i32,
    pub(crate) order: u64,
}

pub(crate) fn gpui_session_attention_notification_candidates(
    previous: &GpuiSidebarSessionStatusIndicatorsState,
    next: &GpuiSidebarSessionStatusIndicatorsState,
) -> Vec<GpuiSessionAttentionNotificationCandidate> {
    /*
    CDXC:GPUISettingsNotifications 2026-06-26-06:56:
    Attention notification detection is an edge detector over the sanitized status model, not a payload replay or count watcher. A row is eligible only when its bounded session id was absent from the previous attention set and the next row itself carries a bounded title/project title already accepted by the parser.
    */
    let previous_attention_session_ids = previous
        .projects
        .iter()
        .flat_map(|project| project.sessions.iter())
        .filter(|session| session.status == GpuiStatusIndicatorStatus::Attention)
        .map(|session| session.session_id.as_str())
        .collect::<HashSet<_>>();
    let mut emitted_session_ids = HashSet::new();
    let mut candidates = Vec::new();
    for project in &next.projects {
        for session in &project.sessions {
            if session.status != GpuiStatusIndicatorStatus::Attention
                || previous_attention_session_ids.contains(session.session_id.as_str())
                || !emitted_session_ids.insert(session.session_id.as_str())
            {
                continue;
            }
            candidates.push(GpuiSessionAttentionNotificationCandidate {
                body: if project.title.trim().is_empty() {
                    "Ghostex".to_string()
                } else {
                    project.title.clone()
                },
                icon_data_url: project.icon_data_url.clone(),
                session_id: session.session_id.clone(),
                title: session.title.clone(),
            });
        }
    }
    candidates
}

pub(crate) fn gpui_menu_bar_status_item_visible_state(
    state: &GpuiSidebarSessionStatusIndicatorsState,
) -> Option<GpuiMenuBarStatusItemState> {
    /*
    CDXC:GPUIMenuBarStatusItem 2026-06-26-05:42:
    Match the macOS menu-bar visibility rule in pure Rust before calling AppKit: the saved hideMenuBarSessionStatusIndicators setting removes the status item, attention/working counts suppress the available count, and an idle available badge appears only when no action-state count is visible.

    CDXC:GPUIMenuBarStatusItem 2026-06-26-05:44:
    Current macOS parity keeps the menu-bar item visible when the saved hide setting is false, even with zero sessions, because the button is the Running Agents dropdown target. Represent the empty case as an available-style count of 0.
    */
    if state.hide_menu_bar_indicators {
        return None;
    }
    if state.attention_count > 0 || state.working_count > 0 {
        return Some(GpuiMenuBarStatusItemState {
            attention_count: state.attention_count,
            available_count: 0,
            working_count: state.working_count,
        });
    }
    Some(GpuiMenuBarStatusItemState {
        attention_count: 0,
        available_count: state.available_count,
        working_count: 0,
    })
}

pub(crate) fn gpui_sidebar_native_app_shot_prompt_from_json(
    text: &str,
) -> Result<GpuiSidebarNativeAppShotPromptMessage, ()> {
    /*
    CDXC:GPUIAppShots 2026-06-25-23:28:
    App Shot prompt insertion is a strictly allowlisted session contract. Accept only version/type, one bounded gxserver presentation session id, and the already formatted prompt string; reject generic action names, paths as separate fields, command/stdout/stderr data, NULs, and oversized payloads before terminal ownership is consulted.

    CDXC:GPUIAppShots 2026-06-26-04:27:
    Remote App Shot insertion may identify only a machine-scoped `remote:<machine>:session:<project>:<session>` row. The parser must still reject malformed remote ids and any renderer-provided path, SSH, URL, token, command, output, or terminal text fields.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object
        .keys()
        .any(|key| !["version", "type", "sessionId", "prompt"].contains(&key.as_str()))
    {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE)
    {
        return Err(());
    }
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|session_id| gpui_sidebar_gxserver_presentation_session_id_allowed(session_id))
        .ok_or(())?
        .to_string();
    let prompt = object
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .filter(|prompt| {
            !prompt.trim().is_empty()
                && prompt.chars().count() <= GPUI_NATIVE_APP_SHOT_PROMPT_MAX_CHARS
                && !prompt.contains('\0')
        })
        .ok_or(())?
        .to_string();
    Ok(GpuiSidebarNativeAppShotPromptMessage { prompt, session_id })
}

pub(crate) fn gpui_spawn_custom_workspace_editor_command(
    command: &GpuiCustomWorkspaceEditorCommand,
    project_path: &Path,
) -> Result<(), String> {
    let mut process = match &command.executable {
        GpuiCustomWorkspaceEditorExecutable::AbsolutePath(path) => std::process::Command::new(path),
        GpuiCustomWorkspaceEditorExecutable::PathSearch(executable) => {
            let mut process = std::process::Command::new("/usr/bin/env");
            process.arg(executable);
            process
        }
    };
    for arg in &command.args {
        process.arg(arg);
    }
    process
        .arg(project_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open that project.".to_string())
}

pub(crate) fn gpui_spawn_workspace_editor_command(
    target: GpuiWorkspaceEditorTarget,
    project_path: &Path,
) -> Result<(), String> {
    let mut command = std::process::Command::new("/usr/bin/env");
    command.arg(target.command);
    match target.launch_kind {
        GpuiWorkspaceEditorLaunchKind::DirectPath => {
            command.arg(project_path);
        }
        GpuiWorkspaceEditorLaunchKind::VscodeCompatible => {
            command.arg(project_path).arg("--reuse-window");
        }
        GpuiWorkspaceEditorLaunchKind::ZedCompatible => {
            command.arg(project_path).arg("--existing");
        }
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open that project.".to_string())
}

pub(crate) fn gpui_command_exists_on_path(command: &str) -> bool {
    if command.is_empty()
        || command.contains('/')
        || command.contains('\\')
        || command.chars().any(char::is_whitespace)
    {
        return false;
    }
    let Some(path_value) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_value).any(|directory| {
        let candidate = directory.join(command);
        gpui_is_executable_file(&candidate)
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_named_app_exists(app_name: &str) -> bool {
    let bundle_name = format!("{app_name}.app");
    [
        Some(PathBuf::from("/Applications")),
        env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications")),
        Some(PathBuf::from("/System/Applications")),
    ]
    .into_iter()
    .flatten()
    .any(|directory| directory.join(&bundle_name).is_dir())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_macos_named_app_exists(_app_name: &str) -> bool {
    false
}

pub(crate) fn set_ghostex_gpui_main_menus(source_workarea_cef_owns_native_focus: bool, cx: &App) {
    cx.set_menus(ghostex_gpui_main_menus_for_source_focus(
        source_workarea_cef_owns_native_focus,
    ));
    #[cfg(target_os = "macos")]
    cef::refresh_application_menu_hooks();
}

pub(crate) fn register_ghostex_gpui_main_menu_actions(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    main_window: gpui::AnyWindowHandle,
    cx: &mut App,
) {
    /*
    CDXC:GPUIMainMenuActions 2026-07-10:
    GPUI validates native menu items against the active window dispatch tree
    plus app-global action listeners. Menu validation can run without that
    GPUI dispatch tree while native CEF/Ghostty responders or child panels own
    AppKit focus, so application and window commands must be registered
    globally instead of existing only on the rendered shell root. Keep
    focus-sensitive File/Edit commands on their normal window/responder paths.
    */
    cx.on_action(|_: &AboutGhostexGpui, _cx| {
        #[cfg(target_os = "macos")]
        unsafe {
            GhostexGpuiShowStandardAboutPanel()
        };
    });
    cx.on_action({
        let app = app.clone();
        move |_: &CheckForGhostexGpuiUpdates, cx| {
            /*
            CDXC:GPUIMainMenuUpdater 2026-07-24:
            App-menu update checks dispatch while GPUI already owns the active
            window update. Defer the Sparkle handoff until that action cycle
            returns so the main window can be borrowed and Sparkle can present
            its standard user-initiated update or no-update UI.
            */
            let app = app.clone();
            cx.defer(move |cx| {
                let _ = main_window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| app.check_for_gpui_updates(window, cx));
                });
            });
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &OpenGpuiSettingsModal, cx| {
            /*
            CDXC:GPUIMainMenuSettings 2026-07-24:
            Native app-menu actions dispatch while GPUI already owns the active
            window update. Defer the Settings window mutation until that action
            cycle returns so the main window can be borrowed normally instead
            of silently rejecting a re-entrant update.
            */
            let app = app.clone();
            cx.defer(move |cx| {
                let _ = main_window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.open_gpui_app_modal_from_titlebar(
                            GpuiAppModalKind::Settings,
                            window,
                            cx,
                        );
                    });
                });
            });
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &OpenGpuiPluginsModal, cx| {
            let app = app.clone();
            cx.defer(move |cx| {
                let _ = main_window.update(cx, |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.open_gpui_settings_plugins_page(Some(window), cx);
                    });
                });
            });
        }
    });
    cx.on_action(|_: &HideGhostexGpui, cx| cx.hide());
    cx.on_action(|_: &HideGhostexGpuiOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllGhostexGpuiApps, cx| cx.unhide_other_apps());
    cx.on_action(|_: &QuitGhostexGpui, cx| {
        GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
        cx.quit();
    });
    cx.on_action(move |_: &MinimizeGhostexGpuiWindow, cx| {
        let _ = main_window.update(cx, |_, window, _cx| window.minimize_window());
    });
    cx.on_action(move |_: &ZoomGhostexGpuiWindow, cx| {
        let _ = main_window.update(cx, |_, window, _cx| window.zoom_window());
    });
}

/// Native app menu bar (macOS `installMainMenu` parity, AppDelegate.swift
/// :2533-2663): App (About/Check for Updates/Settings/Hide/Quit),
/// File → Close Pane ⌘W, the Edit clipboard set (first-responder OS actions so
/// CEF and Ghostty views handle them natively), and Window → Minimize/Zoom.
/// Undo/Redo are omitted from the GPUI-owned menu because gpui routes them
/// through app actions instead of first-responder selectors; the macOS CEF hook
/// installs them after each menu replacement.
pub(crate) fn ghostex_gpui_main_menus_for_source_focus(
    source_workarea_cef_owns_native_focus: bool,
) -> Vec<gpui::Menu> {
    use gpui::{Menu, MenuItem, OsAction};
    /*
    CDXC:GPUISourceViewHotkeyPassthrough 2026-07-05:
    GPUI macOS derives menu item key equivalents from the keymap when
    `cx.set_menus` builds NSMenuItems. While Source CEF owns native focus,
    replace File > Close Pane with a menu-only action that has no keybinding,
    so `[NSApp mainMenu] performKeyEquivalent:` cannot consume Cmd-W before
    AppKit offers it to embedded VSCode. The CEF menu-install hook similarly
    removes standard Edit key equivalents while Source owns focus, without
    removing their clickable actions. App-reserved quit/hide/minimize
    equivalents stay on their normal actions.
    */
    let close_pane_item = if source_workarea_cef_owns_native_focus {
        MenuItem::action("Close Pane", CloseFocusedSurfaceMenuOnly)
    } else {
        MenuItem::action("Close Pane", CloseFocusedSurface)
    };
    vec![
        Menu::new("Ghostex").items(vec![
            MenuItem::action("About Ghostex", AboutGhostexGpui),
            MenuItem::action("Check for Updates…", CheckForGhostexGpuiUpdates),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenGpuiSettingsModal),
            MenuItem::separator(),
            MenuItem::action("Hide Ghostex", HideGhostexGpui),
            MenuItem::action("Hide Others", HideGhostexGpuiOthers),
            MenuItem::action("Show All", ShowAllGhostexGpuiApps),
            MenuItem::separator(),
            MenuItem::action("Quit Ghostex", QuitGhostexGpui),
        ]),
        Menu::new("File").items(vec![close_pane_item]),
        Menu::new("Edit").items(vec![
            MenuItem::os_action("Cut", GpuiEditMenuCut, OsAction::Cut),
            MenuItem::os_action("Copy", GpuiEditMenuCopy, OsAction::Copy),
            MenuItem::os_action("Paste", GpuiEditMenuPaste, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", GpuiEditMenuSelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Window").items(vec![
            MenuItem::action("Minimize", MinimizeGhostexGpuiWindow),
            MenuItem::action("Zoom", ZoomGhostexGpuiWindow),
        ]),
    ]
}

pub(crate) const GPUI_WINDOW_FRAME_STATE_VERSION: u64 = 1;
pub(crate) const GPUI_WINDOW_FRAME_MIN_WIDTH: f32 = 800.0;
pub(crate) const GPUI_WINDOW_FRAME_MIN_HEIGHT: f32 = 600.0;

/// Window frame persistence (macOS `persistMainWindowChrome` /
/// `restoredInitialWindowFrame` parity): the frame is stored as a display
/// uuid plus a display-relative origin so a moved or removed monitor restores
/// onto an existing display instead of offscreen.
#[derive(Clone, PartialEq)]
pub(crate) struct GpuiWindowFrameState {
    pub(crate) state: String,
    pub(crate) display_uuid: String,
    pub(crate) relative_origin_x: f32,
    pub(crate) relative_origin_y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

thread_local! {
    // The latest observed frame lives process-locally so quit and
    // last-window-close persistence never need the (possibly already
    // dropped) app entity.
    pub(crate) static GPUI_LATEST_WINDOW_FRAME_STATE: RefCell<Option<GpuiWindowFrameState>> =
        const { RefCell::new(None) };
}

pub(crate) fn gpui_window_frame_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-window-frame-state.json")
}

pub(crate) fn gpui_window_frame_state_from_window(
    window: &Window,
    cx: &gpui::App,
) -> Option<GpuiWindowFrameState> {
    let display = window.display(cx)?;
    let display_origin = display.bounds().origin;
    let display_uuid = display.uuid().ok()?.to_string();
    let (state, bounds) = match window.window_bounds() {
        WindowBounds::Windowed(bounds) => ("windowed", bounds),
        WindowBounds::Maximized(bounds) => ("maximized", bounds),
        WindowBounds::Fullscreen(bounds) => ("fullscreen", bounds),
    };
    Some(GpuiWindowFrameState {
        state: state.to_string(),
        display_uuid,
        relative_origin_x: (bounds.origin.x - display_origin.x).as_f32(),
        relative_origin_y: (bounds.origin.y - display_origin.y).as_f32(),
        width: bounds.size.width.as_f32(),
        height: bounds.size.height.as_f32(),
    })
}

pub(crate) fn record_gpui_window_frame_state(window: &Window, cx: &gpui::App) {
    let Some(state) = gpui_window_frame_state_from_window(window, cx) else {
        return;
    };
    GPUI_LATEST_WINDOW_FRAME_STATE.with(|latest| {
        *latest.borrow_mut() = Some(state);
    });
}

pub(crate) fn persist_gpui_window_frame_state() {
    let Some(state) = GPUI_LATEST_WINDOW_FRAME_STATE.with(|latest| latest.borrow().clone()) else {
        return;
    };
    write_gpui_window_frame_state_file(&gpui_window_frame_state_path(), &state);
}

pub(crate) fn write_gpui_window_frame_state_file(path: &Path, state: &GpuiWindowFrameState) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "displayUuid": state.display_uuid,
        "height": state.height,
        "relativeOriginX": state.relative_origin_x,
        "relativeOriginY": state.relative_origin_y,
        "state": state.state,
        "version": GPUI_WINDOW_FRAME_STATE_VERSION,
        "width": state.width,
    });
    let _ = fs::write(path, payload.to_string());
}

pub(crate) fn load_gpui_window_frame_state() -> Option<GpuiWindowFrameState> {
    load_gpui_window_frame_state_file(&gpui_window_frame_state_path())
}

pub(crate) fn load_gpui_window_frame_state_file(path: &Path) -> Option<GpuiWindowFrameState> {
    let text = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    if value.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_WINDOW_FRAME_STATE_VERSION)
    {
        return None;
    }
    let number = |key: &str| -> Option<f32> {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|number| number.is_finite())
            .map(|number| number as f32)
    };
    let state = value
        .get("state")
        .and_then(serde_json::Value::as_str)
        .filter(|state| matches!(*state, "windowed" | "maximized" | "fullscreen"))?
        .to_string();
    Some(GpuiWindowFrameState {
        state,
        display_uuid: value
            .get("displayUuid")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        relative_origin_x: number("relativeOriginX")?,
        relative_origin_y: number("relativeOriginY")?,
        width: number("width")?,
        height: number("height")?,
    })
}

/// Restores the persisted frame with the macOS multi-monitor rules: prefer
/// the stored display by uuid, fall back to the primary display, clamp the
/// size to the display and a minimum, and keep the origin inside the display.
pub(crate) fn restored_gpui_window_bounds(cx: &gpui::App) -> Option<WindowBounds> {
    let state = load_gpui_window_frame_state()?;
    restored_gpui_window_bounds_from_state(
        state,
        GPUI_WINDOW_FRAME_MIN_WIDTH,
        GPUI_WINDOW_FRAME_MIN_HEIGHT,
        cx,
    )
}

pub(crate) fn restored_gpui_window_bounds_from_state(
    state: GpuiWindowFrameState,
    min_width: f32,
    min_height: f32,
    cx: &gpui::App,
) -> Option<WindowBounds> {
    let displays = cx.displays();
    let display = displays
        .iter()
        .find(|display| {
            display
                .uuid()
                .ok()
                .is_some_and(|uuid| uuid.to_string() == state.display_uuid)
        })
        .cloned()
        .or_else(|| cx.primary_display())
        .or_else(|| displays.first().cloned())?;
    let display_bounds = display.bounds();
    let width = px(state
        .width
        .max(min_width)
        .min(display_bounds.size.width.as_f32()));
    let height = px(state
        .height
        .max(min_height)
        .min(display_bounds.size.height.as_f32()));
    let max_x = (display_bounds.size.width - width).max(px(0.0));
    let max_y = (display_bounds.size.height - height).max(px(0.0));
    let origin = gpui::point(
        display_bounds.origin.x + px(state.relative_origin_x).clamp(px(0.0), max_x),
        display_bounds.origin.y + px(state.relative_origin_y).clamp(px(0.0), max_y),
    );
    let bounds = Bounds::new(origin, size(width, height));
    Some(match state.state.as_str() {
        "maximized" => WindowBounds::Maximized(bounds),
        "fullscreen" => WindowBounds::Fullscreen(bounds),
        _ => WindowBounds::Windowed(bounds),
    })
}

pub(crate) fn gpui_first_run_onboarding_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-first-run-onboarding-state.json")
}

/// Keep first-run onboarding in GPUI-owned state rather than coupling native
/// onboarding lifecycle to any CEF profile or page-local storage.
#[derive(Clone, Default)]
pub(crate) struct GpuiFirstRunOnboardingState {
    pub(crate) tips_and_tricks_seen: bool,
    pub(crate) highlighted_features_seen_revision: Option<String>,
    pub(crate) first_launch_setup_seen_revision: Option<String>,
    pub(crate) os_integration_onboarding_seen: bool,
    pub(crate) first_launch_setup_complete: bool,
    pub(crate) windows_terminal_setup_complete: bool,
}

/// CDXC:GPUIFirstRunOnboardingOnce 2026-08-18: the markers whose surfaces are
/// user-visible, so they are written only after that surface is on screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuiFirstRunOnboardingMarker {
    FirstLaunchSetupSeen,
    OsIntegrationOnboardingSeen,
}

pub(crate) fn load_gpui_first_run_onboarding_state() -> GpuiFirstRunOnboardingState {
    let Some(value) = fs::read_to_string(gpui_first_run_onboarding_state_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return GpuiFirstRunOnboardingState::default();
    };
    GpuiFirstRunOnboardingState {
        tips_and_tricks_seen: value
            .get("tipsAndTricksSeen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        highlighted_features_seen_revision: value
            .get("highlightedFeaturesSeenRevision")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        first_launch_setup_seen_revision: value
            .get("firstLaunchSetupSeenRevision")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        os_integration_onboarding_seen: value
            .get("osIntegrationOnboardingSeen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        first_launch_setup_complete: value
            .get("firstLaunchSetupComplete")
            // Written under the Windows-only name until 2026-08-19.
            .or_else(|| value.get("windowsFirstLaunchSetupComplete"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        windows_terminal_setup_complete: value
            .get("windowsTerminalSetupComplete")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}

pub(crate) fn persist_gpui_first_run_onboarding_state(state: &GpuiFirstRunOnboardingState) {
    let path = gpui_first_run_onboarding_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "firstLaunchSetupSeenRevision": state.first_launch_setup_seen_revision,
        "highlightedFeaturesSeenRevision": state.highlighted_features_seen_revision,
        "osIntegrationOnboardingSeen": state.os_integration_onboarding_seen,
        "tipsAndTricksSeen": state.tips_and_tricks_seen,
        "firstLaunchSetupComplete": state.first_launch_setup_complete,
        "windowsTerminalSetupComplete": state.windows_terminal_setup_complete,
    });
    let _ = fs::write(path, payload.to_string());
}
