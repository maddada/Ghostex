//! Sidebar and side panel extension panels: gxserver owns one panel document per extension and
//! surface (its sidebar, its side panel, or one side panel per session), checks it against the
//! panel schema, keeps its revision, saves it, and applies editable-checklist edits itself.
//!
//! Saved as `<extensions>/panels/<id>/<surface>.json` (`{revision, updatedAt, document}`). Until
//! the first write a surface shows the document the manifest ships (`sidebar.panel`,
//! `sidePanel.panel`) at revision 0. Every change is told to the extension on its event stream
//! (`/api/ext/v1/events`, ext_api.rs) and to clients in the `extensionPanels` snapshot field
//! (presentation.rs).
//!
//! SEE-ALSO: packages/gx-protocol/src/extension_panel.rs (the document types),
//! schemas/ghostex-sidebar-panel.schema.json in the Ghostex-extensions repo (the same checks for
//! authors, `tooling/validate.mjs`).

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ghostex_gx_protocol::extension_panel::{
    ChecklistItem, PanelAction, PanelBlockBody, PanelDocument, PanelMenuItem, PanelRow,
    EXTENSION_PANEL_SCHEMA,
};
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

use super::{
    validate_relative_path, ExtensionError, ExtensionKind, ExtensionManifest, ExtensionPermission,
    ExtensionResult,
};

/// The largest panel document, as JSON.
pub(crate) const PANEL_DOCUMENT_MAX_BYTES: usize = 64 * 1024;
const BLOCKS_MAX: usize = 64;
const TABS_MAX: usize = 8;
const ITEMS_MAX: usize = 200;
const MENU_MAX: usize = 12;
const LABEL_MAX_CHARS: usize = 200;
const TEXT_MAX_CHARS: usize = 4000;
const ACTION_DATA_MAX_BYTES: usize = 4 * 1024;
const EVENT_QUEUE: usize = 256;
const DOTS: &[&str] = &["green", "yellow", "red", "blue", "gray"];
const BUTTON_STYLES: &[&str] = &["primary", "default"];

/// Which document of an extension.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PanelSurface {
    Sidebar,
    SidePanel,
    /// `sidePanel.scope: "session"`: one document per session.
    SessionSidePanel {
        project_id: String,
        session_id: String,
    },
}

impl PanelSurface {
    /// `surface` (`sidebar` | `sidePanel`, `sidebar` when absent) and `session`
    /// (`<projectId>:<sessionId>`, only for a per-session side panel).
    pub(crate) fn parse(surface: Option<&str>, session: Option<&str>) -> ExtensionResult<Self> {
        match (surface.unwrap_or("sidebar"), session) {
            ("sidebar", None) => Ok(Self::Sidebar),
            ("sidebar", Some(_)) => Err(ExtensionError::bad_request(
                "A sidebar panel has no session; session goes with surface=sidePanel.",
            )),
            ("sidePanel", None) => Ok(Self::SidePanel),
            ("sidePanel", Some(session)) => {
                let (project_id, session_id) = session.split_once(':').ok_or_else(|| {
                    ExtensionError::bad_request("session must be <projectId>:<sessionId>.")
                })?;
                if !valid_ref_part(project_id) || !valid_ref_part(session_id) {
                    return Err(ExtensionError::bad_request(
                        "session must be <projectId>:<sessionId>.",
                    ));
                }
                Ok(Self::SessionSidePanel {
                    project_id: project_id.to_string(),
                    session_id: session_id.to_string(),
                })
            }
            (other, _) => Err(ExtensionError::bad_request(format!(
                "Unknown panel surface {other:?}; use sidebar or sidePanel."
            ))),
        }
    }

    fn file_name(&self) -> String {
        match self {
            Self::Sidebar => "sidebar.json".to_string(),
            Self::SidePanel => "sidePanel.json".to_string(),
            Self::SessionSidePanel {
                project_id,
                session_id,
            } => format!("sidePanel.{project_id}.{session_id}.json"),
        }
    }

    /// The surface and session keys events and API answers carry.
    pub(crate) fn describe(&self, into: &mut Map<String, Value>) {
        match self {
            Self::Sidebar => {
                into.insert("surface".into(), json!("sidebar"));
            }
            Self::SidePanel => {
                into.insert("surface".into(), json!("sidePanel"));
            }
            Self::SessionSidePanel {
                project_id,
                session_id,
            } => {
                into.insert("surface".into(), json!("sidePanel"));
                into.insert(
                    "session".into(),
                    json!(format!("{project_id}:{session_id}")),
                );
            }
        }
    }

