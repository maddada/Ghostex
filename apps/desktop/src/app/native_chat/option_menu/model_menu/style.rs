use super::super::super::appearance::ChatAppearance;
use gpui::{Hsla, rgb};

pub(super) const CARD_WIDTH: f32 = 304.0;
pub(super) const FLYOUT_WIDTH: f32 = 232.0;
pub(crate) const CARD_RADIUS: f32 = 12.0;
pub(super) const ITEM_RADIUS: f32 = 8.0;
pub(super) const BAR_HEIGHT: f32 = 40.0;
/// The search field under the tabs, drawn only on a long list.
pub(super) const SEARCH_HEIGHT: f32 = 36.0;
/// The model list keeps one height whatever the tab holds, so switching tabs never moves the card.
pub(super) const LIST_HEIGHT: f32 = 216.0;
pub(super) const TRAIT_ROW_HEIGHT: f32 = 30.0;
pub(super) const ROW_GAP: f32 = 2.0;
pub(super) const BUTTON_GAP: f32 = 4.0;
/// Past this many, the footer's buttons wrap onto another line rather than squeeze their values.
const BUTTONS_PER_LINE: usize = 4;

/// How many lines the footer's buttons take, and how many sit on each.
pub(super) fn button_lines(buttons: usize) -> (usize, usize) {
    let lines = buttons.div_ceil(BUTTONS_PER_LINE);
    (
        lines,
        if lines == 0 {
            0
        } else {
            buttons.div_ceil(lines)
        },
    )
}
pub(super) const ERROR_HEIGHT: f32 = 58.0;
/// The key reminder along the card's bottom edge.
pub(super) const HINTS_HEIGHT: f32 = 26.0;

/// The picker's tones, from the same two surfaces the chat's other menus use.
pub(super) struct Palette {
    pub(super) text: Hsla,
    pub(super) muted: Hsla,
    pub(super) surface: Hsla,
    pub(super) border: Hsla,
    pub(super) accent: Hsla,
    pub(super) star: Hsla,
    /// The tone the merged pill's fast marker uses, for a footer button that is switched on.
    pub(super) on: Hsla,
    ink: Hsla,
}

impl Palette {
    pub(super) fn new(appearance: &ChatAppearance) -> Self {
        let light = appearance.light;
        let text: Hsla = rgb(if light { 0x292929 } else { 0xfcfcfc }).into();
        let ink: Hsla = rgb(if light { 0x000000 } else { 0xffffff }).into();
        Self {
            text,
            muted: text.opacity(0.64),
            surface: appearance.menu_surface(),
            border: ink.opacity(0.12),
            accent: appearance.control_primary,
            star: rgb(if light { 0xd97706 } else { 0xfbbf24 }).into(),
            on: appearance.primary,
            ink,
        }
    }

    /// A wash of the surface's contrast colour: hairlines, hover and selection fills.
    pub(super) fn ink(&self, alpha: f32) -> Hsla {
        self.ink.opacity(alpha)
    }
}
