use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, ClipboardItem, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px, rgb, svg,
};
use gpui_component::text::TextView;
use serde_json::{Value, json};

/*
 * Rich Markdown in the native transcript.
 *
 * Everything that needs a decision about the Markdown (which fence names a
 * file, which quote is a GitHub alert, where a table starts, which typed word
 * is a path) is decided in the shared projection and arrives marked with a
 * private-use character. This module only splits on those marks and lays the
 * pieces out.
 *
 * CDXC:SessionChat 2026-09-18 SEE-ALSO:
 * The marks are written by the core's native Markdown pass
 * (packages/gx-chat-core/src/transcript/native_markdown.rs). Alert colours
 * mirror the --alert-* families the React chat's theme rules defined in
 * packages/core-ui/styles/chat.css until 2026-09-25.
 */

const ALERT_OPEN: &str = "\u{E000}alert:";
const ALERT_CLOSE: &str = "\u{E000}/alert";
const TABLE_OPEN: &str = "\u{E000}table";
const TABLE_CLOSE: &str = "\u{E000}/table";

const IMAGE_OPEN: &str = "\u{E000}image:";
const MARK: char = '\u{E000}';

/// Hovering anywhere over a table is what brings its actions up.
const TABLE_GROUP: &str = "native-chat-table";

enum Segment {
    Markdown(String),
    Alert {
        kind: String,
        children: Vec<Segment>,
    },
    Table(String),
    /// A line that has a picture in it, as the run of text and pictures it is made of.
    Flow(Vec<FlowPart>),
}

enum FlowPart {
    Text(String),
    Image(Value),
}

/// Splits one marked line into the text and pictures it was written as.
///
/// A `TextView` lays out one string, so a picture inside a sentence cannot be an inline element of
/// it; the line becomes a wrapping row of short text views and thumbnails instead, which reads the
/// same for the prompts and captions pictures are actually written into.
fn flow(line: &str) -> Vec<FlowPart> {
    fn push_text(parts: &mut Vec<FlowPart>, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            parts.push(FlowPart::Text(text.to_owned()));
        }
    }
    let mut parts = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(IMAGE_OPEN) {
        let payload = &rest[start + IMAGE_OPEN.len()..];
        let Some(end) = payload.find(MARK) else {
            break;
        };
        let Ok(image) = serde_json::from_str::<Value>(&payload[..end]) else {
            break;
        };
        push_text(&mut parts, &rest[..start]);
        parts.push(FlowPart::Image(image));
        rest = &payload[end + MARK.len_utf8()..];
    }
    push_text(&mut parts, rest);
    parts
}

/// GitHub's five alert kinds: label, glyph, and the accent each one is tinted with.
fn alert_style(kind: &str, light: bool) -> (&'static str, &'static str, gpui::Hsla) {
    let (label, icon, light_color, dark_color) = match kind {
        "tip" => ("Tip", "titlebar/bulb.svg", 0x009966, 0x00d492),
        "important" => (
            "Important",
            "titlebar/message-report.svg",
            0x9810fa,
            0xc27aff,
        ),
        "warning" => ("Warning", "titlebar/alert-triangle.svg", 0xe17100, 0xfe9a00),
        "caution" => ("Caution", "titlebar/alert-octagon.svg", 0xe7000b, 0xff6467),
        _ => ("Note", "titlebar/info-circle.svg", 0x1447e6, 0x51a2ff),
    };
    (
        label,
        icon,
        rgb(if light { light_color } else { dark_color }).into(),
    )
}

