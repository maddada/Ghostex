//! The four Quick Access row shapes: Commands, Projects, Sessions and Saved Prompts.
//!
//! Every row is the same 40px line: a glyph, the title, a muted subtitle beside it, and
//! right-aligned accessories (keycaps, counts, chips, time, the lifecycle dot). Rows carry no
//! buttons of their own; what a row can do lives in its actions menu (right-click, or the footer's
//! Actions panel), which the controller builds (packages/gx-core/src/quick_access/).
use super::chrome::{
    asset_icon_path, quick_access_icon, quick_access_keycap, quick_access_tooltip,
};
use super::model::{QuickAccessIcon, QuickAccessPromptChip, QuickAccessRow};
use super::palette::{
    QUICK_ACCESS_META_FONT_SIZE, QUICK_ACCESS_ROW_FONT_SIZE, QUICK_ACCESS_ROW_HEIGHT,
    QUICK_ACCESS_ROW_PADDING_X, QUICK_ACCESS_ROW_RADIUS, QuickAccessPalette, hsla, parse_css_color,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, Window, div, px, svg,
};
use gpui_component::h_flex;

pub(crate) struct RowCallbacks<V: 'static> {
    pub(crate) on_activate: std::rc::Rc<dyn Fn(&mut V, String, &mut Window, &mut Context<V>)>,
    pub(crate) on_hover: std::rc::Rc<dyn Fn(&mut V, String, &mut Window, &mut Context<V>)>,
    pub(crate) on_secondary: std::rc::Rc<
        dyn Fn(&mut V, String, gpui::Point<gpui::Pixels>, &mut Window, &mut Context<V>),
    >,
}

impl<V: 'static> Clone for RowCallbacks<V> {
    fn clone(&self) -> Self {
        Self {
            on_activate: self.on_activate.clone(),
            on_hover: self.on_hover.clone(),
            on_secondary: self.on_secondary.clone(),
        }
    }
}

/// The shared row line: hover selects, click activates, right-click opens the row's actions.
fn row_shell<V: 'static>(
    p: &QuickAccessPalette,
    id: (&'static str, usize),
    key: &str,
    active: bool,
    can_activate: bool,
    callbacks: &RowCallbacks<V>,
    cx: &mut Context<V>,
) -> Stateful<gpui::Div> {
    let p = *p;
    let activate = callbacks.on_activate.clone();
    let hover = callbacks.on_hover.clone();
    let secondary = callbacks.on_secondary.clone();
    let hover_key = key.to_string();
    let secondary_key = key.to_string();
    let activate_key = key.to_string();
    h_flex()
        .id(id)
        .w_full()
        .flex_shrink_0()
        .h(px(QUICK_ACCESS_ROW_HEIGHT))
        .min_h(px(QUICK_ACCESS_ROW_HEIGHT))
        .px(px(QUICK_ACCESS_ROW_PADDING_X))
        .gap(px(11.0))
        .items_center()
        .rounded(px(QUICK_ACCESS_ROW_RADIUS))
        .cursor_default()
        .when(active, |this| this.bg(hsla(p.row_selected)))
        .on_mouse_move(cx.listener(move |this, _, window, cx| {
            hover(this, hover_key.clone(), window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                secondary(this, secondary_key.clone(), event.position, window, cx);
            }),
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            if can_activate {
                activate(this, activate_key.clone(), window, cx);
            }
        }))
}

/// A 22px tile behind a line glyph, so icon rows and image rows share one left edge.
fn glyph_tile(p: &QuickAccessPalette, icon: &QuickAccessIcon) -> AnyElement {
    match icon {
        QuickAccessIcon::Image { .. } => div()
            .flex_shrink_0()
            .size(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .child(quick_access_icon(icon, 20.0, p.item))
            .into_any_element(),
        _ => div()
            .flex_shrink_0()
            .size(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .bg(hsla(p.keycap))
            .child(quick_access_icon(icon, 14.0, p.item))
            .into_any_element(),
    }
}

/// Title and muted subtitle on one baseline. The subtitle gives way first.
fn row_text(p: &QuickAccessPalette, title: &str, subtitle: &str) -> gpui::Div {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap(px(9.0))
        .items_baseline()
        .child(
            div()
                .flex_shrink(1.0)
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(QUICK_ACCESS_ROW_FONT_SIZE))
                .line_height(px(20.0))
                .text_color(hsla(p.foreground))
                .child(SharedString::from(title.to_string())),
        )
        .children((!subtitle.is_empty()).then(|| {
            div()
                .flex_shrink(8.0)
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(QUICK_ACCESS_META_FONT_SIZE))
                .line_height(px(20.0))
                .text_color(hsla(p.muted))
                .child(SharedString::from(subtitle.to_string()))
        }))
}

