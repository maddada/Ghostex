//! Shared GPUI elements from the desktop's `element/`; the native-only ones wrap CEF surfaces and window corners.
pub(crate) mod clip_above;
pub(crate) mod count_badge;
#[allow(unused_imports)]
pub(crate) use clip_above::*;
#[allow(unused_imports)]
pub(crate) use count_badge::*;

/// The desktop's glass gutter marker for the Floating sidebar card (element/work_area_frame.rs); the
/// browser draws no frame, so it marks nothing.
pub(crate) fn work_area_frame_sidebar_card_marker(
    _part: u8,
    _radii: gpui::Corners<gpui::Pixels>,
) -> impl gpui::IntoElement {
    gpui::Empty
}
