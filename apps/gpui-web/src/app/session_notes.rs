//! The desktop's `app/session_notes.rs` for a browser page: the page has no settings file, so the
//! handshake's `builtInFeatures` of the computer serving it says whether Session notes is on.

pub(crate) fn session_notes_on() -> bool {
    crate::app::read_aloud::computer_feature_on(
        ghostex_settings_catalog::built_in_extensions::SESSION_NOTES,
    )
}
