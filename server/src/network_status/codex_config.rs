//! The API endpoint a Codex session really talks to, read from its own `config.toml`.
//!
//! CDXC:OfflineHold 2026-11-17 WHY:
//! A Codex whose `CODEX_HOME` config sends it through a local proxy (`model_provider = "cliproxy"` with `[model_providers.cliproxy] base_url = "http://127.0.0.1:8317/v1"`) never talks to chatgpt.com, yet the online check and account recovery's quick tries probed chatgpt.com for it. The probe passed while the proxy refused every call, so each try was spent against it. The session's own provider `base_url` is the host that tells whether its agent can get through.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, PoisonError},
    time::SystemTime,
};

/// The base URL of the model provider `config.toml` in `home` selects, when it names one with a
/// `base_url`. Cached by the file's modification time.
pub(super) fn codex_base_url(home: &Path) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Option<SystemTime>, Option<String>)>>> =
        OnceLock::new();
    let path = home.join("config.toml");
    let modified = std::fs::metadata(&path)
        .ok()
        .and_then(|meta| meta.modified().ok());
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some((seen, url)) = cache.get(&path) {
        if *seen == modified {
            return url.clone();
        }
    }
    let url = modified
        .and_then(|_| std::fs::read_to_string(&path).ok())
        .and_then(|text| base_url_of(&text));
    cache.insert(path, (modified, url.clone()));
    url
}

fn base_url_of(text: &str) -> Option<String> {
    let config = text.parse::<toml_edit::DocumentMut>().ok()?;
    let provider = config.get("model_provider")?.as_str()?;
    config
        .get("model_providers")?
        .get(provider)?
        .get("base_url")?
        .as_str()
        .map(str::to_string)
}
