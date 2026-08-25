// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::{HashMap, HashSet},
    process::{Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::{Context as _, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use futures::StreamExt as _;
use gpui::http_client::HttpRequestExt as _;
use gpui::{
    Action, App, AppContext as _, Asset, Bounds, Element, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled as _, Window, div,
    img, point, prelude::FluentBuilder as _, px, rgb, size, svg,
};
use gpui_component::{Theme, ThemeMode, h_flex, menu::PopupMenu, tooltip::Tooltip, v_flex};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn titlebar_svg_icon(
    path: &'static str,
    icon_size: f32,
    color: Hsla,
) -> impl IntoElement {
    svg().size(px(icon_size)).path(path).text_color(color)
}

/// Tooltip sized to sit fully inside the titlebar strip rather than
/// overflowing it, for the left/right placements that center on the trigger.
pub(crate) fn titlebar_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    Tooltip::new(text)
        .my_0()
        .py_0()
        .h(px(TITLEBAR_TOOLTIP_HEIGHT))
        .text_size(px(TITLEBAR_TOOLTIP_TEXT_SIZE))
        .line_height(px(TITLEBAR_TOOLTIP_LINE_HEIGHT))
        .whitespace_nowrap()
        .build(window, cx)
}

pub(crate) fn titlebar_popup_menu_width(kind: GpuiTitlebarPopupKind) -> f32 {
    match kind {
        GpuiTitlebarPopupKind::Actions | GpuiTitlebarPopupKind::OpenTargets => {
            TITLEBAR_POPUP_COMPACT_WIDTH
        }
        GpuiTitlebarPopupKind::Extensions => TITLEBAR_POPUP_EXTENSIONS_WIDTH,
        GpuiTitlebarPopupKind::Git => TITLEBAR_POPUP_GIT_WIDTH,
        GpuiTitlebarPopupKind::Resources => TITLEBAR_POPUP_RESOURCES_WIDTH,
        GpuiTitlebarPopupKind::Tips => TITLEBAR_POPUP_TIPS_WIDTH,
    }
}

pub(crate) fn titlebar_popup_menu_height_for_rows(row_heights: &[f32]) -> f32 {
    titlebar_popup_menu_height_for_rows_with_chrome(
        row_heights,
        TITLEBAR_POPUP_MENU_VERTICAL_CHROME,
    )
}

pub(crate) fn titlebar_popup_menu_height_for_rows_with_chrome(
    row_heights: &[f32],
    vertical_chrome: f32,
) -> f32 {
    let rows: f32 = row_heights.iter().sum();
    let gaps = TITLEBAR_POPUP_MENU_ITEM_GAP * row_heights.len().saturating_sub(1) as f32;
    rows + gaps + vertical_chrome
}

pub(crate) fn titlebar_popup_window_bounds_for_trigger_bounds(
    kind: GpuiTitlebarPopupKind,
    trigger_bounds: Bounds<Pixels>,
    content_height: f32,
    window: &Window,
) -> Bounds<Pixels> {
    let main_window_bounds = window.bounds();
    let width = titlebar_popup_menu_width(kind);
    let max_height = match kind {
        GpuiTitlebarPopupKind::Resources | GpuiTitlebarPopupKind::Tips => {
            TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT
        }
        _ => TITLEBAR_POPUP_MENU_MAX_HEIGHT,
    };
    let available_height = (main_window_bounds.size.height.as_f32() - 28.0).max(180.0);
    let height = content_height.min(max_height).min(available_height);
    let horizontal_margin = 8.0;
    let min_left = main_window_bounds.origin.x.as_f32() + horizontal_margin;
    let max_left = main_window_bounds.origin.x.as_f32() + main_window_bounds.size.width.as_f32()
        - width
        - horizontal_margin;
    let desired_left =
        main_window_bounds.origin.x.as_f32() + trigger_bounds.top_right().x.as_f32() - width;
    let left = desired_left.clamp(min_left, max_left.max(min_left));
    let below_top = main_window_bounds.origin.y.as_f32()
        + trigger_bounds.bottom().as_f32()
        + TITLEBAR_POPUP_MENU_GAP
        - TITLEBAR_POPUP_VERTICAL_OFFSET;
    let above_top = main_window_bounds.origin.y.as_f32() + trigger_bounds.top().as_f32()
        - TITLEBAR_POPUP_MENU_GAP
        - height
        - TITLEBAR_POPUP_VERTICAL_OFFSET;
    let bottom_limit = main_window_bounds.origin.y.as_f32()
        + main_window_bounds.size.height.as_f32()
        - horizontal_margin;
    let top =
        if below_top + height <= bottom_limit || above_top < main_window_bounds.origin.y.as_f32() {
            below_top
        } else {
            above_top
        };

    Bounds {
        origin: point(px(left), px(top)),
        size: size(px(width), px(height)),
    }
}

pub(crate) fn gpui_titlebar_project_selection_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
    settings_key: &str,
    project_id: &str,
) -> Option<String> {
    settings
        .get(settings_key)
        .and_then(serde_json::Value::as_object)
        .and_then(|selections| selections.get(project_id))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty() && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        })
        .map(str::to_string)
}

pub(crate) fn gpui_persist_titlebar_project_selection(
    settings_key: &str,
    project_id: &str,
    value: &str,
) -> Result<(), shared_settings::SharedSidebarSettingsWriteError> {
    let project_id = project_id.trim();
    let value = value.trim();
    if project_id.is_empty()
        || value.is_empty()
        || project_id.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || value.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || !matches!(
            settings_key,
            GPUI_TITLEBAR_OPEN_TARGET_SELECTIONS_SETTINGS_KEY
                | GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY
        )
    {
        return Ok(());
    }

    let mut settings = shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .clone();
    let selections = settings
        .entry(settings_key.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !selections.is_object() {
        *selections = serde_json::Value::Object(serde_json::Map::new());
    }
    let selections = selections
        .as_object_mut()
        .expect("titlebar selection map must be an object");
    if !selections.contains_key(project_id)
        && selections.len() >= GPUI_TITLEBAR_SELECTION_PROJECT_LIMIT
        && let Some(oldest_key) = selections.keys().next().cloned()
    {
        selections.remove(&oldest_key);
    }
    selections.insert(
        project_id.to_string(),
        serde_json::Value::String(value.to_string()),
    );
    shared_settings::write_shared_sidebar_settings_object(settings).map(|_| ())
}

pub(crate) fn gpui_titlebar_tips_read_ids_from_settings() -> HashSet<String> {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get(GPUI_TITLEBAR_TIPS_READ_IDS_SETTINGS_KEY)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|id| TITLEBAR_TIP_IDS.contains(id))
        .map(str::to_string)
        .collect()
}

pub(crate) fn gpui_titlebar_tips_unread_count_from_settings() -> u64 {
    let read_ids = gpui_titlebar_tips_read_ids_from_settings();
    GPUI_NATIVE_TITLEBAR_TIPS
        .iter()
        .filter(|tip| !read_ids.contains(tip.id))
        .count() as u64
}

pub(crate) fn gpui_mark_titlebar_tip_read(tip_id: &str) {
    if !TITLEBAR_TIP_IDS.contains(&tip_id) {
        return;
    }
    let mut settings = shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .clone();
    let mut read_ids = gpui_titlebar_tips_read_ids_from_settings();
    read_ids.insert(tip_id.to_string());
    let ordered = TITLEBAR_TIP_IDS
        .iter()
        .filter(|id| read_ids.contains(**id))
        .map(|id| serde_json::Value::String((*id).to_string()))
        .collect::<Vec<_>>();
    settings.insert(
        GPUI_TITLEBAR_TIPS_READ_IDS_SETTINGS_KEY.to_string(),
        serde_json::Value::Array(ordered),
    );
    let _ = shared_settings::write_shared_sidebar_settings_object(settings);
}

