//! The guest page, pinned to this release (GitHub #495, Q1722 "pinned on the relay").
//!
//! The share relay serves the page a guest opens (the GPUI web build in guest mode), and that
//! page reads the link's key from its `#fragment`. A relay that served a changed page could send
//! the key, and the code typed into it, anywhere. So this release carries the exact page it trusts:
//! [`MANIFEST`] (`guest_page.manifest`, committed beside this file) lists every file of the page
//! with its SHA-256, in `shasum -a 256` form sorted by path, and the page's hash is the SHA-256 of
//! that manifest. Its first 16 hex characters are the page id in every link,
//! `https://share.ghostex.dev/s/<page id>/#<key>`.
//!
//! Before gxserver registers any shared chat with a relay, and again every [`RECHECK_MS`] while a
//! chat is shared, it downloads that page from the relay, file by file, the way a guest's browser
//! would, and compares every byte with the manifest ([`verify`]). Any difference, a missing file
//! or an unreachable relay: no route is registered, so no guest can reach a chat through that page
//! and the owner is told why (`/api/collab` `guestPage`). The relay itself serves only page folders
//! whose files hash to their folder's name, and the page checks its own script (Subresource
//! Integrity in `index.html`) and its WebAssembly (a SHA-256 built into the script) before running
//! them, so a page that starts from the right `index.html` runs only the release's code.
//!
//! What this does not protect, said plainly (and in Help): a relay operator who serves the right
//! page to this computer's checks and a changed one to a guest's browser. A browser can't prove
//! which page it got; anyone can compare the page they were served with the published hash
//! (`server/share-relay/README.md`), but a guest won't. The check makes a changed page visible
//! the moment it is served to everyone, and stops this computer from sharing through it.
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (#495, Q1722): "Guest page: pinned on the relay (option A): the relay serves it, and the owner's side pins it to the release's page hash; a changed page is refused."
//! SEE-ALSO: server/share-relay/src/main.rs (versioned page folders), apps/gpui-web/www/vite.config.js (integrity and the WebAssembly hash), server/share-relay/build-linux.sh (writes the manifest).

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Every file of the guest page this release trusts: `<sha256>  <path>` lines sorted by path.
pub const MANIFEST: &str = include_str!("guest_page.manifest");
/// How long a passed check holds before the page is downloaded again.
pub const RECHECK_MS: i64 = 60 * 60_000;
/// How soon a failed check is tried again.
pub const RETRY_MS: i64 = 10 * 60_000;
/// Largest page file the check reads (the WebAssembly is the big one).
const MAX_FILE_BYTES: u64 = 96 << 20;

/// What the last check of the relay's page found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinState {
    Unchecked,
    /// Every file matched.
    Verified {
        at_ms: i64,
    },
    /// The relay served something else: the first file that differed, or was missing.
    Mismatch {
        path: String,
        at_ms: i64,
    },
    /// The relay could not be reached or answered with an error.
    Unreachable {
        message: String,
        at_ms: i64,
    },
}

/// SHA-256 of [`MANIFEST`], hex: the page hash the relay's folder is named after.
pub fn page_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hex::encode(Sha256::digest(MANIFEST.as_bytes())))
}

/// The page id in links: the page hash's first 16 hex characters, or `dev` for an unpinned
/// development build ([`unpinned`]).
pub fn page_id() -> &'static str {
    if unpinned() {
        return "dev";
    }
    &page_hash()[..16]
}

/// A development build (never a release) started with `GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE=1`
/// skips the check and links to the relay's `dev` page (`--dev-page`), for testing a page that
/// is still changing.
pub fn unpinned() -> bool {
    cfg!(debug_assertions)
        && std::env::var("GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE").is_ok_and(|value| value == "1")
}

/// The manifest's files, `(sha256 hex, path)`, as written.
pub fn manifest_files(manifest: &str) -> Vec<(&str, &str)> {
    manifest
        .lines()
        .filter_map(|line| {
            let (hash, path) = line.split_once("  ")?;
            (hash.len() == 64 && !path.is_empty()).then_some((hash, path))
        })
        .collect()
}

