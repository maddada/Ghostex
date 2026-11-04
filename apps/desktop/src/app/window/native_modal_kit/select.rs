use super::*;
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, InteractiveElement as _, IntoElement,
    MouseDownEvent, ParentElement as _, Pixels, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, point, px, rgb,
    size,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;

/// The shadcn select, drawn in-window: a raised 32px trigger and an anchored
/// popover with 28px rows (selected row filled at 12% foreground, hovered or
/// keyboard-highlighted row on the accent). Owned by the modal entity.
pub(crate) struct ModalSelect {
    pub(crate) open: bool,
    pub(crate) highlight: Option<usize>,
    pub(crate) trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    scroll: ScrollHandle,
}

pub(crate) enum ModalSelectKey {
    /// The key moved the highlight or closed the menu.
    Consumed,
    /// Enter chose the highlighted row.
    Choose(usize),
    /// The menu is closed, so the key is the modal's to handle.
    Ignored,
}

impl ModalSelect {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            highlight: None,
            trigger_bounds: Rc::new(Cell::new(None)),
            scroll: ScrollHandle::new(),
        }
    }

    pub(crate) fn toggle(&mut self, selected: Option<usize>) {
        self.open = !self.open;
        self.highlight = self.open.then(|| selected.unwrap_or(0));
        if let Some(index) = self.highlight {
            self.scroll.scroll_to_item(index);
        }
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn handle_key(&mut self, key: &str, count: usize) -> ModalSelectKey {
        if !self.open {
            return ModalSelectKey::Ignored;
        }
        match key {
            "escape" => {
                self.open = false;
                ModalSelectKey::Consumed
            }
            "up" | "down" if count > 0 => {
                let delta: isize = if key == "up" { -1 } else { 1 };
                let current = self.highlight.unwrap_or(0) as isize;
                self.highlight = Some((current + delta).rem_euclid(count as isize) as usize);
                self.scroll.scroll_to_item(self.highlight.unwrap());
                ModalSelectKey::Consumed
            }
            "enter" => {
                self.open = false;
                match self.highlight {
                    Some(index) => ModalSelectKey::Choose(index),
                    None => ModalSelectKey::Consumed,
                }
            }
            _ => ModalSelectKey::Consumed,
        }
    }
}

pub(crate) fn modal_select_trigger<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    value: Option<String>,
    placeholder: &'static str,
    disabled: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let open = select.open;
    h_flex()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_between()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if open { p.focus_border } else { p.hairline }))
        .bg(hsla(p.raised))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .when(!open, |this| {
                    this.hover(move |this| this.bg(hsla(p.raised_hover)))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_toggle(this, window, cx);
                }))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(value.is_none(), |this| this.text_color(hsla(p.muted)))
                .child(value.unwrap_or_else(|| placeholder.to_string())),
        )
        .child(
            div()
                .flex_shrink_0()
                .child(modal_icon(ICON_SELECTOR, 16.0, p.muted)),
        )
        .into_any_element()
}

