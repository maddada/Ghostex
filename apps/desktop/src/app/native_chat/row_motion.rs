//! A running tool call's shimmer and spinner, drawn over the cached transcript rows rather than in
//! them. The row keeps each animated part's place (the label laid out in clear text, the spinner's
//! box empty) and records where it painted; the chat draws the moving part over that place after
//! the transcript (`render_row_motion`), as the status row's spark does (status_tail.rs).
//!
//! CDXC:SessionChat 2026-11-10 WHY:
//! Measured on the user's running app with agents working: 91% of the main thread's busy time was in display frames, and the transcript's rows were built, laid out and painted again on nearly every one of them, because the shimmer and spinner on a running tool call (tool_card.rs, tool_run.rs) animated inside the cached transcript view and notified it thirty times a second. That breaks the rule the 2026-10-18 WHY in working_card.rs and the 2026-11-07 WHY in status_tail.rs state: nothing in a transcript row may animate. Drawn here, a tick is one of the chat's animation-only frames (helpers/indicator_animation.rs), so only the label and the spinner are drawn again and the rows stay cached. The look and the thirty frames a second stay; the shimmer on running calls is the user's 2026-11-09 decision in gx-chat-core `transcript/tool_card.rs`.
//! Each part follows its row's current place in the list (`ListState::bounds_for_item`), so scrolling, a fold or a row changing height moves it with the row, and it is clipped to what clipped its place in the row (the card, a fold opening, the transcript's visible area). A fade around the row (a call growing in, a fold opening, the transcript arriving) is applied through `opacity_scope`, since GPUI does not tell an element its ancestors' opacity. A row the main list does not hold (the subagent viewer's, which the chat draws itself) animates in place as before.

use super::{
    appearance::ChatAppearance,
    state::NativeChatView,
    tool_card::{SHIMMER, SPIN, shimmer_text},
};
use crate::app::helpers::ThrottledAnimationExt as _;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Div, Element, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, SharedString,
    Styled as _, TextStyle, TextStyleRefinement, Window, canvas, div, px,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

thread_local! {
    /// The product of the `opacity_scope`s around the element being prepainted.
    static SCOPED_OPACITY: Cell<f32> = const { Cell::new(1.0) };
}

/// What a part draws.
#[derive(Clone)]
enum MotionKind {
    /// The label with the tool card's band of light sweeping across it (`shimmer_text`).
    Shimmer {
        label: SharedString,
        font: SharedString,
        dim: Hsla,
        bright: Hsla,
    },
    /// The spinner glyph turning.
    Spin { color: Hsla },
}

/// Where a part's place painted: its bounds, the clip it was painted in, the text style its label
/// was laid out with and the opacity of the fades around it. Window pixels when measured, offsets
/// from the row's origin once placed.
#[derive(Clone)]
struct Place {
    bounds: Bounds<Pixels>,
    mask: Bounds<Pixels>,
    text: TextStyleRefinement,
    opacity: f32,
}

struct Part {
    /// The main list's row the part is in.
    row: usize,
    kind: MotionKind,
    /// The row was drawn again and still holds the part. A row drawn again without it drops it.
    held: bool,
    /// Measured by the place's latest prepaint, not yet turned into offsets from the row.
    measured: Option<Place>,
    placed: Option<Place>,
}

/// The parts the transcript's rows hold, keyed by element id.
#[derive(Default)]
pub(crate) struct RowMotion {
    parts: Rc<RefCell<HashMap<SharedString, Part>>>,
    /// The main list's row being drawn, while one is.
    drawing: Cell<Option<usize>>,
}

impl RowMotion {
    /// The main list's row `row` is being drawn: the parts it held wait for it to hold them again.
    pub(super) fn start_row(&self, row: usize) {
        self.drawing.set(Some(row));
        for part in self.parts.borrow_mut().values_mut() {
            if part.row == row {
                part.held = false;
            }
        }
    }

    /// The row being drawn is done.
    pub(super) fn end_row(&self) {
        self.drawing.set(None);
    }

