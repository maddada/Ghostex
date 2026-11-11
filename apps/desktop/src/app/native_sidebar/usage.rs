// The bottom of the sidebar: the account usage strip, which is the last thing in it, the small
// handle that brings the strip back while it is hidden, and the workspace button's line above
// them. The meters themselves are the shared renderer in app/titlebar/account_usage.rs; this
// module owns only the strip around them.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, deferred, div, px,
};
use gpui_component::ElementExt as _;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::{h_flex, v_flex};

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::titlebar::account_usage::{
    ACCOUNT_USAGE_BADGE_GAP, ACCOUNT_USAGE_BADGE_GLYPH_WIDTH, ACCOUNT_USAGE_BADGE_TEXT_SIZE,
    GpuiAccountUsageMeter, GpuiAccountUsageMeterHost, GpuiAccountUsageMeterRoute,
};

/// The narrowest a meter reads at: a provider glyph plus two monospace numbers.
/// The strip fits as many of these per row as the current sidebar width allows.
const SIDEBAR_USAGE_METER_MIN_WIDTH: f32 = 58.0;

/// Four meters per row, whatever the sidebar's width allows below that.
const SIDEBAR_USAGE_MAX_COLUMNS: usize = 4;

/// The space between two cards, and between two rows of them.
const SIDEBAR_USAGE_GAP: f32 = 4.0;

/// The strip's own inset. The cards fill the width left between these two edges,
/// so this is also the gap the user sees before the first card and after the last.
const SIDEBAR_USAGE_INSET: f32 = 8.0;

/// One monospaced character cell as a fraction of the badge's text size. Only the clamp
/// in `align_badge_columns` reads it, and it errs high on purpose: over-estimating a cell
/// pads one character less, while under-estimating would let a card overflow its column.
const SIDEBAR_USAGE_BADGE_ADVANCE: f32 = 0.65;

/// The line the workspace button sits in, above the strip: its button's height, the space above
/// it, and the space under it while the line is the sidebar's last (the strip hidden or absent).
const SIDEBAR_BOTTOM_LINE_BUTTON_HEIGHT: f32 = 28.0;
const SIDEBAR_BOTTOM_LINE_TOP_PADDING: f32 = 5.0;
const SIDEBAR_BOTTOM_LINE_BOTTOM_PADDING: f32 = 6.0;

/// The line that holds only the handle, while the strip is hidden and there is no workspace
/// button: the handle and the space under it.
const SIDEBAR_USAGE_HANDLE_LINE_HEIGHT: f32 = 24.0;
const SIDEBAR_USAGE_HANDLE_BOTTOM_PADDING: f32 = 6.0;

/// The message the strip's right-click row posts, answered where the sidebar menu's own rows are
/// (gx_store/sidebar_more_menu.rs).
pub(crate) const HIDE_SIDEBAR_USAGE_MESSAGE_TYPE: &str = "hideSidebarAccountUsage";

/// The handle's own size: a small chip with the chart glyph.
const SIDEBAR_USAGE_HANDLE_WIDTH: f32 = 30.0;
const SIDEBAR_USAGE_HANDLE_HEIGHT: f32 = 16.0;

/// A meter card's height, before the zoom scale.
const SIDEBAR_USAGE_METER_HEIGHT: f32 = 26.0;

/// The strip's space above its first row of cards and below its last.
const SIDEBAR_USAGE_TOP_PADDING: f32 = 10.0;
const SIDEBAR_USAGE_BOTTOM_PADDING: f32 = 8.0;

/// The frosted panel's padding around the cards. The panel is inset by the rest of the strip's
/// padding, so the cards sit exactly where the pinned strip puts them.
const SIDEBAR_USAGE_FROSTED_PADDING: f32 = 4.0;

