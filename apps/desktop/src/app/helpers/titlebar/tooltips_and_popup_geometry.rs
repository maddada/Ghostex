use gpui::{Bounds, Hsla, IntoElement, Pixels, Styled as _, Window, point, px, size, svg};
use gpui_component::tooltip::Tooltip;

use crate::*;

pub(crate) fn titlebar_svg_icon(
    path: &'static str,
    icon_size: f32,
    color: Hsla,
) -> impl IntoElement {
    svg().size(px(icon_size)).path(path).text_color(color)
}

/// CDXC:Titlebar 2026-09-19 DECISION:
/// User: a titlebar button tooltip names the button and then the hotkey assigned to it in Settings > Hotkeys, so rebinding the action there changes the tooltip too. Actions left unassigned show the label alone.
pub(crate) fn titlebar_tooltip_label(label: &str, action_id: &str) -> gpui::SharedString {
    match gpui_configured_hotkey_label(action_id) {
        Some(shortcut) if !shortcut.is_empty() => format!("{label} ({shortcut})").into(),
        _ => label.to_string().into(),
    }
}

/// The app's tooltip for a button: the plain mini menu bubble at its natural size, centred beside
/// the trigger with the bubble's own gap, and any "(⌃⌥V)" after the label drawn as key caps
/// (`gpui_component::tooltip::split_trailing_shortcut`).
///
/// CDXC:Tooltips 2026-10-29 DECISION:
/// User, of this tooltip on the header's voice button ("Talk to the voice agent (^⌥V)", a 20px pill with no gap, hugging the button): "this tooltip is very bad man", and of the context ring's "Hide session details" (the stock bubble): "this one is much better ... please apply 2nd one everywhere that makes sense instead of the one we have in Image #1". The fixed 20px height, the zero margins and the 12px text are gone; header, titlebar, tab strip, Files and Kanban buttons all draw the stock bubble. Under window glass it draws in the frosted tooltip host with the frosted menus' fill and the Menu blur value as is (`apply_frosted_tooltip_blur`); without glass it is the solid dark tooltip colour.
/// SEE-ALSO: native_sidebar/tooltips.rs (`sidebar_style_tooltip`), window/settings_modal/fields/row.rs (Settings' tooltips).
pub(crate) fn titlebar_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    Tooltip::new(text).build(window, cx)
}

/// How far below the pointer a list row's tooltip starts: past the pointer's arrow, so the
/// bubble never sits under the pointer's tip.
const LIST_ROW_TOOLTIP_DROP: f32 = 20.0;

/// The tooltip for a row in a list (a file's path, say): the titlebar tooltip's bubble, wrapped to
/// the room beside the pointer instead of cut at the list's edge, and dropped below the pointer.
///
/// CDXC:Tooltips 2026-10-09 DECISION:
/// User, of the files list's path tooltips: "the tooltip here should appear below the button of the current element so I can't hover on it (it blocks clicks)" and "this tooltip is showing cut off when the files list is not floating". A row's tooltip opens below the pointer, clear of its arrow, and wraps to the room it has (the floating list's own window, or the space beside a native pane) instead of running one line past the edge; its window never takes the pointer (CDXC:Tooltips 2026-10-09 in titlebar_popup_chrome.rs).
pub(crate) fn list_row_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    Tooltip::new(text)
        .mt(px(LIST_ROW_TOOLTIP_DROP))
        .mb_0()
        .build(window, cx)
}

