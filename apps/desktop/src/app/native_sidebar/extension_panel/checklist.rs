//! The `checklist` block. An editable checklist needs no code from the extension: the user ticks
//! an item, renames or deletes it from its right-click menu, or adds one with Add item, and
//! gxserver applies the edit and tells the extension (actions.rs).

use ghostex_gx_protocol::extension_panel::{ChecklistItem, PanelBlock};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::v_flex;

use super::PanelPlace;
use super::actions::ExtensionPanelEdit;
use super::surface::row_frame;
use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_sidebar_command_icon_asset_path;
use crate::app::native_sidebar::appearance::SidebarAppearance;

impl GhostexGpuiApp {
    pub(super) fn render_native_panel_checklist(
        &self,
        block: &PanelBlock,
        items: &[ChecklistItem],
        editable: bool,
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let add_editor = editable
            .then(|| self.native_panel_editor("extensionAdd", place, &block.id, None))
            .flatten();
        let add_row = editable.then(|| match add_editor {
            Some(editor) => div()
                .px(px(6.0 * scale))
                .on_action(
                    cx.listener(|app, _: &gpui_component::input::Escape, _, cx| {
                        cx.stop_propagation();
                        app.finish_native_sidebar_rename(false, cx);
                    }),
                )
                .child(gpui_component::input::Input::new(&editor.input).h(px(24.0 * scale)))
                .into_any_element(),
            None => {
                let open_place = place.clone();
                let open_block = block.id.clone();
                row_frame(
                    format!("ext-add-{}-{}", place.ui_key(), block.id),
                    appearance,
                )
                .role(gpui::Role::Button)
                .aria_label("Add item")
                .text_color(appearance.muted)
                .child(
                    gpui::svg()
                        .size(px(14.0 * scale))
                        .flex_shrink_0()
                        .path(gpui_sidebar_command_icon_asset_path(Some("plus")))
                        .text_color(appearance.muted),
                )
                .child(div().flex_1().min_w_0().truncate().child("Add item"))
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.begin_native_panel_item_editor(
                        &open_place,
                        &open_block,
                        None,
                        "",
                        window,
                        cx,
                    );
                }))
                .into_any_element()
            }
        });
        v_flex()
            .w_full()
            .min_w_0()
            .children(items.iter().map(|item| {
                self.render_native_panel_checklist_item(
                    block, item, editable, place, appearance, cx,
                )
            }))
            .children(add_row)
            .into_any_element()
    }

    fn render_native_panel_checklist_item(
        &self,
        block: &PanelBlock,
        item: &ChecklistItem,
        editable: bool,
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let check_icon = if item.done {
            "circle-check-filled"
        } else {
            "circle"
        };
        let check_color = if item.done {
            appearance.foreground
        } else {
            appearance.muted
        };
        let toggle_place = place.clone();
        let toggle_block = block.id.clone();
        let toggle_item = item.id.clone();
        let done = item.done;
        let checkbox = div()
            .id(gpui::SharedString::from(format!(
                "ext-check-{}-{}-{}",
                place.ui_key(),
                block.id,
                item.id
            )))
            .role(gpui::Role::CheckBox)
            .aria_label(if done { "Mark not done" } else { "Mark done" })
            .aria_toggled(done.into())
            .size(px(18.0 * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                gpui::svg()
                    .size(px(15.0 * scale))
                    .path(gpui_sidebar_command_icon_asset_path(Some(check_icon)))
                    .text_color(check_color),
            )
            .when(editable, |checkbox| {
                checkbox.on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.send_native_panel_edit(
                        &toggle_place,
                        &toggle_block,
                        ExtensionPanelEdit::Check {
                            item_id: toggle_item.clone(),
                            done: !done,
                        },
                        cx,
                    );
                }))
            });
        let editor = editable
            .then(|| self.native_panel_editor("extensionItem", place, &block.id, Some(&item.id)))
            .flatten();
        let text: AnyElement = match editor {
            Some(editor) => div()
                .flex_1()
                .min_w_0()
                .on_action(
                    cx.listener(|app, _: &gpui_component::input::Escape, _, cx| {
                        cx.stop_propagation();
                        app.finish_native_sidebar_rename(false, cx);
                    }),
                )
                .child(gpui_component::input::Input::new(&editor.input).h(px(24.0 * scale)))
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(item.done, |text| {
                    text.text_color(appearance.muted).line_through()
                })
                .child(item.text.clone())
                .into_any_element(),
        };
        // A tap on the row runs the item's action; on an editable checklist without one it ticks
        // the item, like the box.
        let row_place = place.clone();
        let row_block = block.id.clone();
        let row_item = item.id.clone();
        let row_action = item.action.clone();
        let menu = (editable || item.menu.is_some()).then(|| {
            Self::native_panel_menu(
                place,
                &block.id,
                Some(&item.id),
                item.menu.as_deref(),
                editable,
            )
        });
        let frame = row_frame(
            format!("ext-item-{}-{}-{}", place.ui_key(), block.id, item.id),
            appearance,
        )
        .child(checkbox)
        .child(text)
        .when(row_action.is_some() || editable, |frame| {
            frame.on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                match &row_action {
                    Some(action) => app.run_native_panel_action(
                        &row_place,
                        &row_block,
                        Some(&row_item),
                        action,
                        window,
                        cx,
                    ),
                    None => app.send_native_panel_edit(
                        &row_place,
                        &row_block,
                        ExtensionPanelEdit::Check {
                            item_id: row_item.clone(),
                            done: !done,
                        },
                        cx,
                    ),
                }
            }))
        })
        .when_some(menu, |frame, menu| {
            frame.on_mouse_down(
                MouseButton::Right,
                cx.listener(move |_, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    Self::show_native_sidebar_context_menu(
                        &menu,
                        None,
                        event.position,
                        scale,
                        window,
                        cx,
                    );
                }),
            )
        });
        frame.into_any_element()
    }
}
