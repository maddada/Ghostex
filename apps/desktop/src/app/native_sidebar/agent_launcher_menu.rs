use super::menu_state::SidebarMenuPanel;
use crate::app::window::native_modal_kit::{MODAL_MONO_FONT, css_mix, dark_theme_text_colors};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Bounds, BoxShadow, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, StatefulInteractiveElement, Styled, div, img, point, px, rgb,
    svg,
};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;

/// `GROUP_AGENT_MENU_WIDTH_PX` in the deleted React sidebar's packages/core-ui/session-group-section.tsx (git history).
pub(crate) const AGENT_LAUNCHER_MENU_WIDTH: f32 = 220.0;
const PANEL_PADDING: f32 = 6.0;
const ROW_GAP: f32 = 2.0;
/// 13px menu text at the panel's 1.4 line height.
const LINE_HEIGHT: f32 = 13.0 * 1.4;
const ROW_PADDING_Y: f32 = 8.0;
const ROW_HEIGHT: f32 = ROW_PADDING_Y * 2.0 + LINE_HEIGHT;
const USAGE_LINE_HEIGHT: f32 = 10.5 * 1.2;
const ACCOUNT_ROW_HEIGHT: f32 = ROW_HEIGHT + 2.0 + USAGE_LINE_HEIGHT;
const SEPARATOR_HEIGHT: f32 = 13.0;
const HINT_LINE_HEIGHT: f32 = 11.0 * 1.5;
/// Average 11px glyph width, used only to guess hint wrapping before the first measurement.
const HINT_CHARACTER_WIDTH: f32 = 5.5;

enum LauncherRow {
    Separator,
    Hint,
    /// Account rows always carry `detail`; an empty one means the account has no usage line.
    Account {
        usage: bool,
    },
    Plain,
}

impl LauncherRow {
    fn of(item: &Value) -> Self {
        if item["separator"] == true {
            Self::Separator
        } else if item["disabled"] == true && item.get("command").is_none() {
            Self::Hint
        } else if let Some(detail) = item.get("detail") {
            Self::Account {
                usage: detail.as_str().is_some_and(|detail| !detail.is_empty()),
            }
        } else {
            Self::Plain
        }
    }
}

/// Border-box height of `items` in a launcher panel `width` wide, before the rows are measured.
pub(crate) fn estimated_height(items: &[Value], width: Pixels, scale: f32) -> Pixels {
    let text_width = (f32::from(width) / scale - 2.0 - PANEL_PADDING * 2.0 - 20.0).max(1.0);
    let rows: f32 = items
        .iter()
        .map(|item| match LauncherRow::of(item) {
            LauncherRow::Separator => SEPARATOR_HEIGHT,
            LauncherRow::Hint => {
                let characters = item["label"].as_str().unwrap_or_default().chars().count();
                let lines = (characters as f32 * HINT_CHARACTER_WIDTH / text_width)
                    .ceil()
                    .max(1.0);
                ROW_PADDING_Y * 2.0 + lines * HINT_LINE_HEIGHT
            }
            LauncherRow::Account { usage: true } => ACCOUNT_ROW_HEIGHT,
            LauncherRow::Account { usage: false } | LauncherRow::Plain => ROW_HEIGHT,
        })
        .sum();
    let gaps = ROW_GAP * items.len().saturating_sub(1) as f32;
    px((rows + gaps + PANEL_PADDING * 2.0) * scale + 2.0)
}

/// The React launcher's `--app-foreground` / `--app-muted` pair for the sidebar theme.
#[derive(Clone, Copy)]
struct LauncherPalette {
    light: bool,
    foreground: Hsla,
    muted: Hsla,
    /// `.group-agent-menu-label`: 86% foreground, 14% muted.
    label: Hsla,
    /// `.group-agent-launcher-icon`, the code mark of an agent without a logo: 62% foreground, 38% muted.
    agent_mark: Hsla,
    hover: Hsla,
}

impl LauncherPalette {
    fn resolve(settings: Option<&serde_json::Map<String, Value>>) -> Self {
        let light = settings.is_some_and(sidebar_uses_light_theme);
        let (foreground, muted) = if light {
            (0x262626, 0x717171)
        } else {
            let (foreground, muted, _) = dark_theme_text_colors(
                settings
                    .and_then(|settings| settings.get("sidebarTheme"))
                    .and_then(Value::as_str),
            );
            (foreground, muted)
        };
        Self {
            light,
            foreground: rgb(foreground).into(),
            muted: rgb(muted).into(),
            label: css_mix(rgb(foreground), 0.86, rgb(muted)).into(),
            agent_mark: css_mix(rgb(foreground), 0.62, rgb(muted)).into(),
            hover: titlebar_popup_menu_hover_color(),
        }
    }
}

