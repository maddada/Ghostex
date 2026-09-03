use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Instant,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::Value;

/*
CDXC:Icons 2026-07-29 (discovered icons):
A project's icon should be the icon the PROJECT itself ships — the favicon or app
icon in its repository — not a folder glyph and not only the icon somebody
manually attached in Ghostex. This is the server half of that: gxserver discovers
a representative icon file inside a project's checkout and publishes it as a data
URL on `GxserverPresentationProject.discoveredIconDataUrl`.

Both sidebar versions render this value. Icon discovery therefore follows the
published-project lifecycle itself and must never be gated by which sidebar
layout happens to be selected.

The discovery logic uses a stable candidate list and precedence:

  1. The well-known favicon / app-icon locations, in the fixed order below.
  2. An icon declared by a source file: `<link rel="icon" href="...">` in an HTML
     entry point, or the same shape written as object metadata in a TanStack /
     Remix style root route. The href resolves against `public/` first and then
     against the root.

Everything Ghostex adds on top exists because Ghostex ships the BYTES over a
protocol rather than serving a signed file URL:

- A hard size cap (`MAX_PROJECT_ICON_BYTES`), matching the cap gpui already
  applies to browser-tab favicons, so no presentation snapshot can be inflated by
  a repository that keeps a 4 MB PNG at `assets/logo.png`.
- An extension allowlist, because the data URL carries a MIME type and a file we
  cannot name a type for is not renderable anyway.
- Containment: a candidate must be a regular file that resolves INSIDE the
  project after symlinks, so neither a hostile relative path nor a symlinked
  `favicon.svg` can make the daemon read `~/.ssh/id_rsa` and publish it.

Cost rules mirror `project_git_remote` exactly, because it is the same kind of
work (a bounded filesystem probe feeding a TTL cache that presentation only
READS):

- One entry per unique FAMILY ROOT path, so a registered worktree project
  inherits its parent checkout's icon instead of paying its own probe.
- A background pass on gxserver's own clock, budgeted per pass, with presentation
  never probing — except the first sighting of a newly published project, so the
  delta that announces a project already carries its icon.
- Deltas only when the icon's CONTENT changes, compared by hash rather than by
  re-comparing a ~90 KB data URL string on every pass.
*/

/// How long a discovered icon stays authoritative. Ten minutes, the same as the
/// `origin` probe: a repository's favicon changes about as often as its remote,
/// and the pass otherwise re-reads every project's icon file every minute.
pub const PROJECT_ICON_TTL_MS: i64 = 10 * 60_000;

/// A project with NO discoverable icon is re-checked far less often. The
/// negative answer costs the most work (the whole candidate list plus the source
/// scan) and is the least likely to change, so paying it twice an hour instead of
/// six times an hour is where the pass's budget actually goes.
pub const MISSING_PROJECT_ICON_TTL_MS: i64 = 30 * 60_000;

/// Upper bound on probes in one pass, mirroring the `origin` probe's budget so a
/// machine restored from a large workspace file spreads its first pass over a few
/// minutes; the oldest entries go first.
pub const MAX_PROJECT_ICON_PROBES_PER_PASS: usize = 24;

/*
The icon bytes cap, deliberately the same number gpui applies to browser-tab
favicons (`BROWSER_FAVICON_IMAGE_MAX_BYTES` in `apps/desktop/src/app/consts.rs`): both feed a
16px chrome icon, so a file that is too big to be a tab favicon is too big to be
a project icon. Base64 inflates 64 KiB to ~87 KB of data URL, which stays under
the 96 KB data-URL ceiling that same code enforces.

An oversized candidate is SKIPPED, not fatal: the scan continues to the next
candidate, exactly as it does for a candidate that does not exist. A repository
whose `assets/logo.png` is a 4 MB export still gets its 3 KB `favicon.svg`.
*/
pub const MAX_PROJECT_ICON_BYTES: u64 = 64 * 1024;

/// Cap on an icon-source file (an HTML entry point or a root route module) that
/// is scanned for a `<link rel="icon">`. Generous enough for a real bundled
/// `index.html`, bounded enough that a generated megabyte file is skipped.
pub const MAX_ICON_SOURCE_BYTES: u64 = 512 * 1024;

/*
Well-known favicon paths checked in order. The order is the contract: `favicon.svg` before
`favicon.ico` because a vector icon scales to any chrome size, root before
`public/` before framework-specific app directories.
*/
pub const FAVICON_CANDIDATES: &[&str] = &[
    "favicon.svg",
    "favicon.ico",
    "favicon.png",
    "public/favicon.svg",
    "public/favicon.ico",
    "public/favicon.png",
    "app/favicon.ico",
    "app/favicon.png",
    "app/icon.svg",
    "app/icon.png",
    "app/icon.ico",
    "src/favicon.ico",
    "src/favicon.svg",
    "src/app/favicon.ico",
    "src/app/icon.svg",
    "src/app/icon.png",
    "assets/icon.svg",
    "assets/icon.png",
    "assets/logo.svg",
    "assets/logo.png",
    ".idea/icon.svg",
];

/*
Files that may declare an icon through `<link rel="icon">` or the equivalent
object metadata, in order.
*/
pub const ICON_SOURCE_FILES: &[&str] = &[
    "index.html",
    "public/index.html",
    "app/routes/__root.tsx",
    "src/routes/__root.tsx",
    "app/root.tsx",
    "src/root.tsx",
    "src/index.html",
];

