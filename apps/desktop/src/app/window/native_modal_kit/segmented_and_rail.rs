use super::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Rgba, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div, point,
    px, rgb,
};
use gpui_component::h_flex;

/// One segment of [`modal_segmented_control`]: an optional 16px leading icon and the label.
pub(crate) struct ModalSegmentedItem {
    pub(crate) icon: Option<&'static str>,
    pub(crate) label: &'static str,
}

/// `SegmentedControl variant='raised' stretch` (packages/components/ui/segmented-control.tsx (deleted 2026-10-01)
/// skinned by packages/components/ui/raised-tab-rail.css (deleted 2026-10-01), ink washes under glass per
/// [`raised_rail_colors`]): a 32px inset track
/// (`light-dark(#ededed, #202020)`, 14% hairline, 8px radius, 3px padding and
/// gap) whose segments split the width at 13px/400 in `light-dark(#525252, #b8b8b8)`,
/// hover on `light-dark(#e3e3e3, #292929)`, and the pressed segment raised on
/// `light-dark(#ffffff, #363636)` with a 1px 8% ring plus a `0 1px 3px` shadow.
/// gpui svgs take no inherited color, so the icon is painted in the segment's
/// resting or pressed text color and does not follow the hover tint.
pub(crate) fn modal_segmented_control<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    items: &[ModalSegmentedItem],
    selected: usize,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    modal_segmented_control_with_extras(p, id, items, Vec::new(), selected, on_select, cx)
}

/// [`modal_segmented_control`] with an extra element after a segment's label (the Relay picker's
/// (i)): `extras[index]` for segment `index`, none where it is missing or `None`. The extra sits
/// inside the segment, so it must stop its own clicks if pressing it must not select the segment.
///
/// CDXC:AppModal 2026-11-09 DECISION:
/// User, of the Relay picker's "Public relays only" segment: "no side padding, text touches the pill's edges". A segment was `min-width: 0`, so a label wider than its equal share overflowed the segment and the raised highlight; the segment now never narrows below its label plus its padding (equal widths still, whenever every label fits its share).
pub(crate) fn modal_segmented_control_with_extras<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    items: &[ModalSegmentedItem],
    extras: Vec<Option<AnyElement>>,
    selected: usize,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let mut extras = extras.into_iter();
    let RaisedRailColors {
        track,
        track_border,
        text,
        hover: hover_bg,
        active_text,
        pressed: pressed_bg,
        ring,
    } = raised_rail_colors(p.light, p.glass);
    let segments = items.iter().enumerate().map(|(index, item)| {
        let pressed = index == selected;
        let on_select = on_select.clone();
        let extra = extras.next().flatten();
        h_flex()
            .id((id, index))
            .role(gpui::Role::RadioButton)
            .aria_label(item.label)
            .aria_toggled(a11y_toggled(pressed))
            .flex_1()
            .flex_basis(px(0.0))
            .h_full()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .px(px(12.0))
            .rounded(px(5.0))
            .text_size(px(13.0))
            .line_height(px(20.0))
            .whitespace_nowrap()
            .text_color(hsla(if pressed { active_text } else { text }))
            .cursor_pointer()
            .when(pressed, |this| {
                this.bg(hsla(pressed_bg)).shadow(vec![
                    gpui::BoxShadow {
                        color: hsla(modal_rgba(0x000000, 0.14)),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(3.0),
                        spread_radius: px(0.0),
                        inset: false,
                    },
                    gpui::BoxShadow {
                        color: hsla(ring),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(1.0),
                        inset: false,
                    },
                ])
            })
            .when(!pressed, |this| {
                this.hover(move |this| this.bg(hsla(hover_bg)).text_color(hsla(active_text)))
            })
            .on_press(cx, move |this, window, cx| {
                on_select(this, index, window, cx);
            })
            .children(item.icon.map(|icon| {
                modal_icon(icon, 16.0, if pressed { active_text } else { text }).flex_shrink_0()
            }))
            .child(item.label)
            .children(extra)
    });
    h_flex()
        .id(id)
        .w_full()
        .h(px(MODAL_CONTROL_HEIGHT))
        .items_stretch()
        .gap(px(3.0))
        .p(px(3.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(track_border))
        .bg(hsla(track))
        .overflow_hidden()
        .children(segments)
        .into_any_element()
}