impl GhostexGpuiApp {
    /// CDXC:AgentLauncher 2026-09-19 DECISION:
    /// User: the GPUI project header Select Agent menu and its Select Account page must look just like the React sidebar's.
    /// Mirrors `AgentLauncherMenuItems` in the deleted React sidebar's packages/core-ui/accounts/agent-launcher-menu.tsx (git history) with its groups.css, accounts.css, and app-menu-panel.css rules: a 220px panel right-aligned under the chevron, 2px row gaps, the accounts button before the chat mark, the last-used agent highlighted, and two-line account rows with a monospace usage line.
    pub(super) fn render_agent_launcher_menu_panel(
        &self,
        panel_index: usize,
        panel: &SidebarMenuPanel,
        bounds: Bounds<Pixels>,
        relative: Point<Pixels>,
        scale: f32,
        view: gpui::Entity<Self>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let palette = LauncherPalette::resolve(
            self.native_sidebar
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.hud["settings"].as_object()),
        );
        let scaled = |value: f32| px(value * scale);
        let frosted = crate::app::window::frosted_host::frosted_hosting_active();
        let mut content = v_flex()
            .on_children_prepainted(super::menus::measure_menu_panel(
                view,
                panel_index,
                scaled(PANEL_PADDING * 2.0) + px(2.0),
            ))
            .id(format!("native-sidebar-menu-panel-{panel_index}"))
            .absolute()
            .left(relative.x)
            .top(relative.y)
            .w(bounds.size.width)
            .max_h(bounds.size.height)
            .overflow_y_scroll()
            .track_scroll(&panel.scroll)
            .occlude()
            .p(scaled(PANEL_PADDING))
            .gap(scaled(ROW_GAP))
            .rounded(scaled(8.0))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(if frosted {
                popup_window_surface(titlebar_popup_menu_background())
            } else {
                titlebar_popup_menu_background()
            })
            // In the frosted host the panel sits on its window's blur, which has no room for a shadow.
            .shadow(if frosted {
                Vec::new()
            } else {
                vec![
                    BoxShadow {
                        color: gpui::hsla(0.0, 0.0, 0.0, 0.32),
                        offset: point(px(0.0), scaled(14.0)),
                        blur_radius: scaled(28.0),
                        spread_radius: px(0.0),
                        inset: false,
                    },
                    BoxShadow {
                        color: gpui::hsla(0.0, 0.0, 1.0, 0.04),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(1.0),
                        inset: false,
                    },
                ]
            })
            .text_color(palette.foreground)
            .font_weight(FontWeight::NORMAL)
            .text_size(scaled(13.0))
            .line_height(scaled(LINE_HEIGHT))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());
        for (item_index, item) in panel.items.iter().enumerate() {
            let label = item["label"].as_str().unwrap_or_default().to_owned();
            let row = match LauncherRow::of(item) {
                LauncherRow::Separator => div()
                    .w_full()
                    .flex_shrink_0()
                    .py(scaled(6.0))
                    .px(scaled(4.0))
                    .child(
                        div()
                            .h(px(1.0))
                            .w_full()
                            .bg(titlebar_popup_menu_border_color().opacity(0.7)),
                    )
                    .into_any_element(),
                LauncherRow::Hint => div()
                    .w_full()
                    .flex_shrink_0()
                    .px(scaled(10.0))
                    .py(scaled(ROW_PADDING_Y))
                    .text_size(scaled(11.0))
                    .line_height(scaled(HINT_LINE_HEIGHT))
                    .text_color(palette.muted)
                    .child(label)
                    .into_any_element(),
                LauncherRow::Account { .. } | LauncherRow::Plain => self.render_agent_launcher_row(
                    panel_index,
                    item_index,
                    item,
                    label,
                    panel.selected == Some(item_index),
                    palette,
                    scale,
                    cx,
                ),
            };
            content = content.child(row);
        }
        content.into_any_element()
    }

    fn render_agent_launcher_row(
        &self,
        panel_index: usize,
        item_index: usize,
        item: &Value,
        label: String,
        selected: bool,
        palette: LauncherPalette,
        scale: f32,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scaled = |value: f32| px(value * scale);
        let disabled = item["disabled"] == true;
        let primary = item["primary"] == true;
        let account = matches!(LauncherRow::of(item), LauncherRow::Account { .. });
        let icon = if let Some(image) = item["imageDataUrl"].as_str().and_then(|value| {
            super::images::agent_image(value, item["agentIcon"].as_str(), palette.light)
        }) {
            Some(
                img(image)
                    .size(scaled(if account { 16.0 } else { 14.0 }))
                    .flex_shrink_0()
                    .into_any_element(),
            )
        } else {
            item["icon"].as_str().map(|icon| {
                // The React Tabler strokes: the titlebar copies of these two are drawn heavier.
                let (path, color) = match icon {
                    "chevron-left" => (
                        gpui::SharedString::new_static("modals/new-thread-picker/chevron-left.svg"),
                        palette.foreground,
                    ),
                    "code" => (
                        gpui::SharedString::new_static("modals/new-thread-picker/code.svg"),
                        palette.agent_mark,
                    ),
                    icon => (
                        gpui_sidebar_command_icon_asset_path(Some(icon)),
                        palette.foreground,
                    ),
                };
                svg()
                    .path(path)
                    .size(scaled(14.0))
                    .flex_shrink_0()
                    .text_color(color)
                    .into_any_element()
            })
        };
        let name = div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_color(if primary {
                palette.foreground
            } else {
                palette.label
            })
            .when(primary, |name| name.font_weight(FontWeight::SEMIBOLD))
            .child(label);
        let body = if account {
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(scaled(2.0))
                .child(h_flex().min_w_0().gap(scaled(6.0)).child(name).when_some(
                    item["suffix"].as_str(),
                    |heading, suffix| {
                        heading.child(
                            div()
                                .flex_shrink_0()
                                .text_size(scaled(11.0))
                                .text_color(palette.muted)
                                .child(suffix.to_owned()),
                        )
                    },
                ))
                .when_some(
                    item["detail"].as_str().filter(|detail| !detail.is_empty()),
                    |copy, usage| {
                        copy.child(
                            div()
                                .truncate()
                                .font_family(MODAL_MONO_FONT)
                                .text_size(scaled(10.5))
                                .line_height(scaled(USAGE_LINE_HEIGHT))
                                .text_color(palette.muted)
                                .child(usage.to_owned()),
                        )
                    },
                )
                .into_any_element()
        } else {
            name.into_any_element()
        };
        let launch = h_flex()
            .id(format!(
                "native-agent-launcher-row-{panel_index}-{item_index}"
            ))
            .flex_1()
            .min_w_0()
            .px(scaled(10.0))
            .py(scaled(ROW_PADDING_Y))
            .gap(scaled(8.0))
            .children(icon)
            .child(body)
            .when(!disabled, |launch| {
                launch
                    // The row highlights only while the launch area is hovered, not the accounts button.
                    .on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
                        let Some(panel) = app
                            .native_sidebar
                            .menu
                            .as_mut()
                            .and_then(|menu| menu.panels.get_mut(panel_index))
                        else {
                            return;
                        };
                        if *hovered {
                            panel.selected = Some(item_index);
                        } else if panel.selected == Some(item_index) {
                            panel.selected = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.activate_native_sidebar_menu_item(
                            panel_index,
                            item_index,
                            Bounds::default(),
                            true,
                            window,
                            cx,
                        );
                    }))
            });
        h_flex()
            .w_full()
            .flex_shrink_0()
            .rounded(scaled(6.0))
            .when(disabled, |row| row.opacity(0.42))
            .when(!disabled && (selected || primary), |row| {
                row.bg(palette.hover)
            })
            .child(launch)
            .when_some(item.get("secondary"), |row, secondary| {
                let count = secondary["label"].as_str().unwrap_or_default().to_owned();
                row.child(
                    h_flex()
                        .id(format!(
                            "native-agent-launcher-accounts-{panel_index}-{item_index}"
                        ))
                        .flex_shrink_0()
                        .min_w(scaled(28.0))
                        .justify_center()
                        .gap(scaled(8.0))
                        // The React hover outline is inset 1px, so the border takes 1px of the 5px/4px padding.
                        .py(scaled(5.0) - px(1.0))
                        .px(scaled(4.0) - px(1.0))
                        .border_1()
                        .border_color(gpui::transparent_black())
                        .rounded(scaled(5.0))
                        .opacity(0.58)
                        .hover(|button| {
                            button
                                .bg(palette.hover)
                                .border_color(palette.foreground.opacity(0.18))
                                .opacity(0.9)
                        })
                        .child(
                            svg()
                                .path("titlebar/user.svg")
                                .size(scaled(14.0))
                                .flex_shrink_0()
                                .text_color(palette.foreground),
                        )
                        .when(!count.is_empty(), |button| {
                            button.child(
                                div()
                                    .text_size(scaled(12.0))
                                    .line_height(scaled(12.0))
                                    .child(count),
                            )
                        })
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.activate_native_sidebar_menu_secondary(panel_index, item_index, cx);
                        })),
                )
            })
            .when(item["supportsChat"] == true, |row| {
                row.child(
                    div().flex_shrink_0().ml(scaled(6.0)).mr(scaled(5.0)).child(
                        svg()
                            .path("modals/new-thread-picker/message-circle.svg")
                            .size(scaled(14.0))
                            .text_color(palette.muted.opacity(0.58)),
                    ),
                )
            })
            .into_any_element()
    }
}
