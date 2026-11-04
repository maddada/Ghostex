//! The native Docs view's state, kept on the app like the native Kanban's so the view can call the
//! app's own file bridge, session routing and toasts directly.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{Entity, FocusHandle, ScrollHandle, SharedString, Subscription, Task};
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
    /// Plain text and code: the code editor.
    Text,
    Image,
    Html,
    Excalidraw,
    /// Played by the embed page's `<video>`.
    Video,
    /// Played by the embed page's `<audio>`.
    Audio,
    /// Shown by the embedded browser's own PDF viewer in the embed page.
    Pdf,
    /// A picture GPUI cannot decode that the embed page's `<img>` can (AVIF).
    BrowserImage,
    /// A font file, previewed by the embed page.
    Font,
    /// A SQLite database, browsed read-only natively (`sqlite_view.rs`).
    Sqlite,
    /// Opened by the system's own app instead of in Files.
    SystemApp,
    /// A page an agent published in chat (`ghostex show`), loaded straight from the gxserver
    /// that serves it (`published_page.rs`); there is no file behind it.
    Page,
}

/// CDXC:Docs 2026-09-27 DECISION:
/// User: video and audio play in the Files view's embedded browser "to keep it simple", and formats that browser cannot play open in the system app. Ghostex's CEF is the standard build without licensed codecs, so H.264 and AAC (most `.mp4`, `.mov`, `.m4v`, `.m4a`, `.aac`) cannot play there; PDFs wait for a later change ("pdf we'll think about later", since answered below).
///
/// CDXC:Docs 2026-10-17 DECISION:
/// User picked, on the Files view formats page: PDF "browser viewer" (Q222), AVIF "in browser" with HEIC and JPEG XL staying in the system app (Q227), and fonts "preview page" (Q228). PDFs, AVIF pictures and font files show in the Files view's embed page (`apps/desktop/views/manage/embed.tsx`); `.heic`, `.heif`, `.jxl`, `.jp2` and `.apng` keep opening in the system app.
const BROWSER_VIDEO_EXTENSIONS: &[&str] = &["webm", "ogv"];
const BROWSER_AUDIO_EXTENSIONS: &[&str] = &["mp3", "ogg", "oga", "opus", "wav", "flac"];
/// Media the embedded browser cannot decode, and images neither GPUI nor the browser can draw.
const SYSTEM_APP_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "mkv", "wmv", "flv", "3gp", "3g2", "mpeg", "mpg", "m2ts", "mts",
    "vob", "asf", "ogm", "m4a", "aac", "aif", "aiff", "wma", "heic", "heif", "apng", "jp2", "jxl",
];
/// Pictures the embed page shows because GPUI cannot decode them.
pub(super) const BROWSER_IMAGE_EXTENSIONS: &[&str] = &["avif"];
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "woff", "woff2"];
pub(super) const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "jpe", "gif", "webp", "svg", "bmp", "ico", "tif", "tiff",
];
/// Images Files hands to the system app.
pub(super) const SYSTEM_APP_IMAGE_EXTENSIONS: &[&str] = &["heic", "heif", "apng", "jp2", "jxl"];
pub(super) const VIDEO_EXTENSIONS: &[&str] = &[
    "webm", "ogv", "mp4", "m4v", "mov", "avi", "mkv", "wmv", "flv", "3gp", "3g2", "mpeg", "mpg",
    "m2ts", "mts", "vob", "asf", "ogm",
];
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "ogg", "oga", "opus", "wav", "flac", "m4a", "aac", "aif", "aiff", "wma",
];

/// What kind of media a file is, whether or not Files can play it: the "Images / Videos / Audio
/// open in" settings and the files list's icons go by this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsMediaKind {
    Image,
    Video,
    Audio,
}

