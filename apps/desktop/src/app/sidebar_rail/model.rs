//! The rail's measures, each icon's artwork, what an icon shows, and the window's rail state.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(not(ghostex_web))]
use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Bounds, Hsla, Pixels};

/// CDXC:Sidebar 2026-11-21 DECISION:
/// User: "can we please make our sidebar more similar in terms of gaps between buttons and size of
/// buttons and font size to this?", next to another app's sidebar (only its spacing and sizes, not
/// its look). Measured from its @2x screenshot: a 52pt column, 36pt rounded-square buttons 8pt in
/// from each side and 44pt apart, 16pt glyphs. The rail takes those measures; this supersedes the
/// 2026-11-20 "40px max" column with 32pt buttons 2pt apart. The icons keep their 20pt box (their
/// glyphs were already the reference's size).
///
/// The rail's width, unscaled.
pub(crate) const RAIL_WIDTH: f32 = 52.0;
/// A rail button, square.
pub(crate) const RAIL_BUTTON: f32 = 36.0;
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
/// Between two buttons: a 44pt pitch.
pub(crate) const RAIL_GAP: f32 = 8.0;
pub(crate) const RAIL_BUTTON_RADIUS: f32 = 9.0;

/// How much of what the sidebar's tint lets through the rail's tint covers as well, under glass.
const RAIL_GLASS_EXTRA_COVER: f32 = 0.5;

/// The rail's tint under glass: the sidebar's colour (`sidebar_glass_tint`, which already follows
/// Sidebar tint, the art themes and Live) covering half of what the sidebar lets through, so 88%
/// becomes 94%, 93% becomes 96.5% and 70% becomes 85%.
pub(crate) fn rail_glass_tint(sidebar: Hsla) -> Hsla {
    Hsla {
        a: sidebar.a + (1.0 - sidebar.a) * RAIL_GLASS_EXTRA_COVER,
        ..sidebar
    }
}

/// What to lay over the sidebar's tint for the rail's: its colour at the extra cover. Over a tint of
/// alpha `a` it makes `a + (1 - a) * cover`, which is `rail_glass_tint`.
pub(crate) fn rail_glass_gutter_tint(sidebar: Hsla) -> Hsla {
    Hsla {
        a: RAIL_GLASS_EXTRA_COVER,
        ..sidebar
    }
}

/// The rail's fill: a little more solid than the sidebar's under glass, the sidebar's own with it off.
///
/// CDXC:Sidebar 2026-11-21 DECISION:
/// User: the rail "is not taking the same bg config as the sidebar itself". It was the same tint; what showed through at the screen's edge (the Dock's or Stage Manager's icons) made it read differently. The user chose to make the rail slightly more solid than the sidebar so that is muted: under glass, art themes and Live, the rail's tint covers half of what the sidebar's leaves showing, never past opaque, in the sidebar's colour so the two read as one surface. A flat +12 points was weighed and left out: at the default 88% it would make the rail fully opaque. Themes without glass keep the sidebar's own gradient.
pub(crate) fn sidebar_rail_fill(glass: bool) -> gpui::Background {
    if glass {
        rail_glass_tint(crate::app::helpers::sidebar_glass_tint()).into()
    } else {
        crate::app::helpers::sidebar_chrome_fill(false, 180.0)
    }
}

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

/// The pinned items' keys, top to bottom: rail items' keys and links' `link:<id>`.
pub(crate) fn sidebar_rail_pins() -> Vec<String> {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    ghostex_gx_core::sidebar_rail::rail_pins(settings.object())
}

/// The artwork of the item `key`.
pub(crate) fn rail_item_icon(key: &str) -> &'static str {
    match key {
        "search" => "titlebar/search.svg",
        "mainHub" => crate::app::native_sidebar::chief_row_view::FLOWER_ICON,
        "agentsHub" => "titlebar/users-group.svg",
        "allAutomations" => "titlebar/clock.svg",
        "commands" => "titlebar/bolt.svg",
        "hermes" => "agent-icons/hermes-agent.svg",
        "claude" => "agent-icons/claude.svg",
        "grok" => "agent-icons/grok-build.svg",
        "voiceCall" => "titlebar/headset.svg",
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

/// A dot on an icon (Main Hub's). The web build's items carry none.
#[cfg_attr(ghostex_web, allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RailBadge {
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

/// A pinned link, as its rail button shows it (packages/gx-core/src/sidebar_rail.rs,
/// `RAIL_LINK_PIN_PREFIX`). The web build has no links' page, so it never pins one.
#[cfg_attr(ghostex_web, allow(dead_code))]
#[derive(Clone)]
pub(crate) struct RailLinkPin {
    pub(crate) id: String,
    /// Its name, or its host and port without one: the tooltip.
    pub(crate) label: String,
    /// The site's icon once its page has been opened; the globe without one.
    pub(crate) image: Option<std::sync::Arc<gpui::Image>>,
    /// Its page is the one on screen.
    pub(crate) showing: bool,
}

/// What a rail button draws: its tooltip, and its artwork (an image, such as a link's site icon,
/// stands in for the outline icon when there is one).
#[derive(Clone)]
pub(crate) struct RailButtonFace {
    pub(crate) label: String,
    pub(crate) icon: &'static str,
    pub(crate) icon_size: f32,
    pub(crate) image: Option<std::sync::Arc<gpui::Image>>,
}

/// A pinned icon being dragged along the rail.
#[derive(Clone)]
pub(crate) struct DraggedRailItem {
    pub(crate) key: String,
    pub(crate) face: RailButtonFace,
}

/// The icon that follows the pointer while one is dragged.
pub(crate) struct RailDragPreview {
    pub(crate) face: RailButtonFace,
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
    items: RefCell<HashMap<String, RailBounds>>,
    /// Which half of the rail has the pointer (`Some(true)`: the top), for the hidden sidebar's
    /// reveal (`sidebar_rail_reveal_hover` in host.rs). None off the rail.
    #[cfg(not(ghostex_web))]
    pub(crate) reveal_half: Option<bool>,
    /// Counts the pointer's moves between halves, so a dwell that ended elsewhere does nothing.
    #[cfg(not(ghostex_web))]
    pub(crate) reveal_generation: u64,
    /// The rail armed the reveal, so leaving it disarms what it armed.
    #[cfg(not(ghostex_web))]
    pub(crate) reveal_armed: bool,
    /// The x the window's traffic lights were last moved by for the rail
    /// (`sync_window_lights_rail_shift` in render.rs); None before the first move.
    #[cfg(all(target_os = "macos", not(ghostex_web)))]
    pub(crate) lights_shift: Option<f32>,
}

impl SidebarRailState {
    /// Where item `key`'s button was last painted.
    pub(crate) fn item_bounds(&self, key: &str) -> RailBounds {
        self.items
            .borrow_mut()
            .entry(key.to_string())
            .or_default()
            .clone()
    }
}
