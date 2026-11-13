//! The chat box's attachment references (`[Image #1](path)`, `[File #2](path)`, …) whose file is
//! on another computer than the session's: which ones the draft holds, and the draft once the host
//! copied their files over.
//!
//! CDXC:Clipboard 2026-11-13 DECISION: User: an image pill pasted in a PC session's chat, cut and pasted into a Mac session's chat, kept its `C:\…` path, so the hover thumbnail was empty and the Mac agent got a path it can't open. A reference whose file sits on another computer than the session's gets the file copied to the session's computer (as a paste into a remote session's chat box uploads it) and its path rewritten to the copy, when the text is pasted into the chat box and again before a send; the original file is left alone.
//! SEE-ALSO: apps/desktop/src/app/native_chat/carry_attachments.rs (the desktop and web host that copies the files).

use crate::composer::reference_pills::{composer_references, is_attachment_label};

/// The paths of `text`'s attachment references, each once, in order.
pub fn attachment_reference_paths(text: &str) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for reference in composer_references(text) {
        if is_attachment_label(&reference.label) && !paths.contains(&reference.path) {
            paths.push(reference.path);
        }
    }
    paths
}

/// `text` with every attachment reference whose path is the first of a `moved` pair pointing at
/// the second. Other references, and words that only mention the old path, stay as they are.
pub fn retarget_attachment_references(text: &str, moved: &[(String, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for reference in composer_references(text) {
        if !is_attachment_label(&reference.label) {
            continue;
        }
        let Some((_, to)) = moved.iter().find(|(from, _)| *from == reference.path) else {
            continue;
        };
        let start = byte_offset(text, reference.start);
        let end = byte_offset(text, reference.end);
        if start < copied {
            continue;
        }
        out.push_str(&text[copied..start]);
        out.push_str(&format!("[{}]({to})", reference.label));
        copied = end;
    }
    out.push_str(&text[copied..]);
    out
}

/// The byte offset of UTF-16 offset `utf16` in `text`.
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += character.len_utf16();
    }
    text.len()
}
