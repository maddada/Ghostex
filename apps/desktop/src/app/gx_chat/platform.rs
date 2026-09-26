//! The clock, the timezone and the random source, which the chat host reads and the core never does.
//!
//! One of three files `apps/gpui-web/src/app/gx_chat/` replaces with a browser twin (with
//! `worker.rs` and `storage_backend.rs`); every other file of this folder is compiled by both apps.

/// Milliseconds since the Unix epoch.
pub(super) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0)
}

/// Minutes east of UTC on this computer right now.
pub(super) fn utc_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

/// Sixteen bytes from the OS random source a v4 UUID uses.
pub(super) fn random_bytes() -> [u8; 16] {
    uuid::Uuid::new_v4().into_bytes()
}

/// A fresh v4 UUID in its canonical spelling, which is what `crypto.randomUUID()` gives.
pub(super) fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}
