//! Add Project's folder picker, the parts gxserver answers: what each listed folder is (a Git
//! repository, its branch, its main language, already a Ghostex project) and the sidebar's places
//! on this computer (home, disks, the usual project folders, projects other coding apps opened,
//! recent and pinned folders, and the repositories worth suggesting).
//!
//! CDXC:AddProject 2026-11-21 DECISION:
//! User (Q4380, layout B; Q4381): Add Project is a folder picker with a sidebar of places and columns with a preview. The sidebar shows Home, the disks and connected volumes (drive letters on Windows, `/Volumes` on macOS, `/mnt`, `/media` and `/run/media` mounts on Linux), WSL homes on Windows, Desktop, Documents and Downloads, the Default projects folder, the usual project folders under home that exist, folders holding several Git repositories, projects other coding apps opened (never named in the UI), recent and pinned folders, and the connected computers; cloud-synced folders are left out. Each computer answers for its own disks, so a remote computer's picker shows that computer's places.
//! CDXC:AddProject 2026-11-21 WHY:
//! The details ride on the browse call the picker already makes, so they must never make it slow: one directory read per listed folder, no `git` process, a cache keyed by the folder's modified time, a time budget per answer (what is left unchecked arrives plain, with `detailsPartial`), and nothing at all on a network filesystem.
//! SEE-ALSO: server/src/server/project_paths.rs (the browse and inspection), apps/desktop/src/app/window/add_project_modal/ (the picker).
use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

/// How long one browse answer may spend reading folders' details.
const DETAILS_BUDGET: Duration = Duration::from_millis(250);
/// Names read from one folder to find its manifest; a huge folder is not read to the end.
const DETAILS_MAX_NAMES: usize = 400;
const DETAILS_TTL: Duration = Duration::from_secs(30);
const DETAILS_CACHE_MAX: usize = 4_000;
const PLACES_TTL: Duration = Duration::from_secs(120);
const PLACES_SCAN_BUDGET: Duration = Duration::from_millis(600);
const RECENT_MAX: usize = 5;
const PINNED_MAX: usize = 12;
const OTHER_APPS_MAX: usize = 8;
const SUGGESTED_MAX: usize = 24;
/// A folder holding this many Git repositories counts as a project folder.
const REPO_PARENT_MIN: usize = 3;
const REPO_PARENTS_MAX: usize = 6;

/// What one folder is, as the picker shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FolderDetails {
    pub(crate) git: bool,
    /// `.git` is a file: a linked worktree or a submodule, not a repository of its own.
    pub(crate) linked: bool,
    pub(crate) branch: Option<String>,
    pub(crate) language: Option<&'static str>,
}

struct CachedDetails {
    modified: Option<SystemTime>,
    read_at: Instant,
    details: FolderDetails,
}

fn details_cache() -> &'static Mutex<HashMap<PathBuf, CachedDetails>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedDetails>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The branch a repository's HEAD names, or the short commit of a detached HEAD. A `.git` file
/// (a worktree or a submodule) points at the real git folder.
fn read_git_branch(folder: &Path) -> Option<String> {
    let dot_git = folder.join(".git");
    let git_dir = if dot_git.is_file() {
        let text = fs::read_to_string(&dot_git).ok()?;
        let target = text.trim().strip_prefix("gitdir:")?.trim().to_string();
        let target = PathBuf::from(target);
        if target.is_absolute() {
            target
        } else {
            folder.join(target)
        }
    } else {
        dot_git
    };
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        Some(reference) => Some(
            reference
                .trim()
                .strip_prefix("refs/heads/")
                .unwrap_or(reference.trim())
                .to_string(),
        ),
        None if head.len() >= 7 => Some(head[..7].to_string()),
        None => None,
    }
}