    /// Registers `id` in the row being drawn and returns the element that records where its place
    /// paints, or `None` outside the main list's rows.
    fn hold(&self, id: &SharedString, kind: MotionKind) -> Option<AnyElement> {
        let row = self.drawing.get()?;
        let mut parts = self.parts.borrow_mut();
        match parts.get_mut(id) {
            Some(part) => {
                part.row = row;
                part.kind = kind;
                part.held = true;
            }
            None => {
                parts.insert(
                    id.clone(),
                    Part {
                        row,
                        kind,
                        held: true,
                        measured: None,
                        placed: None,
                    },
                );
            }
        }
        drop(parts);
        let (parts, id) = (self.parts.clone(), id.clone());
        Some(
            canvas(
                move |bounds, window, _| {
                    if let Some(part) = parts.borrow_mut().get_mut(&id) {
                        part.measured = Some(Place {
                            bounds,
                            mask: window.content_mask().bounds,
                            text: text_layout(&window.text_style()),
                            opacity: SCOPED_OPACITY.get(),
                        });
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        )
    }
}

/// What lays a label out (its runs carry the font and colours): the size, line and wrapping.
fn text_layout(style: &TextStyle) -> TextStyleRefinement {
    TextStyleRefinement {
        font_size: Some(style.font_size),
        line_height: Some(style.line_height),
        font_smoothing: Some(style.font_smoothing),
        white_space: Some(style.white_space),
        text_overflow: style.text_overflow.clone(),
        text_align: Some(style.text_align),
        line_clamp: style.line_clamp,
        ..Default::default()
    }
}

impl MotionKind {
    /// The part as drawn over its place, `size`, at `opacity`.
    fn element(&self, id: SharedString, size: gpui::Size<Pixels>, opacity: f32) -> AnyElement {
        match self.clone() {
            MotionKind::Shimmer {
                label,
                font,
                dim,
                bright,
            } => div()
                .w(size.width)
                .h(size.height)
                .overflow_hidden()
                .opacity(opacity)
                .with_throttled_animation(id, SHIMMER, move |title, phase| {
                    title.child(shimmer_text(&label, &font, phase, dim, bright))
                })
                .into_any_element(),
            MotionKind::Spin { color } => gpui::svg()
                .path("titlebar/loader2.svg")
                .w(size.width)
                .h(size.height)
                .text_color(color)
                .opacity(opacity)
                .with_throttled_animation(id, SPIN, |glyph, progress| {
                    glyph.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                        progress,
                    )))
                })
                .into_any_element(),
        }
    }
}

impl NativeChatView {
    /// A running call's label in `container` with the shimmer sweeping across it.
    pub(super) fn running_shimmer(
        &self,
        id: SharedString,
        container: Div,
        label: String,
        p: &ChatAppearance,
    ) -> AnyElement {
        let (dim, bright) = (p.muted.opacity(0.8), p.foreground);
        let font = SharedString::from(p.font.clone());
        let kind = MotionKind::Shimmer {
            label: label.clone().into(),
            font: font.clone(),
            dim,
            bright,
        };
        match self.row_motion.hold(&id, kind) {
            // The label laid out exactly as drawn, in clear text, so its place has its size.
            Some(place) => {
                let clear = gpui::transparent_black();
                container
                    .relative()
                    .child(shimmer_text(&label, &font, 0.0, clear, clear))
                    .child(place)
                    .into_any_element()
            }
            None => container
                .with_throttled_animation(id, SHIMMER, move |title, phase| {
                    title.child(shimmer_text(&label, &font, phase, dim, bright))
                })
                .into_any_element(),
        }
    }

    /// A running call's spinner, `size` square.
    pub(super) fn running_spinner(&self, id: SharedString, size: f32, color: Hsla) -> AnyElement {
        match self.row_motion.hold(&id, MotionKind::Spin { color }) {
            Some(place) => div()
                .relative()
                .size(px(size))
                .flex_shrink_0()
                .child(place)
                .into_any_element(),
            None => gpui::svg()
                .path("titlebar/loader2.svg")
                .size(px(size))
                .flex_shrink_0()
                .text_color(color)
                .with_throttled_animation(id, SPIN, |glyph, progress| {
                    glyph.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                        progress,
                    )))
                })
                .into_any_element(),
        }
    }

    /// The running calls' parts over the transcript's rows. Placed after the transcript, so a row
    /// painted this frame has recorded where its parts go.
    pub(super) fn render_row_motion(&self) -> Option<AnyElement> {
        if self.list.item_count() == 0 {
            return None;
        }
        Some(
            RowMotionLayer {
                parts: self.row_motion.parts.clone(),
                list: self.list.clone(),
                drawn: Vec::new(),
            }
            .into_any_element(),
        )
    }
}