/// One segment of [`modal_raised_tab_rail`] or [`modal_bordered_segmented_control`]: its label
/// and an optional trailing note (a shortcut hint, a count) in its own quieter tone.
pub(crate) struct ModalRailItem {
    pub(crate) label: SharedString,
    pub(crate) trailing: Option<SharedString>,
    /// A small status dot drawn after the label, in this colour (the bordered control only).
    pub(crate) dot: Option<Rgba>,
}

/// The `.raised-tab-rail` tokens (packages/components/ui/raised-tab-rail.css, deleted 2026-10-01) shared by every
/// raised rail and segmented control: the modal kit's, Settings' Remote page and the chat's
/// account menu.
#[derive(Clone, Copy)]
pub(crate) struct RaisedRailColors {
    pub(crate) track: Rgba,
    pub(crate) track_border: Rgba,
    pub(crate) text: Rgba,
    pub(crate) hover: Rgba,
    pub(crate) active_text: Rgba,
    pub(crate) pressed: Rgba,
    pub(crate) ring: Rgba,
}

/// CDXC:Theming 2026-09-29 WHY:
/// The rail's fixed `#202020` track and `#363636` pressed tab painted an opaque slab across the top of the frosted Agents Hub. Under window glass the track, hover and pressed fills are ink washes like the rest of the frosted palette ([`ModalPalette::frosted`]); the hover and pressed washes sit on top of the track's.
pub(crate) fn raised_rail_colors(light: bool, glass: bool) -> RaisedRailColors {
    let (ink, ring) = if light {
        (0x000000, modal_rgba(0x000000, 0.08))
    } else {
        (0xffffff, modal_rgba(0xffffff, 0.08))
    };
    let track_border = modal_rgba(ink, 0.14);
    let (text, active_text) = if light {
        (rgb(0x525252), rgb(0x262626))
    } else {
        (rgb(0xb8b8b8), rgb(0xf5f5f5))
    };
    let (track, hover, pressed) = match (glass, light) {
        (true, true) => (
            modal_rgba(ink, 0.05),
            modal_rgba(ink, 0.05),
            modal_rgba(0xffffff, 0.85),
        ),
        (true, false) => (
            modal_rgba(ink, 0.04),
            modal_rgba(ink, 0.05),
            modal_rgba(ink, 0.12),
        ),
        (false, true) => (rgb(0xededed), rgb(0xe3e3e3), rgb(0xffffff)),
        (false, false) => (rgb(0x202020), rgb(0x292929), rgb(0x363636)),
    };
    RaisedRailColors {
        track,
        track_border,
        text,
        hover,
        active_text,
        pressed,
        ring,
    }
}

/// `TabsList variant='raised'` stretched over the modal width (packages/components/ui/tabs.tsx (deleted 2026-10-01)
/// with raised-tab-rail.css): a 40px inset track (8px radius, 3px padding and gap) whose equal
/// tabs carry a 13px label and an 11px/500 trailing hint 6px after it, `trailing` at rest and
/// `trailing_active` on the pressed tab. The pressed tab is raised exactly like
/// [`modal_segmented_control`]'s pressed segment.
#[allow(clippy::too_many_arguments)]
pub(crate) fn modal_raised_tab_rail<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    items: &[ModalRailItem],
    selected: usize,
    trailing: Rgba,
    trailing_active: Rgba,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let RaisedRailColors {
        track,
        track_border,
        text,
        hover: hover_bg,
        active_text,
        pressed: pressed_bg,
        ring,
    } = raised_rail_colors(p.light, p.glass);
    let tabs = items.iter().enumerate().map(|(index, item)| {
        let pressed = index == selected;
        let on_select = on_select.clone();
        h_flex()
            .id((id, index))
            .role(gpui::Role::Button)
            .aria_label(item.label.clone())
            .aria_selected(pressed)
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .h_full()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .px(px(10.0))
            .rounded(px(5.0))
            .text_size(px(13.0))
            .line_height(px(20.0))
            .whitespace_nowrap()
            .text_color(hsla(if pressed { active_text } else { text }))
            .cursor_pointer()
            .when(pressed, |this| {
                this.bg(hsla(pressed_bg)).shadow(vec![
                    gpui::BoxShadow {
                        color: hsla(modal_rgba(0x000000, 0.14)),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(3.0),
                        spread_radius: px(0.0),
                        inset: false,
                    },
                    gpui::BoxShadow {
                        color: hsla(ring),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(1.0),
                        inset: false,
                    },
                ])
            })
            .when(!pressed, |this| {
                this.hover(move |this| this.bg(hsla(hover_bg)).text_color(hsla(active_text)))
            })
            .on_press(cx, move |this, window, cx| {
                on_select(this, index, window, cx);
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(item.label.clone()),
            )
            .children(item.trailing.clone().map(|hint| {
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(if pressed { trailing_active } else { trailing }))
                    .child(hint)
            }))
    });
    h_flex()
        .id(id)
        .w_full()
        .flex_shrink_0()
        .h(px(40.0))
        .items_stretch()
        .gap(px(3.0))
        .p(px(3.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(track_border))
        .bg(hsla(track))
        .overflow_hidden()
        .children(tabs)
        .into_any_element()
}

