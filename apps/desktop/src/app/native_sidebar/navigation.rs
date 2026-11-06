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

/// A built-in extension's button (Claude's): in the sidebar's bottom bar, and while Claude is on
/// also in the header's copy of the top row. Listed by the desktop's
/// app/claude_web/sidebar_button.rs (the web build lists none).
pub(crate) struct SidebarExtensionButton {
    /// The view it opens (`TitlebarMode::Extension`).
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) icon: &'static str,
    /// Its view is the one on screen.
    pub(crate) showing: bool,
}

/// The gap between the top row's buttons, in unscaled points (the row's `gap`).
const SIDEBAR_TOP_ROW_GAP: f32 = 4.0;

/// The row's side padding, in unscaled points.
const SIDEBAR_TOP_ROW_PADDING: f32 = 5.0;

/// The two leading toggles (Toggle sidebar, Agents Panel) in the docked row, before the zoom: each is its
/// 18pt icon, its 1.5pt left offset and 7pt of padding either side (`header_panel_toggle_button`),
/// 4pt apart.
const SIDEBAR_TOP_ROW_TOGGLES_WIDTH: f32 = 2.0
    * (2.0 * TITLEBAR_BUTTON_HORIZONTAL_PADDING
        + TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE
        + TITLEBAR_SIDEBAR_COLLAPSE_ICON_LEFT_OFFSET)
    + 4.0;

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

