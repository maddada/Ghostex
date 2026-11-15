//! The lines installed extensions add to the note ordinary agent sessions get when they start or
//! resume (`agentNote` in a manifest, under the `agents.note` permission).
//!
//! CDXC:Extensions 2026-11-16 SEE-ALSO: types.rs (`ExtensionPermission::AgentsNote`, the user's
//! decision), agents/ghostex_note.rs (`current_launch_note_text` joins these in).

use crate::paths::GxserverPaths;

use super::{read_manifest, read_store, ExtensionPermission};

/// The `agentNote` of every installed extension that is on and was granted `agents.note`, in
/// extension id order. Read fresh from the store at each launch, so turning the extension off,
/// removing the permission or uninstalling it leaves the next start or resume without its line.
pub(crate) fn installed_agent_notes(paths: &GxserverPaths) -> Vec<String> {
    let Ok(store) = read_store(&paths.extensions_store_file) else {
        return Vec::new();
    };
    let installed = paths.extensions_dir.join("installed");
    store
        .iter()
        .filter(|(_, entry)| {
            entry.enabled
                && entry
                    .granted_permissions
                    .contains(&ExtensionPermission::AgentsNote)
        })
        .filter_map(|(id, _)| {
            let manifest = read_manifest(&installed.join(id), Some(id)).ok()?;
            let note = manifest.agent_note?;
            manifest
                .permissions
                .contains(&ExtensionPermission::AgentsNote)
                .then(|| note.trim().to_string())
        })
        .filter(|note| !note.is_empty())
        .collect()
}
