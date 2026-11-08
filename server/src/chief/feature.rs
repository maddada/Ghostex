//! Whether the Chief built-in extension is on (`chiefHidden` on Settings > Extensions, on by
//! default). Read fresh from the settings file, which the desktop's Settings writes directly, so
//! turning it off takes the head role away on the very next request (head.rs) without a restart.

use ghostex_settings_catalog::built_in_extensions::{self, CHIEF};
use serde_json::Value;

/// Whether Chief is on in `settings` (the defaults when there are none: on).
pub(crate) fn chief_on_in(settings: Option<&Value>) -> bool {
    built_in_extensions::on_with(CHIEF, |key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}

/// Whether Chief is on, read fresh from this gxserver's settings file.
pub(crate) fn chief_on(paths: &crate::paths::GxserverPaths) -> bool {
    chief_on_in(crate::shared_settings_file::read_shared_settings(paths).as_ref())
}

/// What a caller is told while Chief is off. Only the user turns it back on (the key is not
/// agent-writable), so the message names Settings, not a command.
pub(crate) fn chief_off_message() -> String {
    built_in_extensions::turned_off_message(CHIEF)
}

/// The routes that put a message or keystrokes into a session (`ghostex agents send` uses the
/// first two).
const MESSAGE_ROUTES: &[&str] = &[
    "/api/queueSessionChatPrompt",
    "/api/sendSessionChatMessage",
    "/api/sendSessionMessage",
    "/api/sendSessionText",
    "/api/sendSessionEnter",
];

/*
CDXC:Chief 2026-11-08 DECISION:
Coordinator, on the user's behalf: refuse `agents send` to Chief's session while the extension is off. A message would wake its sleeping agent (the send wakes a session) although the user turned Chief off; only the user's own devices may still reach it.
*/
/// Whether a call to `endpoint_path` puts something into session `target` that is Chief's while
/// Chief is off, from a caller other than the owner's devices.
pub(crate) fn refuses_message(
    caller_is_owner_device: bool,
    endpoint_path: &str,
    target: Option<(&str, &str)>,
    chief: Option<&crate::coordinators::SessionKey>,
    on: bool,
) -> bool {
    !on && !caller_is_owner_device
        && MESSAGE_ROUTES.contains(&endpoint_path)
        && matches!((target, chief), (Some((project, session)), Some((chief_project, chief_session)))
            if project == chief_project && session == chief_session)
}
