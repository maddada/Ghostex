//! Where the note composer goes beside the text it is about, kept free of the app so the rules
//! can be tested.
//!
//! CDXC:Docs 2026-11-02 DECISION:
//! User: "when i click "add a comment" on some text in the markdown editor in the files in the sidepanel we shouldn't make the modal that appears cover the actual text that im commenting on like this please". The composer (for a new comment and for editing one) opens below the commented text's last line with a small gap; when that does not fit in the document's visible area it opens above the first line; when neither fits the document scrolls once so the text and the composer both show; and when even that cannot fit (a selection taller than the view) the composer sits beside the text, and over it only when the view has no room beside it either. It stays inside the document's area and follows the text as the document scrolls. Placement uses the editor's laid-out line bounds, never a fixed line height, because the Files text zoom changes them.

use gpui::{Bounds, Pixels, Size, point, px};

/// The space between the commented text and the composer.
pub(crate) const COMPOSER_GAP: f32 = 8.0;
/// The space kept between the composer and the edges of the document's visible area.
pub(crate) const COMPOSER_MARGIN: f32 = 12.0;

/// Which side of the commented text the composer went to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ComposerSide {
    Below,
    Above,
    /// Left or right of the text, which is too tall to show with the composer above or below it.
    Beside,
    /// Over the text: no room anywhere else.
    Over,
}

/// Where the composer goes, and how far the document scrolls first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ComposerPlacement {
    /// The composer's frame, in window coordinates, after the scroll.
    pub(crate) frame: Bounds<Pixels>,
    pub(crate) side: ComposerSide,
    /// How far the document scrolls (positive moves its content up); 0 when it stays.
    pub(crate) scroll_by: f32,
}

