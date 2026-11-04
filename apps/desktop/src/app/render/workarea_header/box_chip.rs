//! The header's box chip: shown while the work area shows a session that runs on a Cloud Box, a
//! pill with the box's state dot, its name, how long it has run and about what it has cost, whose
//! menu holds what only a box does (terminal, snapshots, the branch's pull request, stop, delete).
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION:
//! User (Q1000 "header and settings", Q1017 "chip and settings", D2 "time and money"): the chip
//! sits in the session header beside the session-details ring on every chat that runs on a cloud
//! box, shaped `● ☁ box-c31 · 42m · ≈$0.02`; the dot is blue while starting, green running, amber
//! idle (tooltip "Pauses in 6 min"), grey paused or stopped, red failed. Its menu offers Open
//! terminal, Save as snapshot, Update snapshot (with a dot when updating makes sense, D9), Pull
//! branch, Open PR, Stop and Delete; a box with unpushed work is deleted only on "Delete anyway" (D13).
//! SEE-ALSO: server/src/cloud_boxes/ (the box row and its actions).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use ghostex_gx_protocol::cloud_box::{CloudBox, CloudBoxState, cloud_box_id_from_machine_id};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use gpui_component::ElementExt as _;
use serde_json::json;

use crate::app::actions::CloudBoxMenuPicked;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::*;

const ICON_CLOUD: &str = "titlebar/cloud.svg";
/// A row older than this is read again from home gxserver when the chip draws.
const STALE_AFTER: Duration = Duration::from_secs(20);
/// The running time is a minute count, so the chip redraws about once a minute.
const TICK: Duration = Duration::from_secs(30);

thread_local! {
    static ANCHOR: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
    /// The last box rows read, with when.
    static ROWS: RefCell<HashMap<String, (CloudBox, Instant)>> = RefCell::new(HashMap::new());
    static READING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

fn cached_row(box_id: &str) -> Option<(CloudBox, Instant)> {
    ROWS.with(|rows| rows.borrow().get(box_id).cloned())
}

/// `42m`, `3h 05m`.
fn running_for(started_at: Option<&str>) -> Option<String> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at?).ok()?;
    let minutes = (chrono::Utc::now() - started.with_timezone(&chrono::Utc))
        .num_minutes()
        .max(0);
    Some(if minutes < 60 {
        format!("{minutes}m")
    } else {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    })
}

fn money(usd: f64) -> String {
    if usd < 0.1 {
        format!("${usd:.3}")
    } else {
        format!("${usd:.2}")
    }
}

/// `4 vCPU · 8 GB` from a size id (`4x8`).
fn size_label(size: Option<&str>) -> Option<String> {
    let (cpus, gigabytes) = size?.split_once('x')?;
    Some(format!("{cpus} vCPU · {gigabytes} GB"))
}

fn state_color(state: &CloudBoxState) -> u32 {
    match state {
        CloudBoxState::Pending | CloudBoxState::Creating | CloudBoxState::Booting => 0x3b82f6,
        CloudBoxState::Running => 0x22c55e,
        CloudBoxState::Idle => 0xf59e0b,
        CloudBoxState::Failed => 0xef4444,
        _ => 0x8a8a8a,
    }
}

fn state_words(row: &CloudBox) -> String {
    match &row.state {
        CloudBoxState::Pending | CloudBoxState::Creating | CloudBoxState::Booting => {
            "starting".to_string()
        }
        CloudBoxState::Idle => row
            .state_detail
            .clone()
            .unwrap_or_else(|| "idle".to_string()),
        CloudBoxState::Running => "running".to_string(),
        other => other.as_str().to_string(),
    }
}

impl GhostexGpuiApp {
    /// The box the work area's session runs on: its hidden machine (`cloudbox:<boxId>`), or the
    /// session's `cloudBox` field.
    fn workarea_cloud_box(&self) -> Option<(TerminalSessionId, String)> {
        let shell = self.focused_agents_or_companion_shell_session_id()?;
        let key = self.workspace_terminal_session_key(shell)?;
        if let Some(box_id) = key
            .machine
            .remote_id()
            .and_then(cloud_box_id_from_machine_id)
        {
            return Some((shell, box_id.to_string()));
        }
        let cloud = self
            .gx_store
            .core
            .presentation()
            .session(&key)?
            .cloud_box
            .clone()?;
        (!cloud.pending && !cloud.box_id.is_empty()).then_some((shell, cloud.box_id))
    }

