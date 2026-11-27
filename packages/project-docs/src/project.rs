//! The Files view's whole-project scope: the folders it never lists or searches, and the search
//! over the project's files. Desktop (local projects) and gxserver (remote projects) share it so
//! both list and find the same files.

use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// CDXC:Docs 2026-09-27 DECISION:
/// User: the Files view lists the whole project but ignores "folders like node_modules and other cache and build files to keep the file list more performant". `.gitignore` is deliberately not applied: this repo's own `docs/` folder is gitignored and is exactly what Files must keep showing.
pub const SKIPPED_DIRECTORY_NAMES: &[&str] = &[
    ".cache",
    ".git",
    ".ghostex",
    ".gradle",
    ".next",
    ".nuxt",
    ".pytest_cache",
    ".ruff_cache",
    ".svelte-kit",
    ".turbo",
    ".tox",
    ".venv",
    ".vite",
    "DerivedData",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "storybook-static",
    "target",
    "tmp",
    "venv",
    "zig-out",
];

/// Files the list never shows.
pub const SKIPPED_FILE_NAMES: &[&str] = &[".DS_Store"];

pub fn is_skipped_directory(name: &str) -> bool {
    SKIPPED_DIRECTORY_NAMES.contains(&name)
}

pub fn is_skipped_file(name: &str) -> bool {
    SKIPPED_FILE_NAMES.contains(&name)
}

/// The project's own Ghostex folder and the Canvas drawings inside it.
pub const GHOSTEX_FOLDER_NAME: &str = ".ghostex";
pub const DRAWINGS_FOLDER_NAME: &str = "drawings";

/// Whether the Files list and its search leave out `name`, a child of the folder at `parent`
/// (relative to the listed `root`, `/`-separated, "" for the root itself).
///
/// CDXC:Canvas 2026-11-27 DECISION:
/// User (Q5293): "Show drawings". Files on the desktop, the browser build and the phone shows the `.ghostex/drawings` folder, and only that, so drawings can be opened, dragged or attached from Files and the phone; the rest of `.ghostex` stays hidden and the drawings stay out of git (`.ghostex/.gitignore` holds `*`). The root `.ghostex` is listed only while it holds a `drawings` folder, so a project without drawings shows nothing new. A `.ghostex` folder deeper in the project stays hidden.
pub fn is_hidden_entry(root: &Path, parent: &str, name: &str, is_directory: bool) -> bool {
    let parent = parent.trim_matches('/');
    if parent == GHOSTEX_FOLDER_NAME {
        return !(is_directory && name == DRAWINGS_FOLDER_NAME);
    }
    if is_directory && name == GHOSTEX_FOLDER_NAME && parent.is_empty() {
        return !root
            .join(GHOSTEX_FOLDER_NAME)
            .join(DRAWINGS_FOLDER_NAME)
            .is_dir();
    }
    (is_directory && is_skipped_directory(name)) || is_skipped_file(name)
}

pub struct SearchMatch {
    /// Relative to the searched root, `/`-separated.
    pub relative_path: String,
    pub is_directory: bool,
}

pub struct SearchOutcome {
    pub matches: Vec<SearchMatch>,
    /// More than `max_matches` paths matched; only the best are returned.
    pub truncated: bool,
    /// The project has more than `max_scanned` files and folders; the rest were not searched.
    pub incomplete: bool,
}

/// Every file and folder under one root, as last walked.
struct Index {
    built: Instant,
    entries: Vec<(String, bool)>,
    incomplete: bool,
}

static INDEXES: OnceLock<Mutex<HashMap<PathBuf, Arc<Index>>>> = OnceLock::new();
/// How long a walk answers searches before the next search walks again.
const INDEX_AGE: Duration = Duration::from_secs(20);
const MAX_INDEXES: usize = 8;

