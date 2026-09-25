use super::{appearance::SidebarAppearance, model::NativeSidebarSnapshot};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::h_flex;
use serde_json::json;

impl GhostexGpuiApp {
    pub(crate) fn render_native_machine_tabs(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if snapshot.machines.len() <= 1 {
            return None;
        }
        let scale = appearance.scale;
        Some(h_flex().w_full().h(px(28.0 * scale)).border_y_1().border_color(appearance.foreground.opacity(0.12))
            .children(snapshot.machines.iter().enumerate().map(|(index, machine)| {
                let id = machine.id.clone();
                let reconnect = id.clone();
                let menu_id = id.clone();
                let selected = snapshot.selected_machine_id == id;
                let busy = matches!(machine.state.as_str(), "connecting" | "installing" | "busy");
                let failed = matches!(machine.state.as_str(), "failed" | "error");
                let tooltip = machine.message.as_ref().map(|message| format!("{}: {message}", machine.label)).unwrap_or_else(|| machine.label.clone());
                let icon = gpui::svg().path(if id == "local" { "titlebar/device-desktop.svg" } else if busy { "titlebar/loader2.svg" } else { "titlebar/cloud.svg" }).size(px(14.0 * scale)).text_color(if failed { rgb(0xff9494).into() } else { appearance.muted });
                let glyph = if busy { icon.with_throttled_animation(format!("native-machine-busy-{id}"), std::time::Duration::from_millis(900), |icon, progress| icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(progress)))).into_any_element() } else { icon.into_any_element() };
                h_flex().id(format!("native-sidebar-machine-{id}")).role(gpui::Role::Tab).aria_label(machine.label.clone()).aria_selected(selected).flex_1().min_w_0().h_full().px(px(6.0 * scale)).justify_center().gap(px(4.0 * scale)).text_size(px(12.0 * scale))
                    .when(index > 0, |row| row.border_l_1().border_color(appearance.foreground.opacity(0.12)))
                    .when(selected, |row| row.bg(appearance.selected)).hover(|row| row.bg(appearance.hover))
                    .child(div().id(format!("native-machine-connect-{id}")).flex_shrink_0().child(glyph)
                        .when(id != "local" && machine.state != "connected" && !busy, |icon| icon.on_click(cx.listener(move |app, _, _, cx| { cx.stop_propagation(); app.remote_reconnect_from_sidebar(&reconnect, cx); }))))
                    .child(div().min_w_0().truncate().child(machine.label.clone()))
                    /*
                    CDXC:Sidebar 2026-09-25 DECISION:
                    User: stop showing the status on the machine we're on and show it on the other machine tabs as dots, not numbers, exactly like the Spaces. The selected tab's sessions are already listed below it.
                    */
                    .when(!selected && (machine.working_count > 0 || machine.attention_count > 0 || machine.background_work_count > 0), |row| row.child(super::status::status_dot_stack(machine.working_count, machine.attention_count, machine.background_work_count, scale)))
                    .when(self.native_sidebar.pointer_inside && self.native_sidebar.menu.is_none() && !cx.has_active_drag(), |row| row.tooltip_show_delay(appearance.tooltip_delay).tooltip(move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx)))
                    .on_click(cx.listener(move |app, _, _, cx| { cx.stop_propagation(); app.dispatch_native_sidebar_ui(json!({"type": "selectMachine", "machineId": id}), cx); }))
                    .when(machine.id != "local", |row| row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        cx.stop_propagation();
                        let items = json!([
                            {"label": "Hide Machine", "icon": "eye-off", "command": {"type": "machineAction", "action": "hide", "machineId": menu_id}},
                            {"separator": true},
                            {"label": "Configure Machines", "icon": "settings", "command": {"type": "machineAction", "action": "configure", "machineId": menu_id}}
                        ]);
                        Self::show_native_sidebar_menu(&items, event.position, scale, window, cx);
                    }))
            })).into_any_element())
    }
}
