use super::super::window::ChatOptionMenuPanel;
use super::style::{
    BAR_HEIGHT, BUTTON_GAP, CARD_RADIUS, ERROR_HEIGHT, FLYOUT_WIDTH, ITEM_RADIUS, LIST_HEIGHT,
    Palette, ROW_GAP, TRAIT_ROW_HEIGHT, button_lines,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use serde_json::{Value, json};

const HEADING_HEIGHT: f32 = 26.0;
const CHOICES_MAX_HEIGHT: f32 = 240.0;

fn choices_height(choices: usize) -> f32 {
    let choices = choices as f32;
    (choices * TRAIT_ROW_HEIGHT + (choices - 1.0).max(0.0) * ROW_GAP).min(CHOICES_MAX_HEIGHT)
}

/// The side list's height without the 14px `open_panel_at` adds for a row menu.
pub(in crate::app::native_chat::option_menu) fn flyout_height(setting: &Value) -> f32 {
    let choices = setting["choices"].as_array().map_or(0, Vec::len);
    10.0 + HEADING_HEIGHT + ROW_GAP + choices_height(choices) - 14.0
}

impl ChatOptionMenuPanel {
    fn model_flyout_is_open(&self, index: usize, cx: &Context<Self>) -> bool {
        self.model_menu
            .as_ref()
            .is_some_and(|state| state.flyout == Some(index))
            && self.menu.read(cx).windows.len() > self.depth + 1
    }

    /// A footer button with at most two values flips to the other one; a longer list opens beside the card.
    pub(super) fn activate_model_button(
        &mut self,
        index: usize,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.model_menu.as_ref() else {
            return;
        };
        let traits = state.display_traits(&self.menu.read(cx).model_efforts);
        let Some(setting) = traits.get(index) else {
            return;
        };
        if setting["disabled"] == true || state.view["disabled"] == true {
            return;
        }
        if !setting["toggle"].is_object() {
            self.open_model_flyout(index, window, cx);
            return;
        }
        let command = json!({
            "type": "modelMenuTrait",
            "id": setting["id"],
            "value": setting["toggle"]["value"],
            "exitPlan": setting["toggle"]["exitPlan"],
            "secondary": secondary,
        });
        self.model_menu_send(command, cx);
    }

    /// Opens the footer button's side list, or shuts it when it is the one already open.
    fn open_model_flyout(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let depth = self.depth;
        if self.model_flyout_is_open(index, cx) {
            if let Some(state) = self.model_menu.as_mut() {
                state.flyout = None;
            }
            self.menu
                .update(cx, |menu, cx| menu.truncate(depth + 1, true, cx));
            cx.notify();
            return;
        }
        let efforts = self.menu.read(cx).model_efforts.clone();
        let Some(state) = self.model_menu.as_mut() else {
            return;
        };
        let Some(setting) = state.display_traits(&efforts).get(index).cloned() else {
            return;
        };
        if setting["disabled"] == true
            || state.view["disabled"] == true
            || setting["choices"].as_array().is_none_or(Vec::is_empty)
        {
            return;
        }
        state.flyout = Some(index);
        state.active = state.rows().len() + index;
        let error = if state.view["error"].is_string() {
            ERROR_HEIGHT
        } else {
            0.0
        };
        let scale = self.menu.read(cx).appearance.scale;
        let line = index / button_lines(state.traits().len()).1.max(1);
        // Beside the card rather than over it, level with the button's line.
        let mut anchor = window.bounds();
        anchor.origin.y += px((1.0
            + BAR_HEIGHT
            + error
            + LIST_HEIGHT
            + 5.0
            + line as f32 * (TRAIT_ROW_HEIGHT + BUTTON_GAP))
            * scale);
        anchor.size.height = px(TRAIT_ROW_HEIGHT * scale);
        self.menu.update(cx, |menu, cx| {
            menu.open_panel(
                vec![json!({"modelMenuFlyout":setting})],
                anchor,
                FLYOUT_WIDTH,
                depth + 1,
                cx,
            )
        });
        cx.notify();
    }

    /// A choice applies and only the side list closes, so the picker stays up for the next change.
    fn choose_model_flyout(&mut self, index: usize, secondary: bool, cx: &mut Context<Self>) {
        let Some(setting) = self.rows.first().map(|row| &row["modelMenuFlyout"]) else {
            return;
        };
        let Some(choice) = setting["choices"].get(index) else {
            return;
        };
        let depth = self.depth;
        // The Reasoning list follows the highlighted model; for any model but the one in use it only
        // sets the level that model's pick will carry.
        if let (Some(row), Some(value)) = (setting["browse"].as_str(), choice["value"].as_str()) {
            let (row, value) = (row.to_owned(), value.to_owned());
            let current = setting["browseCurrent"] == true;
            self.menu.update(cx, |menu, cx| {
                menu.model_efforts.insert(row, value);
                if !current {
                    menu.truncate(depth, true, cx);
                    if let Some(handle) = menu.windows.get(depth.saturating_sub(1)).copied() {
                        cx.defer(move |cx| {
                            let _ = handle.update(cx, |_, window, _| window.refresh());
                        });
                    }
                }
            });
            if !current {
                return;
            }
        }
        let command = json!({
            "type": "modelMenuTrait",
            "id": setting["id"],
            "value": choice["value"],
            "exitPlan": choice["exitPlan"],
            "secondary": secondary,
        });
        self.menu.update(cx, |menu, cx| {
            menu.dispatch(command, cx);
            menu.truncate(depth, true, cx);
        });
    }

    fn model_flyout_cursor(&self, choices: &[Value]) -> usize {
        self.selected
            .or_else(|| choices.iter().position(|choice| choice["selected"] == true))
            .unwrap_or(0)
    }

    /// True when this panel is a side list and took the key.
    pub(in crate::app::native_chat::option_menu) fn model_flyout_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(choices) = self
            .rows
            .first()
            .and_then(|row| row["modelMenuFlyout"]["choices"].as_array())
            .cloned()
        else {
            return false;
        };
        let key = &event.keystroke;
        let cursor = self.model_flyout_cursor(&choices);
        match key.key.as_str() {
            "escape" | "left" => {
                let depth = self.depth;
                self.menu
                    .update(cx, |menu, cx| menu.truncate(depth, true, cx));
            }
            "up" | "down" | "n" | "p" if !choices.is_empty() => {
                if matches!(key.key.as_str(), "n" | "p") && !key.modifiers.control {
                    return true;
                }
                // The ends stop rather than wrap, as the picker's own list does.
                let next = if matches!(key.key.as_str(), "up" | "p") {
                    cursor.saturating_sub(1)
                } else {
                    (cursor + 1).min(choices.len() - 1)
                };
                self.selected = Some(next);
                self.scroll.scroll_to_item(next);
            }
            "enter" | "space" => self.choose_model_flyout(cursor, key.modifiers.shift, cx),
            _ => {}
        }
        true
    }

    pub(in crate::app::native_chat::option_menu) fn render_model_flyout(
        &mut self,
        setting: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let appearance = self.menu.read(cx).appearance.clone();
        let scale = appearance.scale;
        let palette = Palette::new(&appearance);
        let choices = setting["choices"].as_array().cloned().unwrap_or_default();
        let cursor = self.model_flyout_cursor(&choices);
        // Uppercase with hair spaces between the letters stands in for letter spacing, which gpui text lacks.
        let heading = setting["label"]
            .as_str()
            .unwrap_or_default()
            .to_uppercase()
            .chars()
            .map(String::from)
            .collect::<Vec<_>>()
            .join("\u{200a}");
        let mut list = div()
            .id("model-menu-flyout-choices")
            .flex()
            .flex_col()
            .gap(px(ROW_GAP * scale))
            .h(px(choices_height(choices.len()) * scale))
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        for (index, choice) in choices.iter().enumerate() {
            let label = choice["label"].as_str().unwrap_or_default().to_owned();
            list = list.child(
                div()
                    .id(("model-menu-flyout-choice", index))
                    .role(gpui::Role::MenuItemRadio)
                    .aria_label(label.clone())
                    .flex_shrink_0()
                    .h(px(TRAIT_ROW_HEIGHT * scale))
                    .px(px(8.0 * scale))
                    .flex()
                    .items_center()
                    .gap(px(10.0 * scale))
                    .rounded(px(ITEM_RADIUS * scale))
                    .when(index == cursor, |row| row.bg(palette.ink(0.11)))
                    .on_mouse_move(cx.listener(move |panel, _, _, cx| {
                        if panel.selected != Some(index) {
                            panel.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel.choose_model_flyout(index, false, cx)
                    }))
                    // Right-click applies the choice to this session only, where the agent can.
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |panel, _, _, cx| {
                            panel.choose_model_flyout(index, true, cx)
                        }),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when(choice["isDefault"] == true, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(10.0 * scale))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(palette.muted)
                                .child("Default"),
                        )
                    })
                    .when(choice["selected"] == true, |row| {
                        row.child(
                            svg()
                                .path("titlebar/check.svg")
                                .flex_shrink_0()
                                .size(px(14.0 * scale))
                                .text_color(palette.text),
                        )
                    }),
            );
        }
        div()
            .id("chat-model-menu-flyout")
            .role(gpui::Role::Menu)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .gap(px(ROW_GAP * scale))
            .rounded(px(CARD_RADIUS * scale))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .p(px(4.0 * scale))
            .overflow_hidden()
            .font_family(appearance.font)
            .text_color(palette.text)
            .text_size(px(13.0 * scale))
            .on_key_down(cx.listener(|panel, event, window, cx| {
                if panel.model_flyout_key(event, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(HEADING_HEIGHT * scale))
                    .px(px(8.0 * scale))
                    .pt(px(8.0 * scale))
                    .text_size(px(10.0 * scale))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(palette.muted)
                    .child(heading),
            )
            .child(list)
            .into_any_element()
    }
}
