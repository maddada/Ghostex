//! The computer a session row runs on, as that computer's icon after the row's title.
//!
//! CDXC:RemoteMachines 2026-11-12 DECISION:
//! User: "please show the machine icon on all sessions in the sidebar even local ones when we hav multiple machines connected ... no need to show WN for short machine name i just want different icon". While more than one computer is in the sidebar, every session row, this computer's included, ends its title line with its computer's icon (the one picked for it, gx-core computer_icons.rs) and no name; the row's tooltip says "On <computer>". With only this computer there is none. A thread drawn from a computer the sidebar does not list keeps a plain computer icon. This supersedes the short-name badge of 2026-11-06.
//!
//! CDXC:RemoteMachines 2026-11-04 DECISION:
//! User, of a thread a coordinator here started on their Windows computer: "why dont i see that thread in the workers under you??". The thread sits under its coordinator like a local one, and says which computer runs it (since 2026-11-12 with that computer's icon), so a click that opens it there is no surprise.
//!
//! CDXC:RemoteMachines 2026-11-08 DECISION:
//! User, on the offline-notice page (Q1837): "just show WN with redish color in sidebar to indicate, that's enough". The computer's mark takes the soft red the computer's tile icon turns when it fails (machines.rs) while that computer is offline (gx_chat/computers.rs); nothing else on the row changes. Since 2026-11-12 the mark is the icon, not the short name.

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div};
use serde_json::Value;

use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The session's computer is one this computer knows is offline.
fn computer_offline(session: &NativeSidebarSession) -> bool {
    let Some(key) = ghostex_gx_core::SessionKey::parse_sidebar_session_id(&session.session_id)
    else {
        return false;
    };
    let Some(machine_id) = key.machine.remote_id() else {
        return false;
    };
    let (_, computers) = crate::app::gx_chat::computers::current();
    computers
        .get(machine_id)
        .is_some_and(|computer| computer.offline)
}

/// The name of the computer a session of another computer runs on, as the row's details carry it
/// (a coordinator's thread from there, or a merged project's session).
fn other_computer_name(session: &NativeSidebarSession) -> Option<&str> {
    session
        .details
        .get("computerName")
        .or_else(|| session.details.get("machineName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

impl GhostexGpuiApp {
    /// The name of the computer a row's tooltip says it runs on, when one is drawn.
    pub(crate) fn session_computer_name(&self, session: &NativeSidebarSession) -> Option<String> {
        other_computer_name(session).map(str::to_owned).or_else(|| {
            self.sidebar_computer_mark(&session.session_id)
                .map(|(_, name)| name.to_owned())
        })
    }

    /// The row's computer icon, when it draws one.
    pub(crate) fn session_computer_badge(
        &self,
        session: &NativeSidebarSession,
        appearance: &SidebarAppearance,
    ) -> Option<AnyElement> {
        let asset = match self.sidebar_computer_mark(&session.session_id) {
            Some((asset, _)) => asset,
            None => {
                other_computer_name(session)?;
                "titlebar/device-desktop.svg"
            }
        };
        let color = if computer_offline(session) {
            gpui::rgb(0xff9494).into()
        } else {
            appearance.muted
        };
        let scale = appearance.scale;
        Some(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .child(titlebar_svg_icon(asset, 12.0 * scale, color))
                .into_any_element(),
        )
    }
}