/// The open select's list in a popover host window (see [`ModalPopoverHost`]): the same rows as
/// the in-window list, on the modal's frosted surface, answering clicks on the modal entity. `None`
/// when no host takes it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hosted_modal_select_menu<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    items: &[String],
    selected: Option<usize>,
    trigger: Bounds<Pixels>,
    on_choose: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    modal_popover_host(p)?;
    let p = *p;
    let highlight = select.highlight;
    let scroll = select.scroll.clone();
    let items: Rc<Vec<String>> = Rc::new(items.to_vec());
    // 28px rows inside 4px padding and a 1px border, bounded like the in-window list.
    let list_height = (items.len() as f32 * 28.0 + 10.0).min(288.0);
    let top = if select_menu_opens_upward(trigger, px(list_height), window) {
        trigger.origin.y - px(4.0) - px(list_height)
    } else {
        trigger.origin.y + trigger.size.height + px(4.0)
    };
    let frame = Bounds::new(
        point(trigger.origin.x, top),
        size(trigger.size.width, px(list_height)),
    );
    let entity = cx.weak_entity();
    let owner = window.window_handle();
    let content: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement> = Rc::new(move |_, _| {
        let rows = items.iter().enumerate().map(|(index, item)| {
            let is_selected = selected == Some(index);
            let highlighted = highlight == Some(index);
            h_flex()
                .id((id, index))
                .w_full()
                .flex_shrink_0()
                .h(px(28.0))
                .px(px(8.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(6.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.foreground))
                .cursor_default()
                .when(is_selected, |this| {
                    this.bg(hsla(p.menu_selected_background()))
                })
                .when(!is_selected && highlighted, |this| this.bg(hsla(p.accent)))
                .when(!is_selected, |this| {
                    this.hover(move |this| this.bg(hsla(p.accent)))
                })
                .on_click(hosted_row_click(
                    entity.clone(),
                    owner,
                    index,
                    on_choose.clone(),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(item.clone()),
                )
        });
        v_flex()
            .id(id)
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .p(px(4.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.menu_border))
            .bg(hsla(p.surface))
            .font_family(MODAL_UI_FONT)
            .children(rows)
            .into_any_element()
    });
    host_modal_popover(&p, id, trigger, frame, content, on_dismiss, window, cx)
}

/// The open select popover, rendered at the modal root so it floats above everything.
pub(crate) fn modal_select_menu<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    items: &[String],
    selected: Option<usize>,
    on_choose: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    if !select.open {
        hide_hosted_modal_popover(id, window, cx);
        return None;
    }
    let trigger = select.trigger_bounds.get()?;
    let p = *p;
    let highlight = select.highlight;
    let on_dismiss = Rc::new(on_dismiss);
    if let Some(element) = hosted_modal_select_menu(
        &p,
        select,
        id,
        items,
        selected,
        trigger,
        on_choose.clone(),
        {
            let on_dismiss = on_dismiss.clone();
            move |this: &mut V, window: &mut Window, cx: &mut Context<V>| {
                (*on_dismiss)(this, window, cx)
            }
        },
        window,
        cx,
    ) {
        return Some(element);
    }
    // CDXC:AppModal 2026-09-16 WHY:
    // Anchoring only repositions the popup; it cannot make a long session list fit or scroll. Bound the list to the window and keep keyboard highlights in the same scroll container.
    let max_height = px(288.0).min((window.viewport_size().height - px(16.0)).max(px(0.0)));
    // 32px rows inside 4px padding and a 1px border.
    let list_height = px(items.len() as f32 * 32.0 + 10.0).min(max_height);
    let upward = select_menu_opens_upward(trigger, list_height, window);
    let position = if upward {
        point(trigger.origin.x, trigger.origin.y - px(4.0))
    } else {
        point(
            trigger.origin.x,
            trigger.origin.y + trigger.size.height + px(4.0),
        )
    };
    let rows = items.iter().enumerate().map(|(index, item)| {
        let is_selected = selected == Some(index);
        let highlighted = highlight == Some(index);
        let on_choose = on_choose.clone();
        h_flex()
            .id((id, index))
            .w_full()
            .flex_shrink_0()
            .min_h(px(28.0))
            .px(px(8.0))
            .py(px(6.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(6.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .cursor_default()
            .when(is_selected, |this| {
                this.bg(hsla(p.menu_selected_background()))
            })
            .when(!is_selected && highlighted, |this| this.bg(hsla(p.accent)))
            .when(!is_selected, |this| {
                this.hover(move |this| this.bg(hsla(p.accent)))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                on_choose(this, index, window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(item.clone()),
            )
    });
    Some(
        deferred(
            anchored()
                .when(upward, |this| this.anchor(gpui::Anchor::BottomLeft))
                .position(position)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    v_flex()
                        .id(id)
                        .occlude()
                        .w(trigger.size.width)
                        .max_h(max_height)
                        .overflow_y_scroll()
                        .track_scroll(&select.scroll)
                        .p(px(4.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.menu_border))
                        .bg(hsla(p.menu_background))
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(
                            move |this, event: &MouseDownEvent, window, cx| {
                                // A mouse-down on the trigger is the trigger's own toggle-close.
                                if trigger.contains(&event.position) {
                                    return;
                                }
                                (*on_dismiss)(this, window, cx);
                            },
                        ))
                        .children(rows),
                ),
        )
        .with_priority(1)
        .into_any_element(),
    )
}

/// The React searchable `Select` filter: the query's whitespace-separated,
/// case-insensitive terms must all occur in the row's search text. An empty
/// query matches everything.
pub(crate) fn modal_select_filter_matches(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|term| text.contains(&term.to_lowercase()))
}

/// A row of [`modal_searchable_select_menu`]. A disabled row is the
/// `<SelectItem disabled>` placeholder a React list renders while it is empty.
pub(crate) struct ModalSearchableSelectRow {
    pub(crate) label: String,
    pub(crate) disabled: bool,
}

/// The filter field at the top of a searchable dropdown (the React `InputGroup` of
/// `searchable-dropdown.css`): 32px, 6px radius, the `--input` border over a `bg-input/30` fill
/// composited on `surface`, and while `focused` the `--ring` border with its 3px 20% halo. The
/// caller adds the text (an `Input`, or text it paints itself) and the trailing search icon or
/// clear button. Every searchable dropdown's filter is drawn by this one helper.
pub(crate) fn modal_filter_field(light: bool, surface: gpui::Rgba, focused: bool) -> gpui::Div {
    // `--input` and `bg-input/30` (theme.css, modals-light.css). The fill is
    // composited over the popup surface because gpui paints the focus-ring
    // shadow behind the element, where a translucent fill would let it through.
    let (input_border, input_background) = if light {
        (
            modal_rgba(0x000000, 0.16),
            css_mix(rgb(0x000000), 0.16 * 0.3, surface),
        )
    } else {
        (
            modal_rgba(0xffffff, 0.15),
            css_mix(rgb(0xffffff), 0.15 * 0.3, surface),
        )
    };
    // `--ring`: oklch(55.6% 0 0) dark, #737373 light.
    let ring = rgb(0x737373);
    h_flex()
        .w_full()
        .h(px(32.0))
        .pl(px(10.0))
        .pr(px(12.0))
        .gap(px(8.0))
        .items_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(if focused { ring } else { input_border }))
        .bg(hsla(input_background))
        .when(focused, |this| {
            this.shadow(vec![gpui::BoxShadow {
                color: hsla(rgba_of(ring, 0.2)),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(3.0),
                inset: false,
            }])
        })
}

/// A filter field whose keystrokes come from its menu (the menu lives in a frosted window that
/// never takes the keyboard): always drawn focused, with a caret after the typed text, the
/// placeholder after the caret while nothing is typed, and the search icon at half opacity.
#[allow(clippy::too_many_arguments)]
pub(crate) fn modal_typed_filter_field(
    light: bool,
    surface: gpui::Rgba,
    query: &str,
    placeholder: &str,
    foreground: gpui::Rgba,
    muted: gpui::Rgba,
    icon_size: f32,
    caret_height: f32,
) -> gpui::Div {
    modal_filter_field(light, surface, true)
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .items_center()
                .children(
                    (!query.is_empty())
                        .then(|| div().text_color(hsla(foreground)).child(query.to_owned())),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(1.0))
                        .h(px(caret_height))
                        .bg(hsla(foreground)),
                )
                .children(query.is_empty().then(|| {
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(hsla(muted))
                        .child(placeholder.to_owned())
                })),
        )
        .child(
            gpui::svg()
                .path("modals/kit/search.svg")
                .size(px(icon_size))
                .flex_shrink_0()
                .text_color(hsla(rgba_of(muted, 0.5))),
        )
}