pub(crate) fn gpui_parse_native_resource_processes(
    output: &str,
    process_id: impl Fn(u32) -> u32,
) -> Vec<GpuiNativeResourceProcess> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let system_pid = fields.next()?.parse::<u32>().ok()?;
            let system_ppid = fields.next()?.parse::<u32>().ok()?;
            let cpu = fields.next()?.parse::<f64>().ok()?;
            let rss_kb = fields.next()?.parse::<f64>().ok()?;
            let command = fields.collect::<Vec<_>>().join(" ");
            if command.is_empty() {
                return None;
            }
            Some(GpuiNativeResourceProcess {
                command,
                cpu,
                pid: process_id(system_pid),
                ppid: process_id(system_ppid),
                memory_mb: rss_kb / 1024.0,
                system_pid,
            })
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_read_native_resource_processes() -> Vec<GpuiNativeResourceProcess> {
    let Ok(output) = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,pcpu=,rss=,command="])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut processes =
        gpui_parse_native_resource_processes(&String::from_utf8_lossy(&output.stdout), |pid| pid);
    /*
    CDXC:GPUITitlebarResources 2026-08-19-12:10:
    `ps rss` is resident-set size, which charges every shared page to each
    process that maps it. The app runs one CEF helper per surface and they all
    map the same ~76 MB Chromium framework, so summing rss over the process
    tree invented tens of GB that no process actually owns, while individual
    helper rows read far below what Activity Monitor shows for the same pid.
    macOS reports process memory as the phys_footprint ledger (what Activity
    Monitor and `top` print), so read that per pid instead. Processes owned by
    another user deny the ledger read and keep their rss sample; those never
    reach a Resources row.
    */
    #[cfg(target_os = "macos")]
    for process in &mut processes {
        if let Some(footprint_mb) = gpui_native_resource_phys_footprint_mb(process.system_pid) {
            process.memory_mb = footprint_mb;
        }
    }
    processes
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_native_resource_phys_footprint_mb(pid: u32) -> Option<f64> {
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfoV0 {
        ri_uuid: [u8; 16],
        ri_user_time: u64,
        ri_system_time: u64,
        ri_pkg_idle_wkups: u64,
        ri_interrupt_wkups: u64,
        ri_pageins: u64,
        ri_wired_size: u64,
        ri_resident_size: u64,
        ri_phys_footprint: u64,
        ri_proc_start_abstime: u64,
        ri_proc_exit_abstime: u64,
    }

    unsafe extern "C" {
        fn proc_pid_rusage(
            pid: std::ffi::c_int,
            flavor: std::ffi::c_int,
            buffer: *mut std::ffi::c_void,
        ) -> std::ffi::c_int;
    }

    const RUSAGE_INFO_V0: std::ffi::c_int = 0;
    let mut info = RusageInfoV0::default();
    let status = unsafe {
        proc_pid_rusage(
            pid as std::ffi::c_int,
            RUSAGE_INFO_V0,
            (&raw mut info).cast::<std::ffi::c_void>(),
        )
    };
    (status == 0).then_some(info.ri_phys_footprint as f64 / (1024.0 * 1024.0))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_native_resource_processes() -> Vec<GpuiNativeResourceProcess> {
    use std::sync::Mutex;
    use sysinfo::{
        MINIMUM_CPU_UPDATE_INTERVAL, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
    };

    /*
    Windows has two real process domains: the Win32 Ghostex/CEF tree and the
    selected WSL2 distribution that owns terminals, zmx, agents, and gxserver.
    Sample both instead of executing Unix paths on the Win32 host. The high bit
    keeps Win32 process-tree identities separate from Linux PIDs while rows
    continue to display and act on each process's real system PID.
    */
    const WINDOWS_PROCESS_ID_BIT: u32 = 1 << 31;
    static WINDOWS_PROCESS_SYSTEM: OnceLock<Mutex<(System, bool)>> = OnceLock::new();

    let refresh_kind = ProcessRefreshKind::new()
        .with_cpu()
        .with_memory()
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet);
    let mut native_processes = WINDOWS_PROCESS_SYSTEM
        .get_or_init(|| Mutex::new((System::new(), false)))
        .lock()
        .ok()
        .map(|mut state| {
            if !state.1 {
                state
                    .0
                    .refresh_processes_specifics(ProcessesToUpdate::All, refresh_kind);
                thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
                state.1 = true;
            }
            state
                .0
                .refresh_processes_specifics(ProcessesToUpdate::All, refresh_kind);
            state
                .0
                .processes()
                .iter()
                .filter_map(|(pid, process)| {
                    let system_pid = pid.as_u32();
                    if system_pid == 0 || system_pid & WINDOWS_PROCESS_ID_BIT != 0 {
                        return None;
                    }
                    let command = if process.cmd().is_empty() {
                        process
                            .exe()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| process.name().to_string_lossy().into_owned())
                    } else {
                        process
                            .cmd()
                            .iter()
                            .map(|part| part.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    (!command.is_empty()).then(|| GpuiNativeResourceProcess {
                        command,
                        cpu: process.cpu_usage() as f64,
                        pid: system_pid | WINDOWS_PROCESS_ID_BIT,
                        ppid: process
                            .parent()
                            .map(|pid| pid.as_u32() | WINDOWS_PROCESS_ID_BIT)
                            .unwrap_or(WINDOWS_PROCESS_ID_BIT),
                        memory_mb: process.memory() as f64 / (1024.0 * 1024.0),
                        system_pid,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(output) = windows_terminal_backend::resource_process_snapshot() {
        native_processes.extend(gpui_parse_native_resource_processes(&output, |pid| pid));
    }
    native_processes
}

pub(crate) fn gpui_parse_native_resource_servers(output: &str) -> Vec<GpuiNativeResourceServer> {
    let mut pid = None;
    let mut seen = HashSet::new();
    let mut servers = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (field, value) = line.split_at(1);
        match field {
            "p" => pid = value.parse::<u32>().ok(),
            "n" => {
                let Some(pid) = pid else { continue };
                let endpoint = value.split_whitespace().next().unwrap_or_default();
                let Some(raw_port) = endpoint.rsplit(':').next() else {
                    continue;
                };
                let Ok(port) = raw_port.parse::<u16>() else {
                    continue;
                };
                if !seen.insert((pid, port)) {
                    continue;
                }
                servers.push(GpuiNativeResourceServer {
                    label: format!("localhost:{port}"),
                    pid,
                    port,
                    url: format!("http://localhost:{port}"),
                });
            }
            _ => {}
        }
    }
    servers
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_read_native_resource_servers() -> Vec<GpuiNativeResourceServer> {
    let Ok(output) = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    gpui_parse_native_resource_servers(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_native_resource_servers() -> Vec<GpuiNativeResourceServer> {
    windows_terminal_backend::resource_server_snapshot()
        .map(|output| gpui_parse_native_resource_servers(&output))
        .unwrap_or_default()
}

pub(crate) fn gpui_parse_native_resource_process_cwds(
    output: &str,
) -> HashMap<u32, std::path::PathBuf> {
    let mut pid = None;
    let mut cwds = HashMap::new();
    for line in output.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        let (field, value) = line.split_at(1);
        match field {
            "p" => pid = value.parse::<u32>().ok(),
            "n" => {
                let Some(pid) = pid else { continue };
                if value.starts_with('/') {
                    cwds.insert(pid, std::path::PathBuf::from(value));
                }
            }
            _ => {}
        }
    }
    cwds
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_read_native_resource_process_cwds(
    pids: &[u32],
) -> HashMap<u32, std::path::PathBuf> {
    if pids.is_empty() {
        return HashMap::new();
    }
    let pid_list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let Ok(output) = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-a", "-d", "cwd", "-p", &pid_list, "-F", "pn"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return HashMap::new();
    };
    /*
    lsof exits non-zero as soon as one of the listed pids has already gone,
    which is routine when sampling a process list a moment after reading it.
    The surviving pids are still printed on stdout, so parse the output on its
    own terms instead of gating on the exit status.
    */
    gpui_parse_native_resource_process_cwds(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_native_resource_process_cwds(
    pids: &[u32],
) -> HashMap<u32, std::path::PathBuf> {
    windows_terminal_backend::resource_process_cwd_snapshot(pids)
        .map(|output| gpui_parse_native_resource_process_cwds(&output))
        .unwrap_or_default()
}

pub(crate) fn gpui_native_resource_children_by_parent(
    processes: &[GpuiNativeResourceProcess],
) -> HashMap<u32, Vec<GpuiNativeResourceProcess>> {
    let mut children = HashMap::<u32, Vec<GpuiNativeResourceProcess>>::new();
    for process in processes {
        children
            .entry(process.ppid)
            .or_default()
            .push(process.clone());
    }
    children
}

pub(crate) fn titlebar_popup_reading_header(
    icon_path: &'static str,
    title: String,
    summary: String,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .min_h(px(38.0))
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .text_color(titlebar_active_text_color())
        .child(
            h_flex()
                .items_center()
                .gap(px(10.0))
                .text_size(px(15.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(titlebar_svg_icon(icon_path, 18.0, titlebar_icon_color()))
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(titlebar_inactive_text_color())
                .child(summary),
        )
}

pub(crate) fn titlebar_popup_tip_row(
    icon_path: &'static str,
    title: String,
    body: String,
    unread: bool,
) -> impl IntoElement {
    h_flex()
        .min_w_0()
        .w_full()
        .min_h(px(58.0))
        .items_start()
        .gap(px(11.0))
        .py(px(7.0))
        .child(
            div()
                .relative()
                .flex()
                .w(px(20.0))
                .pt(px(2.0))
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(icon_path, 16.0, titlebar_icon_color()))
                .when(unread, |this| {
                    this.child(
                        div()
                            .absolute()
                            .right(px(-1.0))
                            .top_0()
                            .size(px(6.0))
                            .rounded_full()
                            .bg(rgb(0x95d7f6)),
                    )
                }),
        )
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(titlebar_active_text_color())
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .line_height(px(15.0))
                        .text_color(titlebar_inactive_text_color())
                        .child(body),
                ),
        )
}

pub(crate) fn titlebar_popup_resource_row(
    row: GpuiNativeResourceRow,
    disabled: bool,
) -> impl IntoElement {
    let foreground = if disabled {
        titlebar_popup_menu_disabled_text_color()
    } else {
        titlebar_active_text_color()
    };
    h_flex()
        .min_w_0()
        .w_full()
        .min_h(px(48.0))
        .items_center()
        .gap(px(11.0))
        .child(
            div()
                .flex()
                .size(px(28.0))
                .items_center()
                .justify_center()
                .bg(rgb(0xffffff).opacity(0.07))
                .child(titlebar_svg_icon(row.icon_path, 16.0, foreground)),
        )
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .gap(px(1.0))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .child(row.label),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(11.0))
                        .text_color(titlebar_inactive_text_color())
                        .child(row.detail),
                ),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .gap(px(12.0))
                .text_size(px(12.0))
                .text_color(titlebar_inactive_text_color())
                .child(format_gpui_resource_cpu(row.cpu))
                .child(format_gpui_resource_memory(row.memory_mb)),
        )
}

pub(crate) fn titlebar_popup_standard_menu_row(
    icon_path: &'static str,
    icon_size: f32,
    label: String,
    disabled: bool,
) -> impl IntoElement {
    let text_color = if disabled {
        titlebar_popup_menu_disabled_text_color()
    } else {
        titlebar_text_color()
    };
    let icon_color = if disabled {
        titlebar_popup_menu_disabled_text_color()
    } else {
        titlebar_icon_color()
    };

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(text_color)
        .child(
            div()
                .flex()
                .w(px(icon_size.max(TITLEBAR_POPUP_MENU_ROW_ICON_SIZE)))
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(icon_path, icon_size, icon_color)),
        )
        .child(
            div()
                .min_w_0()
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

pub(crate) fn titlebar_popup_extension_menu_row(
    extension: GpuiInstalledExtension,
) -> impl IntoElement {
    let pin_action = ToggleGpuiExtensionPin {
        extension_id: extension.id.clone(),
        pinned: !extension.pinned,
    };
    let pin_icon = if extension.pinned {
        "titlebar/pin-slash.svg"
    } else {
        "titlebar/pin.svg"
    };
    let pin_tooltip = if extension.pinned { "Unpin" } else { "Pin" };
    let placement_label = extension.launch_placement_label();

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .min_h(px(TITLEBAR_POPUP_EXTENSION_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .text_color(titlebar_text_color())
        .child(
            div()
                .flex_shrink_0()
                .size(px(20.0))
                .items_center()
                .justify_center()
                .child(img(extension.icon_image).size(px(18.0))),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                .child(extension.title),
        )
        .child(
            div()
                .flex_shrink_0()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(3.0))
                .bg(rgb(0xffffff).opacity(0.07))
                .text_size(px(10.0))
                .text_color(titlebar_inactive_text_color())
                .child(placement_label),
        )
        .child(
            div()
                .id(format!("ghostex-gpui-extension-pin-{}", extension.id))
                .flex_shrink_0()
                .size(px(26.0))
                .items_center()
                .justify_center()
                .rounded(px(3.0))
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(pin_action.clone()), cx);
                })
                .child(titlebar_svg_icon(pin_icon, 14.0, titlebar_icon_color()))
                .tooltip(move |window, cx| titlebar_tooltip(pin_tooltip, window, cx)),
        )
}

pub(crate) fn titlebar_popup_empty_menu_row(label: String) -> impl IntoElement {
    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(titlebar_popup_menu_disabled_text_color())
        .child(
            div()
                .min_w_0()
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

pub(crate) fn titlebar_popup_action_menu_row(action: GpuiTitlebarAction) -> impl IntoElement {
    let icon_path = titlebar_action_icon_path(Some(&action));
    let label = action.titlebar_menu_name();
    let (preview, preview_unconfigured) = action.titlebar_menu_preview();

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_ACTION_ROW_HEIGHT))
        .items_start()
        .gap(px(10.0))
        .rounded(px(4.0))
        .py(px(6.0))
        .text_color(titlebar_text_color())
        .child(
            div()
                .flex()
                .w(px(TITLEBAR_POPUP_MENU_ROW_ICON_SIZE))
                .pt(px(1.0))
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    icon_path,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    titlebar_icon_color(),
                )),
        )
        .child(
            v_flex()
                .min_w(px(0.0))
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .gap(px(1.0))
                .child(
                    div()
                        .min_w_0()
                        .max_w_full()
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::NORMAL)
                        .child(label),
                )
                .child(
                    div()
                        .min_w_0()
                        .max_w_full()
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(TITLEBAR_POPUP_ACTION_PREVIEW_TEXT_SIZE))
                        .line_height(px(14.0))
                        .text_color(titlebar_popup_menu_preview_text_color())
                        .when(preview_unconfigured, |this| this.italic())
                        .child(preview),
                ),
        )
}

pub(crate) fn titlebar_popup_git_section(mut menu: PopupMenu, label: &'static str) -> PopupMenu {
    menu =
        menu.menu_element_with_disabled(Box::new(CopyGpuiTitlebarGitBranch), true, move |_, _| {
            titlebar_popup_git_section_label(label)
        });
    menu
}

pub(crate) fn titlebar_popup_git_section_label(label: &'static str) -> impl IntoElement {
    h_flex()
        .w_full()
        .min_h(px(TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT))
        .items_center()
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(titlebar_popup_git_section_label_color())
        .child(label)
}

pub(crate) fn titlebar_popup_git_branch_menu_row(
    branch: String,
    disabled: bool,
) -> impl IntoElement {
    titlebar_popup_git_status_menu_row(
        TITLEBAR_ICON_GIT_COMMIT,
        "Branch".to_string(),
        titlebar_popup_git_value_text(branch, disabled),
        disabled,
    )
}

pub(crate) fn titlebar_popup_git_changes_menu_row(
    additions: u64,
    deletions: u64,
) -> impl IntoElement {
    titlebar_popup_git_status_menu_row(
        TITLEBAR_ICON_CODE,
        "Changes".to_string(),
        h_flex()
            .gap(px(6.0))
            .flex_shrink_0()
            .text_size(px(12.0))
            .child(
                div()
                    .text_color(titlebar_popup_git_additions_color())
                    .child(format!("+{additions}")),
            )
            .child(
                div()
                    .text_color(titlebar_popup_git_deletions_color())
                    .child(format!("\u{2212}{deletions}")),
            ),
        false,
    )
}

pub(crate) fn titlebar_popup_git_commits_menu_row(
    ahead_count: u64,
    behind_count: u64,
    disabled: bool,
) -> impl IntoElement {
    let has_commits_to_sync = ahead_count > 0 || behind_count > 0;
    titlebar_popup_git_status_menu_row(
        TITLEBAR_ICON_GIT_COMPARE,
        if has_commits_to_sync {
            "Sync upstream".to_string()
        } else {
            "No commits to sync".to_string()
        },
        titlebar_popup_git_value_text(
            if has_commits_to_sync {
                format!("\u{2191}{ahead_count} \u{2193}{behind_count}")
            } else {
                String::new()
            },
            disabled,
        ),
        disabled,
    )
}

pub(crate) fn titlebar_popup_git_status_menu_row(
    icon_path: &'static str,
    label: String,
    value: impl IntoElement + 'static,
    disabled: bool,
) -> impl IntoElement {
    let icon_color = if disabled {
        titlebar_popup_git_disabled_icon_color()
    } else {
        titlebar_icon_color()
    };

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(titlebar_text_color())
        .child(
            div()
                .flex()
                .w(px(18.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    icon_path,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    icon_color,
                )),
        )
        .child(
            h_flex()
                .min_w_0()
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_shrink_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .child(value),
        )
}

pub(crate) fn titlebar_popup_git_value_text(value: String, disabled: bool) -> impl IntoElement {
    let color = if disabled {
        titlebar_popup_menu_disabled_text_color()
    } else {
        titlebar_inactive_text_color()
    };

    div()
        .min_w_0()
        .max_w(px(176.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(12.0))
        .text_color(color)
        .child(value)
}

pub(crate) fn titlebar_popup_git_action_menu_row(row: GpuiTitlebarGitMenuRow) -> impl IntoElement {
    let icon_path = titlebar_git_action_icon_path(row.action);
    let icon_color = if row.disabled {
        titlebar_popup_git_disabled_icon_color()
    } else {
        titlebar_icon_color()
    };

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(titlebar_text_color())
        .child(
            div()
                .flex()
                .w(px(18.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    icon_path,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    icon_color,
                )),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(row.label),
        )
}

pub(crate) fn titlebar_git_action_icon_path(action: GpuiTitlebarGitMenuActionId) -> &'static str {
    match action {
        GpuiTitlebarGitMenuActionId::Commit => TITLEBAR_ICON_GIT_COMMIT,
        GpuiTitlebarGitMenuActionId::Push => TITLEBAR_ICON_UPLOAD,
        GpuiTitlebarGitMenuActionId::Pr => TITLEBAR_ICON_GIT_PULL_REQUEST,
        GpuiTitlebarGitMenuActionId::SyncMain | GpuiTitlebarGitMenuActionId::SyncRemote => {
            TITLEBAR_ICON_GIT_COMPARE
        }
        GpuiTitlebarGitMenuActionId::MultiRelease => TITLEBAR_ICON_STACK_PUSH,
        GpuiTitlebarGitMenuActionId::Release => TITLEBAR_ICON_ROCKET,
    }
}

pub(crate) fn titlebar_action_icon_path(action: Option<&GpuiTitlebarAction>) -> &'static str {
    let Some(action) = action else {
        return TITLEBAR_ICON_SETTINGS;
    };
    titlebar_sidebar_command_icon_path(action.icon.as_deref().unwrap_or("playerPlay"))
}

pub(crate) fn titlebar_sidebar_command_icon_path(icon: &str) -> &'static str {
    match icon {
        "playerPlay" => TITLEBAR_ICON_PLAYER_PLAY,
        "api" => "titlebar/api.svg",
        "archive" => "titlebar/archive.svg",
        "bell" => "titlebar/bell.svg",
        "bolt" => "titlebar/bolt.svg",
        "book" => "titlebar/book.svg",
        "brain" => "titlebar/brain.svg",
        "braces" => "titlebar/braces.svg",
        "brandDocker" => "titlebar/brand-docker.svg",
        "brandGithub" => "titlebar/brand-github.svg",
        "brandPython" => "titlebar/brand-python.svg",
        "brandReact" => "titlebar/brand-react.svg",
        "brandVscode" => "titlebar/brand-vscode.svg",
        "bug" => "titlebar/bug.svg",
        "chartBar" => "titlebar/chart-bar.svg",
        "cloud" => "titlebar/cloud.svg",
        "checklist" => "titlebar/checklist.svg",
        "clock" => "titlebar/clock.svg",
        "code" => "titlebar/code.svg",
        "command" => "titlebar/command.svg",
        "cpu" => "titlebar/cpu.svg",
        "database" => "titlebar/database.svg",
        "deviceDesktop" => TITLEBAR_ICON_DEVICE_DESKTOP,
        "deviceLaptop" => "titlebar/device-laptop.svg",
        "download" => TITLEBAR_ICON_DOWNLOAD,
        "fileCode" => "titlebar/file-code.svg",
        "fileDiff" => "titlebar/file-diff.svg",
        "fileSearch" => "titlebar/file-search.svg",
        "fileText" => "titlebar/file-text.svg",
        "flask" => "titlebar/flask.svg",
        "folder" => "titlebar/folder.svg",
        "folderOpen" => TITLEBAR_ICON_FOLDER_OPEN,
        "gitBranch" => "titlebar/git-branch.svg",
        "gitCommit" => TITLEBAR_ICON_GIT_COMMIT,
        "gitMerge" => "titlebar/git-merge.svg",
        "gitPullRequest" => TITLEBAR_ICON_GIT_PULL_REQUEST,
        "key" => "titlebar/key.svg",
        "layoutDashboard" => "titlebar/layout-dashboard.svg",
        "link" => "titlebar/link.svg",
        "lock" => "titlebar/lock.svg",
        "messageCircle" => "titlebar/message-circle.svg",
        "package" => "titlebar/package.svg",
        "pencilCode" => "titlebar/pencil-code.svg",
        "refresh" => "titlebar/refresh.svg",
        "robot" => "titlebar/robot.svg",
        "route" => "titlebar/route.svg",
        "rocket" => TITLEBAR_ICON_ROCKET,
        "search" => BROWSER_ICON_SEARCH,
        "server" => "titlebar/server.svg",
        "settings" => TITLEBAR_ICON_SETTINGS,
        "shieldSearch" => "titlebar/shield-search.svg",
        "sparkles" => "titlebar/sparkles.svg",
        "stack" => "titlebar/stack.svg",
        "terminal" => "titlebar/terminal-2.svg",
        "testPipe" => "titlebar/test-pipe.svg",
        "tool" => "titlebar/tool.svg",
        "upload" => TITLEBAR_ICON_UPLOAD,
        "wand" => "titlebar/wand.svg",
        "world" => BROWSER_ICON_WORLD,
        _ => unreachable!("validated sidebar command icon id must be mapped"),
    }
}

