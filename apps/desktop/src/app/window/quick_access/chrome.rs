//! The Quick Access shell controls that every tab shares: the search line with its filters, the
//! footer that carries the tabs and the actions, the select pickers, and the portaled menus those
//! pickers open.
use super::model::{QuickAccessIcon, QuickAccessOption, QuickAccessSegment, QuickAccessSelect};
use super::palette::{
    QUICK_ACCESS_CONTROL_HEIGHT, QUICK_ACCESS_FILTER_HEIGHT, QUICK_ACCESS_FILTER_MAX_WIDTH,
    QUICK_ACCESS_FOOTER_HEIGHT, QUICK_ACCESS_ITEM_FONT_SIZE, QUICK_ACCESS_RADIUS_CONTROL,
    QUICK_ACCESS_RADIUS_MENU_ITEM, QUICK_ACCESS_SEARCH_BAR_HEIGHT, QUICK_ACCESS_SEARCH_FONT_SIZE,
    QuickAccessPalette, hsla, parse_css_color,
};
use crate::app::window::native_modal_kit::MODAL_UI_FONT;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, InteractiveElement as _, IntoElement,
    MouseDownEvent, ParentElement as _, Pixels, Rgba, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, img, point, px,
    svg,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(crate) const ICON_SEARCH: &str = "modals/kit/search.svg";
pub(crate) const ICON_CLEAR: &str = "modals/kit/x.svg";
pub(crate) const ICON_SELECTOR: &str = "modals/kit/selector.svg";
pub(crate) const ICON_CHECK: &str = "titlebar/check.svg";

/// The frames the open menus' stand-ins last laid out, in the Quick Access window's coordinates,
/// each with the id of the menu it stands in for.
pub(crate) type QuickAccessMenuFrames = Rc<RefCell<Vec<(&'static str, Bounds<Pixels>)>>>;

/// Where a Quick Access menu draws.
///
/// CDXC:Theming 2026-09-26 DECISION:
/// User, of the Sessions scope dropdown drawn as a solid dark box over the frosted Quick Access window: "the dropdown here shouldn't show like this when app has transparency enabled", "i want the dropdown to be glassy when transparency is enabled". Nothing inside a window can blur what is under it, so under glass on macOS every Quick Access menu (the filter pickers, a row's actions menu, the Actions panel) draws in a frosted host window of its own (`frosted_host.rs`) with the app's frosted menu fill, the way the sidebar's menus do. Quick Access keeps an invisible stand-in of the same size where the menu would be: it sets the host's frame, keeps the rows under the menu from lighting up, and closes the menu on a click elsewhere. Glass off, the menus draw in the window with their solid fill as before.
#[derive(Clone)]
pub(crate) enum QuickAccessMenuPaint {
    InWindow,
    StandIn(QuickAccessMenuFrames),
    /// The menu itself, filling its frosted host window.
    Hosted,
}

/// Wraps an invisible menu panel as its stand-in (`QuickAccessMenuPaint::StandIn`): it reports the
/// panel's frame to `frames`, rendering the window again when that frame moves, takes the pointer
/// over the menu's area, and runs `on_mouse_down_out` for a press anywhere else in the window.
pub(crate) fn quick_access_menu_stand_in<V: 'static>(
    panel: impl IntoElement,
    id: &'static str,
    frames: QuickAccessMenuFrames,
    on_mouse_down_out: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let view = cx.entity().downgrade();
    div()
        .flex()
        .occlude()
        .on_mouse_down_out(on_mouse_down_out)
        .on_children_prepainted(move |bounds, _window, cx| {
            let Some(bounds) = bounds.first().copied() else {
                return;
            };
            {
                let mut frames = frames.borrow_mut();
                match frames.iter_mut().find(|(laid_out, _)| *laid_out == id) {
                    Some((_, known)) if *known == bounds => return,
                    Some((_, known)) => *known = bounds,
                    None => frames.push((id, bounds)),
                }
            }
            if let Some(view) = view.upgrade() {
                view.update(cx, |_, cx| cx.notify());
            }
        })
        .child(panel)
        .into_any_element()
}