    /// The manifest file this surface starts from, or a refusal when the manifest has no such
    /// surface.
    fn template_path<'a>(
        &self,
        manifest: &'a ExtensionManifest,
    ) -> ExtensionResult<Option<&'a str>> {
        use ghostex_gx_protocol::extension_panel::SidePanelScope;
        if manifest.kind != Some(ExtensionKind::Sidebar) {
            return Err(ExtensionError::bad_request(format!(
                "Extension {:?} is not a sidebar extension.",
                manifest.name
            )));
        }
        match self {
            Self::Sidebar => manifest
                .sidebar
                .as_ref()
                .map(|sidebar| sidebar.panel.as_deref())
                .ok_or_else(|| {
                    ExtensionError::bad_request(format!(
                        "Extension {:?} has no sidebar panel; its manifest declares only sidePanel.",
                        manifest.name
                    ))
                }),
            Self::SidePanel | Self::SessionSidePanel { .. } => {
                let side_panel = manifest.side_panel.as_ref().ok_or_else(|| {
                    ExtensionError::bad_request(format!(
                        "Extension {:?} has no side panel.",
                        manifest.name
                    ))
                })?;
                let per_session = side_panel.scope == Some(SidePanelScope::Session);
                match (self, per_session) {
                    (Self::SidePanel, true) => Err(ExtensionError::bad_request(
                        "This side panel has one document per session: pass session=<projectId>:<sessionId>.",
                    )),
                    (Self::SessionSidePanel { .. }, false) => Err(ExtensionError::bad_request(
                        "This side panel has one document for the computer: leave session out.",
                    )),
                    _ => Ok(side_panel.panel.as_deref()),
                }
            }
        }
    }
}

fn valid_ref_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// One saved (or template) document.
#[derive(Clone, Debug)]
pub(crate) struct PanelRecord {
    pub(crate) revision: u64,
    pub(crate) updated_at: Option<String>,
    pub(crate) document: PanelDocument,
}

impl PanelRecord {
    pub(crate) fn to_json(&self, extension_id: &str, surface: &PanelSurface) -> Value {
        let mut answer = Map::new();
        answer.insert("extension".into(), json!(extension_id));
        surface.describe(&mut answer);
        answer.insert("revision".into(), json!(self.revision));
        if let Some(updated_at) = &self.updated_at {
            answer.insert("updatedAt".into(), json!(updated_at));
        }
        answer.insert(
            "document".into(),
            serde_json::to_value(&self.document).unwrap_or(Value::Null),
        );
        Value::Object(answer)
    }
}

/// Something an extension hears on `/api/ext/v1/events` about its own panels.
#[derive(Clone, Debug)]
pub(crate) struct PanelEvent {
    pub(crate) extension_id: String,
    pub(crate) payload: Value,
}

/// Who changed a panel, for the `panelChanged` event.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PanelWriter {
    Extension,
    User,
}

#[derive(Clone)]
pub(crate) struct PanelStore {
    dir: PathBuf,
    inner: Arc<Mutex<HashMap<(String, PanelSurface), PanelRecord>>>,
    events: broadcast::Sender<PanelEvent>,
}

