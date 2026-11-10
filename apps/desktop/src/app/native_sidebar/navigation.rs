use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use serde_json::json;

use super::appearance::SidebarAppearance;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::render::window_drag_region::window_drag_region;
use crate::*;

/// The narrowest a top-row button gets when the sidebar is too narrow for the full 34pt slots:
/// its 15pt icon plus 2pt either side, so every button stays visible.
///
/// CDXC:Sidebar 2026-10-29 DECISION:
/// User, asked whether Claude's button may stay hidden at the default macOS sidebar width (235pt,
/// 1pt short of fitting Claude, Search and the menu at the old 20pt minimum): "Yes, make it fit".
/// The minimum is 19pt (was 20pt), which only matters while a row is squeezed: at any width where
/// the buttons fit at their full 34pt slots, nothing changed.
pub(crate) const SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH: f32 = 19.0;

/// A top-row button's full width, its height and its corners, before the zoom: the one button
/// size every row at the top of the sidebar shares (28 high, corner 7).
///
/// CDXC:Sidebar 2026-11-11 DECISION:
/// User (sidebar top page, Q2420 A, Q2421, Q2422, Q2426): keep the four rows at the top of the sidebar but tidy them: one button height and one corner radius for every row (28 high, radius 7), even spacing, and every row on the same left and right edges (8pt in). Search stays in the top row and Add project moves up beside it, to its right ("i dont like new project shown like button without bg there fam add it to the top row instead please to the right of search"); the bell and the menu stay. When the sidebar is narrow, words give way before controls do.
/// SEE-ALSO: native_sidebar/new_session_row.rs, selectors.rs, machines.rs, bots.rs, unified/header.rs and notification_feed/mod.rs (the bell), which share these sizes.
pub(crate) const SIDEBAR_TOP_ROW_BUTTON_WIDTH: f32 = 30.0;
pub(crate) const SIDEBAR_TOP_ROW_BUTTON_HEIGHT: f32 = 28.0;
pub(crate) const SIDEBAR_TOP_ROW_BUTTON_RADIUS: f32 = 7.0;
/// Where every row at the top of the sidebar ends (and, but for the top row, starts).
pub(crate) const SIDEBAR_TOP_EDGE_INSET: f32 = 8.0;
/// The room between the top row's buttons, and the tighter room once they have to narrow.
const TOP_ROW_GAP: f32 = 4.0;
const TOP_ROW_TIGHT_GAP: f32 = 2.0;

/// How the top row's buttons share the room right of Toggle sidebar.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TopRowFit {
    /// Whether Add project stays in the row; it folds into the menu, whose first row it already
    /// is, when the row cannot hold every button at its narrowest.
    add_project: bool,
    gap: f32,
    /// The row's right padding, which gives way last, only so the menu never runs past the edge.
    trailing: f32,
}

/// `room` is the row's width less its leading padding and Toggle sidebar, `buttons` how many sit
/// right of it counting Add project, `toggle` whether Toggle sidebar is there (its gap counts),
/// `trailing` the right padding the row wants. All in window points, at `scale`.
///
/// CDXC:Sidebar 2026-11-11 WHY:
/// The user skipped the sidebar page's question on the narrowest width (Q2460), so its recommended answer stands: at 190 four buttons cannot fit beside Toggle sidebar, so Add project folds into the menu (its first row) and comes back as soon as all four fit at their narrowest, about 210 wide.
fn top_row_fit(room: f32, buttons: usize, toggle: bool, trailing: f32, scale: f32) -> TopRowFit {
    let need = |count: usize, width: f32, gap: f32| {
        let gaps = if toggle {
            count
        } else {
            count.saturating_sub(1)
        };
        (count as f32 * width + gaps as f32 * gap) * scale
    };
    if need(buttons, SIDEBAR_TOP_ROW_BUTTON_WIDTH, TOP_ROW_GAP) + trailing <= room {
        return TopRowFit {
            add_project: true,
            gap: TOP_ROW_GAP,
            trailing,
        };
    }
    let narrowest = |count| need(count, SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH, TOP_ROW_TIGHT_GAP);
    if narrowest(buttons) + trailing <= room {
        return TopRowFit {
            add_project: true,
            gap: TOP_ROW_TIGHT_GAP,
            trailing,
        };
    }
    TopRowFit {
        add_project: false,
        gap: TOP_ROW_TIGHT_GAP,
        trailing: trailing.min((room - narrowest(buttons - 1)).max(0.0)),
    }
}

