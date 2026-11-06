//! The extension card grid (extensions-modal/extension-card.tsx (deleted 2026-10-01), extension-surface.tsx (deleted 2026-10-01) and
//! extension-grid.css): `ExtensionGridCard`, the three-to-a-row `ExtensionCardGrid` whose wide
//! cells (an inline editor) land under the row holding their card, the labelled
//! `ExtensionCardGroup`, the icon tiles, the empty states and the dashed Add view cell.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::RowControl;
use super::super::super::fields::{
    SizedButtonSize, SizedButtonVariant, icon, settings_icon, settings_sized_button,
    wrapped_tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use gpui_component::{h_flex, v_flex};
use std::sync::Arc;

/// `.extension-card-grid { gap: 0.625rem }`.
const GRID_GAP: f32 = 10.0;
/// `min-height: 9.5rem`.
pub(crate) const CARD_MIN_HEIGHT: f32 = 152.0;
/// `EXTENSION_ICON_COLOR` (`#b9b9b9`; the light theme's `--extension-icon-color` is `#525252`).
pub(crate) fn extension_icon_color(p: &SettingsPalette) -> gpui::Rgba {
    if p.light {
        gpui::rgb(0x525252)
    } else {
        gpui::rgb(0xb9b9b9)
    }
}

/// The columns of `.extension-card-grid` for a grid `width` wide (its container queries).
pub(crate) fn grid_columns(width: f32) -> usize {
    if width <= 360.0 {
        1
    } else if width <= 560.0 {
        2
    } else {
        3
    }
}

/// The width of the page column the grids fill: the page column, or less in a narrow window
/// (the Settings body's 16px padding and gap around the 192px rail, and the page's 20px gutters).
pub(crate) fn page_column_width(window: &Window) -> f32 {
    let viewport = f32::from(window.viewport_size().width);
    let area = viewport - 16.0 * 3.0 - super::super::super::rail::RAIL_WIDTH;
    (area - super::super::super::page::CONTENT_GUTTER * 2.0)
        .min(super::super::super::page::CONTENT_MAX_WIDTH)
        .max(0.0)
}

/// One cell of the grid: a card and the wide editor that belongs under its row, if open.
pub(crate) struct GridCell {
    pub(crate) card: AnyElement,
    pub(crate) wide: Option<AnyElement>,
}

impl GridCell {
    pub(crate) fn card(card: AnyElement) -> Self {
        Self { card, wide: None }
    }
}

/// `ExtensionCardGrid`: rows of `columns` cards of equal width (and equal height within a row);
/// each wide cell follows the row its card is in, and `trailing_wide` goes after every row.
pub(crate) fn card_grid(
    cells: Vec<GridCell>,
    trailing_wide: Option<AnyElement>,
    columns: usize,
) -> AnyElement {
    let columns = columns.max(1);
    let mut rows: Vec<AnyElement> = Vec::new();
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let mut row_cards: Vec<AnyElement> = Vec::new();
        let mut row_wide: Vec<AnyElement> = Vec::new();
        for _ in 0..columns {
            let Some(cell) = cells.next() else {
                break;
            };
            row_cards.push(cell.card);
            if let Some(wide) = cell.wide {
                row_wide.push(wide);
            }
        }
        let filler = columns - row_cards.len();
        rows.push(
            h_flex()
                .w_full()
                .items_stretch()
                .gap(px(GRID_GAP))
                .children(
                    row_cards
                        .into_iter()
                        .map(|card| div().flex_1().min_w_0().flex().child(card)),
                )
                .children((0..filler).map(|_| div().flex_1().min_w_0()))
                .into_any_element(),
        );
        rows.extend(
            row_wide
                .into_iter()
                .map(|wide| div().w_full().min_w_0().child(wide).into_any_element()),
        );
    }
    if let Some(wide) = trailing_wide {
        rows.push(div().w_full().min_w_0().child(wide).into_any_element());
    }
    v_flex()
        .w_full()
        .gap(px(GRID_GAP))
        .children(rows)
        .into_any_element()
}

/// `ExtensionCardGroup`: a small label and count over a run of cards.
pub(crate) fn card_group(
    p: &SettingsPalette,
    label: impl Into<SharedString>,
    count: Option<usize>,
    first: bool,
    grid: AnyElement,
) -> AnyElement {
    v_flex()
        .w_full()
        .when(!first, |this| this.mt(px(16.0)))
        .child(
            h_flex()
                .mx(px(2.0))
                .mb(px(8.0))
                .items_baseline()
                .gap(px(6.0))
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(p.muted))
                .child(label.into())
                .children(count.map(|count| {
                    div()
                        .text_color(hsla(css_fade(p.muted, 0.7)))
                        .child(count.to_string())
                })),
        )
        .child(grid)
        .into_any_element()
}