    /// Reads the box row from home gxserver, then redraws; a read already running is not repeated.
    fn read_cloud_box_row(box_id: String, cx: &mut gpui::Context<Self>) {
        let started = READING.with(|reading| reading.borrow_mut().insert(box_id.clone()));
        if !started {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let params = json!({ "action": "get", "boxId": box_id });
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result("/api/cloudBoxes", &params, Duration::from_secs(15))
                })
                .await;
            if let Some(row) = result
                .ok()
                .and_then(|value| serde_json::from_value::<CloudBox>(value["box"].clone()).ok())
            {
                ROWS.with(|rows| {
                    rows.borrow_mut()
                        .insert(box_id.clone(), (row, Instant::now()))
                });
            }
            READING.with(|reading| reading.borrow_mut().remove(&box_id));
            let _ = this.update(cx, |_, cx| cx.notify());
            background.timer(TICK).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    pub(crate) fn render_workarea_header_box_chip(
        &self,
        compact: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let (_, box_id) = self.workarea_cloud_box()?;
        let cached = cached_row(&box_id);
        if cached
            .as_ref()
            .is_none_or(|(_, read_at)| read_at.elapsed() > STALE_AFTER)
        {
            Self::read_cloud_box_row(box_id.clone(), cx);
        }
        let row = cached.map(|(row, _)| row);
        let text = titlebar_active_text_color();
        let muted = titlebar_icon_color();
        let name = row
            .as_ref()
            .map(|row| row.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Cloud box".to_string());
        let mut parts = vec![name.clone()];
        let mut tooltip = Vec::new();
        if let Some(row) = &row {
            if row.state.is_reachable() {
                if let Some(time) = running_for(row.started_at.as_deref()) {
                    parts.push(time);
                }
            } else {
                parts.push(state_words(row));
            }
            if let Some(cost) = row.cost_total_usd {
                parts.push(format!("≈{}", money(cost)));
            }
            tooltip.push(row.provider.label().to_string());
            if let Some(size) = size_label(row.size.as_deref()) {
                tooltip.push(size);
            }
            if let Some(rate) = row.usd_per_hour {
                tooltip.push(format!("≈{}/h", money(rate)));
            }
            if let Some(cost) = row.cost_total_usd {
                tooltip.push(format!(
                    "{} so far{}",
                    money(cost),
                    if row.cost_estimated {
                        " (estimate)"
                    } else {
                        ""
                    }
                ));
            }
            if matches!(row.state, CloudBoxState::Idle | CloudBoxState::Failed) {
                if let Some(detail) = &row.state_detail {
                    tooltip.push(detail.clone());
                }
            }
        }
        let tooltip = if tooltip.is_empty() {
            name.clone()
        } else {
            tooltip.join(" · ")
        };
        let dot = row
            .as_ref()
            .map(|row| state_color(&row.state))
            .unwrap_or(0x8a8a8a);
        let label = if compact { name } else { parts.join(" · ") };
        Some(
            div()
                .id("ghostex-gpui-workarea-header-box-chip")
                .role(gpui::Role::Button)
                .aria_label(format!("Cloud box {label}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
                .rounded(px(TITLEBAR_BUTTON_RADIUS))
                .cursor_default()
                .when(cfg!(target_os = "windows"), |this| this.occlude())
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .child(div().size(px(7.0)).rounded_full().bg(rgb(dot)))
                .child(titlebar_svg_icon(ICON_CLOUD, 14.0, text))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(text)
                        .whitespace_nowrap()
                        .child(label),
                )
                .when(
                    row.as_ref()
                        .is_some_and(|row| row.snapshot_update_reason.is_some()),
                    |this| this.child(div().size(px(5.0)).rounded_full().bg(rgb(0x3b82f6))),
                )
                .child(titlebar_svg_icon(TITLEBAR_ICON_CHEVRON_DOWN, 11.0, muted))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_cloud_box_menu(cx);
                    }),
                )
                .on_a11y_action(gpui::AccessibleAction::Click, {
                    let app = cx.entity().downgrade();
                    move |_, _, cx| {
                        let _ = app.update(cx, |this, cx| this.toggle_cloud_box_menu(cx));
                    }
                })
                .on_prepaint(|bounds, _, _| ANCHOR.with(|anchor| anchor.set(Some(bounds))))
                .into_any_element(),
        )
    }

    /// Opens (or closes) the box menu under the chip.
    pub(crate) fn toggle_cloud_box_menu(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(main), Some(trigger)) =
            (self.main_window_handle, ANCHOR.with(|anchor| anchor.get()))
        else {
            return;
        };
        let Some((shell, box_id)) = self.workarea_cloud_box() else {
            return;
        };
        let row = cached_row(&box_id).map(|(row, _)| row);
        let reachable = row.as_ref().is_some_and(|row| row.state.is_reachable());
        let showing_terminal = !self.agents_chat_mode_sessions.contains(&shell);
        let pick = |pick: &'static str| -> Box<dyn gpui::Action> {
            Box::new(CloudBoxMenuPicked {
                shell_session_id: shell.0,
                box_id: box_id.clone(),
                pick,
            })
        };
        let update_label = match row
            .as_ref()
            .and_then(|row| row.snapshot_update_reason.clone())
        {
            Some(reason) => format!("Update snapshot • {reason}"),
            None => "Update snapshot".to_string(),
        };
        let has_branch = row
            .as_ref()
            .is_some_and(|row| row.sessions.iter().any(|session| session.branch.is_some()));
        let unpushed = row.as_ref().and_then(|row| row.has_unpushed_work) == Some(true);
        let menu = GpuiContextMenu::new()
            .menu_with_disabled(
                if showing_terminal {
                    "Show chat"
                } else {
                    "Open terminal"
                },
                false,
                pick("terminal"),
            )
            .separator()
            .menu_with_disabled("Save as snapshot", !reachable, pick("saveSnapshot"))
            .menu_with_disabled(update_label, !reachable, pick("updateSnapshot"))
            .menu_with_disabled("Pull branch", !has_branch, pick("pull"))
            .menu_with_disabled("Open PR", !has_branch, pick("openPr"))
            .separator()
            .menu_with_disabled("Stop", !reachable, pick("stop"))
            .menu_with_disabled(
                if unpushed {
                    "Delete anyway (unpushed work)…"
                } else {
                    "Delete…"
                },
                false,
                pick(if unpushed { "deleteForce" } else { "delete" }),
            );
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                menu.toggle_below_for_app(app, trigger, window, cx);
            });
        });
    }

    /// Runs a row of the box menu.
    pub(crate) fn run_cloud_box_menu_pick(
        &mut self,
        picked: &CloudBoxMenuPicked,
        cx: &mut gpui::Context<Self>,
    ) {
        let box_id = picked.box_id.clone();
        let row = cached_row(&box_id).map(|(row, _)| row);
        let name = row
            .as_ref()
            .map(|row| row.name.clone())
            .unwrap_or_else(|| "the box".to_string());
        let (params, done) = match picked.pick {
            "terminal" => {
                self.handoff_agents_session_chat_mode(
                    TerminalSessionId(picked.shell_session_id),
                    cx,
                );
                return;
            }
            "openPr" => {
                let Some(row) = row else {
                    return;
                };
                let Some(branch) = row
                    .sessions
                    .iter()
                    .find_map(|session| session.branch.clone())
                else {
                    return;
                };
                // The project key of a GitHub repository is `github.com/<owner>/<repo>`.
                if row.project_key.starts_with("github.com/") {
                    gpui_open_external_url(
                        &format!("https://{}/compare/{branch}?expand=1", row.project_key),
                        cx,
                    );
                } else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Open PR needs a GitHub repository",
                        &format!("Push {branch} and open the pull request on your Git host."),
                        cx,
                    );
                }
                return;
            }
            "pull" => {
                self.pull_cloud_box_branch(box_id, name, cx);
                return;
            }
            "saveSnapshot" => (
                json!({ "action": "saveSnapshot", "boxId": box_id, "makeDefault": true }),
                format!("Saving a snapshot of {name}"),
            ),
            "updateSnapshot" => (
                json!({ "action": "saveSnapshot", "boxId": box_id, "replaceDefault": true }),
                format!("Updating the project's snapshot from {name}"),
            ),
            "stop" => (
                json!({ "action": "stop", "boxId": box_id }),
                format!("Stopping {name}"),
            ),
            "delete" => (
                json!({ "action": "delete", "boxId": box_id }),
                format!("Deleting {name}"),
            ),
            "deleteForce" => (
                json!({ "action": "delete", "boxId": box_id, "force": true }),
                format!("Deleting {name}"),
            ),
            _ => return,
        };
        self.dispatch_gpui_workspace_action_toast("info", &done, "", cx);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result_with_code(
                        "/api/cloudBoxes",
                        &params,
                        Duration::from_secs(120),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                ROWS.with(|rows| rows.borrow_mut().remove(&box_id));
                if let Err(error) = result {
                    let title = if error.code.as_deref() == Some("cloudBoxHasUnpushedWork") {
                        "Has unpushed work: push it, or choose Delete anyway"
                    } else {
                        "The box didn't do that"
                    };
                    this.dispatch_gpui_workspace_action_toast("warning", title, &error.message, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Pull branch: the box session's pushed branch into a worktree of its project here
    /// (`pullBranch`, server/src/server/worktree_ops/pull_box_branch.rs).
    fn pull_cloud_box_branch(
        &mut self,
        box_id: String,
        name: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            &format!("Pulling the branch from {name}"),
            "",
            cx,
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let params = json!({ "action": "pullBranch", "boxId": box_id });
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result("/api/cloudBoxes", &params, Duration::from_secs(300))
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(value) => {
                    let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
                    let title = if value["created"] == true {
                        format!("Pulled {} into a new worktree", text("branch"))
                    } else {
                        format!("Updated {} from {name}", text("branch"))
                    };
                    let detail = match value["note"].as_str() {
                        Some(note) => format!("{} · {note}", text("path")),
                        None => text("path"),
                    };
                    this.dispatch_gpui_workspace_action_toast("success", &title, &detail, cx);
                }
                Err(error) => {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Couldn't pull the branch",
                        &error,
                        cx,
                    );
                }
            });
        })
        .detach();
    }
}
