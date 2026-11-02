//! Which of the query row's layouts fits the window: the width left for the query field is worked out from the measured text of the placeholder and the button labels, so the long placeholder is never cut in whatever font or window width.
//!
//! CDXC:PromptSearch 2026-11-03 DECISION:
//! User picked "steps decided from measured room" for the cut-off placeholder (Q1141 D). The query row keeps every label while the field has room for the long placeholder; below that the Fav, View, Copy and Fork labels go, then Grouping's, then the four fold into a "…" menu, and only when even that is too narrow does the short placeholder (the user kept "Search your prompts") replace the long one. This replaced the fixed 970 pt and 830 pt breakpoints, which sat 20 pt under the 990 pt window the dialog opens at and ignored the font. Copy used to keep its label "so the toolbar does not shift"; it now follows the steps.
use super::window::{FIND_PLACEHOLDER, FIND_PLACEHOLDER_SHORT};
use gpui::{TextRun, Window, font, px};

/// The Agents and Projects dropdown triggers keep these fixed widths (CDXC:PromptSearch 2026-09-16).
pub(super) const FILTER_AGENTS_WIDTH: f32 = 128.0;
pub(super) const FILTER_PROJECTS_WIDTH: f32 = 160.0;
/// The query row's own padding on each side and the gap between its children.
const ROW_PADDING: f32 = 14.0;
const ROW_GAP: f32 = 10.0;
/// The gap between the buttons.
const BUTTON_GAP: f32 = 6.0;
/// A button's border (2), horizontal padding (20) and icon (14); a label adds the 6 pt gap and its text.
const BUTTON_ICON_ONLY_WIDTH: f32 = 36.0;
const BUTTON_LABEL_GAP: f32 = 6.0;
/// Room kept after the placeholder for the caret and rounding.
const PLACEHOLDER_SLACK: f32 = 8.0;
/// Text sizes the row draws at.
const PLACEHOLDER_SIZE: f32 = 15.0;
const LABEL_SIZE: f32 = 13.0;

/// How the Fav, View, Copy and Fork buttons are drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ActionsStep {
    Labels,
    Icons,
    /// One "…" button that opens a menu of the four.
    Menu,
}

/// The query row's layout for one window width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct ToolbarLayout {
    pub(super) grouping_label: bool,
    pub(super) actions: ActionsStep,
    pub(super) short_placeholder: bool,
}

/// Measured text widths, in points, at the sizes the row draws them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct ToolbarMetrics {
    pub(super) placeholder_long: f32,
    pub(super) placeholder_short: f32,
    pub(super) chevron: f32,
    pub(super) grouping: f32,
    /// Fav, View, Copy, Fork.
    pub(super) actions: [f32; 4],
}

impl ToolbarMetrics {
    pub(super) fn measure(window: &Window, family: &gpui::SharedString) -> Self {
        let width = |text: &str, size: f32| -> f32 {
            let run = TextRun {
                len: text.len(),
                font: font(family.clone()),
                ..Default::default()
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(text.to_owned().into(), px(size), &[run], None)
                    .width,
            )
        };
        Self {
            placeholder_long: width(FIND_PLACEHOLDER, PLACEHOLDER_SIZE),
            placeholder_short: width(FIND_PLACEHOLDER_SHORT, PLACEHOLDER_SIZE),
            chevron: width("\u{276F}", PLACEHOLDER_SIZE),
            grouping: width("Grouping", LABEL_SIZE),
            actions: [
                width("Fav", LABEL_SIZE),
                width("View", LABEL_SIZE),
                width("Copy", LABEL_SIZE),
                width("Fork", LABEL_SIZE),
            ],
        }
    }
}

fn labelled(text: f32) -> f32 {
    BUTTON_ICON_ONLY_WIDTH + BUTTON_LABEL_GAP + text
}

/// The width the buttons take, gaps included, in `layout`.
fn buttons_width(layout: ToolbarLayout, metrics: &ToolbarMetrics) -> f32 {
    let mut widths = vec![
        if layout.grouping_label {
            labelled(metrics.grouping)
        } else {
            BUTTON_ICON_ONLY_WIDTH
        },
        FILTER_AGENTS_WIDTH,
        FILTER_PROJECTS_WIDTH,
    ];
    match layout.actions {
        ActionsStep::Labels => widths.extend(metrics.actions.iter().map(|text| labelled(*text))),
        ActionsStep::Icons => widths.extend([BUTTON_ICON_ONLY_WIDTH; 4]),
        ActionsStep::Menu => widths.push(BUTTON_ICON_ONLY_WIDTH),
    }
    widths.iter().sum::<f32>() + BUTTON_GAP * (widths.len() - 1) as f32
}