pub(crate) fn file_extension(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

impl DocsMediaKind {
    pub(crate) fn for_path(path: &str) -> Option<Self> {
        let extension = file_extension(path);
        let extension = extension.as_str();
        if IMAGE_EXTENSIONS.contains(&extension)
            || BROWSER_IMAGE_EXTENSIONS.contains(&extension)
            || SYSTEM_APP_IMAGE_EXTENSIONS.contains(&extension)
        {
            Some(Self::Image)
        } else if VIDEO_EXTENSIONS.contains(&extension) {
            Some(Self::Video)
        } else if AUDIO_EXTENSIONS.contains(&extension) {
            Some(Self::Audio)
        } else {
            None
        }
    }
}

impl DocsFileKind {
    pub(crate) fn for_path(path: &str) -> Self {
        let extension = file_extension(path);
        let extension = extension.as_str();
        match extension {
            "md" | "markdown" | "mdx" => Self::Markdown,
            "html" | "htm" => Self::Html,
            "excalidraw" => Self::Excalidraw,
            _ if IMAGE_EXTENSIONS.contains(&extension) => Self::Image,
            _ if BROWSER_VIDEO_EXTENSIONS.contains(&extension) => Self::Video,
            _ if BROWSER_AUDIO_EXTENSIONS.contains(&extension) => Self::Audio,
            "pdf" => Self::Pdf,
            _ if BROWSER_IMAGE_EXTENSIONS.contains(&extension) => Self::BrowserImage,
            _ if FONT_EXTENSIONS.contains(&extension) => Self::Font,
            _ if ghostex_docs::sqlite::SQLITE_EXTENSIONS.contains(&extension) => Self::Sqlite,
            _ if SYSTEM_APP_EXTENSIONS.contains(&extension) => Self::SystemApp,
            _ => Self::Text,
        }
    }

    /// Drawn by the embed page rather than natively.
    pub(crate) fn uses_browser_area(self) -> bool {
        matches!(
            self,
            Self::Html
                | Self::Excalidraw
                | Self::Video
                | Self::Audio
                | Self::Pdf
                | Self::BrowserImage
                | Self::Font
                | Self::Page
        )
    }

    /// Shown as a picture, a player or a viewer rather than text: the header shows the file's own
    /// icon and an Open in system app button.
    pub(crate) fn is_viewer(self) -> bool {
        matches!(
            self,
            Self::Image
                | Self::Video
                | Self::Audio
                | Self::Pdf
                | Self::BrowserImage
                | Self::Font
                | Self::Sqlite
        )
    }

    pub(crate) fn is_svg(path: &str) -> bool {
        file_extension(path) == "svg"
    }

    /// The gpui-component highlighter language for a text file, from its name or extension.
    pub(crate) fn editor_language(path: &str) -> &'static str {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        match name {
            "Makefile" | "makefile" | "GNUmakefile" => return "make",
            "CMakeLists.txt" => return "cmake",
            "Gemfile" | "Rakefile" => return "ruby",
            ".bashrc" | ".zshrc" | ".profile" | ".bash_profile" | ".envrc" => return "bash",
            // Comment-and-pattern files read best as shell (CDXC:Docs 2026-10-17, grammars.rs).
            ".gitignore" | ".dockerignore" | ".npmignore" | ".prettierignore" | ".eslintignore"
            | ".gitattributes" | ".gitmodules" | ".npmrc" | ".env" => return "bash",
            ".editorconfig" | ".gitconfig" => return "toml",
            ".babelrc" | ".eslintrc" | ".prettierrc" => return "json",
            _ if name.starts_with(".env.") => return "bash",
            _ if name.starts_with("Dockerfile") || name.starts_with("Containerfile") => {
                return "dockerfile";
            }
            _ => {}
        }
        match file_extension(path).as_str() {
            "md" | "markdown" | "mdx" => "markdown",
            "rs" => "rust",
            "ts" | "mts" | "cts" => "typescript",
            "tsx" => "tsx",
            "js" | "mjs" | "cjs" | "jsx" => "javascript",
            "json" | "jsonc" | "json5" | "excalidraw" | "jsonl" | "ndjson" | "ipynb"
            | "geojson" | "webmanifest" => "json",
            "env" | "ignore" => "bash",
            "ini" | "cfg" | "conf" | "properties" => "toml",
            "dockerfile" | "containerfile" => "dockerfile",
            "ps1" | "psm1" | "psd1" => "powershell",
            "tf" | "tfvars" | "hcl" => "hcl",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "py" | "pyi" => "python",
            "go" => "go",
            "sh" | "bash" | "zsh" => "bash",
            "css" | "scss" | "less" => "css",
            "html" | "htm" | "svg" | "xml" | "plist" | "vue" => "html",
            "sql" => "sql",
            "zig" | "zon" => "zig",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "mm" | "m" => "cpp",
            "cs" => "csharp",
            "java" => "java",
            "rb" | "rake" | "gemspec" => "ruby",
            "erb" => "erb",
            "ejs" => "ejs",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "scala" | "sc" => "scala",
            "lua" => "lua",
            "php" => "php",
            "ex" | "exs" => "elixir",
            "graphql" | "gql" => "graphql",
            "proto" => "proto",
            "svelte" => "svelte",
            "astro" => "astro",
            "diff" | "patch" => "diff",
            "mk" => "make",
            "cmake" => "cmake",
            _ => "text",
        }
    }
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
    /// Read, but not something Files can show (binary, too large): the reason, with a button that
    /// opens the file in the system app.
    Unsupported(String),
}