pub(crate) fn titlebar_open_target_icon_for_id(target_id: &str) -> (&'static str, f32) {
    match target_id {
        "finder" => (TITLEBAR_ICON_FOLDER_OPEN, 16.0),
        "cursor" => ("titlebar/cursor.svg", 17.0),
        "vscode" | "vscode-insiders" => (TITLEBAR_ICON_VSCODE, 17.0),
        "vscodium" => ("titlebar/vscodium.svg", 17.0),
        "zed" => ("titlebar/zed.svg", 17.0),
        "antigravity" => ("titlebar/antigravity.svg", 17.0),
        "idea" => ("titlebar/intellijidea.svg", 17.0),
        "phpstorm" => ("titlebar/phpstorm.svg", 17.0),
        "pycharm" => ("titlebar/pycharm.svg", 17.0),
        "rider" => ("titlebar/rider.svg", 17.0),
        "rubymine" => ("titlebar/rubymine.svg", 17.0),
        "webstorm" => ("titlebar/webstorm.svg", 17.0),
        "aqua" | "clion" | "datagrip" | "dataspell" | "goland" | "rustrover" => {
            ("titlebar/jetbrains.svg", 17.0)
        }
        "trae" | "kiro" => (TITLEBAR_ICON_BOX, 16.0),
        _ => (TITLEBAR_ICON_BOX, 16.0),
    }
}

