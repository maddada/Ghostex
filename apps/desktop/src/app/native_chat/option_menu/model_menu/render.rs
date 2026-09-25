use super::super::window::ChatOptionMenuPanel;
use super::style::{
    BAR_HEIGHT, BUTTON_GAP, CARD_RADIUS, ERROR_HEIGHT, HINTS_HEIGHT, ITEM_RADIUS, LIST_HEIGHT,
    Palette, ROW_GAP, TRAIT_ROW_HEIGHT, button_lines,
};
use crate::app::native_chat::appearance::ChatAppearance;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg, uniform_list,
};
use serde_json::{Value, json};

/// An agent's logo in its brand tone; a white mark turns dark on the light surface.
fn agent_logo(icon: &str, appearance: &ChatAppearance) -> gpui::Svg {
    let color = crate::app::helpers::workspace_tab_agent_icon_accent_color(icon);
    let color = if appearance.light && matches!(color, 0xffffff | 0xedecec) {
        0x27272a
    } else {
        color
    };
    svg()
        .path(format!("agent-icons/{icon}.svg"))
        .flex_shrink_0()
        .text_color(gpui::rgb(color))
}

impl ChatOptionMenuPanel {
    fn render_model_tabs(
        &self,
        view: &Value,
        appearance: &ChatAppearance,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let mut bar = div()
            .flex_shrink_0()
            .h(px(BAR_HEIGHT * scale))
            .px(px(4.0 * scale))
            .border_b_1()
            .border_color(palette.ink(0.08))
            .flex()
            .items_center()
            .gap(px(ROW_GAP * scale));
        for (index, tab) in view["tabs"].as_array().into_iter().flatten().enumerate() {
            let active = tab["active"] == true;
            let id = tab["id"].clone();
            let name = tab["name"].as_str().unwrap_or_default().to_owned();
            // CDXC:SessionChat 2026-09-24 DECISION:
            // User: show a handoff icon where switching to another agent means another agent CLI, but "don't show the icon for handover so much, just show it on the agent icons up top": another agent's tab carries a small badge and the model rows carry none. A draft switches agents without a handoff, so it shows none.
            let handoff = tab["handoff"] == true;
            let tooltip = if handoff {
                format!("{name}: picking a model hands off to {name}")
            } else {
                name.clone()
            };
            let hover = palette.ink(0.06);
            let icon = match tab["icon"].as_str() {
                Some(icon) => agent_logo(icon, appearance).size(px(16.0 * scale)),
                None => svg()
                    .path("titlebar/star-filled.svg")
                    .size(px(15.0 * scale))
                    .text_color(if active { palette.text } else { palette.muted }),
            };
            bar = bar.child(
                div()
                    .id(("model-menu-tab", index))
                    .role(gpui::Role::Tab)
                    .aria_label(name)
                    .relative()
                    .size(px(32.0 * scale))
                    .rounded(px(ITEM_RADIUS * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!active, |tab| tab.hover(move |style| style.bg(hover)))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel.model_menu_send(json!({"type":"modelMenuView","tab":id}), cx);
                    }))
                    .child(icon.opacity(if active { 1.0 } else { 0.72 }))
                    .when(handoff, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .right(px(3.0 * scale))
                                .bottom(px(3.0 * scale))
                                .size(px(12.0 * scale))
                                .rounded_full()
                                .bg(palette.surface)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    svg()
                                        .path("titlebar/switch-horizontal.svg")
                                        .size(px(9.0 * scale))
                                        .text_color(palette.muted),
                                ),
                        )
                    })
                    // The 32px tab sits centred in the 40px bar, so 4px below it is the bar's own hairline.
                    .when(active, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .bottom(px(-4.0 * scale))
                                .left(px(6.0 * scale))
                                .right(px(6.0 * scale))
                                .h(px(2.0 * scale))
                                .rounded(px(1.0 * scale))
                                .bg(palette.accent),
                        )
                    }),
            );
        }
        bar.into_any_element()
    }

    fn render_model_row(
        &self,
        index: usize,
        row: &Value,
        active: bool,
        appearance: &ChatAppearance,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let label = row["label"].as_str().unwrap_or_default().to_owned();
        let description = row["description"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
        let selected = row["selected"] == true;
        let favorite = row["favorite"] == true;
        let key = row["key"].clone();
        let name = div()
            .flex_shrink_0()
            .max_w_full()
            .truncate()
            .text_size(px(12.5 * scale))
            .font_weight(gpui::FontWeight::MEDIUM)
            .child(label.clone());
        // Favorites mix agents, so each row leads with its agent's logo on the model's one line.
        let body = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * scale))
            .when(row["showAgent"] == true, |body| {
                body.child(
                    agent_logo(row["icon"].as_str().unwrap_or_default(), appearance)
                        .size(px(14.0 * scale)),
                )
            })
            .child(name);
        // CDXC:SessionChat 2026-09-24 DECISION:
        // User: a model's description is not written next to it in the picker; an info-circle icon that appears when the row is hovered shows it on hover instead (replaces the eye of 2026-09-22).
        const TOOLTIP_WIDTH: f32 = 220.0;
        let about = description.map(|description| {
            let about_hover = palette.ink(0.08);
            div()
                .id(("model-menu-about", index))
                .role(gpui::Role::Button)
                .aria_label(format!("About {label}"))
                .flex_shrink_0()
                .size(px(22.0 * scale))
                .rounded(px(ITEM_RADIUS * scale))
                .flex()
                .items_center()
                .justify_center()
                .opacity(if active { 1.0 } else { 0.0 })
                .hover(move |style| style.bg(about_hover))
                .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                // The picker is its own small window and a tooltip cannot draw past it, so the text wraps
                // at a width that fits inside the card instead of running off its edge on one line.
                .tooltip(move |window, cx| {
                    let description = description.clone();
                    gpui_component::tooltip::Tooltip::element(move |_, _| {
                        div()
                            .w(px(TOOLTIP_WIDTH * scale))
                            .whitespace_normal()
                            .child(description.clone())
                    })
                    .build(window, cx)
                })
                .child(
                    svg()
                        .path("titlebar/info-circle.svg")
                        .size(px(13.0 * scale))
                        .text_color(palette.muted),
                )
        });
        let star_hover = palette.ink(0.08);
        let item = div()
            .id(("model-menu-row", index))
            .role(gpui::Role::MenuItemRadio)
            .aria_label(label)
            .px(px(8.0 * scale))
            .py(px(4.0 * scale))
            .rounded(px(ITEM_RADIUS * scale))
            .flex()
            .items_center()
            .gap(px(10.0 * scale))
            // The selected row's ring is a border every row reserves, so rows keep one height.
            .border_1()
            .border_color(if selected {
                palette.ink(0.09)
            } else {
                gpui::transparent_black()
            })
            .when(selected, |item| item.bg(palette.ink(0.11)))
            .when(active && !selected, |item| item.bg(palette.ink(0.05)))
            // Hover moves the keyboard cursor instead of painting its own wash, so two rows never look lit.
            .on_mouse_move(cx.listener(move |panel, _, _, cx| {
                if let Some(state) = panel.model_menu.as_mut()
                    && state.active != index
                {
                    state.active = index;
                    cx.notify();
                }
            }))
            // A click keeps its old meaning; it carries a level only when the keyboard or the Reasoning list moved one.
            .on_click(cx.listener(move |panel, _, _, cx| {
                let effort = panel.model_menu_browsed_effort(index, cx);
                panel.model_menu_pick(index, false, effort, cx);
            }))
            // Right-click applies the model to this session only, where the agent can.
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |panel, _, _, cx| {
                    let effort = panel.model_menu_browsed_effort(index, cx);
                    panel.model_menu_pick(index, true, effort, cx);
                }),
            )
            .child(body)
            .children(about)
            .when_some(row["shortcut"].as_u64(), |item, slot| {
                item.child(
                    div()
                        .flex_shrink_0()
                        .px(px(5.0 * scale))
                        .py(px(1.0 * scale))
                        .rounded(px(5.0 * scale))
                        .bg(palette.ink(0.05))
                        .font_family("Menlo")
                        .text_size(px(10.0 * scale))
                        .text_color(palette.muted)
                        .child(format!("⌘{slot}")),
                )
            })
            .child(
                div()
                    .id(("model-menu-star", index))
                    .role(gpui::Role::Button)
                    .aria_label(if favorite { "Remove star" } else { "Star" })
                    .flex_shrink_0()
                    .size(px(22.0 * scale))
                    .rounded(px(ITEM_RADIUS * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(move |style| style.bg(star_hover))
                    .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        cx.stop_propagation();
                        panel.model_menu_send(json!({"type":"modelMenuFavorite","key":key}), cx);
                    }))
                    .child(
                        svg()
                            .path(if favorite {
                                "titlebar/star-filled.svg"
                            } else {
                                "titlebar/star.svg"
                            })
                            .size(px(13.0 * scale))
                            .text_color(if favorite {
                                palette.star
                            } else {
                                palette.muted
                            }),
                    ),
            );
        // The list measures one item for all of them, so the gap between rows is each item's own padding.
        div().pb(px(ROW_GAP * scale)).child(item).into_any_element()
    }

    fn render_model_traits(
        &self,
        view: &Value,
        traits: &[Value],
        active: usize,
        shake: f32,
        appearance: &ChatAppearance,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let scale = appearance.scale;
        if traits.is_empty() {
            return None;
        }
        let rows = view["rows"].as_array().map_or(0, Vec::len);
        let open = self
            .model_menu
            .as_ref()
            .and_then(|state| state.flyout)
            .filter(|_| self.menu.read(cx).windows.len() > self.depth + 1);
        let mut tray = div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(palette.ink(0.08))
            .p(px(4.0 * scale))
            .flex()
            .flex_col()
            .gap(px(BUTTON_GAP * scale));
        let per_line = button_lines(traits.len()).1.max(1);
        for (line, settings) in traits.chunks(per_line).enumerate() {
            let mut buttons = div().flex().gap(px(BUTTON_GAP * scale));
            for (offset, setting) in settings.iter().enumerate() {
                let index = line * per_line + offset;
                let disabled = setting["disabled"] == true || view["disabled"] == true;
                let label = setting["label"].as_str().unwrap_or_default().to_owned();
                let value = setting["valueLabel"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let icon = match setting["icon"].as_str() {
                    Some("reasoning") => Some("titlebar/brain.svg"),
                    Some("context") => Some("titlebar/chart-bar.svg"),
                    Some("fast") => Some("titlebar/bolt.svg"),
                    Some("account") => Some("titlebar/user.svg"),
                    _ => None,
                };
                let letter = setting["icon"]
                    .as_str()
                    .and_then(super::keys::button_letter);
                let shaking = setting["id"] == "effort" && shake != 0.0;
                // Fast mode reads as a switch: lit in the pill's marker tone when on, dimmed when off.
                let fast = setting["icon"] == "fast";
                let on = fast
                    && setting["choices"].as_array().is_some_and(|choices| {
                        choices
                            .iter()
                            .any(|choice| choice["selected"] == true && choice["label"] == "On")
                    });
                let tone = if on {
                    palette.on
                } else if fast {
                    palette.muted
                } else {
                    palette.text
                };
                let lit = open == Some(index) || active == rows + index;
                let tooltip = if value.is_empty() {
                    label.clone()
                } else {
                    format!("{label}: {value}")
                };
                let tooltip = match letter {
                    Some(letter) => format!("{tooltip} ({letter})"),
                    None => tooltip,
                };
                let tooltip = match setting["icon"].as_str() {
                    Some("fast") => format!("{tooltip} (F)"),
                    Some("context") => format!("{tooltip} (C)"),
                    _ => tooltip,
                };
                // CDXC:SessionChat 2026-09-24 DECISION:
                // User: the footer's values ("Default", "Medium") must not be cut short. Reasoning, Context Window and Fast keep their full width and grow into the spare room; only the Account button and labelled option buttons give way and truncate.
                let keeps_width = matches!(
                    setting["icon"].as_str(),
                    Some("reasoning" | "context" | "fast")
                );
                buttons = buttons.child(
                    div()
                        .id(("model-menu-setting", index))
                        .role(gpui::Role::MenuItem)
                        .aria_label(tooltip.clone())
                        .aria_expanded(open == Some(index))
                        .flex_auto()
                        .map(|button| {
                            if keeps_width {
                                button.flex_shrink_0()
                            } else {
                                button.min_w_0()
                            }
                        })
                        .h(px(TRAIT_ROW_HEIGHT * scale))
                        .px(px(6.0 * scale))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(6.0 * scale))
                        .rounded(px(ITEM_RADIUS * scale))
                        .text_size(px(12.0 * scale))
                        .opacity(if disabled { 0.42 } else { 1.0 })
                        .when(lit, |button| button.bg(palette.ink(0.11)))
                        .when(shaking, |button| button.relative().left(px(shake * scale)))
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                        .on_mouse_move(cx.listener(move |panel, _, _, cx| {
                            if let Some(state) = panel.model_menu.as_mut()
                                && state.active != rows + index
                            {
                                state.active = rows + index;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |panel, _, window, cx| {
                            panel.activate_model_button(index, false, window, cx)
                        }))
                        // Right-click applies the change to this session only, where the agent can.
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |panel, _, window, cx| {
                                panel.activate_model_button(index, true, window, cx)
                            }),
                        )
                        .map(|button| match icon {
                            Some(path) => button.child(
                                div()
                                    .relative()
                                    .flex_shrink_0()
                                    .child(
                                        svg()
                                            .path(path)
                                            .size(px(14.0 * scale))
                                            .text_color(if on { palette.on } else { palette.muted })
                                            .when(fast && !on, |icon| icon.opacity(0.6)),
                                    )
                                    // The hotkey floats off the icon's bottom-right corner as a letter in a filled circle.
                                    .when_some(letter, |icon, letter| {
                                        icon.child(
                                            div()
                                                .absolute()
                                                .right(px(-5.0 * scale))
                                                .bottom(px(-4.0 * scale))
                                                .size(px(10.0 * scale))
                                                .rounded_full()
                                                .bg(palette.muted)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(7.0 * scale))
                                                .line_height(px(10.0 * scale))
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(palette.surface)
                                                .child(letter),
                                        )
                                    }),
                            ),
                            None => button.child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(palette.muted)
                                    .child(label.clone()),
                            ),
                        })
                        .child(div().min_w_0().truncate().text_color(tone).child(value)),
                );
            }
            tray = tray.child(buttons);
        }
        Some(tray.into_any_element())
    }

    pub(in crate::app::native_chat::option_menu) fn render_model_menu(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let appearance = self.menu.read(cx).appearance.clone();
        let scale = appearance.scale;
        let palette = Palette::new(&appearance);
        let Some(state) = self.model_menu.as_ref() else {
            return div().into_any_element();
        };
        let view = state.view.clone();
        let active = state.active;
        let traits = state.display_traits(&self.menu.read(cx).model_efforts);
        let shake = state
            .shake_at
            .map_or(0.0, |at| shake_offset(at.elapsed().as_secs_f32()));
        if shake != 0.0 {
            window.request_animation_frame();
        }
        let scroll = state.scroll.clone();
        let count = state.rows().len();
        let list = if count == 0 {
            div()
                .px(px(8.0 * scale))
                .py(px(24.0 * scale))
                .text_size(px(12.0 * scale))
                .text_color(palette.muted)
                .text_center()
                .child(view["emptyText"].as_str().unwrap_or_default().to_owned())
                .into_any_element()
        } else {
            uniform_list(
                "model-menu-rows",
                count,
                cx.processor(move |panel, range: std::ops::Range<usize>, _, cx| {
                    let appearance = panel.menu.read(cx).appearance.clone();
                    let palette = Palette::new(&appearance);
                    let Some(state) = panel.model_menu.as_ref() else {
                        return Vec::new();
                    };
                    let active = state.active;
                    let rows: Vec<(usize, Value)> = range
                        .filter_map(|index| Some((index, state.rows().get(index)?.clone())))
                        .collect();
                    rows.iter()
                        .map(|(index, row)| {
                            panel.render_model_row(
                                *index,
                                row,
                                *index == active,
                                &appearance,
                                &palette,
                                cx,
                            )
                        })
                        .collect()
                }),
            )
            .size_full()
            .px(px(4.0 * scale))
            .track_scroll(&scroll)
            .into_any_element()
        };
        div()
            .id("chat-model-menu")
            .key_context(super::keys::KEY_CONTEXT)
            .track_focus(&self.focus)
            .role(gpui::Role::Menu)
            .capture_action(cx.listener(Self::model_menu_key_action))
            .size_full()
            .flex()
            .flex_col()
            .rounded(px(CARD_RADIUS * scale))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            .font_family(appearance.font.clone())
            .text_color(palette.text)
            .text_size(px(13.0 * scale))
            .child(self.render_model_tabs(&view, &appearance, &palette, cx))
            .when_some(view["error"].as_str(), |card, error| {
                // A choice the agent's own list could not offer is said here, where it was made.
                card.child(
                    div()
                        .flex_shrink_0()
                        .h(px(ERROR_HEIGHT * scale))
                        .px(px(12.0 * scale))
                        .py(px(8.0 * scale))
                        .border_b_1()
                        .border_color(palette.ink(0.08))
                        .overflow_hidden()
                        .text_size(px(11.0 * scale))
                        .text_color(palette.muted)
                        .child("Not applied")
                        .child(
                            div()
                                .mt(px(2.0 * scale))
                                .text_size(px(12.0 * scale))
                                .line_height(px(16.0 * scale))
                                .line_clamp(2)
                                .text_color(palette.text)
                                .child(error.to_owned()),
                        ),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(LIST_HEIGHT * scale))
                    .py(px(4.0 * scale))
                    .bg(palette.ink(0.02))
                    // Picks wait while the agent cannot take one; the rows say so by dimming.
                    .opacity(if view["disabled"] == true { 0.5 } else { 1.0 })
                    .child(list),
            )
            .children(self.render_model_traits(
                &view,
                &traits,
                active,
                shake,
                &appearance,
                &palette,
                cx,
            ))
            .child(render_key_hints(&appearance, &palette))
            .into_any_element()
    }
}

