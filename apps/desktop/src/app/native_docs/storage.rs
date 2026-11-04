//! Docs' saved state: the open files, their unsaved drafts and the selected file per project, and
//! whether the files list is pinned. The keys and value shapes are the Docs page's own, so the
//! state the page left behind opens here unchanged.
//!
//! SEE-ALSO: packages/client-storage/catalog.ts (`docsOpenFiles`, `docsDrafts`, `docsActiveFile`,
//! `docsSidebar`), apps/desktop/src/app/gx_store/records_storage.rs (the door these go through).

use std::collections::BTreeMap;
use std::time::Duration;

use gpui::Context;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::gx_store::{
    RecordRead, RecordStore, read_preference_value, read_record_raw, remove_record,
    write_client_document_value, write_record,
};

const MIB: i64 = 1024 * 1024;

/// The catalog's `disk` bounds.
const OPEN_FILES: RecordStore = RecordStore {
    id: "docsOpenFiles",
    version: 1,
    max_entry_bytes: 2 * MIB,
    max_bytes: 16 * MIB,
    max_entries: 2_000,
    max_age_ms: None,
};
const ACTIVE_FILE: RecordStore = RecordStore {
    id: "docsActiveFile",
    ..OPEN_FILES
};
/// `protectedDisk` with an 8 MiB entry: drafts are user work and are never evicted.
const DRAFTS: RecordStore = RecordStore {
    id: "docsDrafts",
    version: 1,
    max_entry_bytes: 8 * MIB,
    max_bytes: 32 * MIB,
    max_entries: 50_000,
    max_age_ms: None,
};

const OPEN_FILES_PREFIX: &str = "ghostex.manage.openFiles.";
/// CDXC:Docs 2026-09-15 DECISION:
/// User: every file opened in Docs gets a row in an open-files list above the tree, each open file keeps its own unsaved draft while another is selected, and drafts survive a quit or reload, still marked unsaved. Open files and drafts are stored per project; a draft is applied on top of the disk content the next time its file is read.
const DRAFTS_PREFIX: &str = "ghostex.manage.drafts.";
const ACTIVE_FILE_PREFIX: &str = "ghostex.manage.activeFile.";
const SIDEBAR_PINNED_KEY: &str = "ghostex.manage.sidebarPinned";
const FORMAT_BAR_COLLAPSED_KEY: &str = "ghostex.manage.formattingBarCollapsed";
const WORD_WRAP_KEY: &str = "ghostex.manage.wordWrap";
/// Drafts are written this long after the last keystroke.
const DRAFT_WRITE_DELAY: Duration = Duration::from_millis(400);

/// `ManageBackgroundDraft`: the edited text and the disk text it was edited from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocsDraft {
    pub(crate) draft: String,
    pub(crate) saved_content: String,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn read_raw(store: RecordStore, key: &str) -> Option<String> {
    match read_record_raw(store, key, now_ms()) {
        Ok(RecordRead::Payload(raw)) => Some(raw),
        _ => None,
    }
}

/// A stored file path as Files addresses it now: an outside file saved under its old
/// `.ghostex-chat-file/<id>/` address becomes its real path, or `None` once its grant is gone.
///
/// An outside file saved with the Windows `\\?\` prefix (before 2026-10-08) loses it here, so it
/// keeps one key and one label.
fn stored_address(project_id: &str, path: String) -> Option<String> {
    if crate::app::helpers::manage_chat_file_address(&path).is_none() {
        return Some(crate::app::helpers::strip_verbatim_prefix(&path));
    }
    crate::app::helpers::manage_chat_file_real_path(project_id, &path)
}

/// `readStoredManageOpenFiles`: the stored list, without review documents (which have no file).
pub(crate) fn read_open_files(project_id: &str) -> Vec<String> {
    let mut open: Vec<String> = Vec::new();
    for path in read_raw(OPEN_FILES, &format!("{OPEN_FILES_PREFIX}{project_id}"))
        .and_then(|raw| serde_json::from_str::<Vec<Value>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| path.as_str().map(str::to_string))
        .filter_map(|path| stored_address(project_id, path))
        .filter(|path| !path.is_empty() && !path.starts_with(".ghostex-review"))
    {
        // A file saved under both spellings of its path is one open file.
        if !open.contains(&path) {
            open.push(path);
        }
    }
    open
}

