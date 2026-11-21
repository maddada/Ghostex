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

use gpui::{AnyElement, Hsla, IntoElement, ParentElement, Styled, div, px};
use serde_json::Value;

use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The dot's diameter, its gap under the icon, and how much of the muted text colour it takes,
/// before the zoom.
const DOT_SIZE: f32 = 3.0;
pub(crate) const DOT_GAP: f32 = 2.0;
const DOT_OPACITY: f32 = 0.55;

/// A computer's icon (`size` px, zoomed), with a small faint dot centred `gap` (unzoomed) under it
/// when `here`: the computer you're on, while there is another computer to tell it from. `muted`
/// is the row's muted text colour, which the dot takes faintly.
///
/// CDXC:RemoteMachines 2026-11-20 DECISION:
/// User, of the line under this computer's icon (Q4248, D): "make it a faint dot instead, like the
/// dock on macOS has". A 3px round dot centred 2px under the icon of the computer you're on, in the
/// row's muted text colour at 55%, wherever a computer's icon shows on the desktop and in the web
/// build: the sidebar's computer tiles and the folded computers button, session rows and Unified
/// cards, a project header's computer icons, the chat's where pill and its card, Session details
/// and Settings > Remote's row for this computer. With one computer nothing is marked, and the
/// phone marks nothing (it is not one of the computers, so none is the one you're on). A tile's
/// status dot sits in its top-right corner (status.rs `tile_status_dot`), clear of this mark, so
/// the icon no longer lifts over it (until 2026-11-21 it did). This supersedes the corner pip (Q4241)
/// and the line (Q4248) of the same day.
///
/// CDXC:RemoteMachines 2026-11-20 DECISION:
/// User, two follow-ups on the dot: a setting to hide it, on by default (Mark the computer you're
/// on, `markComputerYouAreOn`, on Sidebar & Sessions > Sidebar beside the other computer rows), and
/// the dot only "when 2+ computers are actually CONNECTED": a saved computer that is offline or
/// can't be reached no longer counts, where before every computer tab in the sidebar did. The
/// desktop works it out where it rebuilds the machine tabs (gx_store/remote_clients.rs), into
/// `session_directory::mark_this_computer`, which every place that draws the dot asks.
pub(crate) fn with_here_dot_at(
    icon: impl IntoElement,
    size: f32,
    scale: f32,
    here: bool,
    muted: Hsla,
    gap: f32,
) -> AnyElement {
    if !here {
        return icon.into_any_element();
    }
    let dot = DOT_SIZE * scale;
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .child(icon)
        .child(
            div()
                .absolute()
                .left(px((size - dot) / 2.0))
                .top(px(size + gap * scale))
                .size(px(dot))
                .rounded_full()
                .bg(muted.opacity(DOT_OPACITY)),
        )
        .into_any_element()
}

/// [`with_here_dot_at`] at its usual gap.
pub(crate) fn with_here_dot(
    icon: impl IntoElement,
    size: f32,
    scale: f32,
    here: bool,
    muted: Hsla,
) -> AnyElement {
    with_here_dot_at(icon, size, scale, here, muted, DOT_GAP)
}

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
        let (asset, marked) = match self.sidebar_computer_mark(&session.session_id) {
            Some((asset, _)) => (asset, true),
            None => {
                other_computer_name(session)?;
                ("titlebar/device-desktop.svg", false)
            }
        };
        // The computer you're on has its dot (`with_here_dot`) while another one is connected.
        let here = marked
            && crate::app::gx_chat::session_directory::mark_this_computer()
            && ghostex_gx_core::SessionKey::parse_sidebar_session_id(&session.session_id)
                .is_some_and(|key| key.machine.is_local());
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
                .child(with_here_dot(
                    titlebar_svg_icon(asset, 12.0 * scale, color),
                    12.0 * scale,
                    scale,
                    here,
                    appearance.muted,
                ))
                .into_any_element(),
        )
    }
}