fn accessories() -> gpui::Div {
    h_flex().flex_shrink_0().gap(px(10.0)).items_center()
}

fn meta(p: &QuickAccessPalette, text: &str) -> gpui::Div {
    div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_size(px(QUICK_ACCESS_META_FONT_SIZE))
        .line_height(px(20.0))
        .text_color(hsla(p.muted))
        .child(SharedString::from(text.to_string()))
}

fn status_dot(p: &QuickAccessPalette, lit: bool) -> gpui::Div {
    div()
        .flex_shrink_0()
        .size(px(6.0))
        .rounded_full()
        .bg(hsla(if lit { p.status_dot_open } else { p.status_dot }))
}

/// A formatted accelerator (`⌘⇧T`, `Ctrl+Shift+T`) as one keycap per key.
pub(crate) fn hotkey_keycaps(p: &QuickAccessPalette, label: &str) -> AnyElement {
    let mut keys: Vec<String> = Vec::new();
    if label.contains('+') && label.chars().count() > 1 {
        keys.extend(
            label
                .split(['+', ' '])
                .filter(|part| !part.is_empty())
                .map(str::to_string),
        );
    } else {
        // Mac labels have no separator: a symbol is its own key, a run of letters or digits (`F12`) is one key.
        for character in label.chars().filter(|character| !character.is_whitespace()) {
            match keys.last_mut() {
                Some(last)
                    if character.is_ascii_alphanumeric()
                        && last
                            .chars()
                            .all(|previous| previous.is_ascii_alphanumeric()) =>
                {
                    last.push(character)
                }
                _ => keys.push(character.to_string()),
            }
        }
    }
    h_flex()
        .flex_shrink_0()
        .gap(px(3.0))
        .children(keys.iter().map(|key| quick_access_keycap(p, key)))
        .into_any_element()
}