fn parse(lines: &[&str]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let mut index = 0;
    let flush = |prose: &mut Vec<&str>, segments: &mut Vec<Segment>| {
        if prose.iter().any(|line| !line.trim().is_empty()) {
            segments.push(Segment::Markdown(prose.join("\n")));
        }
        prose.clear();
    };
    while index < lines.len() {
        let line = lines[index];
        if let Some(kind) = line.strip_prefix(ALERT_OPEN) {
            let mut depth = 1usize;
            let mut end = index + 1;
            while end < lines.len() && depth > 0 {
                if lines[end].starts_with(ALERT_OPEN) {
                    depth += 1;
                } else if lines[end] == ALERT_CLOSE {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                end += 1;
            }
            flush(&mut prose, &mut segments);
            segments.push(Segment::Alert {
                kind: kind.trim().to_owned(),
                children: parse(&lines[index + 1..end.min(lines.len())]),
            });
            index = end + 1;
            continue;
        }
        if line == TABLE_OPEN {
            let end = lines[index + 1..]
                .iter()
                .position(|line| *line == TABLE_CLOSE)
                .map(|offset| index + 1 + offset)
                .unwrap_or(lines.len());
            flush(&mut prose, &mut segments);
            segments.push(Segment::Table(
                lines[index + 1..end.min(lines.len())].join("\n"),
            ));
            index = end + 1;
            continue;
        }
        if line.contains(IMAGE_OPEN) {
            flush(&mut prose, &mut segments);
            segments.push(Segment::Flow(flow(line)));
            index += 1;
            continue;
        }
        prose.push(line);
        index += 1;
    }
    flush(&mut prose, &mut segments);
    segments
}

/// The cells of one GFM table row, with the escaped pipes put back.
fn table_row_cells(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut characters = line.trim().trim_matches('|').chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' if characters.peek() == Some(&'|') => {
                characters.next();
                if let Some(cell) = cells.last_mut() {
                    cell.push('|');
                }
            }
            '|' => cells.push(String::new()),
            _ => {
                if let Some(cell) = cells.last_mut() {
                    cell.push(character);
                }
            }
        }
    }
    cells
        .into_iter()
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// A cell's words with the inline markers taken off, which is what a spreadsheet wants.
///
/// React reads the rendered cell instead (session-chat-table-clipboard.ts), because its chips know
/// their own source text; the native table renders the Markdown itself, so the source is read here.
fn table_cell_text(cell: &str) -> String {
    let mut text = cell.replace("**", "").replace('`', "");
    while let Some(open) = text.find("](") {
        let Some(label_start) = text[..open].rfind('[') else {
            break;
        };
        let Some(close) = text[open + 2..].find(')') else {
            break;
        };
        let label = text[label_start + 1..open].to_owned();
        text.replace_range(label_start..open + 2 + close + 1, &label);
    }
    text
}

/// GFM's delimiter row, the one line of a table that is punctuation rather than content.
fn is_table_delimiter_row(line: &str) -> bool {
    let body = line.trim();
    !body.is_empty()
        && body
            .chars()
            .all(|character| matches!(character, '|' | '-' | ':' | ' '))
        && body.contains('-')
}

/// React's `sessionChatTableToCsv`: the cells as they read, quoted only when they have to be.
pub(super) fn table_csv(source: &str) -> String {
    source
        .lines()
        .filter(|line| line.contains('|') && !is_table_delimiter_row(line))
        .map(|line| {
            table_row_cells(line)
                .iter()
                .map(|cell| {
                    let value = table_cell_text(cell);
                    if value.contains(['"', '\n', ',']) {
                        format!("\"{}\"", value.replace('"', "\"\""))
                    } else {
                        value
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One button in the table's toolbar.
pub(super) fn table_action(
    id: &'static str,
    icon: &'static str,
    p: &ChatAppearance,
    click: impl Fn(&mut gpui::App) + 'static,
) -> AnyElement {
    let s = p.scale;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0 * s))
        .rounded(px(6.0 * s))
        .chat_cursor_pointer()
        .hover(|style| style.bg(p.border.opacity(0.7)))
        .child(
            svg()
                .path(icon)
                .size(px(14.0 * s))
                .text_color(p.muted)
                .flex_shrink_0(),
        )
        // A table's own control consumes the press, so the heading row behind
        // it does not read it as a click on its own text.
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            click(cx)
        })
        .into_any_element()
}

/// A copy button in the table's toolbar, named for the format it copies.
///
/// CDXC:SessionChat 2026-09-24 DECISION:
/// The two copy buttons "both just copy but one as csv and one as md, which isn't clear from the icons, maybe write md and csv next to the icons": both show the copy icon with the format written beside it.
pub(super) fn table_copy_action(
    id: &'static str,
    format: &'static str,
    p: &ChatAppearance,
    click: impl Fn(&mut gpui::App) + 'static,
) -> AnyElement {
    let s = p.scale;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(3.0 * s))
        .h(px(22.0 * s))
        .px(px(5.0 * s))
        .rounded(px(6.0 * s))
        .chat_cursor_pointer()
        .hover(|style| style.bg(p.border.opacity(0.7)))
        .child(
            svg()
                .path("titlebar/copy.svg")
                .size(px(14.0 * s))
                .text_color(p.muted)
                .flex_shrink_0(),
        )
        .child(
            div()
                .text_size(px(11.0 * s))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.muted)
                .child(format),
        )
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            click(cx)
        })
        .into_any_element()
}

