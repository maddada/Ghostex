//! Saved Prompts' two write surfaces: the add/edit form that replaces the list
//! (`.ghostex-stashed-prompt-editor`) and the create-tag popover
//! (`.ghostex-stashed-prompt-tag-popover`), ported from packages/core-ui/stashed-prompts-modal.tsx.
use super::chrome::{QuickAccessMenuState, asset_icon_path, quick_access_select_trigger};
use super::model::{QuickAccessPromptEditor, QuickAccessTagComposer};
use super::palette::{
    QUICK_ACCESS_CONTROL_HEIGHT, QUICK_ACCESS_ITEM_FONT_SIZE, QUICK_ACCESS_RADIUS_CONTROL,
    QuickAccessPalette, hsla, parse_css_color,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    MouseDownEvent, ParentElement as _, Rgba, SharedString, StatefulInteractiveElement as _,
    Styled as _, anchored, deferred, div, px, svg,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use serde_json::json;

/// The add/edit form: heading, the project and tag pickers, the prompt body,
/// an optional error line, then Cancel and the primary submit.
pub(crate) fn quick_access_prompt_editor<V: 'static>(
    p: &QuickAccessPalette,
    editor: &QuickAccessPromptEditor,
    input: Option<&Entity<InputState>>,
    project_menu: &QuickAccessMenuState,
    tag_menu: &QuickAccessMenuState,
    cx: &mut Context<V>,
) -> AnyElement
where
    V: EditorHost,
{
    let p = *p;
    let saving = editor.saving;
    let can_submit = !editor.content.trim().is_empty() && !saving;
    let show_project = !editor.projects.options.is_empty();
    v_flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .gap(px(10.0))
        .p(px(9.0))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(p.foreground))
                .child(SharedString::from(editor.heading.clone())),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .w_full()
                .gap(px(6.0))
                .children(show_project.then(|| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .on_children_prepainted(super::chrome::capture_bounds(
                            project_menu.trigger_bounds.clone(),
                            0,
                        ))
                        .child(quick_access_select_trigger(
                            &p,
                            &editor.projects,
                            project_menu,
                            "quick-access-editor-project",
                            None,
                            |this: &mut V, _window, cx| {
                                this.toggle_editor_project_menu(cx);
                            },
                            cx,
                        ))
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .on_children_prepainted(super::chrome::capture_bounds(
                            tag_menu.trigger_bounds.clone(),
                            0,
                        ))
                        .child(quick_access_select_trigger(
                            &p,
                            &editor.tags,
                            tag_menu,
                            "quick-access-editor-tag",
                            None,
                            |this: &mut V, _window, cx| {
                                this.toggle_editor_tag_menu(cx);
                            },
                            cx,
                        )),
                )
                .child(
                    div()
                        .id("quick-access-editor-favorite")
                        .flex_shrink_0()
                        .size(px(EDITOR_CONTROL_HEIGHT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(QUICK_ACCESS_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .cursor_pointer()
                        .hover(move |this| this.bg(hsla(p.raised)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.post_editor(json!({ "type": "editorFavorite" }), cx);
                        }))
                        .child(
                            svg()
                                .path(asset_icon_path(if editor.is_favorite {
                                    "star-filled"
                                } else {
                                    "star"
                                }))
                                .size(px(16.0))
                                .text_color(hsla(if editor.is_favorite {
                                    p.favorite
                                } else {
                                    p.muted
                                })),
                        ),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .p(px(10.0))
                .rounded(px(QUICK_ACCESS_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(Rgba {
                    a: 0.03,
                    ..p.foreground
                }))
                .when(saving, |this| this.opacity(0.6))
                .children(input.map(|input| {
                    div().size_full().child(
                        Input::new(input)
                            .with_size(ComponentSize::Small)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .disabled(saving)
                            .w_full()
                            .h_full()
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                            .text_color(hsla(p.foreground)),
                    )
                })),
        )
        .children((!editor.error.is_empty()).then(|| {
            div()
                .flex_shrink_0()
                .text_size(px(12.0))
                .text_color(hsla(p.destructive))
                .child(SharedString::from(editor.error.clone()))
        }))
        .child(
            h_flex()
                .flex_shrink_0()
                .w_full()
                .gap(px(6.0))
                .justify_end()
                .child(editor_button(
                    &p,
                    "quick-access-editor-cancel",
                    "Cancel",
                    false,
                    saving,
                    |this: &mut V, cx| this.post_editor(json!({ "type": "editorCancel" }), cx),
                    cx,
                ))
                .child(editor_button(
                    &p,
                    "quick-access-editor-submit",
                    &editor.submit_label,
                    true,
                    !can_submit,
                    |this: &mut V, cx| this.post_editor(json!({ "type": "editorSubmit" }), cx),
                    cx,
                )),
        )
        .into_any_element()
}

/// `.ghostex-stashed-prompt-editor-button`: a 12px outline pill, filled for the primary.
fn editor_button<V: 'static>(
    p: &QuickAccessPalette,
    id: &'static str,
    label: &str,
    primary: bool,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    h_flex()
        .id(id)
        .flex_shrink_0()
        .py(px(6.0))
        .px(px(10.0))
        .items_center()
        .justify_center()
        .rounded(px(QUICK_ACCESS_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .text_size(px(12.0))
        .when(primary, |this| {
            this.bg(hsla(p.foreground)).text_color(hsla(p.solid_window))
        })
        .when(!primary, |this| this.text_color(hsla(p.foreground)))
        .when(disabled, |this| this.opacity(0.45).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| {
                    this.bg(hsla(if primary {
                        Rgba {
                            a: 0.88,
                            ..p.foreground
                        }
                    } else {
                        p.raised
                    }))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    on_click(this, cx);
                }))
        })
        .child(SharedString::from(label.to_string()))
        .into_any_element()
}

/// The create-tag popover: a name field, the eight-swatch palette and Create.
pub(crate) fn quick_access_tag_composer<V: 'static>(
    p: &QuickAccessPalette,
    composer: &QuickAccessTagComposer,
    input: Option<&Entity<InputState>>,
    anchor: Option<gpui::Point<gpui::Pixels>>,
    cx: &mut Context<V>,
) -> AnyElement
where
    V: EditorHost,
{
    let p = *p;
    let position = anchor.unwrap_or_else(|| gpui::point(px(120.0), px(120.0)));
    let selected = composer.color.clone();
    let swatches = composer
        .colors
        .iter()
        .enumerate()
        .map(|(index, color)| {
            let value = color.clone();
            let active = *color == selected;
            let fill = parse_css_color(color, p.foreground);
            div()
                .id(("quick-access-tag-swatch", index))
                .size(px(18.0))
                .rounded_full()
                .border_2()
                .border_color(hsla(if active {
                    p.foreground
                } else {
                    Rgba {
                        a: 0.0,
                        ..p.foreground
                    }
                }))
                .bg(hsla(fill))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.post_editor(
                        json!({ "type": "tagComposerField", "field": "color", "value": value.clone() }),
                        cx,
                    );
                }))
        })
        .collect::<Vec<_>>();
    deferred(
        anchored()
            .position(position)
            .snap_to_window_with_margin(px(8.0))
            .child(
                v_flex()
                    .id("quick-access-tag-composer")
                    .occlude()
                    .min_w(px(224.0))
                    .p(px(6.0))
                    .rounded(px(QUICK_ACCESS_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.menu_border))
                    .bg(hsla(p.menu_background))
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                        this.post_editor(json!({ "type": "tagComposerCancel" }), cx);
                    }))
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(6.0))
                            .text_size(px(11.0))
                            .text_color(hsla(p.muted))
                            .child("NEW TAG"),
                    )
                    .children(input.map(|input| {
                        div()
                            .mx(px(4.0))
                            .mt(px(2.0))
                            .mb(px(4.0))
                            .h(px(30.0))
                            .px(px(9.0))
                            .flex()
                            .items_center()
                            .rounded(px(QUICK_ACCESS_RADIUS_CONTROL))
                            .border_1()
                            .border_color(hsla(p.hairline))
                            .bg(hsla(p.raised))
                            .child(
                                Input::new(input)
                                    .with_size(ComponentSize::Small)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .w_full()
                                    .px(px(0.0))
                                    .py(px(0.0))
                                    .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                                    .text_color(hsla(p.foreground)),
                            )
                    }))
                    .child(
                        h_flex()
                            .gap(px(7.0))
                            .pt(px(8.0))
                            .pb(px(4.0))
                            .px(px(5.0))
                            .children(swatches),
                    )
                    .children((!composer.error.is_empty()).then(|| {
                        div()
                            .px(px(5.0))
                            .text_size(px(12.0))
                            .text_color(hsla(p.destructive))
                            .child(SharedString::from(composer.error.clone()))
                    }))
                    .child(
                        h_flex()
                            .justify_end()
                            .pt(px(8.0))
                            .pb(px(2.0))
                            .px(px(4.0))
                            .child(editor_button(
                                &p,
                                "quick-access-tag-create",
                                "Create",
                                true,
                                composer.name.trim().is_empty(),
                                |this: &mut V, cx| {
                                    this.post_editor(json!({ "type": "tagComposerSubmit" }), cx)
                                },
                                cx,
                            )),
                    ),
            ),
    )
    .with_priority(3)
    .into_any_element()
}

/// What the editor and tag popover need from the window that hosts them.
pub(crate) trait EditorHost: 'static + Sized {
    fn post_editor(&mut self, command: serde_json::Value, cx: &mut Context<Self>);
    fn toggle_editor_project_menu(&mut self, cx: &mut Context<Self>);
    fn toggle_editor_tag_menu(&mut self, cx: &mut Context<Self>);
}

/// Quick Access uses one 32px control height everywhere, including this form.
pub(crate) const EDITOR_CONTROL_HEIGHT: f32 = QUICK_ACCESS_CONTROL_HEIGHT;