impl GhostexGpuiApp {
    /// CDXC:Sidebar 2026-09-22 DECISION:
    /// User: hovering the account usage button at the bottom of the sidebar shows the accounts floating over the bottom of the list, without pushing the content or the scroll area, at exactly the place they take when pinned; clicking the button pins the strip there above the Commands row as before. The button keeps its bar-chart icon by default and only turns into a pin while hovered: an outline pin while unpinned (the Tabler pin, the exact outline twin of the filled pin, not the old `pin.svg` shape the user rejected), a filled pin while pinned. The near-limit dot the button carried is gone. This supersedes the 2026-09-20 rule that the chart button was a plain show/hide toggle: a click still pins and unpins, and a hover now peeks.
    ///
    /// CDXC:Sidebar 2026-09-20 DECISION:
    /// User: each meter is a card with its own background, the cards fill their column so the rows line up, and their content is centred, which is the `space-around` look the user asked for: the gap from the sidebar's edge to the first card's content matches the gap from the last card's content to the other edge. The strip also sits lower, with more room between the session list and the first row of cards.
    ///
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// The strip itself takes no click any more. A press on a meter still opens that account's usage popup, and the strip's padding and its leftover columns are inert, so the only ways in and out are the handle and the strip's right-click menu (`render_native_sidebar_bottom`).
    pub(crate) fn render_native_sidebar_usage(
        &self,
        appearance: &SidebarAppearance,
        meters: Vec<GpuiAccountUsageMeter>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.sidebar_usage_visible {
            return None;
        }
        self.render_native_sidebar_usage_strip(appearance, meters, None, window, cx)
    }