/// The compact key reminder along the card's bottom edge.
fn render_key_hints(appearance: &ChatAppearance, palette: &Palette) -> AnyElement {
    const HINTS: [(&str, &str); 5] = [
        ("↑↓", "model"),
        ("←→", "effort"),
        ("⇥", "agent"),
        ("⏎", "save"),
        ("esc", "close"),
    ];
    let scale = appearance.scale;
    div()
        .flex_shrink_0()
        .h(px(HINTS_HEIGHT * scale))
        .px(px(10.0 * scale))
        .border_t_1()
        .border_color(palette.ink(0.08))
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(10.5 * scale))
        .text_color(palette.muted)
        .children(HINTS.map(|(keys, label)| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0 * scale))
                .child(div().text_color(palette.text).child(keys))
                .child(label)
        }))
        .into_any_element()
}

/// The Reasoning button's shake when Left or Right can go no further: 3px left, 3px right, back,
/// over 220ms (`ghostex-chat-model-menu-shake` in session-chat-model-menu.css). Zero once it is over.
fn shake_offset(seconds: f32) -> f32 {
    const DURATION: f32 = 0.22;
    if !(0.0..DURATION).contains(&seconds) {
        return 0.0;
    }
    let t = seconds / DURATION;
    let offset = if t < 0.3 {
        -3.0 * t / 0.3
    } else if t < 0.6 {
        -3.0 + 6.0 * (t - 0.3) / 0.3
    } else {
        3.0 * (1.0 - t) / 0.4
    };
    // Zero is "not shaking", so the last frames settle at a hair off it instead.
    if offset == 0.0 { 0.01 } else { offset }
}
