//! The Beads view's frame: the header both layouts share, the filter strip, the cards, the Tree
//! (or the Board the app draws) and the details or new-bead column beside it.

use ghostex_gx_core::beads_view::BeadsFilter;
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, list, px,
};
use gpui_component::input::Input;
use gpui_component::{Sizable as _, Size as ComponentSize};

use super::menus::{BeadsMenu, BeadsMenuKind};
use super::view::{BeadsLayout, BeadsView};
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::widgets::{CONTROL_HEIGHT, CONTROL_RADIUS};
use crate::app::window::native_modal_kit::{
    ModalRailItem, ModalSegmentedItem, modal_bordered_segmented_control, modal_segmented_control,
    modal_switch,
};

pub(crate) fn section_label(text: impl Into<SharedString>, p: &KanbanPalette) -> gpui::Div {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.faint)
        .child(text.into().to_uppercase())
}

/// A compact button for rows and headers.
pub(crate) fn small_button(
    id: impl Into<ElementId>,
    icon: Option<&'static str>,
    label: Option<&'static str>,
    p: &KanbanPalette,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(5.0))
        .h(px(26.0))
        .when(label.is_none(), |this| this.w(px(26.0)))
        .when(label.is_some(), |this| this.px(px(9.0)))
        .rounded(px(7.0))
        .border_1()
        .border_color(p.border)
        .bg(p.control)
        .cursor_pointer()
        .text_size(px(12.0))
        .text_color(p.foreground)
        .hover(move |style| style.bg(hover))
        .when_some(icon, |this, icon| {
            this.child(titlebar_svg_icon(icon, 13.0, p.muted))
        })
        .when_some(label, |this, label| this.child(label))
}

/// One segment of a joined control (Tree | Board, Pull | Push).
fn segment(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    selected: bool,
    p: &KanbanPalette,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.0))
        .h_full()
        .px(px(11.0))
        .cursor_pointer()
        .text_size(px(13.0))
        .text_color(if selected { p.foreground } else { p.muted })
        .when(selected, |this| this.bg(p.control_hover))
        .when(!selected, |this| this.hover(move |style| style.bg(hover)))
        .child(titlebar_svg_icon(
            icon,
            14.0,
            if selected { p.foreground } else { p.muted },
        ))
        .child(label)
}

fn joined(p: &KanbanPalette) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(CONTROL_HEIGHT))
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(p.border)
        .bg(p.control)
        .overflow_hidden()
}

