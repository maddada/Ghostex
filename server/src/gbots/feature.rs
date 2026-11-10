//! Whether GBots are on: the GBots switch (`gbotsEnabled`, Settings > Extensions, off by default)
//! and Main Hub (`mainHubHidden`, the Chief of Staff extension) both on. Read fresh from the
//! settings file for every `/api/gbots` call, so turning either off refuses the next command
//! without a restart; the sweep (sweep.rs) keeps the cached answer presentations read.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User (P1): "the GBots switch in Settings > Extensions turns them on or off and needs Main Hub." GBots live under Chief of Staff in Main Hub, so with Main Hub off they are off whatever their own switch says, and every `ghostex gbots` verb says which switch to turn on.

use std::sync::atomic::{AtomicU8, Ordering};

use ghostex_settings_catalog::built_in_extensions::{self, GBOTS};
use serde_json::Value;

use crate::paths::GxserverPaths;

/// Whether the GBots switch itself is on in `settings` (Main Hub aside).
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
    crate::chief::chief_on(paths) && switch_on_in(settings.as_ref())
}

/// What a caller is told while GBots are off. Only the user turns them on (neither key is
/// agent-writable), so it names Settings, not a command.
pub(crate) fn gbots_off_message(paths: &GxserverPaths) -> String {
    if crate::chief::chief_on(paths) {
        built_in_extensions::turned_off_message(GBOTS)
    } else {
        "GBots need Main Hub. Turn on Main Hub, then GBots, in Settings > Extensions.".to_string()
    }
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
