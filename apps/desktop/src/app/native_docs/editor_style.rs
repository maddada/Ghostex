//! The live Markdown editor's colours (the Docs page's `MANAGE_MEO_THEME`), light and dark.

use gpui::{Font, FontWeight, Hsla, rgb, rgba};

use super::fonts::DOCS_MONO;
use super::palette::DocsPalette;

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// Body text and the heading colour the editor paints with.
pub(crate) fn body_color(p: &DocsPalette) -> Hsla {
    hex(if p.light { 0x27272a } else { 0xd4d4d4 })
}

pub(crate) fn mono_font() -> Font {
    Font {
        weight: FontWeight::NORMAL,
        ..gpui::font(DOCS_MONO)
    }
}

/// CDXC:Docs 2026-09-05 DECISION:
/// User: match Docs to the Kanban board, use a near-black formatting bar, and replace the banana-yellow and bright-green Markdown palette with a calmer theme. This supersedes the previous blue headings, orange inline code, and blue-gray code-block palette.
pub(crate) fn syntax_style(p: &DocsPalette) -> zorite_editor::SyntaxStyle {
    let light = p.light;
    let pick =
        |light_value: u32, dark_value: u32| hex(if light { light_value } else { dark_value });
    let ink = |alpha: f32| hex(if light { 0x000000 } else { 0xffffff }).opacity(alpha);
    zorite_editor::SyntaxStyle {
        marker: pick(0x626269, 0x858585),
        code: pick(0x73513a, 0xc4b5db),
        code_bg: if p.glass {
            ink(0.06)
        } else {
            pick(0xf3f4f6, 0x1d1d1d)
        },
        link: pick(0x315d88, 0x9bbce0),
        tag: pick(0x715299, 0xb6a3cc),
        quote: pick(0x626269, 0xa3a3a3),
        alert_note: pick(0x315d88, 0x9bbce0),
        alert_tip: pick(0x475569, 0xa4bac8),
        alert_important: pick(0x715299, 0xb6a3cc),
        alert_warning: pick(0x27272a, 0xe5e5e5),
        alert_caution: pick(0x27272a, 0xededed),
        alert_icons: None,
        rule: pick(0xd4d4d8, 0x303030),
        mark_bg: rgba(0xe2b34047).into(),
        block_label: None,
        block_label_gen: 0,
        block_ref_count: None,
        popover_bg: pick(0xffffff, 0x0e0e0e),
        popover_border: ink(0.12),
        popover_fg: pick(0x18181b, 0xf4f4f5).opacity(0.88),
        popover_hover: ink(0.105),
        popover_divider: ink(0.1),
        popover_danger: pick(0xbe123c, 0xfda4af),
        mono: mono_font(),
        property_icon: None,
        heading: Some(pick(0x27272a, 0xededed)),
    }
}
