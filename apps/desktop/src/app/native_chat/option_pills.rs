use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, px, svg,
};
use serde_json::Value;
use std::{cell::Cell, rc::Rc};

fn pill(
    kind: &'static str,
    label: &str,
    values: &Value,
    open: bool,
    appearance: &ChatAppearance,
    cx: &mut Context<NativeChatView>,
    // The model pill's frame is also kept on the view, where Option+P opens the pop-up against it.
    store: Option<Rc<Cell<gpui::Bounds<gpui::Pixels>>>>,
) -> AnyElement {
    let bounds = store.unwrap_or_else(|| Rc::new(Cell::new(gpui::Bounds::default())));
    let measured = bounds.clone();
    let scale = appearance.scale;
    let loading = label.is_empty();
    let icon_only = kind == "mode";
    let title = if loading {
        format!("Reading {}…", kind)
    } else {
        format!(
            "{}: {label}",
            match kind {
                "model" => "Model",
                "mode" => "Mode",
                _ => "Options",
            }
        )
    };
    let tooltip = if !loading && kind != "mode" {
        let shortcut = crate::app::hotkeys::gpui_configured_hotkey_label("openModelPicker");
        if kind == "model" {
            match shortcut.filter(|_| merged_menu(values)) {
                Some(shortcut) => format!("Model ({shortcut})"),
                None => "Model".to_owned(),
            }
        } else {
            let tooltip = values["optionsTooltip"].as_str().unwrap_or("Options");
            match shortcut {
                Some(shortcut) => tooltip.replace("{shortcut}", &shortcut),
                None => tooltip.replace(" ({shortcut})", ""),
            }
        }
    } else {
        title.clone()
    };
    let display = if kind == "model" {
        values["modelDisplay"].as_str().unwrap_or(label)
    } else {
        label
    };
    // The merged pill: the picker's own label, then its reasoning and context window, muted.
    let merged = kind == "model" && values["modelMenu"].is_object();
    let display = values["modelMenu"]["pill"]["label"]
        .as_str()
        .filter(|_| merged)
        .unwrap_or(display);
    let suffix = values["modelMenu"]["pill"]["suffix"]
        .as_str()
        .filter(|_| merged && !loading)
        .map(str::to_owned);
    let mut item = div()
        .id(format!("chat-{kind}-picker"))
        .relative()
        .role(gpui::Role::Button)
        .aria_label(title.clone())
        .chat_cursor_pointer()
        .min_w_0()
        .max_w(px(if merged { 248.0 } else { 160.0 } * scale))
        .h(px(24.0 * scale))
        .px(px(if icon_only { 6.0 } else { 10.0 } * scale))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(4.0 * scale))
        .rounded_full()
        // React's ghost Button carried `aria-expanded:bg-muted`, so an open menu keeps its pill lit.
        .when(open, |item| item.bg(appearance.border))
        .hover(|style| style.bg(appearance.border))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        });
    if kind == "model"
        && let Some(icon) = values["agentIcon"].as_str()
    {
        let color = crate::app::helpers::workspace_tab_agent_icon_accent_color(icon);
        let color = if appearance.light && matches!(color, 0xffffff | 0xedecec) {
            0x27272a
        } else {
            color
        };
        let indicator = values["accountIndicator"]
            .as_str()
            .filter(|value| !value.is_empty());
        let logo = svg()
            .path(format!("agent-icons/{icon}.svg"))
            .text_color(gpui::rgb(color));
        /*
        React's `.gx-account-mark` in packages/core-ui/accounts/accounts.css: once the session is
        bound to an account, the pill's logo grows to 19.2px and dims to 0.3 so the account's mark
        can sit centred on it in the account monospace, 9.9px semibold, muted except on Codex.
        A session without a bound account keeps the plain 14px logo.
        */
        item = item.child(match indicator {
            Some(indicator) => div()
                .relative()
                .flex_shrink_0()
                .mr(px(2.0 * scale))
                .size(px(19.2 * scale))
                .flex()
                .items_center()
                .justify_center()
                .child(logo.absolute().size_full().opacity(0.3))
                .child(
                    div()
                        .font_family("Menlo")
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_size(px(9.9 * scale))
                        .line_height(px(9.9 * scale))
                        .text_color(if icon == "codex" {
                            gpui::Hsla::from(gpui::rgb(0x7db8fb))
                        } else {
                            appearance.muted
                        })
                        .child(indicator.to_owned()),
                ),
            None => div()
                .flex_shrink_0()
                .mr(px(2.0 * scale))
                .child(logo.size(px(14.0 * scale))),
        });
    }
    if icon_only && let Some(mode) = values["modeValue"].as_str() {
        let (icon, color) = match mode {
            "accept-edits" => ("mode-advance", 0xd3bff8),
            "auto" => ("mode-advance", 0xf6daa0),
            "bypass" => ("mode-advance", 0xffb6c5),
            "manual" => ("mode-pause", 0xd2d4dc),
            "plan" => ("mode-pause", 0xa6ddd8),
            _ => ("mode-pause", 0xd2d4dc),
        };
        let mut color = gpui::rgb(color);
        if appearance.light {
            color.r *= 0.525;
            color.g *= 0.525;
            color.b *= 0.525;
        }
        item = item.child(
            svg()
                .path(format!("titlebar/{icon}.svg"))
                .w(px(16.0 * scale))
                .h(px(14.0 * scale))
                .text_color(color)
                .opacity(0.55),
        );
    } else if !icon_only {
        item = item
            .when(!loading, |item| {
                item.child(div().min_w_0().text_ellipsis().child(display.to_owned()))
            })
            .when_some(suffix, |item, suffix| {
                item.child(
                    div()
                        .min_w_0()
                        .flex_shrink(1000.0)
                        .text_ellipsis()
                        .text_color(appearance.muted.opacity(0.8))
                        .child(suffix),
                )
            })
            .when(loading, |item| {
                item.child(
                    div()
                        .w(px(if kind == "model" { 52.0 } else { 36.0 } * scale))
                        .h(px(10.0 * scale))
                        .rounded_full()
                        .bg(appearance.primary.opacity(0.24)),
                )
            });
        if (kind == "options" || merged) && !loading {
            for (key, icon) in [("fast", "bolt"), ("plan", "map")] {
                if values[key] == true {
                    item = item.child(
                        svg()
                            .path(format!("titlebar/{icon}.svg"))
                            .size(px(12.0 * scale))
                            .text_color(appearance.primary),
                    );
                }
            }
        }
        item = item.child(
            svg()
                .path("titlebar/chevron-down.svg")
                .size(px(12.0 * scale))
                .flex_shrink_0()
                .text_color(appearance.primary),
        );
    }
    item.on_click(cx.listener(move |chat, _, window, cx| {
        if merged {
            chat.show_model_menu(bounds.get(), window, cx);
        } else if kind == "model" || !loading {
            chat.show_option_menu(kind, bounds.get(), window, cx);
        }
    }))
    .child(
        canvas(
            move |bounds, _, _| {
                measured.set(bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
    )
    .into_any_element()
}

impl NativeChatView {
    /// `optionLabels`, plus the picker's pill label when this agent has the merged model picker.
    fn option_pill_values(&self) -> Value {
        let mut values = self.snapshot["optionLabels"].clone();
        if self.snapshot["modelMenu"].is_object() && values.is_object() {
            values["modelMenu"] = serde_json::json!({"pill": self.snapshot["modelMenu"]["pill"]});
        }
        values
    }

    pub(super) fn option_pills_width(
        &self,
        appearance: &ChatAppearance,
        window: &gpui::Window,
    ) -> f32 {
        let values = &self.option_pill_values();
        let merged = values["modelMenu"].is_object();
        let scale = appearance.scale;
        let mut style = window.text_style();
        style.font_family = appearance.font.clone().into();
        let text_width = |label: &str| {
            window
                .text_system()
                .shape_line(
                    label.to_owned().into(),
                    px(13.0 * scale),
                    &[style.to_run(label.len())],
                    None,
                )
                .width
                .as_f32()
        };
        let measure = |kind: &str| {
            let label = values[if kind == "model" {
                "modelDisplay"
            } else {
                "options"
            }]
            .as_str()
            .unwrap_or_default();
            let pill = &values["modelMenu"]["pill"];
            let label = pill["label"]
                .as_str()
                .filter(|_| kind == "model")
                .unwrap_or(label);
            // The merged pill's muted suffix, with the 4px gap that sets it off from the name.
            let suffix = pill["suffix"]
                .as_str()
                .filter(|_| kind == "model" && !label.is_empty())
                .map_or(0.0, |suffix| text_width(suffix) + 4.0 * scale);
            let text_width = if label.is_empty() {
                (if kind == "model" { 52.0 } else { 36.0 }) * scale
            } else {
                text_width(label) + suffix
            };
            let agent = if kind == "model" && values["agentIcon"].is_string() {
                if values["accountIndicator"]
                    .as_str()
                    .is_some_and(|v| !v.is_empty())
                {
                    26.0
                } else {
                    20.0
                }
            } else {
                0.0
            };
            let badges = if (kind == "options" || merged) && !label.is_empty() {
                ["fast", "plan"]
                    .iter()
                    .filter(|key| values[**key] == true)
                    .count() as f32
                    * 16.0
            } else {
                0.0
            };
            (text_width + (36.0 + agent + badges) * scale)
                .min(if merged { 248.0 } else { 160.0 } * scale)
        };
        if values["showModel"] != true {
            return 0.0;
        }
        let mut width = measure("model");
        if self.snapshot["contextMeter"].is_object() {
            width += 32.0 * scale;
        }
        if values["showOptions"] == true && !merged {
            width += measure("options") + 2.0 * scale;
        }
        if self.snapshot["optionMenus"]["mode"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
        {
            width += 30.0 * scale;
        }
        width
    }

    pub(super) fn render_option_pills(
        &self,
        model: &str,
        options: &str,
        appearance: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let values = &self.option_pill_values();
        let merged = values["modelMenu"].is_object();
        let open =
            |kind: &str| self.chat_menu_is_open(super::menu_toggle::option_pill_trigger_id(kind));
        div()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .items_center()
            .gap(px(2.0 * appearance.scale))
            .text_size(px(13.0 * appearance.scale))
            .text_color(appearance.primary)
            .when(values["showModel"] == true, |item| {
                item.child(pill(
                    "model",
                    model,
                    values,
                    open("model"),
                    appearance,
                    cx,
                    Some(self.model_pill_bounds.clone()),
                ))
            })
            .when(
                values["showOptions"] == true
                    && !merged
                    && self.snapshot["composerOverflow"]["optionsOverflowed"] != true,
                |item| {
                    item.child(pill(
                        "options",
                        options,
                        values,
                        open("options"),
                        appearance,
                        cx,
                        None,
                    ))
                },
            )
            .when(
                self.snapshot["optionMenus"]["mode"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty()),
                |item| {
                    item.child(pill(
                        "mode",
                        values["mode"].as_str().unwrap_or_default(),
                        values,
                        open("mode"),
                        appearance,
                        cx,
                        None,
                    ))
                },
            )
            .when(
                self.snapshot["contextMeter"].is_object()
                    && self.snapshot["composerOverflow"]["optionsOverflowed"] != true,
                |item| item.child(self.render_context_meter(appearance, cx)),
            )
            .into_any_element()
    }
}

/// Whether the model pill opens the model pop-up, which Option+P opens too.
fn merged_menu(values: &Value) -> bool {
    values["modelMenu"].is_object()
}
