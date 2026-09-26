//! The native Docs view's colours and type. The solid values are the Docs page's own tokens
//! (`apps/desktop/views/manage/styles.ts`); under window glass the fills become light washes of
//! the text ink, the way the native Kanban's do, so the frosted work area shows through.

use gpui::{Hsla, Window, rgb};

use crate::app::native_chat::appearance::ChatAppearance;

#[derive(Clone)]
pub(crate) struct DocsPalette {
    pub(crate) glass: bool,
    pub(crate) light: bool,
    pub(crate) font: String,
    pub(crate) mono_font: String,
    /// The document's fill; transparent under glass.
    pub(crate) page: Hsla,
    /// The files list, the search row and the header rows.
    pub(crate) chrome: Hsla,
    /// A floating files list covers the document, so it is always opaque.
    pub(crate) floating: Hsla,
    /// Menus and floating bars.
    pub(crate) raised: Hsla,
    pub(crate) text: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) subtle: Hsla,
    pub(crate) accent: Hsla,
    /// `--manage-row-surface`: a selected row, or the row whose menu is open.
    pub(crate) row_surface: Hsla,
    /// `--app-context-menu-hover-background`.
    pub(crate) row_hover: Hsla,
    /// File rows at rest, and hovered or selected.
    pub(crate) row_text: Hsla,
    pub(crate) row_text_strong: Hsla,
    /// A folder above the open file.
    pub(crate) ancestor_text: Hsla,
    pub(crate) toolbar_icon: Hsla,
    pub(crate) toolbar_disabled: Hsla,
    /// Header button hover and open state.
    pub(crate) control_hover: Hsla,
    /// `--manage-border`.
    pub(crate) border: Hsla,
    pub(crate) border_strong: Hsla,
    /// The search row's and the open-files list's bottom rule.
    pub(crate) rule: Hsla,
    /// The docked list's own left edge.
    pub(crate) divider: Hsla,
    pub(crate) danger: Hsla,
    pub(crate) green: Hsla,
    pub(crate) amber: Hsla,
    pub(crate) send: Hsla,
}

fn ink(light: bool, alpha: f32) -> Hsla {
    Hsla::from(rgb(if light { 0x000000 } else { 0xffffff })).opacity(alpha)
}

fn pick(light: bool, light_value: u32, dark_value: u32) -> Hsla {
    rgb(if light { light_value } else { dark_value }).into()
}

impl DocsPalette {
    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: in light mode the Docs files list, its search row, and the document and formatting toolbars use #f4f4f5, and the search row's bottom border is #e5e5e5. Dark mode keeps the 2026-09-07 decision (files sidebar, search row, and header rows use #0b0b0b) until the user shares dark colours.
    ///
    /// CDXC:Docs 2026-09-24 DECISION:
    /// User: after the move to GPUI, Docs has transparency in the background because it is GPUI-based, not web-based anymore. Under glass the page paints no fill and its rows are washes of the text ink, like Kanban and Automate; this supersedes the 2026-09-23 Theming decision that drew the page as one solid card below the tab strip.
    pub(crate) fn current(window: &Window) -> Self {
        let glass = crate::app::helpers::window_glass_active_in(window);
        let chat = ChatAppearance::current(&serde_json::Value::Null);
        let light = chat.light;
        let solid_chrome = pick(light, 0xf4f4f5, 0x0b0b0b);
        let wash = |light_alpha: f32, dark_alpha: f32| {
            ink(light, if light { light_alpha } else { dark_alpha })
        };
        Self {
            glass,
            light,
            font: chat.font.clone(),
            mono_font: crate::app::native_chat::fonts::CHAT_MONO.to_string(),
            page: if glass {
                gpui::transparent_black()
            } else {
                chat.background
            },
            chrome: if glass {
                wash(0.03, 0.04)
            } else {
                solid_chrome
            },
            floating: solid_chrome,
            raised: pick(light, 0xffffff, 0x161616),
            text: pick(light, 0x27272a, 0xe5e5e5),
            muted: pick(light, 0x626269, 0xa3a3a3),
            subtle: pick(light, 0x71717a, 0x808080),
            accent: pick(light, 0x315d88, 0x9bbce0),
            row_surface: if glass {
                wash(0.08, 0.1)
            } else {
                pick(light, 0xe9e9eb, 0x202020)
            },
            row_hover: if glass {
                wash(0.05, 0.07)
            } else {
                pick(light, 0xefefef, 0x202020)
            },
            row_text: pick(light, 0x52525b, 0xb4b8c0),
            row_text_strong: pick(light, 0x3f3f46, 0xd8d8d8),
            ancestor_text: pick(light, 0x18181b, 0xffffff),
            toolbar_icon: pick(light, 0x3f3f46, 0xa3a3a3),
            toolbar_disabled: pick(light, 0xb5b5bd, 0x55555d),
            control_hover: ink(light, 0.08),
            border: ink(light, 0.11),
            border_strong: ink(light, 0.12),
            rule: if glass {
                ink(light, 0.08)
            } else {
                pick(light, 0xe5e5e5, 0x292929)
            },
            divider: if glass {
                ink(light, 0.08)
            } else {
                pick(light, 0xe5e5e5, 0x212121)
            },
            danger: pick(light, 0xbe123c, 0xfda4af),
            green: pick(light, 0x15803d, 0x86efac),
            amber: rgb(0xfbbf24).into(),
            send: pick(light, 0x0f766e, 0x8ed3f3),
        }
    }
}
