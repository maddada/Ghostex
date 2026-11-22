/*!
Codex (0.163+) compresses every rollout that has not been written for seven days
into `<rollout>.jsonl.zst` beside it when it starts, removing the plain file; a
later `codex resume` decompresses it back to the original `.jsonl` before
appending. gxserver's readers tail, page and seek plain `.jsonl` files, so a
compressed rollout is offered to them as a plain mirror under
`<gxserver state dir>/codex-rollout-mirror/<real plain path>`, decoded once and
refreshed when the compressed file's mtime changes. Codex's own files are only
read.
*/

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
use ruzstd::decoding::StreamingDecoder;

const COMPRESSED_SUFFIX: &str = ".zst";
static MIRROR_WRITE_LOCK: Mutex<()> = Mutex::new(());
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn is_compressed_rollout(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".jsonl.zst"))
}

fn compressed_sibling(plain: &Path) -> Option<PathBuf> {
    let name = plain.file_name()?.to_str()?;
    name.ends_with(".jsonl")
        .then(|| plain.with_file_name(format!("{name}{COMPRESSED_SUFFIX}")))
}

fn plain_sibling(compressed: &Path) -> Option<PathBuf> {
    let name = compressed.file_name()?.to_str()?;
    Some(compressed.with_file_name(name.strip_suffix(COMPRESSED_SUFFIX)?))
}

fn mirror_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        ghostex_paths::GhostexPaths::resolve()
            .gxserver_state_dir()
            .join("codex-rollout-mirror")
    })
}

/// `/Users/a/.codex/sessions/…/x.jsonl` mirrors to `<root>/Users/a/.codex/sessions/…/x.jsonl`
/// (`C:\…` to `<root>/C/…`), so the `sessions/YYYY/MM/DD` shape the successor and fork
/// scans read from a rollout path survives and `source_rollout_path` can map it back.
fn mirror_path(plain: &Path) -> Option<PathBuf> {
    let mut mirror = mirror_root().to_path_buf();
    for component in plain.components() {
        match component {
            Component::Prefix(prefix) => {
                let drive = prefix.as_os_str().to_str()?.strip_suffix(':')?;
                if drive.len() != 1 {
                    return None;
                }
                mirror.push(drive);
            }
            Component::RootDir => {}
            Component::Normal(part) => mirror.push(part),
            Component::CurDir | Component::ParentDir => return None,
        }
    }
    plain.is_absolute().then_some(mirror)
}