/// `/Users/me/dev/app` as `~/dev/app`, the way the row's subtitle reads best.
fn home_relative(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

pub(crate) fn quick_access_row<V: 'static>(
    p: &QuickAccessPalette,
    row: &QuickAccessRow,
    index: usize,
    selected: bool,
    hovered: bool,
    callbacks: &RowCallbacks<V>,
    cx: &mut Context<V>,
) -> AnyElement {
    let active = selected || hovered;
    match row {
        QuickAccessRow::Command {
            key,
            title,
            icon,
            hotkey,
        } => row_shell(
            p,
            ("quick-access-command", index),
            key,
            active,
            true,
            callbacks,
            cx,
        )
        .child(glyph_tile(p, icon))
        .child(row_text(p, title, ""))
        .children((!hotkey.is_empty()).then(|| hotkey_keycaps(p, hotkey)))
        .into_any_element(),
        QuickAccessRow::Project {
            key,
            title,
            icon,
            tooltip,
            session_count,
            is_open,
            is_hidden,
        } => {
            let sessions = match session_count {
                0 => String::new(),
                1 => "1 session".to_string(),
                count => format!("{count} sessions"),
            };
            row_shell(
                p,
                ("quick-access-project", index),
                key,
                active,
                true,
                callbacks,
                cx,
            )
            .tooltip({
                let tooltip = tooltip.clone();
                move |window, cx| quick_access_tooltip(tooltip.clone(), window, cx)
            })
            .child(glyph_tile(p, icon))
            .child(row_text(p, title, &home_relative(tooltip)))
            .child(
                accessories()
                    .children(is_hidden.then(|| {
                        svg()
                            .path(asset_icon_path("eye-off"))
                            .size(px(14.0))
                            .flex_shrink_0()
                            .text_color(hsla(p.muted))
                    }))
                    .children((!sessions.is_empty()).then(|| meta(p, &sessions)))
                    .child(status_dot(p, *is_open)),
            )
            .into_any_element()
        }
        QuickAccessRow::Session {
            key,
            title,
            icon,
            project_label,
            file_size,
            file_size_loading,
            time,
            in_sidebar,
            sleeping,
            can_activate,
            ..
        } => row_shell(
            p,
            ("quick-access-session", index),
            key,
            active,
            *can_activate,
            callbacks,
            cx,
        )
        .child(glyph_tile(p, icon))
        .child(row_text(p, title, project_label))
        .child(
            accessories()
                .children((!file_size.is_empty()).then(|| {
                    meta(p, file_size).when(*file_size_loading, |this| this.opacity(0.35))
                }))
                .child(
                    div()
                        .flex_shrink_0()
                        .min_w(px(28.0))
                        .flex()
                        .justify_end()
                        .child(meta(p, time)),
                )
                .child(status_dot(p, *in_sidebar && !*sleeping)),
        )
        .into_any_element(),
        QuickAccessRow::Prompt {
            key,
            title,
            tooltip,
            project_name,
            project_icon,
            session_title,
            tags,
            time,
            is_favorite,
        } => {
            let glyph = if matches!(project_icon, QuickAccessIcon::None) {
                QuickAccessIcon::Asset {
                    name: "note".to_string(),
                    color: None,
                }
            } else {
                project_icon.clone()
            };
            row_shell(
                p,
                ("quick-access-prompt", index),
                key,
                active,
                true,
                callbacks,
                cx,
            )
            .tooltip({
                let tooltip = tooltip.clone();
                move |window, cx| quick_access_tooltip(tooltip.clone(), window, cx)
            })
            .child(glyph_tile(p, &glyph))
            .child(row_text(p, title, project_name))
            .child(
                accessories()
                    .gap(px(7.0))
                    .children(is_favorite.then(|| {
                        svg()
                            .path(asset_icon_path("star-filled"))
                            .size(px(13.0))
                            .flex_shrink_0()
                            .text_color(hsla(p.favorite))
                    }))
                    .children((!session_title.is_empty()).then(|| neutral_chip(p, session_title)))
                    .children(tags.iter().take(2).map(|tag| tag_chip(p, tag)))
                    .child(meta(p, time)),
            )
            .into_any_element()
        }
    }
}

/// A tag: a 20px chip with the tag's color as a dot.
fn tag_chip(p: &QuickAccessPalette, chip: &QuickAccessPromptChip) -> AnyElement {
    let color = chip
        .color
        .as_deref()
        .map(|color| parse_css_color(color, p.foreground))
        .unwrap_or(p.foreground);
    h_flex()
        .flex_shrink_0()
        .max_w(px(110.0))
        .h(px(20.0))
        .pl(px(6.0))
        .pr(px(7.0))
        .gap(px(5.0))
        .items_center()
        .rounded(px(5.0))
        .bg(hsla(p.chip_background(color)))
        .text_size(px(11.5))
        .line_height(px(14.0))
        .text_color(hsla(p.chip_text(color)))
        .child(
            div()
                .flex_shrink_0()
                .size(px(6.0))
                .rounded_full()
                .bg(hsla(color)),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(SharedString::from(chip.label.clone())),
        )
        .into_any_element()
}

/// The source session's title: the tag chip's geometry with no tag color, capped
/// so a long title cannot push the tags or the time off the row.
fn neutral_chip(p: &QuickAccessPalette, label: &str) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .max_w(px(120.0))
        .h(px(20.0))
        .px(px(7.0))
        .items_center()
        .rounded(px(5.0))
        .bg(hsla(p.keycap))
        .text_size(px(11.5))
        .line_height(px(14.0))
        .text_color(hsla(p.muted))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(SharedString::from(label.to_string())),
        )
        .into_any_element()
}
