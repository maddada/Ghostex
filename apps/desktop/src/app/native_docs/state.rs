//! The native Docs view's state, kept on the app like the native Kanban's so the view can call the
//! app's own file bridge, session routing and toasts directly.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{Entity, FocusHandle, ScrollHandle, Subscription, Task};
use gpui_component::input::{EditorState, InputState};

/// The project a Docs view belongs to. A change of any part reloads the files from scratch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocsProjectKey {
    pub(crate) project_id: String,
    pub(crate) project_path: std::path::PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsLoadState {
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsEntryKind {
    Directory,
    File,
}

/// One row of the bridge's `list` answer (`ProjectDocsFileEntry`).
#[derive(Clone, Debug)]
pub(crate) struct DocsEntry {
    pub(crate) path: String,
    /// The path as the tree names it (`<mount name>/...` for mounted Docs folders).
    pub(crate) display_path: String,
    pub(crate) name: String,
    pub(crate) kind: DocsEntryKind,
    pub(crate) depth: usize,
    pub(crate) size: Option<u64>,
    pub(crate) modified_at: Option<String>,
}

/// How a file opens, decided by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsFileKind {
    Markdown,
    Text,
    Image,
    Html,
    Excalidraw,
}

impl DocsFileKind {
    pub(crate) fn for_path(path: &str) -> Self {
        let extension = path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "md" | "markdown" | "mdx" => Self::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif" => Self::Image,
            "html" | "htm" => Self::Html,
            "excalidraw" => Self::Excalidraw,
            _ => Self::Text,
        }
    }

    /// The gpui-component highlighter language for a text file, from its extension.
    pub(crate) fn editor_language(path: &str) -> &'static str {
        let extension = path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "md" | "markdown" | "mdx" => "markdown",
            "rs" => "rust",
            "ts" | "mts" | "cts" => "typescript",
            "tsx" => "tsx",
            "js" | "mjs" | "cjs" | "jsx" => "javascript",
            "json" | "jsonc" => "json",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "py" => "python",
            "go" => "go",
            "sh" | "bash" | "zsh" => "bash",
            "css" => "css",
            "html" | "htm" => "html",
            "sql" => "sql",
            "zig" => "zig",
            "c" | "h" => "c",
            "cpp" | "cc" | "hpp" => "cpp",
            "java" => "java",
            "rb" => "ruby",
            "swift" => "swift",
            "kt" => "kotlin",
            _ => "text",
        }
    }
}

/// A tree row turned into a name field.
pub(crate) struct DocsRename {
    pub(crate) path: String,
    pub(crate) input: Entity<InputState>,
    pub(crate) _subscription: Subscription,
}

/// How the files list is showing when it is not docked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsTransient {
    /// Held open by hovering the corner button or the edge band.
    Peek,
    /// Opened on purpose on a narrow view, or by Cmd+F.
    Drawer,
}

/// The files list sliding in or out over the document.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DocsSlide {
    pub(crate) opening: bool,
    pub(crate) started: std::time::Instant,
    pub(crate) id: u64,
}

/// A Markdown document's editor mode: Live renders the Markdown around the caret (the Docs
/// page's `initialMode: 'live'`), Source shows it raw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum DocsMarkdownMode {
    #[default]
    Live,
    Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocsDocumentLoad {
    Loading,
    Ready,
    Error(String),
}

/// An open file: its row in Open Files, its editor and its unsaved state.
pub(crate) struct DocsDocument {
    pub(crate) path: String,
    pub(crate) display_path: String,
    pub(crate) name: String,
    pub(crate) kind: DocsFileKind,
    pub(crate) load: DocsDocumentLoad,
    /// The text as last read from or written to disk.
    pub(crate) saved_text: String,
    /// Text read from disk that is waiting for the next draw to get its editor.
    pub(crate) pending_text: Option<String>,
    /// An image file's picture, once read.
    pub(crate) image: Option<std::sync::Arc<gpui::Image>>,
    /// Plain text and HTML source: the code editor.
    pub(crate) editor: Option<Entity<EditorState>>,
    /// Markdown: the live editor.
    pub(crate) live: Option<Entity<zorite_editor::EditorState>>,
    pub(crate) _live_subscription: Option<Subscription>,
    /// The document's scroll position (the live editor does not scroll itself).
    pub(crate) scroll: ScrollHandle,
    /// The file at HEAD, and each line's change against it, for the gutter's git stripe.
    pub(crate) git_base: Option<String>,
    pub(crate) changes: Option<(Vec<super::gutter::LineChange>, Vec<usize>)>,
    pub(crate) dirty: bool,
    pub(crate) saving: bool,
    pub(crate) mode: DocsMarkdownMode,
    pub(crate) size: Option<u64>,
    /// HTML files: the page's annotation tool (Agentation) is on.
    pub(crate) html_annotate: bool,
    /// Bumped by Reload so the browser area loads the file again.
    pub(crate) embed_revision: u64,
    /// The agent session this document was opened from; its notes go back there.
    pub(crate) origin_session: Option<crate::TerminalSessionId>,
    /// An agent reply under review: the session it came from, by title.
    pub(crate) review_session_title: Option<String>,
    /// The file changed on disk while open; Reload shows it (Markdown never reloads on its own).
    pub(crate) external_change: bool,
    /// `path\0modifiedAt\0size` as last seen, for the change poll.
    pub(crate) disk_signature: Option<String>,
    /// "Saved" shows in the header until then.
    pub(crate) saved_flash_until: Option<std::time::Instant>,
    /// The title's tooltip reads "Copied!" until then.
    pub(crate) title_copied_until: Option<std::time::Instant>,
    pub(crate) _editor_subscription: Option<Subscription>,
}