/// The y the window corner's controls centre on, on every OS: the macOS traffic lights'
/// (`traffic_light_position` at 11.5pt, 14pt tall) and the workarea header's toggles.
const WINDOW_CORNER_CENTER_Y: f32 = 18.5;

/// The docked row's top padding at this sidebar zoom, which keeps its buttons on
/// `WINDOW_CORNER_CENTER_Y`. It is the row's usual 5pt at 100% and runs out at a zoom so large the
/// buttons would have to start above the window.
///
/// CDXC:Sidebar 2026-10-31 WHY:
/// GitHub #375: the sidebar zoom shrinks this row, so at 90% a padding that scaled with it put the buttons 1.85pt above the traffic lights and the workarea header's own buttons, whatever their size. The lights and the header do not zoom, so this row holds their centre line instead of scaling away from it.
fn corner_row_top_padding(scale: f32) -> f32 {
    (WINDOW_CORNER_CENTER_Y - TITLEBAR_CONTROL_HEIGHT * scale / 2.0).max(0.0)
}

/// The floating sidebar's top row's top padding while the panel carries the traffic lights: the
/// docked row's, which centres its buttons on the lights, `gutter` lower (the work area frame's,
/// which moves the lights down too; `floating_sidebar_lights_row`).
fn lights_row_top_padding(scale: f32, gutter: f32) -> f32 {
    corner_row_top_padding(scale) + gutter
}

/// Where the red light's visible edge starts from the floating sidebar's left edge: its button sits
/// at the window's own light position (`workspace_traffic_light_position`, 11pt without the frame;
/// with it the panel moves in by the same gutter as the lights) and AppKit draws the light's rim
/// half a point inside it (measured at 2x). The lights do not scale with the sidebar's zoom.
const FLOATING_ROW_EDGE_INSET: f32 = 11.5;

/// The floating sidebar's top row's right padding, so its last icon ends as far from the panel's
/// right edge as the red light starts from its left: the inset less what lies between a top-row
/// button's edge and the glyph it draws, at this zoom (`FLOATING_ROW_ICON_INSET`).
///
/// Never below 0: from about 123% the room between a button's edge and its glyph alone is wider
/// than the inset, so there the gap is that room (12.2pt at 130%). Running the row past the
/// sidebar's edge instead would cut the menu button's hover fill and click area at the panel's edge.
///
/// CDXC:Sidebar 2026-11-07 WHY:
/// A contributor change from the GitHub #432 follow-up PR, not a maintainer decision (promote it to a DECISION if you agree with it). The traffic lights and the row's icons sit symmetrically in the floating sidebar's top row: the last icon (the menu) ends as far from the panel's right edge as the red light starts from its left, and the lights and icons share one centre line with as much room under them as above (`lights_row_top_padding`), so the lights read as part of the row rather than as something laid over it.
fn floating_row_trailing_padding(scale: f32) -> f32 {
    (FLOATING_ROW_EDGE_INSET - FLOATING_ROW_ICON_INSET * scale).max(0.0)
}