/// Where an open document stands against its file on disk, the way VS Code tracks it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum DocsDiskState {
    #[default]
    InSync,
    /// The file changed on disk while the document had unsaved edits: the disk's text, kept for
    /// Compare and Take theirs. The edits stay until the user picks.
    Conflict(String),
    /// The file is gone from disk; the document keeps its text and Save writes it back.
    Deleted,
}

/// An open file: its row in Open Files, its editor and its unsaved state. Its path and names are
/// shared strings because the open file tabs draw them on every frame.
pub(crate) struct DocsDocument {
    pub(crate) path: SharedString,
    pub(crate) display_path: SharedString,
    pub(crate) name: SharedString,
    pub(crate) kind: DocsFileKind,
    pub(crate) load: DocsDocumentLoad,
    /// The text as last read from or written to disk.
    pub(crate) saved_text: String,
    /// Text read from disk that is waiting for the next draw to get its editor.
    pub(crate) pending_text: Option<String>,
    /// An image file's picture, once read.
    pub(crate) image: Option<std::sync::Arc<gpui::Image>>,
    /// The picture's zoom and position (`picture_zoom.rs`).
    pub(crate) picture: super::picture_zoom::DocsPictureView,
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
    /// The diff of the latest edit against `git_base`, running in the background.
    pub(crate) changes_task: Option<Task<()>>,
    /// Markdown: the live editor's text as of its last change, which the git marks and the note
    /// ranges are in step with (`edit_tracking.rs`).
    pub(crate) tracked_text: Option<String>,
    /// Finds the notes again and prerenders blocks once typing pauses (`edit_tracking.rs`).
    pub(crate) typing_settle: Option<Task<()>>,
    pub(crate) dirty: bool,
    pub(crate) saving: bool,
    /// Saves the browser area's page sent for this file that have not answered yet.
    pub(crate) page_saves_in_flight: u32,
    pub(crate) mode: DocsMarkdownMode,
    pub(crate) size: Option<u64>,
    /// HTML files: the page's annotation tool (Agentation) is on.
    pub(crate) html_annotate: bool,
    /// SVG images: the source is showing in the code editor instead of the picture.
    pub(crate) svg_source: bool,
    /// Bumped by Reload so the browser area loads the file again.
    pub(crate) embed_revision: u64,
    /// The agent session this document was opened from; its notes go back there.
    pub(crate) origin_session: Option<crate::TerminalSessionId>,
    /// A published page (`DocsFileKind::Page`): where it is served from.
    pub(crate) page: Option<super::published_page::DocsPageSource>,
    /// An agent reply under review: the session it came from, by title.
    pub(crate) review_session_title: Option<String>,
    /// The file on disk against this document: in step, changed under unsaved edits, or gone.
    pub(crate) disk: DocsDiskState,
    /// Compare is on: the editor area shows the diff from the file on disk to the unsaved edits.
    pub(crate) disk_compare: bool,
    /// Newer text from disk for a clean code editor, applied in place by the next draw (the
    /// editor needs a window to take it).
    pub(crate) disk_reload: Option<String>,
    /// `path\0modifiedAt\0size` as last seen, for the change poll.
    pub(crate) disk_signature: Option<String>,
    /// "Saved" shows in the header until then.
    pub(crate) saved_flash_until: Option<std::time::Instant>,
    /// The title's tooltip reads "Copied!" until then.
    pub(crate) title_copied_until: Option<std::time::Instant>,
    pub(crate) _editor_subscription: Option<Subscription>,
    /// CSV/TSV and JSON: the header's Table / Tree switch is on (CSV opens with it on).
    pub(crate) data_view_open: bool,
    /// The table or tree, built from the editor's text while the switch is on (`data_views.rs`).
    pub(crate) data_view: Option<super::data_views::DocsDataView>,
    /// Reopened from the last visit (`native_docs_restore_open_files`) rather than opened now, so
    /// a file deleted since then closes quietly (`native_docs_apply_read`).
    pub(crate) restored: bool,
}

