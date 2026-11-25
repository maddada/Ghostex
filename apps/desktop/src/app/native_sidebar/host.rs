//! The docked sidebar's own GPUI view, so a redraw that is not the sidebar's reuses its last frame.

use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, WeakEntity, Window, div,
};

use gpui::prelude::FluentBuilder as _;

use crate::GhostexGpuiApp;

/// CDXC:Sidebar 2026-09-21 WHY:
/// The sidebar was drawn inline by the window root, so every chat redraw (a streamed token, a transcript scroll tick, a composer keystroke) rebuilt and laid out the whole sidebar as well.
/// As a cached view it is reused on those frames. Its state and listeners stay on `GhostexGpuiApp`, and it observes that entity, so every app notify redraws it in the same frame and nothing the sidebar reads can go stale: a focus move still repaints its highlight with the list update that caused it.
/// Moving sidebar state into this view so store updates notify it alone was weighed and left out: measured after the chat views were cached, the root and sidebar renders together cost under two percent of the main thread.
pub(crate) struct NativeSidebarHost {
    app: WeakEntity<GhostexGpuiApp>,
    _app_changed: Subscription,
}

impl NativeSidebarHost {
    fn new(app: &Entity<GhostexGpuiApp>, cx: &mut Context<Self>) -> Self {
        Self {
            app: app.downgrade(),
            _app_changed: cx.observe(app, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for NativeSidebarHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.app.upgrade() {
            Some(app) => app.update(cx, |app, cx| app.render_native_sidebar(window, cx)),
            None => div().into_any_element(),
        }
    }
}

impl GhostexGpuiApp {
    /// The docked sidebar as the body row draws it: as is in the Framed work area layout, and in
    /// the Floating sidebar layout as the card (its top gutter is the body row's), or, while the
    /// rail is shown, as the card's right half, the rail being its left (`render_rail_card`).
    pub(crate) fn render_sidebar_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let sidebar = self.render_docked_native_sidebar(cx);
        if !crate::app::helpers::floating_sidebar_layout() {
            return sidebar;
        }
        let joined = self.sidebar_rail_shown();
        floating_card(sidebar, CardPart::new(!joined, true, true))
    }

    /// The rail in the Floating sidebar layout: the left half of the card the docked sidebar
    /// finishes, or, with the sidebar hidden, a card of its own.
    ///
    /// CDXC:Workarea 2026-11-25 DECISION:
    /// User, of the rail on the flat back beside the floating sidebar card: "mergee these both". The rail and the sidebar are one card: the rail draws its left half and the sidebar its right half, side by side with one background, so they read as one floating surface; with the sidebar hidden the rail is a card on its own.
    pub(crate) fn render_rail_card(&self, rail: AnyElement, sidebar_shown: bool) -> AnyElement {
        floating_card(rail, CardPart::new(true, !sidebar_shown, false))
    }

    pub(crate) fn render_docked_native_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.entity();
        let host = self
            .native_sidebar
            .host
            .get_or_insert_with(|| cx.new(|cx| NativeSidebarHost::new(&app, cx)))
            .clone();
        gpui::AnyView::from(host)
            .cached(gpui::StyleRefinement::default().size_full())
            .into_any_element()
    }
}

/// Which sides of the floating card one part draws: its left edge (rounded, outlined and one gutter
/// in from the window) and its right edge (rounded and outlined), and whether it fills the width it
/// is given (the sidebar's part) or keeps its content's (the rail's).
#[derive(Clone, Copy)]
struct CardPart {
    left: bool,
    right: bool,
    fill: bool,
}

impl CardPart {
    fn new(left: bool, right: bool, fill: bool) -> Self {
        Self { left, right, fill }
    }
}

/// `content` as part of the Floating sidebar layout's card: one gutter in from the window's left
/// (when it holds the card's left edge) and bottom edges, rounded and outlined on the sides it
/// holds, dark frosted glass over an artwork, the sidebar's solid colour otherwise
/// (CDXC:Workarea 2026-11-25 on `frosted_glass_backdrop` in helpers/window_art.rs).
fn floating_card(content: AnyElement, part: CardPart) -> AnyElement {
    use crate::app::helpers::{
        WORK_AREA_FRAME_BORDER, WORK_AREA_FRAME_MARGIN, WORK_AREA_FRAME_RADIUS,
        chrome_uses_light_appearance, frosted_glass_backdrop, sidebar_chrome_fill,
        work_area_frame_border_color,
    };
    let margin = gpui::px(WORK_AREA_FRAME_MARGIN);
    let radius = |on: bool| gpui::px(if on { WORK_AREA_FRAME_RADIUS } else { 0.0 });
    let radii = gpui::Corners {
        top_left: radius(part.left),
        bottom_left: radius(part.left),
        top_right: radius(part.right),
        bottom_right: radius(part.right),
    };
    let glass = frosted_glass_backdrop(radii, chrome_uses_light_appearance());
    let solid = glass.is_none();
    let border = gpui::px(WORK_AREA_FRAME_BORDER);
    let mut card = div()
        .relative()
        .h_full()
        .flex()
        .flex_col()
        .border_t(border)
        .border_b(border)
        .border_color(work_area_frame_border_color())
        .rounded_tl(radii.top_left)
        .rounded_bl(radii.bottom_left)
        .rounded_tr(radii.top_right)
        .rounded_br(radii.bottom_right)
        .when(part.left, |card| card.border_l(border))
        .when(part.right, |card| card.border_r(border))
        .when(part.fill, |card| card.flex_1())
        .when(solid, |card| card.bg(sidebar_chrome_fill(false, 180.0)))
        .overflow_hidden();
    // The glass gutter leaves this part's shape out (element/work_area_frame.rs).
    let marker_part = if part.fill { 1 } else { 0 };
    card = card.children(glass).child(content).child(
        crate::app::element::work_area_frame_sidebar_card_marker(marker_part, radii),
    );
    div()
        .h_full()
        .flex()
        .when(part.fill, |wrapper| wrapper.size_full())
        .when(part.left, |wrapper| wrapper.pl(margin))
        .pb(margin)
        .child(card)
        .into_any_element()
}
