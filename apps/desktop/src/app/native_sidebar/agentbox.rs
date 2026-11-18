//! A box session in the sidebar: the provider badge on its row and the four box actions of its
//! context menu (Open Box Web App, Open Box Screen, Stop Box, Destroy Box…). The GPUI web build
//! compiles this file too.
//!
//! CDXC:AgentBox 2026-10-01 WHY:
//! The row says where the agent runs (the provider's label beside a box, cloud or server glyph)
//! because a box session looks like any other session of its agent otherwise. The menu actions
//! are calls to gxserver's `/api/agentbox`, which owns the `agentbox` CLI; a box's web app and
//! screen open in Ghostex's own Browser (a new tab of the page in the web build).
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/session.rs (the menu rows),
//! apps/desktop/src/app/gx_store/sidebar_snapshot.rs (`agentbox` in a row's details).

use std::time::Duration;

use ghostex_gx_core::SessionKey;
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::{Value, json};

use super::tooltips::SidebarTooltipSpan;
use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc_with_timeout;
use crate::app::helpers::*;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};

/// `agentbox url` can take a while for a cloud box (it opens an SSH forward).
const OPEN_TARGET_TIMEOUT: Duration = Duration::from_secs(75);
/// `agentbox stop` and `destroy`, which gxserver bounds at two minutes.
const BOX_CALL_TIMEOUT: Duration = Duration::from_secs(150);

fn badge_icon(provider: &str) -> &'static str {
    if provider == "docker" {
        "titlebar/box.svg"
    } else if provider.starts_with("docker:") {
        "titlebar/server.svg"
    } else {
        "titlebar/cloud.svg"
    }
}

/// The badge after a box session's title: the provider glyph and label, with the box's name in
/// its tooltip.
pub(super) fn agentbox_badge(
    session: &NativeSidebarSession,
    appearance: &SidebarAppearance,
    tooltip_span: SidebarTooltipSpan,
    tooltips: bool,
) -> Option<AnyElement> {
    let agentbox = session.details.get("agentbox")?;
    let provider = agentbox["provider"].as_str().unwrap_or_default().to_owned();
    let label = agentbox["providerLabel"]
        .as_str()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(&provider)
        .to_owned();
    let box_name = agentbox["boxName"].as_str().unwrap_or_default();
    let tooltip = format!("Runs in agentbox {box_name} ({label})");
    let scale = appearance.scale;
    Some(
        div()
            .id(format!("native-session-agentbox-{}", session.session_id))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(3.0 * scale))
            .max_w(px(96.0 * scale))
            .child(titlebar_svg_icon(
                badge_icon(&provider),
                12.0 * scale,
                appearance.muted,
            ))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.5 * scale))
                    .text_color(appearance.muted)
                    .child(label),
            )
            .when(tooltips, |badge| {
                badge.managed_discrete_tooltip_with_placement(
                    tooltip_span.placement(),
                    appearance.tooltip_delay,
                    move |window, cx| {
                        super::tooltips::sidebar_tooltip(
                            tooltip.clone(),
                            tooltip_span,
                            scale,
                            window,
                            cx,
                        )
                    },
                )
            })
            .into_any_element(),
    )
}

impl GhostexGpuiApp {
    /// The drawn row's `agentbox` details, for the box name a toast or the Destroy prompt names.
    fn native_sidebar_agentbox_name(&self, row_id: &str) -> Option<String> {
        self.native_sidebar
            .snapshot
            .as_ref()?
            .groups
            .iter()
            .flat_map(|group| group.sessions.iter())
            .find(|session| session.session_id == row_id)?
            .details
            .get("agentbox")?
            .get("boxName")?
            .as_str()
            .map(str::to_owned)
    }

