//! The scrolling body every Settings page renders into (`SettingsNativeScrollArea` +
//! `.settings-page-width`): the 924px column centered in the content area with 20px gutters,
//! 12px above each section and 24px between them, 80px below the last one, and the scroll
//! anchors the rail and deep links use.
use super::super::native_modal_kit::*;
use super::model::SettingsTabId;
use super::palette::SettingsPalette;
use super::shell::BODY_PADDING;
use super::store::{ScrollRestore, SettingsStore, SmoothScroll, scroll_top_for_child};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Bounds, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, point, px,
};
use gpui_component::v_flex;

/// `--settings-content-max-width`, 20% wider than the React 770px column so the page fills the
/// 20% bigger window (CDXC:Settings 2026-10-14 in app/model/app_modal_kind.rs) instead of
/// floating in it with empty bands on both sides.
pub(crate) const CONTENT_MAX_WIDTH: f32 = 924.0;
/// `--settings-content-inline-padding`.
pub(crate) const CONTENT_GUTTER: f32 = 20.0;

/// One child of a page: a section (with the anchor id the rail and deep links scroll to) or any
/// other block (a separator, the Reset to defaults button).
pub(crate) struct PageBlock {
    pub(crate) anchor: Option<String>,
    pub(crate) element: AnyElement,
    /// Space above it: 12px for a section (`.settings-list-section { margin-top: 0.75rem }`).
    pub(crate) margin_top: f32,
}

impl PageBlock {
    pub(crate) fn section(anchor: impl Into<String>, element: impl IntoElement) -> Self {
        Self {
            anchor: Some(anchor.into()),
            element: element.into_any_element(),
            margin_top: 12.0,
        }
    }

    pub(crate) fn plain(element: impl IntoElement) -> Self {
        Self {
            anchor: None,
            element: element.into_any_element(),
            margin_top: 0.0,
        }
    }
}

/// Space above the page title, below the search field.
const HEADER_TOP: f32 = 20.0;

/// The page's title with its one-line intro under it (`ghostex_settings_catalog::page_intro`),
/// above the first section, whose own extra top space it takes over.
///
/// CDXC:Settings 2026-11-08 DECISION:
/// User (Q1712 "Page title and line"): every Settings page shows its title with a one-line intro under it, no card; the lines are the catalog's (packages/settings-catalog/src/intros.rs).
fn with_page_header(
    tab: SettingsTabId,
    p: &SettingsPalette,
    blocks: Vec<PageBlock>,
) -> Vec<PageBlock> {
    let header = v_flex()
        .w_full()
        .px(px(2.0))
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(20.0))
                .line_height(px(26.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child(tab.title()),
        )
        .when_some(
            ghostex_settings_catalog::page_intro(tab.id()),
            |header, intro| {
                header.child(
                    div()
                        .text_size(px(13.5))
                        .line_height(px(19.0))
                        .text_color(hsla(p.muted))
                        .child(intro),
                )
            },
        );
    let mut blocks = blocks;
    // Pages add 20px over their first section's own 12px; the header takes that place now.
    if let Some(first) = blocks.first_mut() {
        first.margin_top = first.margin_top.min(12.0);
    }
    let mut out = Vec::with_capacity(blocks.len() + 1);
    out.push(PageBlock {
        anchor: None,
        element: header.into_any_element(),
        margin_top: HEADER_TOP,
    });
    out.extend(blocks);
    out
}

/// `getMostlyVisibleSettingsSectionId`: the anchor with the most height in the viewport, ties to
/// the one nearest the viewport's center.
fn mostly_visible(
    anchors: &[(String, Bounds<Pixels>)],
    viewport: Bounds<Pixels>,
) -> Option<String> {
    let viewport_center = f32::from(viewport.origin.y) + f32::from(viewport.size.height) / 2.0;
    let mut best: Option<(f32, f32, &String)> = None;
    for (id, bounds) in anchors {
        let top = f32::from(bounds.origin.y);
        let bottom = top + f32::from(bounds.size.height);
        let visible = (bottom.min(f32::from(viewport.origin.y + viewport.size.height))
            - top.max(f32::from(viewport.origin.y)))
        .max(0.0);
        if visible <= 0.0 {
            continue;
        }
        let distance = ((top + bottom) / 2.0 - viewport_center).abs();
        let better = match best {
            None => true,
            Some((best_visible, best_distance, _)) => {
                visible > best_visible || (visible == best_visible && distance < best_distance)
            }
        };
        if better {
            best = Some((visible, distance, id));
        }
    }
    best.map(|(_, _, id)| id.clone())
}