/*
The renderable formats. Ghostex has to name a MIME type inside the data
URL, so a candidate whose extension is not here is not a usable icon.

Every entry renders in the sidebar's Chromium (CEF) surface.
*/
fn icon_mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "ico" => Some("image/x-icon"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct ProjectIcon {
    /// `data:<mime>;base64,<bytes>` — what presentation publishes verbatim.
    pub data_url: String,
    /*
    A hash of the MIME type plus the file bytes: what the refresh pass compares
    to decide whether anything actually changed. Hashing rather than holding two
    ~90 KB strings side by side per project is the point — the comparison is the
    hot path (every project, every pass), the data URL is only the payload.

    Because the MIME type is hashed with the bytes, equal hashes mean equal data
    URLs, so this is a sound stand-in for comparing the published value.
    */
    pub content_hash: u64,
    /// Project-relative path the icon came from, for logs and tests. Never
    /// published: presentation ships bytes, not filesystem layout.
    pub source_relative_path: String,
}

impl PartialEq for ProjectIcon {
    /// Identity IS the published content. Two probes that found the same bytes
    /// through different candidate paths publish the same data URL, so they are
    /// the same icon and must not produce a delta.
    fn eq(&self, other: &Self) -> bool {
        self.content_hash == other.content_hash
    }
}

impl Eq for ProjectIcon {}

/*
The probe surface, injected so the cache's TTL / budget / delta rules are
testable without touching a filesystem — same shape as `ProjectGitRemoteProber`.
*/
pub trait ProjectIconProber {
    /// `None` when the project has no discoverable, usable icon.
    fn probe(&self, path: &str) -> Option<ProjectIcon>;
}

#[derive(Clone, Debug)]
struct ProjectIconEntry {
    probed_at_ms: i64,
    /// `None` is the negative entry: probed, and nothing usable was found.
    icon: Option<ProjectIcon>,
}

#[derive(Default)]
pub struct ProjectIconCache {
    entries: HashMap<String, ProjectIconEntry>,
}

impl ProjectIconCache {
    pub fn get(&self, path: &str) -> Option<ProjectIcon> {
        self.entries.get(path).and_then(|entry| entry.icon.clone())
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Seeds an entry directly. Only the refresh pass, the first-sighting warm,
    /// and tests should use this.
    pub fn set(&mut self, path: &str, icon: Option<ProjectIcon>, monotonic_now_ms: i64) {
        self.entries.insert(
            path.to_string(),
            ProjectIconEntry {
                probed_at_ms: monotonic_now_ms,
                icon,
            },
        );
    }

    /*
    Phase one of a pass: drop paths no published project points at any more, then
    pick the stale ones, oldest first, up to the budget. Paths are copied out so
    the pass can read files with the lock RELEASED — presentation reads this
    cache and must never wait on the filesystem.
    */
    fn plan_refresh(&mut self, paths: &[String], monotonic_now_ms: i64) -> Vec<String> {
        let mut wanted: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for path in paths {
            let path = path.trim();
            if path.is_empty() {
                continue;
            }
            if seen.insert(path) {
                wanted.push(path);
            }
        }
        self.entries.retain(|path, _| seen.contains(path.as_str()));

        let mut stale: Vec<(i64, String)> = wanted
            .into_iter()
            .filter_map(|path| match self.entries.get(path) {
                None => Some((i64::MIN, path.to_string())),
                Some(entry) => {
                    let ttl = if entry.icon.is_some() {
                        PROJECT_ICON_TTL_MS
                    } else {
                        MISSING_PROJECT_ICON_TTL_MS
                    };
                    (monotonic_now_ms - entry.probed_at_ms >= ttl)
                        .then(|| (entry.probed_at_ms, path.to_string()))
                }
            })
            .collect();
        stale.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        stale
            .into_iter()
            .take(MAX_PROJECT_ICON_PROBES_PER_PASS)
            .map(|(_, path)| path)
            .collect()
    }

    /*
    Phase three: fold results back in and report which paths actually CHANGED.
    Comparison is by content hash (see `ProjectIcon::eq`), so re-reading an
    unchanged favicon publishes nothing at all.
    */
    fn apply_refresh(
        &mut self,
        results: Vec<(String, Option<ProjectIcon>)>,
        monotonic_now_ms: i64,
    ) -> Vec<String> {
        let mut changed = Vec::new();
        for (path, icon) in results {
            let previous = self.entries.get(&path).and_then(|entry| entry.icon.clone());
            if previous != icon {
                changed.push(path.clone());
            }
            self.entries.insert(
                path,
                ProjectIconEntry {
                    probed_at_ms: monotonic_now_ms,
                    icon,
                },
            );
        }
        changed
    }
}

/*
One refresh pass over `paths`. The cache lock is taken twice — to plan and to
merge — and never held while the filesystem is read. Returns the paths whose
published icon changed, which is exactly the set the caller turns into project
presentation deltas.
*/
pub fn run_project_icon_refresh_pass(
    cache: &Mutex<ProjectIconCache>,
    paths: &[String],
    prober: &dyn ProjectIconProber,
    monotonic_now_ms: i64,
) -> Vec<String> {
    let targets = {
        let Ok(mut cache) = cache.lock() else {
            return Vec::new();
        };
        cache.plan_refresh(paths, monotonic_now_ms)
    };
    if targets.is_empty() {
        return Vec::new();
    }

    let results = targets
        .into_iter()
        .map(|path| {
            let icon = prober.probe(&path);
            (path, icon)
        })
        .collect::<Vec<_>>();

    let Ok(mut cache) = cache.lock() else {
        return Vec::new();
    };
    cache.apply_refresh(results, monotonic_now_ms)
}

// ---------------------------------------------------------------------------
// process-wide cache
// ---------------------------------------------------------------------------

fn project_icon_cache() -> &'static Mutex<ProjectIconCache> {
    static CACHE: OnceLock<Mutex<ProjectIconCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ProjectIconCache::default()))
}

