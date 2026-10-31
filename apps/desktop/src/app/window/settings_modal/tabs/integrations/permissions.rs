//! The Accessibility and Screen Recording rows of SpaceO and Fast Computer & Browser Use in their
//! tool sections (tool_sections.rs), and the re-checks that keep their status live while the page is
//! open (the grant flow itself is in helpers/os_cli/tool_permissions.rs).

use super::*;
use gpui::Subscription;
use std::time::Instant;

const ICON_ACCESSIBILITY: &str = "modals/settings/shield.svg";
const ICON_SCREEN_RECORDING: &str = "modals/settings/eye.svg";

/// While a Grant is pending the page re-checks every 2 seconds for at most 10 minutes;
/// otherwise every 10 seconds.
const WATCH_INTERVAL: Duration = Duration::from_secs(2);
const WATCH_LIMIT: Duration = Duration::from_secs(10 * 60);
const IDLE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PermissionTool {
    Spaceo,
    CuaDriver,
}

impl PermissionTool {
    fn id(self) -> &'static str {
        match self {
            Self::Spaceo => "spaceo",
            Self::CuaDriver => "cuaDriver",
        }
    }

    fn name(self) -> String {
        match self {
            Self::Spaceo => spaceo::spaceo_name(),
            Self::CuaDriver => trycua_name(),
        }
    }

    /// The entry macOS lists in System Settings for this tool's grants.
    fn grantee(self) -> &'static str {
        match self {
            Self::Spaceo => "spaceo",
            Self::CuaDriver => "Cua Driver",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Permission {
    Accessibility,
    ScreenRecording,
}

impl Permission {
    const ALL: [Self; 2] = [Self::Accessibility, Self::ScreenRecording];

    fn id(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::ScreenRecording => "screenRecording",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Accessibility => "Accessibility",
            Self::ScreenRecording => "Screen Recording",
        }
    }

    /// The pane's name in System Settings, which since macOS 15 is longer than the permission's.
    fn pane(self) -> &'static str {
        match self {
            Self::Accessibility => "Accessibility",
            Self::ScreenRecording => "Screen & System Audio Recording",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionState {
    Checking,
    NotInstalled,
    Granted,
    NotGranted,
    NeedsRestart,
    Restarting,
    Unknown,
}

/// The state the host computed (`GpuiToolPermissionState`), one string per grant.
fn permission_state(
    status: Option<&Value>,
    tool: PermissionTool,
    permission: Permission,
) -> PermissionState {
    if status.is_none() {
        return PermissionState::Checking;
    }
    let (installed, accessibility, screen_recording, restarting) = match tool {
        PermissionTool::Spaceo => (
            "spaceoInstalled",
            "spaceoAccessibilityPermission",
            "spaceoScreenRecordingPermission",
            "spaceoRestarting",
        ),
        PermissionTool::CuaDriver => (
            "cuaDriverInstalled",
            "cuaDriverAccessibilityPermission",
            "cuaDriverScreenRecordingPermission",
            "cuaDriverRestarting",
        ),
    };
    if flag(status, installed) != Some(true) {
        return PermissionState::NotInstalled;
    }
    let key = match permission {
        Permission::Accessibility => accessibility,
        Permission::ScreenRecording => screen_recording,
    };
    match text(status, key) {
        Some("granted") => PermissionState::Granted,
        Some("notGranted") => PermissionState::NotGranted,
        Some("needsRestart") if flag(status, restarting) == Some(true) => {
            PermissionState::Restarting
        }
        Some("needsRestart") => PermissionState::NeedsRestart,
        _ => PermissionState::Unknown,
    }
}

/// During a Grant CuaDriver restarts by itself once macOS has both permissions and its daemon
/// still can't use them, since the user already asked for this to work. Once per Grant: a daemon
/// still saying no after that keeps the row's Restart button instead of restarting in a loop.
fn cua_driver_should_auto_restart(states: &[PermissionState]) -> bool {
    states.contains(&PermissionState::NeedsRestart)
        && states.iter().all(|state| {
            matches!(
                state,
                PermissionState::Granted | PermissionState::NeedsRestart
            )
        })
}

/// A Grant click: the tool's grants are re-checked every 2 seconds until they land.
struct PermissionWatch {
    tool: PermissionTool,
    started: Instant,
    restart_requested: bool,
}

#[derive(Default)]
pub(super) struct PermissionPolling {
    watch: Option<PermissionWatch>,
    last_check: Option<Instant>,
    ticker: Option<Task<()>>,
    activation: Option<Subscription>,
}

impl IntegrationsTab {
    /// Starts the once-a-second tick that decides when to re-check (`permission_tick`).
    pub(super) fn start_permission_polling(&mut self, cx: &mut Context<Self>) {
        if !cfg!(target_os = "macos") {
            return;
        }
        self.permissions.ticker = Some(cx.spawn(async move |page, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if page
                    .update(cx, |page, cx| page.permission_tick(cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    /// Coming back from System Settings re-checks at the next tick instead of waiting.
    pub(super) fn observe_permission_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.permissions.activation.is_some() || !cfg!(target_os = "macos") {
            return;
        }
        self.permissions.activation = Some(cx.observe_window_activation(
            window,
            |page: &mut Self, window, _cx| {
                if window.is_window_active() {
                    page.permissions.last_check = None;
                }
            },
        ));
    }

    fn permission_tick(&mut self, cx: &mut Context<Self>) {
        let (active, status) = {
            let store = self.store.read(cx);
            (
                store.active_tab() == SettingsTabId::Integrations,
                store.host_payload("ghostexCliStatus").cloned(),
            )
        };
        let Some(status) = status else {
            return;
        };
        let status = Some(&status);
        if !active
            || (flag(status, "spaceoInstalled") != Some(true)
                && flag(status, "cuaDriverInstalled") != Some(true))
        {
            return;
        }
        if let Some(watch) = self.permissions.watch.as_mut() {
            let states =
                Permission::ALL.map(|permission| permission_state(status, watch.tool, permission));
            if states
                .iter()
                .all(|state| *state == PermissionState::Granted)
            {
                let name = watch.tool.name();
                self.permissions.watch = None;
                let description = format!("{name} has Accessibility and Screen Recording.");
                self.store.update(cx, |store, cx| {
                    store.toast("success", "Permissions granted", &description, cx)
                });
                cx.notify();
            } else if watch.started.elapsed() >= WATCH_LIMIT {
                self.permissions.watch = None;
                cx.notify();
            } else if watch.tool == PermissionTool::CuaDriver
                && !watch.restart_requested
                && cua_driver_should_auto_restart(&states)
            {
                watch.restart_requested = true;
                post_store_message(&self.store, json!({ "type": "restartCuaDriver" }), cx);
            }
        }
        let interval = if self.permissions.watch.is_some() {
            WATCH_INTERVAL
        } else {
            IDLE_INTERVAL
        };
        if self
            .permissions
            .last_check
            .is_none_or(|checked| checked.elapsed() >= interval)
        {
            self.permissions.last_check = Some(Instant::now());
            post_store_message(
                &self.store,
                json!({ "type": "requestToolPermissionStatus" }),
                cx,
            );
        }
    }

    fn start_permission_grant(
        &mut self,
        tool: PermissionTool,
        permission: Permission,
        cx: &mut Context<Self>,
    ) {
        post_store_message(
            &self.store,
            json!({ "type": "grantToolPermission", "tool": tool.id(), "permission": permission.id() }),
            cx,
        );
        self.permissions.watch = Some(PermissionWatch {
            tool,
            started: Instant::now(),
            restart_requested: false,
        });
        cx.notify();
    }

    /// One row per grant. Every row always shows its status, granted or not.
    pub(super) fn permission_rows(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        tool: PermissionTool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        Permission::ALL
            .into_iter()
            .map(|permission| self.permission_row(p, status, tool, permission, cx))
            .collect()
    }

    fn permission_row(
        &mut self,
        p: &SettingsPalette,
        status: Option<&Value>,
        tool: PermissionTool,
        permission: Permission,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = tool.name();
        let state = permission_state(status, tool, permission);
        let watching = self
            .permissions
            .watch
            .as_ref()
            .is_some_and(|watch| watch.tool == tool);
        let why = match (tool, permission) {
            (PermissionTool::Spaceo, Permission::Accessibility) => {
                format!("Lets {name} click and type in the apps it runs for agents.")
            }
            (PermissionTool::Spaceo, Permission::ScreenRecording) => {
                format!("Lets {name} take screenshots of the apps it runs for agents.")
            }
            (PermissionTool::CuaDriver, Permission::Accessibility) => {
                format!("Lets {name} click and type in apps.")
            }
            (PermissionTool::CuaDriver, Permission::ScreenRecording) => {
                format!("Lets {name} see what is on the desktop.")
            }
        };
        let (status_label, tone) = match state {
            PermissionState::Checking => ("Checking", ListItemStatus::Neutral),
            PermissionState::NotInstalled => ("Not installed", ListItemStatus::Neutral),
            PermissionState::Granted => ("Granted", ListItemStatus::Success),
            PermissionState::NotGranted => ("Not granted", ListItemStatus::Warning),
            PermissionState::NeedsRestart => ("Needs restart", ListItemStatus::Warning),
            PermissionState::Restarting => ("Restarting", ListItemStatus::Warning),
            PermissionState::Unknown => ("Unknown", ListItemStatus::Neutral),
        };
        let grantee = tool.grantee();
        let pane = permission.label();
        let settings_pane = permission.pane();
        let grant_error = (tool == PermissionTool::CuaDriver)
            .then(|| text(status, "cuaDriverGrantError"))
            .flatten();
        let cua_daemon_stale = flag(status, "cuaDriverDaemonStale") == Some(true);
        let detail = match state {
            PermissionState::NotGranted | PermissionState::Unknown
                if watching && tool == PermissionTool::CuaDriver =>
            {
                let turn_on = format!(
                    "Turn on {grantee} in System Settings > Privacy & Security > {settings_pane}. This page updates as soon as you do. Not listed? Click + and add /Applications/CuaDriver.app. Already on? That entry is out of date: select it, remove it with the minus button, then click Grant again."
                );
                match grant_error {
                    Some(error) => format!("{error} {turn_on}"),
                    None => turn_on,
                }
            }
            PermissionState::NotGranted | PermissionState::Unknown if watching => format!(
                "Turn on {grantee} in System Settings > Privacy & Security > {settings_pane}. This page updates as soon as you do."
            ),
            PermissionState::NotGranted | PermissionState::Unknown if grant_error.is_some() => {
                format!(
                    "{} Click Grant to try again. If {grantee} is already on in {settings_pane}, that entry is out of date: select it, remove it with the minus button, then click Grant.",
                    grant_error.unwrap_or_default()
                )
            }
            PermissionState::NotGranted => format!(
                "{why} Grant asks macOS for it and opens System Settings, where you turn on {grantee}."
            ),
            PermissionState::Unknown => format!("{why} Ghostex could not read this permission."),
            PermissionState::NeedsRestart if tool == PermissionTool::CuaDriver => format!(
                "{} Restarting ends what agents are doing with it right now.",
                if cua_daemon_stale {
                    format!(
                        "{grantee} is still running a copy from before it was last reinstalled, so agents can't rely on this permission until it restarts."
                    )
                } else {
                    format!(
                        "macOS allows it, but {grantee}'s running service can't use it until it restarts."
                    )
                }
            ),
            PermissionState::NeedsRestart => format!(
                "macOS allowed it after {name} started, so {name} needs a restart to use it. The restart waits for agents using {name} to finish."
            ),
            PermissionState::Restarting if tool == PermissionTool::CuaDriver => {
                format!("{grantee} is restarting.")
            }
            PermissionState::Restarting => {
                format!("{name} restarts as soon as the agents using it finish.")
            }
            PermissionState::NotInstalled => format!("{why} Install {name} first."),
            PermissionState::Granted | PermissionState::Checking => why,
        };
        let row_id = format!("{}-{}", tool.id(), permission.id());
        let mut controls = vec![permission_status_label(p, status_label, tone)];
        match state {
            PermissionState::NotGranted | PermissionState::Unknown => {
                controls.push(settings_button(
                    p,
                    ElementId::Name(format!("integrations-grant-{row_id}").into()),
                    "Grant",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.start_permission_grant(tool, permission, cx)
                    },
                    cx,
                ));
            }
            PermissionState::NeedsRestart | PermissionState::Restarting => {
                let restarting = state == PermissionState::Restarting;
                let (restart_name, tooltip, message) = match tool {
                    PermissionTool::Spaceo => (
                        name.clone(),
                        format!("{name} restarts once the agents using it finish."),
                        "restartSpaceo",
                    ),
                    PermissionTool::CuaDriver => (
                        grantee.to_string(),
                        format!("{grantee} stops and starts again right away."),
                        "restartCuaDriver",
                    ),
                };
                controls.push(settings_button(
                    p,
                    ElementId::Name(format!("integrations-restart-{row_id}").into()),
                    if restarting {
                        format!("Restarting {restart_name}…")
                    } else {
                        format!("Restart {restart_name}")
                    },
                    restarting.then_some(ICON_LOADER),
                    ButtonVariant::Outline,
                    restarting,
                    Some(tooltip.into()),
                    move |page: &mut Self, _window, cx| {
                        post_store_message(&page.store, json!({ "type": message }), cx);
                    },
                    cx,
                ));
            }
            _ => {}
        }
        permission_row_layout(
            p,
            &row_id,
            tone,
            match permission {
                Permission::Accessibility => ICON_ACCESSIBILITY,
                Permission::ScreenRecording => ICON_SCREEN_RECORDING,
            },
            format!("{name}: {pane}"),
            detail,
            controls,
        )
    }
}

/// The status word with its dot, always visible at the row's end.
///
/// CDXC:Settings 2026-10-14 DECISION:
/// User: "we should always show the status of each permission after it's done." Permission rows spell their state out (Granted, Not granted, Needs restart) next to the dot, an exception to the 2026-09-09 dot-only rule the other Integrations rows keep.
fn permission_status_label(p: &SettingsPalette, label: &str, tone: ListItemStatus) -> AnyElement {
    let (dot, color) = match tone {
        ListItemStatus::Success => (modal_rgba(0x34d399, 0.8), modal_rgba(0x34d399, 1.0)),
        ListItemStatus::Warning => (modal_rgba(0xfbbf24, 0.85), modal_rgba(0xfbbf24, 1.0)),
        ListItemStatus::Neutral => (modal_rgba(0xffffff, 0.2), p.muted),
    };
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .child(div().size(px(6.0)).rounded_full().bg(hsla(dot)))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(if p.light { p.foreground } else { color }))
                .child(label.to_string()),
        )
        .into_any_element()
}

/// `integration_row`'s layout with the detail written under the label instead of behind the
/// info icon, so what to do next is always readable.
fn permission_row_layout(
    p: &SettingsPalette,
    id: &str,
    tone: ListItemStatus,
    icon: &'static str,
    label: String,
    detail: String,
    controls: Vec<AnyElement>,
) -> AnyElement {
    let dot = match tone {
        ListItemStatus::Success => modal_rgba(0x34d399, 0.8),
        ListItemStatus::Warning => modal_rgba(0xfbbf24, 0.85),
        ListItemStatus::Neutral => modal_rgba(0xffffff, 0.2),
    };
    h_flex()
        .id(ElementId::Name(format!("integration-row-{id}").into()))
        .w_full()
        .min_h(px(52.0))
        .px(px(20.0))
        .py(px(10.0))
        .gap(px(24.0))
        .items_center()
        .justify_between()
        .child(
            div()
                .flex_shrink_0()
                .size(px(6.0))
                .rounded_full()
                .bg(hsla(dot)),
        )
        .child(
            div()
                .flex_shrink_0()
                .size(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .child(settings_icon(icon, 18.0, p.muted)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.9))
                        .text_color(hsla(p.foreground))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.muted))
                        .child(detail),
                ),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .justify_end()
                .gap(px(12.0))
                .children(controls),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cua_driver_rows_read_the_hosts_per_grant_state() {
        let status = json!({
            "cuaDriverInstalled": true,
            "cuaDriverAccessibilityPermission": "granted",
            "cuaDriverScreenRecordingPermission": "needsRestart",
            "cuaDriverAccessibilityPermissionGranted": true,
            "cuaDriverScreenRecordingPermissionGranted": false,
        });
        let state =
            |permission| permission_state(Some(&status), PermissionTool::CuaDriver, permission);
        assert_eq!(state(Permission::Accessibility), PermissionState::Granted);
        assert_eq!(
            state(Permission::ScreenRecording),
            PermissionState::NeedsRestart
        );
    }

    #[test]
    fn a_restarting_cua_driver_shows_restarting_not_a_second_button() {
        let status = json!({
            "cuaDriverInstalled": true,
            "cuaDriverScreenRecordingPermission": "needsRestart",
            "cuaDriverRestarting": true,
        });
        assert_eq!(
            permission_state(
                Some(&status),
                PermissionTool::CuaDriver,
                Permission::ScreenRecording
            ),
            PermissionState::Restarting
        );
    }

    #[test]
    fn a_grant_restarts_cua_driver_only_once_macos_has_both() {
        use PermissionState::*;
        assert!(cua_driver_should_auto_restart(&[Granted, NeedsRestart]));
        assert!(cua_driver_should_auto_restart(&[
            NeedsRestart,
            NeedsRestart
        ]));
        assert!(!cua_driver_should_auto_restart(&[NeedsRestart, NotGranted]));
        assert!(!cua_driver_should_auto_restart(&[Granted, Granted]));
        assert!(!cua_driver_should_auto_restart(&[Restarting, Granted]));
    }
}