/// Paints the titlebar update-download ring: a dim full-circle track plus a
/// clockwise-from-noon fill arc for the normalized Sparkle progress ratio.
/// Unknown progress (`None`) paints the track only, matching the React ring's
/// empty-fill unknown-size state.
pub(crate) fn paint_titlebar_update_progress_ring(
    bounds: Bounds<Pixels>,
    progress: Option<f64>,
    window: &mut Window,
) {
    let center_x = bounds.left().as_f32() + bounds.size.width.as_f32() / 2.0;
    let center_y = bounds.top().as_f32() + bounds.size.height.as_f32() / 2.0;
    let radius = TITLEBAR_UPDATE_PROGRESS_RING_RADIUS;
    let radii = gpui::point(px(radius), px(radius));

    let mut track = gpui::PathBuilder::stroke(px(TITLEBAR_UPDATE_PROGRESS_RING_STROKE));
    track.move_to(gpui::point(px(center_x + radius), px(center_y)));
    track.arc_to(
        radii,
        px(0.0),
        false,
        true,
        gpui::point(px(center_x - radius), px(center_y)),
    );
    track.arc_to(
        radii,
        px(0.0),
        false,
        true,
        gpui::point(px(center_x + radius), px(center_y)),
    );
    if let Ok(path) = track.build() {
        window.paint_path(path, titlebar_update_progress_track_color());
    }

    let clamped = match progress {
        Some(progress) => progress.clamp(0.0, 1.0) as f32,
        None => {
            // Match the legacy CSS pending-fill animation: grow from 4% to
            // 72% over the first 55% of a 1.25s cycle, then contract again.
            let elapsed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f32();
            let phase = (elapsed % 1.25) / 1.25;
            let leg = if phase <= 0.55 {
                phase / 0.55
            } else {
                1.0 - ((phase - 0.55) / 0.45)
            };
            let eased = leg.clamp(0.0, 1.0);
            let eased = eased * eased * (3.0 - 2.0 * eased);
            0.04 + (0.72 - 0.04) * eased
        }
    };
    if clamped <= 0.0 {
        return;
    }
    // Cap just under a full turn so the arc endpoint never coincides with its
    // start point (a zero-length lyon arc would drop the full-progress ring).
    let sweep = clamped.min(0.999) * std::f32::consts::TAU;
    let end_angle = -std::f32::consts::FRAC_PI_2 + sweep;
    let mut fill = gpui::PathBuilder::stroke(px(TITLEBAR_UPDATE_PROGRESS_RING_STROKE));
    fill.move_to(gpui::point(px(center_x), px(center_y - radius)));
    fill.arc_to(
        radii,
        px(0.0),
        sweep > std::f32::consts::PI,
        true,
        gpui::point(
            px(center_x + radius * end_angle.cos()),
            px(center_y + radius * end_angle.sin()),
        ),
    );
    if let Ok(path) = fill.build() {
        window.paint_path(path, titlebar_active_text_color());
    }
}

pub(crate) fn paint_titlebar_git_busy_spinner(bounds: Bounds<Pixels>, window: &mut Window) {
    let center_x = bounds.left().as_f32() + bounds.size.width.as_f32() / 2.0;
    let center_y = bounds.top().as_f32() + bounds.size.height.as_f32() / 2.0;
    let radius = 5.5;
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::PI * 1.45;
    let end_angle = start_angle + sweep;
    let radii = gpui::point(px(radius), px(radius));
    let mut path = gpui::PathBuilder::stroke(px(1.6));
    path.move_to(gpui::point(
        px(center_x + radius * start_angle.cos()),
        px(center_y + radius * start_angle.sin()),
    ));
    path.arc_to(
        radii,
        px(0.0),
        sweep > std::f32::consts::PI,
        true,
        gpui::point(
            px(center_x + radius * end_angle.cos()),
            px(center_y + radius * end_angle.sin()),
        ),
    );
    if let Ok(path) = path.build() {
        window.paint_path(path, titlebar_active_text_color());
    }
}

pub(crate) fn titlebar_update_progress_track_color() -> Hsla {
    rgb(0xffffff).opacity(0.24).into()
}

pub(crate) fn titlebar_update_available_color() -> Hsla {
    titlebar_active_text_color()
}

pub(crate) fn titlebar_update_downloading_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.92)
        .into()
}

pub(crate) fn titlebar_background() -> Hsla {
    rgb(GPUI_TITLEBAR_BACKGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

/*
CDXC:GPUITitlebarGradient 2026-07-22:
The titlebar strip paints the sidebar's shared gradient stops horizontally
(left = darker sidebar top stop, right = lighter sidebar bottom stop) so the
chrome reads as one continuous surface. Solid consumers (popup borders, modal
host fills) keep `titlebar_background()`.
*/
pub(crate) fn titlebar_gradient_fill() -> gpui::Background {
    gpui::linear_gradient(
        90.,
        gpui::linear_color_stop(
            rgb(GPUI_TITLEBAR_GRADIENT_LEFT_RGB.load(Ordering::Relaxed) as u32),
            0.,
        ),
        gpui::linear_color_stop(
            rgb(GPUI_TITLEBAR_GRADIENT_RIGHT_RGB.load(Ordering::Relaxed) as u32),
            1.,
        ),
    )
}

pub(crate) fn titlebar_button_border_color() -> Hsla {
    rgb(0x252525).into()
}

pub(crate) fn titlebar_button_hover_color() -> Hsla {
    rgb(0xffffff).opacity(0.08).into()
}

pub(crate) fn titlebar_active_segment_color() -> Hsla {
    rgb(0xffffff).opacity(0.11).into()
}

/*
CDXC:GPUITitlebarDropdownChrome 2026-07-09:
All titlebar dropdown surfaces (the Git/Actions/Open In popup menus and the
Tips/Resources CEF reading panels) share one chrome spec after visual review:
#0e0e0e background, 1px #303030 border, 2px corner radius.
*/
pub(crate) fn titlebar_popup_menu_background() -> Hsla {
    rgb(0x0e0e0e).into()
}

pub(crate) fn titlebar_popup_menu_border_color() -> Hsla {
    rgb(0x303030).into()
}

pub(crate) fn apply_gpui_component_dark_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.popover = titlebar_popup_menu_background();
    theme.popover_foreground = titlebar_text_color();
    theme.border = titlebar_popup_menu_border_color();
    theme.radius = px(2.0);
}

pub(crate) fn titlebar_popup_menu_disabled_text_color() -> Hsla {
    rgb(0xffffff).opacity(0.34).into()
}

pub(crate) fn titlebar_popup_menu_preview_text_color() -> Hsla {
    rgb(0xffffff).opacity(0.48).into()
}

pub(crate) fn titlebar_popup_git_section_label_color() -> Hsla {
    rgb(0xffffff).opacity(0.55).into()
}

pub(crate) fn titlebar_popup_git_disabled_icon_color() -> Hsla {
    rgb(0xffffff).opacity(0.42).into()
}

pub(crate) fn titlebar_popup_git_additions_color() -> Hsla {
    rgb(0x4ade80).into()
}

pub(crate) fn titlebar_popup_git_deletions_color() -> Hsla {
    rgb(0xf87171).into()
}

pub(crate) fn titlebar_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.84)
        .into()
}

pub(crate) fn titlebar_project_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.92)
        .into()
}

pub(crate) fn titlebar_active_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

pub(crate) fn titlebar_inactive_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.68)
        .into()
}

pub(crate) fn titlebar_disabled_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.30)
        .into()
}

pub(crate) fn titlebar_disabled_segment_color() -> Hsla {
    rgb(0xffffff).opacity(0.025).into()
}

pub(crate) fn titlebar_icon_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.84)
        .into()
}

pub(crate) fn titlebar_icon_hover_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

/// Rust port of `getSidebarTitlebarGradientColors` /
/// `normalizedSidebarTitlebarTintDirection` in packages/shared/ghostex-settings.ts: the
/// tint direction is the background's per-channel deviation from its average,
/// normalized by its largest channel magnitude (neutral grays stay neutral),
/// and the two stops sit at +2 and +10 of that direction. Rounding matches JS
/// `Math.round` (half toward positive infinity) so both sides emit identical
/// hex stops.
pub(crate) fn sidebar_titlebar_gradient_stops(background: u32) -> (u32, u32) {
    let base = sidebar_titlebar_rgb_channels(background);
    let direction = sidebar_titlebar_tint_direction(base);
    let stop = |amount: f32| -> u32 {
        sidebar_titlebar_pack_rgb([
            base[0] + direction[0] * amount,
            base[1] + direction[1] * amount,
            base[2] + direction[2] * amount,
        ])
    };
    (stop(2.0), stop(10.0))
}

pub(crate) fn sidebar_titlebar_rgb_channels(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
        (color & 0xff) as f32,
    ]
}

/// JS `Math.round` (half toward positive infinity) + 0-255 clamp per channel,
/// so Rust emits the identical hex the shared TS pipeline computes.
pub(crate) fn sidebar_titlebar_pack_rgb(channels: [f32; 3]) -> u32 {
    channels.iter().fold(0u32, |rgb, value| {
        (rgb << 8) | ((value + 0.5).floor().clamp(0.0, 255.0) as u32)
    })
}

