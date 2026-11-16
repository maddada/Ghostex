//! The native Kanban's colours and type, taken from the native chat's appearance so the board and
//! the chat read as one app, with the chat's frosted treatment under window glass.
//!
//! CDXC:ProjectBoard 2026-11-10 WHY:
//! The board is the Beads view's Board layout and is muted to match its Tree (the pitch's 05-board): grey lane glyphs, mono priority tags and grey label chips instead of coloured lane dots, priority bars and label dots. The agent line's dot is grey: the board's link state says only whether the linked session is live, not whether its agent is working, so there is nothing yet to colour green.

use gpui::{Hsla, Window, rgb};

use crate::app::native_chat::appearance::ChatAppearance;

#[derive(Clone)]
pub(crate) struct KanbanPalette {
    pub(crate) glass: bool,
    pub(crate) font: String,
    /// The page fill; transparent under glass so the frosted work area shows through.
    pub(crate) page: Hsla,
    pub(crate) lane: Hsla,
    pub(crate) lane_drop: Hsla,
    pub(crate) card: Hsla,
    pub(crate) card_hover: Hsla,
    pub(crate) control: Hsla,
    pub(crate) control_hover: Hsla,
    pub(crate) panel: Hsla,
    pub(crate) border: Hsla,
    pub(crate) border_strong: Hsla,
    pub(crate) foreground: Hsla,
    pub(crate) primary: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) faint: Hsla,
    pub(crate) accent: Hsla,
    pub(crate) danger: Hsla,
    pub(crate) button: Hsla,
    pub(crate) button_text: Hsla,
}

fn ink(light: bool, alpha: f32) -> Hsla {
    Hsla::from(rgb(if light { 0x000000 } else { 0xffffff })).opacity(alpha)
}

fn step(color: Hsla, light: bool, amount: f32) -> Hsla {
    Hsla {
        l: if light {
            (color.l - amount).max(0.0)
        } else {
            (color.l + amount).min(1.0)
        },
        ..color
    }
}

impl KanbanPalette {
    pub(crate) fn current(window: &Window) -> Self {
        let glass = crate::app::helpers::window_glass_active_in(window);
        let chat = ChatAppearance::current(&serde_json::Value::Null);
        let light = chat.light;
        let (page, lane, card, control, panel, border) = if glass {
            (
                gpui::transparent_black(),
                ink(light, if light { 0.03 } else { 0.04 }),
                ink(light, if light { 0.04 } else { 0.06 }),
                ink(light, if light { 0.04 } else { 0.06 }),
                ink(light, if light { 0.03 } else { 0.05 }),
                ink(light, 0.08),
            )
        } else {
            (
                chat.background,
                chat.input,
                chat.card_panel,
                chat.input,
                chat.card_panel,
                chat.border,
            )
        };
        let card_hover = if glass {
            ink(light, if light { 0.07 } else { 0.09 })
        } else {
            step(card, light, 0.03)
        };
        let control_hover = if glass {
            ink(light, if light { 0.07 } else { 0.1 })
        } else {
            step(control, light, 0.04)
        };
        Self {
            glass,
            font: chat.font.clone(),
            page,
            lane,
            lane_drop: if glass {
                ink(light, if light { 0.06 } else { 0.08 })
            } else {
                step(lane, light, 0.03)
            },
            card,
            card_hover,
            control,
            control_hover,
            panel,
            // The Borders theme role, when set (helpers/theme_roles.rs).
            border: crate::app::helpers::theme_role_or("border", light, border),
            border_strong: ink(light, if light { 0.16 } else { 0.18 }),
            foreground: chat.foreground,
            primary: chat.primary,
            muted: chat.muted,
            faint: chat.muted.opacity(0.7),
            accent: chat.accent,
            danger: chat.error(),
            button: chat.control_primary,
            button_text: rgb(if light { 0xffffff } else { 0x0d0d0d }).into(),
        }
    }
}
