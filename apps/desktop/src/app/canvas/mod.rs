//! The built-in Canvas view: Ghostex's own Excalidraw on the project's drawings.
//!
//! CDXC:Canvas 2026-11-27 DECISION:
//! User (canvas page, Q1 A): "a built-in Canvas view" replaces the Community Canvas extension: a
//! card in Open a view ("Canvas: Sketches and diagrams on a board") that opens Ghostex's own
//! Excalidraw on your last drawing. Drawings live "hidden in the project" (Q2 C) in
//! `.ghostex/drawings/` at the project's root; Ghostex writes `.ghostex/.gitignore` holding `*`
//! when it makes the folder, so git never sees it and the repo's own `.gitignore` is untouched.
//! Extras picked (Q4): the board switcher and reopen last. The user: "i dont want to fork
//! excalidraw if possible i wanna keep it as default as much as possible", so the drawing itself is
//! the Files view's embed page (`apps/desktop/views/manage/embed.tsx`, stock `@excalidraw/excalidraw`
//! through its public props), which already saves each drawing to its file and keeps its scroll
//! and zoom in it; everything Canvas adds (the switcher, the folder, the last drawing) is Ghostex's
//! own, around the page.
//!
//! The view keeps the extension's id, `extension:canvas`, like Storybook did, so saved tabs and
//! the "canvas" view numbers carry over. While the Community extension is still installed (its
//! boards could not be moved yet, server/src/extensions/canvas_migration.rs) the id is the
//! extension's and this view stays out of the way.
//!
//! Canvas needs a project on this computer: with no project, or a remote one, its card in Open a
//! view is disabled with a reason.

pub(crate) mod dialog;
pub(crate) mod menu;
pub(crate) mod note;

use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::Context;
use serde_json::json;

use crate::app::gx_store::{RecordRead, RecordStore, read_record_raw, write_record};
use crate::app::helpers::*;
use crate::*;

/// The view's id: the Community Canvas extension's, so its saved tabs and numbers carry over.
pub(crate) const CANVAS_VIEW_ID: &str = "canvas";
/// Settings > Customize's switch for the view.
pub(crate) const CANVAS_VIEW_TAB_HIDDEN_SETTINGS_KEY: &str = "canvasViewTabHidden";
/// Where a project's drawings live, from its root.
pub(crate) const CANVAS_DRAWINGS_RELATIVE_PATH: &str = ".ghostex/drawings";
const DRAWING_EXTENSION: &str = ".excalidraw";
/// The picker card's reason when there is no project to keep drawings in.
pub(crate) const CANVAS_NO_PROJECT_REASON: &str = "Open a project on this computer to draw in it";

/// What `.ghostex/.gitignore` holds when Ghostex makes the folder.
///
/// SEE-ALSO: server/src/extensions/canvas_migration.rs `GHOSTEX_FOLDER_GITIGNORE`, the same text.
const GHOSTEX_FOLDER_GITIGNORE: &str = "# Ghostex keeps this folder out of git. A drawing included in git\n# from Canvas gets its own line below.\n*\n";

/// The drawing each project had open last, by file name (`canvasLastDrawing` in
/// packages/client-storage/catalog.ts).
const LAST_DRAWING: RecordStore = RecordStore {
    id: "canvasLastDrawing",
    version: 1,
    max_entry_bytes: 4 * 1024,
    max_bytes: 4 * 1024 * 1024,
    max_entries: 2_000,
    max_age_ms: None,
};
const LAST_DRAWING_PREFIX: &str = "ghostex.canvas.lastDrawing.";

static CANVAS_EXTENSION_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Whether the Community Canvas extension is still installed (its boards not moved yet).
pub(crate) fn canvas_extension_installed() -> bool {
    CANVAS_EXTENSION_INSTALLED.load(Ordering::Relaxed)
}

/// Records whether the Community Canvas extension is installed; true when that changed.
pub(crate) fn note_canvas_extension_installed(installed: bool) -> bool {
    CANVAS_EXTENSION_INSTALLED.swap(installed, Ordering::Relaxed) != installed
}