impl PanelStore {
    pub(crate) fn new(extensions_dir: &Path) -> Self {
        let (events, _) = broadcast::channel(EVENT_QUEUE);
        Self {
            dir: extensions_dir.join("panels"),
            inner: Arc::new(Mutex::new(HashMap::new())),
            events,
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<PanelEvent> {
        self.events.subscribe()
    }

    pub(crate) fn emit(&self, extension_id: &str, payload: Value) {
        let _ = self.events.send(PanelEvent {
            extension_id: extension_id.to_string(),
            payload,
        });
    }

    /// The saved document, or the manifest's at revision 0, or an empty one.
    pub(crate) fn read(
        &self,
        manifest: &ExtensionManifest,
        payload_dir: &Path,
        surface: &PanelSurface,
    ) -> ExtensionResult<PanelRecord> {
        surface.template_path(manifest)?;
        let key = (manifest.name.clone(), surface.clone());
        if let Some(record) = self.lock()?.get(&key) {
            return Ok(record.clone());
        }
        let record = match self.read_saved(&manifest.name, surface)? {
            Some(record) => record,
            None => self.template(manifest, payload_dir, surface)?,
        };
        self.lock()?.insert(key, record.clone());
        Ok(record)
    }

    /// The document the manifest ships for this surface (a per-session side panel's sessions all
    /// start from it), at revision 0; an empty panel when it ships none.
    pub(crate) fn template(
        &self,
        manifest: &ExtensionManifest,
        payload_dir: &Path,
        surface: &PanelSurface,
    ) -> ExtensionResult<PanelRecord> {
        Ok(PanelRecord {
            revision: 0,
            updated_at: None,
            document: match surface.template_path(manifest)? {
                Some(path) => read_panel_file(payload_dir, path, "panel", manifest)?,
                None => empty_document(),
            },
        })
    }

    /// Replaces a document. `base_revision`, when given, must be the current revision.
    pub(crate) fn write(
        &self,
        manifest: &ExtensionManifest,
        payload_dir: &Path,
        surface: &PanelSurface,
        document: &Value,
        base_revision: Option<u64>,
        granted: &[ExtensionPermission],
        writer: PanelWriter,
    ) -> ExtensionResult<PanelRecord> {
        let document =
            validate_panel_document(document, |permission| granted.contains(&permission))?;
        self.commit(
            manifest,
            payload_dir,
            surface,
            base_revision,
            writer,
            |_| Ok(document),
        )
    }

    /// Upserts blocks by id, removes blocks, and replaces `title` or `tabs`, then checks the whole
    /// document again.
    pub(crate) fn patch(
        &self,
        manifest: &ExtensionManifest,
        payload_dir: &Path,
        surface: &PanelSurface,
        patch: &Map<String, Value>,
        base_revision: Option<u64>,
        granted: &[ExtensionPermission],
    ) -> ExtensionResult<PanelRecord> {
        for key in patch.keys() {
            if !matches!(
                key.as_str(),
                "blocks" | "removeBlocks" | "title" | "tabs" | "baseRevision"
            ) {
                return Err(ExtensionError::bad_request(format!(
                    "Unknown panel patch key {key:?}; use blocks, removeBlocks, title, tabs or baseRevision."
                )));
            }
        }
        self.commit(
            manifest,
            payload_dir,
            surface,
            base_revision,
            PanelWriter::Extension,
            |current| {
                let mut document = serde_json::to_value(current).map_err(|error| {
                    ExtensionError::internal(format!("Could not encode panel: {error}"))
                })?;
                apply_patch(&mut document, patch)?;
                validate_panel_document(&document, |permission| granted.contains(&permission))
            },
        )
    }

    /// Applies a user's edit to an editable checklist (`check`, `add`, `rename`, `delete`).
    pub(crate) fn edit_checklist(
        &self,
        manifest: &ExtensionManifest,
        payload_dir: &Path,
        surface: &PanelSurface,
        block_id: &str,
        edit: &ChecklistEdit,
        base_revision: Option<u64>,
    ) -> ExtensionResult<PanelRecord> {
        self.commit(
            manifest,
            payload_dir,
            surface,
            base_revision,
            PanelWriter::User,
            |current| {
                let mut document = current.clone();
                let block = document
                    .blocks
                    .iter_mut()
                    .find(|block| block.id == block_id)
                    .ok_or_else(|| {
                        ExtensionError::not_found(format!("The panel has no block {block_id:?}."))
                    })?;
                let PanelBlockBody::Checklist { editable, items } = &mut block.body else {
                    return Err(ExtensionError::bad_request(format!(
                        "Block {block_id:?} is not a checklist."
                    )));
                };
                if *editable != Some(true) {
                    return Err(ExtensionError::bad_request(format!(
                        "Checklist {block_id:?} is not editable."
                    )));
                }
                edit.apply(items)?;
                // The result must still be a valid document (lengths, item count).
                let value = serde_json::to_value(&document).map_err(|error| {
                    ExtensionError::internal(format!("Could not encode panel: {error}"))
                })?;
                validate_panel_document(&value, |_| true)
            },
        )
    }

    /// Forgets every panel of an uninstalled extension.
    pub(crate) fn remove_extension(&self, extension_id: &str) -> ExtensionResult<()> {
        self.lock()?.retain(|(id, _), _| id != extension_id);
        let dir = self.dir.join(extension_id);
        match fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ExtensionError::internal(format!(
                "Could not remove the panels of extension {extension_id}: {error}"
            ))),
        }
    }

