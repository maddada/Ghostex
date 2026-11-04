//! Whether the Session notes built-in extension is on for this computer
//! (`sessionNotesHidden`, Settings > Extensions). The chat's More actions row, the terminal bar's
//! row and the note dialog ask here; the web build answers from its own twin
//! (apps/gpui-web/src/app/session_notes.rs), the computer's `builtInFeatures`.
//!
//! CDXC:SessionNotes 2026-10-30 SEE-ALSO: the user's decision on the `sessionNotes` entry in packages/settings-catalog/src/data/official_extensions.rs.

pub(crate) fn session_notes_on() -> bool {
    crate::app::titlebar::open_targets_and_window_controls::built_in_extension_enabled(
        ghostex_settings_catalog::built_in_extensions::SESSION_NOTES,
    )
}