/// `titlebar/<kebab-name>.svg`, the same bundle the sidebar's action icons use.
pub(crate) fn asset_icon_path(name: &str) -> SharedString {
    crate::app::helpers::gpui_sidebar_command_icon_asset_path(Some(name))
}

/// One row/menu glyph. The slot keeps its size when the row has no icon so
/// titles stay on the same left edge, exactly like the React `size-4` slot.
pub(crate) fn quick_access_icon(icon: &QuickAccessIcon, size: f32, color: Rgba) -> AnyElement {
    match icon {
        QuickAccessIcon::Asset { name, color: tint } => svg()
            .path(asset_icon_path(name))
            .size(px(size))
            .flex_shrink_0()
            .text_color(hsla(
                tint.as_deref()
                    .map(|tint| parse_css_color(tint, color))
                    .unwrap_or(color),
            ))
            .into_any_element(),
        QuickAccessIcon::Image { url } => {
            match crate::app::native_sidebar::images::sidebar_image(url) {
                Some(image) => img(image)
                    .size(px(size))
                    .flex_shrink_0()
                    .rounded(px(3.0))
                    .into_any_element(),
                None => div().size(px(size)).flex_shrink_0().into_any_element(),
            }
        }
        QuickAccessIcon::None => div().size(px(size)).flex_shrink_0().into_any_element(),
    }
}

/// The search line: the window's header. One large borderless field with the
/// tab's filters at its right edge, hairlined off the list below it.
pub(crate) fn quick_access_search_bar<V: 'static>(
    p: &QuickAccessPalette,
    state: &gpui::Entity<InputState>,
    has_query: bool,
    filters: Vec<AnyElement>,
    on_clear: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    h_flex()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(QUICK_ACCESS_SEARCH_BAR_HEIGHT))
        .pl(px(16.0))
        .pr(px(10.0))
        .gap(px(10.0))
        .items_center()
        .border_b_1()
        .border_color(hsla(p.hairline))
        .child(
            svg()
                .path(ICON_SEARCH)
                .size(px(18.0))
                .flex_shrink_0()
                .text_color(hsla(p.muted)),
        )
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .with_size(ComponentSize::Medium)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(QUICK_ACCESS_SEARCH_FONT_SIZE))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .children(has_query.then(|| {
            div()
                .id("quick-access-search-clear")
                .size(px(24.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(QUICK_ACCESS_RADIUS_MENU_ITEM))
                .cursor_pointer()
                .hover(move |this| this.bg(hsla(p.raised_hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_clear(this, window, cx);
                }))
                .child(
                    svg()
                        .path(ICON_CLEAR)
                        .size(px(15.0))
                        .text_color(hsla(p.muted)),
                )
        }))
        .children((!filters.is_empty()).then(|| {
            h_flex()
                .flex_shrink_0()
                .gap(px(6.0))
                .items_center()
                .children(filters)
        }))
        .into_any_element()
}