fn monotonic_now_ms() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as i64
}

/// Runs one pass against the process-wide cache with the real filesystem prober.
/// Blocking: callers must be on a blocking worker, never on a request path.
pub fn refresh_project_icon_cache(paths: &[String]) -> Vec<String> {
    run_project_icon_refresh_pass(
        project_icon_cache(),
        paths,
        &SystemProjectIconProber,
        monotonic_now_ms(),
    )
}

/// Read-only cache lookup. Never probes, so it is safe on the request path.
pub fn cached_project_icon(path: &str) -> Option<ProjectIcon> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    project_icon_cache().lock().ok()?.get(path)
}

/// The published `discoveredIconDataUrl` value for a project path. Two states
/// only: a data URL string, or an absent key (unprobed, or nothing discoverable).
pub fn published_project_icon_data_url(path: &str) -> Option<Value> {
    cached_project_icon(path).map(|icon| Value::String(icon.data_url))
}

/*
The path a project's icon is discovered from, and the cache key everywhere.

It is deliberately THE SAME key the `origin` probe uses: a registered worktree
project resolves to its FAMILY ROOT, so a project and all of its worktrees share
one probe and show one icon. A worktree is a checkout of the same repository —
it ships the same favicon — so probing each one separately
would spend N filesystem scans to reach N identical answers.
*/
pub fn project_icon_key(project: &Value) -> Option<String> {
    crate::project_git_remote::project_git_remote_key(project)
}

/*
First sighting of a project, mirroring `ensure_project_git_remote_probed`: a
brand-new registration would otherwise show a folder glyph until the next
background pass. Probes ONLY when the cache has no entry for the path at all, so
every later delta for the same project is a pure cache read.
*/
pub fn ensure_project_icon_probed(project: &Value) {
    let Some(path) = project_icon_key(project) else {
        return;
    };
    let already_probed = match project_icon_cache().lock() {
        Ok(cache) => cache.contains(&path),
        Err(_) => true,
    };
    if already_probed {
        return;
    }
    // Probed with the lock RELEASED: presentation reads this cache and must
    // never wait on the filesystem. A concurrent first sighting of the same path
    // costs one duplicate probe and nothing else.
    let icon = SystemProjectIconProber.probe(&path);
    if let Ok(mut cache) = project_icon_cache().lock() {
        cache.set(&path, icon, monotonic_now_ms());
    }
}

/*
The warm every project DELTA runs. Gated on PUBLICATION for the same reason the
`origin` warm is: a parked or hidden project is dropped from the refresh pass's
path set and therefore evicted, so probing one would only feed a cache entry the
next pass throws away — while a project that RETURNS to presentation must carry
its icon in the delta that restores it rather than a minute later.
*/
pub fn ensure_published_project_icon_probed(project: &Value) {
    if !crate::presentation::should_include_presentation_project(project) {
        return;
    }
    ensure_project_icon_probed(project);
}

#[cfg(test)]
pub fn set_cached_project_icon_for_test(path: &str, icon: Option<ProjectIcon>) {
    if let Ok(mut cache) = project_icon_cache().lock() {
        cache.set(path, icon, monotonic_now_ms());
    }
}

/// Drops one path's entry exactly as `plan_refresh` does for a project that is
/// no longer published, so a test can reproduce the park→pass→restore sequence
/// without running a pass against the process-wide cache.
#[cfg(test)]
pub fn forget_cached_project_icon_for_test(path: &str) {
    if let Ok(mut cache) = project_icon_cache().lock() {
        cache.entries.remove(path);
    }
}

// ---------------------------------------------------------------------------
// the real prober
// ---------------------------------------------------------------------------

pub struct SystemProjectIconProber;

impl ProjectIconProber for SystemProjectIconProber {
    fn probe(&self, path: &str) -> Option<ProjectIcon> {
        discover_project_icon(Path::new(path))
    }
}

/*
The discovery itself, with the file loaded into a bounded data URL.
*/
pub fn discover_project_icon(root: &Path) -> Option<ProjectIcon> {
    // The root is canonicalized ONCE, and every candidate is measured against
    // that canonical root. Doing it here rather than per candidate also means a
    // project path that no longer exists costs one failed syscall, not thirty.
    let canonical_root = std::fs::canonicalize(root).ok()?;

    for candidate in FAVICON_CANDIDATES {
        if let Some(icon) = load_project_icon_candidate(&canonical_root, candidate) {
            return Some(icon);
        }
    }

    for source_file in ICON_SOURCE_FILES {
        let Some(source) = read_capped_text(&canonical_root, source_file, MAX_ICON_SOURCE_BYTES)
        else {
            continue;
        };
        let Some(href) = extract_icon_href(&source) else {
            continue;
        };
        // Resolve a declared href against `public/` first and then against the
        // root, because the href is a served URL ("/favicon.png")
        // and `public/` is what most frameworks serve from.
        let clean = href.trim_start_matches('/').to_string();
        for candidate in [format!("public/{clean}"), clean.clone()] {
            if let Some(icon) = load_project_icon_candidate(&canonical_root, &candidate) {
                return Some(icon);
            }
        }
    }

    None
}

