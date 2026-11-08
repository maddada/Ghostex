//! The Profile tab: name, role and description, the look (icon and colour), the agent and model
//! it runs on, its approval level and its department.
use super::super::native_modal_kit::*;
use super::model::*;
use super::parts::{page_header, read_only, row, section, wide_row};
use super::window::GpuiGbotSettingsModalWindow;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb, svg,
};
use gpui_component::{h_flex, v_flex};

const ICON_TILE: f32 = 36.0;
const SWATCH: f32 = 26.0;

impl GpuiGbotSettingsModalWindow {
    pub(super) fn render_profile(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let kit = self.kit;
        let who = section(
            &p,
            "About",
            vec![
                row(
                    &p,
                    "Name",
                    modal_text_input(&kit, &self.name_input, self.saving, window, cx),
                ),
                row(
                    &p,
                    "Role",
                    modal_text_input(&kit, &self.title_input, self.saving, window, cx),
                ),
                wide_row(
                    &p,
                    "Description",
                    modal_text_area(
                        &kit,
                        &self.description_input,
                        Some(76.0),
                        self.saving,
                        window,
                        cx,
                    ),
                    None,
                ),
            ],
        );
        let look = section(
            &p,
            "Look",
            vec![
                wide_row(&p, "Icon", self.render_icon_grid(cx), None),
                wide_row(&p, "Colour", self.render_swatches(cx), None),
            ],
        );
        let runs = section(
            &p,
            "Runs on",
            vec![
                row(&p, "Agent", self.render_agent_trigger(cx)),
                row(
                    &p,
                    "Model",
                    modal_text_input(&kit, &self.model_input, self.saving, window, cx),
                ),
                self.render_approval(cx),
                row(
                    &p,
                    "Department",
                    read_only(
                        &p,
                        self.draft
                            .department_id
                            .clone()
                            .unwrap_or_else(|| "None".to_string()),
                    ),
                ),
            ],
        );
        let column = v_flex()
            .w_full()
            .pr(px(20.0))
            .pb(px(8.0))
            .gap(px(24.0))
            .child(page_header(
                &p,
                "Profile",
                "Who this GBot is, how it looks in the sidebar and what it runs on.",
            ))
            .child(who)
            .child(look)
            .child(runs);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id("gbot-settings-profile-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.page_scroll)
                    .child(column),
            )
            .child(modal_edge_scrollbar(&self.page_scroll))
            .into_any_element()
    }

    /// The icon grid: the picker's icons, plus the bot's own icon first when it is another one
    /// (set from the CLI), so the current pick always shows.
    fn render_icon_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let color = rgb(gbot_color_hex(&self.draft.color));
        let mut icons: Vec<String> = GBOT_ICONS.iter().map(|icon| icon.to_string()).collect();
        if !icons.contains(&self.original.icon) {
            icons.insert(0, self.original.icon.clone());
        }
        let tiles = icons.into_iter().enumerate().map(|(index, icon)| {
            let selected = icon == self.draft.icon;
            let pick = icon.clone();
            div()
                .id(("gbot-settings-icon", index))
                .role(gpui::Role::RadioButton)
                .aria_label(SharedString::from(icon.clone()))
                .aria_toggled(a11y_toggled(selected))
                .size(px(ICON_TILE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(if selected {
                    rgba_of(color, 0.7)
                } else {
                    p.hairline
                }))
                .when(selected, |this| this.bg(hsla(rgba_of(color, 0.14))))
                .when(!selected, |this| {
                    this.cursor_pointer()
                        .hover(move |this| this.bg(hsla(p.raised_hover)))
                })
                .on_press(cx, move |this: &mut Self, _window, cx| {
                    if !this.saving {
                        this.draft.icon = pick.clone();
                        cx.notify();
                    }
                })
                .child(
                    svg()
                        .path(SharedString::from(gbot_icon_path(&icon)))
                        .size(px(18.0))
                        .text_color(hsla(if selected { color } else { p.muted })),
                )
        });
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(8.0))
            .children(tiles)
            .into_any_element()
    }

    fn render_swatches(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let swatches = GBOT_COLORS.iter().enumerate().map(|(index, (id, hex))| {
            let selected = *id == self.draft.color;
            let color = rgb(*hex);
            let id = *id;
            div()
                .id(("gbot-settings-colour", index))
                .role(gpui::Role::RadioButton)
                .aria_label(id)
                .aria_toggled(a11y_toggled(selected))
                .size(px(SWATCH + 8.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_2()
                .border_color(hsla(if selected {
                    rgba_of(p.foreground, 0.85)
                } else {
                    transparent_rgba()
                }))
                .when(!selected, |this| this.cursor_pointer())
                .on_press(cx, move |this: &mut Self, _window, cx| {
                    if !this.saving {
                        this.draft.color = id.to_string();
                        cx.notify();
                    }
                })
                .child(div().size(px(SWATCH)).rounded_full().bg(hsla(color)))
        });
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(6.0))
            .children(swatches)
            .into_any_element()
    }

    fn render_agent_trigger(&self, cx: &mut Context<Self>) -> AnyElement {
        let kit = self.kit;
        let value = self
            .agent_index()
            .and_then(|index| self.agents.get(index))
            .map(|(_, name)| name.clone());
        let trigger = modal_select_trigger(
            &kit,
            &self.agent_select,
            "gbot-settings-agent",
            value,
            "Pick an agent",
            self.saving || self.agents.is_empty(),
            |this: &mut Self, _window, cx| this.toggle_agent_select(cx),
            cx,
        );
        h_flex()
            .w_full()
            .on_children_prepainted(capture_child_bounds(
                self.agent_select.trigger_bounds.clone(),
                0,
            ))
            .child(trigger)
            .into_any_element()
    }

    /// The open agent list, drawn over the window.
    pub(super) fn render_agent_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let labels: Vec<String> = self.agents.iter().map(|(_, name)| name.clone()).collect();
        modal_select_menu(
            &self.kit,
            &self.agent_select,
            "gbot-settings-agent-menu",
            &labels,
            self.agent_index(),
            |this: &mut Self, index, _window, cx| this.choose_agent(index, cx),
            |this: &mut Self, _window, cx| {
                this.agent_select.close();
                cx.notify();
            },
            window,
            cx,
        )
    }

    fn render_approval(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let items: Vec<ModalSegmentedItem> = GBOT_APPROVALS
            .iter()
            .map(|(_, label, _)| ModalSegmentedItem { icon: None, label })
            .collect();
        let selected = GBOT_APPROVALS
            .iter()
            .position(|(id, _, _)| *id == self.draft.approval)
            .unwrap_or(0);
        let control = modal_segmented_control(
            &self.kit,
            "gbot-settings-approval",
            &items,
            selected,
            |this: &mut Self, index, _window, cx| {
                if !this.saving {
                    this.draft.approval = GBOT_APPROVALS[index].0.to_string();
                    cx.notify();
                }
            },
            cx,
        );
        wide_row(
            &p,
            "Approval",
            control,
            Some(format!("{}\n{GBOT_CODEX_APPROVAL_NOTE}", GBOT_APPROVALS[selected].2).into()),
        )
    }
}

fn transparent_rgba() -> gpui::Rgba {
    gpui::Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}
