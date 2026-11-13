//! Project icons by content ref: `ghostex sessions --json --mobile-summary --icon-refs` and
//! `iconRefs=1` on the `sessionsList` stream send `discoveredIconRef` in place of
//! `discoveredIconDataUrl`; `ghostex sessions --json --project-icons <ref,…>` answers the data URLs.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::ghostex_cli::args::{FlagValue, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/// The most refs one `--project-icons` request may name.
const PROJECT_ICONS_MAX: usize = 32;

/// The ref of a discovered icon: the first 16 hex characters of the SHA-256 of its data URL.
fn icon_ref(data_url: &str) -> String {
    let mut hex = hex::encode(Sha256::digest(data_url.as_bytes()));
    hex.truncate(16);
    hex
}

/// CDXC:StateSync 2026-11-13 WHY: the phone polls the mobile summary every 5 s over SSH, and the projects' discovered icons (base64 data URLs) were about 80% of its bytes (322 of 411 KB on a real computer), so a phone that asks gets each icon as a content ref and fetches the ones it has not cached with `--project-icons`. The ref is a hash of the data URL, so it holds across CLI runs and gxserver restarts and changes when the icon does. Without `--icon-refs` (or `iconRefs=1` on the stream) the payload is unchanged, so phones at 11.2.1 and later keep working, and a gxserver without this ignores the flag and sends the data URLs.
pub(crate) fn swap_icons_for_refs(summary: &mut Map<String, Value>) {
    let Some(Value::Array(projects)) = summary.get_mut("projects") else {
        return;
    };
    for project in projects {
        let Some(map) = project.as_object_mut() else {
            continue;
        };
        let Some(icon) = map.remove("discoveredIconDataUrl") else {
            continue;
        };
        if let Some(data_url) = icon.as_str().filter(|text| !text.is_empty()) {
            map.insert("discoveredIconRef".to_string(), json!(icon_ref(data_url)));
        }
    }
}

/// `ghostex sessions --json --project-icons <ref,ref,…>`: `{"ok":true,"icons":{ref: dataUrl}}` for
/// the asked refs that match a project's discovered icon in gxserver's presentation snapshot (where
/// the summary takes them from); unknown refs are left out.
pub(super) fn project_icons_command(flags: &Flags) -> CliResult<()> {
    let mut wanted: Vec<String> = Vec::new();
    if let Some(FlagValue::Text(list)) = flags.0.get("projectIcons") {
        for item in list
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if !wanted.iter().any(|known| known == item) {
                wanted.push(item.to_string());
            }
        }
    }
    if wanted.len() > PROJECT_ICONS_MAX {
        return Err(CliError::Other(format!(
            "--project-icons takes at most {PROJECT_ICONS_MAX} refs."
        )));
    }
    let mut icons = Map::new();
    if !wanted.is_empty() {
        let response = call_gxserver_rpc(
            "/api/readPresentationSnapshot",
            &json!({ "onlySessions": [] }),
            flags,
        )?;
        let projects = response
            .get("snapshot")
            .and_then(|snapshot| snapshot.get("projects"))
            .and_then(Value::as_array);
        for project in projects.into_iter().flatten() {
            let Some(data_url) = project
                .get("discoveredIconDataUrl")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            else {
                continue;
            };
            let reference = icon_ref(data_url);
            if wanted.contains(&reference) {
                icons.insert(reference, json!(data_url));
            }
        }
    }
    print_json(&json!({ "icons": icons, "ok": true }));
    Ok(())
}
