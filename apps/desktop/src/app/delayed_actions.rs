//! Whether the Delayed Send and Close After Done Staff picks are on for this computer
//! (`delayedSendEnabled`, `closeAfterDoneEnabled`, Settings > Extensions > Staff picks). The chat's
//! and the terminal bar's More actions, the Delayed Send dialog and the toggles ask here; the web
//! build answers from its own twin (apps/gpui-web/src/app/delayed_actions.rs), the computer's
//! `builtInFeatures`.
//!
//! CDXC:DelayedSend 2026-11-28 SEE-ALSO: the user's decision on the `delayedSend` entry in
//! packages/settings-catalog/src/data/official_extensions.rs. While one is off nothing offers it,
//! but a delayed send already scheduled still goes out and an armed Close After Done still closes
//! its session; the card's countdown and the way to cancel stay.

pub(crate) fn delayed_send_on() -> bool {
    crate::app::titlebar::open_targets_and_window_controls::built_in_extension_enabled(
        ghostex_settings_catalog::built_in_extensions::DELAYED_SEND,
    )
}

pub(crate) fn close_after_done_on() -> bool {
    crate::app::titlebar::open_targets_and_window_controls::built_in_extension_enabled(
        ghostex_settings_catalog::built_in_extensions::CLOSE_AFTER_DONE,
    )
}