/// One filter in the search line: a 28px pill sized to its label (capped, so a
/// long project name ellipsizes instead of squeezing the search text).
pub(crate) fn quick_access_filter_trigger<V: 'static>(
    p: &QuickAccessPalette,
    select: &QuickAccessSelect,
    menu: &QuickAccessMenuState,
    id: &'static str,
    active: bool,
    tooltip: Option<String>,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let open = menu.open;
    let dot = (!select.color.is_empty()).then(|| parse_css_color(&select.color, p.muted));
    h_flex()
        .id(id)
        .flex_shrink_0()
        .when_some(tooltip.filter(|_| !open), |this, tooltip| {
            this.tooltip(move |window, cx| quick_access_tooltip(tooltip.clone(), window, cx))
        })
        .max_w(px(QUICK_ACCESS_FILTER_MAX_WIDTH))
        .h(px(QUICK_ACCESS_FILTER_HEIGHT))
        .pl(px(10.0))
        .pr(px(7.0))
        .gap(px(6.0))
        .items_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(hsla(if open {
            p.focus_border
        } else if active {
            Rgba {
                a: 0.55,
                ..p.accent
            }
        } else {
            p.hairline
        }))
        .bg(hsla(if open { p.raised_hover } else { p.raised }))
        .text_size(px(12.5))
        .line_height(px(18.0))
        .text_color(hsla(if active { p.foreground } else { p.item }))
        .cursor_pointer()
        .when(!open, |this| {
            this.hover(move |this| this.bg(hsla(p.raised_hover)))
        })
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            on_toggle(this, window, cx);
        }))
        .children(dot.map(|dot| {
            div()
                .flex_shrink_0()
                .size(px(7.0))
                .rounded_full()
                .bg(hsla(dot))
        }))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(select.label.clone()),
        )
        .child(
            svg()
                .path(ICON_SELECTOR)
                .size(px(13.0))
                .flex_shrink_0()
                .text_color(hsla(p.muted)),
        )
        .into_any_element()
}

/// A single-choice filter (Saved / Recovered / Sent, All / Closed / External)
/// shaped as the select the search line's other filters already are.
pub(crate) fn segments_as_select(
    segments: &[QuickAccessSegment],
    selected: &str,
) -> QuickAccessSelect {
    QuickAccessSelect {
        label: segments
            .iter()
            .find(|segment| segment.value == selected)
            .map(|segment| segment.label.clone())
            .unwrap_or_default(),
        options: segments
            .iter()
            .map(|segment| QuickAccessOption {
                value: segment.value.clone(),
                label: segment.label.clone(),
                selected: segment.value == selected,
                ..QuickAccessOption::default()
            })
            .collect(),
        ..QuickAccessSelect::default()
    }
}

/// One keycap in the footer and the actions menu.
pub(crate) fn quick_access_keycap(p: &QuickAccessPalette, label: &str) -> AnyElement {
    div()
        .flex_shrink_0()
        .min_w(px(20.0))
        .h(px(20.0))
        .px(px(5.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .bg(hsla(p.keycap))
        .text_size(px(11.5))
        .line_height(px(14.0))
        .text_color(hsla(p.muted))
        .child(SharedString::from(label.to_string()))
        .into_any_element()
}

/// The footer: the four tabs on the left, what Return does to the selected row
/// and the Actions panel on the right.
pub(crate) fn quick_access_footer<V: 'static>(
    p: &QuickAccessPalette,
    tabs: &[(SharedString, SharedString, bool)],
    primary_action: &str,
    actions_open: bool,
    on_select_tab: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    on_primary: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    on_actions: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let tab_items = tabs
        .iter()
        .enumerate()
        .map(|(index, (label, hotkey, active))| {
            let active = *active;
            let on_select_tab = on_select_tab.clone();
            h_flex()
                .id(("quick-access-tab", index))
                .flex_shrink_0()
                .h(px(28.0))
                .px(px(9.0))
                .gap(px(6.0))
                .items_center()
                .rounded(px(7.0))
                .text_size(px(12.5))
                .line_height(px(18.0))
                .whitespace_nowrap()
                .text_color(hsla(if active { p.foreground } else { p.muted }))
                .cursor_pointer()
                .when(active, |this| this.bg(hsla(p.footer_active)))
                .when(!active, |this| {
                    this.hover(move |this| this.bg(hsla(p.raised)).text_color(hsla(p.foreground)))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_select_tab(this, index, window, cx);
                }))
                .child(label.clone())
                // `⌘1` fits beside the label; a spelled-out `Ctrl+1` would push the last tab off the footer.
                .children((hotkey.chars().count() <= 2).then(|| {
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(hsla(if active {
                            p.rail_active_hotkey
                        } else {
                            p.rail_hotkey
                        }))
                        .child(hotkey.clone())
                }))
        })
        .collect::<Vec<_>>();
    let footer_action = move |id: &'static str, active: bool| {
        h_flex()
            .id(id)
            .flex_shrink_0()
            .h(px(28.0))
            .pl(px(9.0))
            .pr(px(6.0))
            .gap(px(7.0))
            .items_center()
            .rounded(px(7.0))
            .text_size(px(12.5))
            .line_height(px(18.0))
            .whitespace_nowrap()
            .cursor_pointer()
            .when(active, |this| this.bg(hsla(p.footer_active)))
            .hover(move |this| this.bg(hsla(p.raised)))
    };
    h_flex()
        .id("quick-access-footer")
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(QUICK_ACCESS_FOOTER_HEIGHT))
        .px(px(8.0))
        .gap(px(8.0))
        .items_center()
        .border_t_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.footer))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .items_center()
                .overflow_hidden()
                .children(tab_items),
        )
        .children((!primary_action.is_empty()).then(|| {
            footer_action("quick-access-primary-action", false)
                .text_color(hsla(p.foreground))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_primary(this, window, cx);
                }))
                .child(SharedString::from(primary_action.to_string()))
                .child(quick_access_keycap(&p, "↵"))
        }))
        .children((!primary_action.is_empty()).then(|| {
            div()
                .flex_shrink_0()
                .w(px(1.0))
                .h(px(16.0))
                .bg(hsla(p.hairline))
        }))
        .child(
            footer_action("quick-access-open-actions", actions_open)
                .text_color(hsla(p.item))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_actions(this, window, cx);
                }))
                .child("Actions")
                .child(
                    h_flex()
                        .gap(px(3.0))
                        .child(quick_access_keycap(&p, "⌘"))
                        .child(quick_access_keycap(&p, "K")),
                ),
        )
        .into_any_element()
}

