use super::window::ChatOptionMenuPanel;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};

impl ChatOptionMenuPanel {
    fn activate(
        &mut self,
        index: usize,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        if row["disabled"] == true || row["heading"] == true || row["separator"] == true {
            return;
        }
        if let Some(children) = row["children"].as_array() {
            if self.child == Some(index) && self.menu.read(cx).windows.len() > self.depth + 1 {
                // Pressing the parent row of an open submenu shuts that submenu and leaves this
                // panel up, the same toggle a chat-box trigger gives its own menu (menu_toggle.rs).
                // Hovering it again, and the right arrow, still only keep the submenu open.
                if toggle {
                    self.child = None;
                    let depth = self.depth;
                    self.menu
                        .update(cx, |menu, cx| menu.truncate(depth + 1, true, cx));
                    cx.notify();
                }
                return;
            }
            self.child = Some(index);
            let scale = self.menu.read(cx).appearance.scale;
            let m = self.menu.read(cx).metrics();
            let mut anchor = window.bounds();
            anchor.origin.y += px((m.chrome() / 2.0
                + self.heights[..index].iter().sum::<f32>()
                + index as f32 * m.gap)
                * scale)
                + self.scroll.offset().y;
            anchor.size.height = px(self.heights[index] * scale);
            let rows = children.clone();
            self.menu.update(cx, |menu, cx| {
                menu.open_panel(
                    rows,
                    anchor,
                    if children
                        .first()
                        .is_some_and(|row| row["accounts"].is_object())
                    {
                        super::accounts::ACCOUNT_PANEL_WIDTH
                    } else if children
                        .first()
                        .is_some_and(|row| row["context"]["details"].is_array())
                    {
                        320.0
                    } else {
                        256.0
                    },
                    self.depth + 1,
                    cx,
                )
            });
        } else if let Some(command) = row.get("command") {
            let command = command.clone();
            if row["keepOpen"] == true {
                // A select opened from a panel: apply the pick, close only the list.
                let depth = self.depth;
                self.menu.update(cx, |menu, cx| {
                    menu.dispatch(command, cx);
                    menu.truncate(depth, true, cx);
                });
            } else {
                self.menu
                    .update(cx, |menu, cx| menu.close(Some(command), cx));
            }
        }
    }

