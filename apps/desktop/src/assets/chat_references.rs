use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
struct Artwork {
    icons: HashMap<String, String>,
}

static ARTWORK: LazyLock<Artwork> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../packages/gx-chat-core/visual/reference-visual.json"
    ))
    .expect("shared reference artwork")
});

pub(super) fn asset(key: &str) -> Option<&'static [u8]> {
    ARTWORK
        .icons
        .get(key.strip_suffix(".svg")?)
        .map(String::as_bytes)
}