/// The Codex-owned plain rollout path a mirror stands for; any other path is returned as-is.
pub(crate) fn source_rollout_path(path: &Path) -> PathBuf {
    let Ok(relative) = path.strip_prefix(mirror_root()) else {
        return path.to_path_buf();
    };
    #[cfg(windows)]
    {
        let mut components = relative.components();
        if let Some(Component::Normal(drive)) = components.next() {
            let mut source = PathBuf::from(format!("{}:\\", drive.to_string_lossy()));
            source.push(components.as_path());
            return source;
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    {
        Path::new("/").join(relative)
    }
}

/// A mirror whose Codex rollout is plain again (resumed) or gone: readers that hold it must
/// resolve the rollout again instead of serving the frozen copy.
pub(crate) fn is_superseded_mirror(path: &Path) -> bool {
    if !path.starts_with(mirror_root()) {
        return false;
    }
    let source = source_rollout_path(path);
    source.is_file() || !compressed_sibling(&source).is_some_and(|compressed| compressed.is_file())
}

/// CDXC:SessionChat 2026-10-24 WHY:
/// Codex compressed every rollout idle for a week on 2026-10-22, so chats over them read nothing and an account switch started those sessions fresh instead of resuming them. Decompressing in place would race Codex's own compression and resume, so gxserver reads a mirror and leaves Codex's files alone.
/// A readable plain rollout for a Codex `.jsonl` or `.jsonl.zst` path: the plain file when
/// Codex has it, else a mirror of the compressed one. `None` when neither exists.
pub(crate) fn readable_rollout(path: &Path) -> Option<PathBuf> {
    let (plain, compressed) = if is_compressed_rollout(path) {
        (plain_sibling(path)?, path.to_path_buf())
    } else {
        (path.to_path_buf(), compressed_sibling(path)?)
    };
    if plain.is_file() {
        if let Some(stale) = mirror_path(&plain).filter(|mirror| mirror.is_file()) {
            let _ = fs::remove_file(stale);
        }
        return Some(plain);
    }
    if !compressed.is_file() {
        return None;
    }
    let mirror = mirror_compressed_rollout(&plain, &compressed)?;
    note_mirror_read(&mirror);
    Some(mirror)
}

/// Stamps the mirror's last-read time, which [`prune_unread_mirrors`] measures from. The
/// modification time is left alone: it is the freshness mark that matches the compressed source.
/// Skipped when the stamp is less than a day old, so a chat tailing its rollout does not write
/// metadata on every read.
fn note_mirror_read(mirror: &Path) {
    let now = std::time::SystemTime::now();
    let recent = fs::metadata(mirror)
        .and_then(|meta| meta.accessed())
        .ok()
        .and_then(|accessed| now.duration_since(accessed).ok())
        .is_some_and(|age| age < crate::storage_cleanup::DAY);
    if recent {
        return;
    }
    if let Ok(file) = File::options().write(true).open(mirror) {
        let _ = file.set_times(fs::FileTimes::new().set_accessed(now));
    }
}

/// CDXC:ServerDaemon 2026-11-03 DECISION:
/// User chose: a mirror copy nothing has read for 30 days is deleted. It is only a decoded copy of Codex's compressed file, so the next read of that chat decodes it again (a few seconds for the largest rollouts).
const MIRROR_MAX_UNREAD: std::time::Duration =
    std::time::Duration::from_secs(30 * crate::storage_cleanup::DAY.as_secs());

/// Deletes the mirror copies unread for [`MIRROR_MAX_UNREAD`], and decode leftovers a crash or a
/// killed daemon left behind.
pub(crate) fn prune_unread_mirrors(now: std::time::SystemTime) -> crate::storage_cleanup::Tally {
    use crate::storage_cleanup::{prune_tree, Since};
    let _guard = MIRROR_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut tally = prune_tree(mirror_root(), Since::Accessed, MIRROR_MAX_UNREAD, now);
    // `<mirror>.jsonl.tmp-<pid>-<n>` files are written and renamed under the lock held here, so
    // any still present is abandoned; one modified over a day ago surely is.
    tally.add(prune_tree_matching(mirror_root(), now, |name| {
        name.contains(".jsonl.tmp-")
    }));
    tally
}

fn prune_tree_matching(
    dir: &Path,
    now: std::time::SystemTime,
    matches: fn(&str) -> bool,
) -> crate::storage_cleanup::Tally {
    use crate::storage_cleanup::{is_older, remove_file, Since, Tally, DAY};
    let mut tally = Tally::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return tally;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            tally.add(prune_tree_matching(&path, now, matches));
        } else if metadata.is_file()
            && matches(&entry.file_name().to_string_lossy())
            && is_older(&metadata, Since::Modified, DAY, now)
        {
            remove_file(&path, &metadata, &mut tally);
        }
    }
    tally
}

fn mirror_compressed_rollout(plain: &Path, compressed: &Path) -> Option<PathBuf> {
    let mirror = mirror_path(plain)?;
    let source_modified = fs::metadata(compressed)
        .and_then(|meta| meta.modified())
        .ok()?;
    let fresh = |mirror: &Path| {
        fs::metadata(mirror)
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified == source_modified)
    };
    if fresh(&mirror) {
        return Some(mirror);
    }
    let _guard = MIRROR_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if fresh(&mirror) {
        return Some(mirror);
    }
    match write_mirror(compressed, &mirror, source_modified) {
        Ok(()) => Some(mirror),
        Err(error) => {
            eprintln!(
                "gxserver: could not decompress Codex rollout {}: {error}",
                compressed.display()
            );
            None
        }
    }
}

fn write_mirror(
    compressed: &Path,
    mirror: &Path,
    source_modified: std::time::SystemTime,
) -> io::Result<()> {
    let parent = mirror
        .parent()
        .ok_or_else(|| io::Error::other("mirror path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temp = mirror.with_extension(format!(
        "jsonl.tmp-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut output = File::create(&temp)?;
        decode_zstd(BufReader::new(File::open(compressed)?), &mut output)?;
        output.flush()?;
        output.set_modified(source_modified)?;
        drop(output);
        fs::rename(&temp, mirror)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Every frame in order; skippable frames (seek tables) are passed over.
fn decode_zstd(mut input: BufReader<File>, output: &mut impl Write) -> io::Result<()> {
    while !input.fill_buf()?.is_empty() {
        match StreamingDecoder::new(&mut input) {
            Ok(mut decoder) => {
                io::copy(&mut decoder, output)?;
            }
            Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
                length,
                ..
            })) => {
                io::copy(&mut (&mut input).take(u64::from(length)), &mut io::sink())?;
            }
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        }
    }
    Ok(())
}
