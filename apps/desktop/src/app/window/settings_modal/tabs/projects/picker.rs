//! The project selector of the Project section: an outline trigger showing the selected project
//! (folder tile, name over path, chevron) and the searchable popover of every main project, aligned
//! to the trigger's right edge 8px below it, with the caret in its search field.
//!
//! CDXC:Projects 2026-06-14-17:29 SEE-ALSO: one selector with a searchable dropdown of project paths replaces the always-visible project list (packages/core-ui/settings-modal/tabs/projects.tsx (deleted 2026-10-01), `.projects-settings-selector-*` in styles/modals.css (deleted 2026-11-06)).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    SearchableList, SearchableListKey, SearchablePopoverSpec, filter_by_score, icon,
    searchable_popover, searchable_row, settings_icon,
};
use super::super::super::palette::SettingsPalette;
use super::{ProjectItem, ProjectsTab};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Rgba, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};

/// `.settings-list-row-control .projects-settings-selector-trigger { width: 22rem }`.
const TRIGGER_WIDTH: f32 = 352.0;

fn card_active(p: &SettingsPalette) -> Rgba {
    if p.light {
        rgb(0xefefef)
    } else {
        rgb(0x2a2a2a)
    }
}

fn app_background(p: &SettingsPalette) -> Rgba {
    if p.light {
        rgb(0xf4f4f5)
    } else {
        rgb(0x0e0e0e)
    }
}

/// The popover surface (`bg-popover`).
fn popover_background(p: &SettingsPalette) -> Rgba {
    if p.light {
        rgb(0xffffff)
    } else {
        rgb(0x0e0e0e)
    }
}

fn project_copy(
    p: &SettingsPalette,
    name: &str,
    path: &str,
    name_size: f32,
    name_line: f32,
    name_weight: FontWeight,
) -> AnyElement {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(2.0))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(name_size))
                .line_height(px(name_line))
                .font_weight(name_weight)
                .text_color(hsla(p.foreground))
                .child(name.to_string()),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(13.0))
                .line_height(px(17.55))
                .text_color(hsla(p.muted))
                .child(path.to_string()),
        )
        .into_any_element()
}

fn picker(tab: &mut ProjectsTab) -> &mut SearchableList {
    &mut tab.picker
}

/// The selector with its popover, `projects` in sidebar order.
pub(super) fn project_picker(
    tab: &mut ProjectsTab,
    p: &SettingsPalette,
    projects: &[ProjectItem],
    selected: &ProjectItem,
    window: &mut Window,
    cx: &mut Context<ProjectsTab>,
) -> AnyElement {
    let list = picker(tab);
    list.sync_query(cx);
    let query = list.query(cx);
    let values: Vec<String> = projects
        .iter()
        .map(|project| format!("{} {}", project.name, project.path))
        .collect();
    let results = filter_by_score(&values, &query);
    let (open, highlight, focus, trigger_bounds) = (
        list.open,
        list.highlight,
        list.focus.clone(),
        list.trigger.clone(),
    );
    let (background, border) = if open {
        (
            css_mix(card_active(p), 0.92, app_background(p)),
            css_mix(p.hairline, 0.74, p.foreground),
        )
    } else if p.light {
        (p.surface, p.hairline)
    } else {
        (gpui::rgba(0x00000000), p.hairline)
    };
    let hover = if p.light {
        rgb(0xf1f1f1)
    } else {
        css_fade(p.hairline, 0.3)
    };
    let tile = div()
        .flex_shrink_0()
        .size(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.48))
        .bg(hsla(css_fade(card_active(p), 0.66)))
        .child(settings_icon(
            icon::FOLDER_OPEN,
            16.0,
            css_mix(p.muted, 0.72, p.foreground),
        ));
    let trigger = h_flex()
        .id("projects-selector-trigger")
        .track_focus(&focus)
        .w(px(TRIGGER_WIDTH))
        .max_w_full()
        .min_h(px(40.0))
        .px(px(10.0))
        .py(px(4.0))
        .gap(px(10.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .bg(hsla(background))
        .cursor_pointer()
        .when(!open, |this| this.hover(move |this| this.bg(hsla(hover))))
        .on_click(
            cx.listener(|tab: &mut ProjectsTab, _: &ClickEvent, window, cx| {
                let list = picker(tab);
                if list.open {
                    list.close(window, cx);
                } else {
                    list.open(true, window, cx);
                }
                cx.notify();
            }),
        )
        .child(tile)
        .child(project_copy(
            p,
            &selected.name,
            &selected.path,
            14.0,
            17.5,
            FontWeight::NORMAL,
        ))
        .child(settings_icon(icon::CHEVRON_DOWN, 16.0, p.foreground).flex_shrink_0());
    let option_icon = css_mix(p.muted, 0.78, p.foreground);
    let rows: Vec<AnyElement> = results
        .iter()
        .enumerate()
        .map(|(position, index)| {
            let project = &projects[*index];
            let project_id = project.project_id.clone();
            searchable_row(
                p,
                ("projects-selector-option", *index),
                47.8,
                6.0,
                10.0,
                position == highlight,
                project.project_id == selected.project_id,
                vec![
                    settings_icon(icon::FOLDER_OPEN, 16.0, option_icon)
                        .flex_shrink_0()
                        .into_any_element(),
                    project_copy(
                        p,
                        &project.name,
                        &project.path,
                        13.0,
                        16.25,
                        FontWeight(650.0),
                    ),
                ],
                move |tab: &mut ProjectsTab, window, cx| {
                    tab.select_project(&project_id, window, cx);
                },
                cx,
            )
        })
        .collect();
    let spec = SearchablePopoverSpec {
        id: SharedString::from("projects-selector-popover"),
        background: popover_background(p),
        border: if p.light {
            modal_rgba(0x000000, 0.14)
        } else {
            modal_rgba(0xffffff, 0.12)
        },
        shadow: true,
        align_end: true,
        side_offset: 8.0,
        list_max_height: 342.0,
        heading: Some("Projects".into()),
        empty_text: "No matching projects".into(),
        empty_compact: false,
        row_height: 47.8,
    };
    let popover = searchable_popover(
        p,
        picker(tab),
        spec,
        rows,
        |tab: &mut ProjectsTab, window, cx| {
            picker(tab).close(window, cx);
            cx.notify();
        },
        |tab: &mut ProjectsTab, window, cx| {
            let list = picker(tab);
            list.search.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            cx.notify();
        },
        window,
        cx,
    );
    let key_ids: Vec<String> = results
        .iter()
        .map(|index| projects[*index].project_id.clone())
        .collect();
    div()
        .max_w_full()
        .on_key_down(cx.listener(
            move |tab: &mut ProjectsTab, event: &KeyDownEvent, window, cx| {
                let list = picker(tab);
                if !list.open {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space" | "down")
                        && list.focus.is_focused(window)
                    {
                        list.open(true, window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                    return;
                }
                match list.handle_key(event, key_ids.len(), window, cx) {
                    SearchableListKey::Choose(position) => {
                        if let Some(project_id) = key_ids.get(position).cloned() {
                            tab.select_project(&project_id, window, cx);
                        }
                    }
                    SearchableListKey::Close => {
                        list.close(window, cx);
                        list.search
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    SearchableListKey::Moved => {}
                    SearchableListKey::Ignored => return,
                }
                cx.stop_propagation();
                cx.notify();
            },
        ))
        .on_children_prepainted(capture_child_bounds(trigger_bounds, 0))
        .child(trigger)
        .when_some(popover, |this, popover| this.child(popover))
        .into_any_element()
}