/// `readStoredManageActiveFile`: the selected file, if it is still in the stored open list.
pub(crate) fn read_active_file(project_id: &str, open: &[String]) -> Option<String> {
    read_raw(ACTIVE_FILE, &format!("{ACTIVE_FILE_PREFIX}{project_id}"))
        .and_then(|path| stored_address(project_id, path))
        .filter(|path| open.contains(path))
}

/// `readStoredManageDrafts`: drafts whose text still differs from what they were edited from.
///
/// CDXC:Docs 2026-09-25 WHY: The web Docs page stashed an empty draft for a file whose text had
/// not arrived yet (every entry of that shape in real profiles has `draft: ""` over a full
/// `savedContent`), so restoring it opened a blank, dirty document that one save would wipe. An
/// empty draft over a non-empty file is never kept.
pub(crate) fn read_drafts(project_id: &str) -> BTreeMap<String, DocsDraft> {
    let Some(Value::Object(stored)) = read_raw(DRAFTS, &format!("{DRAFTS_PREFIX}{project_id}"))
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return BTreeMap::new();
    };
    stored
        .into_iter()
        .filter_map(|(path, value)| {
            let path = stored_address(project_id, path)?;
            let draft = value["draft"].as_str()?.to_string();
            let saved_content = value["savedContent"].as_str()?.to_string();
            (draft != saved_content && !draft.is_empty()).then_some((
                path,
                DocsDraft {
                    draft,
                    saved_content,
                },
            ))
        })
        .collect()
}

/// The files list's pinned intent; pinned unless the user hid it.
pub(crate) fn read_sidebar_pinned() -> bool {
    read_preference_value(SIDEBAR_PINNED_KEY)
        .ok()
        .flatten()
        .as_deref()
        != Some("false")
}