impl BeadsView {
    fn render_header(
        &mut self,
        p: &KanbanPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ready = self.workspace_ready();
        let has_remote = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.sync.remote.is_some());
        let sync_ok =
            self.has_capability(ghostex_gx_protocol::beads_view::capability::SYNC) && !self.legacy;
        let (synced, sync_failed) = self.synced_label();
        let running = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.sync.running.is_some());
        let tree = self.layout == BeadsLayout::Tree;
        let focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        let can_create = ready || self.legacy;
        let narrow = self.is_narrow();
        let search_collapsed = self.search_collapsed(focused);
        if !focused && self.options.query.trim().is_empty() {
            self.search_open = false;
        }
        let Some(modal) = self.modal else {
            return div().into_any_element();
        };
        let left = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .flex_basis(px(if narrow { 0.0 } else { 380.0 }))
            .min_w_0()
            .child(
                div()
                    .flex_none()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Beads"),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(if narrow { 164.0 } else { 190.0 }))
                    .child(modal_segmented_control(
                        &modal,
                        "beads-layout",
                        &[
                            ModalSegmentedItem {
                                icon: Some("titlebar/sitemap.svg"),
                                label: "Tree",
                            },
                            ModalSegmentedItem {
                                icon: Some("titlebar/layout-board-split.svg"),
                                label: "Board",
                            },
                        ],
                        if tree { 0 } else { 1 },
                        |this: &mut Self, index, window, cx| {
                            let layout = if index == 0 {
                                BeadsLayout::Tree
                            } else {
                                BeadsLayout::Board
                            };
                            this.set_layout(layout, window, cx)
                        },
                        cx,
                    )),
            )
            .when(search_collapsed, |this| {
                this.child(self.render_search_icon(p, cx))
            })
            .when(!search_collapsed, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .w(px(260.0))
                        .flex_shrink(1.0)
                        .min_w(px(120.0))
                        .h(px(CONTROL_HEIGHT))
                        .px(px(10.0))
                        .rounded(px(CONTROL_RADIUS))
                        .border_1()
                        .border_color(if focused { p.border_strong } else { p.border })
                        .bg(p.control)
                        .child(titlebar_svg_icon("titlebar/search.svg", 14.0, p.muted))
                        .child(
                            div().flex_1().min_w_0().child(
                                Input::new(&self.search)
                                    .with_size(ComponentSize::Small)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .w_full()
                                    .px(px(0.0))
                                    .py(px(0.0))
                                    .text_size(px(13.0))
                                    .text_color(p.foreground),
                            ),
                        ),
                )
            });
        let right = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.0))
            .ml_auto()
            .when(narrow && ready && sync_ok && has_remote, |this| {
                this.child(self.render_sync_pill(
                    super::layout::short_sync_label(&synced),
                    sync_failed,
                    p,
                    cx,
                ))
            })
            .when(!narrow && ready && sync_ok && has_remote, |this| {
                this.child(
                    joined(p)
                        .child(
                            div()
                                .id("beads-sync-status")
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .h_full()
                                .px(px(10.0))
                                .cursor_pointer()
                                .text_size(px(13.0))
                                .text_color(p.foreground)
                                .child(titlebar_svg_icon(
                                    if sync_failed {
                                        "titlebar/alert-triangle.svg"
                                    } else {
                                        "titlebar/cloud.svg"
                                    },
                                    14.0,
                                    if sync_failed { p.danger } else { p.muted },
                                ))
                                .child(synced)
                                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                                    this.open_menu(
                                        BeadsMenu::new(BeadsMenuKind::Sync, event.position()),
                                        cx,
                                    )
                                })),
                        )
                        .child(div().w(px(1.0)).h_full().bg(p.border))
                        .child(
                            segment("beads-pull", "titlebar/download.svg", "Pull", false, p)
                                .when(running, |this| this.opacity(0.45))
                                .when(!running, |this| {
                                    this.on_click(cx.listener(|this, _, window, cx| {
                                        this.start_sync("pull", window, cx)
                                    }))
                                }),
                        )
                        .child(div().w(px(1.0)).h_full().bg(p.border))
                        .child(
                            segment("beads-push", "titlebar/upload.svg", "Push", false, p)
                                .when(running, |this| this.opacity(0.45))
                                .when(!running, |this| {
                                    this.on_click(cx.listener(|this, _, window, cx| {
                                        this.start_sync("push", window, cx)
                                    }))
                                }),
                        ),
                )
            })
            .child(
                crate::app::native_kanban::widgets::kanban_button(
                    "beads-refresh",
                    Some("titlebar/refresh.svg"),
                    None,
                    crate::app::native_kanban::widgets::KanbanButtonKind::Secondary,
                    false,
                    p,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(fetch) = this.fetch_now(true, cx) {
                        fetch.detach();
                    }
                    if this.layout == BeadsLayout::Board {
                        (this.host.clone())(super::view::BeadsRequest::Refresh, window, cx);
                    }
                })),
            )
            .child(
                crate::app::native_kanban::widgets::kanban_button(
                    "beads-new",
                    Some("titlebar/plus.svg"),
                    (!narrow).then(|| "New bead".into()),
                    crate::app::native_kanban::widgets::KanbanButtonKind::Primary,
                    !can_create,
                    p,
                )
                .when(can_create, |this| {
                    this.on_click(cx.listener(|this, _, window, cx| {
                        let board = (this.layout == BeadsLayout::Board).then(|| "open".to_string());
                        this.open_create(None, board, window, cx)
                    }))
                }),
            );
        // CDXC:ProjectBoard 2026-11-12 WHY: in the side panel at its usual width the one-row header ran past the panel's edge once the Sync controls showed, and New bead could not be reached. The row wraps: the sync controls, Refresh and New bead move to a second line, right-aligned, when they do not fit beside the title, layout switch and search.
        div()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap_x(px(10.0))
            .gap_y(px(8.0))
            .pb(px(12.0))
            .border_b_1()
            .border_color(p.border)
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn render_filters(&mut self, p: &KanbanPalette, cx: &mut Context<Self>) -> AnyElement {
        let readiness_ok = self.tree.readiness_ok;
        let current = self.options.filter;
        let sort = self.options.sort;
        let show_done = self.options.show_done;
        let hover = p.control_hover;
        let ink = p.foreground.to_rgb();
        let filters: Vec<BeadsFilter> = BeadsFilter::ALL
            .into_iter()
            .filter(|filter| readiness_ok || !filter.needs_readiness())
            .collect();
        let filters_for_click = filters.clone();
        let Some(modal) = self.modal else {
            return div().into_any_element();
        };
        let narrow = self.filters_compact();
        let control = modal_bordered_segmented_control(
            ink,
            gpui::Rgba { a: 0.14, ..ink },
            gpui::Rgba { a: 0.55, ..ink },
            "beads-filters",
            &filters
                .iter()
                .map(|filter| ModalRailItem {
                    label: if narrow {
                        super::layout::short_filter_label(*filter).into()
                    } else {
                        filter.label().into()
                    },
                    trailing: Some(self.tree.counts.get(*filter).to_string().into()),
                    dot: None,
                })
                .collect::<Vec<_>>(),
            filters
                .iter()
                .position(|filter| *filter == current)
                .unwrap_or(0),
            move |this: &mut Self, index, _, cx| {
                if let Some(filter) = filters_for_click.get(index).copied() {
                    this.set_filter(filter, cx)
                }
            },
            cx,
        );
        // CDXC:ProjectBoard 2026-11-13 WHY: in the panel's usual narrow width the filters stay the shared segmented control (one control kind for this single-select) and scroll sideways instead of wrapping, and Sort and Show done move into one view-options menu, so the filter row keeps one line.
        if narrow {
            return div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id("beads-filters-scroll")
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .overflow_x_scroll()
                        .child(control),
                )
                .child(self.render_view_options_button(p, cx))
                .into_any_element();
        }
        div()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .child(control)
            .child(div().flex_1())
            .child(
                div()
                    .id("beads-show-done")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .h(px(CONTROL_HEIGHT))
                    .rounded(px(CONTROL_RADIUS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .text_size(px(13.0))
                    .text_color(p.muted)
                    .child(modal_switch(&modal, show_done, false))
                    .child("Show done")
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_show_done(cx))),
            )
            .child(
                div()
                    .id("beads-sort")
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .h(px(CONTROL_HEIGHT))
                    .rounded(px(CONTROL_RADIUS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .text_size(px(13.0))
                    .text_color(p.foreground)
                    .child(titlebar_svg_icon("titlebar/arrows-sort.svg", 14.0, p.muted))
                    .child(sort.label())
                    .child(titlebar_svg_icon(
                        "titlebar/chevron-down.svg",
                        12.0,
                        p.muted,
                    ))
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                        this.open_menu(BeadsMenu::new(BeadsMenuKind::Sort, event.position()), cx)
                    })),
            )
            .into_any_element()
    }

    fn render_tree_body(&mut self, p: &KanbanPalette, cx: &mut Context<Self>) -> AnyElement {
        if self.items.is_empty() {
            let text = if self.loading || self.snapshot.is_none() {
                "Loading beads…"
            } else if !self.options.query.trim().is_empty()
                || self.options.filter != BeadsFilter::All
            {
                "No beads match."
            } else {
                "No open beads. Create one with New bead."
            };
            return div()
                .flex_1()
                .pt(px(24.0))
                .text_size(px(13.0))
                .text_color(p.muted)
                .child(text)
                .into_any_element();
        }
        div()
            .flex_1()
            .min_h_0()
            .h_full()
            .child(
                list(
                    self.list.clone(),
                    cx.processor(|this, index, window, cx| {
                        this.render_tree_item(index, window, cx)
                    }),
                )
                .size_full()
                .pb(px(16.0)),
            )
            .into_any_element()
    }
}

