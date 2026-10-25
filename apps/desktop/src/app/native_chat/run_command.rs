//! Run in terminal: what a shell command in a reply offers.
//!
//! CDXC:SessionChat 2026-10-24 SEE-ALSO: which fences and inline spans hold a command, and the text
//! each runs, is decided in packages/gx-chat-core/src/transcript/run_command.rs. A fence's arrives
//! as `run` on its header JSON and gets a Run button beside wrap and copy (`code_block.rs`). An
//! inline span's arrives as a `ghostex-run:` link drawn here as the span itself, with a small bar
//! above it on hover holding Copy and Paste in terminal, which never covers the span's text. The
//! presses are the core's `runInTerminal` and `pasteInTerminal` actions, which the host performs
//! (apps/desktop/src/app/session_chat/run_in_terminal.rs).

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::transcript::run_command::{RUN_COMMAND_HREF_PREFIX, command_from_href};
use gpui::{
    AnyElement, ClipboardItem, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, WeakEntity, div, px, svg,
};
use gpui_component::text::{InlineLink, InlineLinkAction, InlineLinkBar, InlineLinkFrame};
use serde_json::json;

const RUN_ICON: &str = "titlebar/terminal-2.svg";
const COPY_ICON: &str = "titlebar/copy.svg";
/// What an inline span's hover buttons press: the span's own `ghostex-run:` address under another
/// scheme, so a press on the span's text (which keeps it selectable) is told apart from them.
const COPY_PRESS_PREFIX: &str = "ghostex-run-copy:";
const PASTE_PRESS_PREFIX: &str = "ghostex-run-paste:";

/// Hands one command to the core, which asks the host for the terminal: `runInTerminal` types it
/// and presses Enter, `pasteInTerminal` only types it.
fn send(chat: &WeakEntity<NativeChatView>, action: &str, command: String, cx: &mut gpui::App) {
    let _ = chat.update(cx, |chat, cx| {
        chat.invoke(json!({"type": action, "command": command}), cx)
    });
}

/// The fence header's Run button: the terminal glyph with "Run" beside it, so it reads apart from
/// the copy button next to it.
pub(super) fn run_button(
    command: String,
    chat: &WeakEntity<NativeChatView>,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let chat = chat.clone();
    div()
        .id("run-fence")
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
                .path(RUN_ICON)
                .size(px(14.0 * s))
                .text_color(p.muted)
                .flex_shrink_0(),
        )
        .child(
            div()
                .text_size(px(11.0 * s))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.muted)
                .child("Run"),
        )
        // The header's own control consumes the press, like the copy button beside it.
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            send(&chat, "runInTerminal", command.clone(), cx);
        })
        .into_any_element()
}

/// The look an inline command span keeps once it is a link: the transcript's inline code chip, and
/// the bar of Copy and Paste in terminal above it while the pointer is on it, in the look of a
/// message's hover actions (`message_actions.rs`). `None` for any other link.
pub(super) fn inline_presentation(
    href: &str,
    label: &str,
    p: &ChatAppearance,
) -> Option<InlineLink> {
    command_from_href(href)?;
    let encoded = href.strip_prefix(RUN_COMMAND_HREF_PREFIX)?;
    let code = super::markdown_style::inline_code(p);
    let s = p.scale;
    Some(InlineLink {
        label: label.to_owned().into(),
        color: p.prose,
        font_family: Some(code.font_family.clone()),
        font_scale: Some(code.font_scale),
        frame: Some(InlineLinkFrame {
            padding_x: code.padding_x,
            border_width: code.border_width,
            border_color: code.border_color,
            radius: code.radius,
            hover_background: None,
            background: Some(code.background),
        }),
        hover_actions: vec![
            InlineLinkAction {
                url: format!("{COPY_PRESS_PREFIX}{encoded}").into(),
                label: "Copy".into(),
                icon: COPY_ICON.into(),
            },
            InlineLinkAction {
                url: format!("{PASTE_PRESS_PREFIX}{encoded}").into(),
                label: "Paste in terminal".into(),
                icon: RUN_ICON.into(),
            },
        ],
        hover_bar: Some(InlineLinkBar {
            background: p.input,
            border_color: p.border,
            border_width: px(1.0),
            radius: px(8.0 * s),
            padding: px(2.0 * s),
            gap: px(2.0 * s),
            spacing: px(2.0 * s),
            button_size: px(22.0 * s),
            button_radius: px(6.0 * s),
            button_hover_background: p.border.opacity(0.7),
            icon_size: px(14.0 * s),
            icon_color: p.muted,
        }),
        ..Default::default()
    })
}

/// A press on a link that belongs to an inline command, answered here: Copy copies the command,
/// Paste in terminal types it into a Commands panel terminal without pressing Enter, and a press on the span's
/// text does nothing, so it can be selected like any code. `false` for every other link.
pub(super) fn handle_link_press(
    href: &str,
    chat: &WeakEntity<NativeChatView>,
    cx: &mut gpui::App,
) -> bool {
    // Decoded through command_from_href, so a press address carries what a run link may: one line.
    if let Some(encoded) = href.strip_prefix(COPY_PRESS_PREFIX) {
        if let Some(command) = command_from_href(&format!("{RUN_COMMAND_HREF_PREFIX}{encoded}")) {
            crate::app::helpers::gpui_copy_to_clipboard(ClipboardItem::new_string(command), cx);
        }
        return true;
    }
    if let Some(encoded) = href.strip_prefix(PASTE_PRESS_PREFIX) {
        if let Some(command) = command_from_href(&format!("{RUN_COMMAND_HREF_PREFIX}{encoded}")) {
            send(chat, "pasteInTerminal", command, cx);
        }
        return true;
    }
    href.starts_with(RUN_COMMAND_HREF_PREFIX)
}