    /// `{ type: 'agentboxSessionAction', sessionId, action }` from a box session's menu.
    pub(crate) fn run_native_sidebar_agentbox_action(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(row_id) = command["sessionId"].as_str() else {
            return;
        };
        let Some(session) =
            SessionKey::parse_sidebar_session_id(row_id).filter(|key| key.machine.is_local())
        else {
            return;
        };
        let box_name = self
            .native_sidebar_agentbox_name(row_id)
            .unwrap_or_else(|| "this box".to_owned());
        match command["action"].as_str() {
            Some("openWeb") => self.open_agentbox_target(session, "web", cx),
            Some("openScreen") => self.open_agentbox_target(session, "screen", cx),
            Some("stop") => self.run_agentbox_box_call(session, "stop", box_name, cx),
            Some("destroy") => {
                let detail = format!(
                    "This deletes {box_name} and everything in it that was not pushed or copied out. The session stays in the sidebar, but its agent cannot be resumed."
                );
                self.gx_store_confirm_in_app(
                    ghostex_gx_core::CloseConfirmation {
                        title: "Destroy box?".to_owned(),
                        message: detail,
                        confirm_label: "Destroy Box".to_owned(),
                    },
                    Box::new(move |app, cx| {
                        app.run_agentbox_box_call(session, "destroy", box_name, cx);
                    }),
                    cx,
                );
            }
            _ => {}
        }
    }

    /// Open Box Web App (`web`) and Open Box Screen (`screen`): gxserver names the URL, the Browser
    /// opens it in the session's project.
    fn open_agentbox_target(
        &mut self,
        session: SessionKey,
        target: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = json!({
            "action": "openTarget",
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "target": target,
        });
        let project_id = session.project_id.clone();
        cx.spawn(async move |this, cx| {
            let answer =
                gx_rpc_with_timeout(None, "/api/agentbox", params, OPEN_TARGET_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                let url = match &answer {
                    Ok(value) => value["url"]
                        .as_str()
                        .map(str::trim)
                        .filter(|url| !url.is_empty())
                        .map(str::to_owned),
                    Err(_) => None,
                };
                let Some(url) = url else {
                    let title = if target == "web" {
                        "Could not open the box's web app"
                    } else {
                        "Could not open the box's screen"
                    };
                    let description = match &answer {
                        Err(error) => error.message.clone(),
                        Ok(_) if target == "web" => "This box has no web app to open. Add an agentbox.yaml with services.web.expose.port to start one.".to_owned(),
                        Ok(_) => "This box has no screen to open.".to_owned(),
                    };
                    this.dispatch_gpui_workspace_action_toast("warning", title, &description, cx);
                    return;
                };
                this.defer_in_main_window(cx, move |this, window, cx| {
                    this.open_browser_url_from_renderer_command(
                        GpuiSidebarOpenBrowserUrlMessage {
                            url,
                            reuse: GpuiBrowserRendererOpenReuse::Exact,
                            from_quick_header: false,
                            project_id: Some(project_id),
                        },
                        window,
                        cx,
                    );
                });
            });
        })
        .detach();
    }

    /// Stop Box and Destroy Box: `agentbox stop` / `agentbox destroy -y` through gxserver.
    fn run_agentbox_box_call(
        &mut self,
        session: SessionKey,
        action: &'static str,
        box_name: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let (busy, done, failed) = match action {
            "destroy" => ("Destroying", "Box destroyed", "Could not destroy the box"),
            _ => ("Stopping", "Box stopped", "Could not stop the box"),
        };
        self.dispatch_gpui_workspace_action_toast("info", &format!("{busy} {box_name}…"), "", cx);
        let params = json!({
            "action": action,
            "projectId": session.project_id,
            "sessionId": session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let answer = gx_rpc_with_timeout(None, "/api/agentbox", params, BOX_CALL_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| match answer {
                Ok(value) if value["ok"] != false => {
                    this.dispatch_gpui_workspace_action_toast("success", done, &box_name, cx);
                }
                Ok(value) => {
                    let output = value["output"]
                        .as_str()
                        .unwrap_or_default()
                        .trim()
                        .to_owned();
                    this.dispatch_gpui_workspace_action_toast("warning", failed, &output, cx);
                }
                Err(error) => {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        failed,
                        &error.message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }
}
