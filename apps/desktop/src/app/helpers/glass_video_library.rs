//! The Ghostex glass video library: short, heavily compressed looping videos made for the blurred
//! glass, listed in a catalogue hosted next to the app, downloaded only when the user asks, and
//! played like any other glass video once saved as `library:<id>`.
//!
//! CDXC:Theming 2026-09-26 DECISION:
//! User: "I want to be able to download videos that I upload somewhere ... I want these 240p videos for users to be able to use. It should be built into the app. I want the videos to be 5 minutes and looping ... store them somewhere so users can download them if they want and preview them", then approved the library mockup (docs/2026-09-25/glass-video-library) with: a video's dark/light tag is only a hint (nothing is filtered by it), a "New" badge for 7 days after a video's `added` date, the catalogue hosted as GitHub release assets of a separate repo first, and one calm video bundled with the app so the gallery works offline on first launch. Nothing downloads until the user presses Get.
//!
//! CDXC:Theming 2026-09-26 WHY:
//! The glass video pieces (the aerial list, "Choose a file…", the resolver the window reads) already live in the desktop app, so the library sits beside them rather than in gxserver: the settings page asks the app over the same app-modal bridge, and the app resolves `library:<id>` in the same place it resolves `aerial:<id>`.

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const LIBRARY_PREFIX: &str = "library:";

/// Where the catalogue lives: the `library` release of the videos repository, whose assets are the
/// catalogue, each video, its poster and its preview (tooling/glass-videos/publish.ts writes them).
pub(crate) const GLASS_VIDEO_CATALOGUE_URL: &str =
    "https://github.com/maddada/ghostex-glass-videos/releases/download/library/catalogue.json";

/// One video of the catalogue. `video`, `poster` and `preview` are URLs in the hosted catalogue and
/// file names next to `bundled.json` for the video shipped inside the app.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogueVideo {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    /// "dark", "light" or "any": a hint shown on the card, never a filter.
    #[serde(default)]
    pub(crate) tone: Option<String>,
    /// YYYY-MM-DD, for the "New" badge.
    #[serde(default)]
    pub(crate) added: Option<String>,
    #[serde(default)]
    pub(crate) duration_seconds: Option<f64>,
    #[serde(default)]
    pub(crate) size_bytes: Option<u64>,
    #[serde(default)]
    pub(crate) sha256: Option<String>,
    pub(crate) video: String,
    #[serde(default)]
    pub(crate) poster: Option<String>,
    #[serde(default)]
    pub(crate) preview: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct Catalogue {
    #[serde(default)]
    videos: Vec<CatalogueVideo>,
}

/// The hosted catalogue as last read (from the network or the cache), so a download can find its
/// entry without asking the network again.
static LAST_CATALOGUE: Mutex<Vec<CatalogueVideo>> = Mutex::new(Vec::new());

/// Whether the last catalogue read reached the network.
static LAST_ONLINE: AtomicBool = AtomicBool::new(false);

/// Cancel flags of the downloads in flight, by video id.
static DOWNLOADS: Mutex<Option<HashMap<String, Arc<AtomicBool>>>> = Mutex::new(None);

/// Ids name files on disk, so only a plain slug is ever accepted.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn library_dir() -> PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .data_dir
        .join("glass-videos")
}

fn downloaded_video_path(id: &str) -> PathBuf {
    library_dir().join("videos").join(format!("{id}.mp4"))
}

fn cached_poster_path(id: &str) -> PathBuf {
    library_dir().join("posters").join(format!("{id}.jpg"))
}

fn cached_preview_path(id: &str) -> PathBuf {
    library_dir().join("previews").join(format!("{id}.webp"))
}

fn cached_catalogue_path() -> PathBuf {
    library_dir().join("catalogue.json")
}

#[cfg(target_os = "macos")]
fn bundled_dir() -> Option<PathBuf> {
    Some(crate::app::helpers::gpui_app_bundle_resources_dir()?.join("glass-videos"))
}

#[cfg(not(target_os = "macos"))]
fn bundled_dir() -> Option<PathBuf> {
    None
}

/// The videos shipped inside the app, with their files resolved to absolute paths.
fn bundled_videos() -> Vec<CatalogueVideo> {
    let Some(dir) = bundled_dir() else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(dir.join("bundled.json")) else {
        return Vec::new();
    };
    let Ok(catalogue) = serde_json::from_str::<Catalogue>(&text) else {
        return Vec::new();
    };
    let absolute = |name: &str| dir.join(name).to_string_lossy().into_owned();
    catalogue
        .videos
        .into_iter()
        .filter(|video| valid_id(&video.id))
        .map(|mut video| {
            video.video = absolute(&video.video);
            video.poster = video.poster.as_deref().map(absolute);
            video.preview = video.preview.as_deref().map(absolute);
            video
        })
        .collect()
}

