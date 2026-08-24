use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::AtomicU64,
    time::Duration,
};

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

/// True for the app's own executables: the Ghostex binary, its CEF helper
/// processes, and the gxserver daemon. These are the app itself, never a
/// user-owned server or runtime.
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
        || gpui_native_resource_is_gxserver_process(process)
        || (cfg!(target_os = "windows")
            && [
                "ghostex.exe",
                "ghostex-gpui.exe",
                "ghostex-gpui-cef-helper.exe",
            ]
            .iter()
            .any(|executable| command.contains(executable)))
}

/*
CDXC:GPUITitlebarResources 2026-08-24:
gxserver is the app's own control plane, and it binds an ephemeral loopback
port the user never types or opens. It lives in `Contents/Resources/`, so the
bundle-path markers above (which deliberately stay narrow, because the same
folder also holds zmx and the terminal runtimes that DO belong to session rows)
never matched it and every launch grew a permanent `localhost:<random>` Dev
Servers row. Identify it by executable name so the bundled runtime, a
`~/.ghostex/` install, and a dev build are all recognised.
*/
pub(crate) fn gpui_native_resource_is_gxserver_process(process: &GpuiNativeResourceProcess) -> bool {
    matches!(
        gpui_native_resource_process_name(process)
            .to_ascii_lowercase()
            .as_str(),
        "gxserver" | "gxserver.exe"
    )
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
Mirrors `server/src/project_docs.rs`: a mounted Docs directory is a notes
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
Mirrors `EXTRA_ROOT_MOUNT_SEGMENT` in `server/src/project_docs.rs`: the
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

