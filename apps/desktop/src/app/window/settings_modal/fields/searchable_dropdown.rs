//! The searchable dropdown (packages/components/ui/searchable-dropdown.css with a cmdk `Command`
//! inside): what a Base UI `Select` of eight or more items becomes (the Extensions page's type and
//! category filters) and what the view scope editor's multi-select opens. `max(320px, trigger)`
//! wide, 8px radius, `0 12px 28px` shadow; a filter field over a 4px-padded list of 32px rows
//! (13px, 6px radius, the highlighted row on 8% foreground), optional group headings, row icons,
//! muted hints and check marks, the empty message, and an optional footer.
//!
//! The native_modal_kit's `modal_searchable_select_menu` draws the trigger-wide, start-aligned
//! variant without groups or checks; this one adds what these two surfaces need.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::row::settings_icon;
use super::{SettingsPage, icon};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, AppContext as _, Bounds, ClickEvent, Context, Entity,
    Focusable as _, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent,
    ParentElement as _, Pixels, ScrollHandle, ScrollStrategy, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, UniformListScrollHandle, Window,
    anchored, deferred, div, point, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;

/// One row of the dropdown.
#[derive(Clone, Debug)]
pub(crate) struct DropdownRow {
    pub(crate) label: SharedString,
    /// Muted text after the label (a project's folder).
    pub(crate) hint: Option<SharedString>,
    pub(crate) icon: Option<&'static str>,
    /// `data-checked`: a check at the row's end.
    pub(crate) checked: bool,
    /// The group heading this row sits under (rows of one group are consecutive).
    pub(crate) group: Option<SharedString>,
}

impl DropdownRow {
    pub(crate) fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            hint: None,
            icon: None,
            checked: false,
            group: None,
        }
    }
}

/// Which edge of the trigger the dropdown lines up with (`align`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropdownAlign {
    Start,
    End,
}

/// A dropdown's open state, filter field and anchor.
pub(crate) struct DropdownState {
    pub(crate) open: bool,
    pub(crate) highlight: Option<usize>,
    pub(crate) trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) filter: Option<Entity<InputState>>,
    pub(crate) scroll: ScrollHandle,
    /// The scroll position of a virtualized list (`searchable_dropdown_shell`'s long lists).
    pub(crate) list_scroll: UniformListScrollHandle,
    /// A `max-h-*` on the popup (the Theme page's terminal palettes use `max-h-80`); `None`
    /// leaves it as tall as the window allows.
    pub(crate) max_height: Option<f32>,
    _subscription: Option<Subscription>,
}

impl Default for DropdownState {
    fn default() -> Self {
        Self {
            open: false,
            highlight: None,
            trigger_bounds: Rc::new(Cell::new(None)),
            filter: None,
            scroll: ScrollHandle::new(),
            list_scroll: UniformListScrollHandle::new(),
            max_height: None,
            _subscription: None,
        }
    }
}

impl DropdownState {
    /// A dropdown whose popup is capped at `max_height` pixels.
    pub(crate) fn with_max_height(max_height: f32) -> Self {
        Self {
            max_height: Some(max_height),
            ..Self::default()
        }
    }

    /// The filter text.
    pub(crate) fn query(&self, cx: &gpui::App) -> String {
        self.filter
            .as_ref()
            .map(|filter| filter.read(cx).value().to_string())
            .unwrap_or_default()
    }
}

/// Opens (or closes) the dropdown held at `state(page)`: clears its filter, focuses it and
/// highlights `selected`.
pub(crate) fn toggle_dropdown<V: SettingsPage>(
    page: &mut V,
    state: fn(&mut V) -> &mut DropdownState,
    placeholder: &'static str,
    selected: Option<usize>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if state(page).filter.is_none() {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe_in(
            &filter,
            window,
            move |page: &mut V, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    state(page).highlight = Some(0);
                    cx.notify();
                }
            },
        );
        let dropdown = state(page);
        dropdown.filter = Some(filter);
        dropdown._subscription = Some(subscription);
    }
    let dropdown = state(page);
    dropdown.open = !dropdown.open;
    dropdown.highlight = dropdown.open.then(|| selected.unwrap_or(0));
    if dropdown.open
        && let Some(filter) = dropdown.filter.clone()
    {
        filter.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
    }
    cx.notify();
}

