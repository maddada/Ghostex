//! The desktop's client-storage doors for the sidebar's client-owned documents (`gx_store/sidebar_ui_storage.rs`, the `preferences` table), over the page's localStorage under the same keys: in the browser era these keys were localStorage keys, and the page keeps its sidebar state there already (`web_commands.rs`). Same signatures, so the desktop's document files compile unchanged.
fn local_storage() -> Result<web_sys::Storage, &'static str> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .ok_or("localStorageUnavailable")
}

/// The stored value of `key`, or `None` when it is absent.
pub(crate) fn read_preference_value(key: &str) -> Result<Option<String>, &'static str> {
    local_storage()?
        .get_item(key)
        .map_err(|_| "localStorageReadFailed")
}

/// Stores `raw` under `key`, or removes the key for `None`. The desktop's size bounds belong to its database; a page's quota refusal is a failure here.
pub(crate) fn write_client_document_value(
    key: &str,
    raw: Option<&str>,
) -> Result<Option<&'static str>, &'static str> {
    let storage = local_storage()?;
    match raw {
        Some(raw) => storage
            .set_item(key, raw)
            .map_err(|_| "localStorageWriteFailed")?,
        None => storage
            .remove_item(key)
            .map_err(|_| "localStorageWriteFailed")?,
    }
    Ok(None)
}