/// Takes no room where it sits, and lays out and paints each part the rows hold at its place in its
/// row's current bounds, inside the clip its place had and the list's visible area.
struct RowMotionLayer {
    parts: Rc<RefCell<HashMap<SharedString, Part>>>,
    list: gpui::ListState,
    /// The parts prepainted this frame, with their clip and text style, for their paint.
    drawn: Vec<(AnyElement, Bounds<Pixels>, TextStyleRefinement)>,
}

impl Element for RowMotionLayer {
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
        let rows = self.list.item_count();
        let visible = self
            .list
            .viewport_bounds()
            .intersect(&window.content_mask().bounds);
        let mut shown = Vec::new();
        {
            let mut parts = self.parts.borrow_mut();
            parts.retain(|_, part| part.held && part.row < rows);
            // Sorted, so parts that overlap draw in the same order every frame.
            let mut ids: Vec<_> = parts.keys().cloned().collect();
            ids.sort();
            for id in ids {
                let part = parts.get_mut(&id).expect("listed above");
                let Some(row) = self.list.bounds_for_item(part.row) else {
                    continue;
                };
                if let Some(measured) = part.measured.take() {
                    let inside = |bounds: Bounds<Pixels>| Bounds {
                        origin: bounds.origin - row.origin,
                        size: bounds.size,
                    };
                    part.placed = Some(Place {
                        bounds: inside(measured.bounds),
                        mask: inside(measured.mask),
                        ..measured
                    });
                }
                let Some(place) = part.placed.clone() else {
                    continue;
                };
                let at_row = |bounds: Bounds<Pixels>| Bounds {
                    origin: bounds.origin + row.origin,
                    size: bounds.size,
                };
                let clip = at_row(place.mask).intersect(&visible);
                if clip.size.width <= Pixels::ZERO || clip.size.height <= Pixels::ZERO {
                    continue;
                }
                let bounds = at_row(place.bounds);
                shown.push((
                    part.kind.element(id, bounds.size, place.opacity),
                    bounds,
                    clip,
                    place.text,
                ));
            }
        }
        self.drawn.clear();
        for (mut element, bounds, clip, text) in shown {
            window.with_text_style(Some(text.clone()), |window| {
                window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                    element.prepaint_as_root(
                        bounds.origin,
                        bounds.size.map(AvailableSpace::Definite),
                        window,
                        cx,
                    );
                })
            });
            self.drawn.push((element, clip, text));
        }
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
        for (element, clip, text) in &mut self.drawn {
            window.with_text_style(Some(text.clone()), |window| {
                window.with_content_mask(Some(ContentMask { bounds: *clip }), |window| {
                    element.paint(window, cx);
                })
            });
        }
    }
}

impl IntoElement for RowMotionLayer {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// `child`, whose own style fades it to `opacity`, telling the running calls' parts inside it
/// (`RowMotion`) that fade, so the parts drawn over the rows fade with them. `opacity` is read at
/// prepaint, so a `with_animation` closure may set it during layout.
pub(super) fn opacity_scope(opacity: Rc<Cell<f32>>, child: AnyElement) -> AnyElement {
    OpacityScope { opacity, child }.into_any_element()
}

/// [`opacity_scope`] at a fixed opacity.
pub(super) fn fixed_opacity_scope(opacity: f32, child: AnyElement) -> AnyElement {
    opacity_scope(Rc::new(Cell::new(opacity)), child)
}

struct OpacityScope {
    opacity: Rc<Cell<f32>>,
    child: AnyElement,
}

impl Element for OpacityScope {
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
        (self.child.request_layout(window, cx), ())
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
        let outer = SCOPED_OPACITY.get();
        SCOPED_OPACITY.set(outer * self.opacity.get());
        self.child.prepaint(window, cx);
        SCOPED_OPACITY.set(outer);
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
        self.child.paint(window, cx);
    }
}

impl IntoElement for OpacityScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
