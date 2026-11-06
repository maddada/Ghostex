//! The phone's chat core as WebAssembly: a plain C ABI over [`JsonChatCore`], which the page in
//! `page/chat-core.html` drives inside the app's hidden WebView.
//!
//! Strings cross as UTF-8 in linear memory. The page writes an argument into a buffer from
//! [`gx_alloc`], passes pointer and length, and frees it with [`gx_free`]. A call that answers text
//! returns a pointer to this module's result buffer; [`gx_result_len`] gives its length, and it stays
//! valid until the next call. Numbers cross as `f64`, so revisions and request ids reach JavaScript
//! exactly (they stay far below 2^53).
//!
//! Cores live in a slot table addressed by handle. A disposed or unknown handle answers the same
//! `disposed` error the native module throws, and `-1` for number answers.

use std::cell::RefCell;

use ghostex_gx_chat_json::JsonChatCore;

thread_local! {
    static CORES: RefCell<Vec<Option<JsonChatCore>>> = const { RefCell::new(Vec::new()) };
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

const DISPOSED: &str = r#"{"error":{"code":"disposed","message":"This chat core was disposed."}}"#;
const BAD_TEXT: &str = r#"{"error":{"code":"badArguments","message":"An argument is not UTF-8."}}"#;

fn with_core<T>(handle: u32, missing: T, call: impl FnOnce(&JsonChatCore) -> T) -> T {
    CORES.with(
        |cores| match cores.borrow().get(handle as usize).and_then(Option::as_ref) {
            Some(core) => call(core),
            None => missing,
        },
    )
}

fn finish(text: String) -> *const u8 {
    RESULT.with(|result| {
        let mut result = result.borrow_mut();
        *result = text.into_bytes();
        result.as_ptr()
    })
}

/// # Safety
/// `ptr..ptr + len` must be memory the page wrote through [`gx_alloc`].
unsafe fn text<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).ok()
}

#[no_mangle]
pub extern "C" fn gx_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// # Safety
/// `ptr` and `len` must be exactly what [`gx_alloc`] returned and was asked for.
#[no_mangle]
pub unsafe extern "C" fn gx_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn gx_result_len() -> usize {
    RESULT.with(|result| result.borrow().len())
}

/// Seeds the random stream contexts draw from when the host leaves `randomUnits`/`randomIds` out.
/// The page calls it once, with 64 bits from `crypto.getRandomValues`, before the first core.
#[no_mangle]
pub extern "C" fn gx_seed(low: u32, high: u32) {
    #[cfg(target_arch = "wasm32")]
    ghostex_gx_chat_json::seed_entropy((u64::from(high) << 32) | u64::from(low));
    // A native build of this crate (cargo check) draws from the OS source instead.
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (low, high);
}

#[no_mangle]
pub extern "C" fn gx_version() -> *const u8 {
    finish(concat!("gx-chat-wasm ", env!("CARGO_PKG_VERSION")).to_string())
}