/// The main language a folder's manifest names, read from the folder's own file names.
fn language_from_names(names: &[String]) -> Option<&'static str> {
    let has = |name: &str| names.iter().any(|entry| entry == name);
    let has_suffix = |suffix: &str| names.iter().any(|entry| entry.ends_with(suffix));
    if has("Cargo.toml") {
        return Some("Rust");
    }
    if has("go.mod") {
        return Some("Go");
    }
    if has("Package.swift") || has_suffix(".xcodeproj") || has_suffix(".xcworkspace") {
        return Some("Swift");
    }
    if has("pubspec.yaml") {
        return Some("Dart");
    }
    if has("mix.exs") {
        return Some("Elixir");
    }
    if has_suffix(".csproj") || has_suffix(".sln") || has_suffix(".fsproj") {
        return Some("C#");
    }
    if has("build.gradle.kts") || has("settings.gradle.kts") {
        return Some("Kotlin");
    }
    if has("pom.xml") || has("build.gradle") {
        return Some("Java");
    }
    if has("tsconfig.json") || has("deno.json") || has("deno.jsonc") {
        return Some("TypeScript");
    }
    if has("package.json") {
        return Some("JavaScript");
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") || has("Pipfile") {
        return Some("Python");
    }
    if has("Gemfile") {
        return Some("Ruby");
    }
    if has("composer.json") {
        return Some("PHP");
    }
    if has("CMakeLists.txt") {
        return Some("C++");
    }
    None
}

fn read_folder_details(folder: &Path) -> FolderDetails {
    let Ok(dirents) = fs::read_dir(folder) else {
        return FolderDetails::default();
    };
    let names: Vec<String> = dirents
        .flatten()
        .take(DETAILS_MAX_NAMES)
        .map(|dirent| dirent.file_name().to_string_lossy().to_string())
        .collect();
    let git = names.iter().any(|name| name == ".git");
    FolderDetails {
        git,
        linked: git && folder.join(".git").is_file(),
        branch: git.then(|| read_git_branch(folder)).flatten(),
        language: language_from_names(&names),
    }
}

/// A folder's details, from the cache while the folder is unchanged and the entry is fresh.
pub(crate) fn folder_details(folder: &Path) -> FolderDetails {
    let modified = fs::metadata(folder)
        .and_then(|metadata| metadata.modified())
        .ok();
    if let Ok(cache) = details_cache().lock() {
        if let Some(cached) = cache.get(folder) {
            if cached.modified == modified && cached.read_at.elapsed() < DETAILS_TTL {
                return cached.details.clone();
            }
        }
    }
    let details = read_folder_details(folder);
    if let Ok(mut cache) = details_cache().lock() {
        if cache.len() >= DETAILS_CACHE_MAX {
            cache.clear();
        }
        cache.insert(
            folder.to_path_buf(),
            CachedDetails {
                modified,
                read_at: Instant::now(),
                details: details.clone(),
            },
        );
    }
    details
}

/// True for a folder on a network share, where reading every listed folder would be slow.
#[cfg(target_os = "macos")]
pub(crate) fn is_network_filesystem(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return false;
    }
    let name: Vec<u8> = stat
        .f_fstypename
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect();
    matches!(
        name.as_slice(),
        b"smbfs" | b"nfs" | b"afpfs" | b"webdav" | b"cifs" | b"macfuse" | b"osxfuse"
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn is_network_filesystem(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return false;
    }
    // NFS, SMB, CIFS, SMB2, FUSE (sshfs and the like), and WSL's 9p mounts of Windows drives.
    matches!(
        stat.f_type as u64,
        0x6969 | 0x517B | 0xFF53_4D42 | 0xFE53_4D42 | 0x6573_5546 | 0x0102_1997
    )
}

#[cfg(windows)]
pub(crate) fn is_network_filesystem(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    let text = path.to_string_lossy();
    if text.starts_with("\\\\") {
        return !text.to_lowercase().starts_with("\\\\wsl");
    }
    let Some(root) = text.get(..3) else {
        return false;
    };
    let wide: Vec<u16> = std::ffi::OsStr::new(root)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr()) == 4 }
}

/// The registered, visible projects by path, to mark the folders already in Ghostex.
pub(crate) fn registered_project_paths(state: &AppState) -> HashMap<String, String> {
    registered_project_paths_for(&state.paths, state.metadata.server_id.as_str())
}

fn registered_project_paths_for(paths: &GxserverPaths, server_id: &str) -> HashMap<String, String> {
    let Ok(db) = open_gxserver_database(paths) else {
        return HashMap::new();
    };
    let Ok(projects) = DomainRepository::new(&db, server_id).list_projects() else {
        return HashMap::new();
    };
    projects
        .iter()
        .filter(|project| project.get("visibility").and_then(Value::as_str) != Some("hidden"))
        .filter_map(|project| {
            let path = project.get("path").and_then(Value::as_str)?;
            let id = project.get("projectId").and_then(Value::as_str)?;
            Some((trim_path_text(path), id.to_string()))
        })
        .collect()
}

