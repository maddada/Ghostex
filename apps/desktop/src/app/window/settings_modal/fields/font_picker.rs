//! The app font's and the monospace font's lists: the fonts installed on this computer, searchable,
//! each row drawn in its own face. It opens the shared searchable dropdown around a virtualized list, because a computer
//! can have hundreds of families and only the rows on screen should load their fonts.
use super::super::super::native_modal_kit::{capture_child_bounds, hsla, rgba_of};
use super::super::palette::SettingsPalette;
use super::controls::{ButtonVariant, settings_button};
use super::row::{CONTROL_HEIGHT, PageAction, RowSpec, SELECT_WIDTH, setting_row, settings_icon};
use super::searchable_dropdown::{
    DropdownAlign, DropdownState, searchable_dropdown_shell, toggle_dropdown,
};
use super::{CHECK_ICON, SettingsPage, icon};
use crate::ui_fonts::{
    FontRow, TERMINAL_CONFIG_LABEL, draws_its_own_name, exact_family, font_rows,
    installed_families, monospace_default_face, monospace_font_rows, monospace_label,
    picked_family, read_installed_families,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px, uniform_list,
};
use gpui_component::h_flex;
use std::rc::Rc;

const ROW_HEIGHT: f32 = 32.0;
/// At most nine rows show at once, as in the other dropdowns.
const LIST_HEIGHT: f32 = 8.0 + ROW_HEIGHT * 9.0;
const DEFAULT_LABEL: &str = "App default";

/// Which font setting a list picks for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontList {
    /// Every installed family, for the app font.
    App,
    /// Only monospaced families, for the monospace font, whose value may be a Ghostty list.
    Monospace,
}

impl FontList {
    fn rows(self, installed: Option<&[(String, bool)]>, value: &str, query: &str) -> Vec<FontRow> {
        match self {
            FontList::App => font_rows(installed, value, query),
            FontList::Monospace => monospace_font_rows(installed, value, query),
        }
    }

    fn label(self, value: &str, installed: Option<&[(String, bool)]>) -> String {
        match self {
            FontList::App => font_picker_label(value, installed),
            FontList::Monospace => monospace_label(value, installed),
        }
    }

    /// The row a saved `value` selects.
    fn selected_index(self, rows: &[FontRow], value: &str) -> Option<usize> {
        let saved = match self {
            FontList::App => picked_family(value).unwrap_or_default(),
            FontList::Monospace => value.trim(),
        };
        rows.iter()
            .position(|row| row.value().eq_ignore_ascii_case(saved))
    }
}

/// What the closed control reads for the saved `value`.
fn font_picker_label(value: &str, installed: Option<&[(String, bool)]>) -> String {
    let Some(family) = picked_family(value) else {
        return DEFAULT_LABEL.to_string();
    };
    match installed {
        Some(installed)
            if !installed
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(family)) =>
        {
            format!("{family} (not installed)")
        }
        _ => family.to_string(),
    }
}

/// What a pick leaves waiting for Apply: `None` when it is what is saved already.
fn pending_pick(picked: String, saved: &str) -> Option<String> {
    (picked.trim() != saved.trim()).then_some(picked)
}