/// The composer's place for `text` (the commented text's bounds, in window coordinates) inside
/// `visible` (the document's visible area). `may_scroll` allows the one scroll that brings the
/// text and the composer into view together; it is asked once, when the composer opens.
pub(crate) fn place_composer(
    text: Bounds<Pixels>,
    visible: Bounds<Pixels>,
    size: Size<Pixels>,
    may_scroll: bool,
) -> ComposerPlacement {
    let (gap, margin) = (COMPOSER_GAP, COMPOSER_MARGIN);
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let (view_top, view_bottom) = (
        f32::from(visible.top()) + margin,
        f32::from(visible.bottom()) - margin,
    );
    let (view_left, view_right) = (
        f32::from(visible.left()) + margin,
        f32::from(visible.right()) - margin,
    );
    // Text out of view (a note edited from the Annotations list) first scrolls into it: its top
    // to the view's top when it is above, its bottom to the view's bottom when it is below.
    let mut scroll_by = 0.0;
    if may_scroll {
        let (top, bottom) = (f32::from(text.top()), f32::from(text.bottom()));
        if top < f32::from(visible.top()) {
            scroll_by = top - view_top;
        } else if bottom > f32::from(visible.bottom()) {
            scroll_by = (bottom - view_bottom).min(top - view_top);
        }
    }
    let (text_top, text_bottom) = (
        f32::from(text.top()) - scroll_by,
        f32::from(text.bottom()) - scroll_by,
    );
    let clamp_x = |left: f32| left.min(view_right - width).max(view_left);
    let clamp_y = |top: f32| top.min(view_bottom - height).max(view_top);
    let centered = clamp_x(f32::from(text.center().x) - width / 2.0);
    let place = |left: f32, top: f32, side: ComposerSide, scroll_by: f32| ComposerPlacement {
        frame: Bounds::new(point(px(left), px(clamp_y(top))), size),
        side,
        scroll_by,
    };

    let below = text_bottom + gap;
    if below + height <= view_bottom {
        return place(centered, below, ComposerSide::Below, scroll_by);
    }
    let above = text_top - gap - height;
    if above >= view_top {
        return place(centered, above, ComposerSide::Above, scroll_by);
    }
    let needed = (text_bottom - text_top) + gap + height;
    if may_scroll && needed <= view_bottom - view_top {
        // Just enough more for the composer to fit below the text, which stays in view.
        let more = below + height - view_bottom;
        return place(
            centered,
            below - more,
            ComposerSide::Below,
            scroll_by + more,
        );
    }
    let (text_left, text_right) = (f32::from(text.left()), f32::from(text.right()));
    let top = text_top.max(view_top);
    if text_right + gap + width <= view_right {
        return place(text_right + gap, top, ComposerSide::Beside, scroll_by);
    }
    if text_left - gap - width >= view_left {
        return place(
            text_left - gap - width,
            top,
            ComposerSide::Beside,
            scroll_by,
        );
    }
    place(centered, below, ComposerSide::Over, scroll_by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn bounds(left: f32, top: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(left), px(top)), size(px(width), px(height)))
    }

    const COMPOSER: (f32, f32) = (360.0, 202.0);

    fn place(text: Bounds<Pixels>, visible: Bounds<Pixels>, may_scroll: bool) -> ComposerPlacement {
        place_composer(
            text,
            visible,
            size(px(COMPOSER.0), px(COMPOSER.1)),
            may_scroll,
        )
    }

    fn overlaps_vertically(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
        a.top() < b.bottom() && b.top() < a.bottom()
    }

    #[test]
    fn opens_below_the_last_line_when_there_is_room() {
        let visible = bounds(0.0, 40.0, 900.0, 700.0);
        // A two-line selection: its first line's top at 100, its last line's bottom at 150 (a
        // zoomed line height, read from the layout).
        let text = bounds(200.0, 100.0, 300.0, 50.0);
        let placed = place(text, visible, true);
        assert_eq!(placed.side, ComposerSide::Below);
        assert_eq!(placed.scroll_by, 0.0);
        assert_eq!(placed.frame.top(), px(150.0 + COMPOSER_GAP));
        assert!(!overlaps_vertically(placed.frame, text));
        assert_eq!(placed.frame.center().x, text.center().x);
    }

    #[test]
    fn opens_above_the_first_line_when_below_does_not_fit() {
        let visible = bounds(0.0, 40.0, 900.0, 700.0);
        let text = bounds(200.0, 600.0, 300.0, 24.0);
        let placed = place(text, visible, true);
        assert_eq!(placed.side, ComposerSide::Above);
        assert_eq!(placed.scroll_by, 0.0);
        assert_eq!(placed.frame.bottom(), px(600.0 - COMPOSER_GAP));
        assert!(!overlaps_vertically(placed.frame, text));
    }

    #[test]
    fn scrolls_once_when_neither_side_fits() {
        // A short view: 300px tall, the text in its middle.
        let visible = bounds(0.0, 0.0, 900.0, 300.0);
        let text = bounds(200.0, 130.0, 300.0, 30.0);
        let placed = place(text, visible, true);
        assert_eq!(placed.side, ComposerSide::Below);
        assert!(placed.scroll_by > 0.0);
        let scrolled = bounds(200.0, 130.0 - placed.scroll_by, 300.0, 30.0);
        assert!(scrolled.top() >= px(COMPOSER_MARGIN));
        assert!(placed.frame.bottom() <= px(300.0 - COMPOSER_MARGIN));
        assert!(!overlaps_vertically(placed.frame, scrolled));
    }

    #[test]
    fn scrolls_text_above_the_view_into_it() {
        let visible = bounds(0.0, 0.0, 900.0, 400.0);
        let text = bounds(200.0, -500.0, 300.0, 30.0);
        let placed = place(text, visible, true);
        assert_eq!(placed.side, ComposerSide::Below);
        assert_eq!(placed.scroll_by, -500.0 - COMPOSER_MARGIN);
        let scrolled = bounds(200.0, -500.0 - placed.scroll_by, 300.0, 30.0);
        assert!(!overlaps_vertically(placed.frame, scrolled));
    }

    #[test]
    fn scrolls_text_below_the_view_into_it() {
        let visible = bounds(0.0, 0.0, 900.0, 400.0);
        let text = bounds(200.0, 900.0, 300.0, 30.0);
        let placed = place(text, visible, true);
        assert!(placed.scroll_by > 0.0);
        let scrolled = bounds(200.0, 900.0 - placed.scroll_by, 300.0, 30.0);
        assert!(scrolled.top() >= px(0.0) && scrolled.bottom() <= px(400.0));
        assert!(!overlaps_vertically(placed.frame, scrolled));
        assert!(placed.frame.bottom() <= px(400.0 - COMPOSER_MARGIN));
    }

    #[test]
    fn goes_beside_text_too_tall_to_scroll_into_view_with_it() {
        let visible = bounds(0.0, 0.0, 1400.0, 300.0);
        let text = bounds(100.0, 20.0, 700.0, 260.0);
        let placed = place(text, visible, true);
        assert_eq!(placed.side, ComposerSide::Beside);
        assert!(placed.frame.left() >= text.right());
        assert!(placed.frame.right() <= px(1400.0 - COMPOSER_MARGIN));
    }

    #[test]
    fn stays_inside_the_view_when_nothing_fits() {
        let visible = bounds(0.0, 0.0, 500.0, 300.0);
        let text = bounds(20.0, 20.0, 460.0, 260.0);
        let placed = place(text, visible, true);
        assert_eq!(placed.side, ComposerSide::Over);
        assert!(placed.frame.top() >= px(COMPOSER_MARGIN));
        assert!(placed.frame.bottom() <= px(300.0 - COMPOSER_MARGIN));
        assert!(placed.frame.left() >= px(COMPOSER_MARGIN));
    }

    #[test]
    fn follows_without_scrolling_after_it_opened() {
        let visible = bounds(0.0, 0.0, 900.0, 300.0);
        let text = bounds(200.0, 130.0, 300.0, 30.0);
        let placed = place(text, visible, false);
        assert_eq!(placed.scroll_by, 0.0);
        assert!(placed.frame.top() >= px(COMPOSER_MARGIN));
        assert!(placed.frame.bottom() <= px(300.0 - COMPOSER_MARGIN));
    }
}
