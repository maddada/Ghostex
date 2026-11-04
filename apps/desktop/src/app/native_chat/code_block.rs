use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClipboardItem, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, div, px, svg,
};
use gpui_component::text::CodeBlock;
use serde::Deserialize;
use serde_json::json;

/// What the shared presentation decided this fence's header shows.
///
/// CDXC:SessionChat 2026-09-18 SEE-ALSO:
/// Written by the core's native Markdown pass in packages/gx-chat-core/src/transcript/native_markdown.rs.
#[derive(Deserialize)]
struct FenceHeader {
    /// The file the fence names; absent on a fence that only carries a command.
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    href: Option<String>,
    /// The command its Run button runs, percent-encoded (`run_command.rs` in gx-chat-core).
    #[serde(default)]
    run: Option<String>,
}

/// The private-use character the shared projection marks a fence's header with.
const MARK: char = '\u{E000}';

fn parse(meta: Option<&str>) -> Option<FenceHeader> {
    let (_, marked) = meta?.split_once(MARK)?;
    serde_json::from_str(marked).ok()
}

fn icon_path(icon: &str) -> &'static str {
    match icon {
        "markdown" => "titlebar/markdown.svg",
        "file-code" => "titlebar/file-code.svg",
        _ => "titlebar/file.svg",
    }
}

/// One of the fence header's icon buttons: a 22px square with a 14px glyph, a hover fill and the
/// app's tooltip bubble naming it.
///
/// CDXC:SessionChat 2026-10-27 DECISION: User, of a code block's header: "for these 3 buttons we should have tooltip. run shouldn't be the only one with text, all 3 buttons should be icon only". Wrap lines, Run in terminal and Copy are the same icon-only button, each with a tooltip.
pub(super) fn action(
    id: impl Into<gpui::ElementId>,
    icon: &'static str,
    label: impl Into<gpui::SharedString>,
    p: &ChatAppearance,
    click: impl Fn(&mut App) + 'static,
) -> AnyElement {
    let s = p.scale;
    let label: gpui::SharedString = label.into();
    let tooltip = label.clone();
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.0 * s))
        .rounded(px(6.0 * s))
        .chat_cursor_pointer()
        .hover(|style| style.bg(p.border.opacity(0.7)))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .child(
            svg()
                .path(icon)
                .size(px(14.0 * s))
                .text_color(p.muted)
                .flex_shrink_0(),
        )
        // A fence's own control consumes the press: the row behind it (an
        // assistant heading that opens its tool calls) is a trigger too, and
        // React excludes a `button` from it the same way.
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            click(cx)
        })
        .into_any_element()
}

/// Which block a wrap choice belongs to: the message's own Markdown plus where the fence starts.
///
/// React keys the same state on the fence's start offset
/// (`code-wrap:${node.position.start.offset}` in session-chat-markdown.tsx (deleted 2026-10-01)), so a block keeps its
/// choice while a turn streams and loses it only when the fence itself moves.
pub(super) fn wrap_key(id: &str, block: &CodeBlock) -> String {
    match block.span {
        Some(span) => format!("{id}:{}", span.start),
        None => format!("{id}:{}", block.code().len()),
    }
}

/// The row above a fenced block: what the fence is, and what can be done with it.
///
/// The label is the file the fence named when it named one (with the same glyph
/// an inline path carries) and the bare language otherwise, which is every fence
/// an agent writes without meta. Copy is always there; open is there only when
/// the name really is a path the host can open. A long block (`expanded` is
/// `Some`) also gets the button that shows it whole or caps it again.
pub(super) fn header(
    block: &CodeBlock,
    id: &str,
    wrapped: bool,
    expanded: Option<bool>,
    chat: &WeakEntity<NativeChatView>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let key = wrap_key(id, block);
    // A fence written without a language carries the core's header as its only info word.
    let lang = block.lang().map(|lang| lang.to_string());
    let header_word = lang.as_deref().filter(|lang| lang.starts_with(MARK));
    let header = parse(header_word.or(block.meta().as_deref()));
    let run = header
        .as_ref()
        .and_then(|header| header.run.as_deref())
        .and_then(ghostex_gx_chat_core::transcript::run_command::decode_command);
    let named = header.filter(|header| header.label.is_some());
    let language = lang
        .filter(|lang| !lang.starts_with(MARK))
        .unwrap_or_else(|| "code".to_owned());
    let label = named
        .as_ref()
        .and_then(|header| header.label.clone())
        .unwrap_or_else(|| language.to_lowercase());
    let code = block.code().to_string();
    let open = named.as_ref().and_then(|header| header.href.clone());
    let chat = chat.clone();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0 * s))
        .w_full()
        .pl(px(12.0 * s))
        .pr(px(6.0 * s))
        .py(px(2.0 * s))
        .border_b(px(1.0))
        .border_color(p.border)
        .child(
            div()
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(5.0 * s))
                .text_size(px(11.0 * s))
                .text_color(p.muted)
                .when_some(named.as_ref(), |this, header| {
                    this.child(
                        svg()
                            .path(icon_path(&header.icon))
                            .size(px(13.0 * s))
                            .text_color(p.muted)
                            .flex_shrink_0(),
                    )
                })
                .child(div().min_w_0().truncate().child(label)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(2.0 * s))
                .when_some(expanded, |this, expanded| {
                    let chat = chat.clone();
                    let key = key.clone();
                    let (icon, label) = if expanded {
                        ("titlebar/arrows-diagonal-minimize.svg", "Collapse code")
                    } else {
                        ("titlebar/arrows-diagonal.svg", "Show all code")
                    };
                    this.child(action("expand-fence", icon, label, p, move |cx| {
                        let key = key.clone();
                        let _ = chat.update(cx, |chat, cx| {
                            chat.set_code_expanded(key, !expanded, cx);
                        });
                    }))
                })
                .child({
                    let chat = chat.clone();
                    let label = if wrapped {
                        "Don't wrap lines"
                    } else {
                        "Wrap lines"
                    };
                    action(
                        "wrap-fence",
                        "titlebar/text-wrap.svg",
                        label,
                        p,
                        move |cx| {
                            let key = key.clone();
                            let _ = chat.update(cx, |chat, cx| {
                                chat.set_code_wrap(key, !wrapped, cx);
                            });
                        },
                    )
                })
                .when_some(open, |this, href| {
                    let chat = chat.clone();
                    this.child(action(
                        "open-fence-file",
                        "titlebar/external-link.svg",
                        "Open file",
                        p,
                        move |cx| {
                            let _ = chat.update(cx, |chat, cx| {
                                chat.invoke(
                                    json!({"type":"openMarkdownLink","href":href,"external":false}),
                                    cx,
                                )
                            });
                        },
                    ))
                })
                .when_some(run, |this, command| {
                    this.child(super::run_command::run_button(command, &chat, p))
                })
                .child(action(
                    "copy-fence",
                    "titlebar/copy.svg",
                    "Copy",
                    p,
                    move |cx| {
                        crate::app::helpers::gpui_copy_to_clipboard(
                            ClipboardItem::new_string(code.clone()),
                            cx,
                        );
                    },
                )),
        )
        .into_any_element()
}