/// A setting row with the installed fonts' list and its Apply button. A pick only waits in
/// `pending`, which the list and its closed control show, until Apply hands it to `on_apply`: the
/// value to save, empty for the app's default or for the monospace list's terminal config.
///
/// CDXC:DesignSystem 2026-10-31 DECISION:
/// User: "we need to refresh the app after changing the app's font (we need apply button there for this)". The refresh turned out to be a stale text measurement in gpui, now fixed where it was kept (`TextMeasureInputs::fonts_generation`), so Apply redraws every window in place: no relaunch, and no agent or session is touched. The Apply button stays so trying fonts in the list does not redraw the whole app, or reflow every terminal for a monospace font, on each pick.
/// SEE-ALSO: .dependencies/zed/crates/gpui/src/fast/text.rs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn font_picker_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    id: &'static str,
    list: FontList,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &str,
    state: fn(&mut V) -> &mut DropdownState,
    pending: fn(&mut V) -> &mut Option<String>,
    on_apply: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let saved = value.to_string();
    // A pick the setting has since reached by another way (`ghostex settings set`) is not waiting.
    if pending(page)
        .as_deref()
        .is_some_and(|pick| pick.trim() == saved.trim())
    {
        *pending(page) = None;
    }
    let waiting = pending(page).clone();
    let value = waiting.as_deref().unwrap_or(value);
    let on_reset = on_reset.map(|reset| -> PageAction<V> {
        Rc::new(
            move |page: &mut V, window: &mut Window, cx: &mut Context<V>| {
                *pending(page) = None;
                reset(page, window, cx);
            },
        )
    });
    let on_change = {
        let saved = saved.clone();
        move |page: &mut V, picked: String, _: &mut Window, cx: &mut Context<V>| {
            *pending(page) = pending_pick(picked, &saved);
            cx.notify();
        }
    };
    let apply = settings_button(
        p,
        SharedString::from(format!("{id}-apply")),
        "Apply",
        None,
        ButtonVariant::Default,
        waiting.is_none(),
        None,
        move |page: &mut V, window: &mut Window, cx: &mut Context<V>| {
            if let Some(pick) = pending(page).take() {
                on_apply(page, pick, window, cx);
            }
            cx.notify();
        },
        cx,
    );
    let installed = installed_families();
    let installed = installed.as_ref().map(|families| families.as_slice());
    let label = list.label(value, installed);
    let (open, bounds) = {
        let dropdown = state(page);
        (dropdown.open, dropdown.trigger_bounds.clone())
    };
    let value = value.to_string();
    let hover = p.raised_hover;
    let selected_on_open = {
        let value = value.clone();
        move || {
            let installed = installed_families();
            let rows = list.rows(
                installed.as_ref().map(|families| families.as_slice()),
                &value,
                "",
            );
            list.selected_index(&rows, &value)
        }
    };
    let trigger = div()
        .w(px(SELECT_WIDTH))
        .max_w_full()
        .flex_shrink_0()
        .on_children_prepainted(capture_child_bounds(bounds, 0))
        .child(
            h_flex()
                .id(SharedString::from(format!("{id}-trigger")))
                .w_full()
                .h(px(CONTROL_HEIGHT))
                .px(px(12.0))
                .gap(px(6.0))
                .items_center()
                .justify_between()
                .rounded(px(8.0))
                .border_1()
                .border_color(hsla(if open { p.focus_border } else { p.hairline }))
                .bg(hsla(p.raised))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.foreground))
                .cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_click(
                    cx.listener(move |page: &mut V, _: &ClickEvent, window, cx| {
                        let opening = !state(page).open;
                        toggle_dropdown(
                            page,
                            state,
                            "Search fonts...",
                            selected_on_open(),
                            window,
                            cx,
                        );
                        if opening {
                            // Fonts installed since the last read show up without a restart.
                            let read = read_installed_families(cx);
                            cx.spawn(async move |page, cx| {
                                read.await;
                                page.update(cx, |_, cx| cx.notify()).ok();
                            })
                            .detach();
                        }
                    }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .child(settings_icon(icon::CHEVRON_DOWN, 16.0, p.muted).flex_shrink_0()),
        );
    let on_change = Rc::new(on_change);
    let popup = if open {
        let query = state(page).query(cx);
        let rows = Rc::new(list.rows(installed, &value, &query));
        popup(
            p, id, list, state, page, rows, &value, on_change, window, cx,
        )
    } else {
        None
    };
    let control = h_flex()
        .flex_shrink_0()
        .gap(px(8.0))
        .items_center()
        .child(div().flex_shrink_0().child(trigger).children(popup))
        .child(apply)
        .into_any_element();
    setting_row(p, id, spec, on_reset, control, cx)
}

