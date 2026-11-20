//! The rail's measures, each icon's artwork, what an icon shows, and the window's rail state.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(not(ghostex_web))]
use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Bounds, Hsla, Pixels};

/// The rail's width, unscaled: the user's "40px max".
pub(crate) const RAIL_WIDTH: f32 = 40.0;
/// A rail button, square.
pub(crate) const RAIL_BUTTON: f32 = 32.0;
/// An outline icon's box (the user's "20px icons").
pub(crate) const RAIL_ICON: f32 = 20.0;
/// Meet the rail's window size, for the app modal kinds (model/app_modal_kind.rs). The pop-up is
/// the desktop's (app/window/rail_intro_modal.rs); the web build never opens it.
#[cfg(not(ghostex_web))]
pub(crate) use crate::app::window::{RAIL_INTRO_MODAL_INITIAL_HEIGHT, RAIL_INTRO_MODAL_WIDTH};
#[cfg(ghostex_web)]
pub(crate) const RAIL_INTRO_MODAL_WIDTH: f32 = 0.0;
#[cfg(ghostex_web)]
pub(crate) const RAIL_INTRO_MODAL_INITIAL_HEIGHT: f32 = 0.0;
/// Between two buttons.
pub(crate) const RAIL_GAP: f32 = 2.0;
pub(crate) const RAIL_BUTTON_RADIUS: f32 = 8.0;

/// Whether this computer's settings choose the rail.
pub(crate) fn sidebar_rail_on() -> bool {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    ghostex_gx_core::sidebar_rail::rail_layout_on(settings.object())
}

/// The layout the settings chose when last read: 0 not read yet, 1 rail, 2 classic.
#[cfg(not(ghostex_web))]
static SIDEBAR_LAYOUT_SEEN: AtomicU8 = AtomicU8::new(0);
/// The pop-up's Switch changed the layout to `rail`: counted here, with its source, and noted as
/// seen so the settings read that follows does not count it again as a Settings change. Each
/// Switch counts, however quickly they follow each other.
#[cfg(not(ghostex_web))]
pub(crate) fn note_sidebar_layout_switched_from_intro(rail: bool) {
    SIDEBAR_LAYOUT_SEEN.store(if rail { 1 } else { 2 }, Ordering::Relaxed);
    crate::app::helpers::telemetry::record_gpui_sidebar_layout_changed_telemetry(
        if rail { "rail" } else { "classic" },
        "intro",
    );
}

/// Called whenever the settings are read: a change of layout after the first read is a switch from
/// Settings or `ghostex settings`, counted once.
#[cfg(not(ghostex_web))]
pub(crate) fn note_sidebar_layout(settings: &serde_json::Map<String, serde_json::Value>) {
    let rail = ghostex_gx_core::sidebar_rail::rail_layout_on(settings);
    let now = if rail { 1 } else { 2 };
    let before = SIDEBAR_LAYOUT_SEEN.swap(now, Ordering::Relaxed);
    if before == 0 || before == now {
        return;
    }
    // Switches from the Meet the rail pop-up are counted by the pop-up itself.
    let source = "settings";
    crate::app::helpers::telemetry::record_gpui_sidebar_layout_changed_telemetry(
        if rail { "rail" } else { "classic" },
        source,
    );
}

/// The pinned items' keys, top to bottom.
pub(crate) fn sidebar_rail_pins() -> Vec<&'static str> {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    ghostex_gx_core::sidebar_rail::rail_pins(settings.object())
}

/// The artwork of the item `key`.
pub(crate) fn rail_item_icon(key: &str) -> &'static str {
    match key {
        "search" => "titlebar/search.svg",
        "notifications" => "titlebar/bell.svg",
        "mainHub" => "titlebar/ghostex-flower.svg",
        "agentsHub" => "titlebar/users-group.svg",
        "allAutomations" => "titlebar/clock.svg",
        "commands" => "titlebar/bolt.svg",
        "hermes" => "agent-icons/hermes-agent.svg",
        "claude" => "agent-icons/claude.svg",
        "grok" => "agent-icons/grok-build.svg",
        "voiceCall" => "titlebar/headset.svg",
        "files" => "titlebar/folder.svg",
        "changes" => "titlebar/file-diff.svg",
        "browser" => "titlebar/browser.svg",
        "projectBoard" => "titlebar/layout-board-split.svg",
        "devServers" => "titlebar/world.svg",
        "resources" => "titlebar/device-desktop.svg",
        "extensions" => "titlebar/puzzle.svg",
        "connectPhone" => "titlebar/devices.svg",
        "settings" => "titlebar/settings.svg",
        _ => "titlebar/dots.svg",
    }
}

/// The icon's size in its 20pt box. Outline icons fill the box; the solid logos fill their square
/// edge to edge and look heavier, so they are drawn smaller (the header's rule, header_pins_look.rs).
pub(crate) fn rail_item_icon_size(key: &str) -> f32 {
    match key {
        "claude" => 15.0,
        "grok" => 16.0,
        "hermes" => 18.0,
        _ => RAIL_ICON,
    }
}

/// A count or a dot on an icon. The web build's items carry none.
#[cfg_attr(ghostex_web, allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RailBadge {
    Count(usize, Hsla),
    Dot(Hsla),
}

/// What one item shows now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RailItemState {
    /// The item applies here (its extension is on, its view can open in this window).
    pub(crate) available: bool,
    /// Its page, view or popup is the one on screen: the filled tile, and an unpinned item shows
    /// on the rail while this holds.
    pub(crate) showing: bool,
    pub(crate) badge: Option<RailBadge>,
}

/// A pinned icon being dragged along the rail.
#[derive(Clone, Debug)]
pub(crate) struct DraggedRailItem {
    pub(crate) key: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) icon_size: f32,
}

/// The icon that follows the pointer while one is dragged.
pub(crate) struct RailDragPreview {
    pub(crate) icon: &'static str,
    pub(crate) icon_size: f32,
    pub(crate) fill: Hsla,
    pub(crate) border: Hsla,
    pub(crate) color: Hsla,
}

/// Where a rail button was last painted.
pub(crate) type RailBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

/// The window's rail: where its buttons were painted, for the menus and popups they open.
#[derive(Default)]
pub(crate) struct SidebarRailState {
    /// The ⋯ button.
    pub(crate) more_bounds: RailBounds,
    items: RefCell<HashMap<&'static str, RailBounds>>,
}

impl SidebarRailState {
    /// Where item `key`'s button was last painted.
    pub(crate) fn item_bounds(&self, key: &'static str) -> RailBounds {
        self.items.borrow_mut().entry(key).or_default().clone()
    }
}
