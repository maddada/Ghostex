//! Moving the Community Canvas extension's boards into drawings, then removing it.
//!
//! CDXC:Canvas 2026-11-27 DECISION:
//! User (canvas page Q1 A, Q3 A): Canvas becomes a built-in view that opens Ghostex's own
//! Excalidraw on the project's drawings in `.ghostex/drawings/`, the Community Canvas extension
//! leaves the Store, and for people who have it installed "every board becomes a .excalidraw file
//! in its project's .ghostex/drawings/, Canvas is uninstalled, and a one-time note says where they
//! went". Uninstalling deletes the extension's storage, so the order is fixed: every board is
//! written (never over an existing file) and read back first, and only then is Canvas
//! uninstalled; when anything fails, the files this run wrote are removed again, Canvas stays
//! installed and working, and no note is shown. Background pictures were not picked for the
//! built-in, so they are dropped (the note says so only when a board had one).
//!
//! CDXC:Canvas 2026-11-28 DECISION:
//! User (Q5292, "No project folder"): a board that is not linked to a project, or whose project
//! is gone, goes to No project's folder, `~/.ghostex/drawings/` (No project's chats start in the
//! home folder, so its drawings live under it, as Main Hub's do), and Canvas opens it there while
//! No project is the active project. A board with nothing drawn on it carries nothing to keep and
//! is not written.
//!
//! The extension's storage (`ExtensionStoreEntry::storage`) holds `boards` (the index:
//! `{schemaVersion, boards: [{id, name, projectId}], lastOpen}`), `board:<id>` (an Excalidraw scene
//! `{schemaVersion: 6, id, elements, appState, files}`), `background:<id>` (`{color, fit, image,
//! …}`) and `library`. Ghostex-extensions `extensions/canvas/src/{boards,board,background}.ts`.

use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// The Community Canvas extension, as the Store published it.
pub(crate) const CANVAS_EXTENSION_ID: &str = "canvas";
const CANVAS_EXTENSION_AUTHOR: &str = "banozz";

/// The one-time note the desktop shows after the move, in the extensions folder.
pub(crate) const CANVAS_MOVED_NOTE_FILE: &str = "canvas-moved.json";
/// Why the last start could not move the boards, for support logs and the report.
const CANVAS_MOVE_BLOCKED_FILE: &str = "canvas-move-blocked.json";

/// The project folder Ghostex keeps its own things in, and the drawings under it.
pub(crate) const GHOSTEX_PROJECT_FOLDER: &str = ".ghostex";
pub(crate) const GHOSTEX_DRAWINGS_FOLDER: &str = "drawings";

/// The last Excalidraw scene version Canvas's own engine wrote; older boards need Canvas's
/// JavaScript converter (`migrate.ts`), which only runs inside the extension.
const CANVAS_FIRST_EXCALIDRAW_SCHEMA: i64 = 6;