/// The icon tile (`.extensions-icon`): 36px on the raised tone with a hairline edge, the glyph
/// in the icon grey. `large` is the detail page's 44px tile on the panel tone.
pub(crate) fn icon_tile(p: &SettingsPalette, glyph: Option<AnyElement>, large: bool) -> AnyElement {
    let (size, radius, background) = if large {
        (44.0, MODAL_RADIUS_SECTION, p.modal.panel)
    } else {
        (36.0, MODAL_RADIUS_CONTROL, p.modal.raised)
    };
    div()
        .flex_shrink_0()
        .size(px(size))
        .p(px(if large { 8.0 } else { 6.0 }))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .border_1()
        .border_color(hsla(p.modal.hairline))
        .bg(hsla(background))
        .children(glyph)
        .into_any_element()
}

/// A Tabler glyph in a tile (the built-in cards, the Store's puzzle piece).
pub(crate) fn tabler_tile(p: &SettingsPalette, path: &'static str, large: bool) -> AnyElement {
    icon_tile(
        p,
        Some(settings_icon(path, 16.0, extension_icon_color(p)).into_any_element()),
        large,
    )
}

/// `ExtensionIcon`: the extension's own icon masked with the icon grey, the puzzle piece when it
/// has none, and an empty tile while it loads.
pub(crate) fn extension_icon_tile(
    p: &SettingsPalette,
    icon: Option<Arc<[u8]>>,
    has_source: bool,
    large: bool,
) -> AnyElement {
    match icon {
        Some(bytes) => icon_tile(
            p,
            Some(
                svg()
                    .data(&bytes)
                    .size_full()
                    .text_color(hsla(extension_icon_color(p)))
                    .into_any_element(),
            ),
            large,
        ),
        None if has_source => icon_tile(p, None, large),
        None => tabler_tile(p, "modals/settings/puzzle.svg", large),
    }
}

/// What an `ExtensionGridCard` shows.
pub(crate) struct GridCardSpec {
    pub(crate) id: SharedString,
    pub(crate) icon: AnyElement,
    pub(crate) control: Option<AnyElement>,
    pub(crate) leading: Option<AnyElement>,
    pub(crate) title: SharedString,
    pub(crate) description: SharedString,
    /// Extra content under the description (the chat bar option row).
    pub(crate) extra: Option<AnyElement>,
    pub(crate) scope_summary: Option<SharedString>,
    pub(crate) meta: SharedString,
    pub(crate) actions: Vec<AnyElement>,
    pub(crate) editing: bool,
    pub(crate) enabled: bool,
    /// The card is being dragged (`.extension-grid-card-sortable[data-dragging='true']`).
    pub(crate) dragging: bool,
}

/// CDXC:Extensions 2026-09-24 DECISION:
/// User: every extension on the Settings Extensions page (built-in, installed, Store and the user's own views) is a card in a grid, three to a row, instead of a list row. One card shape serves all four so they read as one family: icon and the on/off switch (or Install) on top, title, description, an optional scope label, and a footer with the type or author on the left and the row actions, which appear on hover or focus. The switch is the state, so there is no status dot; a card that is off dims its icon.
/// `ExtensionGridCard`: icon and control on top, title, three lines of description, an optional
/// scope label, and a footer with the meta text and the actions that appear on hover.
pub(crate) fn grid_card(p: &SettingsPalette, spec: GridCardSpec) -> AnyElement {
    let group: SharedString = format!("extension-card-{}", spec.id).into();
    let hover_surface = p.raised_hover;
    let ring = p.ring;
    let editing = spec.editing;
    let enabled = spec.enabled;
    let actions = (!spec.actions.is_empty()).then(|| {
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.0))
            .overflow_hidden()
            .when(!editing, |this| {
                this.max_w(px(0.0))
                    .opacity(0.0)
                    .group_hover(group.clone(), |this| this.max_w(px(256.0)).opacity(1.0))
            })
            .children(spec.actions)
    });
    v_flex()
        .id(SharedString::from(format!("extension-card-{}", spec.id)))
        .group(group.clone())
        .w_full()
        .min_w_0()
        .min_h(px(CARD_MIN_HEIGHT))
        .pt(px(12.0))
        .px(px(12.0))
        .pb(px(8.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(if editing { ring } else { p.hairline }))
        .bg(hsla(p.raised))
        .when(editing, |this| {
            this.shadow(vec![gpui::BoxShadow {
                color: hsla(css_fade(ring, 0.2)),
                offset: gpui::point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(3.0),
                inset: false,
            }])
        })
        .when(spec.dragging, |this| this.opacity(0.6))
        .hover(move |this| this.bg(hsla(hover_surface)))
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap(px(6.0))
                .children(spec.leading)
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .when(!enabled, |this| this.opacity(0.55))
                        .child(spec.icon),
                )
                .child(
                    h_flex()
                        .ml_auto()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(6.0))
                        .children(spec.control),
                ),
        )
        .child(
            div()
                .mt(px(10.0))
                .w_full()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.foreground))
                .child(spec.title),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "extension-card-description-{}",
                    spec.id
                )))
                .mt(px(2.0))
                .w_full()
                .min_w_0()
                .line_clamp(3)
                .text_ellipsis()
                .text_size(px(13.0))
                .line_height(px(18.85))
                .text_color(hsla(p.foreground_alpha(0.75)))
                .when(!spec.description.is_empty(), |this| {
                    this.tooltip(wrapped_tooltip_text(spec.description.clone()))
                })
                .child(spec.description),
        )
        .children(spec.extra)
        .children(spec.scope_summary.map(|summary| {
            div()
                .mt(px(8.0))
                .max_w_full()
                .flex_none()
                .self_start()
                .px(px(6.0))
                .py(px(1.0))
                .rounded(px(5.0))
                .bg(hsla(p.foreground_alpha(0.08)))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.0))
                .line_height(px(17.14))
                .text_color(hsla(p.foreground_alpha(0.8)))
                .child(summary)
        }))
        .child(
            h_flex()
                .mt_auto()
                .w_full()
                .min_h(px(28.0))
                .pt(px(8.0))
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .line_height(px(17.14))
                        .text_color(hsla(p.muted))
                        .child(spec.meta),
                )
                .children(actions),
        )
        .into_any_element()
}