/// The right-aligned toolbar under a table: what a reader can do with it.
///
/// It keeps its row of space and fades in with the pointer, because a two-by-two
/// table of short cells is the common case and deserves no chrome at all. It
/// opens the table in the larger preview, and copies it as Markdown or as CSV.
fn table_actions(
    source: String,
    chat: &gpui::WeakEntity<NativeChatView>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let markdown = source.clone();
    let csv = source.clone();
    let open_chat = chat.clone();
    div()
        .flex()
        .justify_end()
        .w_full()
        .gap(px(2.0 * s))
        .opacity(0.0)
        .group_hover(TABLE_GROUP, |style| style.opacity(1.0))
        .child(table_action(
            "open-table",
            "titlebar/external-link.svg",
            p,
            move |cx| {
                let source = source.clone();
                let _ = open_chat.update(cx, |chat, cx| chat.open_table_preview(source, cx));
            },
        ))
        .child(table_copy_action("copy-table", "MD", p, move |cx| {
            crate::app::helpers::gpui_copy_to_clipboard(
                ClipboardItem::new_string(markdown.clone()),
                cx,
            );
        }))
        .child(table_copy_action("copy-table-csv", "CSV", p, move |cx| {
            crate::app::helpers::gpui_copy_to_clipboard(
                ClipboardItem::new_string(table_csv(&csv)),
                cx,
            );
        }))
        .into_any_element()
}

impl NativeChatView {
    /// The reader turned wrapping on or off for one fenced block.
    ///
    /// Only that block reflows, so the blocks already on screen keep the reader's place; the choice
    /// becomes the one every block mounted after it starts with, which is what React remembers in
    /// session-chat-code-wrap.ts.
    pub(super) fn set_code_wrap(
        &mut self,
        key: String,
        wrapped: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.code_wrap.insert(key, wrapped);
        self.code_wrap_default = wrapped;
        self.list.remeasure();
        cx.notify();
    }

    fn text_view(
        &self,
        id: String,
        content: String,
        references: &Value,
        p: &ChatAppearance,
        cx: &gpui::Context<Self>,
    ) -> AnyElement {
        let chat = cx.weak_entity();
        let menu_chat = chat.clone();
        let header_chat = chat.clone();
        let header_appearance = p.clone();
        let mut style = super::markdown_style::text_style(p);
        style.is_dark = !p.light;
        style.highlight_theme = super::markdown_style::highlight_theme(p.light);
        // React's `--chat-table-cell-max`, min(24rem, 60cqw): one long cell cannot claim the
        // whole row, and a narrow pane lowers the cap so a wide table usually just fits.
        let pane_width = f32::from(self.bounds.get().size.width);
        let mut cell_max = 384.0 * p.scale;
        if pane_width > 0.0 {
            cell_max = cell_max.min(pane_width * 0.6);
        }
        style.table_cell_max_width = Some(px(cell_max));
        // CDXC:SessionChat 2026-09-25 DECISION: collapsed cells cut their text off, so the user
        // took the option away: every table wraps its cells inside the capped columns, and a table
        // wider than the pane scrolls sideways. Supersedes the 2026-09-24 collapse toggle.
        style.table_wrap_cells = true;
        let references = super::markdown_links::presentations(references, p);
        let header_id = id.clone();
        let wrap_id = id.clone();
        let wrap_choices = self.code_wrap.clone();
        let wrap_default = self.code_wrap_default;
        let wraps = move |block: &gpui_component::text::CodeBlock| {
            wrap_choices
                .get(&super::code_block::wrap_key(&wrap_id, block))
                .copied()
                .unwrap_or(wrap_default)
        };
        let header_wraps = wraps.clone();
        TextView::markdown(id, content)
            .min_w_0()
            .max_w(gpui::relative(1.0))
            .link_presentation(move |href, label| {
                references
                    .get(&(href.to_owned(), label.to_owned()))
                    .cloned()
            })
            .on_link_click(move |href, event, _, cx| {
                // Only a primary click opens a link; a right press opens the link menu below.
                if !event.standard_click() {
                    return;
                }
                let _ = chat.update(cx, |chat, cx| {
                    chat.invoke(
                        json!({"type":"openMarkdownLink","href":href,"external":event.modifiers().shift}),
                        cx,
                    )
                });
            })
            .on_link_secondary_click(super::markdown_links::secondary_click(menu_chat))
            .code_block_actions(move |block, _, _| {
                super::code_block::header(
                    block,
                    &header_id,
                    header_wraps(block),
                    &header_chat,
                    &header_appearance,
                )
            })
            // React's fences scroll sideways until the reader asks for wrapping
            // (session-chat-code-wrap.ts); the native ones follow the same choice.
            .code_block_wrap(wraps)
            .selectable(true)
            .style(style)
            .text_size(px(14.0 * p.scale))
            .line_height(px(22.75 * p.scale))
            .text_color(p.prose)
            .into_any_element()
    }