/// Checks the page a relay serves under `base` (`<relay>/s/<page id>/`) against `manifest`.
/// `fetch` downloads one URL. The folder's own address must answer with `index.html` (that is
/// what a link opens), and every listed file must match.
pub fn verify(
    manifest: &str,
    base: &str,
    fetch: impl Fn(&str) -> Result<Vec<u8>, String>,
    now_ms: i64,
) -> PinState {
    let files = manifest_files(manifest);
    let Some((index_hash, _)) = files.iter().find(|(_, path)| *path == "index.html") else {
        return PinState::Mismatch {
            path: "index.html (this build has no pinned guest page)".to_string(),
            at_ms: now_ms,
        };
    };
    let base = base.trim_end_matches('/');
    let checks = std::iter::once((*index_hash, String::new())).chain(
        files
            .iter()
            .map(|(hash, path)| (*hash, (*path).to_string())),
    );
    for (hash, path) in checks {
        let url = format!("{base}/{path}");
        match fetch(&url) {
            Ok(bytes) => {
                if hex::encode(Sha256::digest(&bytes)) != hash {
                    return PinState::Mismatch {
                        path: if path.is_empty() {
                            "/".to_string()
                        } else {
                            path
                        },
                        at_ms: now_ms,
                    };
                }
            }
            Err(message) => {
                return PinState::Unreachable {
                    message: format!("{url}: {message}"),
                    at_ms: now_ms,
                }
            }
        }
    }
    PinState::Verified { at_ms: now_ms }
}

fn state_cell() -> &'static Mutex<PinState> {
    static STATE: OnceLock<Mutex<PinState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(PinState::Unchecked))
}

pub fn state() -> PinState {
    state_cell()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn set_state(next: PinState) {
    *state_cell().lock().unwrap_or_else(PoisonError::into_inner) = next;
}

/// Forgets the last check, so the next share downloads the page again.
pub fn invalidate() {
    set_state(PinState::Unchecked);
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let agent = crate::http_agent::answering_agent();
    let response = agent
        .get(url)
        .header("cache-control", "no-cache")
        .config()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .call()
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(response.into_body().into_reader(), MAX_FILE_BYTES + 1),
        &mut bytes,
    )
    .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("file too large".to_string());
    }
    Ok(bytes)
}

/// `true` when the relay's page matches this release: from the last check while it is fresh,
/// otherwise after downloading it again. Runs the downloads off the async threads.
pub async fn ensure_verified(relay_url: &str) -> bool {
    if unpinned() {
        return true;
    }
    let now = now_ms();
    let fresh = match state() {
        PinState::Verified { at_ms } => now - at_ms < RECHECK_MS,
        PinState::Mismatch { at_ms, .. } | PinState::Unreachable { at_ms, .. } => {
            now - at_ms < RETRY_MS
        }
        PinState::Unchecked => false,
    };
    if !fresh {
        let base = format!("{relay_url}/s/{}/", page_id());
        let next = tokio::task::spawn_blocking(move || verify(MANIFEST, &base, fetch, now_ms()))
            .await
            .unwrap_or(PinState::Unreachable {
                message: "The check stopped.".to_string(),
                at_ms: now,
            });
        if !matches!(next, PinState::Verified { .. }) {
            eprintln!("[collab] the share relay's guest page failed its check: {next:?}");
        }
        set_state(next);
    }
    matches!(state(), PinState::Verified { .. })
}