/// Whether `id` by `author` is the Community Canvas extension: it leaves the Store and cannot be
/// installed again, but an installed copy keeps working until its boards are moved.
pub(crate) fn is_canvas_extension(id: &str, author: &str) -> bool {
    id == CANVAS_EXTENSION_ID && author.trim() == CANVAS_EXTENSION_AUTHOR
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CanvasMovedNote {
    /// How many boards became drawings.
    pub(crate) boards: usize,
    /// A board had a background picture, which did not come along.
    pub(crate) dropped_pictures: bool,
    /// The drawing each project should open first: its Canvas board that was open last, by file
    /// name in `.ghostex/drawings/`.
    #[serde(default)]
    pub(crate) last_drawings: BTreeMap<String, String>,
    /// Every file written, by project id, for support.
    #[serde(default)]
    pub(crate) files: Vec<CanvasMovedFile>,
    /// The desktop showed the note.
    #[serde(default)]
    pub(crate) shown: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CanvasMovedFile {
    pub(crate) project_id: String,
    pub(crate) path: String,
}

struct BoardToMove {
    id: String,
    name: String,
    project_id: Option<String>,
    elements: Vec<Value>,
    app_state: Map<String, Value>,
    files: Map<String, Value>,
    picture: bool,
}

/// Runs once per gxserver start, off the request path. Does nothing unless the Community Canvas
/// extension is installed.
pub(crate) fn run(state: &AppState) {
    let extensions_dir = state.paths.extensions_dir.clone();
    let payload = extensions_dir.join("installed").join(CANVAS_EXTENSION_ID);
    let Ok(manifest) = super::read_manifest(&payload, Some(CANVAS_EXTENSION_ID)) else {
        return;
    };
    if !is_canvas_extension(&manifest.name, &manifest.author) {
        return;
    }
    let note_path = extensions_dir.join(CANVAS_MOVED_NOTE_FILE);
    // The boards were written on an earlier start and only the uninstall failed: finish it.
    if note_path.exists() {
        uninstall(state);
        return;
    }
    match move_boards(state) {
        Ok(note) => {
            let _ = fs::remove_file(extensions_dir.join(CANVAS_MOVE_BLOCKED_FILE));
            if let Err(error) = write_json_atomically(&note_path, &note) {
                eprintln!("Canvas: could not write the moved-boards note: {error}");
                remove_written(&note.files);
                return;
            }
            uninstall(state);
        }
        Err(reason) => {
            eprintln!("Canvas: boards not moved, the extension stays installed: {reason}");
            let _ = write_json_atomically(
                &extensions_dir.join(CANVAS_MOVE_BLOCKED_FILE),
                &json!({ "reason": reason }),
            );
        }
    }
}

fn uninstall(state: &AppState) {
    if let Err(error) = state.extension_registry.uninstall(CANVAS_EXTENSION_ID) {
        eprintln!("Canvas: boards moved, but the extension could not be removed: {error}");
    }
}

/// Writes every board with content into its project's drawings folder and reads each back.
/// On any failure the files written so far are removed and the reason returned.
fn move_boards(state: &AppState) -> Result<CanvasMovedNote, String> {
    let store =
        super::read_store(&state.paths.extensions_store_file).map_err(|error| error.to_string())?;
    let storage = store
        .get(CANVAS_EXTENSION_ID)
        .map(|entry| entry.storage.clone())
        .unwrap_or_default();
    let (boards, last_open) = read_boards(&storage)?;
    let boards: Vec<BoardToMove> = boards
        .into_iter()
        .filter(|board| !board.elements.is_empty())
        .collect();

    let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    // Where each board goes: its project's folder, or No project's (the home folder).
    let mut targets: Vec<(String, PathBuf)> = Vec::with_capacity(boards.len());
    let mut no_project: Option<(String, PathBuf)> = None;
    for board in &boards {
        let project = board.project_id.as_deref().and_then(|project_id| {
            repository
                .get_project_without_session_history(project_id)
                .ok()
                .flatten()
                .and_then(|project| {
                    project
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|path| !path.is_empty())
                        .map(PathBuf::from)
                })
                .filter(|path| path.is_dir())
                .map(|path| (project_id.to_string(), path))
        });
        let target = match project {
            Some(target) => target,
            None => match &no_project {
                Some(target) => target.clone(),
                None => {
                    let target = no_project_target(state, &db, &repository).map_err(|error| {
                        format!("board \"{}\" has no project, and {error}", board.name)
                    })?;
                    no_project = Some(target.clone());
                    target
                }
            },
        };
        targets.push(target);
    }

    let mut note = CanvasMovedNote::default();
    for (board, (project_id, project)) in boards.iter().zip(&targets) {
        let written = write_board(project, board).and_then(|path| {
            verify_board(&path, board)?;
            Ok(path)
        });
        match written {
            Ok(path) => {
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if last_open.as_deref() == Some(board.id.as_str())
                    || !note.last_drawings.contains_key(project_id)
                {
                    note.last_drawings.insert(project_id.clone(), file_name);
                }
                note.files.push(CanvasMovedFile {
                    project_id: project_id.clone(),
                    path: path.to_string_lossy().into_owned(),
                });
                note.boards += 1;
                note.dropped_pictures |= board.picture;
            }
            Err(error) => {
                remove_written(&note.files);
                return Err(format!("board \"{}\": {error}", board.name));
            }
        }
    }
    Ok(note)
}

/// No project's id and the folder its drawings go under: the home folder, so they land in
/// `~/.ghostex/drawings/`, where Canvas opens them in No project (apps/desktop/src/app/canvas).
fn no_project_target(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
) -> Result<(String, PathBuf), String> {
    let project = crate::no_project::ensure_no_project_project(state, db, repository)
        .map_err(|error| format!("No project could not be found: {error}"))?;
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .ok_or("No project has no id.")?
        .to_string();
    let home = crate::resume_lookup::home_dir();
    if !home.is_dir() {
        return Err(format!("the home folder {} is missing", home.display()));
    }
    Ok((project_id, home))
}

fn remove_written(files: &[CanvasMovedFile]) {
    for file in files {
        let _ = fs::remove_file(&file.path);
    }
}

/// The boards in Canvas's index, with their scenes, and the board that was open last.
fn read_boards(
    storage: &BTreeMap<String, Value>,
) -> Result<(Vec<BoardToMove>, Option<String>), String> {
    let index = storage.get("boards");
    let mut entries: Vec<(String, String, Option<String>)> = index
        .and_then(|index| index.get("boards"))
        .and_then(Value::as_array)
        .map(|boards| {
            boards
                .iter()
                .filter_map(|board| {
                    let id = board.get("id")?.as_str()?.trim();
                    (!id.is_empty()).then(|| {
                        (
                            id.to_string(),
                            board
                                .get("name")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|name| !name.is_empty())
                                .unwrap_or("Canvas")
                                .to_string(),
                            board
                                .get("projectId")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|id| !id.is_empty())
                                .map(str::to_string),
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // A first run that never wrote its index still has the board the skeleton wrote.
    if entries.is_empty() && storage.contains_key("board:default") {
        entries.push(("default".into(), "Canvas".into(), None));
    }
    let last_open = index
        .and_then(|index| index.get("lastOpen"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut boards = Vec::new();
    for (id, name, project_id) in entries {
        let scene = storage
            .get(&format!("board:{id}"))
            .filter(|scene| scene.is_object());
        let picture = storage
            .get(&format!("background:{id}"))
            .and_then(|background| background.get("image"))
            .and_then(Value::as_str)
            .is_some_and(|image| !image.is_empty());
        let Some(scene) = scene else {
            boards.push(BoardToMove {
                id,
                name,
                project_id,
                elements: Vec::new(),
                app_state: Map::new(),
                files: Map::new(),
                picture,
            });
            continue;
        };
        let elements: Vec<Value> = scene
            .get("elements")
            .and_then(Value::as_array)
            .map(|elements| {
                elements
                    .iter()
                    .filter(|element| {
                        element.is_object()
                            && element.get("isDeleted").and_then(Value::as_bool) != Some(true)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let version = scene
            .get("schemaVersion")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if version < CANVAS_FIRST_EXCALIDRAW_SCHEMA && !elements.is_empty() {
            return Err(format!(
                "board \"{name}\" was last saved by Canvas's older drawing engine (schema {version}); open it in Canvas once so Canvas converts it"
            ));
        }
        boards.push(BoardToMove {
            id,
            name,
            project_id,
            elements,
            app_state: scene
                .get("appState")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            files: scene
                .get("files")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            picture,
        });
    }
    Ok((boards, last_open))
}

/// Creates `<project>/.ghostex/` and its `.gitignore` (`*`, so git never sees the folder and the
/// repo's own `.gitignore` is never touched), then `drawings/` inside it.
///
/// SEE-ALSO: apps/desktop/src/app/canvas/mod.rs `ensure_drawings_folder` makes the same folder the same way.
pub(crate) fn ensure_drawings_folder(project: &Path) -> std::io::Result<PathBuf> {
    let ghostex = project.join(GHOSTEX_PROJECT_FOLDER);
    fs::create_dir_all(&ghostex)?;
    let gitignore = ghostex.join(".gitignore");
    if !gitignore.exists() {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&gitignore)?;
        file.write_all(GHOSTEX_FOLDER_GITIGNORE.as_bytes())?;
    }
    let drawings = ghostex.join(GHOSTEX_DRAWINGS_FOLDER);
    fs::create_dir_all(&drawings)?;
    Ok(drawings)
}

/// What `.ghostex/.gitignore` holds when Ghostex makes the folder.
pub(crate) const GHOSTEX_FOLDER_GITIGNORE: &str = "# Ghostex keeps this folder out of git. A drawing included in git\n# from Canvas gets its own line below.\n*\n";

/// A file name from a board name: no path separators or characters other systems refuse.
fn drawing_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches('.').trim();
    if cleaned.is_empty() {
        "Canvas".to_string()
    } else {
        cleaned.chars().take(80).collect()
    }
}

fn write_board(project: &Path, board: &BoardToMove) -> Result<PathBuf, String> {
    let drawings = ensure_drawings_folder(project).map_err(|error| error.to_string())?;
    let mut app_state = Map::new();
    for (key, value) in &board.app_state {
        let kept = matches!(
            key.as_str(),
            "scrollX" | "scrollY" | "zoom" | "theme" | "gridModeEnabled" | "objectsSnapModeEnabled"
        ) || key.starts_with("currentItem");
        if kept {
            app_state.insert(key.clone(), value.clone());
        }
    }
    let content = serde_json::to_string_pretty(&json!({
        "type": "excalidraw",
        "version": 2,
        "source": "https://excalidraw.com",
        "elements": board.elements,
        "appState": app_state,
        "files": board.files,
    }))
    .map_err(|error| error.to_string())?;
    let stem = drawing_file_stem(&board.name);
    for attempt in 1..1000 {
        let name = if attempt == 1 {
            format!("{stem}.excalidraw")
        } else {
            format!("{stem} {attempt}.excalidraw")
        };
        let path = drawings.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let written = file
                    .write_all(content.as_bytes())
                    .and_then(|()| file.sync_all());
                if let Err(error) = written {
                    let _ = fs::remove_file(&path);
                    return Err(error.to_string());
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("no free file name".to_string())
}

/// Reads a written drawing back: the same elements (by id and version) and the same pictures.
fn verify_board(path: &Path, board: &BoardToMove) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let key = |element: &Value| {
        (
            element.get("id").cloned().unwrap_or(Value::Null),
            element.get("version").cloned().unwrap_or(Value::Null),
        )
    };
    let read: Vec<_> = value
        .get("elements")
        .and_then(Value::as_array)
        .map(|elements| elements.iter().map(key).collect())
        .unwrap_or_default();
    let expected: Vec<_> = board.elements.iter().map(key).collect();
    if read != expected {
        return Err(format!("{} did not read back the same", path.display()));
    }
    let files = value.get("files").and_then(Value::as_object);
    if files.map(|files| files.len()).unwrap_or(0) != board.files.len() {
        return Err(format!("{} lost pictures on the way", path.display()));
    }
    Ok(())
}

fn write_json_atomically(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// The note to show, when the boards were moved and Canvas is gone and the desktop has not shown
/// it yet.
pub(crate) fn pending_note(extensions_dir: &Path) -> Option<CanvasMovedNote> {
    if extensions_dir
        .join("installed")
        .join(CANVAS_EXTENSION_ID)
        .exists()
    {
        return None;
    }
    let note: CanvasMovedNote =
        serde_json::from_slice(&fs::read(extensions_dir.join(CANVAS_MOVED_NOTE_FILE)).ok()?)
            .ok()?;
    (!note.shown).then_some(note)
}

/// The desktop showed the note: it is not shown again.
pub(crate) fn mark_note_shown(extensions_dir: &Path) -> std::io::Result<()> {
    let path = extensions_dir.join(CANVAS_MOVED_NOTE_FILE);
    let Ok(bytes) = fs::read(&path) else {
        return Ok(());
    };
    let Ok(mut note) = serde_json::from_slice::<CanvasMovedNote>(&bytes) else {
        return Ok(());
    };
    note.shown = true;
    write_json_atomically(&path, &note)
}
