//! Run in terminal: what a shell command in a reply offers.
//!
//! CDXC:SessionChat 2026-10-24 SEE-ALSO: which fences and inline spans hold a command, and the text
//! each runs, is decided in packages/gx-chat-core/src/transcript/run_command.rs. A fence's arrives
//! as `run` on its header JSON and gets an icon-only Run in terminal button beside wrap and copy (`code_block.rs`). An
//! inline span's arrives as a `ghostex-run:` link drawn here as the span itself, with a small bar
//! at its bottom right corner on hover holding Copy and Paste in terminal, which never covers the
//! span's text. The presses are the core's `runInTerminal` and `pasteInTerminal` actions, which the host performs
//! (apps/desktop/src/app/session_chat/run_in_terminal.rs).

use super::{appearance::ChatAppearance, state::NativeChatView};
use ghostex_gx_chat_core::transcript::run_command::{RUN_COMMAND_HREF_PREFIX, command_from_href};
use gpui::{AnyElement, ClipboardItem, WeakEntity, px};
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

/// The fence header's Run button: the terminal glyph alone, like the wrap and copy buttons beside it.
pub(super) fn run_button(
    command: String,
    chat: &WeakEntity<NativeChatView>,
    p: &ChatAppearance,
) -> AnyElement {
    let chat = chat.clone();
    // The header's own control consumes the press, like the copy button beside it.
    super::code_block::action("run-fence", RUN_ICON, "Run in terminal", p, move |cx| {
        send(&chat, "runInTerminal", command.clone(), cx);
    })
}

/// The look an inline command span keeps once it is a link: the transcript's inline code chip, and
/// the bar of Copy and Paste in terminal at its bottom right corner while the pointer is on it, in
/// the chat right-click menu's look (`hover_bar_look`). `None` for any other link.
///
/// CDXC:SessionChat 2026-11-13 DECISION: User: "For the ones we show next to a command, please make those appear on the bottom right of the terminal command box, not the top left like we do now." The bar hangs below the span, right-aligned to its right edge (above it, still right-aligned, only when the transcript has no room below); the gpui-component fork places it (`text/inline_link.rs`, `text/inline_flow.rs`).
///
/// CDXC:Tooltips 2026-10-25 WHY: The bar is drawn by the transcript's text element inside the chat's own window, whose hover moves between the span and the bar; a frosted host window would take the pointer off the span and drop the bar, so it takes the menu's in-window fill (`ChatMenuGlass::in_window`) instead of a frosted host.
pub(super) fn inline_presentation(
    href: &str,
    label: &str,
    p: &ChatAppearance,
) -> Option<InlineLink> {
    command_from_href(href)?;
    let encoded = href.strip_prefix(RUN_COMMAND_HREF_PREFIX)?;
    let code = super::markdown_style::inline_code(p);
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
        hover_bar: Some(hover_bar_look(p)),
        ..Default::default()
    })
}

/// The look of an inline command's hover bar, which the transcript selection's bar shares
/// (`selection_bar.rs`): the chat right-click menu's material drawn in the chat's window
/// (`ChatMenuGlass::in_window`, CDXC:SessionChat 2026-11-13), and icon buttons in the menu's ink.
pub(super) fn hover_bar_look(p: &ChatAppearance) -> InlineLinkBar {
    let s = p.scale;
    let glass = super::option_menu::ChatMenuGlass::in_window(p);
    InlineLinkBar {
        background: glass.fill,
        border_color: glass.edge,
        border_width: px(1.0),
        radius: glass.radius,
        padding: px(2.0 * s),
        gap: px(2.0 * s),
        spacing: px(2.0 * s),
        button_size: px(22.0 * s),
        button_radius: px(6.0 * s),
        button_hover_background: glass.hover,
        icon_size: px(14.0 * s),
        icon_color: glass.ink,
        shadow: Vec::new(),
    }
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
