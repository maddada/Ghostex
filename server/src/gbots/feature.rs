//! Whether GBots are on: the GBots switch (`gbotsEnabled`, Settings > Extensions, off by default).
//! Read fresh from the settings file for every `/api/gbots` call, so turning it off refuses the
//! next command without a restart; the sweep (sweep.rs) keeps the cached answer presentations read.

use std::sync::atomic::{AtomicU8, Ordering};

use ghostex_settings_catalog::built_in_extensions::{self, GBOTS};
use serde_json::Value;

use crate::paths::GxserverPaths;

/// Whether the GBots switch itself is on in `settings`.
fn switch_on_in(settings: Option<&Value>) -> bool {
    built_in_extensions::on_with(GBOTS, |key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}

/// Whether GBots are on, read fresh from this gxserver's settings file.
pub(crate) fn gbots_on(paths: &GxserverPaths) -> bool {
    let settings = crate::shared_settings_file::read_shared_settings(paths);
    switch_on_in(settings.as_ref())
}

/// What a caller is told while GBots are off: it names Settings, and this computer, since the
/// caller may be another computer's app (CDXC:Extensions 2026-11-21 in the settings catalog's
/// built_in_extensions.rs).
pub(crate) fn gbots_off_message(_paths: &GxserverPaths) -> String {
    let computer = crate::connected_computers::this_computer_name();
    built_in_extensions::turned_off_on_message(GBOTS, &computer)
}

/// GBots as gxserver last published them: 0 not read yet, 1 off, 2 on (sweep.rs keeps it).
static SHOWN: AtomicU8 = AtomicU8::new(0);

/// Whether clients get the GBots fields (`gbotsBot`, `gbotsChat`); read fresh only before the
/// sweep has, as in the `ghostex` CLI's own process.
pub(crate) fn gbots_shown() -> bool {
    match SHOWN.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => gbots_on(&crate::paths::get_gxserver_paths(None)),
    }
}

pub(super) fn remember_gbots_shown(on: bool) {
    SHOWN.store(if on { 2 } else { 1 }, Ordering::Relaxed);
}
