//! Native GPUI Ghostex Update dialog, the desktop twin of the React
//! `UpdateAvailableModal` in packages/core-ui/update-available-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:Release 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, and the new gpui modal must be EXACTLY 1 to 1 matching the React one: layout, copy, colors, radii, spacing, fonts, states and keys in both appearances. The release notes are the `.ghostex-chat-markdown` prose the React card renders through react-markdown (headings, two-level bullet lists, inline code chips, bold, quotes, links as plain text, images dropped), so this module carries a small block renderer for exactly that subset instead of a generic markdown engine.
//! SEE-ALSO: packages/core-ui/update-available-modal.tsx (deleted 2026-10-01) and the `.update-available-modal-*` rules in packages/core-ui/styles.css (deleted 2026-11-06) plus the `.ghostex-chat-markdown` rules in packages/core-ui/styles/chat.css (deleted 2026-11-06) (the React twin and the CSS mirrored below), apps/desktop/src/app/window/native_modal_kit/ (shared chrome and controls), apps/desktop/src/app/update_available_modal_lifecycle.rs (open, close, Windows updater actions), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, FocusHandle, FontWeight, HighlightStyle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledText, Window, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use std::ops::Range;
use std::rc::Rc;

/// `APP_MODAL_HOST_UPDATE_AVAILABLE_WINDOW_WIDTH`: the child window the React dialog opened in.
pub(crate) const UPDATE_AVAILABLE_MODAL_WIDTH: f32 = 640.0;
/// CDXC:Release 2026-09-18 DECISION:
/// User: make the update dialog 1.5x taller. The notes use the additional space while the footer stays visible.
/// SEE-ALSO: packages/core-ui/styles.css (deleted 2026-11-06), apps/desktop/src/app/update_available_modal_lifecycle.rs.
pub(crate) const UPDATE_AVAILABLE_MODAL_INITIAL_HEIGHT: f32 = 560.0 * 1.5;

const TITLE_READY: &str = "Ghostex is ready to update";
const TITLE_AVAILABLE: &str = "A Ghostex update is available";
const FALLBACK_NOTES: &str = "This update includes improvements and fixes for Ghostex.";
const PORTABLE_NOTE: &str = "This portable copy will be updated in place and remain portable.";
const LATER: &str = "Later";
const CANCEL: &str = "Cancel";
const RESTART_AND_UPDATE: &str = "Restart and update";
const DOWNLOAD_UPDATE: &str = "Download update";
const OPEN_DOWNLOAD_PAGE: &str = "Open download page";

/// `.update-available-modal-notes`: 13px prose at line-height 1.55.
const NOTES_FONT_SIZE: f32 = 13.0;
const NOTES_LINE_HEIGHT: f32 = 20.15;
const HEADER_HEIGHT: f32 = 20.8 + 6.0 + 20.15;
/// The shadcn `Card size='sm'`: 16px padding all around (`py-4` plus `px-4` on its content).
const CARD_PADDING: f32 = 16.0;
/// `.ghostex-chat-markdown` block margins (`0.65rem`) and list gutter (`1.25rem`) at the 16px root size.
const BLOCK_MARGIN: f32 = 10.4;
const LIST_GUTTER: f32 = 20.0;
/// `li + li { margin-top: 0.25rem }`.
const LIST_ITEM_GAP: f32 = 4.0;
/// Headings: 600 weight, line-height 1.3, `margin: 1.25rem 0 0.5rem`.
const HEADING_MARGIN_TOP: f32 = 20.0;
const HEADING_MARGIN_BOTTOM: f32 = 8.0;
/// `:not(pre) > code`: 6px radius, `0.1rem 0.35rem` padding, `--chat-code-size` 13px.
const CODE_RADIUS: f32 = 6.0;
const CODE_PADDING_X: f32 = 5.6;
const CODE_FONT_SIZE: f32 = 13.0;
/// The inline chip is the mono glyph box plus 1.6px of padding above and below: 20.2px in Chrome, centered on the line.
const CODE_CHIP_HEIGHT: f32 = 20.2;
/// `blockquote`: 2px left rule, `0.8rem` left padding, muted text.
const QUOTE_RULE_WIDTH: f32 = 2.0;
const QUOTE_PADDING_LEFT: f32 = 12.8;
/// Chrome's outside markers at 13px, measured against the React render: the
/// `disc` is a 4px dot 5px into the 20px gutter and 9.75px below the line top;
/// the nested `circle` is a 6px ring 4px in and 8.25px down.
const DISC_DIAMETER: f32 = 4.0;
const DISC_INSET: f32 = 5.0;
const DISC_TOP: f32 = 9.75;
const CIRCLE_DIAMETER: f32 = 6.0;
const CIRCLE_INSET: f32 = 4.0;
const CIRCLE_TOP: f32 = 8.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateAvailableState {
    Available,
    Ready,
    /// CDXC:Release 2026-09-22 DECISION:
    /// User: Linux does not install updates itself; its dialog shows the same changelog and its button opens the download page. Native only: the React twin is never opened on Linux.
    Notify,
}

