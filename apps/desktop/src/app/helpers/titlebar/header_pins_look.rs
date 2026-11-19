//! The header's pinnable controls and how they look: the 16px icon rule, the icon-only button and
//! the ⋯ menu's row with its pin. Its own file so the native-modal-demo can draw the app's real
//! rows and buttons; what pins them and what they do is render/workarea_header/header_pins.rs.
use gpui::{
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _, px,
};
use gpui_component::h_flex;

use super::*;
use crate::*;

/// The logos the header's bot buttons draw (claude_web/place.rs `WebPlace::icon`).
pub(crate) const HEADER_CLAUDE_ICON: &str = "agent-icons/claude.svg";
pub(crate) const HEADER_GROK_ICON: &str = "agent-icons/grok-build.svg";

/// One control of the header's trailing half that can be pinned or kept in the ⋯ menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderSlot {
    Work,
    SessionDetails,
    Share,
    Popout,
    Start,
    Open,
    Commit,
    Claude,
    GrokBot,
    Voice,
    BottomPanel,
    SidePanel,
}

/// The ⋯ menu's groups, in order, with the rows each holds in order.
pub(crate) const HEADER_MORE_SECTIONS: [(&str, &[HeaderSlot]); 4] = [
    (
        "This chat",
        &[
            HeaderSlot::Work,
            HeaderSlot::SessionDetails,
            HeaderSlot::Share,
            HeaderSlot::Popout,
        ],
    ),
    (
        "Project",
        &[HeaderSlot::Start, HeaderSlot::Open, HeaderSlot::Commit],
    ),
    (
        "Agents",
        &[HeaderSlot::Claude, HeaderSlot::GrokBot, HeaderSlot::Voice],
    ),
    ("Panels", &[HeaderSlot::BottomPanel, HeaderSlot::SidePanel]),
];

impl HeaderSlot {
    pub(crate) const ALL: [Self; 12] = [
        Self::Work,
        Self::SessionDetails,
        Self::Share,
        Self::Popout,
        Self::Start,
        Self::Open,
        Self::Commit,
        Self::Claude,
        Self::GrokBot,
        Self::Voice,
        Self::BottomPanel,
        Self::SidePanel,
    ];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::SessionDetails => "sessionDetails",
            Self::Share => "share",
            Self::Popout => "popout",
            Self::Start => "start",
            Self::Open => "open",
            Self::Commit => "commit",
            Self::Claude => "claude",
            Self::GrokBot => "grokBot",
            Self::Voice => "voice",
            Self::BottomPanel => "bottomPanel",
            Self::SidePanel => "sidePanel",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.key() == key)
    }

    /// The chat's and project's own controls start in the header; the rest start in ⋯.
    pub(crate) fn pinned_by_default(self) -> bool {
        !matches!(
            self,
            Self::Share | Self::Popout | Self::Claude | Self::GrokBot | Self::Voice
        )
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Work => "Work",
            Self::SessionDetails => "Session details",
            Self::Share => "Share",
            Self::Popout => "Open in a popup",
            Self::Start => "Start",
            Self::Open => "Open",
            Self::Commit => "Commit",
            Self::Claude => "Claude",
            Self::GrokBot => "Grok Bot",
            Self::Voice => "Voice call",
            Self::BottomPanel => "Bottom panel",
            Self::SidePanel => "Side panel",
        }
    }

    /// The row's icon and its size inside the 16px box (`header_icon_size`).
    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Work => "titlebar/layout-board-split.svg",
            Self::SessionDetails => "titlebar/circle-half.svg",
            Self::Share => "titlebar/users.svg",
            Self::Popout => "titlebar/external-link.svg",
            Self::Start => TITLEBAR_ICON_PLAYER_PLAY,
            Self::Open => TITLEBAR_ICON_FOLDER_OPEN,
            Self::Commit => TITLEBAR_ICON_GIT_COMMIT,
            Self::Claude => HEADER_CLAUDE_ICON,
            Self::GrokBot => HEADER_GROK_ICON,
            Self::Voice => "titlebar/headset.svg",
            Self::BottomPanel => TITLEBAR_ICON_PANEL_BOTTOM,
            Self::SidePanel => TITLEBAR_ICON_PANEL_RIGHT,
        }
    }
}