/// The file a saved `library:<id>` plays: the bundled video, or the downloaded one. A video that is
/// not on this computer resolves to nothing, so the glass shows the live blur and Settings shows
/// the slot as empty rather than playing something else.
pub(crate) fn resolve_library_video(saved: &str) -> Option<PathBuf> {
    let id = saved.strip_prefix(LIBRARY_PREFIX)?;
    if !valid_id(id) {
        return None;
    }
    if let Some(bundled) = bundled_videos().into_iter().find(|video| video.id == id) {
        let path = PathBuf::from(bundled.video);
        return path.is_file().then_some(path);
    }
    let path = downloaded_video_path(id);
    path.is_file().then_some(path)
}

fn http_agent(timeout: Duration) -> ureq::Agent {
    let tls_config = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .tls_config(tls_config)
        .build();
    ureq::Agent::new_with_config(config)
}

/// Reads the hosted catalogue, falling back to the copy saved at the last successful read when the
/// network does not answer. Returns the videos and whether the network answered.
pub(crate) fn fetch_catalogue() -> (Vec<CatalogueVideo>, bool) {
    let cached = || fs::read_to_string(cached_catalogue_path()).unwrap_or_default();
    let (text, online) = match http_agent(Duration::from_secs(10))
        .get(GLASS_VIDEO_CATALOGUE_URL)
        .call()
    {
        Ok(mut response) => match response.body_mut().read_to_string() {
            Ok(text) if serde_json::from_str::<Catalogue>(&text).is_ok() => {
                let path = cached_catalogue_path();
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&path, &text);
                (text, true)
            }
            _ => (cached(), true),
        },
        // The host answered, so this computer is online; the catalogue is just not there (not
        // published yet), and whatever was saved before still lists.
        Err(ureq::Error::StatusCode(_)) => (cached(), true),
        Err(_) => (cached(), false),
    };
    let videos: Vec<CatalogueVideo> = serde_json::from_str::<Catalogue>(&text)
        .map(|catalogue| catalogue.videos)
        .unwrap_or_default()
        .into_iter()
        .filter(|video| valid_id(&video.id))
        .collect();
    if let Ok(mut last) = LAST_CATALOGUE.lock() {
        *last = videos.clone();
    }
    LAST_ONLINE.store(online, Ordering::Relaxed);
    (videos, online)
}

/// The hosted catalogue as last read, without asking the network.
pub(crate) fn last_catalogue() -> Vec<CatalogueVideo> {
    LAST_CATALOGUE
        .lock()
        .map(|last| last.clone())
        .unwrap_or_default()
}

/// Whether the last catalogue read reached the network.
pub(crate) fn last_online() -> bool {
    LAST_ONLINE.load(Ordering::Relaxed)
}

fn data_url(path: &Path) -> Option<String> {
    let mime = match path.extension()?.to_str()? {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => return None,
    };
    let bytes = fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn directory_bytes(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.metadata().ok())
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len())
                .sum()
        })
        .unwrap_or(0)
}

/// What Settings shows: the bundled videos first, then the hosted catalogue, each with its state,
/// and the space downloads take. Pictures of videos on this computer travel as data URLs because
/// the settings page cannot read local files; the others are their hosted URLs.
pub(crate) fn library_listing(catalogue: &[CatalogueVideo], online: bool) -> serde_json::Value {
    let bundled = bundled_videos();
    let mut entries = Vec::new();
    for video in &bundled {
        entries.push(serde_json::json!({
            "id": video.id,
            "name": video.name,
            "tags": video.tags,
            "tone": video.tone,
            "added": video.added,
            "durationSeconds": video.duration_seconds,
            "sizeBytes": video.size_bytes,
            "state": "bundled",
            "poster": video.poster.as_deref().map(Path::new).and_then(data_url),
            "preview": video.preview.as_deref().map(Path::new).and_then(data_url),
        }));
    }
    for video in catalogue {
        if bundled.iter().any(|bundled| bundled.id == video.id) {
            continue;
        }
        let downloaded = downloaded_video_path(&video.id).is_file();
        let local_or_remote = |cached: PathBuf, remote: &Option<String>| {
            data_url(&cached).or_else(|| remote.clone().filter(|_| online))
        };
        entries.push(serde_json::json!({
            "id": video.id,
            "name": video.name,
            "tags": video.tags,
            "tone": video.tone,
            "added": video.added,
            "durationSeconds": video.duration_seconds,
            "sizeBytes": video.size_bytes,
            "state": if downloaded { "downloaded" } else { "available" },
            "poster": local_or_remote(cached_poster_path(&video.id), &video.poster),
            "preview": local_or_remote(cached_preview_path(&video.id), &video.preview),
        }));
    }
    let dir = library_dir();
    let storage_bytes = ["videos", "posters", "previews"]
        .iter()
        .map(|name| directory_bytes(&dir.join(name)))
        .sum::<u64>();
    serde_json::json!({
        "type": "glassVideoLibraryListed",
        "online": online,
        "storageBytes": storage_bytes,
        "videos": entries,
    })
}