pub(crate) fn sidebar_titlebar_tint_direction(base: [f32; 3]) -> [f32; 3] {
    let average = (base[0] + base[1] + base[2]) / 3.0;
    let mut direction = [base[0] - average, base[1] - average, base[2] - average];
    let magnitude = direction
        .iter()
        .map(|channel| channel.abs())
        .fold(0.0f32, f32::max);
    if magnitude < 0.5 {
        return [0.0, 0.0, 0.0];
    }
    for channel in &mut direction {
        *channel /= magnitude;
    }
    direction
}

pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_RGB: u32 = 0x1c1c1c;
pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB: u32 = 0x808080;
pub(crate) const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = 90.0;
pub(crate) const MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = 85.0;
pub(crate) const MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT: f64 = 100.0;
pub(crate) const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT: f64 = 95.0;

/// Mirror of `CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS` in
/// packages/shared/ghostex-settings.ts. Keep both tables in sync.
pub(crate) const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS: [(u32, u32); 17] = [
    (0x000000, 0x000000),
    (0xffffff, 0x0e0e0e),
    (0x808080, 0x0e0e0e),
    (0x88d7ff, 0x0a0f12),
    (0x4f6672, 0x0c0e10),
    (0x884444, 0x0d0005),
    (0x8a5330, 0x100502),
    (0x8a6a2f, 0x110a02),
    (0x657a3f, 0x0c1005),
    (0x3f7a5f, 0x031006),
    (0x2f7d66, 0x03100c),
    (0x287c7f, 0x031011),
    (0x336699, 0x0c0e11),
    (0x4f5f96, 0x080912),
    (0x6c4f8f, 0x0a0611),
    (0x854f7a, 0x100611),
    (0x8a4f5f, 0x100409),
];

pub(crate) fn clamp_sidebar_titlebar_background_darkness_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;
    }
    (value + 0.5).floor().clamp(
        MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
        MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    )
}

pub(crate) fn sidebar_titlebar_background_darkness_for_color(background: u32) -> f64 {
    let [red, green, blue] = sidebar_titlebar_rgb_channels(background);
    let luminance = f64::from(0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255.0;
    clamp_sidebar_titlebar_background_darkness_percent((1.0 - luminance) * 100.0)
}

/// Rust port of `getSidebarTitlebarBackgroundForDarkness` in
/// packages/shared/ghostex-settings.ts: resolve the calibrated dark background for the
/// selected tint (falling back to the neutral default for same-channel tints,
/// or default-base + tint-direction * 4 for uncalibrated tints), then scale it
/// with the Background Contrast slider.
pub(crate) fn sidebar_titlebar_background_for_darkness(darkness_percent: f64, tint: u32) -> u32 {
    let darkness = clamp_sidebar_titlebar_background_darkness_percent(darkness_percent);
    let default_dark_tint_background = CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS
        .iter()
        .find(|(key, _)| *key == tint)
        .map(|(_, value)| sidebar_titlebar_rgb_channels(*value))
        .unwrap_or_else(|| {
            let color = sidebar_titlebar_rgb_channels(tint);
            let spread = color.iter().fold(0.0f32, |max, value| max.max(*value))
                - color.iter().fold(255.0f32, |min, value| min.min(*value));
            if spread < 1.0 {
                return sidebar_titlebar_rgb_channels(
                    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_RGB,
                );
            }
            let direction = sidebar_titlebar_tint_direction(color);
            let base =
                sidebar_titlebar_rgb_channels(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_RGB);
            [
                base[0] + direction[0] * 4.0,
                base[1] + direction[1] * 4.0,
                base[2] + direction[2] * 4.0,
            ]
        });
    if darkness == MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT {
        return 0x000000;
    }
    let scale = ((MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT - darkness)
        / (MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT
            - CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT))
        as f32;
    sidebar_titlebar_pack_rgb([
        default_dark_tint_background[0] * scale,
        default_dark_tint_background[1] * scale,
        default_dark_tint_background[2] * scale,
    ])
}

/// Mirror of the effective-settings resolution in packages/shared/ghostex-settings.ts:
/// the darkness slider (seeded from a valid legacy saved background color when
/// the slider key is missing) plus the tint choice produce the chrome
/// background; the stored `customSidebarTitlebarBackgroundColor` hex itself is
/// never the applied color.
pub(crate) fn resolved_custom_sidebar_titlebar_background(
    object: &serde_json::Map<String, serde_json::Value>,
) -> u32 {
    let legacy_background =
        gpui_settings_hex_rgb(object.get("customSidebarTitlebarBackgroundColor"));
    let darkness_fallback = legacy_background
        .map(sidebar_titlebar_background_darkness_for_color)
        .unwrap_or(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT);
    let darkness = object
        .get("customSidebarTitlebarBackgroundDarknessPercent")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(darkness_fallback);
    let tint = gpui_settings_hex_rgb(object.get("customSidebarTitlebarBackgroundTintColor"))
        .unwrap_or(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB);
    sidebar_titlebar_background_for_darkness(darkness, tint)
}

pub(crate) fn command_pane_titlebar_separator_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneChrome 2026-06-25-13:19:
    Native command titlebar separators use `calibratedWhite:0.54 alpha:0.24`, which is lighter and more translucent than the inactive command pane outline.
    */
    rgb(0x8a8a8a).opacity(0.24).into()
}

pub(crate) fn titlebar_project_label_from_latest_sidebar_snapshot(
    latest_snapshot: Option<&GpuiProjectSnapshot>,
) -> String {
    /*
    CDXC:GPUITitlebarProjectLabel 2026-06-22-19:57:
    The titlebar label is runtime-only sidebar state: use the latest valid snapshot display name and show the static Ghostex label before any valid sidebar payload arrives. Do not read env vars, repo folders, .git metadata, workspace names, fixture names, paths, URLs, sidebar titles, persisted state, or logs to infer the label.
    */
    latest_snapshot
        .map(|snapshot| snapshot.display_name.clone())
        .unwrap_or_else(|| TITLEBAR_PROJECT_LABEL_FALLBACK.to_string())
}

pub(crate) fn cef_parent_native_view(window: &mut Window) -> Result<*mut std::ffi::c_void> {
    /*
    CDXC:GPUICefPlatformSeam 2026-07-04:
    Windowed CEF parents its child views on the GPUI window's native handle:
    the root NSView on macOS, the top-level HWND on Windows, and the X11
    window id on Linux (gpui's X11 backend hands out an Xcb handle; the Xlib
    arm covers the same id space for completeness). The pointer stays opaque
    past this point; only the cef platform adapters interpret it.
    */
    let handle = window
        .window_handle()
        .map_err(|error| anyhow::anyhow!("failed to read GPUI raw window handle: {error:?}"))?;
    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => Ok(handle.ns_view.as_ptr()),
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as *mut std::ffi::c_void),
        RawWindowHandle::Xcb(handle) => Ok(handle.window.get() as usize as *mut std::ffi::c_void),
        RawWindowHandle::Xlib(handle) => Ok(handle.window as usize as *mut std::ffi::c_void),
        other => {
            anyhow::bail!("windowed CEF requires an AppKit, Win32, or X11 parent, got {other:?}")
        }
    }
}

pub(crate) fn normalize_address(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        return Some(trimmed.to_string());
    }
    if trimmed == "localhost"
        || trimmed.starts_with("localhost:")
        || trimmed.starts_with("127.0.0.1")
    {
        return Some(format!("http://{trimmed}"));
    }
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return Some(format!("https://{trimmed}"));
    }
    /*
    CDXC:GPUIBrowserToolbar 2026-06-14-17:42:
    The GPUI address field should resolve committed non-empty text the same way as the macOS browser toolbar: explicit schemes are kept, local hosts use http, domain-like text uses https, and free text becomes an in-pane Google search. Empty commits are not normalized; the toolbar restores the current tab URL and returns focus to page content.
    */
    Some(format!(
        "https://www.google.com/search?q={}",
        encode_search_query(trimmed)
    ))
}

pub(crate) fn titlebar_tips_panel_url() -> Result<String> {
    let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_TITLEBAR_HOST_URL", "titlebar-host.html")
        .context("failed to resolve GPUI titlebar host bundle URL")?;
    Ok(gpui_url_with_query_param(
        &base_url,
        "ghostexTitlebarPanel",
        "tips",
    ))
}

pub(crate) fn titlebar_resources_panel_url() -> Result<String> {
    let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_TITLEBAR_HOST_URL", "titlebar-host.html")
        .context("failed to resolve GPUI titlebar host bundle URL")?;
    Ok(gpui_url_with_query_param(
        &base_url,
        "ghostexTitlebarPanel",
        "resources",
    ))
}

pub(crate) fn gpui_url_with_query_param(url: &str, key: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{key}={value}")
}

pub(crate) fn gpui_titlebar_tips_browser_url_allowed(url: &str) -> bool {
    matches!(url, GHOSTEX_DOCS_URL | GHOSTEX_CHANGELOG_URL)
}

pub(crate) fn gpui_titlebar_resources_browser_url_allowed(url: &str) -> bool {
    let Ok(parsed) = gpui::http_client::Url::parse(url.trim()) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    GPUI_PORTLESS_APP_INTEGRATION_ENABLED
        && gpui_sidebar_portless_state_with_presentation()
            .as_ref()
            .is_some_and(|state| gpui_titlebar_resources_portless_host_allowed(host, state))
}

pub(crate) fn gpui_titlebar_resources_portless_host_allowed(
    host: &str,
    state: &serde_json::Value,
) -> bool {
    let presentation = state.get("presentation");
    if presentation
        .and_then(|presentation| presentation.get("routePreviewStatus"))
        .and_then(serde_json::Value::as_str)
        != Some("current")
    {
        return false;
    }
    presentation
        .and_then(|presentation| presentation.get("routePreviews"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|previews| {
            previews.iter().any(|preview| {
                preview
                    .get("hostname")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|hostname| hostname.eq_ignore_ascii_case(host))
            })
        })
}

pub(crate) fn gpui_open_external_http_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return Err("External URL is invalid.".to_string());
    }
    let parsed = gpui::http_client::Url::parse(trimmed)
        .map_err(|_| "External URL is invalid.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.host_str().is_none()
    {
        return Err("External URL is invalid.".to_string());
    }
    gpui_spawn_os_open(std::ffi::OsStr::new(trimmed))
}

