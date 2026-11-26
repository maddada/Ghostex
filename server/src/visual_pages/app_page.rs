//! A page Ghostex itself ships (the coordinator flow page, coordinators/setup.rs): prepared from
//! the HTML built into gxserver and stored like a published page with no session, so any client
//! opens it at `/visual/<id>`. The same text reuses the same page; a new Ghostex with new text
//! (or a page pruned away) publishes it again.

use std::{fs, path::Path};

use serde_json::json;

use super::{
    prepare::prepare_page,
    store::{read_record, write_page},
};

/// The page id for `html` under `key`, publishing it when this text has no page yet. The index of
/// what was published lives next to the pages (`app-pages.json`).
pub(crate) fn app_page(gxserver_state_dir: &Path, key: &str, html: &str) -> Result<String, String> {
    let index_path = super::store::visual_pages_dir(gxserver_state_dir).join("app-pages.json");
    let mut index = fs::read(&index_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| json!({}));
    let hash = format!("{:x}", fingerprint(html));
    if let Some(id) = index[key]["id"].as_str().filter(|_| index[key]["hash"] == hash.as_str()) {
        if read_record(gxserver_state_dir, id).is_some() {
            return Ok(id.to_string());
        }
    }
    let prepared = prepare_page(html, Path::new("/"))?;
    let title = prepared.title.clone().unwrap_or_else(|| "Live page".to_string());
    let id = write_page(gxserver_state_dir, &title, &prepared.html, None, None)
        .map_err(|error| format!("Could not save the page: {error}"))?;
    index[key] = json!({ "id": id, "hash": hash });
    fs::write(&index_path, index.to_string())
        .map_err(|error| format!("Could not save the page index: {error}"))?;
    Ok(id)
}

/// A stable 64-bit fingerprint of the text (FNV-1a): only tells one version of the page from another.
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
