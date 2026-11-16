//! The page window's top band and its round controls, in points. Plain constants with no `crate::*`,
//! so `native_modal_demo` can include this file for its `page-window-band` preview.

/// The band over the page's top edge that holds the floating controls and moves the window
/// (CDXC:SessionChat 2026-11-04 DECISION in visual_page_modal.rs).
pub(crate) const TOP_BAND_HEIGHT: f32 = 36.0;
/// The floating controls' diameter, and how far they sit from the window's top edge and from its
/// side edge, the same both ways.
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User, of the page window's round buttons sitting closer to the top edge than to the right edge: "make them have padding from top same as from right". The controls hang from the band's top at `CONTROL_INSET` and keep that inset from the side they sit on (the left with the close button first when Close buttons on the right is off, CDXC:AppModal 2026-11-10 in window/round_close_button.rs). They used to be centred in the 36pt band, 5pt from the top and 10pt from the side.
pub(crate) const CONTROL_SIZE: f32 = 26.0;
pub(crate) const CONTROL_INSET: f32 = 8.0;