/// The built-in view, or `None` while the Community extension holds its id.
pub(crate) fn canvas_view() -> Option<GpuiCustomView> {
    if canvas_extension_installed() {
        return None;
    }
    Some(GpuiCustomView {
        id: ExtensionId::new(CANVAS_VIEW_ID)?,
        title: "Canvas".into(),
        enabled: !shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(CANVAS_VIEW_TAB_HIDDEN_SETTINGS_KEY)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        url: String::new(),
        definition: json!({ "id": CANVAS_VIEW_ID, "name": "Canvas" }),
    })
}

impl TitlebarMode {
    pub(crate) fn is_canvas(self) -> bool {
        matches!(self, Self::Extension(id) if id.as_str() == CANVAS_VIEW_ID)
            && !canvas_extension_installed()
    }
}

/// A drawing in `.ghostex/drawings/`.
#[derive(Clone, Debug)]
pub(crate) struct CanvasDrawing {
    /// Its file name, `Release plan.excalidraw`.
    pub(crate) file: String,
    /// What the switcher calls it, `Release plan`.
    pub(crate) name: String,
    pub(crate) modified: std::time::SystemTime,
}

/// The project Canvas draws in: the active project, when it is on this computer.
#[derive(Clone, Debug)]
pub(crate) struct CanvasProject {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
}

impl CanvasProject {
    pub(crate) fn drawings_dir(&self) -> PathBuf {
        self.path.join(CANVAS_DRAWINGS_RELATIVE_PATH)
    }

    fn gitignore(&self) -> PathBuf {
        self.path.join(".ghostex").join(".gitignore")
    }
}

#[derive(Default)]
pub(crate) struct CanvasState {
    /// The drawing open in each project, by file name.
    pub(crate) open: HashMap<String, String>,
    /// Bumped to load the open drawing's page again (after it was renamed).
    pub(crate) revision: u64,
    /// Rename or Delete, over the view.
    pub(crate) dialog: Option<dialog::CanvasDialog>,
    /// The moved-boards note was shown this run.
    pub(crate) note_shown: bool,
}

pub(crate) fn drawing_name(file: &str) -> String {
    file.strip_suffix(DRAWING_EXTENSION)
        .unwrap_or(file)
        .to_string()
}

/// Makes `.ghostex/` (with its `.gitignore`) and `drawings/` inside it.
fn ensure_drawings_folder(project: &CanvasProject) -> std::io::Result<PathBuf> {
    let ghostex = project.path.join(".ghostex");
    fs::create_dir_all(&ghostex)?;
    let gitignore = project.gitignore();
    if !gitignore.exists() {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&gitignore)
        {
            Ok(mut file) => file.write_all(GHOSTEX_FOLDER_GITIGNORE.as_bytes())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let drawings = project.drawings_dir();
    fs::create_dir_all(&drawings)?;
    Ok(drawings)
}

/// Every drawing in the project, most recently edited first.
pub(crate) fn list_drawings(project: &CanvasProject) -> Vec<CanvasDrawing> {
    let Ok(entries) = fs::read_dir(project.drawings_dir()) else {
        return Vec::new();
    };
    let mut drawings: Vec<CanvasDrawing> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            if file.starts_with('.') || !file.to_ascii_lowercase().ends_with(DRAWING_EXTENSION) {
                return None;
            }
            let metadata = entry
                .metadata()
                .ok()
                .filter(|metadata| metadata.is_file())?;
            Some(CanvasDrawing {
                name: drawing_name(&file),
                modified: metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                file,
            })
        })
        .collect();
    drawings.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    drawings
}

