//! A paste too long for the chat box: saved as a text file whose reference goes in the box, with a
//! toast whose Keep inline puts the text back instead.
//!
//! CDXC:Clipboard 2026-11-28 DECISION:
//! User (Q5363, "Long pastes: automatic or ask first?"): "Automatic, with Keep inline". A text paste
//! into the chat box longer than [`LONG_PASTE_BYTES`] is saved through the chat's attachment upload
//! as a text file and shows as the usual file tile with its `[File #N](path)` reference where the
//! text would have gone. A short toast says so, and its Keep inline button swaps the reference for
//! the pasted text, so one click undoes it. Pastes with pictures or files, and text under the limit,
//! paste as before.
//!
//! CDXC:Clipboard 2026-11-28 WHY:
//! 32 KB (about 8,000 tokens, or 600 to 800 lines of code) is far past anything typed by hand, so
//! only whole files and logs cross it; it is the same limit the other agent GUI uses. A chat message
//! is typed into the agent's terminal, and gxserver waits for it in proportion to its size
//! (`SESSION_CHAT_VERIFY_BYTES_PER_MS` in server/src/session_chat_send/constants.rs): 32 KB already
//! adds 16 seconds, and larger pastes are where "The terminal did not accept the pasted message"
//! came from. Every agent folds a paste far smaller than this into its own placeholder anyway
//! (`[Pasted text #1 +69 lines]` and the like), so the agent reading a file loses nothing it showed.
//! SEE-ALSO: apps/desktop/src/app/native_chat/long_paste.rs (desktop and web: the paste, the upload
//! and the toast), window/toast.rs `GpuiChatUndoToast::label`.

use crate::composer::references::{
    insert_path_references, reference_needs_leading_space, ComposerEdit,
};

/// Pasted text longer than this many bytes (UTF-8) goes in as a file.
pub const LONG_PASTE_BYTES: usize = 32 * 1024;

/// The name the saved file is given (gxserver puts a time stamp in front of it).
pub const PASTED_TEXT_FILE_NAME: &str = "pasted-text.txt";

/// The toast's title and its button.
pub const LONG_PASTE_TOAST_TITLE: &str = "Long paste added as a file";
pub const LONG_PASTE_TOAST_DESCRIPTION: &str = "The agent reads it from the file.";
pub const KEEP_INLINE_LABEL: &str = "Keep inline";

/// Whether `text`, pasted into the chat box, goes in as a file.
pub fn is_long_paste(text: &str) -> bool {
    text.len() > LONG_PASTE_BYTES
}

/// A long paste whose file reference was written into the chat box, kept so Keep inline can put
/// its text back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LongPaste {
    /// The saved file, as its reference names it.
    pub path: String,
    /// What was pasted.
    pub text: String,
    /// The reference got a space before it that the text would not have had.
    pub spaced: bool,
}

/// Writes the reference to the saved `path` over `[start, end)` (UTF-16 offsets, the selection
/// when the paste arrived), as any attachment's reference goes in, and remembers what Keep inline
/// needs.
pub fn insert_long_paste(
    draft: &str,
    path: &str,
    text: String,
    start: usize,
    end: usize,
) -> (ComposerEdit, LongPaste) {
    let spaced = reference_needs_leading_space(draft, start);
    let edit = insert_path_references(draft, &[path], start, end);
    (
        edit,
        LongPaste {
            path: path.to_string(),
            text,
            spaced,
        },
    )
}

/// Keep inline: the paste's reference (with the spaces its insertion added) becomes the pasted
/// text, the caret after it. `None` when the reference is no longer in `draft` (removed, or sent).
pub fn keep_inline(draft: &str, paste: &LongPaste) -> Option<ComposerEdit> {
    let close = format!("]({})", paste.path);
    let close_at = draft.find(&close)?;
    let mut start = draft[..close_at].rfind('[')?;
    let mut end = close_at + close.len();
    if draft[end..].starts_with(' ') {
        end += 1;
    }
    if paste.spaced && draft[..start].ends_with(' ') {
        start -= 1;
    }
    let before = &draft[..start];
    Some(ComposerEdit {
        text: format!("{before}{}{}", paste.text, &draft[end..]),
        caret: before.encode_utf16().count() + paste.text.encode_utf16().count(),
    })
}