    /// The per-session side panel documents saved for an extension, by `<projectId>:<sessionId>`.
    pub(crate) fn session_side_panels(
        &self,
        manifest: &ExtensionManifest,
        payload_dir: &Path,
    ) -> Vec<(String, PanelRecord)> {
        let Ok(entries) = fs::read_dir(self.dir.join(&manifest.name)) else {
            return Vec::new();
        };
        let mut panels = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let middle = name.strip_prefix("sidePanel.")?.strip_suffix(".json")?;
                let (project_id, session_id) = middle.split_once('.')?;
                let surface = PanelSurface::SessionSidePanel {
                    project_id: project_id.to_string(),
                    session_id: session_id.to_string(),
                };
                let record = self.read(manifest, payload_dir, &surface).ok()?;
                Some((format!("{project_id}:{session_id}"), record))
            })
            .collect::<Vec<_>>();
        panels.sort_by(|left, right| left.0.cmp(&right.0));
        panels
    }

    fn commit(
        &self,
        manifest: &ExtensionManifest,
        payload_dir: &Path,
        surface: &PanelSurface,
        base_revision: Option<u64>,
        writer: PanelWriter,
        change: impl FnOnce(&PanelDocument) -> ExtensionResult<PanelDocument>,
    ) -> ExtensionResult<PanelRecord> {
        // Read (and cache) first, outside the lock `read` takes itself.
        let current = self.read(manifest, payload_dir, surface)?;
        let mut cache = self.lock()?;
        let key = (manifest.name.clone(), surface.clone());
        let current = cache.get(&key).cloned().unwrap_or(current);
        if let Some(base) = base_revision {
            if base != current.revision {
                return Err(ExtensionError {
                    code: "conflict",
                    message: format!(
                        "The panel changed since revision {base}; it is at revision {} now. Read it again and retry.",
                        current.revision
                    ),
                });
            }
        }
        let document = change(&current.document)?;
        let record = PanelRecord {
            revision: current.revision + 1,
            updated_at: Some(
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ),
            document,
        };
        self.save(&manifest.name, surface, &record)?;
        cache.insert(key, record.clone());
        drop(cache);
        let mut event = Map::new();
        event.insert("type".into(), json!("panelChanged"));
        surface.describe(&mut event);
        event.insert("revision".into(), json!(record.revision));
        event.insert(
            "by".into(),
            json!(match writer {
                PanelWriter::Extension => "extension",
                PanelWriter::User => "user",
            }),
        );
        self.emit(&manifest.name, Value::Object(event));
        Ok(record)
    }

    fn read_saved(
        &self,
        extension_id: &str,
        surface: &PanelSurface,
    ) -> ExtensionResult<Option<PanelRecord>> {
        let path = self.dir.join(extension_id).join(surface.file_name());
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ExtensionError::internal(format!(
                    "Could not read panel {}: {error}",
                    path.display()
                )))
            }
        };
        let saved: Value = serde_json::from_slice(&bytes).map_err(|error| {
            ExtensionError::internal(format!("Panel {} is damaged: {error}", path.display()))
        })?;
        let document = serde_json::from_value::<PanelDocument>(
            saved.get("document").cloned().unwrap_or(Value::Null),
        )
        .map_err(|error| {
            ExtensionError::internal(format!("Panel {} is damaged: {error}", path.display()))
        })?;
        Ok(Some(PanelRecord {
            revision: saved.get("revision").and_then(Value::as_u64).unwrap_or(0),
            updated_at: saved
                .get("updatedAt")
                .and_then(Value::as_str)
                .map(str::to_string),
            document,
        }))
    }

    fn save(
        &self,
        extension_id: &str,
        surface: &PanelSurface,
        record: &PanelRecord,
    ) -> ExtensionResult<()> {
        let dir = self.dir.join(extension_id);
        fs::create_dir_all(&dir).map_err(|error| {
            ExtensionError::internal(format!(
                "Could not create panel folder {}: {error}",
                dir.display()
            ))
        })?;
        let bytes = serde_json::to_vec_pretty(&json!({
            "revision": record.revision,
            "updatedAt": record.updated_at,
            "document": record.document,
        }))
        .map_err(|error| ExtensionError::internal(format!("Could not encode panel: {error}")))?;
        let path = dir.join(surface.file_name());
        let temp = dir.join(format!(
            ".{}.{}.tmp",
            surface.file_name(),
            uuid::Uuid::new_v4()
        ));
        let written = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temp, &path)
        })();
        written.map_err(|error| {
            let _ = fs::remove_file(&temp);
            ExtensionError::internal(format!("Could not save panel {}: {error}", path.display()))
        })
    }

    fn lock(
        &self,
    ) -> ExtensionResult<std::sync::MutexGuard<'_, HashMap<(String, PanelSurface), PanelRecord>>>
    {
        self.inner
            .lock()
            .map_err(|_| ExtensionError::internal("Extension panel store lock was poisoned."))
    }
}

/// A user's edit of an editable checklist.
#[derive(Clone, Debug)]
pub(crate) enum ChecklistEdit {
    Check {
        item_id: String,
        done: bool,
    },
    Add {
        text: String,
        item_id: Option<String>,
    },
    Rename {
        item_id: String,
        text: String,
    },
    Delete {
        item_id: String,
    },
}

