//! The transcript's right-click menu, the Add to Chat quote and the Reply popover's draft.
//!
//! Port of `packages/shared/session-chat-presentation/transcript-menu.ts`.
//!
//! CDXC:SessionChat 2026-09-19 SEE-ALSO:
//! GPUI asks for these rows from `apps/desktop/src/app/native_chat/transcript_menu.rs`. Copy
//! stands alone, disabled, on plain transcript with nothing selected, so the menu never opens
//! empty.

use crate::composer::reference_menu::{reference_menu_rows, ReferenceMenuRow};
use crate::ordered;

/// Which selection row this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptMenuItemId {
    Copy,
    AddToChat,
    Reply,
}

/// One selection row, before its command is attached.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptMenuItem {
    pub id: TranscriptMenuItemId,
    pub label: &'static str,
    pub icon_path: &'static str,
    pub disabled: bool,
}

/// The selection rows of the transcript's right-click menu, after any reference rows.
pub fn transcript_menu_items(
    selection: &str,
    on_reference: bool,
    question_active: bool,
) -> Vec<TranscriptMenuItem> {
    let mut items = Vec::new();
    if !on_reference || !selection.is_empty() {
        items.push(TranscriptMenuItem {
            id: TranscriptMenuItemId::Copy,
            label: "Copy",
            icon_path: "titlebar/copy.svg",
            disabled: selection.is_empty(),
        });
    }
    if !selection.is_empty() {
        items.push(TranscriptMenuItem {
            id: TranscriptMenuItemId::AddToChat,
            label: "Add to Chat",
            icon_path: "titlebar/blockquote.svg",
            disabled: question_active,
        });
        items.push(reply_item(question_active));
    }
    items
}

/// The Reply row, after Add to Chat and under the same rule.
///
/// CDXC:SessionChat 2026-10-29 DECISION:
/// User: "i need a new optino here called \"Reply\" and when i click on that i should see a popup just like this where i can type a reply [Claude's reply popover: a text field, \"Add to prompt ⏎\" and \"Send reply ⌘⏎\"] and when i reply we need to add the reply to the end of chat box". Reply shows whenever Add to Chat does and is disabled when it is. Its popover's Add to prompt (Enter) appends the selection quoted as Add to Chat quotes it, then the typed reply under it, to the end of the chat box (`transcript_reply_draft`); Send reply (Cmd+Enter, Ctrl+Enter off macOS) does the same and then sends the chat box as Send would. Shift+Enter is a new line and Escape or a click outside closes it without changes. Desktop: `apps/desktop/src/app/native_chat/reply_popover.rs`; phone: `apps/mobile/app/src/chat/native/transcript/ReplySheet.tsx`.
fn reply_item(question_active: bool) -> TranscriptMenuItem {
    TranscriptMenuItem {
        id: TranscriptMenuItemId::Reply,
        label: "Reply",
        icon_path: "titlebar/message-circle.svg",
        disabled: question_active,
    }
}

/// Text as a Markdown blockquote, one `>` per line, with bare `>` for blank lines.
pub fn markdown_quote(text: &str) -> String {
    normalize_line_endings(text)
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `/\r\n?/g` to `\n`: a lone carriage return is a line break too.
fn normalize_line_endings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
            out.push('\n');
        } else {
            out.push(character);
        }
    }
    out
}

/// What Add to Chat appends: the selection quoted, then a blank line before the caret.
///
/// CDXC:SessionChat 2026-09-22 DECISION:
/// User: Add to Chat adds one more newline between the quoted message and my text so Markdown renders correctly. This supersedes the 2026-09-07 single-newline decision (`sessionChatTranscriptQuote` in `transcript-menu.ts` carries the same one).
pub fn transcript_quote(selection: &str) -> String {
    format!("{}\n\n", markdown_quote(selection))
}

/// The composer after appending `text` to `current`: a blank line separates it from earlier text,
/// reusing whatever line breaks the draft already ends with.
pub fn append_draft_text(current: &str, text: &str) -> String {
    let separator = if current.is_empty() || current.ends_with("\n\n") {
        ""
    } else if current.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{current}{separator}{text}")
}

/// The chat box after a Reply: `current` with the selection's quote (`quote`, the Reply row's
/// `quote`, which is Add to Chat's text) and the typed reply under it appended at its end, the way
/// Add to Chat appends. An empty reply adds the quote alone, exactly as Add to Chat does.
pub fn transcript_reply_draft(current: &str, quote: &str, reply: &str) -> String {
    append_draft_text(current, &format!("{quote}{}", reply.trim()))
}

/// GPUI's whole transcript menu: the reference rows for `href`, then the selection rows with their
/// commands.
pub fn transcript_menu_rows(
    href: Option<&str>,
    selection: &str,
    question_active: bool,
) -> Vec<ReferenceMenuRow> {
    // `input.href ? … : []` treats an empty string as no href, the way a falsy value does.
    let references = match href {
        Some(href) if !href.is_empty() => reference_menu_rows(href),
        _ => Vec::new(),
    };
    let items = transcript_menu_items(selection, !references.is_empty(), question_active);
    let mut rows = references;
    for item in items {
        rows.push(ReferenceMenuRow {
            command: match item.id {
                TranscriptMenuItemId::Copy => {
                    ordered! { "text": selection, "type": "copyText" }
                }
                TranscriptMenuItemId::AddToChat => ordered! {
                    "text": transcript_quote(selection),
                    "type": "appendToDraft",
                },
                TranscriptMenuItemId::Reply => ordered! {
                    "quote": transcript_quote(selection),
                    "type": "replyToSelection",
                },
            },
            icon_path: item.icon_path.to_string(),
            label: item.label.to_string(),
            disabled: Some(item.disabled),
        });
    }
    rows
}