/// The open state of one Quick Access picker. The window owns one per shelf control.
pub(crate) struct QuickAccessMenuState {
    pub(crate) open: bool,
    pub(crate) query: String,
    pub(crate) highlight: usize,
    pub(crate) trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) scroll: ScrollHandle,
}

impl Default for QuickAccessMenuState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            highlight: 0,
            trigger_bounds: Rc::new(Cell::new(None)),
            scroll: ScrollHandle::new(),
        }
    }
}

impl QuickAccessMenuState {
    pub(crate) fn toggle(&mut self) {
        self.open = !self.open;
        self.query.clear();
        self.highlight = 0;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.query.clear();
    }
}

/// Captures a control's bounds during prepaint so its menu can be anchored to it.
pub(crate) fn capture_bounds(
    cell: Rc<Cell<Option<Bounds<Pixels>>>>,
    index: usize,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
    move |bounds, _window, _cx| cell.set(bounds.get(index).copied())
}

/// The shelf's select trigger: a 32px raised pill with the current label, an
/// optional colored dot, and the selector chevrons.
pub(crate) fn quick_access_select_trigger<V: 'static>(
    p: &QuickAccessPalette,
    select: &QuickAccessSelect,
    menu: &QuickAccessMenuState,
    id: &'static str,
    width: Option<f32>,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let open = menu.open;
    let dot = (!select.color.is_empty()).then(|| parse_css_color(&select.color, p.muted));
    h_flex()
        .id(id)
        .flex_shrink_0()
        .when_some(width, |this, width| this.w(px(width)))
        .when(width.is_none(), |this| this.flex_1().min_w_0())
        .min_w_0()
        .h(px(QUICK_ACCESS_CONTROL_HEIGHT))
        .px(px(10.0))
        .gap(px(6.0))
        .items_center()
        .justify_between()
        .rounded(px(QUICK_ACCESS_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if open { p.focus_border } else { p.hairline }))
        .bg(hsla(p.raised))
        .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
        .line_height(px(20.0))
        .text_color(hsla(p.item))
        .cursor_pointer()
        .when(!open, |this| {
            this.hover(move |this| this.bg(hsla(p.raised_hover)))
        })
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            on_toggle(this, window, cx);
        }))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .gap(px(6.0))
                .items_center()
                .children(dot.map(|dot| {
                    div()
                        .flex_shrink_0()
                        .size(px(7.0))
                        .rounded_full()
                        .bg(hsla(dot))
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(select.label.clone()),
                        )
                        .children((!select.detail.is_empty()).then(|| {
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(11.0))
                                .line_height(px(16.0))
                                .text_color(hsla(p.muted))
                                .child(select.detail.clone())
                        })),
                ),
        )
        .child(
            div().flex_shrink_0().child(
                svg()
                    .path(ICON_SELECTOR)
                    .size(px(16.0))
                    .text_color(hsla(p.muted)),
            ),
        )
        .into_any_element()
}

