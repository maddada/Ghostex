/*
CDXC:Telemetry 2026-10-27:
Which extensions people have on, and which extension views they open.

User: "i wanna track please what extensions people have enabled so i know what
they're using in the app. Also which views being accessesd the most". This
supersedes the 2026-08-26 rule that no extension id ever leaves the machine, but
only for ids Ghostex itself publishes:

- Built-in extensions: the ids of `GHOSTEX_OFFICIAL_EXTENSIONS` in the settings
  catalog (`ghostex_settings_catalog::built_in_extensions`). The catalog is
  compiled in, so a built-in id is a `&'static str` that can only be a catalog
  entry.
- Official Store extensions: the ids listed in the Store's own catalog
  (`extensions::DEFAULT_EXTENSIONS_CATALOG_URL`), read from the last-good copy
  gxserver already caches beside the installed extensions. A new Store
  extension is therefore covered the day it is published, with no code change.
  A catalog URL overridden by `GHOSTEX_EXTENSIONS_CATALOG_URL` is not the
  official Store, so its ids are never treated as official.

Anything else (a local extension, a custom view, a Store id from another
catalog) is user-authored text and is reported only as `custom` (a view) or
counted (the heartbeat's `custom_extensions_enabled`), never by name.

The whitelist is enforced at the taxonomy chokepoint, not only here:
`PropertyValue::Extension` is re-checked against the catalog and the official
Store set when the event is validated, so a caller that builds a
`ExtensionRef::Store("my-secret-tool")` gets the whole event dropped.
*/

use std::{
    collections::BTreeSet,
    path::Path,
    sync::{OnceLock, RwLock},
};

use serde_json::Value;

use ghostex_settings_catalog::built_in_extensions;

/// What an unknown extension or view is reported as.
pub const CUSTOM_EXTENSION: &str = "custom";

/// Store ids are lowercase-and-hyphen (`extensions::validate_extension_id`); this
/// is the same charset, with a length cap so one odd entry cannot bloat an event.
const MAX_STORE_ID_LENGTH: usize = 64;

/// The most Store ids one heartbeat lists. The official Store is far smaller;
/// the cap only bounds a pathological cache file.
pub const MAX_STORE_EXTENSION_LIST_LENGTH: usize = 64;

const CATALOG_CACHE_FILE: &str = "catalog-cache.json";

/// The desktop names the built-in Claude view `claude-web`; the catalog entry
/// that switches it is `claudeWeb`.
const BUILT_IN_VIEW_ALIASES: &[(&str, &str)] = &[("claude-web", "claudeWeb")];

/// An extension as analytics may name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionRef {
    /// A built-in extension: always a settings-catalog id.
    BuiltIn(&'static str),
    /// An official Store extension id.
    Store(String),
    /// Anything else.
    Custom,
}

impl ExtensionRef {
    /// The value sent, after the whitelist is checked again. `None` means the
    /// reference does not pass, and the event carrying it is dropped.
    pub fn sendable(&self) -> Option<String> {
        match self {
            Self::BuiltIn(id) => built_in_extensions::switchable_id(id).map(str::to_string),
            Self::Store(id) => is_official_store_id(id).then(|| id.clone()),
            Self::Custom => Some(CUSTOM_EXTENSION.to_string()),
        }
    }

    /// The throttle key for a view-opened event: one window per reported id,
    /// with every custom view sharing one.
    pub fn throttle_key(&self) -> String {
        match self {
            Self::BuiltIn(id) => format!("extension:{id}"),
            Self::Store(id) => format!("extension:store:{id}"),
            Self::Custom => format!("extension:{CUSTOM_EXTENSION}"),
        }
    }
}

fn official_store_ids() -> &'static RwLock<BTreeSet<String>> {
    static IDS: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();
    IDS.get_or_init(|| RwLock::new(BTreeSet::new()))
}

fn valid_store_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_STORE_ID_LENGTH
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Whether `id` is listed in the official Store catalog this process has read.
pub fn is_official_store_id(id: &str) -> bool {
    let ids = match official_store_ids().read() {
        Ok(ids) => ids,
        Err(poisoned) => poisoned.into_inner(),
    };
    ids.contains(id)
}

/// Add ids to the official Store set. Only ever adds: an extension that leaves
/// the Store stays official until gxserver restarts, which costs nothing, and an
/// add-only set cannot race one reader against another's reload.
pub fn remember_official_store_ids<I, S>(ids: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut set = match official_store_ids().write() {
        Ok(set) => set,
        Err(poisoned) => poisoned.into_inner(),
    };
    for id in ids {
        let id = id.as_ref().trim();
        if valid_store_id(id) {
            set.insert(id.to_string());
        }
    }
}

/// The extension names listed in a Store catalog document.
pub fn catalog_extension_ids(catalog: &Value) -> Vec<String> {
    catalog
        .get("extensions")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Read the official Store's cached catalog from `extensions_dir` into the set.
/// A missing or unreadable cache adds nothing (every Store install writes it
/// first, so an install with Store extensions always has one).
pub fn load_official_store_ids(extensions_dir: &Path) {
    if crate::extensions::catalog_url() != crate::extensions::DEFAULT_EXTENSIONS_CATALOG_URL {
        return;
    }
    let Some(catalog) = std::fs::read_to_string(extensions_dir.join(CATALOG_CACHE_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return;
    };
    remember_official_store_ids(catalog_extension_ids(&catalog));
}

/// The built-in catalog id `id` names, through the desktop's view aliases.
fn built_in_id(id: &str) -> Option<&'static str> {
    let id = BUILT_IN_VIEW_ALIASES
        .iter()
        .find(|(alias, _)| *alias == id)
        .map(|(_, catalog_id)| *catalog_id)
        .unwrap_or(id);
    built_in_extensions::switchable_id(id)
}

/// What an extension view or extension modal reports: its built-in id, its
/// official Store id, or `custom`. Built-in ids win a clash with the Store.
pub fn resolve_extension(id: &str) -> ExtensionRef {
    let id = id.trim();
    if let Some(built_in) = built_in_id(id) {
        return ExtensionRef::BuiltIn(built_in);
    }
    if is_official_store_id(id) {
        return ExtensionRef::Store(id.to_string());
    }
    ExtensionRef::Custom
}

/// The heartbeat's view of the installed extensions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnabledExtensions {
    /// Enabled official Store extensions, sorted and deduplicated.
    pub store: Vec<String>,
    /// How many enabled extensions are not official Store extensions.
    pub custom_count: usize,
}

/// Split the installed extensions that are switched on into official Store ids
/// and a count of the rest. Disabled extensions are not counted at all.
pub fn enabled_extensions_report<'a>(
    installed: impl IntoIterator<Item = (&'a str, bool)>,
) -> EnabledExtensions {
    let mut store = BTreeSet::new();
    let mut custom_count = 0;
    for (id, enabled) in installed {
        if !enabled {
            continue;
        }
        if is_official_store_id(id.trim()) {
            store.insert(id.trim().to_string());
        } else {
            custom_count += 1;
        }
    }
    EnabledExtensions {
        store: store
            .into_iter()
            .take(MAX_STORE_EXTENSION_LIST_LENGTH)
            .collect(),
        custom_count,
    }
}

/// The built-in extensions that are on, read from the settings document (the
/// shipped defaults when it is missing).
pub fn built_in_extensions_on(settings: Option<&Value>) -> Vec<&'static str> {
    built_in_extensions::enabled_ids_with(|key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}
