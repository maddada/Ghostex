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
    label: String,
    icon: String,
    #[serde(default)]
    href: Option<String>,
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

fn action(
    id: &'static str,
    icon: &'static str,
    p: &ChatAppearance,
    click: impl Fn(&mut App) + 'static,
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
/// (`code-wrap:${node.position.start.offset}` in session-chat-markdown.tsx), so a block keeps its
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
/// the name really is a path the host can open.
pub(super) fn header(
    block: &CodeBlock,
    id: &str,
    wrapped: bool,
    chat: &WeakEntity<NativeChatView>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let key = wrap_key(id, block);
    let named = parse(block.meta().as_deref());
    let language = block
        .lang()
        .map(|lang| lang.to_string())
        .unwrap_or_else(|| "code".to_owned());
    let label = named
        .as_ref()
        .map(|header| header.label.clone())
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
                .child({
                    let chat = chat.clone();
                    action("wrap-fence", "titlebar/text-wrap.svg", p, move |cx| {
                        let key = key.clone();
                        let _ = chat.update(cx, |chat, cx| {
                            chat.set_code_wrap(key, !wrapped, cx);
                        });
                    })
                })
                .when_some(open, |this, href| {
                    let chat = chat.clone();
                    this.child(action(
                        "open-fence-file",
                        "titlebar/external-link.svg",
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
                .child(action("copy-fence", "titlebar/copy.svg", p, move |cx| {
                    crate::app::helpers::gpui_copy_to_clipboard(
                        ClipboardItem::new_string(code.clone()),
                        cx,
                    );
                })),
        )
        .into_any_element()
}
