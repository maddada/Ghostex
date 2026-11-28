//! The desktop's `app/delayed_actions.rs` for a browser page: the page has no settings file, so the
//! handshake's `builtInFeatures` of the computer serving it says whether the Delayed Send and
//! Close After Done Staff picks are on.

pub(crate) fn delayed_send_on() -> bool {
    crate::app::read_aloud::computer_feature_on(
        ghostex_settings_catalog::built_in_extensions::DELAYED_SEND,
    )
}

pub(crate) fn close_after_done_on() -> bool {
    crate::app::read_aloud::computer_feature_on(
        ghostex_settings_catalog::built_in_extensions::CLOSE_AFTER_DONE,
    )
}