impl Render for BeadsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(p) = self.palette.clone() else {
            return div().into_any_element();
        };
        let header = self.render_header(&p, window, cx);
        let cards = self.render_cards(&p, cx);
        let empty = self.render_empty_card(&p, cx);
        let tree_layout = self.layout == BeadsLayout::Tree;
        let ready = self.workspace_ready();
        let narrow = self.is_narrow();
        let side = match self.create.is_some() {
            true => self.render_create(&p, window, cx),
            false => self.render_details(&p, window, cx),
        };
        let full_side = narrow && side.is_some();
        // The New bead form has its own title, ✕ and Cancel, so only the details get a back bar.
        let back_bar = (full_side && self.create.is_none()).then(|| self.render_back_bar(&p, cx));
        let divider = (!narrow && side.is_some()).then(|| self.render_side_divider(&p, cx));
        let side_width = self.side_width_now();
        self.show_pending_menu(window, cx);
        let bounds = self.bounds.clone();
        let main: AnyElement = if tree_layout && !ready {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .pt(px(14.0))
                .children(empty)
                .when(self.load_error.is_some(), |this| {
                    this.child(
                        div()
                            .text_size(px(13.0))
                            .text_color(p.muted)
                            .child(self.load_error.clone().unwrap_or_default()),
                    )
                })
                .into_any_element()
        } else if tree_layout {
            let filters = self.render_filters(&p, cx);
            let body = self.render_tree_body(&p, cx);
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(px(4.0))
                .pt(px(12.0))
                .child(filters)
                .child(body)
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .pt(px(12.0))
                .children(self.board.take())
                .into_any_element()
        };
        div()
            .id("beads-view")
            .key_context("BeadsView")
            .track_focus(&self.focus)
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                this.drag_side_divider(event, cx)
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_side_drag(cx)),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_side_drag(cx)),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    if this.menu.is_some() {
                        this.close_menu(cx);
                    } else if this.create.is_some() {
                        this.close_create(cx);
                    } else {
                        this.close_details(cx);
                    }
                }
            }))
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .px(px(16.0))
            .pt(px(14.0))
            .font_family(p.font.clone())
            .text_size(px(13.0))
            .text_color(p.foreground)
            .when(!p.glass, |this| this.bg(p.page))
            .child(header)
            .children(cards.into_iter().map(|card| div().pt(px(12.0)).child(card)))
            .children(back_bar)
            .child(if full_side {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .children(side.map(|side| div().pt(px(8.0)).flex_1().min_h_0().child(side)))
            } else {
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(0.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .pr(px(5.0))
                            .overflow_hidden()
                            .child(main),
                    )
                    .children(divider)
                    .children(side.map(|side| {
                        div()
                            .pt(px(14.0))
                            .h_full()
                            .w(px(side_width))
                            .flex_none()
                            .child(side)
                    }))
            })
            .child(
                gpui::canvas(move |area, _, _| bounds.set(Some(area)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .into_any_element()
    }
}