pub(crate) fn gpui_titlebar_resources_native_pane_state(
    state: TerminalSessionPresentationState,
) -> &'static str {
    match state {
        TerminalSessionPresentationState::Running => "mounted",
        TerminalSessionPresentationState::Mounting => "mounting",
        TerminalSessionPresentationState::Sleeping
        | TerminalSessionPresentationState::StartupFailed
        | TerminalSessionPresentationState::RestoredUnmounted
        | TerminalSessionPresentationState::PoppedOutPlaceholder => "unmounted",
    }
}

pub(crate) fn gpui_titlebar_resources_provider_session_state(
    persistence_provider: &str,
    persistence_name: Option<&str>,
) -> &'static str {
    if persistence_provider == "off" {
        "persistence-disabled"
    } else if persistence_name.is_some_and(|name| !name.trim().is_empty()) {
        "exists"
    } else {
        "unknown"
    }
}

pub(crate) fn gpui_titlebar_resources_project_editor_kind(
    slot_key: ProjectWorkareaCefSurfaceSlotKey,
) -> &'static str {
    match slot_key {
        ProjectWorkareaCefSurfaceSlotKey::Source => "code",
        ProjectWorkareaCefSurfaceSlotKey::Kanban => "tasks",
        ProjectWorkareaCefSurfaceSlotKey::Automate => "automate",
        ProjectWorkareaCefSurfaceSlotKey::Manage => "manage",
        ProjectWorkareaCefSurfaceSlotKey::Extension(_) => "extension",
    }
}

/*
CDXC:WebLinkOpenTarget 2026-08-19:
The titlebar Resources list needs the merged web-link destination as a string
for its own payload, so route it through the same snapshot accessor that owns
the legacy-key precedence instead of reading the raw field a second time.
*/
pub(crate) fn gpui_titlebar_web_link_open_target_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> &'static str {
    if shared_settings::web_links_open_in_app_from_object(settings) {
        "internal-browser"
    } else {
        "system-default-browser"
    }
}

pub(crate) fn gpui_titlebar_resources_string_array_field(
    message: &serde_json::Value,
    field: &str,
) -> Vec<String> {
    message
        .get(field)
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn gpui_titlebar_project_state_update_from_sidebar_state_payload(
    payload: &serde_json::Value,
) -> Option<serde_json::Value> {
    match payload.get("type").and_then(serde_json::Value::as_str)? {
        "agentHookStatus" => Some(serde_json::json!({ "agentHookStatus": payload })),
        "ghostexCliStatus" => Some(serde_json::json!({ "ghostexCliStatus": payload })),
        _ => None,
    }
}

pub(crate) fn gpui_app_modal_unsupported_settings_command_noop(command_type: &str) -> bool {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:36:
    GPUI Settings status/action requests that affect visible loading state must send an explicit contract-shaped response instead of disappearing here. Keep this matcher only for non-loading actions whose production GPUI bridge is still absent, and do not claim success for installers, Launch Services, Ghostty config, preferences panes, or sound previews.

    CDXC:GPUISettingsActionBridge 2026-06-24-11:59:
    Worker 7 removed the remaining non-privileged Settings action commands from this matcher. New GPUI Settings commands should either perform a bounded action, refresh a visible status, or return an explicit unsupported status/toast instead of being added here by default.
    */
    let _ = command_type;
    false
}

pub(crate) fn gpui_open_url(url: &'static str) -> Result<(), String> {
    /*
    CDXC:GPUISettingsActionBridge 2026-06-24-11:59:
    Settings URL actions in GPUI are bounded to hardcoded product URLs from Rust. Do not accept React-provided URLs, shell commands, environment values, or user paths for docs/System Settings opens.
    */
    gpui_spawn_os_open(std::ffi::OsStr::new(url))
}

/// Terminal link clicks (Ghostty OPEN_URL actions) carry runtime-provided
/// text and only fire on an explicit cmd+click, so this mirrors the macOS
/// host's open-url handling: text with a URL scheme opens with its default
/// handler, and anything else is treated as a file path (`~` expanded) so
/// the ghostty link regex's absolute/relative path matches open too.
pub(crate) fn gpui_open_terminal_action_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Terminal link is empty.".to_string());
    }
    if trimmed.len() > 2048 {
        return Err("Terminal link is too long.".to_string());
    }
    let open_value = gpui_terminal_markdown_image_reference_path(trimmed).unwrap_or(trimmed);
    if gpui_terminal_link_has_scheme(open_value) {
        return gpui_spawn_os_open(std::ffi::OsStr::new(open_value));
    }
    gpui_spawn_os_open(gpui_expand_terminal_link_path(open_value).as_os_str())
}

pub(crate) fn gpui_terminal_link_is_web_url(link: &str) -> bool {
    link.get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || link
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
}

pub(crate) fn gpui_titlebar_resource_groups_from_presentation_snapshot(
    snapshot: &serde_json::Value,
    active_project_id: Option<&str>,
) -> Vec<serde_json::Value> {
    let Some(snapshot) = snapshot.as_object() else {
        return Vec::new();
    };
    let projects_by_id = json_array_field(snapshot, "projects")
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let project = project.as_object()?;
            let project_id = json_string_field(project, "projectId")?;
            Some((project_id.to_string(), project.clone()))
        })
        .collect::<HashMap<_, _>>();
    let sessions_by_key = json_array_field(snapshot, "sessions")
        .into_iter()
        .flatten()
        .filter_map(|session| {
            let session = session.as_object()?;
            let project_id = json_string_field(session, "projectId")?;
            let session_id = json_string_field(session, "sessionId")?;
            Some((
                (project_id.to_string(), session_id.to_string()),
                session.clone(),
            ))
        })
        .collect::<HashMap<_, _>>();

    let mut ordered_project_ids: Vec<String> = Vec::new();
    let mut sessions_by_project: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for group in json_array_field(snapshot, "groups").into_iter().flatten() {
        let Some(group) = group.as_object() else {
            continue;
        };
        let Some(project_id) = json_string_field(group, "projectId") else {
            continue;
        };
        for session_id in json_array_field(group, "sessionIds").into_iter().flatten() {
            let Some(session_id) = session_id.as_str() else {
                continue;
            };
            let Some(session) =
                sessions_by_key.get(&(project_id.to_string(), session_id.to_string()))
            else {
                continue;
            };
            let Some(value) = gpui_titlebar_resource_session_from_presentation(project_id, session)
            else {
                continue;
            };
            let sessions = sessions_by_project
                .entry(project_id.to_string())
                .or_default();
            if sessions.is_empty() {
                ordered_project_ids.push(project_id.to_string());
            }
            sessions.push(value);
        }
    }

    ordered_project_ids
        .into_iter()
        .filter_map(|project_id| {
            let sessions = sessions_by_project.remove(&project_id)?;
            let project = projects_by_id.get(&project_id);
            let project_path = project
                .and_then(|project| json_string_field(project, "path"))
                .unwrap_or_default();
            let project_name = project
                .and_then(|project| json_string_field(project, "title"))
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .or_else(|| {
                    project_path
                        .rsplit('/')
                        .find(|component| !component.is_empty())
                })
                .unwrap_or(project_id.as_str())
                .to_string();
            Some(serde_json::json!({
                "groupId": gpui_combined_presentation_project_group_id(&project_id),
                "isActive": active_project_id == Some(project_id.as_str()),
                "projectId": project_id,
                "projectName": project_name,
                "projectPath": project_path,
                "sessions": sessions,
                "title": project_name,
            }))
        })
        .collect()
}

pub(crate) fn gpui_titlebar_resource_session_from_presentation(
    project_id: &str,
    session: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    if json_bool_field(session, "visibleInSidebarByDefault") != Some(true)
        || json_string_field(session, "surface") == Some("commands")
    {
        return None;
    }
    let session_id = json_string_field(session, "sessionId")?;
    let session_kind = match json_string_field(session, "kind")? {
        "agent" | "terminal" => "terminal",
        "browser" => "browser",
        _ => return None,
    };
    let lifecycle_state = json_string_field(session, "lifecycleState").unwrap_or("unknown");
    let is_running = lifecycle_state == "running";
    let title = json_string_field(session, "displayTitle")
        .or_else(|| json_string_field(session, "primaryTitle"))
        .or_else(|| json_string_field(session, "title"))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Terminal");
    let mut value = serde_json::json!({
        "activity": gpui_daemon_agent_status(json_string_field(session, "activity")),
        "isLive": is_running,
        "isRunning": is_running,
        "isSleeping": lifecycle_state == "sleeping",
        "projectId": project_id,
        "sessionId": gpui_combined_presentation_session_id(project_id, session_id),
        "sessionKind": session_kind,
        "terminalTitle": title,
        "title": title,
    });
    let value_object = value.as_object_mut()?;
    /*
    CDXC:StashedPromptSessionAssociation 2026-08-24:
    Carry gxserver's provider conversation id onto the projected row. App-modal
    surfaces that hydrate from this Quick Access projection (Saved Prompts, for
    one) need it to tell which rows belong to the conversation they were opened
    for, because the visible `sessionId` above is the combined presentation id
    and a conversation outlives the gxserver session row that hosts it.
    */
    gpui_insert_optional_string(
        value_object,
        "agentSessionId",
        json_string_field(session, "agentSessionId"),
    );
    /*
    Quick Access reuses this bounded live-session projection. Preserve the
    gxserver-owned visible title and its comparison metadata so the shared
    session card does not reinterpret a confirmed title as an unsynced local
    terminal title and add the `∗` marker.
    */
    gpui_insert_optional_string(
        value_object,
        "displayTitle",
        json_string_field(session, "displayTitle"),
    );
    gpui_insert_optional_string(
        value_object,
        "displayTitleTooltip",
        json_string_field(session, "displayTitleTooltip"),
    );
    gpui_insert_optional_string(
        value_object,
        "primaryTitle",
        json_string_field(session, "primaryTitle").or_else(|| json_string_field(session, "title")),
    );
    if let Some(is_primary_title_terminal_title) =
        json_bool_field(session, "isPrimaryTitleTerminalTitle")
    {
        value_object.insert(
            "isPrimaryTitleTerminalTitle".to_string(),
            serde_json::Value::Bool(is_primary_title_terminal_title),
        );
    }
    /*
    gxserver presentation rows identify agents with values such as `cursor`,
    `droid`, and `grok`, while SidebarSessionItem.agentIcon requires the
    canonical shared icon ids (`cursor-cli`, `factory-droid`, `grok-build`,
    and so on). This projection now feeds Quick Access session cards as well as
    the titlebar Resources list, so normalize at the contract boundary before
    React indexes its closed set of icon metadata.
    */
    gpui_insert_optional_string(
        value_object,
        "agentIcon",
        gpui_sidebar_agent_icon(json_string_field(session, "agentIcon")),
    );
    gpui_insert_optional_string(
        value_object,
        "lastInteractionAt",
        json_string_field(session, "meaningfulActivityAt")
            .or_else(|| json_string_field(session, "lastActiveAt"))
            .or_else(|| json_string_field(session, "updatedAt")),
    );
    gpui_insert_optional_string(
        value_object,
        "providerSessionState",
        json_string_field(session, "providerSessionState").filter(|state| {
            matches!(
                *state,
                "exists" | "missing" | "persistence-disabled" | "unknown"
            )
        }),
    );
    gpui_insert_optional_string(
        value_object,
        "sessionPersistenceName",
        json_string_field(session, "zmxName"),
    );
    gpui_insert_optional_string(
        value_object,
        "sessionPersistenceProvider",
        json_string_field(session, "sessionPersistenceProvider")
            .filter(|provider| matches!(*provider, "tmux" | "zmx" | "zellij")),
    );
    Some(value)
}