/// What the dialog asks its host to do. The dialog removes its own window before sending any of these.
pub(crate) enum UpdateAvailableModalCommand {
    Cancel,
    Download,
    Restart,
    OpenDownloadPage,
}

pub(crate) type UpdateAvailableModalHost = Rc<dyn Fn(UpdateAvailableModalCommand, &mut App)>;

pub(crate) struct UpdateAvailableModalConfig {
    pub(crate) version: String,
    pub(crate) state: UpdateAvailableState,
    pub(crate) notes_markdown: String,
    pub(crate) portable: bool,
    pub(crate) palette: ModalPalette,
}

/// One styled span of a paragraph, heading, list item, or quote.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Inline {
    Text(String),
    Bold(String),
    Code(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ListItem {
    inlines: Vec<Inline>,
    marker: Option<String>,
    children: Vec<ListItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Block {
    Heading { level: u8, inlines: Vec<Inline> },
    Paragraph(Vec<Inline>),
    List(Vec<ListItem>),
    Quote(Vec<Inline>),
}

/// A pending list line: its nesting depth (two spaces per level), an ordered
/// marker (`1.`) or `None` for a bullet, and its text.
fn list_line(line: &str) -> Option<(usize, Option<String>, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    if let Some(text) = rest.strip_prefix("- ").or_else(|| rest.strip_prefix("* ")) {
        return Some((indent / 2, None, text.trim()));
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty()
        && let Some(text) = rest[digits.len()..].strip_prefix(". ")
    {
        return Some((indent / 2, Some(format!("{digits}.")), text.trim()));
    }
    None
}

fn push_list_item(items: &mut Vec<ListItem>, depth: usize, item: ListItem) {
    if depth == 0 || items.is_empty() {
        items.push(item);
        return;
    }
    let last = items.last_mut().expect("non-empty list");
    push_list_item(&mut last.children, depth - 1, item);
}

fn last_list_item(items: &mut [ListItem]) -> Option<&mut ListItem> {
    let last = items.last_mut()?;
    if last.children.is_empty() {
        Some(last)
    } else {
        last_list_item(&mut last.children)
    }
}

/// Inline markdown: `` `code` ``, `**bold**`, `[text](url)` as its text, `![alt](url)` dropped.
fn parse_inlines(text: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    let flush = |plain: &mut String, inlines: &mut Vec<Inline>| {
        if !plain.is_empty() {
            inlines.push(Inline::Text(std::mem::take(plain)));
        }
    };
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            flush(&mut plain, &mut inlines);
            inlines.push(Inline::Code(after[..end].to_string()));
            rest = &after[end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
        {
            flush(&mut plain, &mut inlines);
            inlines.push(Inline::Bold(after[..end].to_string()));
            rest = &after[end + 2..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("![")
            && let Some(close) = after.find("](")
            && let Some(end) = after[close..].find(')')
        {
            rest = &after[close + end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('[')
            && let Some(close) = after.find("](")
            && let Some(end) = after[close..].find(')')
        {
            plain.push_str(&after[..close]);
            rest = &after[close + end + 1..];
            continue;
        }
        let mut chars = rest.chars();
        let ch = chars.next().expect("non-empty rest");
        plain.push(ch);
        rest = chars.as_str();
    }
    flush(&mut plain, &mut inlines);
    inlines
}

/// The block structure of the release notes: headings, bullet and numbered
/// lists nested by two-space indents, quotes, and paragraphs.
fn parse_blocks(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut quote: Vec<String> = Vec::new();
    let mut list: Vec<ListItem> = Vec::new();
    fn close_paragraph(paragraph: &mut Vec<String>, blocks: &mut Vec<Block>) {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(parse_inlines(&paragraph.join(" "))));
            paragraph.clear();
        }
    }
    fn close_quote(quote: &mut Vec<String>, blocks: &mut Vec<Block>) {
        if !quote.is_empty() {
            blocks.push(Block::Quote(parse_inlines(&quote.join(" "))));
            quote.clear();
        }
    }
    fn close_list(list: &mut Vec<ListItem>, blocks: &mut Vec<Block>) {
        if !list.is_empty() {
            blocks.push(Block::List(std::mem::take(list)));
        }
    }
    for raw in markdown.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            close_paragraph(&mut paragraph, &mut blocks);
            close_quote(&mut quote, &mut blocks);
            continue;
        }
        let trimmed = line.trim_start();
        let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            close_paragraph(&mut paragraph, &mut blocks);
            close_quote(&mut quote, &mut blocks);
            close_list(&mut list, &mut blocks);
            blocks.push(Block::Heading {
                level: hashes as u8,
                inlines: parse_inlines(trimmed[hashes..].trim().trim_end_matches('#').trim()),
            });
            continue;
        }
        if let Some(text) = trimmed.strip_prefix('>') {
            close_paragraph(&mut paragraph, &mut blocks);
            close_list(&mut list, &mut blocks);
            quote.push(text.trim().to_string());
            continue;
        }
        if let Some((depth, marker, text)) = list_line(line) {
            close_paragraph(&mut paragraph, &mut blocks);
            close_quote(&mut quote, &mut blocks);
            let depth = if list.is_empty() { 0 } else { depth };
            push_list_item(
                &mut list,
                depth,
                ListItem {
                    inlines: parse_inlines(text),
                    marker,
                    children: Vec::new(),
                },
            );
            continue;
        }
        if !list.is_empty() && line.starts_with(' ') {
            // A wrapped continuation of the previous bullet.
            if let Some(item) = last_list_item(&mut list) {
                let mut joined = inlines_to_markdown(&item.inlines);
                joined.push(' ');
                joined.push_str(trimmed);
                item.inlines = parse_inlines(&joined);
            }
            continue;
        }
        close_list(&mut list, &mut blocks);
        close_quote(&mut quote, &mut blocks);
        paragraph.push(trimmed.to_string());
    }
    close_paragraph(&mut paragraph, &mut blocks);
    close_quote(&mut quote, &mut blocks);
    close_list(&mut list, &mut blocks);
    blocks
}

/// Words with the space that follows each one kept on the word, so a wrapping
/// row of them touches the way inline text does and only breaks at spaces.
fn split_words(text: &str) -> Vec<String> {
    let mut word = String::new();
    let mut words: Vec<String> = Vec::new();
    for ch in text.chars() {
        word.push(ch);
        if ch == ' ' {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn inlines_to_markdown(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(text) => text.clone(),
            Inline::Bold(text) => format!("**{text}**"),
            Inline::Code(text) => format!("`{text}`"),
        })
        .collect()
}

pub(crate) struct GpuiUpdateAvailableModalWindow {
    host: UpdateAvailableModalHost,
    palette: ModalPalette,
    version: String,
    state: UpdateAvailableState,
    portable: bool,
    blocks: Vec<Block>,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiUpdateAvailableModalWindow {
    pub(crate) fn new(
        config: UpdateAvailableModalConfig,
        host: UpdateAvailableModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let notes = config.notes_markdown.trim();
        let blocks = if notes.is_empty() {
            vec![Block::Paragraph(vec![Inline::Text(
                FALLBACK_NOTES.to_string(),
            )])]
        } else {
            parse_blocks(notes)
        };
        Self {
            host,
            palette: config.palette,
            version: config.version,
            state: config.state,
            portable: config.portable,
            blocks,
            fit: ModalFit::fixed(),
            focus_handle,
        }
    }

    fn ready(&self) -> bool {
        self.state == UpdateAvailableState::Ready
    }

    fn close_window_and_send(
        &mut self,
        command: UpdateAvailableModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(UpdateAvailableModalCommand::Cancel, window, cx);
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = match self.state {
            UpdateAvailableState::Ready => UpdateAvailableModalCommand::Restart,
            UpdateAvailableState::Available => UpdateAvailableModalCommand::Download,
            UpdateAvailableState::Notify => UpdateAvailableModalCommand::OpenDownloadPage,
        };
        self.close_window_and_send(command, window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" {
            self.cancel(window, cx);
            cx.stop_propagation();
        }
    }

    /// `--muted` under `--border`: the inline code chip surface and edge.
    fn code_chip_colors(&self) -> (Rgba, Rgba) {
        let p = self.palette;
        if p.light {
            // modals-light.css (deleted 2026-11-06): --muted #f1f1f1, --border rgba(0,0,0,0.14).
            (rgb(0xf1f1f1), p.hairline)
        } else {
            // shadcn.css dark: --muted oklch(0.269 0 0) = #262626, --border oklch(1 0 0 / 10%).
            (p.accent, modal_rgba(0xffffff, 0.10))
        }
    }

    /// `--border` for the blockquote rule.
    fn quote_rule_color(&self) -> Rgba {
        self.code_chip_colors().1
    }

    /// Prose without code spans: one wrapped text run with the bold ranges highlighted.
    fn render_text_run(
        &self,
        inlines: &[Inline],
        font_size: f32,
        line_height: f32,
        weight: FontWeight,
        color: Rgba,
    ) -> AnyElement {
        let mut text = String::new();
        let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
        for inline in inlines {
            match inline {
                Inline::Text(part) => text.push_str(part),
                Inline::Bold(part) => {
                    let start = text.len();
                    text.push_str(part);
                    highlights.push((
                        start..text.len(),
                        HighlightStyle {
                            font_weight: Some(FontWeight::BOLD),
                            ..Default::default()
                        },
                    ));
                }
                Inline::Code(part) => text.push_str(part),
            }
        }
        div()
            .w_full()
            .min_w_0()
            .text_size(px(font_size))
            .line_height(px(line_height))
            .font_weight(weight)
            .text_color(hsla(color))
            .child(StyledText::new(SharedString::from(text)).with_highlights(highlights))
            .into_any_element()
    }

    /// Prose with inline code: a wrapping row of words so the chips keep their
    /// own surface, border and mono font while the line still breaks at spaces.
    fn render_inline_flow(
        &self,
        inlines: &[Inline],
        font_size: f32,
        line_height: f32,
        weight: FontWeight,
        color: Rgba,
    ) -> AnyElement {
        let (chip_background, chip_border) = self.code_chip_colors();
        let mut row = h_flex()
            .w_full()
            .min_w_0()
            .flex_wrap()
            .items_center()
            .text_size(px(font_size))
            .line_height(px(line_height))
            .font_weight(weight)
            .text_color(hsla(color));
        for inline in inlines {
            match inline {
                Inline::Code(code) => {
                    // A code span breaks at its own spaces like Chrome's inline
                    // box does (`box-decoration-break: slice`): each word is a
                    // fragment, the chip's edge and rounding only on the first
                    // and last, so fragments on one line read as one chip.
                    let fragments = split_words(code);
                    let count = fragments.len();
                    for (index, fragment) in fragments.into_iter().enumerate() {
                        let first = index == 0;
                        let last = index + 1 == count;
                        row = row.child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .h(px(CODE_CHIP_HEIGHT))
                                .when(first, |this| {
                                    this.pl(px(CODE_PADDING_X))
                                        .border_l_1()
                                        .rounded_tl(px(CODE_RADIUS))
                                        .rounded_bl(px(CODE_RADIUS))
                                })
                                .when(last, |this| {
                                    this.pr(px(CODE_PADDING_X))
                                        .border_r_1()
                                        .rounded_tr(px(CODE_RADIUS))
                                        .rounded_br(px(CODE_RADIUS))
                                })
                                .border_t_1()
                                .border_b_1()
                                .border_color(hsla(chip_border))
                                .bg(hsla(chip_background))
                                .font_family(MODAL_MONO_FONT)
                                .font_weight(FontWeight::NORMAL)
                                .text_size(px(CODE_FONT_SIZE))
                                .line_height(px(CODE_CHIP_HEIGHT - 2.0))
                                .whitespace_nowrap()
                                .child(SharedString::from(fragment)),
                        );
                    }
                }
                Inline::Text(text) | Inline::Bold(text) => {
                    let bold = matches!(inline, Inline::Bold(_));
                    for word in split_words(text) {
                        row = row.child(
                            div()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .when(bold, |this| this.font_weight(FontWeight::BOLD))
                                .child(SharedString::from(word)),
                        );
                    }
                }
            }
        }
        row.into_any_element()
    }

    fn render_inlines(
        &self,
        inlines: &[Inline],
        font_size: f32,
        line_height: f32,
        weight: FontWeight,
        color: Rgba,
    ) -> AnyElement {
        if inlines
            .iter()
            .any(|inline| matches!(inline, Inline::Code(_)))
        {
            self.render_inline_flow(inlines, font_size, line_height, weight, color)
        } else {
            self.render_text_run(inlines, font_size, line_height, weight, color)
        }
    }

    fn render_marker(&self, item: &ListItem, depth: usize) -> AnyElement {
        let p = self.palette;
        let marker = div()
            .flex_shrink_0()
            .w(px(LIST_GUTTER))
            .h(px(NOTES_LINE_HEIGHT));
        match &item.marker {
            Some(number) => marker
                .flex()
                .items_center()
                .justify_end()
                .pr(px(3.6))
                .text_size(px(NOTES_FONT_SIZE))
                .line_height(px(NOTES_LINE_HEIGHT))
                .child(SharedString::from(number.clone()))
                .into_any_element(),
            None if depth == 0 => marker
                .pt(px(DISC_TOP))
                .pl(px(DISC_INSET))
                .child(
                    div()
                        .size(px(DISC_DIAMETER))
                        .rounded_full()
                        .bg(hsla(p.foreground)),
                )
                .into_any_element(),
            None if depth == 1 => marker
                .pt(px(CIRCLE_TOP))
                .pl(px(CIRCLE_INSET))
                .child(
                    div()
                        .size(px(CIRCLE_DIAMETER))
                        .rounded_full()
                        .border_1()
                        .border_color(hsla(p.foreground)),
                )
                .into_any_element(),
            None => marker
                .pt(px(DISC_TOP))
                .pl(px(DISC_INSET))
                .child(div().size(px(DISC_DIAMETER)).bg(hsla(p.foreground)))
                .into_any_element(),
        }
    }

    fn render_list(&self, items: &[ListItem], depth: usize) -> AnyElement {
        let p = self.palette;
        let mut list = v_flex().w_full().min_w_0();
        for (index, item) in items.iter().enumerate() {
            let mut body = v_flex().flex_1().min_w_0().child(self.render_inlines(
                &item.inlines,
                NOTES_FONT_SIZE,
                NOTES_LINE_HEIGHT,
                FontWeight::NORMAL,
                p.foreground,
            ));
            if !item.children.is_empty() {
                body = body.child(
                    div()
                        .w_full()
                        .mt(px(BLOCK_MARGIN))
                        .child(self.render_list(&item.children, depth + 1)),
                );
            }
            let previous_ended_with_list = index > 0 && !items[index - 1].children.is_empty();
            let gap = if index == 0 {
                0.0
            } else if previous_ended_with_list {
                // The nested list's bottom margin collapses with `li + li`.
                BLOCK_MARGIN.max(LIST_ITEM_GAP)
            } else {
                LIST_ITEM_GAP
            };
            list = list.child(
                h_flex()
                    .w_full()
                    .items_start()
                    .mt(px(gap))
                    .child(self.render_marker(item, depth))
                    .child(body),
            );
        }
        list.into_any_element()
    }

    fn heading_metrics(level: u8) -> (f32, f32) {
        match level {
            1 => (20.0, 26.0),
            2 => (18.0, 23.4),
            3 => (16.0, 20.8),
            _ => (14.0, 18.2),
        }
    }

    fn block_margins(block: &Block) -> (f32, f32) {
        match block {
            Block::Heading { .. } => (HEADING_MARGIN_TOP, HEADING_MARGIN_BOTTOM),
            Block::Paragraph(_) | Block::List(_) | Block::Quote(_) => (BLOCK_MARGIN, BLOCK_MARGIN),
        }
    }

    fn render_block(&self, block: &Block) -> AnyElement {
        let p = self.palette;
        match block {
            Block::Heading { level, inlines } => {
                let (size, line_height) = Self::heading_metrics(*level);
                let color = if *level == 6 { p.muted } else { p.foreground };
                self.render_inlines(inlines, size, line_height, FontWeight::SEMIBOLD, color)
            }
            Block::Paragraph(inlines) => self.render_inlines(
                inlines,
                NOTES_FONT_SIZE,
                NOTES_LINE_HEIGHT,
                FontWeight::NORMAL,
                p.foreground,
            ),
            Block::List(items) => self.render_list(items, 0),
            Block::Quote(inlines) => div()
                .w_full()
                .min_w_0()
                .border_l(px(QUOTE_RULE_WIDTH))
                .border_color(hsla(self.quote_rule_color()))
                .pl(px(QUOTE_PADDING_LEFT))
                .child(self.render_inlines(
                    inlines,
                    NOTES_FONT_SIZE,
                    NOTES_LINE_HEIGHT,
                    FontWeight::NORMAL,
                    p.muted,
                ))
                .into_any_element(),
        }
    }

    /// The release notes card: the blocks stacked with CSS-style collapsed
    /// margins, first block flush with the top, last block flush with the bottom.
    fn render_notes(&self, height: f32) -> AnyElement {
        let p = self.palette;
        let mut notes = v_flex()
            .id("update-available-notes")
            .w_full()
            .min_w_0()
            .h(px(height))
            .flex_shrink_0()
            .overflow_y_scroll()
            .text_size(px(NOTES_FONT_SIZE))
            .line_height(px(NOTES_LINE_HEIGHT))
            .text_color(hsla(p.foreground));
        let mut previous_bottom: Option<f32> = None;
        let last = self.blocks.len().saturating_sub(1);
        for (index, block) in self.blocks.iter().enumerate() {
            let (top, bottom) = Self::block_margins(block);
            let gap = match previous_bottom {
                None => 0.0,
                Some(previous) => previous.max(top),
            };
            let ends_with_nested_list = matches!(
                block,
                Block::List(items) if items.last().is_some_and(|item| !item.children.is_empty())
            );
            let trailing = if index == last {
                if ends_with_nested_list {
                    BLOCK_MARGIN
                } else {
                    0.0
                }
            } else {
                0.0
            };
            notes = notes.child(
                div()
                    .w_full()
                    .min_w_0()
                    // CDXC:Release 2026-09-18 WHY:
                    // Shrinking blocks to the viewport hides their overflowing text from GPUI's scroll bounds, leaving release notes clipped and unable to scroll.
                    .flex_shrink_0()
                    .mt(px(gap))
                    .mb(px(trailing))
                    .child(self.render_block(block)),
            );
            previous_bottom = Some(bottom);
        }
        modal_panel(&p)
            .p(px(CARD_PADDING))
            .child(notes)
            .into_any_element()
    }

    fn render_body(&self, window: &Window) -> AnyElement {
        let p = self.palette;
        let portable_height = if self.portable { 12.0 + 19.5 } else { 0.0 };
        let notes_height = (f32::from(window.viewport_size().height)
            - MODAL_FIT_EXTRA_HEIGHT
            - HEADER_HEIGHT
            - CARD_PADDING * 2.0
            - 2.0
            - portable_height)
            .max(0.0);
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(self.render_notes(notes_height))
            .when(self.portable, |this| {
                this.child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(19.5))
                        .text_color(hsla(p.muted))
                        .child(PORTABLE_NOTE),
                )
            })
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let later = self.state != UpdateAvailableState::Available;
        modal_footer(vec![
            modal_action_button(
                &p,
                "update-available-cancel",
                if later { LATER } else { CANCEL },
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| this.cancel(window, cx),
                cx,
            ),
            modal_action_button(
                &p,
                "update-available-confirm",
                match self.state {
                    UpdateAvailableState::Ready => RESTART_AND_UPDATE,
                    UpdateAvailableState::Available => DOWNLOAD_UPDATE,
                    UpdateAvailableState::Notify => OPEN_DOWNLOAD_PAGE,
                },
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| this.confirm(window, cx),
                cx,
            ),
        ])
    }
}

impl Render for GpuiUpdateAvailableModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let title = if self.ready() {
            TITLE_READY
        } else {
            TITLE_AVAILABLE
        };
        let content = vec![
            modal_header(&p, title, Some(format!("Version {}", self.version))),
            self.render_body(window),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-update-available-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiUpdateAvailableModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