/*
Resolve one project-relative candidate and load it if it is a usable icon.

`None` means "not this one, keep looking" for every reason: outside the project,
missing, not a regular file, an unrenderable format, or too big. The scan can
therefore treat a hostile path and a typo identically, which is what makes the
precedence list safe to run over untrusted repository content.
*/
fn load_project_icon_candidate(canonical_root: &Path, relative_path: &str) -> Option<ProjectIcon> {
    let absolute = resolve_within_root(canonical_root, relative_path)?;
    let mime = icon_mime_type(&absolute)?;
    let metadata = std::fs::metadata(&absolute).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_PROJECT_ICON_BYTES {
        return None;
    }
    let bytes = std::fs::read(&absolute).ok()?;
    // Re-checked after the read: the size that matters is the size of what we
    // are about to publish, and a file can grow between `metadata` and `read`.
    if bytes.is_empty() || bytes.len() as u64 > MAX_PROJECT_ICON_BYTES {
        return None;
    }

    let mut hasher = DefaultHasher::new();
    mime.hash(&mut hasher);
    bytes.hash(&mut hasher);

    Some(ProjectIcon {
        data_url: format!("data:{mime};base64,{}", BASE64_STANDARD.encode(&bytes)),
        content_hash: hasher.finish(),
        source_relative_path: normalize_relative_display_path(relative_path),
    })
}

/// A capped UTF-8 read of a project-relative source file, used for
/// the icon-source scan. Binary or oversized files read as absent.
fn read_capped_text(canonical_root: &Path, relative_path: &str, max_bytes: u64) -> Option<String> {
    let absolute = resolve_within_root(canonical_root, relative_path)?;
    let metadata = std::fs::metadata(&absolute).ok()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    std::fs::read_to_string(&absolute).ok()
}

/*
Containment. A candidate must be a project-relative path that stays inside the
project, and it must still be inside it AFTER symlinks are resolved.

Both halves are load bearing and neither subsumes the other:
- The lexical half rejects an absolute
  path and any `..` segment before touching the filesystem, so a hostile
  path before it can cause a stat outside the project.
- The canonical half rejects a `favicon.svg` that is a symlink to
  `~/.ssh/id_rsa`, which no amount of lexical checking can see.
*/
fn resolve_within_root(canonical_root: &Path, relative_path: &str) -> Option<PathBuf> {
    let trimmed = relative_path.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }
    // Declared HTML paths are URL-shaped and therefore use forward slashes.
    let candidate = Path::new(&trimmed.replace('\\', "/")).to_path_buf();
    if candidate.is_absolute() {
        return None;
    }
    let mut safe = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            // `..`, a root, or a Windows prefix all mean the path is trying to
            // leave the project; there is no benign spelling of that here.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if safe.as_os_str().is_empty() {
        return None;
    }

    let absolute = canonical_root.join(&safe);
    let canonical = std::fs::canonicalize(&absolute).ok()?;
    canonical.starts_with(canonical_root).then_some(canonical)
}

fn normalize_relative_display_path(relative_path: &str) -> String {
    relative_path.trim().replace('\\', "/")
}

/*
`<link rel="icon" href="...">` in an HTML file, or the same pair written as
object metadata (`{ rel: "icon", href: "..." }`) in a TanStack/Remix root route.

This recognizes the same two shapes without a regex dependency,
scanned, because the crate has no regex dependency and the grammar being matched
is two attribute pairs, not a language. Attribute ORDER must not matter (both
spellings appear in the wild), and a query string is dropped exactly as the
reference does — `?v=2` is cache busting for a server, noise for a file read.
*/
pub fn extract_icon_href(source: &str) -> Option<String> {
    if let Some(href) = extract_html_link_icon_href(source) {
        return Some(href);
    }
    extract_object_link_icon_href(source)
}

fn extract_html_link_icon_href(source: &str) -> Option<String> {
    let lowered = source.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(offset) = lowered[cursor..].find("<link") {
        let start = cursor + offset;
        let end = lowered[start..]
            .find('>')
            .map(|index| start + index)
            .unwrap_or(lowered.len());
        let tag = &source[start..end];
        let rel = extract_attribute_value(tag, "rel", '=');
        if matches!(rel.as_deref(), Some("icon") | Some("shortcut icon")) {
            if let Some(href) = extract_attribute_value(tag, "href", '=') {
                if let Some(cleaned) = clean_icon_href(&href) {
                    return Some(cleaned);
                }
            }
        }
        cursor = end.max(start + 5);
    }
    None
}

fn extract_object_link_icon_href(source: &str) -> Option<String> {
    /*
    The object form is matched inside one brace-free chunk, mirroring the
    reference's `[^}]*` fences: `rel` and `href` have to belong to the SAME
    object literal, or an unrelated `href` further down the file could be paired
    with an icon `rel` above it.
    */
    for chunk in source.split(['{', '}']) {
        let rel = extract_attribute_value(chunk, "rel", ':');
        if !matches!(rel.as_deref(), Some("icon") | Some("shortcut icon")) {
            continue;
        }
        if let Some(href) = extract_attribute_value(chunk, "href", ':') {
            if let Some(cleaned) = clean_icon_href(&href) {
                return Some(cleaned);
            }
        }
    }
    None
}