    fn hover(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected == Some(index) {
            return;
        }
        self.selected = Some(index);
        self.hover_task = None;
        // CDXC:SessionChat 2026-09-22 DECISION: The user asked that the Switch Account submenu not close when the pointer moves over another More Actions row. A submenu its row opens only on a press (`openOnHover: false`) therefore stays up while other rows are hovered; a press, the arrow keys, or Escape still replace or close it.
        let pinned = self
            .child
            .is_some_and(|child| child != index && self.rows[child]["openOnHover"] == false)
            && self.menu.read(cx).windows.len() > self.depth + 1;
        if pinned {
            cx.notify();
            return;
        }
        if self.rows[index]["children"].is_array() && self.rows[index]["openOnHover"] != false {
            self.hover_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(180))
                    .await;
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.selected == Some(index) {
                        this.activate(index, false, window, cx);
                    }
                });
            }));
        } else if self.child != Some(index) {
            self.child = None;
            self.menu
                .update(cx, |menu, cx| menu.truncate(self.depth + 1, false, cx));
        }
        cx.notify();
    }

    fn key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if self
            .rows
            .first()
            .is_some_and(|row| row["accounts"].is_object())
        {
            match key {
                "escape" => self.menu.update(cx, |menu, cx| menu.close(None, cx)),
                "left" if self.depth > 0 => self
                    .menu
                    .update(cx, |menu, cx| menu.truncate(self.depth, true, cx)),
                _ => return,
            }
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if let Some(context) = self.rows.first().and_then(|row| row.get("context")) {
            let has_details = context["details"].is_array();
            let compact_enabled = context["compactDisabled"] != true;
            let choices: Vec<_> = [compact_enabled.then_some(0), has_details.then_some(1)]
                .into_iter()
                .flatten()
                .collect();
            match key {
                "escape" => self.menu.update(cx, |menu, cx| menu.close(None, cx)),
                "tab" | "up" | "down" if !choices.is_empty() => {
                    let current = self
                        .selected
                        .and_then(|selected| choices.iter().position(|choice| *choice == selected));
                    let backwards =
                        key == "up" || (key == "tab" && event.keystroke.modifiers.shift);
                    let next = if backwards {
                        current
                            .map(|index| (index + choices.len() - 1) % choices.len())
                            .unwrap_or(choices.len() - 1)
                    } else {
                        current
                            .map(|index| (index + 1) % choices.len())
                            .unwrap_or(0)
                    };
                    self.selected = Some(choices[next]);
                }
                "enter" | "space" => {
                    let command = match self.selected {
                        Some(0) if compact_enabled => Some("contextCompact"),
                        Some(1) if has_details => Some("contextEdit"),
                        _ => None,
                    };
                    if let Some(command) = command {
                        self.menu.update(cx, |menu, cx| {
                            menu.close(Some(serde_json::json!({"type":command})), cx)
                        });
                    }
                }
                _ => return,
            }
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        match key {
            "escape" | "tab" => self.menu.update(cx, |menu, cx| menu.close(None, cx)),
            "left" if self.depth > 0 => self
                .menu
                .update(cx, |menu, cx| menu.truncate(self.depth, true, cx)),
            "enter" | "space" => {
                if let Some(index) = self.selected {
                    self.activate(index, true, window, cx);
                }
            }
            "right" => {
                if let Some(index) = self.selected
                    && self.rows[index]["children"].is_array()
                {
                    self.activate(index, false, window, cx);
                }
            }
            "up" | "down" | "home" | "end" => {
                self.hover_task = None;
                self.child = None;
                self.menu
                    .update(cx, |menu, cx| menu.truncate(self.depth + 1, false, cx));
                let enabled: Vec<_> = self
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| {
                        row["heading"] != true
                            && row["separator"] != true
                            && row["disabled"] != true
                    })
                    .map(|(index, _)| index)
                    .collect();
                if !enabled.is_empty() {
                    let current = self
                        .selected
                        .and_then(|selected| enabled.iter().position(|index| *index == selected));
                    let position = match key {
                        "home" => 0,
                        "end" => enabled.len() - 1,
                        "up" => current
                            .map(|i| (i + enabled.len() - 1) % enabled.len())
                            .unwrap_or(enabled.len() - 1),
                        _ => current.map(|i| (i + 1) % enabled.len()).unwrap_or(0),
                    };
                    self.selected = Some(enabled[position]);
                    self.scroll.scroll_to_item(enabled[position]);
                }
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for ChatOptionMenuPanel {
    /// CDXC:SessionChat 2026-09-13 DECISION: User: chat menus follow light mode, the root menu and both versions of the Switch Account submenu included.
    /// Every panel reads the chat's `appearance.light` here and in `accounts.rs` (`Colors::new`), because each menu is its own window outside the chat pane.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.model_menu.is_some() {
            return self.render_model_menu(window, cx);
        }
        if let Some(setting) = self.rows.first().map(|row| row["modelMenuFlyout"].clone())
            && setting.is_object()
        {
            return self.render_model_flyout(&setting, cx);
        }
        let appearance = self.menu.read(cx).appearance.clone();
        let m = self.menu.read(cx).metrics();
        let scale = appearance.scale;
        let foreground = gpui::rgb(if appearance.light { 0x292929 } else { 0xfcfcfc });
        // A wash of the menu's own ink, like `titlebar_popup_menu_hover_color` on the sidebar menus:
        // the surface follows the chrome colour (and glass), so a fixed grey can land on the surface
        // itself and show no highlight at all.
        let hover = foreground.opacity(if appearance.light { 0.06 } else { 0.08 });
        let border = gpui::rgba(if appearance.light {
            0x0000001f
        } else {
            0xffffff1f
        });
        let mut body = div()
            .id("chat-option-menu-scroll")
            .flex()
            .flex_col()
            .gap(px(m.gap * scale))
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        for (index, row) in self.rows.iter().enumerate() {
            if row["context"].is_object() {
                body = body.child(self.render_context(&row["context"], cx));
                continue;
            }
            if row["accounts"].is_object() {
                body =
                    body.child(self.render_accounts(&row["accounts"], window.bounds().origin, cx));
                continue;
            }
            if row["separator"] == true {
                body = body.child(
                    div()
                        .flex_shrink_0()
                        .h(px(m.separator * scale))
                        .px(px(4.0 * scale))
                        .py(px((m.separator - 1.0) / 2.0 * scale))
                        .child(div().h(px(1.0)).bg(border.opacity(0.7))),
                );
                continue;
            }
            let label = row["label"].as_str().unwrap_or_default().to_owned();
            let description = row["description"].as_str().map(str::to_owned);
            if row["heading"] == true {
                body = body.child(
                    div()
                        .flex_shrink_0()
                        .h(px(self.heights[index] * scale))
                        .px(px(m.row_x * scale))
                        .pt(px(4.0 * scale))
                        .pb(px(2.0 * scale))
                        .text_size(px(11.0 * scale))
                        .text_color(foreground.opacity(0.58))
                        .child(label)
                        .when_some(description, |this, description| {
                            this.child(
                                div()
                                    .mt(px(2.0 * scale))
                                    .line_height(px(16.0 * scale))
                                    .child(description),
                            )
                        }),
                );
                continue;
            }
            let disabled = row["disabled"] == true;
            let selected = self.selected == Some(index);
            let children = row["children"].is_array();
            let mut item = div()
                .id(format!("option-menu-row-{index}"))
                .role(if row.get("checked").is_some() {
                    gpui::Role::MenuItemCheckBox
                } else {
                    gpui::Role::MenuItem
                })
                .aria_label(label.clone())
                .when_some(row["checked"].as_bool(), |item, checked| {
                    item.aria_toggled(if checked {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                })
                .when(children, |item| {
                    item.aria_expanded(self.child == Some(index))
                })
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(m.row_gap * scale))
                .w_full()
                .h(px(self.heights[index] * scale))
                .px(px(m.row_x * scale))
                .py(px(m.row_y * scale))
                .rounded(px(m.row_radius * scale))
                .opacity(if disabled { 0.42 } else { 1.0 })
                .when(selected, |item| item.bg(hover))
                .on_mouse_move(cx.listener(move |this, _, window, cx| {
                    if !disabled {
                        this.hover(index, window, cx);
                    }
                }))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.activate(index, true, window, cx)),
                );
            let icon_path = row["iconPath"].as_str().map(str::to_owned).or_else(|| {
                row["icon"]
                    .as_str()
                    .map(|icon| format!("agent-icons/{icon}.svg"))
            });
            if let Some(icon_path) = icon_path {
                item = item.child(
                    svg()
                        .path(icon_path)
                        .flex_shrink_0()
                        .size(px(m.icon * scale))
                        .text_color(foreground),
                );
            }
            // A lifecycle dot instead of a glyph (the fork branch list).
            if let Some(tone) = row["dot"].as_str() {
                item = item.child(
                    div()
                        .flex_shrink_0()
                        .size(px(6.0 * scale))
                        .rounded_full()
                        .bg(super::super::fork_branches::branch_dot_color(
                            tone,
                            &appearance,
                        )),
                );
            }
            item = item.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_ellipsis().child(label))
                    .when_some(description, |this, description| {
                        this.child(
                            // One line, like React's menu rows: a long subtitle ends in an ellipsis
                            // instead of growing the row to two lines (`measure_rows` agrees).
                            div()
                                .mt(px(2.0 * scale))
                                .text_size(px(12.0 * scale))
                                .line_height(px(16.0 * scale))
                                .truncate()
                                .text_color(foreground.opacity(0.58))
                                .child(description),
                        )
                    }),
            );
            if let Some(detail) = row["detail"].as_str() {
                item = item.child(
                    div()
                        .text_size(px(12.0 * scale))
                        .text_color(foreground.opacity(0.58))
                        .child(detail.to_owned()),
                );
            }
            if children || row.get("checked").is_some() {
                item = item.child(div().flex_shrink_0().size(px(m.icon * scale)).when(
                    children || row["checked"] == true,
                    |item| {
                        item.child(
                            svg()
                                .path(if children {
                                    "titlebar/chevron-right.svg"
                                } else {
                                    "titlebar/check.svg"
                                })
                                .size(px(m.icon * scale))
                                .text_color(foreground),
                        )
                    },
                ));
            }
            body = body.child(item);
        }
        div()
            .id("chat-option-menu")
            .role(gpui::Role::Menu)
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .rounded(px(m.radius * scale))
            .border_1()
            .border_color(border)
            .bg(appearance.menu_surface())
            .p(px(m.padding * scale))
            .font_family(appearance.font)
            .text_color(foreground)
            .text_size(px(m.text * scale))
            .line_height(px(m.line * scale))
            .on_key_down(cx.listener(Self::key))
            .child(body)
            .into_any_element()
    }
}