/// Registers a download of `id`, or returns `None` when one is already running.
pub(crate) fn begin_download(id: &str) -> Option<Arc<AtomicBool>> {
    let mut downloads = DOWNLOADS.lock().ok()?;
    let downloads = downloads.get_or_insert_with(HashMap::new);
    if downloads.contains_key(id) {
        return None;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    downloads.insert(id.to_string(), cancel.clone());
    Some(cancel)
}

pub(crate) fn end_download(id: &str) {
    if let Ok(mut downloads) = DOWNLOADS.lock()
        && let Some(downloads) = downloads.as_mut()
    {
        downloads.remove(id);
    }
}

pub(crate) fn cancel_download(id: &str) {
    if let Ok(downloads) = DOWNLOADS.lock()
        && let Some(cancel) = downloads.as_ref().and_then(|downloads| downloads.get(id))
    {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// Downloads `video` into the library, checking its size and SHA-256 before it replaces anything,
/// then keeps its poster and preview for offline use. `progress` gets the bytes received and the
/// expected total.
pub(crate) fn download_video(
    video: &CatalogueVideo,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    if !valid_id(&video.id) {
        return Err("This video's name is not valid.".to_string());
    }
    let expected_sha = video
        .sha256
        .as_deref()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "This video has no checksum in the catalogue.".to_string())?;
    let target = downloaded_video_path(&video.id);
    let partial = target.with_extension("mp4.part");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not make the folder: {error}"))?;
    }
    let result = (|| -> Result<(), String> {
        let mut response = http_agent(Duration::from_secs(600))
            .get(&video.video)
            .call()
            .map_err(|error| format!("The download did not start: {error}"))?;
        let total = video.size_bytes.or_else(|| {
            response
                .headers()
                .get("content-length")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok())
        });
        let mut reader = response.body_mut().as_reader();
        let mut file =
            fs::File::create(&partial).map_err(|error| format!("Could not save it: {error}"))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut received = 0_u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("The download stopped: {error}"))?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .map_err(|error| format!("Could not save it: {error}"))?;
            hasher.update(&buffer[..count]);
            received = received.saturating_add(count as u64);
            progress(received, total);
        }
        file.sync_all()
            .map_err(|error| format!("Could not save it: {error}"))?;
        if let Some(size) = video.size_bytes
            && size != received
        {
            return Err("The download was incomplete.".to_string());
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected_sha {
            return Err("The download did not match its checksum.".to_string());
        }
        fs::rename(&partial, &target).map_err(|error| format!("Could not save it: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
        return result;
    }
    for (remote, cached) in [
        (&video.poster, cached_poster_path(&video.id)),
        (&video.preview, cached_preview_path(&video.id)),
    ] {
        if let Some(url) = remote {
            let _ = download_small_file(url, &cached);
        }
    }
    Ok(())
}

fn download_small_file(url: &str, target: &Path) -> Result<(), String> {
    let mut response = http_agent(Duration::from_secs(30))
        .get(url)
        .call()
        .map_err(|error| error.to_string())?;
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|error| error.to_string())?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(target, bytes).map_err(|error| error.to_string())
}

/// Deletes a downloaded video with its poster and preview. The bundled one cannot be removed.
pub(crate) fn remove_video(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("This video's name is not valid.".to_string());
    }
    if bundled_videos().iter().any(|video| video.id == id) {
        return Err("The video that comes with Ghostex cannot be removed.".to_string());
    }
    for path in [
        downloaded_video_path(id),
        cached_poster_path(id),
        cached_preview_path(id),
    ] {
        if path.exists() {
            fs::remove_file(&path).map_err(|error| format!("Could not remove it: {error}"))?;
        }
    }
    Ok(())
}