/// `guestPage` in `/api/collab`'s read: the pinned hash and what the last check found, in words
/// for Settings > Collab and the Share popover.
pub fn status_json() -> Value {
    let (status, message) = match state() {
        PinState::Unchecked => ("unchecked", None),
        PinState::Verified { .. } => ("verified", None),
        PinState::Mismatch { path, .. } => (
            "mismatch",
            Some(format!(
                "The share relay's guest page isn't the one this version of Ghostex trusts ({path} differs), so sharing is paused. Update Ghostex, or ask the relay's operator to serve this version's page."
            )),
        ),
        PinState::Unreachable { message, .. } => (
            "unreachable",
            Some(format!(
                "Ghostex couldn't check the share relay's guest page, so sharing waits: {message}"
            )),
        ),
    };
    json!({
        "status": if unpinned() { "unpinned" } else { status },
        "hash": page_hash(),
        "pageId": page_id(),
        "message": message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn manifest_of(files: &[(&str, &[u8])]) -> String {
        let mut lines: Vec<String> = files
            .iter()
            .map(|(path, bytes)| format!("{}  {path}", hex::encode(Sha256::digest(bytes))))
            .collect();
        lines.sort_by(|a, b| a[66..].cmp(&b[66..]));
        lines.join("\n") + "\n"
    }

    fn served(files: &[(&str, &[u8])]) -> HashMap<String, Vec<u8>> {
        let mut map: HashMap<String, Vec<u8>> = files
            .iter()
            .map(|(path, bytes)| (format!("https://relay/s/abc/{path}"), bytes.to_vec()))
            .collect();
        if let Some((_, index)) = files.iter().find(|(path, _)| *path == "index.html") {
            map.insert("https://relay/s/abc/".to_string(), index.to_vec());
        }
        map
    }

    const PAGE: &[(&str, &[u8])] = &[
        ("index.html", b"<html>page</html>"),
        ("assets/index-1.js", b"console.log(1)"),
        ("assets/app-2.wasm", b"\0asm"),
    ];

    #[test]
    fn the_same_page_passes() {
        let manifest = manifest_of(PAGE);
        let relay = served(PAGE);
        let fetch = |url: &str| relay.get(url).cloned().ok_or_else(|| "404".to_string());
        assert_eq!(
            verify(&manifest, "https://relay/s/abc/", fetch, 5),
            PinState::Verified { at_ms: 5 }
        );
    }

    #[test]
    fn a_changed_file_is_refused() {
        let manifest = manifest_of(PAGE);
        let mut relay = served(PAGE);
        relay.insert(
            "https://relay/s/abc/assets/app-2.wasm".to_string(),
            b"\0asm evil".to_vec(),
        );
        let fetch = |url: &str| relay.get(url).cloned().ok_or_else(|| "404".to_string());
        assert!(matches!(
            verify(&manifest, "https://relay/s/abc/", fetch, 5),
            PinState::Mismatch { path, .. } if path == "assets/app-2.wasm"
        ));
    }

    #[test]
    fn a_changed_entry_page_is_refused() {
        // The folder's address (what a link opens) must be the pinned index.html too.
        let manifest = manifest_of(PAGE);
        let mut relay = served(PAGE);
        relay.insert(
            "https://relay/s/abc/".to_string(),
            b"<html>evil</html>".to_vec(),
        );
        let fetch = |url: &str| relay.get(url).cloned().ok_or_else(|| "404".to_string());
        assert!(matches!(
            verify(&manifest, "https://relay/s/abc", fetch, 5),
            PinState::Mismatch { path, .. } if path == "/"
        ));
    }

    #[test]
    fn a_missing_file_or_page_is_not_trusted() {
        let manifest = manifest_of(PAGE);
        let fetch = |_: &str| Err::<Vec<u8>, _>("HTTP 404".to_string());
        assert!(matches!(
            verify(&manifest, "https://relay/s/abc/", fetch, 5),
            PinState::Unreachable { .. }
        ));
        let fetch = |_: &str| Ok(Vec::new());
        assert!(matches!(
            verify("", "https://relay/s/abc/", fetch, 5),
            PinState::Mismatch { .. }
        ));
    }

    #[test]
    fn the_built_in_manifest_is_well_formed() {
        let files = manifest_files(MANIFEST);
        assert!(
            files.iter().any(|(_, path)| *path == "index.html"),
            "guest_page.manifest lists index.html"
        );
        let mut paths: Vec<&str> = files.iter().map(|(_, path)| *path).collect();
        let listed = paths.clone();
        paths.sort_unstable();
        assert_eq!(
            paths, listed,
            "sorted by path, as shasum over a sorted list writes it"
        );
        assert_eq!(page_id().len(), 16);
    }
}