/// A fresh core; reuses the first free slot.
#[no_mangle]
pub extern "C" fn gx_core_new() -> u32 {
    CORES.with(|cores| {
        let mut cores = cores.borrow_mut();
        match cores.iter().position(Option::is_none) {
            Some(free) => {
                cores[free] = Some(JsonChatCore::new());
                free as u32
            }
            None => {
                cores.push(Some(JsonChatCore::new()));
                (cores.len() - 1) as u32
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn gx_core_dispose(handle: u32) {
    CORES.with(|cores| {
        if let Some(slot) = cores.borrow_mut().get_mut(handle as usize) {
            *slot = None;
        }
    });
}

/// # Safety
/// Both ranges must come from [`gx_alloc`].
#[no_mangle]
pub unsafe extern "C" fn gx_core_handle(
    handle: u32,
    event_ptr: *const u8,
    event_len: usize,
    context_ptr: *const u8,
    context_len: usize,
) -> *const u8 {
    let (Some(event), Some(context)) = (text(event_ptr, event_len), text(context_ptr, context_len))
    else {
        return finish(BAD_TEXT.to_string());
    };
    finish(with_core(handle, DISPOSED.to_string(), |core| {
        core.handle(event, context)
    }))
}

/// A negative `last_revision` means the host holds no document, so the snapshot ships.
#[no_mangle]
pub extern "C" fn gx_core_frame_at(handle: u32, last_revision: f64, now_ms: f64) -> *const u8 {
    finish(with_core(handle, DISPOSED.to_string(), |core| {
        core.frame_at(last_revision as i64, now_ms)
    }))
}

/// # Safety
/// Both ranges must come from [`gx_alloc`].
#[no_mangle]
pub unsafe extern "C" fn gx_core_query(
    handle: u32,
    name_ptr: *const u8,
    name_len: usize,
    arguments_ptr: *const u8,
    arguments_len: usize,
) -> *const u8 {
    let (Some(name), Some(arguments)) =
        (text(name_ptr, name_len), text(arguments_ptr, arguments_len))
    else {
        return finish(BAD_TEXT.to_string());
    };
    finish(with_core(handle, DISPOSED.to_string(), |core| {
        core.query(name, arguments)
    }))
}

/// Milliseconds until the earliest armed deadline; `-1` for none, a panicked core or a bad handle.
#[no_mangle]
pub extern "C" fn gx_core_next_wake_ms(handle: u32) -> f64 {
    with_core(handle, -1.0, |core| {
        core.next_wake_ms().map_or(-1.0, |ms| ms as f64)
    })
}

/// The publish revision; `-1` for a bad handle.
#[no_mangle]
pub extern "C" fn gx_core_revision(handle: u32) -> f64 {
    with_core(handle, -1.0, |core| core.revision() as f64)
}

#[no_mangle]
pub extern "C" fn gx_core_forget_sent(handle: u32) -> *const u8 {
    finish(with_core(
        handle,
        DISPOSED.to_string(),
        JsonChatCore::forget_sent,
    ))
}

/// A fresh request id; `-1` for a bad handle.
#[no_mangle]
pub extern "C" fn gx_core_allocate_request_id(handle: u32) -> f64 {
    with_core(handle, -1.0, |core| core.allocate_request_id() as f64)
}

/// # Safety
/// All three ranges must come from [`gx_alloc`].
#[no_mangle]
pub unsafe extern "C" fn gx_retained_snapshot_key(
    machine_ptr: *const u8,
    machine_len: usize,
    project_ptr: *const u8,
    project_len: usize,
    session_ptr: *const u8,
    session_len: usize,
) -> *const u8 {
    let (Some(machine), Some(project), Some(session)) = (
        text(machine_ptr, machine_len),
        text(project_ptr, project_len),
        text(session_ptr, session_len),
    ) else {
        return finish(BAD_TEXT.to_string());
    };
    finish(ghostex_gx_chat_json::retained_snapshot_key(
        machine, project, session,
    ))
}

/// Every account's panel usage columns, without allocating or looking up a chat core.
///
/// # Safety
/// The answer range must come from [`gx_alloc`].
#[no_mangle]
pub unsafe extern "C" fn gx_account_usage_columns(
    now_ms: f64,
    answer_ptr: *const u8,
    answer_len: usize,
) -> *const u8 {
    let Some(answer) = text(answer_ptr, answer_len) else {
        return finish(BAD_TEXT.to_string());
    };
    finish(ghostex_gx_chat_json::account_usage_columns(answer, now_ms))
}

/// One Cloud Boxes provider call (`ghostex_gx_cloud::json_api::call`): a request built or an answer
/// read, for the phone's computer-off path (apps/mobile/app/src/cloudBoxes/providerOps.ts).
///
/// # Safety
/// Both ranges must come from [`gx_alloc`].
#[no_mangle]
pub unsafe extern "C" fn gx_cloud_call(
    name_ptr: *const u8,
    name_len: usize,
    arguments_ptr: *const u8,
    arguments_len: usize,
) -> *const u8 {
    let (Some(name), Some(arguments)) =
        (text(name_ptr, name_len), text(arguments_ptr, arguments_len))
    else {
        return finish(BAD_TEXT.to_string());
    };
    finish(ghostex_gx_cloud::json_api::call(name, arguments))
}