/// The portaled picker: the Codex popup surface with 28px rows, restated here
/// the way `.previous-sessions-tag-filter-menu` restates it outside the modal.
///
/// CDXC:Theming 2026-09-26 DECISION:
/// User, of the accent-blue text on the chosen option: "we shouldn't show this color for selected item's text, just keep it default color for the text". A chosen option keeps the row's text colour and carries the check the app's other menus use, which is also what shows the tags a multi-choice tag filter has on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn quick_access_select_menu<V: 'static>(
    p: &QuickAccessPalette,
    select: &QuickAccessSelect,
    menu: &QuickAccessMenuState,
    id: &'static str,
    min_width: f32,
    paint: &QuickAccessMenuPaint,
    on_choose: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    if !menu.open {
        return None;
    }
    let trigger = menu.trigger_bounds.get()?;
    let p = *p;
    let max_height = px(288.0).min((window.viewport_size().height - px(16.0)).max(px(0.0)));
    let position = point(
        trigger.origin.x,
        trigger.origin.y + trigger.size.height + px(6.0),
    );
    let visible = visible_options(select, &menu.query);
    let highlight = menu.highlight.min(visible.len().saturating_sub(1));
    let rows = visible
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let option = (*option).clone();
            let on_choose = on_choose.clone();
            let highlighted = index == highlight;
            let value = option.value.clone();
            let dot = (!option.color.is_empty()).then(|| parse_css_color(&option.color, p.muted));
            v_flex()
                .id((id, index))
                .w_full()
                .flex_shrink_0()
                .children(option.separated.then(|| {
                    div()
                        .my(px(3.0))
                        .h(px(1.0))
                        .w_full()
                        .bg(hsla(p.menu_border))
                }))
                .child(
                    h_flex()
                        .id(("quick-access-select-row", index))
                        .w_full()
                        .min_h(px(28.0))
                        .px(px(8.0))
                        .gap(px(8.0))
                        .items_center()
                        .rounded(px(QUICK_ACCESS_RADIUS_MENU_ITEM))
                        .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                        .line_height(px(20.0))
                        .text_color(hsla(p.item))
                        .when(option.disabled, |this| this.opacity(0.5))
                        .when(!option.disabled, |this| {
                            this.cursor_pointer().when(highlighted, |this| {
                                this.bg(hsla(p.menu_hover)).text_color(hsla(p.foreground))
                            })
                        })
                        .when(!option.disabled, |this| {
                            this.hover(move |this| this.bg(hsla(p.menu_hover)))
                        })
                        .children(dot.map(|dot| {
                            div()
                                .flex_shrink_0()
                                .size(px(8.0))
                                .rounded_full()
                                .bg(hsla(dot))
                        }))
                        .children(
                            (!matches!(option.icon, QuickAccessIcon::None))
                                .then(|| quick_access_icon(&option.icon, 14.0, p.item)),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(option.label.clone()),
                                )
                                .children((!option.detail.is_empty()).then(|| {
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(11.0))
                                        .line_height(px(16.0))
                                        .text_color(hsla(p.muted))
                                        .child(option.detail.clone())
                                })),
                        )
                        .children(option.selected.then(|| {
                            svg()
                                .path(ICON_CHECK)
                                .size(px(14.0))
                                .flex_shrink_0()
                                .text_color(hsla(p.item))
                        }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            if option.disabled {
                                return;
                            }
                            on_choose(this, value.clone(), window, cx);
                        })),
                )
        })
        .collect::<Vec<_>>();
    let stand_in = matches!(paint, QuickAccessMenuPaint::StandIn(_));
    let panel = v_flex()
        .id(id)
        .p(px(4.0))
        .gap(px(1.0))
        .rounded(px(QUICK_ACCESS_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.menu_border))
        .children(select.searchable.then(|| {
            h_flex()
                .w_full()
                .h(px(28.0))
                .px(px(8.0))
                .mb(px(4.0))
                .items_center()
                .border_b_1()
                .border_color(hsla(p.menu_border))
                .text_size(px(QUICK_ACCESS_ITEM_FONT_SIZE))
                .text_color(hsla(if menu.query.is_empty() {
                    p.muted
                } else {
                    p.foreground
                }))
                .child(if menu.query.is_empty() {
                    select.search_placeholder.clone()
                } else {
                    menu.query.clone()
                })
        }))
        .child(
            v_flex()
                .id((id, 0usize))
                .w_full()
                .min_h_0()
                .overflow_y_scroll()
                // The stand-in lays out the same rows, but the scroll position is the hosted menu's.
                .when(!stand_in, |this| this.track_scroll(&menu.scroll))
                .gap(px(1.0))
                .children(rows),
        );
    if matches!(paint, QuickAccessMenuPaint::Hosted) {
        return Some(
            panel
                .size_full()
                .font_family(MODAL_UI_FONT)
                .bg(hsla(p.hosted_menu_background))
                .into_any_element(),
        );
    }
    let panel = panel
        .min_w(px(min_width.max(f32::from(trigger.size.width))))
        .max_h(max_height);
    let on_mouse_down_out = cx.listener(move |this, event: &MouseDownEvent, window, cx| {
        if trigger.contains(&event.position) {
            return;
        }
        on_dismiss(this, window, cx);
    });
    let content = match paint {
        QuickAccessMenuPaint::StandIn(frames) => {
            quick_access_menu_stand_in(panel.invisible(), id, frames.clone(), on_mouse_down_out, cx)
        }
        _ => panel
            .occlude()
            .bg(hsla(p.menu_background))
            .shadow_lg()
            .on_mouse_down_out(on_mouse_down_out)
            .into_any_element(),
    };
    Some(
        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.0))
                .child(content),
        )
        .with_priority(1)
        .into_any_element(),
    )
}

/// The rows a searchable picker shows for `query`; every row when it has none.
pub(crate) fn visible_options<'a>(
    select: &'a QuickAccessSelect,
    query: &str,
) -> Vec<&'a QuickAccessOption> {
    let query = query.trim();
    select
        .options
        .iter()
        .filter(|option| {
            query.is_empty() || {
                let text = format!("{} {}", option.label, option.detail).to_lowercase();
                query
                    .split_whitespace()
                    .all(|term| text.contains(&term.to_lowercase()))
            }
        })
        .collect()
}

/// The shared app tooltip: up to 30 lines of the row's full text, matching the
/// React `AppTooltip` the project path and prompt preview rows use.
pub(crate) fn quick_access_tooltip(
    text: String,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyView {
    gpui_component::tooltip::Tooltip::element(move |_, _| {
        v_flex().max_w(px(420.0)).gap(px(2.0)).children(
            text.lines()
                .take(30)
                .map(|line| div().text_size(px(12.0)).child(line.to_owned())),
        )
    })
    .py(px(6.0))
    .px(px(8.0))
    .build(window, cx)
}