impl ChecklistEdit {
    /// `{op: "check", itemId, done}`, `{op: "add", text, itemId?}`, `{op: "rename", itemId,
    /// text}`, `{op: "delete", itemId}`.
    pub(crate) fn parse(value: &Value) -> ExtensionResult<Self> {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .ok_or_else(|| ExtensionError::bad_request(format!("The edit needs {key}.")))
        };
        match value.get("op").and_then(Value::as_str) {
            Some("check") => Ok(Self::Check {
                item_id: text("itemId")?,
                done: value
                    .get("done")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| ExtensionError::bad_request("A check edit needs done."))?,
            }),
            Some("add") => Ok(Self::Add {
                text: text("text")?,
                item_id: text("itemId").ok(),
            }),
            Some("rename") => Ok(Self::Rename {
                item_id: text("itemId")?,
                text: text("text")?,
            }),
            Some("delete") => Ok(Self::Delete {
                item_id: text("itemId")?,
            }),
            _ => Err(ExtensionError::bad_request(
                "A checklist edit's op is check, add, rename or delete.",
            )),
        }
    }

    fn apply(&self, items: &mut Vec<ChecklistItem>) -> ExtensionResult<()> {
        let find = |items: &mut Vec<ChecklistItem>, item_id: &str| {
            items
                .iter()
                .position(|item| item.id == item_id)
                .ok_or_else(|| ExtensionError::not_found(format!("No checklist item {item_id:?}.")))
        };
        match self {
            Self::Check { item_id, done } => {
                let index = find(items, item_id)?;
                items[index].done = *done;
            }
            Self::Add { text, item_id } => {
                let id = item_id.clone().unwrap_or_else(|| {
                    format!("u{}", &uuid::Uuid::new_v4().simple().to_string()[..10])
                });
                if items.iter().any(|item| item.id == id) {
                    return Err(ExtensionError::bad_request(format!(
                        "The checklist already has an item {id:?}."
                    )));
                }
                items.push(ChecklistItem {
                    id,
                    text: text.clone(),
                    done: false,
                    action: None,
                    menu: None,
                });
            }
            Self::Rename { item_id, text } => {
                let index = find(items, item_id)?;
                items[index].text = text.clone();
            }
            Self::Delete { item_id } => {
                let index = find(items, item_id)?;
                items.remove(index);
            }
        }
        Ok(())
    }

    pub(crate) fn to_json(&self) -> Value {
        match self {
            Self::Check { item_id, done } => {
                json!({ "op": "check", "itemId": item_id, "done": done })
            }
            Self::Add { text, item_id } => json!({ "op": "add", "text": text, "itemId": item_id }),
            Self::Rename { item_id, text } => {
                json!({ "op": "rename", "itemId": item_id, "text": text })
            }
            Self::Delete { item_id } => json!({ "op": "delete", "itemId": item_id }),
        }
    }
}

fn empty_document() -> PanelDocument {
    PanelDocument {
        schema: EXTENSION_PANEL_SCHEMA,
        title: None,
        tabs: None,
        blocks: Vec::new(),
    }
}

fn apply_patch(document: &mut Value, patch: &Map<String, Value>) -> ExtensionResult<()> {
    let object = document
        .as_object_mut()
        .ok_or_else(|| ExtensionError::internal("The panel is not an object."))?;
    for key in ["title", "tabs"] {
        if let Some(value) = patch.get(key) {
            if value.is_null() {
                object.remove(key);
            } else {
                object.insert(key.to_string(), value.clone());
            }
        }
    }
    let blocks = object
        .entry("blocks")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| ExtensionError::internal("The panel's blocks are not a list."))?;
    if let Some(remove) = patch.get("removeBlocks") {
        let remove = remove
            .as_array()
            .ok_or_else(|| {
                ExtensionError::bad_request("removeBlocks must be a list of block ids.")
            })?
            .iter()
            .map(|id| {
                id.as_str().ok_or_else(|| {
                    ExtensionError::bad_request("removeBlocks must be a list of block ids.")
                })
            })
            .collect::<ExtensionResult<BTreeSet<_>>>()?;
        blocks.retain(|block| {
            !block
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| remove.contains(id))
        });
    }
    if let Some(upserts) = patch.get("blocks") {
        let upserts = upserts
            .as_array()
            .ok_or_else(|| ExtensionError::bad_request("blocks must be a list of blocks."))?;
        for block in upserts {
            let id = block
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| ExtensionError::bad_request("Every patched block needs its id."))?;
            match blocks
                .iter_mut()
                .find(|existing| existing.get("id").and_then(Value::as_str) == Some(id))
            {
                Some(existing) => *existing = block.clone(),
                None => blocks.push(block.clone()),
            }
        }
    }
    Ok(())
}

