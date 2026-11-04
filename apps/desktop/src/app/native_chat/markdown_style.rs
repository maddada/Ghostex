use super::appearance::ChatAppearance;
use gpui::{FontWeight, StyleRefinement, Styled, px, relative, rems, rgb};
use gpui_component::highlighter::HighlightTheme;
use gpui_component::text::{InlineCodeStyle, TextViewStyle};
use serde::Deserialize;
use std::sync::{Arc, LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkdownVisual {
    inline_code: InlineCodeVisual,
    paragraph_gap: f32,
    heading_gap_before: f32,
    heading_line_height: f32,
    heading_font_sizes: [f32; 6],
    code_block: CodeBlockVisual,
}

/// How tall a fenced block's code may grow before it scrolls under its header.
///
/// CDXC:SessionChat 2026-10-08 DECISION: User asked for long code blocks to scroll inside a height cap (GPUI Kit #3322), on the desktop and the phone, with a header button that shows the whole block. A block longer than `collapseAfterLines` shows `collapsedLines` lines and scrolls the rest; one only a few lines over the cap stays whole.
/// CDXC:SessionChat 2026-10-08 SEE-ALSO: `apps/mobile/app/src/chat/native/transcript/markdown/Markdown.tsx` caps the phone's blocks with the same numbers.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeBlockVisual {
    collapsed_lines: f32,
    collapse_after_lines: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InlineCodeVisual {
    font_scale: f32,
    padding_x: f32,
    padding_y: f32,
    border_width: f32,
    radius: f32,
}

/// CDXC:SessionChat 2026-09-17 SEE-ALSO: Markdown typography and inline code metrics come from markdown-visual.json, also consumed by session-chat-markdown.tsx (deleted 2026-10-01).
static VISUAL: LazyLock<MarkdownVisual> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/markdown-visual.json"
    ))
    .expect("shared Markdown appearance")
});

/// The gap between blocks, in pixels, for the layout that stacks rich sections.
pub(super) fn paragraph_gap(p: &ChatAppearance) -> f32 {
    VISUAL.paragraph_gap * p.scale
}

#[derive(Deserialize)]
struct CodeThemes {
    dark: HighlightTheme,
    light: HighlightTheme,
}

/// CDXC:SessionChat 2026-09-18 SEE-ALSO:
/// The two themes are the GitHub palettes React highlights with through Shiki
/// (`SESSION_CHAT_SHIKI_DARK_THEME` / `_LIGHT_THEME` in
/// packages/core-ui/chat/session-chat-code-highlight.ts (deleted 2026-10-01)), written out in
/// packages/gx-chat-core/visual/code-theme.json for the Rust
/// highlighter. Change the Shiki themes and these colours together.
static CODE_THEMES: LazyLock<CodeThemes> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/code-theme.json"
    ))
    .expect("shared code theme")
});

static DARK_CODE_THEME: LazyLock<Arc<HighlightTheme>> =
    LazyLock::new(|| Arc::new(CODE_THEMES.dark.clone()));
static LIGHT_CODE_THEME: LazyLock<Arc<HighlightTheme>> =
    LazyLock::new(|| Arc::new(CODE_THEMES.light.clone()));

/// The syntax palette a fenced block is painted with, matching React's Shiki themes.
pub(crate) fn highlight_theme(light: bool) -> Arc<HighlightTheme> {
    if light {
        LIGHT_CODE_THEME.clone()
    } else {
        DARK_CODE_THEME.clone()
    }
}

/// React's `--chat-list-gutter`: the column a list's markers are right-aligned into.
pub(super) const LIST_GUTTER: f32 = 20.0;
/// React's `li + li` margin.
const LIST_ITEM_GAP: f32 = 4.0;