/// A new empty drawing called `base` (or `base 2`, `base 3`…), never over an existing file.
fn create_drawing(project: &CanvasProject, base: &str) -> std::io::Result<String> {
    let drawings = ensure_drawings_folder(project)?;
    let content = serde_json::to_string_pretty(&json!({
        "type": "excalidraw",
        "version": 2,
        "source": "https://excalidraw.com",
        "elements": [],
        "appState": {},
        "files": {},
    }))
    .map_err(std::io::Error::other)?;
    for attempt in 1..1000 {
        let file = if attempt == 1 {
            format!("{base}{DRAWING_EXTENSION}")
        } else {
            format!("{base} {attempt}{DRAWING_EXTENSION}")
        };
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(drawings.join(&file))
        {
            Ok(mut handle) => {
                handle.write_all(content.as_bytes())?;
                return Ok(file);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("No free drawing name."))
}

/// A typed drawing name as a file name, or why it can't be one.
pub(crate) fn drawing_file_for_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    let name = name
        .strip_suffix(DRAWING_EXTENSION)
        .map(str::trim)
        .unwrap_or(name);
    if name.is_empty() {
        return Err("Give the drawing a name.");
    }
    if name == "." || name == ".." || name.starts_with('.') {
        return Err("A drawing name can't start with a dot.");
    }
    if name.contains(['/', '\\', ':', '\0']) || name.chars().any(char::is_control) {
        return Err("A drawing name can't contain / \\ or :.");
    }
    Ok(format!("{name}{DRAWING_EXTENSION}"))
}

/// The `.ghostex/.gitignore` line that puts `file` in git.
fn git_include_line(file: &str) -> String {
    // The leading `!` re-includes; `\` keeps `*`, `?`, `[` and `\` in a name literal.
    let escaped: String = file
        .chars()
        .flat_map(|c| match c {
            '*' | '?' | '[' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    format!("!drawings/{escaped}")
}

const GIT_INCLUDE_DIRECTORY_LINE: &str = "!drawings/";
/// While any drawing is in git, the ignore file goes with it, so a clone keeps the rest of the
/// folder out of git too.
const GIT_INCLUDE_IGNORE_FILE_LINE: &str = "!.gitignore";

/// Whether `file` is included in git (its line is in `.ghostex/.gitignore`).
pub(crate) fn drawing_included_in_git(project: &CanvasProject, file: &str) -> bool {
    let line = git_include_line(file);
    fs::read_to_string(project.gitignore())
        .is_ok_and(|text| text.lines().any(|existing| existing.trim_end() == line))
}

/// Adds or removes `file`'s line in `.ghostex/.gitignore`. Nothing else in the project changes.
///
/// CDXC:Canvas 2026-11-27 DECISION:
/// User (Q5281 A, "have include in git part of the menu in the switcher pls"): each drawing's ⋯
/// in the switcher has an "Include in git" check, off by default. On, `.ghostex/.gitignore` gets
/// `!drawings/` (so git looks inside the folder), `!.gitignore` (so a clone keeps the rest of the
/// folder out of git too) and `!drawings/<file>`; `*` still ignores the rest of the folder, so only
/// that drawing and the ignore file show in git status. Off removes its line, and the other two go
/// when no drawing is included.
fn set_drawing_included_in_git(
    project: &CanvasProject,
    file: &str,
    included: bool,
) -> std::io::Result<()> {
    ensure_drawings_folder(project)?;
    let path = project.gitignore();
    let text = fs::read_to_string(&path).unwrap_or_else(|_| GHOSTEX_FOLDER_GITIGNORE.to_string());
    let line = git_include_line(file);
    let mut lines: Vec<String> = text
        .lines()
        .map(str::to_string)
        .filter(|existing| existing.trim_end() != line)
        .collect();
    if included {
        if !lines.iter().any(|existing| existing.trim_end() == "*") {
            lines.push("*".to_string());
        }
        for shared in [GIT_INCLUDE_IGNORE_FILE_LINE, GIT_INCLUDE_DIRECTORY_LINE] {
            if !lines.iter().any(|existing| existing.trim_end() == shared) {
                lines.push(shared.to_string());
            }
        }
        lines.push(line);
    } else if !lines.iter().any(|existing| {
        existing.starts_with("!drawings/") && existing.trim_end() != GIT_INCLUDE_DIRECTORY_LINE
    }) {
        lines.retain(|existing| {
            let existing = existing.trim_end();
            existing != GIT_INCLUDE_DIRECTORY_LINE && existing != GIT_INCLUDE_IGNORE_FILE_LINE
        });
    }
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(&path, text)
}

/// Moves a drawing to the Trash (macOS) or deletes it (elsewhere, like the Files view does).
fn remove_drawing(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let trash = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".Trash"))
            .ok_or_else(|| std::io::Error::other("No Trash folder."))?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = drawing_name(&name);
        for attempt in 1..1000 {
            let target = if attempt == 1 {
                trash.join(&name)
            } else {
                trash.join(format!("{stem} {attempt}{DRAWING_EXTENSION}"))
            };
            if target.exists() {
                continue;
            }
            return fs::rename(path, target);
        }
        Err(std::io::Error::other("The Trash has no room for it."))
    }
    #[cfg(not(target_os = "macos"))]
    {
        fs::remove_file(path)
    }
}