/// The page body: registers `blocks`' anchors with the store, restores the page's remembered
/// scroll position on its first frame, applies a pending section scroll once the sections are
/// laid out, and after each scroll reports it and the section mostly in view.
///
/// CDXC:Settings 2026-10-08 WHY:
/// The scrollbar is the scroll area's sibling, never its child. GPUI offsets every child of a scrolling element by the scroll offset, so the bar `.vertical_scrollbar` mounted inside the scrolling div moved its own track with the content: a thumb drag fed that shift back into the offset and lost the pointer, and once scrolled the bar drew outside the viewport. Every other modal already mounts it beside the scroll area.
pub(crate) fn settings_page(
    store: &Entity<SettingsStore>,
    tab: SettingsTabId,
    p: &SettingsPalette,
    blocks: Vec<PageBlock>,
    cx: &mut App,
) -> AnyElement {
    let blocks = with_page_header(tab, p, blocks);
    let (handle, tracker) = store.update(cx, |store, _| {
        (store.scroll_handle(tab), store.tracker(tab))
    });
    let anchors: Vec<Option<String>> = blocks.iter().map(|block| block.anchor.clone()).collect();
    tracker.borrow_mut().children = anchors.clone();
    let store_for_prepaint = store.clone();
    let prepaint_handle = handle.clone();
    let children = blocks.into_iter().map(|block| {
        div()
            .w_full()
            .max_w(px(CONTENT_MAX_WIDTH))
            .flex_shrink_0()
            .when(block.margin_top > 0.0, |this| this.mt(px(block.margin_top)))
            .child(block.element)
    });
    let column = v_flex()
        .w_full()
        .items_center()
        .pl(px(CONTENT_GUTTER))
        // The scroll area reaches over the window's right padding (see `modal_edge_scrollbar`).
        .pr(px(CONTENT_GUTTER + BODY_PADDING))
        .pb(px(80.0))
        .gap(px(24.0))
        .on_children_prepainted(
            move |bounds: Vec<Bounds<Pixels>>, window: &mut Window, cx: &mut App| {
                let viewport = prepaint_handle.bounds();
                let offset = prepaint_handle.offset();
                let restore =
                    store_for_prepaint.update(cx, |store, _| store.take_scroll_restore(tab));
                if let Some(top) = restore.filter(|top| *top > 0.0) {
                    tracker.borrow_mut().restoring = Some(ScrollRestore::new(top));
                }
                let restoring = tracker.borrow().restoring;
                if let Some(mut restore) = restoring {
                    let current = -f32::from(offset.y);
                    let user_scrolled = restore
                        .last
                        .is_some_and(|last| (current - last).abs() > 0.5);
                    if user_scrolled || tracker.borrow().pending_scroll.is_some() {
                        // The wheel, a rail click or a deep link takes over.
                        tracker.borrow_mut().restoring = None;
                    } else {
                        let max = f32::from(prepaint_handle.max_offset().y).max(0.0);
                        let target = restore.top.min(max);
                        let reached = max + 0.5 >= restore.top;
                        restore.last = Some(target);
                        let waiting = !reached && std::time::Instant::now() < restore.until;
                        let mut tracker = tracker.borrow_mut();
                        tracker.restoring = waiting.then_some(restore);
                        tracker.last_offset = Some(-target);
                        drop(tracker);
                        if (current - target).abs() > 0.5 {
                            prepaint_handle.set_offset(point(offset.x, px(-target)));
                            window.refresh();
                        }
                        return;
                    }
                }
                let pending = tracker.borrow_mut().pending_scroll.take();
                if let Some(section) = pending
                    && let Some(child) = anchors
                        .iter()
                        .position(|anchor| anchor.as_deref() == Some(section.as_str()))
                        .and_then(|index| bounds.get(index))
                {
                    // Rail clicks, search results and deep links all `scrollIntoView` smoothly.
                    let from = -f32::from(offset.y);
                    let to = scroll_top_for_child(&prepaint_handle, *child);
                    tracker.borrow_mut().smooth = Some(SmoothScroll::new(from, to));
                    window.request_animation_frame();
                }
                let smooth = tracker.borrow().smooth;
                if let Some(mut smooth) = smooth {
                    let current = -f32::from(prepaint_handle.offset().y);
                    if (current - smooth.last).abs() > 0.5 {
                        // The user scrolled over it: the wheel wins, as in Chromium.
                        tracker.borrow_mut().smooth = None;
                    } else {
                        let (top, done) = smooth.sample();
                        prepaint_handle.set_offset(point(offset.x, px(-top)));
                        smooth.last = top;
                        tracker.borrow_mut().smooth = (!done).then_some(smooth);
                        window.request_animation_frame();
                    }
                }
                // `bounds` were laid out at `offset`; an offset set above shows next frame.
                let offset_y = f32::from(offset.y);
                let scrolled = tracker
                    .borrow()
                    .last_offset
                    .is_some_and(|last| (last - offset_y).abs() > 0.5);
                tracker.borrow_mut().last_offset = Some(offset_y);
                if !scrolled {
                    return;
                }
                let visible: Vec<(String, Bounds<Pixels>)> = anchors
                    .iter()
                    .zip(bounds.iter())
                    .filter_map(|(anchor, bounds)| anchor.clone().map(|anchor| (anchor, *bounds)))
                    .collect();
                let active = mostly_visible(&visible, viewport);
                let changed = active.is_some() && tracker.borrow().active != active;
                store_for_prepaint.update(cx, |store, cx| {
                    store.page_scrolled(tab, cx);
                    if changed {
                        tracker.borrow_mut().active = active.clone();
                        cx.notify();
                    }
                });
            },
        )
        .children(children);
    div()
        .relative()
        .size_full()
        .child(
            div()
                .id(SharedString::from(format!("settings-page-{}", tab.id())))
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&handle)
                .child(column),
        )
        .child(modal_edge_scrollbar(&handle))
        .into_any_element()
}

/// The page's own hairline (`<Separator className='bg-border' />`).
pub(crate) fn page_separator(p: &SettingsPalette) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(hsla(p.hairline))
        .into_any_element()
}