#[allow(clippy::too_many_arguments)]
fn popup<V: SettingsPage>(
    p: &SettingsPalette,
    id: &'static str,
    list: FontList,
    state: fn(&mut V) -> &mut DropdownState,
    page: &mut V,
    rows: Rc<Vec<FontRow>>,
    value: &str,
    on_change: Rc<impl Fn(&mut V, String, &mut Window, &mut Context<V>) + 'static>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let on_choose: Rc<dyn Fn(&mut V, usize, &mut Window, &mut Context<V>)> = {
        let rows = rows.clone();
        Rc::new(move |page, index, window, cx| {
            if let Some(row) = rows.get(index) {
                on_change(page, row.value().to_string(), window, cx);
            }
        })
    };
    let count = rows.len();
    let list_height = (8.0 + count as f32 * ROW_HEIGHT).min(LIST_HEIGHT);
    let palette = *p;
    let selected = list.selected_index(&rows, value);
    let highlight = state(page).highlight;
    let scroll = state(page).list_scroll.clone();
    let row_choose = on_choose.clone();
    let body = (count > 0).then(|| {
        div()
            .w_full()
            .h(px(list_height))
            .flex_shrink_0()
            .min_h_0()
            .p(px(4.0))
            .child(
                uniform_list(
                    SharedString::from(format!("{id}-list")),
                    count,
                    cx.processor(move |_page: &mut V, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                let row = rows.get(index)?;
                                Some(font_row(
                                    &palette,
                                    id,
                                    list,
                                    state,
                                    index,
                                    row,
                                    selected == Some(index),
                                    highlight == Some(index),
                                    row_choose.clone(),
                                    cx,
                                ))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .size_full()
                .track_scroll(&scroll),
            )
            .into_any_element()
    });
    searchable_dropdown_shell(
        p,
        SharedString::from(format!("{id}-menu")),
        state(page),
        state,
        count,
        body,
        "No fonts found.",
        DropdownAlign::End,
        None,
        false,
        None,
        on_choose,
        window,
        cx,
    )
}

#[allow(clippy::too_many_arguments)]
fn font_row<V: SettingsPage>(
    p: &SettingsPalette,
    id: &'static str,
    list: FontList,
    state: fn(&mut V) -> &mut DropdownState,
    index: usize,
    row: &FontRow,
    checked: bool,
    highlighted: bool,
    on_choose: Rc<dyn Fn(&mut V, usize, &mut Window, &mut Context<V>)>,
    cx: &mut Context<V>,
) -> AnyElement {
    let row_highlight = rgba_of(p.foreground, 0.08);
    let (label, hint, face) = match row {
        FontRow::AppDefault => (DEFAULT_LABEL.to_string(), None, None),
        FontRow::MonospaceDefault => (
            row.value().to_string(),
            Some("Default"),
            Some(monospace_default_face()),
        ),
        FontRow::TerminalConfig => (TERMINAL_CONFIG_LABEL.to_string(), None, None),
        FontRow::NotInstalled(name) => (name.clone(), Some("Not installed"), None),
        FontRow::Custom(name) => (name.clone(), Some("Custom"), None),
        FontRow::UseTyped(name) => (format!("Use \"{name}\""), Some("Custom"), None),
        FontRow::Family { name, monospaced } => (
            name.clone(),
            // Every row of the monospace list is one, so only the app font's list says so.
            (*monospaced && list == FontList::App).then_some("Monospace"),
            draws_its_own_name(name).then(|| exact_family(name)),
        ),
    };
    h_flex()
        .id((SharedString::from(format!("{id}-row")), index))
        .w_full()
        .h(px(ROW_HEIGHT))
        .px(px(10.0))
        .gap(px(8.0))
        .items_center()
        .rounded(px(6.0))
        .text_size(px(13.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .cursor_default()
        .when(highlighted, |this| this.bg(hsla(row_highlight)))
        .hover(move |this| this.bg(hsla(row_highlight)))
        .on_click(
            cx.listener(move |page: &mut V, _: &ClickEvent, window, cx| {
                let dropdown = state(page);
                dropdown.open = false;
                on_choose(page, index, window, cx);
                cx.notify();
            }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when_some(face, |this, face| this.font_family(face))
                .child(label),
        )
        .children(hint.map(|hint| {
            div()
                .flex_shrink_0()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child(hint)
        }))
        .child(
            div()
                .flex_shrink_0()
                .when(!checked, |this| this.opacity(0.0))
                .child(settings_icon(CHECK_ICON, 16.0, p.foreground)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_waits_until_it_differs_from_what_is_saved() {
        assert_eq!(
            pending_pick("Menlo".into(), "Monaco").as_deref(),
            Some("Menlo")
        );
        // Apply stays disabled for the font already in use.
        assert_eq!(pending_pick("Menlo".into(), "Menlo"), None);
        assert_eq!(pending_pick(" Menlo ".into(), "Menlo"), None);
    }

    #[test]
    fn a_pick_of_the_default_waits_like_any_other() {
        assert_eq!(pending_pick(String::new(), "Georgia").as_deref(), Some(""));
    }
}
