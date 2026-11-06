use super::*;
use std::ffi::c_void;

/// Terminal → host hooks dispatched from [`VtTerminal::feed`]. `write_pty`
/// receives query auto-replies (DA1, DSR, DECRQM, ...) that must reach the
/// PTY for applications to keep working; `bell` and `title_changed` are
/// notification hooks (the new title is queried from the terminal later).
/// `clipboard_write` receives program-initiated standard-clipboard text
/// (OSC 52 / OSC 1337 Copy), already decoded; selection/primary
/// destinations, clears, and non-`text/plain` representations are reported
/// unsupported, matching the embedded-Ghostty surface path.
#[derive(Default)]
pub struct VtHostCallbacks {
    pub write_pty: Option<Box<dyn FnMut(&[u8]) + Send>>,
    pub bell: Option<Box<dyn FnMut() + Send>>,
    pub title_changed: Option<Box<dyn FnMut() + Send>>,
    pub clipboard_write: Option<Box<dyn FnMut(String) + Send>>,
}

pub(crate) unsafe extern "C" fn write_pty_trampoline(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    if let Some(write_pty) = callbacks.write_pty.as_mut() {
        let bytes: &[u8] = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, len) }
        };
        write_pty(bytes);
    }
}

pub(crate) unsafe extern "C" fn bell_trampoline(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
) {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    if let Some(bell) = callbacks.bell.as_mut() {
        bell();
    }
}

pub(crate) unsafe extern "C" fn title_changed_trampoline(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
) {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    if let Some(title_changed) = callbacks.title_changed.as_mut() {
        title_changed();
    }
}

pub(crate) unsafe extern "C" fn clipboard_write_trampoline(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
    write: *const ffi::GhosttyClipboardWrite,
) {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    let result = if let Some(clipboard_write) = callbacks.clipboard_write.as_mut() {
        if let Some(text) = unsafe { clipboard_write_standard_text_plain(write) } {
            clipboard_write(text);
            ffi::GHOSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS
        } else {
            ffi::GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED
        }
    } else {
        ffi::GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED
    };
    unsafe { reply_to_clipboard_write(write, result) };
}

unsafe fn reply_to_clipboard_write(
    write: *const ffi::GhosttyClipboardWrite,
    result: ffi::GhosttyClipboardWriteResult,
) {
    let Some(write_ref) = (unsafe { write.as_ref() }) else {
        return;
    };
    let Some(reply_fn) = write_ref.reply else {
        return;
    };
    let reply = ffi::GhosttyClipboardWriteReply {
        size: std::mem::size_of::<ffi::GhosttyClipboardWriteReply>(),
        result,
        remember: false,
    };
    unsafe { reply_fn(write, &reply) };
}

/// Copy the non-empty `text/plain` representation out of a standard-clipboard
/// write. Selection/primary destinations, clears, and non-text
/// representations yield `None` (reported unsupported to the library).
unsafe fn clipboard_write_standard_text_plain(
    write: *const ffi::GhosttyClipboardWrite,
) -> Option<String> {
    let write = unsafe { write.as_ref() }?;
    if write.location != ffi::GHOSTTY_CLIPBOARD_LOCATION_STANDARD
        || write.contents.is_null()
        || write.contents_len == 0
    {
        return None;
    }
    let contents = unsafe { std::slice::from_raw_parts(write.contents, write.contents_len) };
    for content in contents {
        if content.mime.ptr.is_null() || content.data.ptr.is_null() || content.data.len == 0 {
            continue;
        }
        let mime = unsafe { std::slice::from_raw_parts(content.mime.ptr, content.mime.len) };
        if mime != b"text/plain" {
            continue;
        }
        let data = unsafe { std::slice::from_raw_parts(content.data.ptr, content.data.len) };
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        return Some(text.to_string());
    }
    None
}
