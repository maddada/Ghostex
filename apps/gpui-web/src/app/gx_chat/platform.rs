//! The desktop's `gx_chat/platform.rs` for a browser page: the page's clock and `crypto`.
//!
//! `std::time::SystemTime::now()` panics on wasm32, and the OS random source a native v4 UUID reads
//! is the page's `crypto` here.

/// Milliseconds since the Unix epoch.
pub(super) fn now_millis() -> i64 {
    js_sys::Date::now() as i64
}

/// Minutes east of UTC in this browser right now. `getTimezoneOffset` counts the other way.
pub(super) fn utc_offset_minutes() -> i32 {
    -(js_sys::Date::new_0().get_timezone_offset() as i32)
}

/// Sixteen bytes from `crypto.getRandomValues`.
pub(super) fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if let Some(crypto) = web_sys::window().and_then(|window| window.crypto().ok()) {
        let _ = crypto.get_random_values_with_u8_array(&mut bytes);
    }
    bytes
}

/// `crypto.randomUUID()`.
pub(super) fn uuid_v4() -> String {
    crate::app::helpers::gpui_random_uuid_string().unwrap_or_default()
}