impl DocsDocument {
    pub(crate) fn new(path: String, display_path: String) -> Self {
        let name = display_path
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or(&display_path)
            .to_string();
        Self {
            kind: DocsFileKind::for_path(&path),
            path,
            display_path,
            name,
            load: DocsDocumentLoad::Loading,
            saved_text: String::new(),
            pending_text: None,
            image: None,
            editor: None,
            live: None,
            _live_subscription: None,
            scroll: ScrollHandle::new(),
            git_base: None,
            changes: None,
            dirty: false,
            saving: false,
            mode: DocsMarkdownMode::default(),
            size: None,
            html_annotate: true,
            origin_session: None,
            embed_revision: 0,
            review_session_title: None,
            external_change: false,
            disk_signature: None,
            saved_flash_until: None,
            title_copied_until: None,
            _editor_subscription: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct NativeDocsState {
    pub(crate) project: Option<DocsProjectKey>,
    /// Bumped on every project change; answers for an older project are dropped.
    pub(crate) generation: u64,
    pub(crate) load_state: Option<DocsLoadState>,
    pub(crate) error: Option<String>,
    pub(crate) entries: Vec<DocsEntry>,
    /// Directory paths the user opened in the tree.
    pub(crate) expanded: BTreeSet<String>,
    pub(crate) search: Option<Entity<InputState>>,
    pub(crate) search_query: String,
    pub(crate) search_subscription: Option<Subscription>,
    pub(crate) documents: Vec<DocsDocument>,
    pub(crate) active: Option<String>,
    /// The persisted intent: the files list is pinned (docked on a wide view) or hidden.
    pub(crate) sidebar_pinned: bool,
    /// A drawer or a peek showing the list over the document.
    pub(crate) transient: Option<DocsTransient>,
    /// The panel's slide, running or last run.
    pub(crate) slide: Option<DocsSlide>,
    /// The 10px edge band is disarmed after the list hides until the pointer leaves the band.
    pub(crate) edge_band_armed: bool,
    /// A pending peek open or peek close.
    pub(crate) peek_timer: Option<Task<()>>,
    /// Folders open because of Expand All rather than one by one.
    pub(crate) expand_all: bool,
    /// The open file's row, to scroll to after Reveal open file.
    pub(crate) reveal_request: Option<String>,
    pub(crate) tree_scroll: ScrollHandle,
    /// What the browser area last showed (file, annotate, revision) and whether it was covered.
    #[allow(clippy::type_complexity)]
    pub(crate) browser_area_key: Option<(Option<(String, bool, u64)>, bool)>,
    /// The poll that watches the open file and the files list.
    pub(crate) watch_task: Option<Task<()>>,
    pub(crate) stat_in_flight: bool,
    /// The agent session the next opened file or reply came from (a chat link or Reply by
    /// Annotating); taken by that open.
    pub(crate) pending_origin: Option<crate::TerminalSessionId>,
    /// A file asked for before Docs synced to the current project.
    pub(crate) pending_open: Option<String>,
    /// A tree row being renamed in place.
    pub(crate) rename: Option<DocsRename>,
    /// The project's notes (`.ghostex/manage-annotations.json`), by file path.
    pub(crate) notes: super::annotations::DocsAnnotationsByPath,
    pub(crate) notes_loaded: bool,
    /// `stable_key()` of the notes as last written, so a save is skipped when nothing changed.
    pub(crate) notes_saved_key: String,
    pub(crate) notes_save_timer: Option<Task<()>>,
    /// The editor's note highlights need recomputing (text or notes changed).
    pub(crate) highlights_stale: bool,
    /// The document whose highlights were last computed.
    pub(crate) highlighted_path: Option<String>,
    /// The note being written or edited.
    pub(crate) composer: Option<super::notes::DocsComposer>,
    /// The Annotations list dropdown is open.
    pub(crate) notes_list_open: bool,
    /// Rendered diagrams, formulas, images and code colours, shared by every open document.
    pub(crate) blocks: super::blocks::SharedCache,
    /// The formatting bar's view toggles (the Docs editor's defaults: all on).
    pub(crate) line_numbers: bool,
    pub(crate) git_changes: bool,
    pub(crate) constrain_width: bool,
    /// The formatting bar's open menu, and the table picker's hovered size.
    pub(crate) format_menu: super::format_bar::DocsFormatMenu,
    pub(crate) table_hover: (usize, usize),
    /// Find and Replace, once opened.
    pub(crate) find: Option<super::find::DocsFind>,
    /// The formatting bar is folded to one pill (remembered).
    pub(crate) format_bar_collapsed: bool,
    /// The selection toolbar shows Meo-style formatting buttons instead of the note buttons.
    pub(crate) toolbar_formatting: bool,
    /// The outcome of the last Send, shown on the button for a few seconds.
    pub(crate) send_status: Option<(super::notes::DocsSendStatus, std::time::Instant)>,
    /// Clear all is armed until then; a second click clears.
    pub(crate) clear_armed_until: Option<std::time::Instant>,
    pub(crate) focus: Option<FocusHandle>,
    /// Built from settings once per appearance change rather than every frame.
    pub(crate) palette: Option<super::palette::DocsPalette>,
    /// Window glass and light chrome as last drawn; a change rebuilds the palette.
    pub(crate) appearance_signature: Option<(bool, bool)>,
    /// Unsaved drafts by path, as stored (`docsDrafts`).
    pub(crate) drafts: BTreeMap<String, super::storage::DocsDraft>,
    /// The pending debounced draft write.
    pub(crate) draft_write: Option<Task<()>>,
    /// The floating files list's own window, while it is out (`drawer.rs`).
    pub(crate) drawer: Option<super::drawer::DocsDrawerHost>,
    /// Where the floating list sits, in the main window's content coordinates, as last synced.
    pub(crate) drawer_frame: Option<gpui::Bounds<gpui::Pixels>>,
    /// The drawer's window is being opened on a deferred task.
    pub(crate) drawer_opening: bool,
    /// The Docs view drew, and synced the drawer, since the main window's last frame began.
    pub(crate) drawer_synced: bool,
    /// The task that closes the drawer on a click elsewhere in the main window is running.
    pub(crate) drawer_click_watch: bool,
    /// The docked list's pin and unpin tween, the one the app's panels use (`panel_motion.rs`).
    pub(crate) docked_motion: crate::app::panel_motion::PanelMotion,
    /// Whether the view was narrow when the tween was last sampled: crossing the breakpoint is a
    /// layout change, which docks or floats the list without a tween.
    pub(crate) docked_narrow: Option<bool>,
    /// How many of the bar's buttons (from the front of `DocsBarItem::OVERFLOW_ORDER`) are in its
    /// "⋯" menu for lack of room.
    pub(crate) format_bar_hidden: usize,
    /// The width the bar may use, measured at the last paint (0 until then).
    pub(crate) format_bar_room: std::rc::Rc<std::cell::Cell<f32>>,
    /// The formatting bar's frosted window under glass (`format_bar_window.rs`).
    pub(crate) format_bar_window: super::format_bar_window::DocsFormatBarWindow,
    /// The selection toolbar's and the composer's frosted windows under glass
    /// (`notes_windows.rs`).
    pub(crate) notes_windows: super::notes_windows::DocsNotesWindows,
    /// A field of the floating list to focus once its window draws (the search, a rename).
    pub(crate) drawer_focus: Option<gpui::Entity<gpui_component::input::InputState>>,
}

impl NativeDocsState {
    /// Drops everything that belongs to the previous project. The search box, the focus handle,
    /// the palette and the pinned intent are app-wide and carry over.
    pub(crate) fn reset_for_project(&mut self, project: DocsProjectKey) {
        let generation = self.generation.wrapping_add(1);
        *self = Self {
            project: Some(project),
            generation,
            load_state: Some(DocsLoadState::Loading),
            search: self.search.take(),
            search_subscription: self.search_subscription.take(),
            focus: self.focus.take(),
            palette: self.palette.take(),
            appearance_signature: self.appearance_signature,
            sidebar_pinned: self.sidebar_pinned,
            format_bar_collapsed: self.format_bar_collapsed,
            blocks: self.blocks.clone(),
            line_numbers: self.line_numbers,
            git_changes: self.git_changes,
            constrain_width: self.constrain_width,
            find: self.find.take(),
            edge_band_armed: true,
            tree_scroll: self.tree_scroll.clone(),
            pending_open: self.pending_open.take(),
            pending_origin: self.pending_origin.take(),
            watch_task: self.watch_task.take(),
            drawer: self.drawer.take(),
            format_bar_window: std::mem::take(&mut self.format_bar_window),
            notes_windows: std::mem::take(&mut self.notes_windows),
            drawer_opening: self.drawer_opening,
            ..Self::default()
        };
    }

    pub(crate) fn document(&self, path: &str) -> Option<&DocsDocument> {
        self.documents.iter().find(|document| document.path == path)
    }

    pub(crate) fn document_mut(&mut self, path: &str) -> Option<&mut DocsDocument> {
        self.documents
            .iter_mut()
            .find(|document| document.path == path)
    }

    pub(crate) fn active_document(&self) -> Option<&DocsDocument> {
        self.document(self.active.as_deref()?)
    }
}