/// The width left for the query field at `window_width` in `layout`.
pub(super) fn field_width(
    window_width: f32,
    layout: ToolbarLayout,
    metrics: &ToolbarMetrics,
) -> f32 {
    window_width
        - 2.0 * ROW_PADDING
        - metrics.chevron
        - 2.0 * ROW_GAP
        - buttons_width(layout, metrics)
}

/// The first layout, richest first, whose field fits the long placeholder; the narrowest layout
/// with the short placeholder when none does.
pub(super) fn toolbar_layout(window_width: f32, metrics: &ToolbarMetrics) -> ToolbarLayout {
    let steps = [
        (true, ActionsStep::Labels),
        (true, ActionsStep::Icons),
        (false, ActionsStep::Icons),
        (false, ActionsStep::Menu),
    ];
    for (grouping_label, actions) in steps {
        let layout = ToolbarLayout {
            grouping_label,
            actions,
            short_placeholder: false,
        };
        if field_width(window_width, layout, metrics)
            >= metrics.placeholder_long + PLACEHOLDER_SLACK
        {
            return layout;
        }
    }
    ToolbarLayout {
        grouping_label: false,
        actions: ActionsStep::Menu,
        short_placeholder: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Widths measured at 13 pt and 15 pt in the dialog's UI font; `scale` stands in for a wider or narrower font.
    fn metrics(scale: f32) -> ToolbarMetrics {
        ToolbarMetrics {
            placeholder_long: 255.0 * scale,
            placeholder_short: 140.0 * scale,
            chevron: 9.0 * scale,
            grouping: 54.0 * scale,
            actions: [20.0 * scale, 28.0 * scale, 30.0 * scale, 26.0 * scale],
        }
    }

    #[test]
    fn wide_window_keeps_every_label() {
        let layout = toolbar_layout(1200.0, &metrics(1.0));
        assert_eq!(layout.actions, ActionsStep::Labels);
        assert!(layout.grouping_label && !layout.short_placeholder);
    }

    #[test]
    fn default_window_drops_the_action_labels_instead_of_cutting_the_placeholder() {
        // 990 pt is where the dialog opens; the long placeholder needs about 263 pt of field.
        let m = metrics(1.0);
        let layout = toolbar_layout(990.0, &m);
        assert_ne!(layout.actions, ActionsStep::Labels);
        assert!(!layout.short_placeholder);
        assert!(field_width(990.0, layout, &m) >= m.placeholder_long);
    }

    #[test]
    fn narrow_window_folds_the_actions_into_the_menu() {
        let m = metrics(1.0);
        let layout = toolbar_layout(720.0, &m);
        assert_eq!(layout.actions, ActionsStep::Menu);
        assert!(!layout.grouping_label && !layout.short_placeholder);
        assert!(field_width(720.0, layout, &m) >= m.placeholder_long);
    }

    #[test]
    fn very_narrow_window_shortens_the_placeholder_last() {
        let layout = toolbar_layout(560.0, &metrics(1.0));
        assert_eq!(layout.actions, ActionsStep::Menu);
        assert!(layout.short_placeholder);
    }

    #[test]
    fn a_wider_font_moves_every_step_up() {
        // At 920 pt the normal font keeps Grouping's label; a 25% wider one has to drop it too.
        let normal = toolbar_layout(920.0, &metrics(1.0));
        let wider = toolbar_layout(920.0, &metrics(1.25));
        assert_eq!(normal.actions, ActionsStep::Icons);
        assert!(normal.grouping_label);
        assert!(!wider.grouping_label);
    }

    #[test]
    fn the_long_placeholder_fits_in_every_step_that_keeps_it() {
        for scale in [0.9_f32, 1.0, 1.25, 1.5] {
            let m = metrics(scale);
            let mut width = 520.0;
            while width <= 1400.0 {
                let layout = toolbar_layout(width, &m);
                if !layout.short_placeholder {
                    assert!(
                        field_width(width, layout, &m) >= m.placeholder_long,
                        "scale {scale} at {width} pt cut the long placeholder"
                    );
                }
                width += 10.0;
            }
        }
    }

    #[test]
    fn steps_only_ever_get_leaner_as_the_window_narrows() {
        let m = metrics(1.0);
        let rank = |layout: ToolbarLayout| {
            (layout.short_placeholder as u8) * 8
                + match layout.actions {
                    ActionsStep::Labels => 0,
                    ActionsStep::Icons => 2,
                    ActionsStep::Menu => 4,
                }
                + (!layout.grouping_label as u8)
        };
        let mut last = 0;
        let mut width = 1400.0;
        while width >= 400.0 {
            let now = rank(toolbar_layout(width, &m));
            assert!(now >= last, "{width} pt is richer than a wider window");
            last = now;
            width -= 5.0;
        }
    }
}