fn trim_path_text(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        path.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Adds `git`, `branch`, `language` and `projectId` to each listed folder, within the time
/// budget; a network folder gets only `projectId`.
pub(crate) fn add_entry_details(result: &mut Value, parent: &Path, state: &AppState) {
    let registered = registered_project_paths(state);
    let network = is_network_filesystem(parent);
    let started = Instant::now();
    let mut partial = network;
    let Some(entries) = result.get_mut("entries").and_then(Value::as_array_mut) else {
        return;
    };
    for entry in entries.iter_mut() {
        let Some(full_path) = entry
            .get("fullPath")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let Some(object) = entry.as_object_mut() else {
            continue;
        };
        if let Some(project_id) = registered.get(&trim_path_text(&full_path)) {
            object.insert("projectId".to_string(), json!(project_id));
        }
        if network {
            continue;
        }
        if started.elapsed() > DETAILS_BUDGET {
            partial = true;
            continue;
        }
        let details = folder_details(Path::new(&full_path));
        if details.git {
            object.insert("git".to_string(), json!(true));
        }
        if let Some(branch) = details.branch {
            object.insert("branch".to_string(), json!(branch));
        }
        if let Some(language) = details.language {
            object.insert("language".to_string(), json!(language));
        }
    }
    if partial {
        if let Some(object) = result.as_object_mut() {
            object.insert("detailsPartial".to_string(), json!(true));
        }
    }
}

/// The repository a folder sits inside, when the folder is not its root: found by walking up to
/// the nearest `.git`, without running git. The home folder and the disk's root never count, so
/// a home folder kept in Git does not make every folder in it "inside a repository".
pub(crate) fn enclosing_repository_root(folder: &Path, home: &Path) -> Option<PathBuf> {
    if is_network_filesystem(folder) || folder.join(".git").exists() {
        return None;
    }
    folder
        .ancestors()
        .skip(1)
        .take(64)
        .take_while(|ancestor| *ancestor != home && ancestor.parent().is_some())
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

// -- places ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct PickerStore {
    #[serde(default)]
    recent: Vec<String>,
    #[serde(default)]
    pinned: Vec<String>,
}

fn store_file(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("project-picker.json")
}

fn read_store(paths: &GxserverPaths) -> PickerStore {
    fs::read_to_string(store_file(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_store(paths: &GxserverPaths, store: &PickerStore) {
    if let Ok(text) = serde_json::to_string_pretty(store) {
        let _ = fs::write(store_file(paths), text);
    }
}

fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// A project was added from this folder: it leads the picker's Recent list.
pub(crate) fn note_recent_location(paths: &GxserverPaths, project_path: &str) {
    if let Ok(mut cache) = places_cache().lock() {
        *cache = None;
    }
    let Some(parent) = Path::new(project_path).parent() else {
        return;
    };
    let parent = path_to_string(parent);
    if parent.is_empty() || parent == "/" || Path::new(&parent) == paths.home_dir {
        return;
    }
    let _guard = store_lock().lock();
    let mut store = read_store(paths);
    store.recent.retain(|existing| existing != &parent);
    store.recent.insert(0, parent);
    store.recent.truncate(RECENT_MAX);
    write_store(paths, &store);
}

fn place(name: impl Into<String>, path: &Path) -> Value {
    json!({ "name": name.into(), "path": path_to_string(path) })
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path_to_string(path))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().map(|metadata| metadata.dev())
}

/// The disks: the startup disk and everything under `/Volumes` on macOS.
#[cfg(target_os = "macos")]
fn volumes(_home: &Path) -> Vec<Value> {
    let mut root_name = None;
    let mut others = Vec::new();
    if let Ok(dirents) = fs::read_dir("/Volumes") {
        for dirent in dirents.flatten() {
            let path = dirent.path();
            let name = dirent.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if fs::canonicalize(&path).ok().as_deref() == Some(Path::new("/")) {
                root_name = Some(name);
                continue;
            }
            if !path.is_dir() {
                continue;
            }
            let kind = if is_network_filesystem(&path) {
                "network"
            } else {
                "external"
            };
            others.push(json!({ "name": name, "path": path_to_string(&path), "kind": kind }));
        }
    }
    others.sort_by_key(|value| value["name"].as_str().unwrap_or_default().to_lowercase());
    let mut out = vec![json!({
        "name": root_name.unwrap_or_else(|| "/".to_string()),
        "path": "/",
        "kind": "startup",
    })];
    out.extend(others);
    out
}

/// The disks: `/`, then what is mounted under `/mnt`, `/media` and `/run/media` (a folder there
/// that is not a mount point is left out). Inside WSL, `/mnt/c` is the Windows drive C:.
#[cfg(all(unix, not(target_os = "macos")))]
fn volumes(home: &Path) -> Vec<Value> {
    let user = folder_name(home);
    let wsl = std::env::var_os("WSL_DISTRO_NAME").is_some()
        || Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists();
    let mut out = vec![json!({ "name": "/", "path": "/", "kind": "startup" })];
    let mut bases = vec![PathBuf::from("/mnt"), PathBuf::from("/media")];
    bases.push(PathBuf::from("/media").join(&user));
    bases.push(PathBuf::from("/run/media").join(&user));
    let mut seen = HashSet::new();
    for base in bases {
        let Some(base_device) = device_of(&base) else {
            continue;
        };
        let Ok(dirents) = fs::read_dir(&base) else {
            continue;
        };
        let mut found: Vec<Value> = Vec::new();
        for dirent in dirents.flatten() {
            let path = dirent.path();
            let name = dirent.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || matches!(name.as_str(), "wsl" | "wslg") || !path.is_dir() {
                continue;
            }
            if device_of(&path) == Some(base_device) || !seen.insert(path.clone()) {
                continue;
            }
            let (label, kind) = if wsl && base == Path::new("/mnt") && name.len() == 1 {
                (format!("{}:", name.to_uppercase()), "drive")
            } else if is_network_filesystem(&path) {
                (name.clone(), "network")
            } else {
                (name.clone(), "external")
            };
            found.push(json!({ "name": label, "path": path_to_string(&path), "kind": kind }));
        }
        found.sort_by_key(|value| value["name"].as_str().unwrap_or_default().to_lowercase());
        out.extend(found);
    }
    out
}

/// The drive letters, each with its volume name, then each WSL distro's home share.
#[cfg(windows)]
fn volumes(_home: &Path) -> Vec<Value> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    let mut out = Vec::new();
    let drives = unsafe { GetLogicalDrives() };
    for index in 0..26u32 {
        if drives & (1 << index) == 0 {
            continue;
        }
        let letter = char::from(b'A' + index as u8);
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
        let mut label = [0u16; 261];
        let read = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        } != 0;
        let label = if read {
            String::from_utf16_lossy(&label[..label.iter().position(|c| *c == 0).unwrap_or(0)])
        } else {
            String::new()
        };
        let (fallback, kind) = match drive_type {
            2 => ("USB Drive", "external"),
            4 => ("Network Drive", "network"),
            5 => ("CD Drive", "external"),
            _ => ("Local Disk", "drive"),
        };
        let label = if label.trim().is_empty() {
            fallback.to_string()
        } else {
            label
        };
        out.push(json!({
            "name": format!("{label} ({letter}:)"),
            "path": format!("{letter}:/"),
            "kind": kind,
        }));
    }
    // Listing `\\wsl.localhost\` can stall while WSL starts, so it gets a second at most.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let names: Vec<String> = fs::read_dir("\\\\wsl.localhost\\")
            .map(|dirents| {
                dirents
                    .flatten()
                    .map(|dirent| dirent.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let _ = sender.send(names);
    });
    if let Ok(names) = receiver.recv_timeout(Duration::from_secs(1)) {
        for name in names {
            out.push(json!({
                "name": format!("{name} (WSL)"),
                "path": format!("\\\\wsl.localhost\\{name}\\home"),
                "kind": "wsl",
            }));
        }
    }
    out
}

const COMMON_PROJECT_FOLDERS: &[&str] = &[
    "dev",
    "Developer",
    "code",
    "Code",
    "projects",
    "Projects",
    "src",
    "repos",
    "workspace",
    "Workspace",
    "git",
    "github",
    "GitHub",
    "work",
    "source/repos",
];

/// Home folders never scanned for repositories: system, media and cloud-synced folders.
const SKIPPED_HOME_FOLDERS: &[&str] = &[
    "Library",
    "Applications",
    "Pictures",
    "Music",
    "Movies",
    "Videos",
    "Public",
    "AppData",
    "node_modules",
    "snap",
    "Dropbox",
    "Google Drive",
    "iCloud Drive",
    "OneDrive",
];

fn is_skipped_home_folder(name: &str) -> bool {
    name.starts_with('.')
        || SKIPPED_HOME_FOLDERS.contains(&name)
        || name.starts_with("OneDrive")
        || name.starts_with("iCloud")
}

fn count_repositories(folder: &Path, started: Instant) -> usize {
    let Ok(dirents) = fs::read_dir(folder) else {
        return 0;
    };
    let mut count = 0;
    for dirent in dirents.flatten().take(300) {
        if started.elapsed() > PLACES_SCAN_BUDGET {
            break;
        }
        if dirent.path().join(".git").exists() {
            count += 1;
        }
    }
    count
}

/// Folders one or two levels under home that hold several Git repositories, beyond the usual names.
fn repository_parents(home: &Path, known: &HashSet<PathBuf>) -> Vec<Value> {
    let started = Instant::now();
    let mut out = Vec::new();
    let Ok(dirents) = fs::read_dir(home) else {
        return out;
    };
    let mut first_level: Vec<PathBuf> = dirents
        .flatten()
        .filter(|dirent| dirent.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|dirent| !is_skipped_home_folder(&dirent.file_name().to_string_lossy()))
        .map(|dirent| dirent.path())
        .collect();
    first_level.sort();
    let mut candidates = first_level.clone();
    for folder in &first_level {
        if started.elapsed() > PLACES_SCAN_BUDGET {
            break;
        }
        if let Ok(children) = fs::read_dir(folder) {
            candidates.extend(
                children
                    .flatten()
                    .take(60)
                    .filter(|dirent| dirent.file_type().is_ok_and(|kind| kind.is_dir()))
                    .filter(|dirent| !dirent.file_name().to_string_lossy().starts_with('.'))
                    .filter(|dirent| !dirent.path().join(".git").exists())
                    .map(|dirent| dirent.path()),
            );
        }
    }
    for candidate in candidates {
        if out.len() >= REPO_PARENTS_MAX || started.elapsed() > PLACES_SCAN_BUDGET {
            break;
        }
        if known.contains(&candidate) || candidate.join(".git").exists() {
            continue;
        }
        if count_repositories(&candidate, started) >= REPO_PARENT_MIN {
            out.push(place(folder_name(&candidate), &candidate));
        }
    }
    out
}

fn read_capped(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > 16 * 1024 * 1024 {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn file_uri_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8()
        .ok()?
        .to_string();
    // `file:///c%3A/Users/...` on Windows.
    let decoded = match decoded.strip_prefix('/') {
        Some(windows) if windows.len() > 2 && windows.as_bytes()[1] == b':' => windows.to_string(),
        _ => decoded,
    };
    Some(decoded)
}

/// The config folders of code editors that keep a `storage.json` of recently opened folders.
fn editor_config_roots(home: &Path) -> Vec<PathBuf> {
    let names = [
        "Code",
        "Code - Insiders",
        "VSCodium",
        "Cursor",
        "Windsurf",
        "Trae",
        "Kiro",
    ];
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"))
    } else {
        home.join(".config")
    };
    names.iter().map(|name| base.join(name)).collect()
}

fn jetbrains_root(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/JetBrains")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"))
            .join("JetBrains")
    } else {
        home.join(".config/JetBrains")
    }
}