/// Drops every cached walk, so the next search sees a file just created, renamed or deleted.
pub fn invalidate_indexes() {
    if let Some(indexes) = INDEXES.get() {
        indexes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

/// CDXC:Docs 2026-09-27 WHY:
/// A walk of a large project (this repo holds over 200,000 files and folders, mostly vendored sources) takes about a second, far too slow to repeat on every keystroke of the Files search. One walk per root is kept for a few seconds and every search in that time filters it in memory; mutations drop it (`invalidate_indexes`).
fn index(root: &Path, max_scanned: usize) -> Arc<Index> {
    let indexes = INDEXES.get_or_init(Default::default);
    if let Some(index) = indexes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(root)
        .filter(|index| index.built.elapsed() < INDEX_AGE)
    {
        return index.clone();
    }
    let index = Arc::new(walk(root, max_scanned));
    let mut indexes = indexes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if indexes.len() >= MAX_INDEXES {
        indexes.retain(|_, index| index.built.elapsed() < INDEX_AGE);
        if indexes.len() >= MAX_INDEXES {
            indexes.clear();
        }
    }
    indexes.insert(root.to_path_buf(), index.clone());
    index
}

/// Walks `root` breadth-first, nearest folders first, leaving out what `is_hidden_entry` hides and
/// never following a symlinked folder.
fn walk(root: &Path, max_scanned: usize) -> Index {
    let mut entries = Vec::new();
    let mut queue: VecDeque<(PathBuf, String)> =
        VecDeque::from([(root.to_path_buf(), String::new())]);
    let mut incomplete = false;
    'walk: while let Some((directory, relative_directory)) = queue.pop_front() {
        let Ok(children) = fs::read_dir(&directory) else {
            continue;
        };
        let mut children: Vec<_> = children.flatten().collect();
        children.sort_by_key(|child| child.file_name());
        for child in children {
            if entries.len() >= max_scanned {
                incomplete = true;
                break 'walk;
            }
            let name = child.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = child.file_type() else {
                continue;
            };
            let is_directory = file_type.is_dir();
            if is_hidden_entry(root, &relative_directory, &name, is_directory) {
                continue;
            }
            let relative_path = if relative_directory.is_empty() {
                name
            } else {
                format!("{relative_directory}/{name}")
            };
            if is_directory {
                queue.push_back((child.path(), relative_path.clone()));
            }
            entries.push((relative_path, is_directory));
        }
    }
    Index {
        built: Instant::now(),
        entries,
        incomplete,
    }
}

/// Every file and folder under `root` whose path contains all of the query's words (case
/// insensitive). Matches on the name sort before matches only on the folder path, then nearer and
/// shorter paths first.
pub fn search(root: &Path, query: &str, max_matches: usize, max_scanned: usize) -> SearchOutcome {
    search_with_extensions(root, query, &[], max_matches, max_scanned)
}

/// [`search`] narrowed to files with one of `extensions` (lowercase, without the dot); an empty
/// list keeps every file and folder. With extensions and no query it lists every such file,
/// sorted by path, which the Files list's type filter draws as a tree.
///
/// CDXC:Docs 2026-10-25 DECISION:
/// User: "make this filter instead of search icon and make that icon a button so i can pick html/markdown/excalidraw/image/video so i can find those files faster". With types picked, the files list shows only those files (folders only where they hold one), and typing still narrows by name within them.
pub fn search_with_extensions(
    root: &Path,
    query: &str,
    extensions: &[String],
    max_matches: usize,
    max_scanned: usize,
) -> SearchOutcome {
    let words: Vec<String> = query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() && extensions.is_empty() {
        return SearchOutcome {
            matches: Vec::new(),
            truncated: false,
            incomplete: false,
        };
    }
    let index = index(root, max_scanned);
    let last_word = words.last().cloned().unwrap_or_default();
    let has_extension = |path: &str| {
        let name = path.rsplit('/').next().unwrap_or(path);
        name.rsplit_once('.').is_some_and(|(stem, extension)| {
            !stem.is_empty()
                && extensions
                    .iter()
                    .any(|wanted| wanted.eq_ignore_ascii_case(extension))
        })
    };
    let mut found: Vec<(bool, usize, usize, &(String, bool))> = index
        .entries
        .iter()
        .filter_map(|entry| {
            if !extensions.is_empty() && (entry.1 || !has_extension(&entry.0)) {
                return None;
            }
            let haystack = entry.0.to_lowercase();
            if !words.iter().all(|word| haystack.contains(word.as_str())) {
                return None;
            }
            let name = haystack.rsplit('/').next().unwrap_or_default();
            Some((
                !name.contains(last_word.as_str()),
                entry.0.matches('/').count(),
                entry.0.len(),
                entry,
            ))
        })
        .collect();
    if words.is_empty() {
        found.sort_by(|a, b| a.3 .0.cmp(&b.3 .0));
    } else {
        found.sort_by_key(|(misses_name, depth, length, _)| (*misses_name, *depth, *length));
    }
    let truncated = found.len() > max_matches;
    SearchOutcome {
        matches: found
            .into_iter()
            .take(max_matches)
            .map(|(_, _, _, (relative_path, is_directory))| SearchMatch {
                relative_path: relative_path.clone(),
                is_directory: *is_directory,
            })
            .collect(),
        truncated,
        incomplete: index.incomplete,
    }
}