pub(crate) fn text_style(p: &ChatAppearance) -> TextViewStyle {
    let mut style = TextViewStyle::default()
        // The chat view never changes the mouse cursor (see native_chat/cursor.rs), so the
        // rendered Markdown asks for no I-beam over its selectable text and no hand over its
        // links, reference pills or linked images. Selecting and clicking are unaffected.
        .default_cursor(true)
        .paragraph_gap(rems(VISUAL.paragraph_gap / 16.0 * p.scale))
        .heading_font_size(|level, base| {
            base * VISUAL.heading_font_sizes[(level.clamp(1, 6) - 1) as usize] / 14.0
        });
    // The card a fenced block sits in, and its header's rule, exactly as the
    // React transcript drew them.
    style.code_block = StyleRefinement::default()
        .text_color(p.code_text)
        .bg(p.input)
        .border_1()
        .border_color(p.border)
        .rounded(px(12.0 * p.scale));
    // The adaptive table layout: columns fit the transcript's width and wrap, and a table
    // scrolls sideways only when its columns' floors cannot fit (see `text_view`).
    let mut table = StyleRefinement::default();
    table.overflow.x = Some(gpui::Overflow::Scroll);
    style.table = table;
    // CDXC:SessionChat 2026-09-24 DECISION:
    // "When I hover over a table, if it's too wide and it's going off-screen, show a small
    // scrollbar at the very bottom below the table", matching the transcript's own bar: the same
    // 5px thumb in the app's scrollbar colours, under the table rather than over its last row.
    style.table_scrollbar = Some(px(super::scrollbar::THICKNESS * p.scale));
    // The React transcript drew a table as rules, not as a grid: no frame
    // around it, no fill behind it or its header, no rule between columns, a
    // full-strength rule under the header and a lighter one under every body row.
    style.table_track = StyleRefinement::default()
        .border(px(0.0))
        .rounded(px(0.0))
        .bg(gpui::transparent_black());
    style.table_cell = StyleRefinement::default()
        .border_r(px(0.0))
        .px(px(12.0 * p.scale))
        .py(px(7.2 * p.scale));
    style.table_row = StyleRefinement::default()
        .border_b(px(1.0))
        .border_color(p.border.opacity(0.6));
    style.table_head_row = StyleRefinement::default()
        .bg(gpui::transparent_black())
        .border_b(px(1.0))
        .border_color(p.border);
    style.table_head_cell = StyleRefinement::default()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.foreground);
    // A list keeps React's 1.25rem marker gutter and the 0.25rem between items.
    style.list = StyleRefinement::default().gap(px(LIST_ITEM_GAP * p.scale));
    // CDXC:SessionChat 2026-10-08 DECISION: User took two pieces of GPUI Kit's typography update (#3371) for the chat on the desktop and the phone: list markers in the muted colour, and a finished task's text dimmed (the Kit fork draws a done item muted) so open items stand out. The rest of that update stays out to keep the chat's look.
    style.list_marker = StyleRefinement::default()
        .min_w(px(LIST_GUTTER * p.scale))
        .text_color(p.muted);
    style.heading_base_font_size = px(14.0 * p.scale);
    // A heading is louder than the paragraph before it, so React gives it more
    // room above (`--chat-heading-gap-before`) than between paragraphs; the
    // block gap is already there, so only the difference is added here. The
    // renderer drops it again for a message's first block, the way
    // `.ghostex-chat-markdown > :first-child` does.
    style.heading = StyleRefinement::default()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.primary)
        .line_height(relative(VISUAL.heading_line_height))
        .pt(px((VISUAL.heading_gap_before - VISUAL.paragraph_gap)
            .max(0.0)
            * p.scale))
        .pb(px(VISUAL.paragraph_gap * p.scale));
    style.inline_code_style = Some(inline_code(p));
    style.prose_swatch = Some(prose_swatch(p));
    style.link = super::markdown_links::transcript_link_color(p);
    // CDXC:SessionChat 2026-10-01 DECISION: User: "remove the underline for links please I don't like it"; a chat link is marked by its color alone.
    style.link_underline = false;
    style
}

/// React's `.ghostex-chat-color-swatch` hairline: one grey ring, the same in both themes.
fn swatch_ring() -> gpui::Hsla {
    gpui::Hsla::from(rgb(0x808080)).opacity(0.45)
}

/// CDXC:SessionChat 2026-09-15 DECISION:
/// User: show a small rounded square beside each color in shared desktop, mobile, and web chat messages.
/// Every hex colour gets one, wherever it was written: inside a code span, and in running prose.
pub(super) fn inline_code(p: &ChatAppearance) -> InlineCodeStyle {
    let visual = &VISUAL.inline_code;
    InlineCodeStyle {
        font_family: super::fonts::CHAT_MONO.into(),
        font_scale: visual.font_scale,
        padding_x: px(visual.padding_x * p.scale),
        padding_y: px(visual.padding_y * p.scale),
        border_width: px(visual.border_width * p.scale),
        radius: px(visual.radius * p.scale),
        background: super::chat_text_look::inline_code_fill(p.light),
        border_color: p.foreground.opacity(if p.light { 0.16 } else { 0.18 }),
        swatches: true,
        swatch: None,
        swatch_border_width: px(p.scale),
        swatch_border_color: swatch_ring(),
        prose: false,
    }
}

/// The same swatch for a colour somebody wrote into a sentence: React tints it wherever it appears
/// outside a fenced block or a link, and the words around it stay prose rather than becoming a chip.
fn prose_swatch(p: &ChatAppearance) -> InlineCodeStyle {
    InlineCodeStyle {
        padding_x: px(0.0),
        padding_y: px(0.0),
        border_width: px(0.0),
        radius: px(0.0),
        background: gpui::transparent_black(),
        border_color: gpui::transparent_black(),
        prose: true,
        ..inline_code(p)
    }
}

/// Whether a fenced block of `lines` lines is long enough to be capped.
pub(super) fn code_block_collapses(lines: usize) -> bool {
    lines > VISUAL.code_block.collapse_after_lines
}

/// The height a capped block's code scrolls inside: `collapsedLines` lines at the transcript's
/// line height, plus the code's own padding above and below.
pub(super) fn code_block_cap(line_height: f32, padding: f32, scale: f32) -> gpui::Pixels {
    gpui::px((VISUAL.code_block.collapsed_lines * line_height + padding * 2.0) * scale)
}