/// CDXC:Titlebar 2026-11-19 DECISION:
/// User: "some of the icons visually look bigger than others (especially the claude one looks bigger than all the others)". Every header icon sits in one 16px box. Outline icons (Tabler, 2-unit stroke) are drawn at 16px, which leaves about 12px of shape; Claude's logo fills its square edge to edge and Grok's nearly so, and solid shapes look heavier, so the solid ones are drawn to about 11px of shape: Claude at 11px, Grok at 12px, Work's board (which leaves a sixth empty each side) at 15px.
pub(crate) fn header_icon_size(path: &str) -> f32 {
    if path == HEADER_CLAUDE_ICON {
        11.0
    } else if path == HEADER_GROK_ICON {
        12.0
    } else if path == "titlebar/layout-board-split.svg" {
        15.0
    } else {
        HEADER_ICON_BOX
    }
}

/// The square every header icon is centred in.
pub(crate) const HEADER_ICON_BOX: f32 = 16.0;

/// `path` drawn by the sizing rule, centred in the 16px box.
pub(crate) fn header_icon(path: &'static str, color: gpui::Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_shrink_0()
        .size(px(HEADER_ICON_BOX))
        .items_center()
        .justify_center()
        .child(titlebar_svg_icon(path, header_icon_size(path), color))
}

/// The shared header look for an icon-only control: 27px tall, at least as wide, one hover fill
/// and one fill while on or open.
pub(crate) fn header_icon_button(
    id: impl Into<gpui::ElementId>,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(TITLEBAR_CONTROL_HEIGHT))
        .min_w(px(TITLEBAR_CONTROL_HEIGHT))
        .px(px(5.5))
        .rounded(px(TITLEBAR_BUTTON_RADIUS))
        .cursor_default()
        .when(cfg!(target_os = "windows"), |this| this.occlude())
        .when(on, |this| this.bg(titlebar_active_segment_color()))
        .hover(move |this| {
            if on {
                this.bg(titlebar_active_segment_color())
            } else {
                this.bg(titlebar_button_hover_color())
            }
        })
}

/// Pins (`pinned: true`) or unpins a header control. From a ⋯ row's pin and the Unpin row of a
/// control's right-click menu.
#[derive(Clone, Debug, Eq, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct PinWorkareaHeaderSlot {
    pub(crate) slot: String,
    pub(crate) pinned: bool,
}

/// A ⋯ row: does what the control's button does.
#[derive(Clone, Debug, Eq, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct RunWorkareaHeaderSlot {
    pub(crate) slot: String,
}


/// One ⋯ row, as the menu and its height read it.
#[derive(Clone, Debug)]
pub(crate) struct HeaderMoreRow {
    pub(crate) slot: HeaderSlot,
    pub(crate) status: Option<String>,
    pub(crate) live: bool,
}


/// One ⋯ row: the control's icon and name, what it is doing now, and the pin, shown while the
/// pointer is on the row.
pub(crate) fn header_more_row(row: HeaderMoreRow) -> impl IntoElement {
    let slot = row.slot;
    let group: gpui::SharedString = format!("header-more-row-{}", slot.key()).into();
    let foreground = titlebar_popup_menu_foreground();
    let muted = titlebar_inactive_text_color();
    let pin = PinWorkareaHeaderSlot {
        slot: slot.key().to_string(),
        pinned: true,
    };
    h_flex()
        .group(group.clone())
        .min_w_0()
        .max_w_full()
        .flex_1()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .gap(px(8.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .text_color(foreground)
        .child(header_icon(slot.icon(), foreground))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(slot.label()),
        )
        .child(div().min_w_0().flex_1())
        .when_some(row.status, |this, status| {
            this.child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(11.5))
                    .text_color(muted)
                    .when(row.live, |this| {
                        this.child(div().size(px(6.0)).rounded_full().bg(gpui::rgb(0x4ade80)))
                    })
                    .child(status),
            )
        })
        .child(
            h_flex()
                .id(gpui::SharedString::from(format!(
                    "ghostex-gpui-header-pin-{}",
                    slot.key()
                )))
                .flex_shrink_0()
                .size(px(24.0))
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .opacity(0.0)
                .group_hover(group, |this| this.opacity(1.0))
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(pin.clone()), cx);
                })
                .child(titlebar_svg_icon("titlebar/pin-outline.svg", 14.0, muted))
                .tooltip(|window, cx| titlebar_tooltip("Pin to header", window, cx)),
        )
}
