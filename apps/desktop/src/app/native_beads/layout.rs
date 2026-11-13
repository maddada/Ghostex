//! How the view fits its panel: below `NARROW_WIDTH` the details and the New bead form take the
//! whole panel behind a back bar, above it they sit beside the tree with a divider that can be
//! dragged. The narrow header and the view-options button live here too.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};

use super::menus::{BeadsMenu, BeadsMenuKind};
use super::render::small_button;
use super::view::BeadsView;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;

/// The panel width under which the details and the New bead form replace the tree.
pub(crate) const NARROW_WIDTH: f32 = 760.0;
/// The details / New bead column beside the tree, before its divider is dragged.
pub(crate) const SIDE_DEFAULT_WIDTH: f32 = 420.0;
const SIDE_MIN_WIDTH: f32 = 340.0;
/// What the tree keeps beside the column, however far the divider is dragged.
const TREE_MIN_WIDTH: f32 = 340.0;
/// The divider's own width: a 1px line in the middle of a 7px strip of its own, a sibling of the
/// tree and the column, so the strip covers neither; the pointer turns to a resize cursor over it.
const DIVIDER_WIDTH: f32 = 7.0;
/// Below this tree width the filter row uses its compact form.
const COMPACT_FILTERS_TREE_WIDTH: f32 = 660.0;

impl BeadsView {
    /// The view's width as last drawn (`None` before the first draw).
    fn drawn_width(&self) -> Option<f32> {
        self.bounds.get().map(|bounds| f32::from(bounds.size.width))
    }

    /// The panel is narrow: details and New bead take it whole, the header is compact.
    pub(crate) fn is_narrow(&self) -> bool {
        self.drawn_width().is_some_and(|width| width < NARROW_WIDTH)
    }

    /// The filter row has too little room for its full labels: a narrow panel, or a wide one
    /// whose tree the details or New bead column (dragged wide) leaves narrow.
    pub(crate) fn filters_compact(&self) -> bool {
        let side_open = self.details.is_some() || self.create.is_some();
        self.is_narrow()
            || (side_open
                && self.drawn_width().is_some_and(|width| {
                    width - self.side_width_now() < COMPACT_FILTERS_TREE_WIDTH
                }))
    }

    /// The column's width beside the tree, kept inside what the panel can give it now.
    pub(crate) fn side_width_now(&self) -> f32 {
        let max = self
            .drawn_width()
            .map(|width| (width - TREE_MIN_WIDTH - DIVIDER_WIDTH).max(SIDE_MIN_WIDTH))
            .unwrap_or(f32::MAX);
        self.side_width.clamp(SIDE_MIN_WIDTH, max)
    }

    /// The bar above a full-panel bead or form: back to the tree, and where the bead sits.
    pub(crate) fn render_back_bar(&self, p: &KanbanPalette, cx: &mut Context<Self>) -> AnyElement {
        let crumb = self
            .details
            .as_ref()
            .and_then(|details| self.row(&details.id))
            .and_then(|row| row.parent.clone())
            .and_then(|parent| self.row(&parent).map(|epic| epic.title.clone()))
            .unwrap_or_else(|| "All beads".to_string());
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .pt(px(10.0))
            .pb(px(2.0))
            .child(
                small_button(
                    "beads-back-to-tree",
                    Some("titlebar/arrow-left.svg"),
                    None,
                    p,
                )
                .on_click(cx.listener(|this, _, _, cx| this.close_details(cx))),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(p.muted)
                    .child(crumb),
            )
            .into_any_element()
    }

    /// The rail between the tree and the column. It is a sibling of both, so it covers nothing;
    /// pressing it starts a drag the view follows until the button is released.
    pub(crate) fn render_side_divider(
        &self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dragging = self.side_drag.is_some();
        let line = if dragging { p.muted } else { p.border };
        let hover_line = p.border_strong;
        div()
            .id("beads-side-divider")
            .flex()
            .flex_none()
            .justify_center()
            .w(px(DIVIDER_WIDTH))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .group("beads-side-divider")
            .child(
                div()
                    .w(px(1.0))
                    .h_full()
                    .bg(line)
                    .group_hover("beads-side-divider", move |style| style.bg(hover_line)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.side_drag = Some((f32::from(event.position.x), this.side_width_now()));
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// Follows a divider drag (the view's root listens while one is in progress).
    pub(crate) fn drag_side_divider(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((start_x, start_width)) = self.side_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.side_drag = None;
            cx.notify();
            return;
        }
        // The column is on the right: moving left widens it.
        self.side_width = start_width + (start_x - f32::from(event.position.x));
        self.side_width = self.side_width_now();
        cx.notify();
    }

    pub(crate) fn end_side_drag(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some((_, start_width)) = self.side_drag.take() else {
            return;
        };
        if (self.side_width - start_width).abs() >= 1.0 {
            (self.host.clone())(
                super::view::BeadsRequest::SaveSideWidth(self.side_width),
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// The narrow header's search: an icon until it is pressed or holds a query.
    pub(crate) fn search_collapsed(&self, focused: bool) -> bool {
        self.is_narrow() && !self.search_open && !focused && self.options.query.trim().is_empty()
    }

    pub(crate) fn render_search_icon(
        &self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let search = self.search.clone();
        small_button("beads-search-open", Some("titlebar/search.svg"), None, p)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.search_open = true;
                search.update(cx, |input, cx| input.focus(window, cx));
                cx.notify();
            }))
            .into_any_element()
    }

    /// Sort and Show done, behind one button in the narrow filter row.
    pub(crate) fn render_view_options_button(
        &self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        small_button(
            "beads-view-options",
            Some("titlebar/arrows-sort.svg"),
            None,
            p,
        )
        .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
            this.open_menu(
                BeadsMenu::new(BeadsMenuKind::ViewOptions, event.position()),
                cx,
            )
        }))
        .into_any_element()
    }

    /// The narrow header's sync status: a compact button whose menu holds Pull and Push.
    pub(crate) fn render_sync_pill(
        &self,
        label: String,
        failed: bool,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = p.control_hover;
        div()
            .id("beads-sync-pill")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.0))
            .h(px(26.0))
            .px(px(8.0))
            .rounded(px(7.0))
            .border_1()
            .border_color(p.border)
            .bg(p.control)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .text_size(px(12.0))
            .text_color(if failed { p.danger } else { p.muted })
            .child(titlebar_svg_icon(
                if failed {
                    "titlebar/alert-triangle.svg"
                } else {
                    "titlebar/cloud.svg"
                },
                13.0,
                if failed { p.danger } else { p.muted },
            ))
            .when(!label.is_empty(), |this| this.child(label))
            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                this.open_menu(BeadsMenu::new(BeadsMenuKind::Sync, event.position()), cx)
            }))
            .into_any_element()
    }
}

/// The filter names the narrow row shows.
pub(crate) fn short_filter_label(filter: ghostex_gx_core::beads_view::BeadsFilter) -> &'static str {
    match filter {
        ghostex_gx_core::beads_view::BeadsFilter::NeedsDecision => "Decision",
        other => other.label(),
    }
}

/// The sync status shortened for the narrow header ("Synced 4 min ago" → "4 min").
pub(crate) fn short_sync_label(label: &str) -> String {
    // Anything longer than an age (in progress, failed, not synced yet) shows as the icon alone,
    // whose colour and the Sync menu say the rest, so the header's actions always fit.
    label
        .trim()
        .strip_prefix("Synced ")
        .map(|rest| rest.trim_end_matches(" ago").to_string())
        .filter(|age| age.chars().count() <= 8)
        .unwrap_or_default()
}