    fn segment(
        &self,
        id: &str,
        index: usize,
        segment: &Segment,
        references: &Value,
        p: &ChatAppearance,
        cx: &gpui::Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        match segment {
            Segment::Markdown(content) => {
                self.text_view(format!("{id}:{index}"), content.clone(), references, p, cx)
            }
            Segment::Table(source) => {
                let key = format!("{id}:{index}");
                div()
                    .group(TABLE_GROUP)
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .gap(px(2.0 * s))
                    .child(self.text_view(key.clone(), source.clone(), references, p, cx))
                    .child(table_actions(source.clone(), &cx.weak_entity(), p))
                    .into_any_element()
            }
            // The air around an inline picture is the picture's own margin, exactly as React's
            // frame carries it, so the row itself adds nothing between a word and the picture
            // beside it. It wraps because a sentence with two pictures in it still has to fit the
            // transcript's width.
            Segment::Flow(parts) => div()
                .flex()
                .flex_wrap()
                .items_center()
                .w_full()
                .min_w_0()
                .children(parts.iter().enumerate().map(|(part, item)| {
                    match item {
                        // The words either side of the picture have to be allowed to shrink below
                        // their own longest line, or a sentence with a picture in it runs past the
                        // bubble's edge instead of wrapping inside it.
                        FlowPart::Text(content) => div()
                            .min_w_0()
                            .child(self.text_view(
                                format!("{id}:{index}:{part}"),
                                content.clone(),
                                references,
                                p,
                                cx,
                            ))
                            .into_any_element(),
                        FlowPart::Image(image) => {
                            self.inline_image(&format!("{id}:{index}:{part}"), image, p, cx)
                        }
                    }
                }))
                .into_any_element(),
            Segment::Alert { kind, children } => {
                let (label, icon, accent) = alert_style(kind, p.light);
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .gap(px(4.8 * s))
                    .border_l(px(2.0 * s))
                    .border_color(accent.opacity(0.7))
                    .pl(px(12.8 * s))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .text_color(accent)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                svg()
                                    .path(icon)
                                    .size(px(15.0 * s))
                                    .text_color(accent)
                                    .flex_shrink_0(),
                            )
                            .child(label),
                    )
                    .children(children.iter().enumerate().map(|(child, segment)| {
                        self.segment(&format!("{id}:{index}"), child, segment, references, p, cx)
                    }))
                    .into_any_element()
            }
        }
    }

    /// One message body, as the transcript renders it.
    pub(super) fn rich_markdown(
        &self,
        id: String,
        content: String,
        references: &Value,
        p: &ChatAppearance,
        cx: &gpui::Context<Self>,
    ) -> AnyElement {
        if !content.contains('\u{E000}') {
            return self.text_view(id, content, references, p, cx);
        }
        let lines = content.lines().collect::<Vec<_>>();
        let segments = parse(&lines);
        if segments.is_empty() {
            return self.text_view(id, String::new(), references, p, cx);
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(super::markdown_style::paragraph_gap(p)))
            .children(
                segments
                    .iter()
                    .enumerate()
                    .map(|(index, segment)| self.segment(&id, index, segment, references, p, cx)),
            )
            .into_any_element()
    }
}