/// Whether Delete can be undone from the Trash here (what the confirm says).
pub(crate) const CANVAS_DELETE_GOES_TO_TRASH: bool = cfg!(target_os = "macos");

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn read_last_drawing(project_id: &str) -> Option<String> {
    match read_record_raw(
        LAST_DRAWING,
        &format!("{LAST_DRAWING_PREFIX}{project_id}"),
        now_ms(),
    ) {
        Ok(RecordRead::Payload(raw)) => Some(raw).filter(|file| !file.is_empty()),
        _ => None,
    }
}

pub(crate) fn write_last_drawing(project_id: &str, file: &str) {
    let _ = write_record(
        LAST_DRAWING,
        &format!("{LAST_DRAWING_PREFIX}{project_id}"),
        file,
        now_ms(),
    );
}

impl GhostexGpuiApp {
    /// The project Canvas draws in, or `None` without a project on this computer.
    pub(crate) fn canvas_project(&self) -> Option<CanvasProject> {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        if snapshot.is_quick_projectless {
            return None;
        }
        let id = snapshot.active_project_id.as_ref()?.0.clone();
        if gpui_remote_project_reference_from_project_id(&id).is_some() {
            return None;
        }
        let path = snapshot.in_memory_project_path.clone()?;
        Some(CanvasProject { id, path })
    }

    /// Whether Canvas can open here (its card is disabled otherwise).
    pub(crate) fn canvas_available(&self) -> bool {
        self.canvas_project().is_some()
    }

    /// The drawing open in this project, by file name.
    pub(crate) fn canvas_open_drawing_file(&self) -> Option<String> {
        let project = self.canvas_project()?;
        self.canvas.open.get(&project.id).cloned()
    }

    /// The tab's label: the open drawing's name, or Canvas before one is open.
    pub(crate) fn canvas_tab_label(&self) -> String {
        self.canvas_open_drawing_file()
            .map(|file| drawing_name(&file))
            .unwrap_or_else(|| "Canvas".to_string())
    }

    /// Picks the drawing to show before the page is made: the one open in this project, else the
    /// one open last time, else the most recently edited, else a new `Untitled` drawing.
    ///
    /// CDXC:Canvas 2026-11-27 DECISION:
    /// User (Q4, reopen last): "opening Canvas shows the drawing you had open last, at the same
    /// spot and zoom". The last drawing is remembered per project here; its spot and zoom are
    /// `appState.scrollX/scrollY/zoom` in the drawing itself, which the editor already saves with
    /// every change and restores on open (`serializeExcalidrawFile` in views/manage/excalidraw-io.ts).
    pub(crate) fn canvas_prepare(&mut self) {
        if !self.active_mode.is_canvas() {
            return;
        }
        let Some(project) = self.canvas_project() else {
            return;
        };
        let drawings = project.drawings_dir();
        if self
            .canvas
            .open
            .get(&project.id)
            .is_some_and(|file| drawings.join(file).is_file())
        {
            return;
        }
        let file = read_last_drawing(&project.id)
            .filter(|file| drawings.join(file).is_file())
            .or_else(|| list_drawings(&project).into_iter().next().map(|d| d.file))
            .or_else(|| create_drawing(&project, "Untitled").ok());
        if let Some(file) = file {
            write_last_drawing(&project.id, &file);
            self.canvas.open.insert(project.id, file);
        }
    }

    /// The Files embed page showing the open drawing (`views/manage/embed.tsx`).
    pub(crate) fn canvas_page_url(&self) -> Option<ProjectWorkareaRealRuntimeUrl> {
        if !crate::app::helpers::web_runtime::web_runtime_available() {
            return None;
        }
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        let project = self.canvas_project()?;
        let file = self.canvas.open.get(&project.id)?;
        let base = manage_workarea_runtime_url_from_project_snapshot(snapshot)?;
        let path = format!("{CANVAS_DRAWINGS_RELATIVE_PATH}/{file}");
        ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(append_url_query_params(
            base.value,
            &[
                ("embed", "1".to_string()),
                ("path", path.clone()),
                ("resourcePath", path),
                ("revision", self.canvas.revision.to_string()),
                ("media", String::new()),
                // Tells this page apart from the Files view's page on the same drawing.
                ("canvas", "1".to_string()),
            ],
        ))
    }