/// Reads and checks a panel file the payload ships. Its actions may use the permissions the
/// manifest declares (the user grants them at install).
pub(crate) fn read_panel_file(
    payload_dir: &Path,
    relative: &str,
    field: &str,
    manifest: &ExtensionManifest,
) -> ExtensionResult<PanelDocument> {
    validate_relative_path(relative, field)?;
    let path = payload_dir.join(relative);
    let bytes = fs::read(&path).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Extension {field} could not be read ({relative}): {error}"
        ))
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        ExtensionError::bad_request(format!(
            "Extension {field} ({relative}) is not JSON: {error}"
        ))
    })?;
    validate_panel_document(&value, |permission| {
        manifest.permissions.contains(&permission)
    })
    .map_err(|error| ExtensionError {
        code: error.code,
        message: format!("{relative}: {}", error.message),
    })
}

/// Checks a panel document against schema 1 and returns it. `allowed` says which permissions
/// its actions may need.
pub(crate) fn validate_panel_document(
    value: &Value,
    allowed: impl Fn(ExtensionPermission) -> bool,
) -> ExtensionResult<PanelDocument> {
    let size = serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if size > PANEL_DOCUMENT_MAX_BYTES {
        return Err(ExtensionError::bad_request(format!(
            "The panel is {size} bytes; the limit is {PANEL_DOCUMENT_MAX_BYTES}."
        )));
    }
    // A panel file may name its schema for editors (`"$schema": "…/ghostex-sidebar-panel.schema.json"`),
    // as the extensions repo's validator allows; it is not part of the document.
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("$schema");
    }
    let value = &value;
    let document: PanelDocument = serde_json::from_value(value.clone()).map_err(|error| {
        ExtensionError::bad_request(format!(
            "The panel does not match the panel schema: {error}"
        ))
    })?;
    if document.schema != EXTENSION_PANEL_SCHEMA {
        return Err(ExtensionError::bad_request(format!(
            "The panel's schema is {}; this Ghostex reads schema {EXTENSION_PANEL_SCHEMA}.",
            document.schema
        )));
    }
    // Typed parsing drops keys it does not know, so compare: a key that did not survive is one
    // the schema does not have (usually a typo).
    let normalized = serde_json::to_value(&document)
        .map_err(|error| ExtensionError::internal(format!("Could not encode panel: {error}")))?;
    if let Some(path) = first_unknown_key(value, &normalized, "") {
        return Err(ExtensionError::bad_request(format!(
            "The panel has a key the panel schema doesn't know: {path}"
        )));
    }
    let mut check = Checker { allowed: &allowed };
    check.document(&document)?;
    Ok(document)
}

fn first_unknown_key(input: &Value, normalized: &Value, path: &str) -> Option<String> {
    match (input, normalized) {
        (Value::Object(input), Value::Object(normalized)) => {
            for (key, value) in input {
                if value.is_null() {
                    continue;
                }
                let child = format!("{path}/{key}");
                match normalized.get(key) {
                    None => return Some(child),
                    Some(next) => {
                        if let Some(found) = first_unknown_key(value, next, &child) {
                            return Some(found);
                        }
                    }
                }
            }
            None
        }
        (Value::Array(input), Value::Array(normalized)) => input
            .iter()
            .zip(normalized)
            .enumerate()
            .find_map(|(index, (value, next))| {
                first_unknown_key(value, next, &format!("{path}/{index}"))
            }),
        _ => None,
    }
}

struct Checker<'a, F: Fn(ExtensionPermission) -> bool> {
    allowed: &'a F,
}