/// Finds `name<separator>"value"` (or `'value'`) in a chunk, matching the
/// attribute name only as a whole word so `data-href` never answers for `href`.
fn extract_attribute_value(chunk: &str, name: &str, separator: char) -> Option<String> {
    let lowered = chunk.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(offset) = lowered[cursor..].find(name) {
        let start = cursor + offset;
        cursor = start + name.len();
        let preceded_by_word_char = start > 0
            && lowered[..start]
                .chars()
                .next_back()
                .is_some_and(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                });
        if preceded_by_word_char {
            continue;
        }
        let rest = &chunk[cursor..];
        let mut characters = rest
            .char_indices()
            .skip_while(|(_, character)| character.is_whitespace());
        let Some((separator_index, found)) = characters.next() else {
            continue;
        };
        if found != separator {
            continue;
        }
        let after_separator = &rest[separator_index + found.len_utf8()..];
        let value_start = after_separator
            .char_indices()
            .find(|(_, character)| !character.is_whitespace());
        let Some((quote_index, quote)) = value_start else {
            continue;
        };
        if quote != '"' && quote != '\'' {
            continue;
        }
        let value_body = &after_separator[quote_index + quote.len_utf8()..];
        let Some(end) = value_body.find(quote) else {
            continue;
        };
        return Some(value_body[..end].to_string());
    }
    None
}