/// From a top-row button's edge to the visible edge of the glyph it shows, at 100%: the button
/// centres a 15pt icon, whose menu glyph shows about 1.85pt inside its box (fitted to the menu
/// icon measured at 2x at 90%, 100% and 110%, within a device pixel).
const FLOATING_ROW_ICON_INSET: f32 = (SIDEBAR_TOP_ROW_BUTTON_WIDTH - 15.0) / 2.0 + 1.85;

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-19 DECISION:
    /// User: the Search row and the Commands row are each one pixel taller.
    /// Both rows keep those heights now that the hairlines are gone; the pixel each border used to
    /// take out of the border-box went back into the padding, so neither row's content moved.
    pub(crate) fn render_native_sidebar_navigation(
        &self,
        appearance: &SidebarAppearance,
        footer: bool,
        usage_meter_count: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let owns_window_corner = !footer && !self.sidebar_collapsed;
        self.render_native_sidebar_row(
            appearance,
            footer,
            usage_meter_count,
            owns_window_corner,
            cx,
        )
    }

    /// The top row (or with `footer` the bottom bar). `owns_window_corner` draws it as the docked
    /// sidebar's top row, in the window's top-left corner: the desktop's Claude header draws that
    /// same row while Claude has the sidebar hidden (app/claude_web/header.rs).
    pub(crate) fn render_native_sidebar_row(
        &self,
        appearance: &SidebarAppearance,
        footer: bool,
        usage_meter_count: usize,
        owns_window_corner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let label = if footer { "Commands" } else { "Search" };
        let action_id = if footer { "commands" } else { "sessions" };
        let snapshot = self
            .native_sidebar
            .snapshot
            .as_ref()
            .expect("navigation follows a snapshot");
        let shortcut = if footer {
            &snapshot.commands_shortcut
        } else {
            &snapshot.search_shortcut
        };
        // The menu is read when the button is pressed; sharing the snapshot keeps this row from
        // copying the menu's JSON on every frame.
        let more_menu_now = self.native_sidebar_more_menu_now(&snapshot.more_menu);
        /*
        CDXC:Sidebar 2026-09-20 DECISION:
        User: with the titlebar row deleted, the sidebar's Search row is what sits in the window's
        top-left corner, so it reserves the macOS traffic lights and is the window's drag handle
        there. It does so only while the sidebar is docked: the hover-reveal panel renders this same
        row below the workarea header, where there are no lights to clear.
        Superseded in part by CDXC:Titlebar 2026-11-07 DECISION in helpers/work_area_frame.rs: on
        macOS the floating sidebar starts at the window's top and shows the lights at this row's
        start, which it keeps clear (`lights_reserve`), and in full screen no row reserves them.
        CDXC:Sidebar 2026-09-23 DECISION:
        User: on Windows and Linux too, the Toggle sidebar button sits at the top
        left of the sidebar, and dragging the sidebar's top row moves the window. So this row owns
        the corner on every OS; only the traffic-light reserve is macOS's own
        (`SIDEBAR_TOGGLE_LEADING_X`). This supersedes the 2026-09-20 rule that kept the row a plain
        row off macOS, with the toggle in the workarea header.
        CDXC:Sidebar 2026-11-10 DECISION:
        User: "please remove this icon from the top of the sidebar it's confusing" (the chat icon,
        Toggle Agents Panel, that sat right of Toggle sidebar here and in the collapsed header).
        Both draw only Toggle sidebar now; the Agents Panel still folds with Expand side panel
        (Cmd+Ctrl+E, the view tab strip's expand button, the command palette).
        CDXC:Sidebar 2026-09-23 WHY:
        On Windows WM_NCHITTEST reports the Drag region as caption wherever no interactive child
        occludes it, and a caption press never reaches an `on_click`, so every button in this row
        occludes there.
        */
        /*
        CDXC:Sidebar 2026-09-21 DECISION:
        User: the Search and the Commands buttons are icon-only buttons with the icon they already
        show, the label and shortcut in the tooltip ("Search (⌘P)", "Commands (⌘⇧P)"), and the
        buttons of both rows align right, against the sidebar's trailing edge. This supersedes the
        2026-09-20 rule that aligned them left.
        CDXC:Sidebar 2026-10-11 DECISION:
        User: the bottom-left Commands item is always just the lightning icon button, aligned to the
        right of the sidebar's bottom bar with its "Commands (Ctrl+Shift+P)" tooltip, and never
        expands to the "Commands" label when the sidebar is wide. This supersedes the 2026-09-28
        width threshold and its 2026-10-11 441px revision.
        */
        /*
        CDXC:Sidebar 2026-10-11 DECISION:
        User: "i actually want search and notifications to be always shown, there's space for those
        too in all cases" (2026-10-05). Search, the bell and the menu button stay in the top row at
        every sidebar width, and nothing collapses into the sidebar menu any more. Send Bug / Feedback is a
        row of the More options menu (CDXC:Feedback 2026-10-14 in
        packages/gx-core/src/sidebar_menu/navigation.rs). This supersedes the 2026-09-23 rule that
        moved Search and Notifications into the menu together. Add project, which joined this row
        on 2026-11-11, is the one button that gives way on a narrow sidebar: it is the menu's
        first row already (`top_row_fit`).
        */
        /*
        CDXC:Sidebar 2026-09-21 DECISION:
        User: the Toggle sidebar button sits left of Search, so it stays in exactly the same spot
        always. While the sidebar is docked this row owns the window's top-left corner, so it draws
        the button (right after the traffic lights on macOS) at the unscaled x the collapsed
        workarea header draws it (`SIDEBAR_TOGGLE_LEADING_X`); the header only draws it while
        collapsed.
        Superseded in part by CDXC:Titlebar 2026-11-07 DECISION in helpers/work_area_frame.rs: on
        macOS the lights hide with the sidebar, so the header
        draws the button at the edge strip's x and it moves left when the sidebar closes; both rows
        still read one value, `sidebar_toggle_leading_x`.
        */
        // With Spaces on, the Hermes button is the Space row's last slot instead.
        let bots_toggle = (!footer && snapshot.bots_enabled && !snapshot.spaces_enabled)
            .then_some(snapshot.bots_mode);
        let bell_visible = !footer && self.titlebar_notification_bell_visible();
        let icon_path = if footer {
            "titlebar/bolt.svg"
        } else {
            BROWSER_ICON_SEARCH
        };
        let tooltip_label: gpui::SharedString = match shortcut.as_deref() {
            Some(shortcut) if !shortcut.is_empty() => format!("{label} ({shortcut})").into(),
            _ => label.into(),
        };
        let tooltip_delay = appearance.tooltip_delay;
        // The toggle's window x while this row owns the corner: past the traffic lights where the
        // window draws them, at the edge strip's width otherwise (sidebar hidden, full screen).
        let corner_toggle_x = owns_window_corner.then(|| {
            crate::app::helpers::sidebar_toggle_leading_x(self.window_controls_reserved())
        });
        // The floating sidebar's top row while the panel carries the traffic lights at its start
        // (`floating_sidebar_lights_row`). Its buttons stay right-aligned as they are; the row keeps
        // the lights clear by the docked row's reserve, so a narrow or zoomed sidebar narrows its
        // buttons instead of sliding them under the green light, and it holds the lights' centre
        // line (`lights_row_top_padding`).
        let lights_row = (!footer && !owns_window_corner)
            .then(|| self.floating_sidebar_lights_row())
            .flatten();
        let lights_reserve = lights_row.map(|(reserve, _)| reserve);
        // On macOS the hidden sidebar's top row is the floating panel's, which runs from the
        // window's top with or without the lights (full screen has none): it centres its buttons on
        // the lights' line with as much room under them as above, and keeps the lights' inset on
        // its right (`floating_row_trailing_padding`). A panel under the header, while the window
        // keeps its own lights (`floating_sidebar_full_height`), keeps the plain row.
        let floating_row = !footer
            && !owns_window_corner
            && self.sidebar_collapsed
            && self.floating_sidebar_full_height();
        let (row_height, top_padding, bottom_padding) =
            if owns_window_corner || lights_row.is_some() || floating_row {
                let top_padding = match lights_row {
                    Some((_, gutter)) => lights_row_top_padding(scale, gutter),
                    None => corner_row_top_padding(scale),
                };
                let bottom_padding = if floating_row {
                    top_padding
                } else {
                    3.0 * scale
                };
                (
                    top_padding + TITLEBAR_CONTROL_HEIGHT * scale + bottom_padding,
                    top_padding,
                    bottom_padding,
                )
            } else {
                let bottom_padding = (if footer { 6.0 } else { 3.0 }) * scale;
                (
                    (if footer { 39.0 } else { 35.0 }) * scale,
                    5.0 * scale,
                    bottom_padding,
                )
            };
        // The top row's buttons right of Toggle sidebar: Search, Add project, the bell, the menu
        // (and Bots, with Spaces off). The bottom bar keeps its own sizes.
        let fit = (!footer).then(|| {
            let width = match f32::from(self.native_sidebar.bounds.size.width) {
                0.0 => self.sidebar_width,
                width => width,
            };
            let leading = corner_toggle_x.or(lights_reserve).unwrap_or(5.0 * scale);
            let toggle = if owns_window_corner {
                (TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE + 2.0 * TITLEBAR_BUTTON_HORIZONTAL_PADDING)
                    * scale
            } else {
                0.0
            };
            let trailing = if floating_row {
                floating_row_trailing_padding(scale)
            } else {
                SIDEBAR_TOP_EDGE_INSET * scale
            };
            let buttons = 3 + usize::from(bell_visible) + usize::from(bots_toggle.is_some());
            top_row_fit(
                width - leading - toggle,
                buttons,
                owns_window_corner,
                trailing,
                scale,
            )
        });
        let gap = fit.map_or(4.0, |fit| fit.gap) * scale;
        let add_project = fit.is_some_and(|fit| fit.add_project);
        // Top-row buttons are 30 wide, 28 high with corner 7; the bottom bar's stay 34 with corner 5.
        let (button_width, button_radius) = if footer {
            (34.0, 5.0)
        } else {
            (SIDEBAR_TOP_ROW_BUTTON_WIDTH, SIDEBAR_TOP_ROW_BUTTON_RADIUS)
        };
        h_flex()
            .w_full()
            .h(px(row_height))
            .pt(px(top_padding))
            .pb(px(bottom_padding))
            .px(px(5.0 * scale))
            .when_some(fit, |row, fit| row.pr(px(fit.trailing)))
            .gap(px(gap))
            /*
            CDXC:Sidebar 2026-10-24 WHY:
            GitHub #288: a right-aligned row that is wider than the sidebar overflows at its start,
            which slid the Toggle sidebar button left by the shortfall and under the macOS green
            window button (about 37pt at a 241pt sidebar with Search, the bell and the menu). The
            corner row therefore starts at the left, its spacer right-aligns the buttons, and the
            buttons right of the spacer narrow to `SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH` before
            anything overflows, so the toggle keeps its reserved x at every width.
            */
            .when(!owns_window_corner, |row| row.justify_end())
            .when_some(corner_toggle_x, |row, toggle_x| {
                window_drag_region(row.pl(px(toggle_x)))
            })
            .when_some(lights_reserve, |row, reserve| row.pl(px(reserve)))
            .overflow_hidden()
            /*
            CDXC:Sidebar 2026-09-20 DECISION:
            User: there is no rule under the Search row and none above the usage strip or the
            Commands row. The session list fades out at its bottom end only
            (native_sidebar/scroll_fade.rs) and nothing shades its top, so these rows draw no border
            at all. This supersedes the 2026-09-19 rule that framed the list with a hairline at each
            end.
            */
            .flex_shrink_0()
            .text_color(appearance.chrome_icon)
            /*
            CDXC:Sidebar 2026-09-23 DECISION:
            User: the Toggle sidebar button at the top of the sidebar has the same color as the
            notification bell there, so this row draws its icon in the sidebar's icon grey rather
            than the header's bright one (since 2026-10-29 that grey is the header's own icon colour, see `SidebarAppearance::chrome_icon`).
            */
            .when(owns_window_corner, |row| {
                // The header's copy of this row (Claude's) sits under the traffic lights the
                // floating sidebar shows, so its toggles draw nothing there meanwhile
                // (`window_lights_over_header`).
                let under_lights = self.sidebar_collapsed && self.window_lights_over_header();
                // The row's unscaled leading padding puts the button on the header's x, whatever
                // the zoom; from there it scales like the rest of the row (GitHub #375). The spacer
                // keeps it left of the row's other buttons, which align right.
                row.child(
                    div()
                        .flex_shrink_0()
                        .when(under_lights, |button| button.invisible())
                        .child(self.render_sidebar_collapse_button(Some(appearance), cx)),
                )
                // The negative margin cancels the row gap this zero-width spacer adds, so the
                // buttons right of it keep their even slots.
                .child(div().flex_1().mr(px(-gap)))
            })
            .when_some(bots_toggle, |row, bots_mode| {
                row.child(self.render_native_sidebar_bots_toggle(bots_mode, appearance, cx))
            })
            // The Claude and Grok Bot buttons are not in this row or the bottom bar: they are in the workarea header, left of the Voice button (CDXC:Extensions 2026-11-10 in app/claude_web/header_buttons.rs).
            // CDXC:Workspaces 2026-10-11 DECISION: User: move the workspace picker out of the Spaces row to the bottom left of the app, in the sidebar's bottom bar, with 5px more margin from the sidebar's left side than the bar's own padding so it lines up with the account row above it. The bar's other buttons sit right of the spacer.
            .when(footer, |row| {
                let tile = self.render_native_sidebar_footer_workspace_tile(appearance, cx);
                row.children(tile.map(|tile| div().flex_shrink_0().ml(px(5.0 * scale)).child(tile)))
                    .child(div().flex_1())
            })
            .child(
                div()
                    .id(format!("native-sidebar-{label}"))
                    .role(gpui::Role::Button)
                    .aria_label(tooltip_label.clone())
                    .when(cfg!(target_os = "windows"), |button| button.occlude())
                    .h(px(SIDEBAR_TOP_ROW_BUTTON_HEIGHT * scale))
                    .w(px(button_width * scale))
                    .rounded(px(button_radius * scale))
                    .flex()
                    .flex_shrink(1.0)
                    .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .hover(|row| row.bg(appearance.hover))
                    .child(titlebar_svg_icon(
                        icon_path,
                        15.0 * scale,
                        appearance.chrome_icon,
                    ))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.dispatch_native_sidebar_ui(
                            json!({"type": "sidebarAction", "action": action_id}),
                            cx,
                        );
                    }))
                    .map(|button| {
                        self.sidebar_focus_stop(
                            button,
                            format!("navigation:{label}"),
                            super::focus_order::SidebarStopPlace::Fixed,
                            appearance,
                            cx,
                        )
                    })
                    .managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::Right,
                        tooltip_delay,
                        move |window, cx| {
                            super::tooltips::sidebar_style_tooltip(
                                tooltip_label.to_string(),
                                window,
                                cx,
                            )
                        },
                    ),
            )
            // Add project, right of Search (CDXC:Sidebar 2026-11-11 DECISION at the top of this
            // file); it folds into the menu while the row is too narrow for it (`top_row_fit`).
            .when(add_project, |row| {
                row.child(
                    div()
                        .id("native-sidebar-add-project")
                        .role(gpui::Role::Button)
                        .aria_label("Add project")
                        .when(cfg!(target_os = "windows"), |button| button.occlude())
                        .h(px(SIDEBAR_TOP_ROW_BUTTON_HEIGHT * scale))
                        .w(px(SIDEBAR_TOP_ROW_BUTTON_WIDTH * scale))
                        .rounded(px(SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
                        .flex()
                        .flex_shrink(1.0)
                        .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
                        .items_center()
                        .justify_center()
                        .cursor_default()
                        .hover(|button| button.bg(appearance.hover))
                        .child(titlebar_svg_icon(
                            "titlebar/folder-plus.svg",
                            15.0 * scale,
                            appearance.chrome_icon,
                        ))
                        .on_click(cx.listener(|app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(
                                json!({"type": "sidebarAction", "action": "addProject"}),
                                cx,
                            );
                        }))
                        .map(|button| {
                            self.sidebar_focus_stop(
                                button,
                                "navigation:add-project",
                                super::focus_order::SidebarStopPlace::Fixed,
                                appearance,
                                cx,
                            )
                        })
                        .managed_discrete_tooltip_with_placement(
                            ManagedTooltipPlacement::Right,
                            tooltip_delay,
                            |window, cx| {
                                super::tooltips::sidebar_style_tooltip(
                                    "Add project".to_string(),
                                    window,
                                    cx,
                                )
                            },
                        ),
                )
            })
            // CDXC:Notifications 2026-09-20 DECISION:
            // User: the notification bell sits in the sidebar's top row, before the sidebar menu button.
            .when(bell_visible, |row| {
                row.child(self.render_sidebar_notification_bell(appearance, cx))
            })
            .when(!footer, |row| {
                let more_menu_now_keyboard = more_menu_now.clone();
                let more_open = self
                    .native_sidebar
                    .menu
                    .as_ref()
                    .is_some_and(|menu| menu.opened_by(super::menus::SIDEBAR_MORE_TRIGGER));
                row.child(
                    div()
                        .id("native-sidebar-more")
                        .role(gpui::Role::Button)
                        .aria_label("Sidebar menu")
                        .aria_expanded(more_open)
                        .when(cfg!(target_os = "windows"), |button| button.occlude())
                        .h(px(SIDEBAR_TOP_ROW_BUTTON_HEIGHT * scale))
                        .w(px(SIDEBAR_TOP_ROW_BUTTON_WIDTH * scale))
                        .rounded(px(SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
                        .flex()
                        .flex_shrink(1.0)
                        .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
                        .items_center()
                        .justify_center()
                        .cursor_default()
                        .when(more_open, |row| row.bg(appearance.hover))
                        .hover(|row| row.bg(appearance.hover))
                        .child(titlebar_svg_icon(
                            "titlebar/menu-2.svg",
                            15.0 * scale,
                            appearance.chrome_icon,
                        ))
                        .on_prepaint({
                            let bounds = self.native_sidebar.more_button_bounds.clone();
                            move |painted, _, _| bounds.set(Some(painted))
                        })
                        .on_mouse_down(MouseButton::Left, {
                            let bounds = self.native_sidebar.more_button_bounds.clone();
                            move |event: &gpui::MouseDownEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                let trigger = bounds.get().unwrap_or_else(|| gpui::Bounds {
                                    origin: event.position,
                                    size: gpui::size(gpui::px(1.0), gpui::px(1.0)),
                                });
                                Self::toggle_native_sidebar_more_menu(
                                    &more_menu_now,
                                    trigger,
                                    event.position,
                                    scale,
                                    window,
                                    cx,
                                );
                            }
                        })
                        // The menu opens on the press; Enter and Space reach it as a click.
                        .on_click({
                            let bounds = self.native_sidebar.more_button_bounds.clone();
                            let more_menu_now = more_menu_now_keyboard;
                            move |event: &gpui::ClickEvent, window, cx| {
                                if !event.is_keyboard() {
                                    return;
                                }
                                cx.stop_propagation();
                                let Some(trigger) = bounds.get() else {
                                    return;
                                };
                                Self::toggle_native_sidebar_more_menu(
                                    &more_menu_now,
                                    trigger,
                                    trigger.center(),
                                    scale,
                                    window,
                                    cx,
                                );
                            }
                        })
                        .map(|button| {
                            self.sidebar_focus_stop(
                                button,
                                "navigation:more",
                                super::focus_order::SidebarStopPlace::Fixed,
                                appearance,
                                cx,
                            )
                        }),
                )
            })
            /*
            CDXC:Sidebar 2026-09-21 DECISION:
            User: Settings gets a one-click gear immediately to the right of the Commands
            button. The gear is now the sidebar's only Settings entry: the sidebar menu no longer carries
            Settings or Hotkeys (packages/gx-core/src/sidebar_menu/navigation.rs). This
            supersedes the 2026-09-20 rule that kept both entries in the menu as a
            deliberate duplicate.
            */
            .when(footer, |row| {
                row.children(self.render_native_sidebar_usage_toggle(
                    appearance,
                    usage_meter_count,
                    cx,
                ))
            })
            .when(footer, |row| {
                row.child(
                    div()
                        .id("native-sidebar-settings")
                        .role(gpui::Role::Button)
                        .aria_label("Settings")
                        .h(px(28.0 * scale))
                        .w(px(34.0 * scale))
                        .rounded(px(5.0 * scale))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .cursor_default()
                        .hover(|row| row.bg(appearance.hover))
                        .child(titlebar_svg_icon(
                            TITLEBAR_ICON_SETTINGS,
                            15.0 * scale,
                            appearance.chrome_icon,
                        ))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.dispatch_native_sidebar_ui(
                                json!({"type": "sidebarAction", "action": "settings"}),
                                cx,
                            );
                        }))
                        .map(|button| {
                            self.sidebar_focus_stop(
                                button,
                                "navigation:settings",
                                super::focus_order::SidebarStopPlace::Fixed,
                                appearance,
                                cx,
                            )
                        })
                        .managed_discrete_tooltip_with_placement(
                            ManagedTooltipPlacement::Right,
                            appearance.tooltip_delay,
                            |window, cx| {
                                super::tooltips::sidebar_style_tooltip(
                                    titlebar_tooltip_label("Settings", "openSettings").to_string(),
                                    window,
                                    cx,
                                )
                            },
                        ),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zooming_keeps_the_docked_row_on_the_window_corner_centre_line() {
        for scale in [0.5, 0.8, 0.9, 1.0, 1.1, 1.25] {
            let centre = corner_row_top_padding(scale) + TITLEBAR_CONTROL_HEIGHT * scale / 2.0;
            assert!((centre - WINDOW_CORNER_CENTER_Y).abs() < 0.001, "{scale}");
        }
    }

    #[test]
    fn the_floating_row_centres_on_the_lights_it_carries() {
        // The lights are 14pt tall at (11, 11.5), or one 6pt gutter lower and further in with the
        // work area frame on.
        for (gutter, lights_top) in [(0.0, 11.5), (6.0, 17.5)] {
            for scale in [0.9, 1.0, 1.1] {
                let centre =
                    lights_row_top_padding(scale, gutter) + TITLEBAR_CONTROL_HEIGHT * scale / 2.0;
                assert!(
                    (centre - (lights_top + 7.0)).abs() < 0.001,
                    "{gutter} {scale}"
                );
            }
        }
        // At full size and without the frame the row keeps the floating row's 5pt padding.
        assert_eq!(lights_row_top_padding(1.0, 0.0), 5.0);
    }

    #[test]
    fn the_floating_row_ends_its_last_icon_at_the_lights_inset() {
        for scale in [0.9, 1.0] {
            let gap = floating_row_trailing_padding(scale) + FLOATING_ROW_ICON_INSET * scale;
            assert!((gap - FLOATING_ROW_EDGE_INSET).abs() < 0.001, "{scale}");
        }
        // Larger, the buttons stay inside the sidebar: the gap is their own room.
        for scale in [1.25, 1.3] {
            assert_eq!(floating_row_trailing_padding(scale), 0.0);
        }
    }

    #[test]
    fn the_docked_row_is_unchanged_at_full_size() {
        assert_eq!(corner_row_top_padding(1.0), 5.0);
    }

    #[test]
    fn a_very_large_zoom_keeps_the_docked_row_inside_the_window() {
        assert_eq!(corner_row_top_padding(2.0), 0.0);
    }
}