/// Closes the dropdown held at `state(page)`.
pub(crate) fn close_dropdown<V: 'static>(
    page: &mut V,
    state: fn(&mut V) -> &mut DropdownState,
    cx: &mut Context<V>,
) {
    let dropdown = state(page);
    if dropdown.open {
        dropdown.open = false;
        cx.notify();
    }
}

/// `--ghostex-tooltip-border`: the popup edge and the filter divider.
fn popup_border(p: &SettingsPalette) -> gpui::Rgba {
    if p.light {
        modal_rgba(0x000000, 0.14)
    } else {
        modal_rgba(0xffffff, 0.12)
    }
}

/// The dropdown, drawn above the page while `state` is open. `rows` are the rows the caller's
/// filter kept; `on_choose` gets the index of the chosen one; `keep_open` leaves the dropdown
/// open after a choice (the multi-select).
///
/// CDXC:Release 2026-10-22 WHY: the body lives in `searchable_dropdown_shared`, generic over the page only; generic over the choose handler too it was copied into the release binary once per call site (about 21 KB each).
#[allow(clippy::too_many_arguments)]
pub(crate) fn searchable_dropdown<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    state: &DropdownState,
    get_state: fn(&mut V) -> &mut DropdownState,
    rows: &[DropdownRow],
    empty_message: &'static str,
    align: DropdownAlign,
    width: Option<f32>,
    keep_open: bool,
    footer: Option<AnyElement>,
    on_choose: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    if !state.open {
        return None;
    }
    searchable_dropdown_shared(
        p,
        id.into(),
        state,
        get_state,
        rows,
        empty_message,
        align,
        width,
        keep_open,
        footer,
        Rc::new(on_choose),
        window,
        cx,
    )
}

#[allow(clippy::too_many_arguments)]
fn searchable_dropdown_shared<V: 'static>(
    p: &SettingsPalette,
    id: SharedString,
    state: &DropdownState,
    get_state: fn(&mut V) -> &mut DropdownState,
    rows: &[DropdownRow],
    empty_message: &'static str,
    align: DropdownAlign,
    width: Option<f32>,
    keep_open: bool,
    footer: Option<AnyElement>,
    on_choose: Rc<dyn Fn(&mut V, usize, &mut Window, &mut Context<V>)>,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let p = *p;
    let highlight = state.highlight;
    let row_highlight = rgba_of(p.foreground, 0.08);
    let mut list: Vec<AnyElement> = Vec::new();
    let mut last_group: Option<SharedString> = None;
    for (index, row) in rows.iter().enumerate() {
        if row.group.is_some() && row.group != last_group {
            last_group = row.group.clone();
            list.push(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.muted))
                    .child(row.group.clone().unwrap_or_default())
                    .into_any_element(),
            );
        }
        let highlighted = highlight == Some(index);
        let on_choose = on_choose.clone();
        list.push(
            h_flex()
                .id((SharedString::from(format!("{id}-row")), index))
                .w_full()
                .min_h(px(32.0))
                .px(px(10.0))
                .py(px(6.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(6.0))
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(hsla(p.foreground))
                .cursor_default()
                .when(highlighted, |this| this.bg(hsla(row_highlight)))
                .hover(move |this| this.bg(hsla(row_highlight)))
                .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                    if !keep_open {
                        close_dropdown(page, get_state, cx);
                    }
                    on_choose(page, index, window, cx);
                }))
                .children(
                    row.icon
                        .map(|path| settings_icon(path, 14.0, p.muted).flex_shrink_0()),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .when(row.hint.is_none(), |this| this.flex_1())
                        .child(row.label.clone()),
                )
                .children(row.hint.clone().map(|hint| {
                    div()
                        .ml_auto()
                        .max_w(gpui::relative(0.45))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .text_color(hsla(p.muted))
                        .child(hint)
                }))
                .child(
                    div()
                        .flex_shrink_0()
                        .when(row.hint.is_some(), |this| this.ml(px(4.0)))
                        .when(row.hint.is_none(), |this| this.ml_auto())
                        .when(!row.checked, |this| this.opacity(0.0))
                        .child(settings_icon(super::CHECK_ICON, 16.0, p.foreground)),
                )
                .into_any_element(),
        );
    }
    let body = (!rows.is_empty()).then(|| {
        v_flex()
            .id(SharedString::from(format!("{id}-list")))
            .w_full()
            .min_h_0()
            .p(px(4.0))
            .max_h(px(288.0))
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .children(list)
            .into_any_element()
    });
    searchable_dropdown_shell(
        &p,
        id,
        state,
        get_state,
        rows.len(),
        body,
        empty_message,
        align,
        width,
        keep_open,
        footer,
        on_choose,
        window,
        cx,
    )
}

