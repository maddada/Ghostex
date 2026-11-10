//! The chat's repeating indicators drawn by the pane that holds the chat, over it, instead of inside
//! the chat's own views. The chat keeps each indicator's place (an empty box of its size) and
//! records where it painted; the pane draws the moving part there, right after the cached chat.
//!
//! CDXC:SessionChat 2026-11-10 WHY:
//! Measured on the user's running app with agents working: the working card's spark asked for its frames from `ComposerHost`, which sits inside the cached chat view, and notifying a view marks every view around it dirty, so thirty times a second the window's root, the chat's chrome and the whole composer were built and laid out again, about 60% of a full frame's cost for as long as an agent worked (the running tool call's shimmer and the status row's spark did the same through the chat's animation-only frames). Drawn by the pane's element in the window root, a tick notifies only the root: the root renders on every frame anyway, and the chat, its composer and its transcript are replayed from cache. The rate stays thirty (the 2026-09-25 WHY in helpers/indicator_animation.rs) and the indicators look as they did: each is drawn at its place's current bounds, inside the clip its place had (the capsule's rounded edge included) and the pane's.
//! A chat drawn by something that does not host the overlay (a page with no `hosted_chat`, the first frame of a new chat) draws its indicators in place as before; `IndicatorOverlay::hosted` says which.

use super::state::NativeChatView;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, EntityId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, Styled as _, Window,
    div, point, px, size,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::{Rc, Weak},
};

/// Builds one indicator layer for the frame being drawn: an element that places itself (it takes
/// no room and draws at window coordinates it works out in its prepaint).
pub(super) type Layer = Rc<dyn Fn() -> AnyElement>;

/// The working card's spark or activity lead (working_card.rs).
pub(super) const WORKING_LEAD: &str = "working-lead";
/// The working card's compaction track (working_card.rs).
pub(super) const WORKING_TRACK: &str = "working-track";
/// The status row's spark (status_tail.rs).
pub(super) const STATUS_TAIL_LEAD: &str = "status-tail-lead";
/// The running tool calls' shimmer and spinner (row_motion.rs).
pub(super) const ROW_MOTION: &str = "row-motion";

thread_local! {
    /// Each chat view's overlay, for the pane that draws the chat.
    static OVERLAYS: RefCell<HashMap<EntityId, Weak<IndicatorOverlay>>> = RefCell::new(HashMap::new());
}

/// One chat's lifted indicators.
#[derive(Default)]
pub(crate) struct IndicatorOverlay {
    /// A pane that draws the overlay holds the chat in the frame being drawn. Set when the pane
    /// renders (before the chat does), cleared once the overlay has drawn.
    hosted: Cell<bool>,
    /// The window draws views again from last frame (Faster rendering): nothing is lifted.
    retained: Cell<bool>,
    layers: RefCell<BTreeMap<&'static str, Layer>>,
}

impl IndicatorOverlay {
    /// Lets the pane that draws the chat `chat` find this overlay; call it from the chat's render.
    pub(super) fn register(self: &Rc<Self>, chat: EntityId) {
        OVERLAYS.with_borrow_mut(|overlays| {
            overlays.retain(|_, overlay| overlay.strong_count() > 0);
            overlays.insert(chat, Rc::downgrade(self));
        });
    }

    /// Whether this frame's indicators are drawn by the pane: lift them, else draw them in place.
    pub(super) fn hosted(&self) -> bool {
        self.hosted.get()
    }

    /// Sets (or, with `None`, removes) the layer `key`. Each layer is set by the render that
    /// draws its place, every time that render runs.
    pub(super) fn set(&self, key: &'static str, layer: Option<Layer>) {
        let mut layers = self.layers.borrow_mut();
        match layer {
            Some(layer) => {
                layers.insert(key, layer);
            }
            None => {
                layers.remove(key);
            }
        }
    }
}

/// The chat view `chat_view`'s element `chat` as its pane draws it, with the chat's lifted
/// indicators drawn over it, inside the pane's clip.
pub(crate) fn hosted_chat(chat_view: EntityId, chat: AnyElement) -> AnyElement {
    let overlay = OVERLAYS.with_borrow(|overlays| overlays.get(&chat_view).and_then(Weak::upgrade));
    if let Some(overlay) = &overlay {
        overlay.hosted.set(true);
    }
    div()
        .size_full()
        .relative()
        .child(chat)
        .children(overlay.map(|overlay| OverlayLayers {
            overlay,
            drawn: Vec::new(),
        }))
        .into_any_element()
}

/// Takes no room; lays out and paints the chat's layers after the chat has drawn this frame.
struct OverlayLayers {
    overlay: Rc<IndicatorOverlay>,
    drawn: Vec<AnyElement>,
}