impl<F: Fn(ExtensionPermission) -> bool> Checker<'_, F> {
    fn document(&mut self, document: &PanelDocument) -> ExtensionResult<()> {
        if let Some(title) = &document.title {
            label("title", title)?;
        }
        let mut tab_ids = BTreeSet::new();
        if let Some(tabs) = &document.tabs {
            if tabs.len() > TABS_MAX {
                return Err(bad(format!("A panel has at most {TABS_MAX} tabs.")));
            }
            for tab in tabs {
                ident("tab id", &tab.id)?;
                label("tab title", &tab.title)?;
                icon("tab icon", tab.icon.as_deref())?;
                if !tab_ids.insert(tab.id.as_str()) {
                    return Err(bad(format!("Two tabs have the id {:?}.", tab.id)));
                }
            }
        }
        if document.blocks.len() > BLOCKS_MAX {
            return Err(bad(format!("A panel has at most {BLOCKS_MAX} blocks.")));
        }
        let mut block_ids = BTreeSet::new();
        for block in &document.blocks {
            ident("block id", &block.id)?;
            if !block_ids.insert(block.id.as_str()) {
                return Err(bad(format!("Two blocks have the id {:?}.", block.id)));
            }
            if let Some(tab) = &block.tab {
                if !tab_ids.contains(tab.as_str()) {
                    return Err(bad(format!(
                        "Block {:?} names tab {tab:?}, which the panel doesn't have.",
                        block.id
                    )));
                }
            }
            if let Some(title) = &block.title {
                label("block title", title)?;
            }
            self.block(&block.id, block.title.as_deref(), &block.body)
                .map_err(|error| bad(format!("Block {:?}: {}", block.id, error.message)))?;
        }
        Ok(())
    }

    fn block(
        &mut self,
        id: &str,
        title: Option<&str>,
        body: &PanelBlockBody,
    ) -> ExtensionResult<()> {
        match body {
            PanelBlockBody::Rows { items } => {
                count(items.len())?;
                for row in items {
                    self.row(row)?;
                }
            }
            PanelBlockBody::Checklist { items, .. } => {
                count(items.len())?;
                let mut ids = BTreeSet::new();
                for item in items {
                    ident("item id", &item.id)?;
                    if !ids.insert(item.id.as_str()) {
                        return Err(bad(format!("Two items have the id {:?}.", item.id)));
                    }
                    label("item text", &item.text)?;
                    self.optional_action(item.action.as_ref())?;
                    self.menu(item.menu.as_deref())?;
                }
            }
            PanelBlockBody::Map { root, groups } => {
                if let Some(root) = root {
                    label("root title", &root.title)?;
                    optional_label("root subtitle", root.subtitle.as_deref())?;
                    icon("root icon", root.icon.as_deref())?;
                }
                count(groups.len())?;
                let mut total = 0;
                for group in groups {
                    label("group title", &group.title)?;
                    optional_label("group subtitle", group.subtitle.as_deref())?;
                    icon("group icon", group.icon.as_deref())?;
                    total += group.items.len();
                    count(total)?;
                    for item in &group.items {
                        self.row(item)?;
                    }
                }
            }
            PanelBlockBody::Progress {
                value,
                max,
                label: text,
            } => {
                let max = max.unwrap_or(100.0);
                if !value.is_finite()
                    || !max.is_finite()
                    || max <= 0.0
                    || *value < 0.0
                    || *value > max
                {
                    return Err(bad("progress needs 0 <= value <= max, with max above 0."));
                }
                optional_label("label", text.as_deref())?;
            }
            PanelBlockBody::Text { text } => {
                if text.chars().count() > TEXT_MAX_CHARS {
                    return Err(bad(format!(
                        "text is longer than {TEXT_MAX_CHARS} characters."
                    )));
                }
            }
            PanelBlockBody::Buttons { items } => {
                if items.is_empty() || items.len() > MENU_MAX {
                    return Err(bad(format!("buttons has 1 to {MENU_MAX} items.")));
                }
                for button in items {
                    if let Some(id) = &button.id {
                        ident("button id", id)?;
                    }
                    label("button title", &button.title)?;
                    icon("button icon", button.icon.as_deref())?;
                    if button
                        .style
                        .as_deref()
                        .is_some_and(|style| !BUTTON_STYLES.contains(&style))
                    {
                        return Err(bad("A button's style is primary or default."));
                    }
                    self.action(&button.action)?;
                }
            }
            PanelBlockBody::Empty {
                subtitle,
                icon: empty_icon,
                action,
            } => {
                if title.is_none() {
                    return Err(bad("An empty block needs a title."));
                }
                optional_label("subtitle", subtitle.as_deref())?;
                icon("icon", empty_icon.as_deref())?;
                self.optional_action(action.as_ref())?;
            }
            PanelBlockBody::Web { .. } => {
                return Err(bad(
                    "web blocks come in a later version of Ghostex; use the native blocks for now.",
                ));
            }
            PanelBlockBody::Unknown => {
                return Err(bad(format!(
                    "block {id:?} has an unknown type; the v1 types are rows, checklist, map, progress, text, buttons and empty."
                )));
            }
        }
        Ok(())
    }

    fn row(&mut self, row: &PanelRow) -> ExtensionResult<()> {
        if let Some(id) = &row.id {
            ident("row id", id)?;
        }
        if row.title.is_none() && row.project.is_none() && row.session.is_none() {
            return Err(bad("A row needs a title, a project or a session."));
        }
        if row.project.is_some() && row.session.is_some() {
            return Err(bad("A row shows a project or a session, not both."));
        }
        optional_label("title", row.title.as_deref())?;
        optional_label("subtitle", row.subtitle.as_deref())?;
        optional_label("trailing", row.trailing.as_deref())?;
        optional_label("badge", row.badge.as_deref())?;
        optional_label("project", row.project.as_deref())?;
        if let Some(session) = &row.session {
            session_ref(session)?;
        }
        icon("icon", row.icon.as_deref())?;
        if row.dot.as_deref().is_some_and(|dot| !DOTS.contains(&dot)) {
            return Err(bad("A row's dot is green, yellow, red, blue or gray."));
        }
        self.optional_action(row.action.as_ref())?;
        self.menu(row.menu.as_deref())
    }

    fn menu(&mut self, menu: Option<&[PanelMenuItem]>) -> ExtensionResult<()> {
        let Some(menu) = menu else {
            return Ok(());
        };
        if menu.len() > MENU_MAX {
            return Err(bad(format!("A menu has at most {MENU_MAX} items.")));
        }
        for item in menu {
            label("menu title", &item.title)?;
            icon("menu icon", item.icon.as_deref())?;
            self.action(&item.action)?;
        }
        Ok(())
    }

    fn optional_action(&mut self, action: Option<&PanelAction>) -> ExtensionResult<()> {
        action.map_or(Ok(()), |action| self.action(action))
    }

    fn action(&mut self, action: &PanelAction) -> ExtensionResult<()> {
        match action {
            PanelAction::OpenProject(project) => label("openProject", project)?,
            PanelAction::FocusSession(session) => session_ref(session)?,
            PanelAction::NewAgent(new_agent) => {
                label("newAgent.project", &new_agent.project)?;
                if new_agent
                    .draft
                    .as_deref()
                    .is_some_and(|draft| draft.chars().count() > TEXT_MAX_CHARS)
                {
                    return Err(bad(format!(
                        "newAgent.draft is longer than {TEXT_MAX_CHARS} characters."
                    )));
                }
                if let Some(agent) = &new_agent.agent {
                    ident("newAgent.agent", agent)?;
                }
            }
            PanelAction::OpenUrl(url) => open_url(url)?,
            PanelAction::OpenView(view) => ident("openView", view)?,
            PanelAction::Extension(target) => {
                ident("extension action name", target.name())?;
                if let ghostex_gx_protocol::extension_panel::ExtensionActionTarget::Detailed {
                    data: Some(data),
                    ..
                } = target
                {
                    let size = serde_json::to_vec(data)
                        .map(|bytes| bytes.len())
                        .unwrap_or(usize::MAX);
                    if size > ACTION_DATA_MAX_BYTES {
                        return Err(bad(format!(
                            "An extension action's data is at most {ACTION_DATA_MAX_BYTES} bytes."
                        )));
                    }
                }
            }
        }
        if let Some(permission) = action
            .required_permission()
            .and_then(ExtensionPermission::from_wire)
        {
            if !(self.allowed)(permission) {
                return Err(bad(format!(
                    "This action needs the {} permission, which the extension doesn't have.",
                    permission.as_str()
                )));
            }
        }
        Ok(())
    }
}