pub(crate) fn gpui_daemon_session_items_from_presentation_snapshot(
    snapshot: &serde_json::Value,
    active_project_id: Option<&str>,
) -> Vec<serde_json::Value> {
    let Some(snapshot) = snapshot.as_object() else {
        return Vec::new();
    };
    let projects_by_id = json_array_field(snapshot, "projects")
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let project = project.as_object()?;
            let project_id = json_string_field(project, "projectId")?;
            Some((project_id.to_string(), project.clone()))
        })
        .collect::<HashMap<_, _>>();
    json_array_field(snapshot, "sessions")
        .into_iter()
        .flatten()
        .filter_map(|session| {
            gpui_presentation_session_to_daemon_session_item(
                session,
                &projects_by_id,
                active_project_id,
            )
        })
        .collect()
}

pub(crate) fn gpui_presentation_session_to_daemon_session_item(
    session: &serde_json::Value,
    projects_by_id: &HashMap<String, serde_json::Map<String, serde_json::Value>>,
    active_project_id: Option<&str>,
) -> Option<serde_json::Value> {
    let session = session.as_object()?;
    let kind = json_string_field(session, "kind")?;
    let surface = json_string_field(session, "surface")?;
    if surface != "workspace" || !matches!(kind, "terminal" | "agent") {
        return None;
    }
    let project_id = json_string_field(session, "projectId")?;
    let session_id = json_string_field(session, "sessionId")?;
    let project = projects_by_id.get(project_id);
    let cwd = json_string_field(session, "cwd")
        .or_else(|| project.and_then(|project| json_string_field(project, "path")))
        .unwrap_or("");
    let title = json_string_field(session, "displayTitle")
        .or_else(|| json_string_field(session, "primaryTitle"))
        .or_else(|| json_string_field(session, "title"));
    let started_at = json_string_field(session, "createdAt")?;
    let shell = json_string_field(session, "sessionPersistenceProvider").unwrap_or("");
    let lifecycle_state = json_string_field(session, "lifecycleState").unwrap_or("unknown");
    let provider_state = json_string_field(session, "providerSessionState").unwrap_or("unknown");
    let status = gpui_daemon_session_status_from_gxserver(lifecycle_state, provider_state);
    let mut item = serde_json::Map::new();
    gpui_insert_optional_string(
        &mut item,
        "agentName",
        json_string_field(session, "agentName").or_else(|| json_string_field(session, "agentId")),
    );
    item.insert(
        "agentStatus".to_string(),
        serde_json::Value::String(
            gpui_daemon_agent_status(json_string_field(session, "activity")).to_string(),
        ),
    );
    /*
    CDXC:GPUIDaemonSessionsModal 2026-06-24-12:00:
    gxserver presentation does not currently expose terminal dimensions. Keep cols/rows as explicit zero values in GPUI Running Sessions until a real dimensions contract exists instead of inventing 80x24 or reading terminal/private process state.
    */
    item.insert("cols".to_string(), serde_json::Value::Number(0.into()));
    item.insert(
        "cwd".to_string(),
        serde_json::Value::String(cwd.to_string()),
    );
    item.insert(
        "isCurrentWorkspace".to_string(),
        serde_json::Value::Bool(active_project_id == Some(project_id)),
    );
    item.insert(
        "ownership".to_string(),
        serde_json::Value::String("gxserver".to_string()),
    );
    item.insert(
        "restoreState".to_string(),
        serde_json::Value::String("live".to_string()),
    );
    item.insert("rows".to_string(), serde_json::Value::Number(0.into()));
    item.insert(
        "sessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    item.insert(
        "shell".to_string(),
        serde_json::Value::String(shell.to_string()),
    );
    item.insert(
        "startedAt".to_string(),
        serde_json::Value::String(started_at.to_string()),
    );
    item.insert(
        "status".to_string(),
        serde_json::Value::String(status.to_string()),
    );
    gpui_insert_optional_string(&mut item, "title", title);
    item.insert(
        "workspaceId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    Some(serde_json::Value::Object(item))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarActionType {
    Browser,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarActionRunMode {
    Default,
    Debug,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarActionLinkTarget {
    External,
    Integrated,
}

/*
CDXC:ProjectActions 2026-07-31-12:00:
Terminal Actions can carry saved links that open alongside the command run, so a
dev-server Action starts the server and surfaces its localhost URL in one click.
Each link targets the project's integrated Browser tab or the OS default browser.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarActionLink {
    pub(crate) target: GpuiTitlebarActionLinkTarget,
    pub(crate) url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarAction {
    pub(crate) action_type: GpuiTitlebarActionType,
    pub(crate) close_terminal_on_exit: bool,
    pub(crate) command: Option<String>,
    pub(crate) command_id: String,
    pub(crate) icon: Option<String>,
    pub(crate) links: Vec<GpuiTitlebarActionLink>,
    pub(crate) name: String,
    pub(crate) play_completion_sound: bool,
    pub(crate) run_mode: GpuiTitlebarActionRunMode,
    pub(crate) url: Option<String>,
}

impl GpuiTitlebarAction {
    pub(crate) fn is_configured(&self) -> bool {
        match self.action_type {
            GpuiTitlebarActionType::Browser => self
                .url
                .as_deref()
                .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
                .is_some(),
            GpuiTitlebarActionType::Terminal => self
                .command
                .as_deref()
                .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
                .is_some(),
        }
    }

    #[allow(dead_code)] // no caller: the native titlebar menus that used these labels were replaced by CEF panels
    pub(crate) fn menu_label(&self) -> String {
        self.action_title()
            .unwrap_or_else(|| self.command_id.clone())
    }

    pub(crate) fn titlebar_menu_name(&self) -> String {
        gpui_normalized_sidebar_command_title(Some(&self.name))
            .unwrap_or_else(|| self.command_id.clone())
    }

    pub(crate) fn titlebar_menu_preview(&self) -> (String, bool) {
        let preview = match self.action_type {
            GpuiTitlebarActionType::Browser => self
                .url
                .as_deref()
                .and_then(|url| gpui_trimmed_nonempty_str(Some(url))),
            GpuiTitlebarActionType::Terminal => self
                .command
                .as_deref()
                .and_then(|command| gpui_trimmed_nonempty_str(Some(command))),
        };
        preview
            .map(|preview| (preview.to_string(), false))
            .unwrap_or_else(|| (TITLEBAR_ACTION_UNCONFIGURED_PREVIEW.to_string(), true))
    }

    pub(crate) fn command_title(&self) -> String {
        /*
        CDXC:GPUICommandPane 2026-06-25-11:42:
        Command-pane Action tabs must use the same visible title rule as macOS: normalized Action name first, otherwise the normalized command text truncated to 20 characters. Do not substitute command ids for unnamed terminal Actions because title-owned reuse and duplicate-title checks depend on this user-facing Action title.
        */
        gpui_normalized_sidebar_command_title(Some(&self.name))
            .or_else(|| {
                gpui_normalized_sidebar_command_title(self.command.as_deref())
                    .map(gpui_sidebar_command_short_title)
            })
            .unwrap_or_else(|| self.command_id.clone())
    }

    #[allow(dead_code)] // no caller: the native titlebar menus that used these labels were replaced by CEF panels
    pub(crate) fn action_title(&self) -> Option<String> {
        gpui_normalized_sidebar_command_title(Some(&self.name)).or_else(|| {
            match self.action_type {
                GpuiTitlebarActionType::Browser => self.url.as_deref(),
                GpuiTitlebarActionType::Terminal => self.command.as_deref(),
            }
            .and_then(|target| {
                gpui_normalized_sidebar_command_title(Some(target))
                    .map(gpui_sidebar_command_short_title)
            })
        })
    }
}

pub(crate) const GPUI_SIDEBAR_METADATA_GENERIC_ERROR: &str = "sidebar metadata write failed";
pub(crate) const GPUI_SIDEBAR_DUPLICATE_ACTION_TITLE_ERROR: &str = "duplicate action title";

pub(crate) fn gpui_titlebar_actions_for_active_project_id(
    active_project_id: Option<&str>,
) -> Vec<GpuiTitlebarAction> {
    /*
    CDXC:GPUITitlebarActions 2026-06-24-14:24:
    The visible GPUI titlebar Actions control must run the same sidebar/gxserver-projected command definitions that Settings and the SidebarApp expose. Read only the shared `hud.commands` contract shape from gxserver-derived project metadata, keep command text and URLs in memory for immediate Browser/command-terminal routing, and never infer actions from paths, git state, labels, env, terminal titles, or filesystem probes.

    CDXC:SidebarHudContract 2026-06-24-20:34:
    Titlebar Actions consume `/api/readSidebarHud` so active-project command scoping comes from the production gxserver contract rather than the GPUI app-modal Rust mirror.
    */
    let commands = gpui_sidebar_hud_from_gxserver(Duration::from_secs(2), active_project_id)
        .map(|hud| hud.commands)
        .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
    gpui_titlebar_actions_from_sidebar_command_buttons(&commands)
}

pub(crate) fn gpui_titlebar_actions_from_sidebar_command_buttons(
    buttons: &serde_json::Value,
) -> Vec<GpuiTitlebarAction> {
    let Some(items) = buttons.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(gpui_titlebar_action_from_sidebar_command_button)
        .collect()
}

pub(crate) fn gpui_titlebar_action_from_sidebar_command_button(
    value: &serde_json::Value,
) -> Option<GpuiTitlebarAction> {
    let object = value.as_object()?;
    let command_id = gpui_trimmed_json_string_field(object, "commandId")?.to_string();
    let action_type = match object.get("actionType").and_then(serde_json::Value::as_str) {
        Some("browser") => GpuiTitlebarActionType::Browser,
        Some("terminal") | None => GpuiTitlebarActionType::Terminal,
        _ => return None,
    };
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let command = object
        .get("command")
        .and_then(serde_json::Value::as_str)
        .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
        .map(str::to_string);
    let url = object
        .get("url")
        .and_then(serde_json::Value::as_str)
        .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
        .map(str::to_string);
    Some(GpuiTitlebarAction {
        action_type,
        close_terminal_on_exit: action_type == GpuiTitlebarActionType::Terminal
            && object
                .get("closeTerminalOnExit")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        command,
        command_id,
        icon: object
            .get("icon")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_sidebar_command_icon)
            .map(str::to_string),
        links: if action_type == GpuiTitlebarActionType::Terminal {
            gpui_titlebar_action_links_from_sidebar_command_button(object)
        } else {
            Vec::new()
        },
        name,
        play_completion_sound: action_type == GpuiTitlebarActionType::Terminal
            && object
                .get("playCompletionSound")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
        run_mode: GpuiTitlebarActionRunMode::Default,
        url,
    })
}

