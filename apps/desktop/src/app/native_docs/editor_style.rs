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
        code: pick(0x374151, 0xc4b5db),
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
        // The former React Docs page's alert icons: lucide Info, Lightbulb, AlertCircle,
        // AlertTriangle and XCircle.
        alert_icons: Some(zorite_editor::AlertIcons {
            note: "files-view/l-info-2.svg".into(),
            tip: "files-view/l-lightbulb-2.svg".into(),
            important: "files-view/l-circle-alert-2.svg".into(),
            warning: "files-view/l-triangle-alert-2.svg".into(),
            caution: "files-view/l-circle-x-2.svg".into(),
        }),
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
        quote_bar: pick(0x27272a, 0xe5e5e5),
        code_border: Some(ink(0.12)),
        revealed_links_need_modifier: true,
    }
}

/// The token colours of fenced code: the former React Docs page's CodeMirror theme, mapped from
/// CodeMirror's tags to the tree-sitter captures the highlighter names. Keywords and numbers purple, functions, types and properties slate, strings
/// and variables in the text colour (green in light), comments grey italic.
pub(crate) fn code_theme(
    light: bool,
) -> std::sync::Arc<gpui_component::highlighter::HighlightTheme> {
    use std::sync::{Arc, LazyLock};

    use gpui_component::highlighter::HighlightTheme;

    fn build(light: bool) -> HighlightTheme {
        // (base01 text, base02 muted, base04 strong, base05 link, base06 slate, base08 purple,
        //  keyword, value, inline code)
        let (text, muted, strong, link, slate, purple, keyword, value, inline) = if light {
            (
                "#27272a", "#626269", "#27272a", "#315d88", "#475569", "#715299", "#715299",
                "#35684f", "#73513a",
            )
        } else {
            (
                "#d4d4d4", "#858585", "#ededed", "#9bbce0", "#a4bac8", "#b6a3cc", "#b6a3cc",
                "#d4d4d4", "#c4b5db",
            )
        };
        // (capture, colour, italic, weight)
        let tokens: [(&str, &str, bool, u16); 41] = [
            ("attribute", slate, false, 0),
            ("boolean", value, false, 0),
            ("comment", muted, true, 0),
            ("comment.doc", muted, true, 0),
            ("constant", value, false, 0),
            ("constructor", slate, false, 0),
            ("embedded", text, false, 0),
            ("emphasis", text, true, 0),
            ("emphasis.strong", text, false, 600),
            ("enum", slate, false, 0),
            ("function", slate, false, 0),
            ("keyword", keyword, false, 700),
            ("label", muted, false, 0),
            ("link_text", link, false, 0),
            ("link_uri", link, false, 0),
            ("number", purple, false, 0),
            ("operator", text, false, 0),
            ("preproc", muted, false, 0),
            ("property", slate, false, 0),
            ("punctuation", text, false, 0),
            ("punctuation.bracket", text, false, 0),
            ("punctuation.delimiter", text, false, 0),
            ("punctuation.list_marker", text, false, 0),
            ("punctuation.special", text, false, 0),
            ("string", value, false, 0),
            ("string.escape", value, false, 0),
            ("string.regex", value, false, 0),
            ("string.special", value, false, 0),
            ("string.special.symbol", value, false, 0),
            ("tag", strong, false, 0),
            ("tag.doctype", muted, false, 0),
            ("text.code.span", inline, false, 0),
            ("text.literal", inline, false, 0),
            ("title", strong, false, 600),
            ("type", slate, false, 0),
            ("variable", value, false, 0),
            ("variable.special", strong, true, 0),
            ("variant", value, false, 0),
            ("hint", muted, false, 0),
            ("predictive", muted, false, 0),
            ("primary", text, false, 0),
        ];
        let mut syntax = serde_json::Map::new();
        for (capture, color, italic, weight) in tokens {
            let mut style = serde_json::Map::new();
            style.insert("color".into(), color.into());
            if italic {
                style.insert("font_style".into(), "italic".into());
            }
            if weight > 0 {
                style.insert("font_weight".into(), weight.into());
            }
            syntax.insert(capture.into(), style.into());
        }
        let mut style = serde_json::Map::new();
        style.insert("syntax".into(), syntax.into());
        let mut theme = serde_json::Map::new();
        theme.insert(
            "name".into(),
            if light {
                "Ghostex Docs Light"
            } else {
                "Ghostex Docs Dark"
            }
            .into(),
        );
        theme.insert(
            "appearance".into(),
            if light { "light" } else { "dark" }.into(),
        );
        theme.insert("style".into(), style.into());
        let theme = serde_json::Value::Object(theme);
        serde_json::from_value(theme).expect("the Docs code theme parses")
    }

    static DARK: LazyLock<Arc<HighlightTheme>> = LazyLock::new(|| Arc::new(build(false)));
    static LIGHT: LazyLock<Arc<HighlightTheme>> = LazyLock::new(|| Arc::new(build(true)));
    if light { LIGHT.clone() } else { DARK.clone() }
}

/// The editor's chrome labels: the Docs page's lowercase "copy" on code blocks.
pub(crate) fn labels() -> zorite_editor::Labels {
    zorite_editor::Labels {
        code_copy: "copy".into(),
        ..Default::default()
    }
}

/// A Mermaid card's colours: the code tint, the document border and text (`mermaid.css` with the
/// Docs page's `.meo-mermaid-react-widget` override).
pub(crate) fn mermaid_colors(p: &DocsPalette) -> super::mermaid_widget::MermaidColors {
    let style = syntax_style(p);
    super::mermaid_widget::MermaidColors {
        light: p.light,
        background: style.code_bg,
        border: p.border,
        text: body_color(p),
        muted: p.muted,
        hover: p.control_hover,
        mono: DOCS_MONO,
    }
}