/// The dropdown around its list: placement, filter field, keyboard and dismissal. `body` is the
/// list itself, `None` when no row is left; a long list passes a virtualized one here, because
/// `searchable_dropdown` lays out every row on every frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn searchable_dropdown_shell<V: 'static>(
    p: &SettingsPalette,
    id: SharedString,
    state: &DropdownState,
    get_state: fn(&mut V) -> &mut DropdownState,
    count: usize,
    body: Option<AnyElement>,
    empty_message: &'static str,
    align: DropdownAlign,
    width: Option<f32>,
    keep_open: bool,
    footer: Option<AnyElement>,
    on_choose: Rc<dyn Fn(&mut V, usize, &mut Window, &mut Context<V>)>,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let trigger = state.trigger_bounds.get()?;
    let filter = state.filter.clone()?;
    let p = *p;
    // `width: max(320px, var(--anchor-width))`, unless the surface sets its own width.
    let width = match width {
        Some(width) => px(width),
        None => px(320.0).max(trigger.size.width),
    };
    let left = match align {
        DropdownAlign::Start => trigger.origin.x,
        DropdownAlign::End => trigger.origin.x + trigger.size.width - width,
    };
    // Base UI opens the popup below its trigger, flips it above when the room below is short and
    // there is more above, and caps it at the available height (`--available-height`).
    let viewport_height = window.viewport_size().height;
    let room_below = viewport_height - (trigger.origin.y + trigger.size.height + px(6.0)) - px(8.0);
    let room_above = trigger.origin.y - px(6.0) - px(8.0);
    // The popup's own height before any cap: search field, the list (at most 288px), footer.
    let list_height = (8.0 + count as f32 * 32.0).min(288.0);
    let natural = px(49.0 + list_height + if footer.is_some() { 29.0 } else { 0.0 });
    let needed = state
        .max_height
        .map(px)
        .map_or(natural, |cap| natural.min(cap));
    let opens_above = room_below < needed && room_above > room_below;
    let available = if opens_above { room_above } else { room_below };
    let position = if opens_above {
        point(left, trigger.origin.y - px(6.0))
    } else {
        point(left, trigger.origin.y + trigger.size.height + px(6.0))
    };
    let border = popup_border(&p);
    let (input_border, input_background) = if p.light {
        (
            modal_rgba(0x000000, 0.16),
            css_mix(gpui::rgb(0x000000), 0.16 * 0.3, p.modal.solid_surface),
        )
    } else {
        (
            modal_rgba(0xffffff, 0.15),
            css_mix(gpui::rgb(0xffffff), 0.15 * 0.3, p.modal.solid_surface),
        )
    };
    let ring = gpui::rgb(0x737373);
    let filter_focused = filter.read(cx).focus_handle(cx).is_focused(window);
    let query_empty = filter.read(cx).value().is_empty();
    let clear_filter = filter.clone();
    let filter_group = h_flex()
        .w_full()
        .h(px(32.0))
        .pl(px(10.0))
        .pr(px(12.0))
        .gap(px(8.0))
        .items_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(if filter_focused { ring } else { input_border }))
        .bg(hsla(input_background))
        .when(filter_focused, |this| {
            this.shadow(vec![gpui::BoxShadow {
                color: hsla(rgba_of(ring, 0.2)),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(3.0),
                inset: false,
            }])
        })
        .child(
            div().flex_1().min_w_0().child(
                Input::new(&filter)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(13.0))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .child(if query_empty {
            div()
                .flex_shrink_0()
                .child(settings_icon(icon::SEARCH, 16.0, rgba_of(p.muted, 0.5)))
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("{id}-clear")))
                .flex_shrink_0()
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .on_click(move |_: &ClickEvent, window, cx| {
                    clear_filter.update(cx, |input, cx| input.set_value("", window, cx));
                })
                .child(settings_icon(icon::X, 16.0, p.muted))
                .into_any_element()
        });
    let key_choose = on_choose.clone();
    let mut popup = v_flex()
        .id(id.clone())
        .occlude()
        .w(width)
        .max_h(
            state
                .max_height
                .map(px)
                .unwrap_or(available)
                .min(available)
                .max(px(0.0)),
        )
        .overflow_hidden()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .bg(hsla(p.modal.solid_surface))
        .font_family(MODAL_UI_FONT)
        .text_color(hsla(p.foreground))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(modal_rgba(0x000000, 0.35)),
            offset: point(px(0.0), px(12.0)),
            blur_radius: px(28.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .on_key_down(cx.listener(move |page, event: &KeyDownEvent, window, cx| {
            let key = event.keystroke.key.as_str();
            let dropdown = get_state(page);
            match key {
                "escape" => {
                    cx.stop_propagation();
                    dropdown.open = false;
                    cx.notify();
                }
                "up" | "down" if count > 0 => {
                    cx.stop_propagation();
                    let delta: isize = if key == "up" { -1 } else { 1 };
                    let current = dropdown.highlight.unwrap_or(0) as isize;
                    let next = (current + delta).rem_euclid(count as isize) as usize;
                    dropdown.highlight = Some(next);
                    dropdown.scroll.scroll_to_item(next);
                    dropdown
                        .list_scroll
                        .scroll_to_item(next, ScrollStrategy::Nearest);
                    cx.notify();
                }
                "enter" => {
                    cx.stop_propagation();
                    let Some(index) = dropdown.highlight.filter(|index| *index < count) else {
                        return;
                    };
                    if !keep_open {
                        dropdown.open = false;
                    }
                    key_choose(page, index, window, cx);
                    cx.notify();
                }
                _ => {}
            }
        }))
        .on_mouse_down_out(
            cx.listener(move |page, event: &MouseDownEvent, _window, cx| {
                if trigger.contains(&event.position) {
                    return;
                }
                close_dropdown(page, get_state, cx);
            }),
        )
        .child(
            div()
                .w_full()
                .flex_shrink_0()
                .p(px(8.0))
                .border_b_1()
                .border_color(hsla(border))
                .child(filter_group),
        );
    match body {
        Some(body) => popup = popup.child(body),
        None => {
            popup = popup.child(
                div()
                    .w_full()
                    .px(px(10.0))
                    .py(px(24.0))
                    .text_center()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .child(empty_message),
            );
        }
    }
    if let Some(footer) = footer {
        popup = popup.child(
            h_flex()
                .w_full()
                .flex_shrink_0()
                .px(px(12.0))
                .py(px(6.0))
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(hsla(border))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(hsla(p.muted))
                .child(footer),
        );
    }
    Some(
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(position)
                .anchor(if opens_above {
                    gpui::Anchor::BottomLeft
                } else {
                    gpui::Anchor::TopLeft
                })
                .snap_to_window_with_margin(px(8.0))
                .child(popup),
        )
        .with_priority(1)
        .into_any_element(),
    )
}