/// Every `key="…"` value of a recent-projects XML file.
fn xml_entry_keys(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find("<entry key=\"") {
        rest = &rest[index + 12..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// App storage, never a project: `~/Library`, `~/Applications`, `/Applications`, `AppData`.
fn is_app_storage(path: &Path, home: &Path) -> bool {
    path.starts_with(home.join("Library"))
        || path.starts_with(home.join("Applications"))
        || path.starts_with("/Applications")
        || path.starts_with("/Library")
        || path.components().any(|part| part.as_os_str() == "AppData")
}

/// Folders other coding apps on this computer opened recently, read from their own recent lists:
/// agent CLIs' per-folder settings and code editors' recent folders. Read only.
fn other_app_folders(home: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<String> = Vec::new();
    let agent_settings = read_capped(&home.join(".claude.json"))
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    if let Some(projects) = agent_settings
        .as_ref()
        .and_then(|value| value.get("projects"))
        .and_then(Value::as_object)
    {
        candidates.extend(projects.keys().cloned());
    }
    if let Some(text) = read_capped(&home.join(".codex/config.toml")) {
        for line in text.lines() {
            let line = line.trim();
            let project = line
                .strip_prefix("[projects.\"")
                .and_then(|rest| rest.find("\"]").map(|end| &rest[..end]));
            if let Some(project) = project {
                candidates.push(project.replace("\\\\", "\\"));
            }
        }
    }
    for root in editor_config_roots(home) {
        let Some(text) = read_capped(&root.join("User/globalStorage/storage.json")) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some(workspaces) = value
            .pointer("/profileAssociations/workspaces")
            .and_then(Value::as_object)
        {
            candidates.extend(workspaces.keys().filter_map(|uri| file_uri_path(uri)));
        }
        if let Some(folders) = value
            .pointer("/backupWorkspaces/folders")
            .and_then(Value::as_array)
        {
            candidates.extend(
                folders
                    .iter()
                    .filter_map(|folder| folder.get("folderUri").and_then(Value::as_str))
                    .filter_map(file_uri_path),
            );
        }
    }
    if let Ok(products) = fs::read_dir(jetbrains_root(home)) {
        for product in products.flatten().take(40) {
            let Some(text) = read_capped(&product.path().join("options/recentProjects.xml")) else {
                continue;
            };
            let home_text = path_to_string(home);
            candidates.extend(
                xml_entry_keys(&text)
                    .into_iter()
                    .map(|key| key.replace("$USER_HOME$", &home_text)),
            );
        }
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .map(|candidate| PathBuf::from(trim_path_text(&candidate)))
        .filter(|path| path.is_absolute() && path != home)
        .filter(|path| {
            // Temporary folders and the agent apps' own worktree folders are not projects.
            !path.components().any(|part| {
                let part = part.as_os_str().to_string_lossy();
                part.starts_with('.') || part == "tmp" || part == "T"
            })
        })
        .filter(|path| seen.insert(path.clone()))
        .filter(|path| !is_app_storage(path, home))
        .take(400)
        .filter(|path| path.is_dir())
        // Only folders that look like a project: a repository or a folder with a manifest, not
        // the folders that hold projects or an app's own storage.
        .filter(|path| {
            let details = folder_details(path);
            !details.linked && (details.git || details.language.is_some())
        })
        .collect()
}

#[derive(Clone)]
struct CachedPlaces {
    read_at: Instant,
    value: Value,
}

fn places_cache() -> &'static Mutex<Option<CachedPlaces>> {
    static CACHE: OnceLock<Mutex<Option<CachedPlaces>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn modified_desc(paths: &mut [PathBuf]) {
    paths.sort_by_key(|path| {
        std::cmp::Reverse(
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok(),
        )
    });
}

/// The scanned places (disks, folders, other apps, suggestions), cached for a short while.
fn scanned_places(home: &Path, registered: &HashMap<String, String>) -> Value {
    if let Ok(cache) = places_cache().lock() {
        if let Some(cached) = cache
            .as_ref()
            .filter(|cached| cached.read_at.elapsed() < PLACES_TTL)
        {
            return cached.value.clone();
        }
    }
    let standard: Vec<Value> = ["Desktop", "Documents", "Downloads"]
        .iter()
        .map(|name| home.join(name))
        .filter(|path| path.is_dir())
        .map(|path| place(folder_name(&path), &path))
        .collect();
    let mut known: HashSet<PathBuf> = HashSet::new();
    let mut project_folders: Vec<Value> = Vec::new();
    let mut seen_canonical = HashSet::new();
    for name in COMMON_PROJECT_FOLDERS {
        let path = home.join(name);
        if !path.is_dir() || path.join(".git").exists() {
            continue;
        }
        // `~/Code` and `~/code` are one folder on a case-insensitive disk.
        let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen_canonical.insert(canonical) {
            continue;
        }
        known.insert(path.clone());
        project_folders.push(place(folder_name(&path), &path));
    }
    for name in ["Desktop", "Documents", "Downloads"] {
        known.insert(home.join(name));
    }
    for parent in repository_parents(home, &known) {
        if let Some(path) = parent["path"].as_str() {
            known.insert(PathBuf::from(path));
        }
        project_folders.push(parent);
    }
    let mut others: Vec<PathBuf> = other_app_folders(home)
        .into_iter()
        .filter(|path| !registered.contains_key(&path_to_string(path)))
        .collect();
    modified_desc(&mut others);
    others.truncate(OTHER_APPS_MAX);
    let other_apps: Vec<Value> = others
        .iter()
        .map(|path| place(folder_name(path), path))
        .collect();
    let started = Instant::now();
    let mut suggested: Vec<Value> = Vec::new();
    let mut suggested_paths = HashSet::new();
    let mut push_suggestion = |path: &Path, source: &str, from: Option<&str>| {
        if suggested.len() >= SUGGESTED_MAX
            || started.elapsed() > PLACES_SCAN_BUDGET
            || registered.contains_key(&path_to_string(path))
            || !suggested_paths.insert(path.to_path_buf())
        {
            return;
        }
        let details = folder_details(path);
        // A linked worktree belongs to a repository that is suggested (or added) on its own.
        if !details.git || details.linked {
            return;
        }
        suggested.push(json!({
            "name": folder_name(path),
            "path": path_to_string(path),
            "branch": details.branch,
            "language": details.language,
            "source": source,
            "from": from,
        }));
    };
    for path in &others {
        push_suggestion(path, "otherApps", None);
    }
    for folder in &project_folders {
        let Some(folder_path) = folder["path"].as_str().map(PathBuf::from) else {
            continue;
        };
        let folder_label = folder_name(&folder_path);
        let Ok(dirents) = fs::read_dir(&folder_path) else {
            continue;
        };
        let mut children: Vec<PathBuf> = dirents
            .flatten()
            .take(200)
            .filter(|dirent| dirent.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|dirent| dirent.path())
            .collect();
        modified_desc(&mut children);
        for child in children {
            push_suggestion(&child, "projectFolder", Some(&folder_label));
        }
    }
    let value = json!({
        "volumes": volumes(home),
        "standard": standard,
        "projectFolders": project_folders,
        "otherApps": other_apps,
        "suggested": suggested,
    });
    if let Ok(mut cache) = places_cache().lock() {
        *cache = Some(CachedPlaces {
            read_at: Instant::now(),
            value: value.clone(),
        });
    }
    value
}

fn stored_places(paths: &[String]) -> Vec<Value> {
    paths
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .map(|path| place(folder_name(&path), &path))
        .collect()
}

/// The picker's places on this computer; `pin` / `unpin` change the pinned folders first.
fn project_picker_places(
    paths: &GxserverPaths,
    server_id: &str,
    params: &Map<String, Value>,
) -> std::result::Result<Value, String> {
    let home = paths.home_dir.clone();
    if let Some(pin) = params.get("pin").and_then(Value::as_str) {
        let path = PathBuf::from(expand_user_path(pin.trim(), &home));
        if !path.is_absolute() || !path.is_dir() {
            return Err("Only an existing folder can be pinned.".to_string());
        }
        let text = path_to_string(&path);
        let _guard = store_lock().lock();
        let mut store = read_store(paths);
        store.pinned.retain(|existing| existing != &text);
        store.pinned.push(text);
        if store.pinned.len() > PINNED_MAX {
            store.pinned.remove(0);
        }
        write_store(paths, &store);
    }
    if let Some(unpin) = params.get("unpin").and_then(Value::as_str) {
        let text = trim_path_text(unpin.trim());
        let _guard = store_lock().lock();
        let mut store = read_store(paths);
        store
            .pinned
            .retain(|existing| trim_path_text(existing) != text);
        write_store(paths, &store);
    }
    if params.get("refresh").and_then(Value::as_bool) == Some(true) {
        if let Ok(mut cache) = places_cache().lock() {
            *cache = None;
        }
    }
    let registered = registered_project_paths_for(paths, server_id);
    let mut value = scanned_places(&home, &registered);
    let store = read_store(paths);
    let platform = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    };
    if let Some(object) = value.as_object_mut() {
        object.insert("home".to_string(), json!(path_to_string(&home)));
        object.insert("platform".to_string(), json!(platform));
        object.insert("recent".to_string(), json!(stored_places(&store.recent)));
        object.insert("pinned".to_string(), json!(stored_places(&store.pinned)));
    }
    Ok(value)
}

/// `/api/projectPickerPlaces`, read off the async runtime: the first answer scans the home folder.
pub(crate) async fn handle_project_picker_places_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_project_directory_browse_params(body) {
        Ok(params) => params,
        Err(error) => return project_path_error_response(endpoint_path, request_id, error),
    };
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.to_string();
    let answer =
        tokio::task::spawn_blocking(move || project_picker_places(&paths, &server_id, &params))
            .await
            .unwrap_or_else(|error| Err(format!("Unable to read places: {error}")));
    match answer {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(message) => routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error("badRequest", message, Some(request_id)),
        ),
    }
}