/// `.extension-grid-card-option`: a caption and a small switch under the description.
pub(crate) fn card_option_row(
    p: &SettingsPalette,
    label: impl Into<SharedString>,
    control: impl Into<RowControl>,
) -> AnyElement {
    let label = label.into();
    let control = control.into();
    h_flex()
        .id(SharedString::from(format!("card-option-{label}")))
        .mt(px(8.0))
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(p.muted))
        .when_some(control.toggle, |row, press| {
            row.switch_row_press(switch_row_wash(p.foreground), false, press)
        })
        .child(label)
        .child(control.element)
        .into_any_element()
}

/// `.extension-grid-card-status`: the Store card's "Up to date" / "Installed vX".
pub(crate) fn card_status(p: &SettingsPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .whitespace_nowrap()
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(p.muted))
        .child(text.into())
        .into_any_element()
}

/// `AddCustomViewCard`: the dashed last cell of "Your views".
pub(crate) fn add_view_card<V: 'static>(
    p: &SettingsPalette,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let hover_border = p.foreground_alpha(0.35);
    let foreground = p.foreground;
    v_flex()
        .id("extensions-add-view")
        .w_full()
        .min_h(px(CARD_MIN_HEIGHT))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_dashed()
        .border_color(hsla(p.foreground_alpha(0.18)))
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(p.muted))
        .cursor_pointer()
        .hover(move |this| {
            this.border_color(hsla(hover_border))
                .text_color(hsla(foreground))
        })
        .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
            on_click(page, window, cx);
        }))
        .child(settings_icon(icon::PLUS, 16.0, p.muted))
        .child("Add view")
        .into_any_element()
}

/// `ExtensionEmptyState`: a 48px icon tile, a title, a description and an optional action.
pub(crate) fn empty_state(
    p: &SettingsPalette,
    glyph: &'static str,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    action: Option<AnyElement>,
) -> AnyElement {
    v_flex()
        .w_full()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .p(px(32.0))
        .child(
            div()
                .mb(px(4.0))
                .size(px(48.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(MODAL_RADIUS_SECTION))
                .border_1()
                .border_color(hsla(p.modal.hairline))
                .bg(hsla(p.modal.panel))
                .child(settings_icon(glyph, 24.0, p.muted)),
        )
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.foreground))
                .child(title.into()),
        )
        .child(
            div()
                .max_w(px(320.0))
                .text_center()
                .text_size(px(13.0))
                .line_height(px(21.13))
                .text_color(hsla(p.muted))
                .child(description.into()),
        )
        .children(action.map(|action| div().mt(px(8.0)).child(action)))
        .into_any_element()
}

/// `ExtensionEmptyStateFilter`: nothing matches the page filter.
pub(crate) fn empty_state_filter<V: 'static>(
    p: &SettingsPalette,
    on_clear: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let clear = settings_sized_button(
        p,
        "extensions-clear-filters",
        "Clear filters",
        Some("modals/settings/filter-off.svg"),
        None,
        SizedButtonVariant::Outline,
        SizedButtonSize::Sm,
        false,
        None,
        on_clear,
        cx,
    );
    empty_state(
        p,
        icon::SEARCH,
        "No matching extensions",
        "Try a different search or clear one of the filters.",
        Some(clear),
    )
}

/// `.extensions-group`: one panel with hairlines between its rows (the detail page's sections).
pub(crate) fn detail_group(p: &SettingsPalette, rows: Vec<AnyElement>) -> AnyElement {
    let hairline = hsla(p.modal.hairline);
    v_flex()
        .w_full()
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hairline)
        .bg(hsla(p.modal.panel))
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            div()
                .w_full()
                .when(index > 0, |this| this.border_t_1().border_color(hairline))
                .child(row)
        }))
        .into_any_element()
}

/// `ExtensionSectionLabel`: the 13px muted caption over a detail section.
pub(crate) fn section_label(p: &SettingsPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(p.muted))
        .child(text.into())
        .into_any_element()
}