pub(crate) fn titlebar_popup_menu_width(kind: GpuiTitlebarPopupKind) -> f32 {
    match kind {
        GpuiTitlebarPopupKind::AccountUsage(_) => 380.0,
        GpuiTitlebarPopupKind::RemoteSites => TITLEBAR_POPUP_RESOURCES_WIDTH,
        GpuiTitlebarPopupKind::SessionLinks => {
            crate::app::native_sidebar::links_list::SESSION_LINKS_LIST_WIDTH
        }
        GpuiTitlebarPopupKind::Actions
        | GpuiTitlebarPopupKind::ContextMenu
        | GpuiTitlebarPopupKind::BrowserActions(_)
        | GpuiTitlebarPopupKind::OpenTargets => TITLEBAR_POPUP_COMPACT_WIDTH,
        GpuiTitlebarPopupKind::BrowserDeviceView(_) => TITLEBAR_POPUP_BROWSER_DEVICE_VIEW_WIDTH,
        GpuiTitlebarPopupKind::Extensions => TITLEBAR_POPUP_EXTENSIONS_WIDTH,
        GpuiTitlebarPopupKind::Git => TITLEBAR_POPUP_GIT_WIDTH,
        GpuiTitlebarPopupKind::Help => TITLEBAR_POPUP_HELP_WIDTH,
        GpuiTitlebarPopupKind::More => TITLEBAR_POPUP_COMPACT_WIDTH,
        GpuiTitlebarPopupKind::Notifications => TITLEBAR_POPUP_NOTIFICATIONS_WIDTH,
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
    width: f32,
    window: &Window,
) -> Bounds<Pixels> {
    let main_window_bounds = window.bounds();
    /*
    CDXC:AgentProviders 2026-09-20 WHY:
    Account usage used to be pinned to the top of the window under the titlebar
    because its trigger was a titlebar button. Its meter now lives at the bottom
    of the sidebar, so it takes the ordinary trigger-relative path: it grows to
    the right of the meter and flips above it when there is no room below.
    */
    /*
    CDXC:ContextMenus 2026-09-26 DECISION:
    User: a context menu with many options is not scrollable. It grows to its full height and only
    the window itself limits it, so the dropdown cap below applies to header dropdowns alone.
    */
    let max_height = match kind {
        GpuiTitlebarPopupKind::ContextMenu | GpuiTitlebarPopupKind::AccountUsage(_) => {
            f32::INFINITY
        }
        GpuiTitlebarPopupKind::SessionLinks => {
            crate::app::native_sidebar::links_list::session_links_max_height(
                main_window_bounds.size.height.as_f32(),
            )
        }
        GpuiTitlebarPopupKind::Notifications => TITLEBAR_POPUP_NOTIFICATIONS_MAX_HEIGHT,
        GpuiTitlebarPopupKind::Resources
        | GpuiTitlebarPopupKind::Tips
        | GpuiTitlebarPopupKind::RemoteSites => TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT,
        _ => TITLEBAR_POPUP_MENU_MAX_HEIGHT,
    };
    if let GpuiTitlebarPopupKind::AccountUsage(_) = kind
        && let Some(frame) =
            account_usage_popup_frame(main_window_bounds, trigger_bounds, content_height, width)
    {
        return frame;
    }
    let available_height = (main_window_bounds.size.height.as_f32() - 28.0).max(180.0);
    let height = content_height.min(max_height).min(available_height);
    let horizontal_margin = 8.0;
    let min_left = main_window_bounds.origin.x.as_f32() + horizontal_margin;
    let max_left = main_window_bounds.origin.x.as_f32() + main_window_bounds.size.width.as_f32()
        - width
        - horizontal_margin;
    // The Notifications bell and the account usage meters sit at the left edge of
    // the window, in the sidebar, so their dropdowns grow to the right from the
    // trigger like a context menu instead of hanging off the trigger's right edge
    // like the titlebar's right-region buttons.
    let desired_left = main_window_bounds.origin.x.as_f32()
        + if matches!(
            kind,
            GpuiTitlebarPopupKind::AccountUsage(_)
                | GpuiTitlebarPopupKind::ContextMenu
                | GpuiTitlebarPopupKind::Notifications
                | GpuiTitlebarPopupKind::SessionLinks
        ) {
            trigger_bounds.left().as_f32()
        } else {
            trigger_bounds.top_right().x.as_f32() - width
        };
    let left = desired_left.clamp(min_left, max_left.max(min_left));
    // A menu dropping from a button keeps the header gap; a context menu anchored to the pointer
    // (a zero-height trigger) opens right at it.
    let trigger_gap = if trigger_bounds.size.height > px(1.0) {
        HEADER_MENU_TRIGGER_GAP
    } else {
        0.0
    };
    let below_top =
        main_window_bounds.origin.y.as_f32() + trigger_bounds.bottom().as_f32() + trigger_gap;
    let above_top =
        main_window_bounds.origin.y.as_f32() + trigger_bounds.top().as_f32() - trigger_gap - height;
    let bottom_limit = main_window_bounds.origin.y.as_f32()
        + main_window_bounds.size.height.as_f32()
        - horizontal_margin;
    let top =
        if below_top + height <= bottom_limit || above_top < main_window_bounds.origin.y.as_f32() {
            below_top
        } else {
            above_top
        };

    // A trigger near the bottom of the window (the sidebar usage strip) leaves no
    // room either below or fully above it, so the panel is held inside the window
    // the same way a context menu is.
    let top = if matches!(
        kind,
        GpuiTitlebarPopupKind::AccountUsage(_) | GpuiTitlebarPopupKind::ContextMenu
    ) {
        let min_top = main_window_bounds.origin.y.as_f32() + horizontal_margin;
        top.clamp(min_top, (bottom_limit - height).max(min_top))
    } else {
        top
    };
    // The chat's Links pill (native_chat/links_pill.rs) opens its list above the chat box, never
    // over it: the list shortens (and scrolls) to the room above its trigger.
    let (top, height) = if kind == GpuiTitlebarPopupKind::SessionLinks {
        let room = (trigger_bounds.top().as_f32() - trigger_gap - horizontal_margin).max(120.0);
        let height = height.min(room);
        (
            main_window_bounds.origin.y.as_f32() + trigger_bounds.top().as_f32()
                - trigger_gap
                - height,
            height,
        )
    } else {
        (top, height)
    };

    Bounds {
        origin: point(px(left), px(top)),
        size: size(px(width), px(height)),
    }
}

/// Between the usage panel and the account tile it opens from, and the window edges it keeps clear of.
const ACCOUNT_USAGE_TRIGGER_GAP: f32 = 8.0;
const ACCOUNT_USAGE_WINDOW_MARGIN: f32 = 8.0;
/// Below this much room above its tile the panel takes the ordinary dropdown placement.
const ACCOUNT_USAGE_MIN_ROOM: f32 = 180.0;

/// CDXC:AgentProviders 2026-10-30 DECISION:
/// User: the usage panel "floats a small gap above the account tiles" and its height fits its
/// content up to what fits on screen, then scrolls. It opens from the tile's left edge, above it,
/// as tall as its content or the room above the tile, whichever is less, so it never covers the
/// tiles. `window_bounds` and the result are in screen space, `trigger_bounds` in the window's.
/// None when the tile has too little room above it (it is not at the bottom of the window).
pub(crate) fn account_usage_popup_frame(
    window_bounds: Bounds<Pixels>,
    trigger_bounds: Bounds<Pixels>,
    content_height: f32,
    width: f32,
) -> Option<Bounds<Pixels>> {
    let room =
        trigger_bounds.top().as_f32() - ACCOUNT_USAGE_TRIGGER_GAP - ACCOUNT_USAGE_WINDOW_MARGIN;
    if room < ACCOUNT_USAGE_MIN_ROOM {
        return None;
    }
    let height = content_height.min(room);
    let min_left = ACCOUNT_USAGE_WINDOW_MARGIN;
    let max_left =
        (window_bounds.size.width.as_f32() - width - ACCOUNT_USAGE_WINDOW_MARGIN).max(min_left);
    let left = trigger_bounds.left().as_f32().clamp(min_left, max_left);
    let top = trigger_bounds.top().as_f32() - ACCOUNT_USAGE_TRIGGER_GAP - height;
    Some(Bounds {
        origin: point(
            window_bounds.origin.x + px(left),
            window_bounds.origin.y + px(top),
        ),
        size: size(px(width), px(height)),
    })
}

#[cfg(test)]
mod tests {
    use super::account_usage_popup_frame;
    use gpui::{Bounds, point, px, size};
    use gpui_component::tooltip::split_trailing_shortcut;

    fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<gpui::Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(width), px(height)),
        }
    }

    /// The Codex tile in the user's screenshot: the panel sits 8px above it, as tall as its content.
    #[test]
    fn the_usage_panel_floats_above_its_tile_at_its_content_height() {
        let window = bounds(100.0, 50.0, 1200.0, 800.0);
        let tile = bounds(280.0, 652.0, 76.0, 26.0);
        let frame = account_usage_popup_frame(window, tile, 520.0, 380.0).unwrap();
        assert_eq!(
            frame,
            bounds(380.0, 50.0 + 652.0 - 8.0 - 520.0, 380.0, 520.0)
        );
    }

    #[test]
    fn a_tall_panel_shortens_to_the_room_above_its_tile_and_scrolls() {
        let window = bounds(0.0, 0.0, 1200.0, 500.0);
        let tile = bounds(10.0, 452.0, 76.0, 26.0);
        let frame = account_usage_popup_frame(window, tile, 900.0, 380.0).unwrap();
        assert_eq!(frame.top(), px(8.0));
        assert_eq!(frame.bottom(), px(452.0 - 8.0));
    }

    #[test]
    fn a_tile_without_room_above_takes_the_ordinary_placement() {
        let window = bounds(0.0, 0.0, 1200.0, 800.0);
        assert!(
            account_usage_popup_frame(window, bounds(10.0, 40.0, 76.0, 26.0), 500.0, 380.0)
                .is_none()
        );
    }

    #[test]
    fn a_bracketed_chord_after_the_label_splits_off() {
        assert_eq!(
            split_trailing_shortcut("Talk to the voice agent (⌃⌥V)"),
            Some(("Talk to the voice agent", "⌃⌥V"))
        );
        assert_eq!(
            split_trailing_shortcut("Back (Ctrl+[)"),
            Some(("Back", "Ctrl+["))
        );
        assert_eq!(
            split_trailing_shortcut("Save all (⌘K ⌘S)"),
            Some(("Save all", "⌘K ⌘S"))
        );
        assert_eq!(
            split_trailing_shortcut("Send (Shift+Enter)"),
            Some(("Send", "Shift+Enter"))
        );
    }

    #[test]
    fn other_brackets_stay_in_the_text() {
        for text in [
            "Runs in agentbox main (local)",
            "Send (⌃Enter interrupts)",
            "Inherit (Opus 5.5)",
            "Zoom (+)",
            "(⌘K)",
            "Plain label",
            "Line one\nLine two (⌘K)",
        ] {
            assert_eq!(split_trailing_shortcut(text), None, "{text}");
        }
    }
}