/// Drops a query string and rejects anything that is not a same-repository file
/// reference (absolute URLs, data URLs, protocol-relative URLs).
fn clean_icon_href(href: &str) -> Option<String> {
    let without_query = href.split(['?', '#']).next().unwrap_or_default().trim();
    if without_query.is_empty() || without_query.starts_with("//") || without_query.contains("://")
    {
        return None;
    }
    if without_query.starts_with("data:") {
        return None;
    }
    Some(without_query.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // -----------------------------------------------------------------------
    // fakes
    // -----------------------------------------------------------------------

    #[derive(Default)]
    struct FakeProber {
        icons: Mutex<HashMap<String, Option<ProjectIcon>>>,
        probes: AtomicUsize,
    }

    impl FakeProber {
        fn set(&self, path: &str, icon: Option<ProjectIcon>) {
            self.icons
                .lock()
                .expect("icons")
                .insert(path.to_string(), icon);
        }
    }

    impl ProjectIconProber for FakeProber {
        fn probe(&self, path: &str) -> Option<ProjectIcon> {
            self.probes.fetch_add(1, Ordering::SeqCst);
            self.icons
                .lock()
                .expect("icons")
                .get(path)
                .cloned()
                .flatten()
        }
    }

    fn icon(marker: u64) -> ProjectIcon {
        ProjectIcon {
            data_url: format!("data:image/png;base64,AAAA{marker}"),
            content_hash: marker,
            source_relative_path: "favicon.png".to_string(),
        }
    }

    fn cache() -> Mutex<ProjectIconCache> {
        Mutex::new(ProjectIconCache::default())
    }

    fn icon_of(cache: &Mutex<ProjectIconCache>, path: &str) -> Option<ProjectIcon> {
        cache.lock().expect("cache").get(path)
    }

    /// A one-pixel PNG, so the tests exercise real bytes and a real base64
    /// payload rather than a text file with a `.png` name.
    const PNG_BYTES: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89,
    ];

    fn write(root: &Path, relative_path: &str, bytes: &[u8]) {
        let target = root.join(relative_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("candidate parent dir");
        }
        std::fs::write(target, bytes).expect("candidate file");
    }

    fn discovered(root: &Path) -> Option<ProjectIcon> {
        discover_project_icon(root)
    }

    // -----------------------------------------------------------------------
    // precedence
    // -----------------------------------------------------------------------

    #[test]
    fn the_well_known_candidates_are_checked_in_the_reference_order() {
        /*
        The order is the contract: a vector root favicon
        outranks `favicon.ico`, root outranks `public/`, and `public/` outranks
        the framework app directories. Asserted by walking the list and removing
        the winner each time, which pins the WHOLE order rather than a sample.
        */
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        for candidate in FAVICON_CANDIDATES {
            write(root, candidate, PNG_BYTES);
        }
        for candidate in FAVICON_CANDIDATES {
            assert_eq!(
                discovered(root)
                    .expect("candidate icon")
                    .source_relative_path,
                *candidate,
                "expected {candidate} to be the next winner"
            );
            std::fs::remove_file(root.join(candidate)).expect("remove candidate");
        }
        assert!(
            discovered(root).is_none(),
            "with every candidate gone the project has no discovered icon"
        );
    }

    #[test]
    fn an_icon_declared_by_a_source_file_is_resolved_against_public_then_the_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(
            root,
            "index.html",
            br#"<html><head><link href="/brand/logo.svg?v=3" rel="icon" /></head></html>"#,
        );
        write(root, "public/brand/logo.svg", b"<svg/>");
        write(root, "brand/logo.svg", b"<svg/>");
        assert_eq!(
            discovered(root)
                .expect("declared icon")
                .source_relative_path,
            "public/brand/logo.svg",
            "the href is a served url, so `public/` is tried first"
        );

        std::fs::remove_file(root.join("public/brand/logo.svg")).expect("remove public copy");
        assert_eq!(
            discovered(root)
                .expect("declared icon")
                .source_relative_path,
            "brand/logo.svg"
        );
    }

    #[test]
    fn the_object_metadata_form_is_recognized_like_the_html_link() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(
            root,
            "src/routes/__root.tsx",
            br#"
            export const Route = createRootRoute({
              head: () => ({
                links: [{ href: "/icon.png", rel: "icon" }],
              }),
            });
            "#,
        );
        write(root, "icon.png", PNG_BYTES);
        assert_eq!(
            discovered(root)
                .expect("declared icon")
                .source_relative_path,
            "icon.png"
        );
    }

    #[test]
    fn href_extraction_matches_the_shapes_the_reference_accepts() {
        assert_eq!(
            extract_icon_href(r#"<link rel="icon" href="/favicon.svg">"#).as_deref(),
            Some("/favicon.svg")
        );
        assert_eq!(
            extract_icon_href(r#"<link href='/favicon.svg' rel='shortcut icon'>"#).as_deref(),
            Some("/favicon.svg"),
            "attribute order and quote style must not matter"
        );
        assert_eq!(
            extract_icon_href(r#"<link rel="icon" href="/favicon.svg?v=9">"#).as_deref(),
            Some("/favicon.svg"),
            "a cache-busting query is not part of the file name"
        );
        assert_eq!(
            extract_icon_href(r#"<link rel="stylesheet" href="/app.css">"#),
            None,
            "only an icon rel counts"
        );
        assert_eq!(
            extract_icon_href(r#"<link rel="icon" href="https://cdn.example/i.png">"#),
            None,
            "a remote icon is not a file in this repository"
        );
        assert_eq!(
            extract_icon_href(r#"<link rel="icon" data-href="/nope.png" href="/yes.png">"#)
                .as_deref(),
            Some("/yes.png"),
            "`data-href` must not answer for `href`"
        );
        assert_eq!(
            extract_icon_href(
                r#"{ rel: "stylesheet", href: "/a.css" } , { rel: "icon", href: "/b.png" }"#
            )
            .as_deref(),
            Some("/b.png"),
            "rel and href must belong to the same object literal"
        );
    }

    // -----------------------------------------------------------------------
    // safety and limits
    // -----------------------------------------------------------------------

    #[cfg(unix)]
    #[test]
    fn a_symlinked_candidate_that_points_outside_the_project_is_refused() {
        /*
        The lexical check cannot see this one: `favicon.png` is a perfectly
        ordinary project-relative path, and only resolving it reveals that it
        leads out of the checkout. Cloning a hostile repository must not make the
        daemon read and broadcast a file from the user's home directory.
        */
        let temp = tempfile::tempdir().expect("tempdir");
        let secret = temp.path().join("secret.png");
        std::fs::write(&secret, PNG_BYTES).expect("secret file");

        let root = temp.path().join("project");
        std::fs::create_dir_all(&root).expect("project dir");
        std::os::unix::fs::symlink(&secret, root.join("favicon.png")).expect("symlink");

        assert!(discovered(&root).is_none());

        // The same symlink INSIDE the project is fine — containment is about
        // where the bytes live, not about symlinks being suspicious.
        let inside = root.join("brand/logo.png");
        std::fs::create_dir_all(inside.parent().expect("brand dir")).expect("brand dir");
        std::fs::write(&inside, PNG_BYTES).expect("inside file");
        std::fs::remove_file(root.join("favicon.png")).expect("remove symlink");
        std::os::unix::fs::symlink(&inside, root.join("favicon.png")).expect("symlink");
        assert!(discovered(&root).is_some());
    }

    #[test]
    fn an_oversized_icon_is_refused_and_the_scan_continues() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let huge = vec![0x61u8; (MAX_PROJECT_ICON_BYTES + 1) as usize];
        write(root, "favicon.svg", &huge);
        write(root, "favicon.png", PNG_BYTES);

        let icon = discovered(root).expect("icon");
        assert_eq!(
            icon.source_relative_path, "favicon.png",
            "the oversized higher-precedence candidate is skipped, not fatal"
        );

        // Exactly at the cap is still accepted: the bound is a maximum, not a
        // strict inequality the caller has to guess at.
        let exact = vec![0x61u8; MAX_PROJECT_ICON_BYTES as usize];
        write(root, "favicon.svg", &exact);
        assert_eq!(
            discovered(root).expect("icon").source_relative_path,
            "favicon.svg"
        );
    }

    #[test]
    fn unrenderable_and_empty_candidates_are_not_icons() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write(root, "favicon.png", b"");
        assert!(
            discovered(root).is_none(),
            "a zero-byte favicon publishes nothing rather than an empty data URL"
        );

        // A DIRECTORY named like a candidate must not be read.
        std::fs::remove_file(root.join("favicon.png")).expect("remove empty file");
        std::fs::create_dir_all(root.join("favicon.png")).expect("candidate dir");
        assert!(discovered(root).is_none());
    }

    #[test]
    fn a_missing_project_directory_probes_as_no_icon() {
        assert!(discover_project_icon(Path::new("/definitely/not/a/real/path")).is_none());
    }

    // -----------------------------------------------------------------------
    // family root
    // -----------------------------------------------------------------------

    #[test]
    fn a_registered_worktree_project_inherits_its_family_root_icon() {
        let plain = json!({ "path": "/repos/ghostex", "projectId": "P100" });
        assert_eq!(project_icon_key(&plain), Some("/repos/ghostex".to_string()));

        let worktree = json!({
            "path": "/repos/ghostex-a1b2c3d4",
            "projectId": "P101",
            "worktree": {
                "branch": "ghostex/a1b2c3d4",
                "parentProjectId": "P100",
                "parentProjectPath": "/repos/ghostex",
            },
        });
        assert_eq!(
            project_icon_key(&worktree),
            Some("/repos/ghostex".to_string()),
            "a worktree shows its parent checkout's icon from the parent's probe"
        );

        // And the published value follows the key, so both rows read the same.
        let key = project_icon_key(&worktree).expect("family root key");
        set_cached_project_icon_for_test(&key, Some(icon(7)));
        assert_eq!(
            published_project_icon_data_url(&key),
            Some(json!("data:image/png;base64,AAAA7"))
        );
        forget_cached_project_icon_for_test(&key);
    }

    // -----------------------------------------------------------------------
    // the published value
    // -----------------------------------------------------------------------

    #[test]
    fn the_published_value_is_a_data_url_or_no_key_at_all() {
        let path = "/tmp/ghostex-project-icon/published";
        assert_eq!(
            published_project_icon_data_url(path),
            None,
            "an unprobed path publishes no key"
        );

        set_cached_project_icon_for_test(path, Some(icon(3)));
        assert_eq!(
            published_project_icon_data_url(path),
            Some(json!("data:image/png;base64,AAAA3"))
        );

        set_cached_project_icon_for_test(path, None);
        assert_eq!(
            published_project_icon_data_url(path),
            None,
            "a probed project with no icon publishes no key either"
        );
        forget_cached_project_icon_for_test(path);
    }

    // -----------------------------------------------------------------------
    // cache behavior
    // -----------------------------------------------------------------------

    #[test]
    fn one_probe_serves_every_project_sharing_a_family_root() {
        let cache = cache();
        let prober = FakeProber::default();
        prober.set("/repos/ghostex", Some(icon(1)));

        let paths = vec![
            "/repos/ghostex".to_string(),
            "/repos/ghostex".to_string(),
            " /repos/ghostex ".to_string(),
        ];
        let changed = run_project_icon_refresh_pass(&cache, &paths, &prober, 0);

        assert_eq!(prober.probes.load(Ordering::SeqCst), 1);
        assert_eq!(changed, vec!["/repos/ghostex".to_string()]);
        assert_eq!(icon_of(&cache, "/repos/ghostex"), Some(icon(1)));
    }

    #[test]
    fn cached_icons_survive_until_their_ttl_and_iconless_projects_last_longer() {
        let cache = cache();
        let prober = FakeProber::default();
        prober.set("/repos/ghostex", Some(icon(1)));
        prober.set("/home/notes", None);
        let paths = vec!["/repos/ghostex".to_string(), "/home/notes".to_string()];

        run_project_icon_refresh_pass(&cache, &paths, &prober, 0);
        assert_eq!(prober.probes.load(Ordering::SeqCst), 2);
        assert!(
            icon_of(&cache, "/home/notes").is_none(),
            "a project with no icon caches as a negative entry"
        );

        run_project_icon_refresh_pass(&cache, &paths, &prober, PROJECT_ICON_TTL_MS - 1);
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            2,
            "nothing is re-probed inside the TTL"
        );

        run_project_icon_refresh_pass(&cache, &paths, &prober, PROJECT_ICON_TTL_MS);
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            3,
            "only the project WITH an icon is due at the ten-minute mark"
        );

        run_project_icon_refresh_pass(&cache, &paths, &prober, MISSING_PROJECT_ICON_TTL_MS);
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            5,
            "at the half-hour mark the iconless project is due as well"
        );
    }

    #[test]
    fn a_pass_reports_only_the_projects_whose_icon_content_changed() {
        /*
        The delta rule, which is why the cache holds a content hash at all: a
        pass that re-reads the same favicon must publish NOTHING, or every
        project on the machine would churn a presentation revision — carrying a
        ~90 KB data URL — once a minute forever.
        */
        let cache = cache();
        let prober = FakeProber::default();
        prober.set("/repos/ghostex", Some(icon(1)));
        prober.set("/repos/quiet", None);
        let paths = vec!["/repos/ghostex".to_string(), "/repos/quiet".to_string()];

        let changed = run_project_icon_refresh_pass(&cache, &paths, &prober, 0);
        assert_eq!(
            changed.len(),
            1,
            "only the project that HAS an icon changed"
        );

        let changed = run_project_icon_refresh_pass(&cache, &paths, &prober, PROJECT_ICON_TTL_MS);
        assert!(
            changed.is_empty(),
            "an unchanged icon publishes no delta: {changed:?}"
        );

        // Same bytes discovered through a different candidate path is the same
        // published icon, so it is still not a change.
        prober.set(
            "/repos/ghostex",
            Some(ProjectIcon {
                source_relative_path: "public/favicon.png".to_string(),
                ..icon(1)
            }),
        );
        let changed =
            run_project_icon_refresh_pass(&cache, &paths, &prober, MISSING_PROJECT_ICON_TTL_MS);
        assert!(
            changed.is_empty(),
            "the published value is the bytes, not where they were found: {changed:?}"
        );

        prober.set("/repos/ghostex", Some(icon(2)));
        let changed =
            run_project_icon_refresh_pass(&cache, &paths, &prober, MISSING_PROJECT_ICON_TTL_MS * 2);
        assert_eq!(
            changed,
            vec!["/repos/ghostex".to_string()],
            "new bytes are a real change, and only that project is re-published"
        );

        prober.set("/repos/ghostex", None);
        let changed =
            run_project_icon_refresh_pass(&cache, &paths, &prober, MISSING_PROJECT_ICON_TTL_MS * 3);
        assert_eq!(
            changed,
            vec!["/repos/ghostex".to_string()],
            "losing the icon flips the key from a data URL to absent"
        );
    }

    #[test]
    fn unregistered_paths_are_dropped_and_the_pass_is_budgeted() {
        let cache = cache();
        let prober = FakeProber::default();
        let mut paths = Vec::new();
        for index in 0..(MAX_PROJECT_ICON_PROBES_PER_PASS + 4) {
            let path = format!("/repos/project-{index:03}");
            prober.set(&path, Some(icon(index as u64 + 1)));
            paths.push(path);
        }

        let changed = run_project_icon_refresh_pass(&cache, &paths, &prober, 0);
        assert_eq!(changed.len(), MAX_PROJECT_ICON_PROBES_PER_PASS);
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            MAX_PROJECT_ICON_PROBES_PER_PASS,
            "one pass never spends more than its budget"
        );

        let changed = run_project_icon_refresh_pass(&cache, &paths, &prober, 1);
        assert_eq!(changed.len(), 4, "the never-probed remainder goes next");

        let survivor = paths[0].clone();
        run_project_icon_refresh_pass(&cache, std::slice::from_ref(&survivor), &prober, 2);
        assert_eq!(cache.lock().expect("cache").len(), 1);
        assert!(icon_of(&cache, &survivor).is_some());
        assert!(icon_of(&cache, &paths[1]).is_none());
    }

    // -----------------------------------------------------------------------
    // end to end through presentation
    // -----------------------------------------------------------------------

    #[test]
    fn a_freshly_registered_project_carries_its_icon_in_the_delta_that_announces_it() {
        /*
        The registration warm, asserted through the real presentation builder:
        a project the user just added must reach Sidebar V2 already carrying its
        repository's icon instead of showing a folder until the next background
        pass.
        */
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        write(&root, "public/favicon.png", PNG_BYTES);

        let paths = crate::paths::get_gxserver_paths(Some(temp.path().join("home")));
        crate::storage::initialize_gxserver_storage(&paths).expect("storage init");
        let db = crate::storage::open_gxserver_database(&paths).expect("open db");
        let repository = crate::domain::DomainRepository::new(&db, "S8a");
        let project = repository
            .create_project(
                json!({ "name": "Repo", "path": root.to_string_lossy() })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let cache_key = project_icon_key(&project).expect("cache key");

        ensure_published_project_icon_probed(&project);
        let delta = crate::presentation::build_presentation_project_delta(
            &repository,
            &project_id,
            "projectAdded",
        )
        .expect("project delta");
        assert_eq!(
            delta
                .get("project")
                .and_then(|project| project.get("discoveredIconDataUrl")),
            Some(&json!(format!(
                "data:image/png;base64,{}",
                BASE64_STANDARD.encode(PNG_BYTES)
            ))),
            "the delta that announces the project must already carry its icon"
        );
        forget_cached_project_icon_for_test(&cache_key);
    }

    #[test]
    fn the_registration_warm_probes_visible_projects_without_a_sidebar_version_gate() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        write(&root, "favicon.png", PNG_BYTES);
        let project = json!({
            "name": "Gated",
            "path": root.to_string_lossy(),
            "projectId": "P-gated-icon",
        });
        let cache_key = project_icon_key(&project).expect("cache key");
        forget_cached_project_icon_for_test(&cache_key);

        ensure_published_project_icon_probed(&project);
        assert!(
            cached_project_icon(&cache_key).is_some(),
            "every visible project warm must discover the icon used by both sidebar versions"
        );
        forget_cached_project_icon_for_test(&cache_key);
    }

    #[test]
    fn a_parked_project_is_never_probed_and_a_restored_one_carries_its_icon() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        write(&root, "favicon.png", PNG_BYTES);

        let paths = crate::paths::get_gxserver_paths(Some(temp.path().join("home")));
        crate::storage::initialize_gxserver_storage(&paths).expect("storage init");
        let db = crate::storage::open_gxserver_database(&paths).expect("open db");
        let repository = crate::domain::DomainRepository::new(&db, "S8b");
        let project = repository
            .create_project(
                json!({ "name": "Repo", "path": root.to_string_lossy() })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let cache_key = project_icon_key(&project).expect("cache key");
        ensure_published_project_icon_probed(&project);
        assert!(cached_project_icon(&cache_key).is_some());

        let parked = repository
            .close_project_to_recent(&project_id)
            .expect("parked");
        forget_cached_project_icon_for_test(&cache_key);
        ensure_published_project_icon_probed(&parked);
        assert!(
            cached_project_icon(&cache_key).is_none(),
            "a parked project must never be probed: the next pass would evict it again"
        );

        let restored = repository
            .restore_recent_project(&project_id)
            .expect("restored");
        ensure_published_project_icon_probed(&restored);
        let delta = crate::presentation::build_presentation_project_delta(
            &repository,
            &project_id,
            "projectUpdated",
        )
        .expect("project delta");
        assert_eq!(
            delta
                .get("project")
                .and_then(|project| project.get("discoveredIconDataUrl")),
            Some(&json!(format!(
                "data:image/png;base64,{}",
                BASE64_STANDARD.encode(PNG_BYTES)
            ))),
            "the delta that restores a parked project carries its icon again"
        );
        forget_cached_project_icon_for_test(&cache_key);
    }
}