fn bad(message: impl Into<String>) -> ExtensionError {
    ExtensionError::bad_request(message)
}

fn count(items: usize) -> ExtensionResult<()> {
    if items > ITEMS_MAX {
        Err(bad(format!("A block has at most {ITEMS_MAX} items.")))
    } else {
        Ok(())
    }
}

fn label(field: &str, value: &str) -> ExtensionResult<()> {
    if value.trim().is_empty() {
        return Err(bad(format!("{field} must not be empty.")));
    }
    if value.chars().count() > LABEL_MAX_CHARS {
        return Err(bad(format!(
            "{field} is longer than {LABEL_MAX_CHARS} characters."
        )));
    }
    Ok(())
}

fn optional_label(field: &str, value: Option<&str>) -> ExtensionResult<()> {
    value.map_or(Ok(()), |value| label(field, value))
}

fn ident(field: &str, value: &str) -> ExtensionResult<()> {
    if !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        Ok(())
    } else {
        Err(bad(format!(
            "{field} {value:?} must be 1-64 letters, digits, '.', '_' or '-'."
        )))
    }
}

fn session_ref(value: &str) -> ExtensionResult<()> {
    match value.split_once(':') {
        Some((project, session)) if valid_ref_part(project) && valid_ref_part(session) => Ok(()),
        _ => Err(bad(format!(
            "session {value:?} must be <projectId>:<sessionId>."
        ))),
    }
}

/// A Tabler icon name (`map`, `focus-2`) or an SVG in the extension's folder.
fn icon(field: &str, value: Option<&str>) -> ExtensionResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let tabler = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if tabler {
        return Ok(());
    }
    if value.to_ascii_lowercase().ends_with(".svg") {
        return validate_relative_path(value, field);
    }
    Err(bad(format!(
        "{field} {value:?} must be a Tabler icon name or an .svg file in the extension."
    )))
}

fn open_url(value: &str) -> ExtensionResult<()> {
    let url =
        url::Url::parse(value).map_err(|_| bad(format!("openUrl {value:?} is not a URL.")))?;
    let loopback = match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    if url.scheme() == "https" || (url.scheme() == "http" && loopback) {
        Ok(())
    } else {
        Err(bad("openUrl must be https (http only for localhost)."))
    }
}