    /// Makes, replaces or hides the page after the drawing changed.
    pub(crate) fn canvas_sync_page(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = app.update(cx, |this, cx| {
                this.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                this.prune_project_workarea_runtime_cef_surfaces_for_current_gates(cx);
                this.update_project_workarea_runtime_cef_surface_visibility(cx);
                cx.notify();
            });
        });
    }

    /// Shows `file` in the view and remembers it for this project.
    pub(crate) fn canvas_show_drawing(&mut self, file: String, cx: &mut Context<Self>) {
        let Some(project) = self.canvas_project() else {
            return;
        };
        write_last_drawing(&project.id, &file);
        self.canvas.open.insert(project.id, file);
        self.canvas_sync_page(cx);
    }

    /// The switcher's New drawing: an empty `Untitled` drawing, shown at once.
    pub(crate) fn canvas_new_drawing(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.canvas_project() else {
            return;
        };
        match create_drawing(&project, "Untitled") {
            Ok(file) => self.canvas_show_drawing(file, cx),
            Err(error) => self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't make a drawing",
                &error.to_string(),
                cx,
            ),
        }
    }

    /// Renames `file` to `name`; an error says why it can't be done.
    pub(crate) fn canvas_rename_drawing(
        &mut self,
        file: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let project = self
            .canvas_project()
            .ok_or_else(|| "Open the project again first.".to_string())?;
        let new_file = drawing_file_for_name(name).map_err(str::to_string)?;
        if new_file == file {
            return Ok(());
        }
        let drawings = project.drawings_dir();
        let case_only = new_file.to_lowercase() == file.to_lowercase();
        if !case_only
            && list_drawings(&project)
                .iter()
                .any(|drawing| drawing.file.to_lowercase() == new_file.to_lowercase())
        {
            return Err("A drawing with that name already exists.".to_string());
        }
        fs::rename(drawings.join(file), drawings.join(&new_file))
            .map_err(|_| "Couldn't rename the drawing.".to_string())?;
        if drawing_included_in_git(&project, file) {
            let _ = set_drawing_included_in_git(&project, file, false);
            let _ = set_drawing_included_in_git(&project, &new_file, true);
        }
        if self.canvas.open.get(&project.id).map(String::as_str) == Some(file) {
            self.canvas.revision += 1;
            self.canvas_show_drawing(new_file, cx);
        } else {
            cx.notify();
        }
        Ok(())
    }

    /// Deletes `file` (to the Trash on macOS); the view moves to the next drawing.
    pub(crate) fn canvas_delete_drawing(
        &mut self,
        file: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let project = self
            .canvas_project()
            .ok_or_else(|| "Open the project again first.".to_string())?;
        remove_drawing(&project.drawings_dir().join(file))
            .map_err(|_| "Couldn't delete the drawing.".to_string())?;
        if drawing_included_in_git(&project, file) {
            let _ = set_drawing_included_in_git(&project, file, false);
        }
        if self.canvas.open.get(&project.id).map(String::as_str) == Some(file) {
            self.canvas.open.remove(&project.id);
            let next = list_drawings(&project)
                .into_iter()
                .next()
                .map(|drawing| drawing.file)
                .or_else(|| create_drawing(&project, "Untitled").ok());
            if let Some(next) = next {
                self.canvas_show_drawing(next, cx);
            }
        } else {
            cx.notify();
        }
        Ok(())
    }

    /// The ⋯ menu's Include in git.
    pub(crate) fn canvas_toggle_git(&mut self, file: &str, cx: &mut Context<Self>) {
        let Some(project) = self.canvas_project() else {
            return;
        };
        let included = drawing_included_in_git(&project, file);
        if let Err(error) = set_drawing_included_in_git(&project, file, !included) {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                "Couldn't change git",
                &error.to_string(),
                cx,
            );
        }
        cx.notify();
    }
}