impl Element for OverlayLayers {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        Some("chat-indicator-overlay".into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = gpui::Style {
            position: gpui::Position::Absolute,
            ..Default::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layers: Vec<Layer> = self.overlay.layers.borrow().values().cloned().collect();
        self.drawn.clear();
        for layer in layers {
            let mut element = layer();
            element.prepaint_as_root(
                point(px(0.0), px(0.0)),
                size(AvailableSpace::MinContent, AvailableSpace::MinContent),
                window,
                cx,
            );
            self.drawn.push(element);
        }
        self.overlay.hosted.set(false);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for element in &mut self.drawn {
            element.paint(window, cx);
        }
    }
}

impl IntoElement for OverlayLayers {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Where a lifted indicator's place painted, with the clip it was painted in, and, for a place
/// inside a fully rounded box that clips it (the working card's capsule), that box's padding edge.
#[derive(Clone, Default)]
pub(super) struct LiftedPlace {
    slot: Rc<Cell<Option<(Bounds<Pixels>, Bounds<Pixels>)>>>,
    rounded_box: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl LiftedPlace {
    /// The element that records where the place paints: put it, absolute and filling, inside the
    /// place's empty box.
    pub(super) fn recorder(&self) -> AnyElement {
        let slot = self.slot.clone();
        gpui::canvas(
            move |bounds, window, _| slot.set(Some((bounds, window.content_mask().bounds))),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    /// The element that records the fully rounded box's padding edge: put it, absolute and
    /// filling, inside that box.
    pub(super) fn box_recorder(&self) -> AnyElement {
        let rounded_box = self.rounded_box.clone();
        gpui::canvas(
            move |bounds, _, _| rounded_box.set(Some(bounds)),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    /// `child` drawn over the place, as a layer element.
    pub(super) fn draw(&self, child: AnyElement) -> AnyElement {
        LiftedAt {
            place: self.clone(),
            child: Some(child),
            clip: None,
        }
        .into_any_element()
    }
}

/// Takes no room; lays out and paints `child` at the place's bounds, inside its clip (and inside
/// its rounded box's corners).
struct LiftedAt {
    place: LiftedPlace,
    child: Option<AnyElement>,
    clip: Option<Bounds<Pixels>>,
}

impl Element for LiftedAt {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = gpui::Style {
            position: gpui::Position::Absolute,
            ..Default::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.clip = None;
        let Some((slot, mask)) = self.place.slot.get() else {
            return;
        };
        let Some(child) = self.child.take() else {
            return;
        };
        let clip = mask.intersect(&window.content_mask().bounds);
        if clip.size.width <= Pixels::ZERO || clip.size.height <= Pixels::ZERO {
            return;
        }
        // Inside a rounded box, the box itself clips the child at its corners, as it did in place.
        let (origin, extent, mut element) = match self.place.rounded_box.get() {
            Some(rounded) => (
                rounded.origin,
                rounded.size,
                div()
                    .w(rounded.size.width)
                    .h(rounded.size.height)
                    .rounded_full()
                    .overflow_hidden()
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .left(slot.origin.x - rounded.origin.x)
                            .top(slot.origin.y - rounded.origin.y)
                            .w(slot.size.width)
                            .h(slot.size.height)
                            .child(child),
                    )
                    .into_any_element(),
            ),
            None => (slot.origin, slot.size, child),
        };
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            element.prepaint_as_root(origin, extent.map(AvailableSpace::Definite), window, cx);
        });
        self.child = Some(element);
        self.clip = Some(clip);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (Some(clip), Some(child)) = (self.clip, self.child.as_mut()) else {
            return;
        };
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            child.paint(window, cx);
        });
    }
}

impl IntoElement for LiftedAt {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl NativeChatView {
    /// Lets the pane find this chat's overlay (`chat` is this view), and says whether the pane
    /// draws the chat's indicators this frame: it holds the chat, and nothing the chat draws over
    /// its own content is showing (the overlay would draw over it). The suggestions popup and the
    /// menus are windows of their own, above both; the popup's shadow is painted in the pane.
    pub(super) fn indicators_lifted(&self, chat: EntityId) -> bool {
        self.indicator_overlay.register(chat);
        self.indicator_overlay.hosted()
            && !self.indicator_overlay.retained.get()
            && !self.suggestion_popup_open()
            && !self.snapshot["accountSwitchCard"].is_object()
            && !self.snapshot["subagent"].is_object()
    }
}

impl NativeChatView {
    /// Decides, before the rows are drawn, whether they draw their moving parts (a running call's
    /// shimmer and spinner, the status row's spark) inside themselves this frame, as they did
    /// before those were drawn over the rows; true when they do.
    ///
    /// CDXC:SessionChat 2026-11-10 WHY:
    /// Two cases found in review of the overlays. Without window glass the "Press Escape again to interrupt" toast (scroll_bottom.rs) is drawn inside the cached transcript after the rows, and anything drawn after the transcript (the pane's overlay, or the chat's own layer) painted over it: a running call's label on the last row showed through the toast. While the toast shows the rows draw their motion themselves, under it; the toast is in the snapshot, so its coming and going draws the rows again.
    /// With Faster rendering on, GPUI Fast draws a view that is dirty only because a view nested in it is again from last frame, rebuilding only the nested view (`.dependencies/zed/crates/gpui/src/fast/splice.rs`, `splice_layout`). A redraw of the transcript alone (a scroll, a hover) then copied the window root's and the chat's last prepaint, the overlay with it, so it stayed where it was while the rows moved, and the chat, not rendered by the root that frame, could draw its indicators in place as well. Under that setting nothing is lifted and every indicator animates inside its row or the composer, as before; each such tick rebuilds only the view it is in, which is what that setting is for.
    pub(super) fn sync_motion_in_place(&mut self, window: &Window) -> bool {
        let retained = window.view_retention();
        self.indicator_overlay.retained.set(retained);
        let glass = crate::app::helpers::window_glass_active_for(self.main_window);
        let in_place = retained || (!glass && self.pill_toast().is_some());
        self.row_motion.set_in_row(in_place);
        in_place
    }
}