/// The default `SegmentedControl` (packages/components/ui/segmented-control.tsx (deleted 2026-10-01) and its
/// canonical rules in packages/core-ui/styles.css (deleted 2026-11-06)): one `border` (8px radius) around equal
/// segments split by the same hairline, 14px labels at 78% foreground, a 6% foreground wash on
/// hover and a 14% one on the pressed segment, which also takes the full foreground. A
/// segment's trailing note (a count) is drawn in `trailing`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn modal_bordered_segmented_control<V: 'static>(
    foreground: Rgba,
    border: Rgba,
    trailing: Rgba,
    id: &'static str,
    items: &[ModalRailItem],
    selected: usize,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    // CDXC:AppModal 2026-10-18 WHY:
    // GPUI's `overflow_hidden` clips to the control's rectangle, not its rounded border, so the
    // pressed or hovered end segment's square fill poked out past the outer corners (seen on the
    // Notifications panel's tabs). The end segments round their outer corners to the border's inner
    // radius instead of relying on the clip.
    let inner_radius = px(MODAL_RADIUS_CONTROL - 1.0);
    let last = items.len().saturating_sub(1);
    let segments = items.iter().enumerate().map(|(index, item)| {
        let pressed = index == selected;
        let on_select = on_select.clone();
        h_flex()
            .id((id, index))
            .role(gpui::Role::RadioButton)
            .aria_label(item.label.clone())
            .aria_toggled(a11y_toggled(pressed))
            // CDXC:AppModal 2026-11-09 DECISION:
            // User, of Remote's tab strip at a narrow Settings width: labels touched their segment edges. A segment takes its label's width plus an equal share of the spare room, and when the strip is too narrow the labels shrink together and end in an ellipsis inside their padding, so a label never touches the edge or hides another tab.
            .flex_auto()
            .min_w_0()
            .h_full()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .px(px(12.0))
            .when(index == 0, |this| this.rounded_l(inner_radius))
            .when(index == last, |this| this.rounded_r(inner_radius))
            .when(index > 0, |this| {
                this.border_l_1().border_color(hsla(border))
            })
            .text_size(px(14.0))
            .line_height(px(20.0))
            .whitespace_nowrap()
            .text_color(hsla(if pressed {
                foreground
            } else {
                rgba_of(foreground, foreground.a * 0.78)
            }))
            .cursor_pointer()
            .when(pressed, |this| this.bg(hsla(rgba_of(foreground, 0.14))))
            .when(!pressed, |this| {
                this.hover(move |this| {
                    this.bg(hsla(rgba_of(foreground, 0.06)))
                        .text_color(hsla(foreground))
                })
            })
            .on_press(cx, move |this, window, cx| {
                on_select(this, index, window, cx);
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(item.label.clone()),
            )
            .children(item.dot.map(|dot| {
                div()
                    .flex_shrink_0()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(hsla(dot))
            }))
            .children(
                item.trailing
                    .clone()
                    .map(|note| div().flex_shrink_0().text_color(hsla(trailing)).child(note)),
            )
    });
    h_flex()
        .id(id)
        .flex_shrink_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .items_stretch()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .overflow_hidden()
        .children(segments)
}