pub(crate) fn gpui_titlebar_action_links_from_sidebar_command_button(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<GpuiTitlebarActionLink> {
    object
        .get("links")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_object)
        .filter_map(|item| {
            let url = item
                .get("url")
                .and_then(serde_json::Value::as_str)
                .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))?;
            Some(GpuiTitlebarActionLink {
                target: match item.get("target").and_then(serde_json::Value::as_str) {
                    Some("external") => GpuiTitlebarActionLinkTarget::External,
                    _ => GpuiTitlebarActionLinkTarget::Integrated,
                },
                url: url.to_string(),
            })
        })
        .collect()
}

pub(crate) fn gpui_titlebar_session_persistence_provider_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> &'static str {
    match settings
        .get("sessionPersistenceProvider")
        .and_then(serde_json::Value::as_str)
    {
        Some("off") => "off",
        Some("tmux") => "tmux",
        Some("zellij") => "zellij",
        Some("zmx") => "zmx",
        _ => "zmx",
    }
}

pub(crate) fn gpui_completion_sound_label(sound: &str) -> &'static str {
    match gpui_normalize_completion_sound(Some(sound)) {
        "success-chime" => "Success Chime",
        "flawless-victory" => "Flawless Victory",
        "pingdouble" => "Ping Double",
        "ping" => "Ping",
        "glass" => "Glass",
        "glimmer" => "Glimmer",
        "shamisen" => "Shamisen",
        "shamisenreverb" => "Shamisen Reverb",
        "arcadeboost" => "Arcade Boost",
        "confirmation-001" => "Confirmation 001",
        "confirmation-002" => "Confirmation 002",
        "confirmation-003" => "Confirmation 003",
        "confirmation-004" => "Confirmation 004",
        "notification-pop" => "Notification Pop",
        "high-up" => "High Up",
        "high-down" => "High Down",
        "low-three-tone" => "Low Three Tone",
        "tone-1" => "Tone 1",
        "three-tone-1" => "Three Tone 1",
        "three-tone-2" => "Three Tone 2",
        "two-tone-1" => "Two Tone 1",
        "two-tone-2" => "Two Tone 2",
        "power-up-5" => "Power Up 5",
        "power-up-6" => "Power Up 6",
        "power-up-8" => "Power Up 8",
        "coin-collect" => "Coin Collect",
        "phaser-up-5" => "Phaser Up 5",
        "zap-two-tone" => "Zap Two Tone",
        "voiceover-pack-male-mission-completed" => "Mission Completed (Male)",
        "voiceover-pack-female-mission-completed" => "Mission Completed (Female)",
        "voiceover-pack-male-you-win" => "You Win (Male)",
        "voiceover-pack-female-congratulations" => "Congratulations (Female)",
        _ => "Arcade",
    }
}

pub(crate) fn append_url_query_params(mut url: String, params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return url;
    }
    url.push(if url.contains('?') { '&' } else { '?' });
    for (index, (key, value)) in params.iter().enumerate() {
        if index > 0 {
            url.push('&');
        }
        url.push_str(&encode_search_query(key));
        url.push('=');
        url.push_str(&encode_search_query(value));
    }
    url
}

pub(crate) fn append_url_query_params_with_percent_encoded_spaces(
    mut url: String,
    params: &[(&str, String)],
) -> String {
    if params.is_empty() {
        return url;
    }
    url.push(if url.contains('?') { '&' } else { '?' });
    for (index, (key, value)) in params.iter().enumerate() {
        if index > 0 {
            url.push('&');
        }
        url.push_str(&encode_search_query(key).replace('+', "%20"));
        url.push('=');
        url.push_str(&encode_search_query(value).replace('+', "%20"));
    }
    url
}

pub(crate) fn titlebar_mode_switcher_items(
    availability: ProjectScopedWorkareaAvailability,
) -> Vec<TitlebarModeSwitcherItem> {
    /*
    CDXC:GPUTitlebarAvailability 2026-07-04-01:00:
    The GPUI titlebar mode list mirrors macOS Quick/projectless presentation: Agents and Source are always visible and selectable; Browser, Kanban, Automate, and Docs stay visible but disabled in Quick context. Activation, hotkeys, restored active mode, and persisted active mode delegate to the same context availability helper.

    CDXC:GPUIProjectScopedWorkareaAvailability 2026-06-22-18:00:
    Kanban, Automate, and Docs must be unavailable without a project, and GPUI currently shares Browser's Quick/projectless disablement through the same titlebar contract. Until a real GPUI project/sidebar snapshot exists, keep GHOSTEX_GPUI_PROJECT_IS_QUICK isolated behind GpuiProjectContext and pass a typed ProjectScopedWorkareaAvailability into mode lists and action guards instead of adding git/path heuristics or fallback project detection.

    CDXC:GPUIProjectScopedWorkareaAvailability 2026-06-22-19:44:
    Runtime App titlebar mode lists, activation guards, and active-mode coercion prefer the latest valid in-memory sidebar project snapshot when available. The fallback availability is supplied by the caller so app runtime code can choose its current strict source without persisting or logging raw snapshot details.

    CDXC:GPUIProjectSidebarBridge 2026-07-04-01:00:
    App-owned titlebar mode lists and active-mode fallback receive fallback availability from the current project context, but Docs/Manage titlebar visibility is unconditional and only project context can disable it.
    */
    availability.titlebar_mode_switcher_items()
}

pub(crate) struct GpuiExtensionViewPresentation {
    pub(crate) title: String,
    pub(crate) icon_data_url: String,
    pub(crate) server_is_static: bool,
}

pub(crate) fn gpui_extension_view_presentation(
    id: ExtensionId,
) -> Option<GpuiExtensionViewPresentation> {
    let payload_dir = shared_settings::ghostex_storage_paths()
        .extensions_dir()
        .join("installed")
        .join(id.as_str());
    let manifest_text = std::fs::read_to_string(payload_dir.join("ghostex-extension.json")).ok()?;
    let manifest = serde_json::from_str::<serde_json::Value>(&manifest_text)
        .ok()?
        .as_object()?
        .clone();
    if manifest.get("name")?.as_str()? != id.as_str() {
        return None;
    }
    let title = manifest.get("title")?.as_str()?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let icon_path = std::path::Path::new(manifest.get("icon")?.as_str()?);
    if icon_path.is_absolute()
        || icon_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
    {
        return None;
    }
    let icon = std::fs::read(payload_dir.join(icon_path)).ok()?;
    if icon.is_empty() || icon.len() > 256 * 1024 {
        return None;
    }
    Some(GpuiExtensionViewPresentation {
        title,
        icon_data_url: format!("data:image/svg+xml;base64,{}", BASE64_STANDARD.encode(icon)),
        server_is_static: manifest
            .get("server")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|server| server.get("static").is_some()),
    })
}

pub(crate) fn titlebar_mode_view_tab_hidden_settings_key(
    mode: TitlebarMode,
) -> Option<&'static str> {
    match mode {
        TitlebarMode::Source => Some(SOURCE_CODE_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Browser => Some(BROWSER_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Kanban => Some(KANBAN_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Automate => Some(AUTOMATE_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Manage => Some(DOCS_VIEW_TAB_HIDDEN_SETTINGS_KEY),
        TitlebarMode::Agents | TitlebarMode::Extension(_) => None,
    }
}

/*
CDXC:DisabledPluginRouting 2026-08-23:
Toasts and menus name a workarea the way Settings → Customize does, which is
not always the way the enum does: `Source` is "Code" and `Manage` is "Docs"
everywhere the user can read it.
*/
pub(crate) fn gpui_titlebar_mode_plugin_display_name(mode: TitlebarMode) -> &'static str {
    match mode {
        TitlebarMode::Agents => "Agents",
        TitlebarMode::Source => "Code",
        TitlebarMode::Browser => "Browser",
        TitlebarMode::Kanban => "Kanban",
        TitlebarMode::Automate => "Automate",
        TitlebarMode::Manage => "Docs",
        TitlebarMode::Extension(id) => id.as_str(),
    }
}

/// What the copied target *is*, for a disabled-workarea toast: Browser is the
/// only workarea reached by a web link, every other one is reached by a path.
pub(crate) fn gpui_disabled_project_workarea_copy_noun(mode: TitlebarMode) -> &'static str {
    match mode {
        TitlebarMode::Browser => "Link",
        _ => "Path",
    }
}

pub(crate) fn gpui_titlebar_mode_hidden_from_settings(mode: TitlebarMode) -> bool {
    let Some(settings_key) = titlebar_mode_view_tab_hidden_settings_key(mode) else {
        return false;
    };
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get(settings_key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn gpui_titlebar_git_action_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onTitlebarGitAction==='function'){{bridge.onTitlebarGitAction(payload);}}else{{const pending=Array.isArray(bridge.pendingTitlebarGitActions)?bridge.pendingTitlebarGitActions:[];pending.push(payload);bridge.pendingTitlebarGitActions=pending;}}}})(); undefined;"
    )
}