/// The searchable shadcn Select popup (`packages/components/ui/select.tsx` (deleted 2026-10-01)
/// with `searchable-dropdown.css`): trigger-wide, 8px radius, `0 12px 28px`
/// shadow, a filter `InputGroup` (32px, 6px radius, `--input` border, ring
/// colored with a 3px 20% halo while focused, search icon at half opacity or a
/// clear button once a query is typed) above a 4px-padded list of 32px rows
/// (13px, 6px radius, the highlighted or hovered row on 8% foreground), and the
/// empty message (13px, 24px 10px, centered) when nothing survives the filter.
/// The popup paints `bg-popover`, which is the modal surface color (`#0e0e0e` / `#ffffff`).
/// `rows` are the rows that survive the caller's filter; `select.highlight`
/// indexes into them. The caller creates the filter `InputState`
/// (single line, with the React `searchPlaceholder`), clears and focuses it
/// when the menu opens, and resets the highlight on `InputEvent::Change`.
pub(crate) fn modal_searchable_select_menu<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    filter: &gpui::Entity<InputState>,
    rows: &[ModalSearchableSelectRow],
    empty_message: &'static str,
    on_choose: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    on_clear_filter: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    if !select.open {
        return None;
    }
    let trigger = select.trigger_bounds.get()?;
    let p = *p;
    let highlight = select.highlight;
    let position = point(
        trigger.origin.x,
        trigger.origin.y + trigger.size.height + px(4.0),
    );
    // `--ghostex-tooltip-border` (theme.css): the popup edge and the filter divider.
    let popup_border = if p.light {
        modal_rgba(0x000000, 0.14)
    } else {
        modal_rgba(0xffffff, 0.12)
    };
    let filter_focused = filter.read(cx).focus_handle(cx).is_focused(window);
    let query = filter.read(cx).value().to_string();
    let row_highlight = rgba_of(p.foreground, 0.08);
    let list_rows = rows.iter().enumerate().map(|(index, row)| {
        let highlighted = highlight == Some(index);
        let disabled = row.disabled;
        let on_choose = on_choose.clone();
        h_flex()
            .id((id, index))
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
            .when(disabled, |this| this.opacity(0.5))
            .when(highlighted, |this| this.bg(hsla(row_highlight)))
            .when(!disabled, |this| {
                this.hover(move |this| this.bg(hsla(row_highlight)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_choose(this, index, window, cx);
                    }))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(row.label.clone()),
            )
    });
    let filter_group = modal_filter_field(p.light, p.solid_surface, filter_focused)
        .child(
            div().flex_1().min_w_0().child(
                Input::new(filter)
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
        .child(if query.is_empty() {
            div()
                .flex_shrink_0()
                .child(modal_icon(
                    "modals/kit/search.svg",
                    16.0,
                    rgba_of(p.muted, 0.5),
                ))
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("{id}-clear-filter")))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .size(px(24.0))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_clear_filter(this, window, cx);
                }))
                .child(modal_icon("modals/kit/x.svg", 16.0, p.muted))
                .into_any_element()
        });
    let mut popup = v_flex()
        .id(id)
        .occlude()
        .w(trigger.size.width)
        .overflow_hidden()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(popup_border))
        .bg(hsla(p.solid_surface))
        .text_color(hsla(p.foreground))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(modal_rgba(0x000000, 0.35)),
            offset: point(px(0.0), px(12.0)),
            blur_radius: px(28.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .on_mouse_down_out(
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if trigger.contains(&event.position) {
                    return;
                }
                on_dismiss(this, window, cx);
            }),
        )
        .child(
            div()
                .w_full()
                .p(px(8.0))
                .border_b_1()
                .border_color(hsla(popup_border))
                .child(filter_group),
        );
    if rows.is_empty() {
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
    popup = popup.child(
        v_flex()
            .id(SharedString::from(format!("{id}-list")))
            .w_full()
            .p(px(4.0))
            .max_h(px(288.0))
            .overflow_y_scroll()
            .children(list_rows),
    );
    Some(
        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.0))
                .child(popup),
        )
        .with_priority(1)
        .into_any_element(),
    )
}