    /// The unpinned strip, floating over the bottom of the list while the pin or the strip
    /// itself is hovered. It is a deferred, absolutely placed child of the sidebar root, so the
    /// list and its scroll area keep their size.
    ///
    /// CDXC:Sidebar 2026-09-26 DECISION:
    /// User, of the peeking strip's near-black band under window glass: "make these glass too please when not pinned, they're dark bg, dont fit transparent aesthetic". While it peeks over the list under glass (macOS and Windows), the strip is a frosted panel in a blurred window of its own (`FrostedHostKind::SidebarUsage`), with the frosted menus' fill and border, over the rows it covers; its meters take their presses there and hand them to this window, where the usage popup and the account menu belong. This window keeps the strip's frame and hover box, and occludes that frame so the rows under the panel do not light up (it goes on getting pointer moves beneath the panel as the key window); the user approved that occluding region ("Yes, OK"), as for the sidebar menus. Pinned, the strip already sits on the sidebar's own glass. Glass off, and on Linux, the band stays as it was.
    pub(crate) fn render_native_sidebar_usage_peek(
        &self,
        appearance: &SidebarAppearance,
        meters: Vec<GpuiAccountUsageMeter>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        use crate::app::window::frosted_host::{
            FrostedHostKind, frosted_hosting_active, hide_frosted_host_over,
        };
        let frosted = frosted_hosting_active() && window_glass_active_in(window);
        // The frosted window's hover counts only while the strip draws there: once it is hidden it
        // may never see the pointer leave.
        let showing = !self.sidebar_usage_visible
            && (self.native_sidebar.usage_pin_hovered
                || self.native_sidebar.usage_peek_hovered
                || (frosted && self.native_sidebar.usage_frosted_hovered));
        if !(showing && frosted) {
            // Only this window's peek: the host is shared with the other workspace windows.
            hide_frosted_host_over(FrostedHostKind::SidebarUsage, window.window_handle(), cx);
        }
        if !showing {
            return None;
        }
        let scale = appearance.scale;
        // The painted box sits on top of the handle's line, so the handle stays under the pointer
        // for the click that pins the strip. Its hover box alone reaches down through the line's
        // top padding to the handle's own hitbox, so a pointer sliding from the handle up into a
        // card never crosses a strip of nothing that would have closed the peek halfway.
        let (line_height, bridge) = self.native_sidebar_usage_handle_line();
        let bridge = bridge * scale;
        let bottom = line_height * scale - bridge;
        let body = if frosted {
            self.render_native_sidebar_usage_frosted_placeholder(appearance, meters.len(), cx)?
        } else {
            let strip =
                self.render_native_sidebar_usage_strip(appearance, meters, None, window, cx)?;
            div()
                .w_full()
                .bg(titlebar_background())
                .border_t_1()
                .border_color(appearance.hover)
                .child(strip)
                .into_any_element()
        };
        Some(
            deferred(
                div()
                    .id("native-sidebar-usage-peek")
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(bottom))
                    .pb(px(bridge))
                    .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                        if app.native_sidebar.usage_peek_hovered != *hovered {
                            app.native_sidebar.usage_peek_hovered = *hovered;
                            cx.notify();
                        }
                    }))
                    .child(body),
            )
            .with_priority(6)
            .into_any_element(),
        )
    }

    /// The peek's frame in this window while its strip draws in the frosted host: an empty box of
    /// the strip's height that places the host window on every paint.
    fn render_native_sidebar_usage_frosted_placeholder(
        &self,
        appearance: &SidebarAppearance,
        count: usize,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        use crate::app::window::frosted_host::{FrostedHostKind, show_frosted_host};
        if count == 0 {
            return None;
        }
        let scale = appearance.scale;
        let columns = self.native_sidebar_usage_columns(count, scale);
        let rows = count.div_ceil(columns);
        let height = (SIDEBAR_USAGE_TOP_PADDING + SIDEBAR_USAGE_BOTTOM_PADDING) * scale
            + rows as f32 * SIDEBAR_USAGE_METER_HEIGHT * scale
            + rows.saturating_sub(1) as f32 * SIDEBAR_USAGE_GAP * scale;
        // The panel is inset from the strip's frame so its padding plus this inset is the strip's.
        let inset_x = (SIDEBAR_USAGE_INSET - SIDEBAR_USAGE_FROSTED_PADDING) * scale;
        let inset_top = (SIDEBAR_USAGE_TOP_PADDING - SIDEBAR_USAGE_FROSTED_PADDING) * scale;
        let inset_bottom = (SIDEBAR_USAGE_BOTTOM_PADDING - SIDEBAR_USAGE_FROSTED_PADDING) * scale;
        let app = cx.entity();
        let appearance = appearance.clone();
        Some(
            div()
                .id("native-sidebar-usage-peek-frame")
                .w_full()
                .h(px(height))
                .occlude()
                .on_prepaint(move |bounds, window, cx| {
                    let frame = gpui::Bounds::new(
                        bounds.origin + gpui::point(px(inset_x), px(inset_top)),
                        gpui::size(
                            bounds.size.width - px(2.0 * inset_x),
                            bounds.size.height - px(inset_top + inset_bottom),
                        ),
                    );
                    let route = GpuiAccountUsageMeterRoute {
                        window: window.window_handle(),
                        offset: frame.origin,
                    };
                    let content_app = app.clone();
                    let appearance = appearance.clone();
                    show_frosted_host(
                        FrostedHostKind::SidebarUsage,
                        window.window_handle(),
                        frame,
                        None,
                        std::rc::Rc::new(move |window, cx| {
                            content_app.update(cx, |app, cx| {
                                app.render_native_sidebar_usage_frosted(
                                    &appearance,
                                    route,
                                    window,
                                    cx,
                                )
                            })
                        }),
                        Some(app.clone()),
                        cx,
                    );
                })
                .into_any_element(),
        )
    }

    /// The peeking strip as its frosted host window draws it: the frosted menus' fill and border
    /// around the cards, which hand their presses to the sidebar's own window.
    fn render_native_sidebar_usage_frosted(
        &self,
        appearance: &SidebarAppearance,
        route: GpuiAccountUsageMeterRoute,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        use crate::app::window::frosted_host::SIDEBAR_USAGE_HOST_RADIUS;
        let strip = self
            .render_native_sidebar_usage_strip(
                appearance,
                self.account_usage_meters(),
                Some(route),
                window,
                cx,
            )
            .unwrap_or_else(|| div().into_any_element());
        div()
            .id("native-sidebar-usage-frosted")
            .size_full()
            .overflow_hidden()
            .rounded(px(SIDEBAR_USAGE_HOST_RADIUS))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(frosted_menu_fill(titlebar_popup_menu_background()))
            .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                if app.native_sidebar.usage_frosted_hovered != *hovered {
                    app.native_sidebar.usage_frosted_hovered = *hovered;
                    cx.notify();
                }
            }))
            .child(strip)
            .into_any_element()
    }

    /// How many meters each row holds for `count` meters at the sidebar's current width. The rows
    /// are balanced: the fewest rows that fit, then the smallest per-row count that still needs no
    /// more rows, so the last row is never a lone card.
    ///
    /// CDXC:Sidebar 2026-10-05 DECISION:
    /// User: "make the accounts at the bottom fill up the available space, not show empty space in
    /// the row". The cards of every row share that row's full width equally, including a last row
    /// with fewer cards, which no longer keeps empty columns. This supersedes the 2026-09-20 rule
    /// that kept the last row's empty columns so its cards stayed aligned with the rows above.
    fn native_sidebar_usage_columns(&self, count: usize, scale: f32) -> usize {
        let fitting = self.native_sidebar_usage_fitting_columns(scale);
        let rows = count.div_ceil(fitting).max(1);
        count.div_ceil(rows).max(1)
    }

    /// How many meters fit in a row at the sidebar's current width.
    fn native_sidebar_usage_fitting_columns(&self, scale: f32) -> usize {
        let gap = SIDEBAR_USAGE_GAP * scale;
        // Every column but the last carries a gap, so the row fits one more card than
        // the plain division would allow.
        let usable_width = (self.sidebar_width - 2.0 * SIDEBAR_USAGE_INSET * scale + gap).max(0.0);
        ((usable_width / (SIDEBAR_USAGE_METER_MIN_WIDTH * scale + gap)).floor() as usize)
            .clamp(1, SIDEBAR_USAGE_MAX_COLUMNS)
    }

    /// The strip's rows of cards. `route` is set when it draws in the frosted host window: the
    /// cards hand their presses to the sidebar's window, and the panel around them supplies the
    /// rest of the strip's padding.
    fn render_native_sidebar_usage_strip(
        &self,
        appearance: &SidebarAppearance,
        mut meters: Vec<GpuiAccountUsageMeter>,
        route: Option<GpuiAccountUsageMeterRoute>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if meters.is_empty() {
            return None;
        }
        let scale = appearance.scale;
        let columns = self.native_sidebar_usage_columns(meters.len(), scale);

        /*
        CDXC:Sidebar 2026-09-20 WHY:
        Resting, hovered and popup-open have to stay three separable fills now that resting is a
        filled card. The old neutral hover (grey at 0.12) landed within a point of the card on the
        sidebar's dark fill and simply vanished, so each state is a step of the same ink instead.
        */
        let (card, card_hover, card_open) = if appearance.light {
            (
                gpui::rgb(0x000000).opacity(0.05),
                gpui::rgb(0x000000).opacity(0.10),
                gpui::rgb(0x000000).opacity(0.17),
            )
        } else {
            (
                gpui::rgb(0xffffff).opacity(0.05),
                gpui::rgb(0xffffff).opacity(0.11),
                gpui::rgb(0xffffff).opacity(0.20),
            )
        };
        let host = GpuiAccountUsageMeterHost {
            element_id_prefix: "native-sidebar-usage-meter",
            anchor_key_prefix: "native-sidebar-usage-meter-anchor",
            height: SIDEBAR_USAGE_METER_HEIGHT * scale,
            padding_x: 4.0 * scale,
            corner_radius: 7.0 * scale,
            background: card.into(),
            fill_width: true,
            hover_background: card_hover.into(),
            // Brighter than the hovered card, so the account whose popup is open
            // stays distinct from the one merely under the pointer.
            open_background: card_open.into(),
            scale,
            route,
        };

        align_badge_columns(&mut meters, &host, columns, self.sidebar_width);

        let rows = meters
            .chunks(columns)
            .map(|row| self.render_native_sidebar_usage_row(row, &host, scale, window, cx))
            .collect::<Vec<_>>();

        // In the frosted panel the panel's 1px border takes the first pixel of its padding.
        let (padding_x, padding_top, padding_bottom) = if route.is_some() {
            let padding = (SIDEBAR_USAGE_FROSTED_PADDING * scale - 1.0).max(0.0);
            (padding, padding, padding)
        } else {
            (
                SIDEBAR_USAGE_INSET * scale,
                SIDEBAR_USAGE_TOP_PADDING * scale,
                SIDEBAR_USAGE_BOTTOM_PADDING * scale,
            )
        };
        // The pinned strip's right-click menu, hung from the strip (above it: it is the sidebar's
        // last thing). The peek has none: it is not pinned, and leaves with the pointer.
        let owner = super::menus::MenuOwner::default();
        Some(
            div()
                .id("native-sidebar-usage-strip")
                .w_full()
                .flex_shrink_0()
                .px(px(padding_x))
                .pt(px(padding_top))
                .pb(px(padding_bottom))
                .on_prepaint(super::menus::record_menu_owner(&owner))
                .when(route.is_none(), |strip| {
                    strip.on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                            if !app.sidebar_usage_visible {
                                return;
                            }
                            cx.stop_propagation();
                            let items = serde_json::json!([{
                                "label": "Hide account usage",
                                "icon": "eye-off",
                                "command": {
                                    "type": "command",
                                    "message": { "type": HIDE_SIDEBAR_USAGE_MESSAGE_TYPE },
                                },
                            }]);
                            Self::show_native_sidebar_context_menu(
                                &items,
                                owner.get(),
                                event.position,
                                scale,
                                window,
                                cx,
                            );
                        }),
                    )
                })
                .child(
                    v_flex()
                        .w_full()
                        .gap(px(SIDEBAR_USAGE_GAP * scale))
                        .cursor_default()
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    fn render_native_sidebar_usage_row(
        &self,
        row: &[GpuiAccountUsageMeter],
        host: &GpuiAccountUsageMeterHost,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .gap(px(SIDEBAR_USAGE_GAP * scale))
            .children(row.iter().map(|meter| {
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .child(self.render_account_usage_meter(meter, host, window, cx))
            }))
            .into_any_element()
    }

    /// The height of the line the handle sits in, and how far down from the line's top the
    /// handle's hitbox starts (both before the zoom): the workspace button's line when that
    /// button shows, else a short line of its own.
    fn native_sidebar_usage_handle_line(&self) -> (f32, f32) {
        if self.native_sidebar_footer_workspace_tile_shown() {
            (
                SIDEBAR_BOTTOM_LINE_TOP_PADDING
                    + SIDEBAR_BOTTOM_LINE_BUTTON_HEIGHT
                    + SIDEBAR_BOTTOM_LINE_BOTTOM_PADDING,
                SIDEBAR_BOTTOM_LINE_TOP_PADDING,
            )
        } else {
            (SIDEBAR_USAGE_HANDLE_LINE_HEIGHT, 0.0)
        }
    }

    /// The bottom of the sidebar: the workspace button's line (only while Workspaces is on), then
    /// the usage strip, which is the last thing in the sidebar; or, while the strip is hidden, the
    /// handle that brings it back, at the right end of that line.
    ///
    /// CDXC:Sidebar 2026-11-11 DECISION:
    /// User (sidebar bottom page, Q2530 A): "want usage to be bottom fully, not have buttons there below them when they are shown". The row of three buttons under the usage meters is gone, on every sidebar (docked and floating). Commands is a row of the sidebar's menu (Q2531) and keeps its hotkey; Settings is that menu's last row and keeps its hotkey, so there is no gear at the bottom; the chart button is a small handle at the bottom right that shows only while the meters are hidden and works as the button did (point at it to peek at the meters, click it to pin them), and a right-click on the pinned meters offers Hide account usage. The workspace button (only while Workspaces is on) sits at the left of a line of its own above the meters. This supersedes the 2026-09-21 rules that put Commands and the Settings gear in a bottom bar, the 2026-09-22 rule that the peek sits exactly where the pinned strip does (it sits on the handle's line, so the handle stays clickable under it), and the place of the 2026-10-11 workspace button (bottom left still, on its own line now).
    pub(crate) fn render_native_sidebar_bottom(
        &self,
        appearance: &SidebarAppearance,
        pinned_meters: Vec<GpuiAccountUsageMeter>,
        meter_count: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let tile = self.render_native_sidebar_footer_workspace_tile(appearance, cx);
        // The handle exists only once an account is starred, and only while its meters are hidden.
        let hidden = meter_count > 0 && !self.sidebar_usage_visible;
        let handle = hidden.then(|| self.render_native_sidebar_usage_handle(appearance, cx));
        let line = if let Some(tile) = tile {
            Some(
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .pt(px(SIDEBAR_BOTTOM_LINE_TOP_PADDING * scale))
                    // The strip under it brings its own space above its cards.
                    .when(hidden || meter_count == 0, |line| {
                        line.pb(px(SIDEBAR_BOTTOM_LINE_BOTTOM_PADDING * scale))
                    })
                    .px(px(SIDEBAR_USAGE_INSET * scale))
                    // CDXC:Workspaces 2026-10-11 DECISION: User: the workspace picker sits at the
                    // bottom left of the app, a little in from the sidebar's edge (10px, as it
                    // was in the bottom bar) so it lines up with the account cards under it.
                    .child(div().flex_shrink_0().ml(px(2.0 * scale)).child(tile))
                    .child(div().flex_1())
                    .children(handle)
                    .into_any_element(),
            )
        } else {
            handle.map(|handle| {
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(SIDEBAR_USAGE_HANDLE_LINE_HEIGHT * scale))
                    .px(px(SIDEBAR_USAGE_INSET * scale))
                    .justify_end()
                    .child(handle)
                    .into_any_element()
            })
        };
        v_flex()
            .w_full()
            .flex_shrink_0()
            .children(line)
            .children(self.render_native_sidebar_usage(appearance, pinned_meters, window, cx))
            .into_any_element()
    }

    /// The handle that brings hidden meters back: a small chip with the bar chart at rest and a
    /// pin while the pointer is on it. Pointing at it peeks the strip, clicking it pins the strip.
    /// Its hover box is as tall as the line it sits in, so the peek above it starts where it ends.
    fn render_native_sidebar_usage_handle(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let hovered = self.native_sidebar.usage_pin_hovered;
        let tooltip_delay = appearance.tooltip_delay;
        let (line_height, top) = self.native_sidebar_usage_handle_line();
        let in_button_line = top > 0.0;
        let height = if in_button_line {
            SIDEBAR_BOTTOM_LINE_BUTTON_HEIGHT
        } else {
            line_height
        };
        // CDXC:Sidebar 2026-09-22 WHY:
        // A GPUI element holds one hover listener, and the managed tooltip installs its own, so an
        // `on_hover` on the chip itself is silently replaced in release builds (debug asserts).
        // The peek's hover therefore lives on this wrapper, the tooltip on the chip inside it.
        div()
            .id("native-sidebar-usage-handle-hover")
            .flex()
            .flex_shrink_0()
            .h(px(height * scale))
            .map(|wrapper| {
                if in_button_line {
                    wrapper.items_center()
                } else {
                    wrapper
                        .items_end()
                        .pb(px(SIDEBAR_USAGE_HANDLE_BOTTOM_PADDING * scale))
                }
            })
            .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                if app.native_sidebar.usage_pin_hovered != *hovered {
                    app.native_sidebar.usage_pin_hovered = *hovered;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("native-sidebar-usage-handle")
                    .role(gpui::Role::Button)
                    .aria_label("Pin account usage")
                    .h(px(SIDEBAR_USAGE_HANDLE_HEIGHT * scale))
                    .w(px(SIDEBAR_USAGE_HANDLE_WIDTH * scale))
                    .rounded(px(5.0 * scale))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .bg(appearance.hover)
                    .child(titlebar_svg_icon(
                        if hovered {
                            "titlebar/pin-outline.svg"
                        } else {
                            "titlebar/chart-bar.svg"
                        },
                        11.0 * scale,
                        if hovered {
                            titlebar_active_text_color()
                        } else {
                            appearance.chrome_icon
                        },
                    ))
                    .on_click(cx.listener(|app, _, _, cx| {
                        cx.stop_propagation();
                        app.toggle_native_sidebar_usage_visible(cx);
                    }))
                    .managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::AboveLeft,
                        tooltip_delay,
                        |window, cx| {
                            super::tooltips::sidebar_style_tooltip(
                                "Pin account usage".to_string(),
                                window,
                                cx,
                            )
                        },
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn toggle_native_sidebar_usage_visible(&mut self, cx: &mut gpui::Context<Self>) {
        self.sidebar_usage_visible = !self.sidebar_usage_visible;
        // The handle and the peek leave with this change and never see the pointer go, so their
        // hovers end here; a stale one would peek the strip the next time it is hidden.
        self.native_sidebar.usage_pin_hovered = false;
        self.native_sidebar.usage_peek_hovered = false;
        self.native_sidebar.usage_frosted_hovered = false;
        self.persist_shell_layout_state();
        cx.notify();
    }
}

/// CDXC:Sidebar 2026-09-20 DECISION:
/// User: a card in one row must line up with the card above it. A card centres its glyph and its
/// number block as one group, so a short block ("5%" over "10%") pushed its glyph right of the wider
/// one ("100%" over "0rs") in the row above. Every line is padded on the left to the width of the
/// longest line in the strip, which the monospaced badge font makes exact, so every card's content
/// is the same width, every glyph sits at the same place in its column, and the numbers right-align
/// on their last character inside a card as well.
///
/// CDXC:Sidebar 2026-09-20 DECISION:
/// User: the padding is clamped to the characters a column can actually show. A Codex line like
/// "100/45%" is seven cells, and widening all eight cards to match it would have made every card
/// clip on a narrow sidebar instead of only that one; a strip that cannot align without clipping
/// stays unaligned instead.
fn align_badge_columns(
    meters: &mut [GpuiAccountUsageMeter],
    host: &GpuiAccountUsageMeterHost,
    columns: usize,
    sidebar_width: f32,
) {
    let longest = meters
        .iter()
        .flat_map(|meter| meter.badge_lines.iter())
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    if longest == 0 {
        return;
    }
    let scale = host.scale;
    let gap = SIDEBAR_USAGE_GAP * scale;
    let column_width = (sidebar_width
        - 2.0 * SIDEBAR_USAGE_INSET * scale
        - columns.saturating_sub(1) as f32 * gap)
        / columns.max(1) as f32;
    let room = column_width
        - 2.0 * host.padding_x
        - ACCOUNT_USAGE_BADGE_GLYPH_WIDTH * scale
        - ACCOUNT_USAGE_BADGE_GAP * scale;
    let cell = ACCOUNT_USAGE_BADGE_TEXT_SIZE * scale * SIDEBAR_USAGE_BADGE_ADVANCE;
    let fits = if cell > 0.0 {
        (room / cell).floor().max(1.0) as usize
    } else {
        longest
    };
    let width = longest.min(fits);
    for line in meters
        .iter_mut()
        .flat_map(|meter| meter.badge_lines.iter_mut())
    {
        if line.chars().count() < width {
            *line = format!("{line:>width$}");
        }
    }
}