/// Moves the Docs page's newer browser-era state into the native database, once, before native
/// Docs first reads it (`import_docs_browser_state`).
pub(crate) fn import_browser_state_once() {
    let profile = std::env::var_os("GHOSTEX_GPUI_CEF_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::shared_settings::ghostex_storage_paths().cef_cache_dir())
        .join("Default");
    let result = crate::app::gx_store::with_write_connection(|connection| {
        ghostex_client_storage::import_docs_browser_state(connection, &profile).map_err(|error| {
            eprintln!("[ghostex-gpui] Could not move the Docs page's saved state: {error:#}");
            "import"
        })
    });
    let _ = result;
}

/// The formatting bar's remembered fold; open unless the user folded it.
pub(crate) fn read_format_bar_collapsed() -> bool {
    read_preference_value(FORMAT_BAR_COLLAPSED_KEY)
        .ok()
        .flatten()
        .as_deref()
        == Some("true")
}

/// The header's wrap toggle for text and code files; off unless the user turned it on.
pub(crate) fn read_word_wrap() -> bool {
    read_preference_value(WORD_WRAP_KEY)
        .ok()
        .flatten()
        .as_deref()
        == Some("true")
}

impl GhostexGpuiApp {
    /// Writes the wrap toggle.
    pub(crate) fn native_docs_persist_word_wrap(&mut self, cx: &mut Context<Self>) {
        let wrap = self.native_docs.word_wrap;
        cx.background_executor()
            .spawn(async move {
                let _ = write_client_document_value(
                    WORD_WRAP_KEY,
                    Some(if wrap { "true" } else { "false" }),
                );
            })
            .detach();
    }

    /// Writes the formatting bar's fold.
    pub(crate) fn native_docs_persist_format_bar(&mut self, cx: &mut Context<Self>) {
        let collapsed = self.native_docs.format_bar_collapsed;
        cx.background_executor()
            .spawn(async move {
                let _ = write_client_document_value(
                    FORMAT_BAR_COLLAPSED_KEY,
                    Some(if collapsed { "true" } else { "false" }),
                );
            })
            .detach();
    }

    fn native_docs_project_id(&self) -> Option<String> {
        self.native_docs
            .project
            .as_ref()
            .map(|project| project.project_id.clone())
    }

    /// Reopens the files that were open when Docs last closed, with their drafts.
    pub(crate) fn native_docs_restore_open_files(&mut self, cx: &mut Context<Self>) {
        let Some(project_id) = self.native_docs_project_id() else {
            return;
        };
        self.native_docs.drafts = read_drafts(&project_id);
        let open = read_open_files(&project_id);
        let active = read_active_file(&project_id, &open).or_else(|| open.first().cloned());
        for path in &open {
            if self.native_docs.document(path).is_none() {
                let mut document = super::state::DocsDocument::new(path.clone(), path.clone());
                document.dirty = self.native_docs.drafts.contains_key(path);
                document.restored = true;
                self.native_docs.documents.push(document);
                self.native_docs_read(path, cx);
            }
        }
        self.native_docs.active = active;
    }

    /// Writes the open list and the selected file for the current project.
    pub(crate) fn native_docs_persist_open_files(&mut self, cx: &mut Context<Self>) {
        let Some(project_id) = self.native_docs_project_id() else {
            return;
        };
        let open: Vec<String> = self
            .native_docs
            .documents
            .iter()
            .map(|document| document.path.to_string())
            .filter(|path| {
                !path.starts_with(".ghostex-review") && !super::published_page::is_page_path(path)
            })
            .collect();
        let active = self.native_docs.active.clone();
        cx.background_executor()
            .spawn(async move {
                let now = now_ms();
                let open_key = format!("{OPEN_FILES_PREFIX}{project_id}");
                let active_key = format!("{ACTIVE_FILE_PREFIX}{project_id}");
                let _ = if open.is_empty() {
                    remove_record(&open_key).map(|_| ())
                } else {
                    write_record(OPEN_FILES, &open_key, &json!(open).to_string(), now).map(|_| ())
                };
                let _ = match active {
                    Some(active) => {
                        write_record(ACTIVE_FILE, &active_key, &active, now).map(|_| ())
                    }
                    None => remove_record(&active_key).map(|_| ()),
                };
            })
            .detach();
    }

    /// The draft to show for `path` over `disk_text`, when the stored one still applies: a file
    /// saved elsewhere with the draft's text drops its stale draft on its own.
    pub(crate) fn native_docs_stored_draft(&self, path: &str, disk_text: &str) -> Option<String> {
        self.native_docs
            .drafts
            .get(path)
            .filter(|draft| draft.draft != disk_text)
            .map(|draft| draft.draft.clone())
    }

    /// Records the editor's text as `path`'s draft and writes the drafts a moment later.
    pub(crate) fn native_docs_schedule_draft_write(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document(path) else {
            return;
        };
        let Some(text) = super::live::document_text(document, cx) else {
            return;
        };
        if text == document.saved_text {
            self.native_docs.drafts.remove(path);
        } else {
            self.native_docs.drafts.insert(
                path.to_string(),
                DocsDraft {
                    draft: text,
                    saved_content: document.saved_text.clone(),
                },
            );
        }
        let generation = self.native_docs.generation;
        self.native_docs.draft_write = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DRAFT_WRITE_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.native_docs.generation == generation {
                    this.native_docs_write_drafts(cx);
                }
            });
        }));
    }

    pub(crate) fn native_docs_clear_draft(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.native_docs.drafts.remove(path).is_some() {
            self.native_docs_write_drafts(cx);
        }
    }

    /// `writeStoredManageDrafts`: the whole map, or no row at all when nothing is unsaved.
    pub(crate) fn native_docs_write_drafts(&mut self, cx: &mut Context<Self>) {
        let Some(project_id) = self.native_docs_project_id() else {
            return;
        };
        let stored: serde_json::Map<String, Value> = self
            .native_docs
            .drafts
            .iter()
            .map(|(path, draft)| {
                (
                    path.clone(),
                    json!({ "draft": draft.draft, "savedContent": draft.saved_content }),
                )
            })
            .collect();
        cx.background_executor()
            .spawn(async move {
                let key = format!("{DRAFTS_PREFIX}{project_id}");
                let _ = if stored.is_empty() {
                    remove_record(&key).map(|_| ())
                } else {
                    write_record(DRAFTS, &key, &Value::Object(stored).to_string(), now_ms())
                        .map(|_| ())
                };
            })
            .detach();
    }

    /// Writes the files list's pinned intent.
    pub(crate) fn native_docs_persist_sidebar_pinned(&mut self, cx: &mut Context<Self>) {
        let pinned = self.native_docs.sidebar_pinned;
        cx.background_executor()
            .spawn(async move {
                let _ = write_client_document_value(
                    SIDEBAR_PINNED_KEY,
                    Some(if pinned { "true" } else { "false" }),
                );
            })
            .detach();
    }
}