impl DocsDocument {
    pub(crate) fn new(path: String, display_path: String) -> Self {
        // An outside file's path is a Windows path with backslashes, so split on both.
        let name = SharedString::new(
            display_path
                .rsplit(['/', '\\'])
                .find(|part| !part.is_empty())
                .unwrap_or(&display_path),
        );
        let data_view_open = super::data_views::opens_as_table(&path);
        Self {
            kind: DocsFileKind::for_path(&path),
            path: path.into(),
            display_path: display_path.into(),
            name,
            load: DocsDocumentLoad::Loading,
            saved_text: String::new(),
            pending_text: None,
            image: None,
            picture: Default::default(),
            editor: None,
            live: None,
            _live_subscription: None,
            scroll: ScrollHandle::new(),
            git_base: None,
            changes: None,
            changes_task: None,
            tracked_text: None,
            typing_settle: None,
            dirty: false,
            saving: false,
            page_saves_in_flight: 0,
            mode: DocsMarkdownMode::default(),
            size: None,
            html_annotate: true,
            svg_source: false,
            origin_session: None,
            page: None,
            embed_revision: 0,
            review_session_title: None,
            disk: DocsDiskState::InSync,
            disk_compare: false,
            disk_reload: None,
            disk_signature: None,
            saved_flash_until: None,
            title_copied_until: None,
            _editor_subscription: None,
            data_view_open,
            data_view: None,
            restored: false,
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
    /// Folders whose children have been listed; the tree loads a folder when it is first opened.
    pub(crate) loaded_folders: BTreeSet<String>,
    /// Each listed folder's last listing `revision`, so an unchanged folder is not redrawn.
    pub(crate) folder_revisions: std::collections::HashMap<String, String>,
    /// The project-wide search's answer for `search_results_query`, drawn instead of the tree
    /// while the search box has text.
    pub(crate) search_results: Option<Vec<DocsEntry>>,
    pub(crate) search_results_query: String,
    /// More files matched than the search returns.
    pub(crate) search_truncated: bool,
    /// The project is larger than the search walks.
    pub(crate) search_incomplete: bool,
    /// The pending debounced search.
    pub(crate) search_task: Option<Task<()>>,
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
    /// A pending peek open or peek close.
    pub(crate) peek_timer: Option<Task<()>>,
    /// Folders open because of Expand All rather than one by one.
    pub(crate) expand_all: bool,
    /// The open file's row, to scroll to after Reveal open file.
    pub(crate) reveal_request: Option<String>,
    pub(crate) tree_scroll: ScrollHandle,
    /// The open file tabs' row (`open_file_tabs.rs`) scrolls sideways; it was last scrolled to the
    /// shown file at that place in the row (`tab_revealed`), so it follows the shown file, and the
    /// shown file's tab moving, without undoing a scroll by hand.
    pub(crate) tab_scroll: ScrollHandle,
    pub(crate) tab_revealed: Option<(String, usize)>,
    /// What the browser area last showed (file, annotate, revision) and whether it was covered.
    #[allow(clippy::type_complexity)]
    pub(crate) browser_area_key: Option<(Option<(SharedString, bool, u64)>, bool)>,
    /// The poll that watches the open file and the files list.
    pub(crate) watch_task: Option<Task<()>>,
    pub(crate) stat_in_flight: bool,
    /// The agent session the next opened file or reply came from (a chat link or Reply by
    /// Annotating); taken by that open.
    pub(crate) pending_origin: Option<crate::TerminalSessionId>,
    /// A file asked for before Docs synced to the current project.
    pub(crate) pending_open: Option<String>,
    /// A published page asked for before Docs synced to the current project.
    pub(crate) pending_page: Option<super::published_page::DocsPendingPage>,
    /// Open File was asked for before Files drew; its next draw shows the search box.
    pub(crate) open_file_prompt: bool,
    /// The Rename item dialog, while open.
    pub(crate) rename_dialog: Option<super::rename_dialog::DocsRenameDialog>,
    /// The row menu last shown (item, kind, where), so Delete can re-open it armed.
    pub(crate) entry_menu: Option<(String, DocsEntryKind, gpui::Point<gpui::Pixels>)>,
    /// The item whose Delete was clicked once; the next click deletes it.
    pub(crate) delete_armed: Option<String>,
    /// The file operation running (action, item), which the menus show and wait for.
    pub(crate) file_operation: Option<(String, String)>,
    /// Folders whose listing is on its way, and the ones that failed with why.
    pub(crate) folders_loading: BTreeSet<String>,
    pub(crate) folder_errors: BTreeMap<String, String>,
    /// The tree row keyboard focus is on (by path), for arrow-key navigation.
    pub(crate) tree_focus: Option<String>,
    /// The tree's and Open Files' keyboard focus.
    pub(crate) tree_focus_handle: Option<FocusHandle>,
    pub(crate) open_files_focus_handle: Option<FocusHandle>,
    /// Rows drawn above the tree's first entry (the loading or error line), as last drawn.
    pub(crate) tree_status_rows: usize,
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
    /// Where that document's notes sit in its text, as last computed with the highlights.
    pub(crate) note_ranges: Vec<super::annotations::DocsAnnotationRange>,
    /// The note being written or edited.
    pub(crate) composer: Option<super::notes::DocsComposer>,
    /// The Annotations list dropdown is open.
    pub(crate) notes_list_open: bool,
    /// Rendered diagrams, formulas, images and code colours, shared by every open document.
    pub(crate) blocks: super::blocks::SharedCache,
    /// The formatting bar's view toggles (the Docs editor's defaults: all on).
    pub(crate) line_numbers: bool,
    pub(crate) git_changes: bool,
    /// Text and code files wrap long lines (remembered; off until the user turns it on).
    pub(crate) word_wrap: bool,
    pub(crate) constrain_width: bool,
    /// The formatting bar's open menu, and the table picker's hovered size.
    pub(crate) format_menu: super::format_bar::DocsFormatMenu,
    pub(crate) table_hover: (usize, usize),
    /// Find and Replace, once opened.
    pub(crate) find: Option<super::find::DocsFind>,
    /// The sidebar shows the Search view (Cmd+Shift+F) instead of the files list.
    pub(crate) search_panel: bool,
    /// The Search view's fields, options and results, once opened (`project_search.rs`).
    pub(crate) project_search: Option<super::project_search::DocsProjectSearch>,
    /// The files list's type filter (`type_filter.rs`): the file types it shows; empty shows all.
    pub(crate) type_filter: std::collections::BTreeSet<super::type_filter::DocsTypeFilter>,
    /// Folders folded while the type filter draws its tree.
    pub(crate) type_filter_collapsed: BTreeSet<String>,
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
    /// The full-window picture viewer, while it is open (`picture_viewer.rs`).
    pub(crate) picture_viewer: Option<gpui::WindowHandle<gpui_component::Root>>,
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
    /// CDXC:Docs 2026-09-27 WHY:
    /// Docs holds one state for the whole app, so drawing it for another project used to drop the files list, the open files and their editors, and coming back loaded them all again behind "Updating files…" and a new HTML page. A project left with Docs as its awake side panel view (CDXC:Workarea 2026-09-26) parks its state here by project id instead, and coming back puts it back as it was.
    pub(crate) parked: std::collections::HashMap<String, NativeDocsState>,
    /// The highest generation handed out, so a new project never reuses a parked one's.
    pub(crate) generations_issued: u64,
    /// Open SQLite databases' browsers, by path (`sqlite_view.rs`).
    pub(crate) sqlite: std::collections::HashMap<String, super::sqlite_view::DocsSqliteView>,
}

impl NativeDocsState {
    /// Starts `project` from nothing and returns the previous project's state, stripped of what
    /// is app-wide (`carry_app_wide_from`).
    pub(crate) fn reset_for_project(&mut self, project: DocsProjectKey) -> Self {
        let generation = self.generations_issued.max(self.generation).wrapping_add(1);
        let mut next = Self {
            project: Some(project),
            generation,
            load_state: Some(DocsLoadState::Loading),
            ..Self::default()
        };
        next.carry_app_wide_from(self);
        next.generations_issued = generation;
        std::mem::replace(self, next)
    }

    /// Puts a parked project's state back, under its own generation so its answers still land,
    /// and returns the previous project's state, stripped of what is app-wide.
    pub(crate) fn restore_parked(&mut self, mut parked: Self) -> Self {
        parked.carry_app_wide_from(self);
        std::mem::replace(self, parked)
    }

    /// Clears what only means something on screen (a peek, an open menu, the drawer's frame) and
    /// what must be asked again (the browser area, a pending stat) before the state is parked.
    pub(crate) fn strip_for_parking(&mut self) {
        self.transient = None;
        self.slide = None;
        self.peek_timer = None;
        self.format_menu = Default::default();
        self.drawer_frame = None;
        self.drawer_synced = false;
        self.drawer_click_watch = false;
        self.drawer_focus = None;
        self.browser_area_key = None;
        self.stat_in_flight = false;
    }

    /// The search box, the focus handle, the palette, the windows, the watch, the pinned intent
    /// and the parked projects belong to the app, not to a project, and move with it.
    fn carry_app_wide_from(&mut self, from: &mut Self) {
        self.search = from.search.take();
        self.search_subscription = from.search_subscription.take();
        self.focus = from.focus.take();
        self.tree_focus_handle = from.tree_focus_handle.take();
        self.open_files_focus_handle = from.open_files_focus_handle.take();
        self.palette = from.palette.take();
        self.appearance_signature = from.appearance_signature;
        self.sidebar_pinned = from.sidebar_pinned;
        self.format_bar_collapsed = from.format_bar_collapsed;
        self.blocks = from.blocks.clone();
        self.line_numbers = from.line_numbers;
        self.git_changes = from.git_changes;
        self.word_wrap = from.word_wrap;
        self.constrain_width = from.constrain_width;
        self.find = from.find.take();
        self.search_panel = from.search_panel;
        self.project_search = from.project_search.take();
        self.type_filter = std::mem::take(&mut from.type_filter);
        self.tree_scroll = from.tree_scroll.clone();
        self.pending_open = from.pending_open.take();
        self.pending_page = from.pending_page.take();
        self.open_file_prompt = std::mem::take(&mut from.open_file_prompt);
        self.pending_origin = from.pending_origin.take();
        self.watch_task = from.watch_task.take();
        self.drawer = from.drawer.take();
        self.format_bar_window = std::mem::take(&mut from.format_bar_window);
        self.notes_windows = std::mem::take(&mut from.notes_windows);
        self.drawer_opening = from.drawer_opening;
        self.parked = std::mem::take(&mut from.parked);
        self.generations_issued = from.generations_issued.max(from.generation);
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
