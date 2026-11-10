//! The shared sidebar settings as the drawing code reads them. The desktop's module owns the settings FILE (watching, migrating, writing); the browser only holds the object gxserver hands it (`/api/readSharedSettings`, `app/gx_store/shared_settings_sync.rs`), behind the same two names the shared files call.
mod appearance;
pub use appearance::*;
mod terminal_layout;
pub use terminal_layout::*;

use std::cell::RefCell;
use std::sync::Arc;

use serde_json::{Map, Value};

#[derive(Clone, Debug, Default)]
pub struct SharedSidebarSettingsSnapshot {
    revision: u64,
    object: Arc<Map<String, Value>>,
}

impl SharedSidebarSettingsSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn content_hash(&self) -> u64 {
        self.revision
    }

    pub fn object(&self) -> &Map<String, Value> {
        &self.object
    }

    pub fn debugging_mode(&self) -> bool {
        self.object.get("debuggingMode").and_then(Value::as_bool) == Some(true)
    }

    /// The desktop snapshot's reader of the terminal's padding and Terminal width.
    pub fn terminal_pane_layout(
        &self,
        apply_width_mode: bool,
    ) -> (f32, f32, Option<TerminalContentWidth>) {
        terminal_pane_layout_from_settings(&self.object, apply_width_mode)
    }
}

#[derive(Default)]
struct PageSettings {
    snapshot: SharedSidebarSettingsSnapshot,
    /// What gxserver last answered.
    saved: Map<String, Value>,
    /// What this page set on its own (`?settings=`, a zoom chord, a folded links group): the page
    /// cannot save Settings, so these stay on top of every later read until the page reloads.
    page: Map<String, Value>,
}

thread_local! {
    static CURRENT: RefCell<PageSettings> = RefCell::default();
}

pub fn shared_sidebar_settings_snapshot() -> SharedSidebarSettingsSnapshot {
    CURRENT.with(|current| current.borrow().snapshot.clone())
}

fn publish(current: &mut PageSettings) {
    let mut object = current.saved.clone();
    object.extend(current.page.clone());
    current.snapshot.revision += 1;
    current.snapshot.object = Arc::new(object);
}

/// Installs a settings object this page composed (the current snapshot with a key changed): the
/// keys that differ from gxserver's copy are kept as the page's own.
pub fn install(object: Map<String, Value>) {
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        for (key, value) in object {
            if current.saved.get(&key) != Some(&value) {
                current.page.insert(key, value);
            }
        }
        publish(&mut current);
    });
}

/// Installs the settings gxserver read from this computer's settings file. `false` when they are
/// what the page already had.
pub fn install_saved(saved: Map<String, Value>) -> bool {
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        if current.saved == saved && current.snapshot.revision > 0 {
            return false;
        }
        current.saved = saved;
        publish(&mut current);
        true
    })
}

/// The desktop's storage folders. A page has no file system; the one reader (an extension modal's manifest size) finds nothing there and uses its default.
pub struct GhostexStoragePaths;

impl GhostexStoragePaths {
    pub fn extensions_dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::from("/ghostex-web-state/extensions")
    }
}

pub fn ghostex_storage_paths() -> GhostexStoragePaths {
    GhostexStoragePaths
}