/// Whether `button_count` right-aligned buttons all fit in the top row at their narrowest
/// (`SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH`). The row that owns the window corner (`corner_toggle_x`,
/// the toggle's window x there) also holds the leading reserve and the two toggles; the
/// hover-reveal row is buttons only.
///
/// CDXC:Sidebar 2026-10-28 WHY:
/// The sidebar menu is the row's last button, so it is what slides off the edge when the row is
/// too narrow (GitHub #288). Extension buttons (Claude's) therefore show only while every button
/// fits at its minimum, so they give way before anything else is clipped.
///
/// Narrowest docked width that fits the row at 1x (macOS, 88pt reserve; Windows and Linux need
/// 78pt less): Claude + Search + menu 233pt (so it shows at the 235pt default), + the bell (or
/// the Hermes button) 256pt, + both 279pt. Every further button adds 23pt.
fn sidebar_top_row_fits(
    sidebar_width: f32,
    scale: f32,
    corner_toggle_x: Option<f32>,
    button_count: usize,
) -> bool {
    let gap = SIDEBAR_TOP_ROW_GAP * scale;
    let count = button_count as f32;
    let buttons = count * SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale + (count - 1.0).max(0.0) * gap;
    let (reserved, toggles) = if let Some(toggle_x) = corner_toggle_x {
        (
            toggle_x + SIDEBAR_TOP_ROW_PADDING * scale,
            SIDEBAR_TOP_ROW_TOGGLES_WIDTH * scale + gap,
        )
    } else {
        (2.0 * SIDEBAR_TOP_ROW_PADDING * scale, 0.0)
    };
    sidebar_width - reserved + 0.01 >= toggles + buttons
}

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
            false,
            cx,
        )
    }

    /// The top row (or with `footer` the bottom bar). `owns_window_corner` draws it as the docked
    /// sidebar's top row, in the window's top-left corner: the desktop's Claude header draws that
    /// same row while Claude has the sidebar hidden (app/claude_web/header.rs), and only that copy
    /// sets `extension_buttons_on_top`, which puts Claude's button back in the top row.
    pub(crate) fn render_native_sidebar_row(
        &self,
        appearance: &SidebarAppearance,
        footer: bool,
        usage_meter_count: usize,
        owns_window_corner: bool,
        extension_buttons_on_top: bool,
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
        Updated 2026-11-06 (GitHub #432): on macOS the floating sidebar starts at the window's top and
        shows the lights at this row's start, which it keeps clear (`lights_reserve`), and in full
        screen no row reserves them (CDXC:Titlebar 2026-11-06 DECISION in helpers/work_area_frame.rs).
        CDXC:Sidebar 2026-09-23 DECISION:
        User: on Windows and Linux too, the Toggle sidebar and Agents Panel buttons sit at the top
        left of the sidebar, and dragging the sidebar's top row moves the window. So this row owns
        the corner on every OS; only the traffic-light reserve is macOS's own
        (`SIDEBAR_TOGGLE_LEADING_X`). This supersedes the 2026-09-20 rule that kept the row a plain
        row off macOS, with both toggles in the workarea header.
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
        moved Search and Notifications into the menu together.
        */
        /*
        CDXC:Sidebar 2026-09-21 DECISION:
        User: the Toggle sidebar button sits left of Search, so it stays in exactly the same spot
        always. While the sidebar is docked this row owns the window's top-left corner, so it draws
        the button (right after the traffic lights on macOS) at the unscaled x the collapsed
        workarea header draws it (`SIDEBAR_TOGGLE_LEADING_X`); the header only draws it while
        collapsed.
        Updated 2026-11-06 (GitHub #432): on macOS the lights hide with the sidebar, so the header
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
        let bottom_padding = (if footer { 6.0 } else { 3.0 }) * scale;
        let (row_height, top_padding) = if owns_window_corner || lights_row.is_some() {
            let top_padding = match lights_row {
                Some((_, gutter)) => lights_row_top_padding(scale, gutter),
                None => corner_row_top_padding(scale),
            };
            (
                top_padding + TITLEBAR_CONTROL_HEIGHT * scale + bottom_padding,
                top_padding,
            )
        } else {
            ((if footer { 39.0 } else { 35.0 }) * scale, 5.0 * scale)
        };
        h_flex()
            .w_full()
            .h(px(row_height))
            .pt(px(top_padding))
            .pb(px(bottom_padding))
            .px(px(5.0 * scale))
            .gap(px(4.0 * scale))
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
            User: the Toggle sidebar and Agents Panel buttons at the top of the sidebar have the
            same color as the notification bell there, so this row draws their icons in the
            sidebar's icon grey rather than the header's bright one (since 2026-10-29 that grey is the header's own icon colour, see `SidebarAppearance::chrome_icon`).
            */
            .when(owns_window_corner, |row| {
                // The row's unscaled leading padding puts the button on the header's x, whatever
                // the zoom; from there it scales like the rest of the row (GitHub #375). The spacer
                // keeps it left of the row's other buttons, which align right.
                row.child(
                    div()
                        .flex_shrink_0()
                        .child(self.render_sidebar_collapse_button(Some(appearance), cx)),
                )
                // Right of Toggle sidebar, as in the collapsed header (workarea_header/breadcrumb.rs).
                .child(
                    div()
                        .flex_shrink_0()
                        .child(self.render_workarea_header_agents_toggle(Some(appearance), cx)),
                )
                // The negative margin cancels the row gap this zero-width spacer adds, so the
                // buttons right of it keep their even 38px slots.
                .child(div().flex_1().mr(px(-4.0 * scale)))
            })
            .when_some(bots_toggle, |row, bots_mode| {
                row.child(self.render_native_sidebar_bots_toggle(bots_mode, appearance, cx))
            })
            /*
            CDXC:Extensions 2026-10-28 DECISION:
            User: "move the Claude button here up to where 'Issues' button is and make this button
            match this color #7C828C (shouldn't be orange)". Claude's button sits in the top row,
            left of Search, and is drawn in the row's icon colour like its neighbours (first #7C828C,
            since 2026-10-29 the header's icon grey, see `SidebarAppearance::chrome_icon`); selected is a hover fill with a bright icon. The bottom bar keeps
            Commands. This supersedes the 2026-10-25 bottom-bar placement
            (app/claude_web/sidebar_button.rs). Issues' briefcase left this row afterwards (see
            CDXC:WorkMode 2026-10-29 in app/work_view/host.rs), so Claude is the row's first
            right-aligned button after the Hermes one.
            CDXC:Extensions 2026-11-04 DECISION:
            The sidebar's own top row no longer draws Claude's button: it is in the bottom bar,
            right of Commands (below). User, asked whether the header's copy of this row keeps it
            while Claude is on and the sidebar hidden: "keep it at the top in this case please". So
            only that copy (`extension_buttons_on_top`, app/claude_web/header.rs) draws it here, in
            its old spot, so Claude closes from where it was.
            */
            .when(!footer && extension_buttons_on_top, |row| {
                // Search, the menu and each button the row shows now.
                let other_buttons =
                    2 + usize::from(bots_toggle.is_some()) + usize::from(bell_visible);
                row.children(self.render_native_sidebar_extension_buttons(
                    appearance,
                    Some((corner_toggle_x, other_buttons)),
                    cx,
                ))
            })
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
                    .h(px(28.0 * scale))
                    .w(px(34.0 * scale))
                    .rounded(px(5.0 * scale))
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
            /*
            CDXC:Extensions 2026-11-04 DECISION:
            User: "move the claude button from the top of the sidebar to the bottom of it please
            (right of the commands button in the bottom of hte sidebar)". Claude's button sits in
            the bottom bar directly right of Commands, sized, spaced, hovered and tooltipped like
            Commands, in the bar's icon colour; selected is a hover fill with a bright icon. This
            supersedes the 2026-10-28 top-row placement beside Search, except in the Claude
            header's copy of the top row (above).
            */
            .when(footer, |row| {
                row.children(self.render_native_sidebar_extension_buttons(appearance, None, cx))
            })
            // CDXC:Notifications 2026-09-20 DECISION:
            // User: the notification bell sits in the sidebar's top row, before the sidebar menu button.
            .when(bell_visible, |row| {
                row.child(self.render_sidebar_notification_bell(appearance, cx))
            })
            .when(!footer, |row| {
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
                        .h_full()
                        .w(px(34.0 * scale))
                        .rounded(px(5.0 * scale))
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

    /// The extension buttons: the extension's own mark in the row's icon colour, its title in the
    /// tooltip; while its view is on screen it is drawn with the hover fill and a bright icon. In
    /// the bottom bar (`top_row` is `None`) they narrow like Commands. In the top row
    /// (`Some((corner_toggle_x, other_buttons))`) they give way first: none shows while the row
    /// could not fit them and its other `other_buttons` at their minimum width.
    fn render_native_sidebar_extension_buttons(
        &self,
        appearance: &SidebarAppearance,
        top_row: Option<(Option<f32>, usize)>,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        let scale = appearance.scale;
        let tooltip_delay = appearance.tooltip_delay;
        let buttons = self.sidebar_extension_buttons();
        if let Some((corner_toggle_x, other_buttons)) = top_row
            && !sidebar_top_row_fits(
                self.sidebar_width,
                scale,
                corner_toggle_x,
                other_buttons + buttons.len(),
            )
        {
            return Vec::new();
        }
        buttons
            .into_iter()
            .map(|button| {
                let SidebarExtensionButton {
                    id,
                    title,
                    icon,
                    showing,
                } = button;
                let click_id = id.clone();
                let tooltip = title.clone();
                div()
                    .id(gpui::SharedString::from(format!(
                        "native-sidebar-extension-{id}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(title)
                    .aria_selected(showing)
                    .when(cfg!(target_os = "windows"), |button| button.occlude())
                    .h(px(28.0 * scale))
                    .w(px(34.0 * scale))
                    .rounded(px(5.0 * scale))
                    .flex()
                    .flex_shrink(1.0)
                    .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .when(showing, |button| button.bg(appearance.hover))
                    .hover(|button| button.bg(appearance.hover))
                    .child(titlebar_svg_icon(
                        icon,
                        15.0 * scale,
                        if showing {
                            titlebar_active_text_color()
                        } else {
                            appearance.chrome_icon
                        },
                    ))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.toggle_sidebar_extension_view(&click_id, window, cx);
                    }))
                    .managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::Right,
                        tooltip_delay,
                        move |window, cx| {
                            super::tooltips::sidebar_style_tooltip(tooltip.clone(), window, cx)
                        },
                    )
                    .into_any_element()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The docked row's toggle x (no frame), or none for the hover-reveal row.
    fn corner_x(corner: bool) -> Option<f32> {
        corner.then_some(SIDEBAR_TOGGLE_LEADING_X)
    }

    // The row's width at which `count` buttons just fit, found by the same rule.
    fn narrowest_fitting(scale: f32, corner: bool, count: usize) -> f32 {
        (50..800)
            .map(|width| width as f32)
            .find(|width| sidebar_top_row_fits(*width, scale, corner_x(corner), count))
            .expect("fits below 800pt")
    }

    #[test]
    fn extra_button_needs_one_more_slot() {
        for corner in [true, false] {
            let without = narrowest_fitting(1.0, corner, 4);
            let with = narrowest_fitting(1.0, corner, 5);
            assert!((with - without - 23.0).abs() <= 1.0);
        }
    }

    #[test]
    fn narrowest_docked_row_matches_the_hand_count() {
        // Claude, Search, the bell and the menu at 19pt with 4pt gaps = 88pt, plus the two
        // toggles (67pt) and their gaps (4 + 4), plus the leading reserve and 5pt right padding.
        let expected = SIDEBAR_TOGGLE_LEADING_X + 5.0 + 67.0 + 4.0 + 4.0 + 88.0;
        assert_eq!(narrowest_fitting(1.0, true, 4), expected);
        // Without the bell: Claude, Search and the menu.
        assert_eq!(narrowest_fitting(1.0, true, 3), expected - 23.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn claude_fits_the_default_sidebar_on_macos_without_the_bell() {
        const DEFAULT_WIDTH: f32 = crate::app::consts::DEFAULT_SIDEBAR_WIDTH;
        // Claude, Search and the menu.
        assert!(sidebar_top_row_fits(DEFAULT_WIDTH, 1.0, corner_x(true), 3));
        assert!(!sidebar_top_row_fits(232.0, 1.0, corner_x(true), 3));
        // With the bell (or Hermes) it needs a wider sidebar, and then it gives way.
        assert!(!sidebar_top_row_fits(DEFAULT_WIDTH, 1.0, corner_x(true), 4));
        assert!(sidebar_top_row_fits(256.0, 1.0, corner_x(true), 4));
    }

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
    fn the_docked_row_is_unchanged_at_full_size() {
        assert_eq!(corner_row_top_padding(1.0), 5.0);
    }

    #[test]
    fn a_very_large_zoom_keeps_the_docked_row_inside_the_window() {
        assert_eq!(corner_row_top_padding(2.0), 0.0);
    }

    #[test]
    fn the_toggles_room_scales_with_the_zoom() {
        // Three buttons and the toggles fit a sidebar that is wide enough at 80% and not at 125%.
        let at_80 = narrowest_fitting(0.8, true, 3);
        let at_125 = narrowest_fitting(1.25, true, 3);
        let reserve = SIDEBAR_TOGGLE_LEADING_X;
        let unscaled = narrowest_fitting(1.0, true, 3) - reserve;
        assert!((at_80 - reserve - unscaled * 0.8).abs() <= 1.0);
        assert!((at_125 - reserve - unscaled * 1.25).abs() <= 1.0);
    }

    #[test]
    fn scale_grows_the_buttons() {
        assert!(!sidebar_top_row_fits(280.0, 1.3, corner_x(false), 10));
        assert!(sidebar_top_row_fits(280.0, 1.0, corner_x(false), 10));
    }

    #[test]
    fn hover_reveal_row_has_no_toggles() {
        assert!(sidebar_top_row_fits(130.0, 1.0, corner_x(false), 5));
        assert!(!sidebar_top_row_fits(130.0, 1.0, corner_x(true), 5));
    }
}
