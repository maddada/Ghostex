//! Whether the Delayed Send and Close After Done Staff picks are on for this computer
//! (`delayedSendEnabled`, `closeAfterDoneEnabled`, Settings > Extensions > Staff picks, both off by
//! default). Read fresh from the settings file for every call, so turning one off refuses the next
//! request without a restart.
//!
//! CDXC:DelayedSend 2026-11-28 DECISION:
//! User: "make delayed actions (delayed send and close after done) both staff pick extensions
//! disabled by default for new users pls. disable it even for existing users. when disabled no
//! settings about it and not shown in any menus". While one is off, gxserver refuses a new delayed
//! send (`/api/scheduleDelayedSend`) and arming Close After Done (`/api/toggleCloseAfterDone` and
//! the `toggleCloseAfterDone` renderer command `ghostex close-after-done` sends), with a message
//! that names the Staff pick to turn on, so an agent or a script learns why. Cancelling,
//! postponing and disarming stay allowed, and a delayed send already scheduled or a session
//! already armed still runs: the user asked for it before the switch went off.
//! SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs (the two entries).

use ghostex_settings_catalog::built_in_extensions::{self, CLOSE_AFTER_DONE, DELAYED_SEND};

use crate::paths::GxserverPaths;

fn on(paths: &GxserverPaths, id: &str) -> bool {
    let settings = crate::shared_settings_file::read_shared_settings(paths);
    built_in_extensions::enabled_in_value(settings.as_ref(), id)
}

/// Whether Delayed Send is on, read fresh from this gxserver's settings file.
pub(crate) fn delayed_send_on(paths: &GxserverPaths) -> bool {
    on(paths, DELAYED_SEND)
}

/// Whether Close After Done is on, read fresh from this gxserver's settings file.
pub(crate) fn close_after_done_on(paths: &GxserverPaths) -> bool {
    on(paths, CLOSE_AFTER_DONE)
}

/// What a caller is told while `id` is off: the Staff pick to turn on, where, and the command
/// that turns it on.
fn off_message(id: &str) -> String {
    let title = built_in_extensions::title(id);
    let computer = crate::connected_computers::this_computer_name();
    let mut message = format!(
        "{title} is turned off on {computer}. Turn on the {title} Staff pick in Settings > Extensions > Staff picks."
    );
    if let Some((key, enables)) = built_in_extensions::switch_key(id) {
        message.push_str(&built_in_extensions::terminal_hint(key, enables));
    }
    message
}

/// The refusal of a new delayed send while Delayed Send is off.
pub(crate) fn delayed_send_off_message() -> String {
    off_message(DELAYED_SEND)
}

/// The refusal of arming Close After Done while it is off.
pub(crate) fn close_after_done_off_message() -> String {
    off_message(CLOSE_AFTER_DONE)
}
