use std::{
    env, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use sha2::{Digest as _, Sha256};

use super::*;
#[cfg(target_os = "linux")]
use std::io::Write as _;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

pub(crate) fn emit(
    progress: &mut dyn FnMut(ComponentStoreProgress),
    component: &ComponentDefinition,
    platform: &str,
    size_bytes: u64,
    phase: ComponentStoreProgressPhase,
) {
    progress(ComponentStoreProgress {
        component: component.name.clone(),
        component_version: component.component_version.clone(),
        downloaded_bytes: 0,
        platform: platform.to_string(),
        phase,
        size_bytes,
    });
}

pub(crate) fn emit_download_progress(
    progress: &mut dyn FnMut(ComponentStoreProgress),
    component: &ComponentDefinition,
    platform: &str,
    size_bytes: u64,
    downloaded_bytes: u64,
) {
    progress(ComponentStoreProgress {
        component: component.name.clone(),
        component_version: component.component_version.clone(),
        downloaded_bytes: downloaded_bytes.min(size_bytes),
        platform: platform.to_string(),
        phase: ComponentStoreProgressPhase::Downloading,
        size_bytes,
    });
}

pub(crate) fn component_store_root() -> Result<PathBuf, String> {
    if let Some(override_root) = env::var_os("GHOSTEX_COMPONENT_STORE_DIR") {
        return Ok(PathBuf::from(override_root));
    }
    if let Some(copy) = ghostex_paths::portable::current() {
        return Ok(copy.data_dir.join("components"));
    }
    #[cfg(target_os = "windows")]
    /*
    CDXC:PlatformSupport 2026-08-16:
    Velopack owns %LOCALAPPDATA%\Ghostex and transactionally replaces that
    entire directory during reinstall/update. Keep large on-demand runtimes in
    a sibling data root so the installer cannot discard a verified CEF payload
    and force every downloaded upgrade to fetch Chromium again. A portable copy
    keeps its components in its own data folder (ghostex_paths::portable), which
    Velopack's portable updates leave alone too.
    */
    return ghostex_paths::windows_component_store_dir().ok_or_else(|| {
        "LOCALAPPDATA is unavailable; cannot locate the Ghostex component store".to_string()
    });
    #[cfg(not(target_os = "windows"))]
    return Ok(ghostex_paths::GhostexPaths::resolve_and_migrate()
        .data_dir
        .join("components"));
}

pub(crate) fn legacy_asset_cache_root() -> Result<PathBuf, String> {
    if let Some(override_root) = env::var_os("GHOSTEX_ON_DEMAND_CACHE_DIR") {
        return Ok(PathBuf::from(override_root));
    }
    if let Some(copy) = ghostex_paths::portable::current() {
        return Ok(copy.data_dir.join("on-demand"));
    }
    #[cfg(target_os = "windows")]
    return env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("GhostexData/on-demand"))
        .ok_or_else(|| {
            "LOCALAPPDATA is unavailable; cannot locate the Ghostex release asset cache".to_string()
        });
    #[cfg(not(target_os = "windows"))]
    return Ok(ghostex_paths::GhostexPaths::resolve_and_migrate()
        .data_dir
        .join("on-demand"));
}

pub(crate) fn current_platform() -> Result<String, String> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(
            "This operating system is unsupported by the Ghostex component store".to_string(),
        );
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        return Err(
            "This CPU architecture is unsupported by the Ghostex component store".to_string(),
        );
    };
    Ok(format!("{os}-{arch}"))
}

pub(crate) fn download_url(repo: &str, tag: &str, asset_name: &str) -> String {
    let base = env::var("GHOSTEX_ON_DEMAND_BASE_URL")
        .unwrap_or_else(|_| format!("https://github.com/{repo}/releases/download"));
    format!("{}/{tag}/{asset_name}", base.trim_end_matches('/'))
}

/// CDXC:CefRuntime 2026-10-31 WHY:
/// The Linux setup window says "You're offline" and retries on its own only for a download that never reached the server, and keeps the raw error for everything else (an HTTP error, a bad download, a full disk), which must not read as a connection problem. The download decides it from the typed error, where it is known, and marks its message with this text, which survives the plain-string errors up to the window.
pub(crate) const NO_CONNECTION_MARK: &str = "(no connection)";

/// Whether a component store error is a download that could not reach the server.
pub(crate) fn is_no_connection_failure(message: &str) -> bool {
    message.contains(NO_CONNECTION_MARK)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn ureq_error_is_no_connection(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Timeout(_)
        | ureq::Error::BodyStalled => true,
        ureq::Error::Io(error) => io_error_is_no_connection(error),
        _ => false,
    }
}

/// A socket that could not connect or lost its peer; ureq wraps its own errors in io errors on reads.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn io_error_is_no_connection(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    if let Some(inner) = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ureq::Error>())
    {
        return ureq_error_is_no_connection(inner);
    }
    matches!(
        error.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::TimedOut
            | ErrorKind::NetworkUnreachable
            | ErrorKind::HostUnreachable
            | ErrorKind::NetworkDown
            | ErrorKind::AddrNotAvailable
    )
        // std's fixed text for a getaddrinfo failure (no DNS), which has no ErrorKind of its own.
        || error.to_string().contains("failed to lookup address information")
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn no_connection_mark(is_no_connection: bool) -> String {
    if is_no_connection {
        format!(" {NO_CONNECTION_MARK}")
    } else {
        String::new()
    }
}

pub(crate) fn download(
    url: &str,
    destination: &Path,
    expected_size: u64,
    progress: &mut dyn FnMut(u64),
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        /*
        CDXC:PlatformSupport 2026-08-16:
        Do not resolve and launch an arbitrary curl.exe from the customer's
        PATH. Windows installations already carry Velopack's in-process HTTPS
        stack for signed application updates; use that same deterministic
        downloader for sealed CEF/code-server assets so a broken or shadowed
        system curl cannot strand Ghostex during first launch. The component
        store still owns exact-size and SHA-256 verification after download.
        */
        let mut last_error = None;
        for attempt in 1..=3 {
            // No HttpOptions: no extra headers and no timeout, as before velopack 1.2.161.
            let result =
                velopack::download::download_url_to_file(url, destination, None, |percent: i16| {
                    if expected_size > 0 {
                        let downloaded =
                            expected_size.saturating_mul(percent.clamp(0, 100) as u64) / 100;
                        progress(downloaded);
                    }
                });
            match result {
                Ok(()) => {
                    if expected_size > 0 {
                        progress(expected_size);
                    }
                    return Ok(());
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                    if attempt < 3 {
                        thread::sleep(Duration::from_millis(500 * attempt));
                    }
                }
            }
        }
        return Err(format!(
            "Could not download component asset from {url}: {}",
            last_error.unwrap_or_else(|| "the HTTPS request failed".to_string())
        ));
    }

    #[cfg(target_os = "linux")]
    {
        /*
        CDXC:PlatformSupport 2026-08-16:
        Installed Ghostex must not depend on a PATH-resolved curl process for
        its first-launch CEF download. Stream through an in-process HTTPS
        client using the Linux system certificate verifier, keep the existing
        bounded retries, and surface the actual request/read/write error. The
        component store still verifies the exact byte count and sealed SHA-256
        before the archive can become an installed runtime.
        */
        let tls_config = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(900)))
            .tls_config(tls_config)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut last_error = None;

        for attempt in 1..=3 {
            let result = (|| -> Result<(), String> {
                let mut response = agent.get(url).call().map_err(|error| {
                    format!(
                        "HTTPS request failed{}: {error}",
                        no_connection_mark(ureq_error_is_no_connection(&error))
                    )
                })?;
                let mut file = File::create(destination).map_err(|error| {
                    format!(
                        "Could not create component download {}: {error}",
                        destination.display()
                    )
                })?;
                let mut reader = response.body_mut().as_reader();
                let mut buffer = [0_u8; 64 * 1024];
                let mut downloaded_bytes = 0_u64;

                loop {
                    let count = reader.read(&mut buffer).map_err(|error| {
                        format!(
                            "HTTPS response read failed{}: {error}",
                            no_connection_mark(io_error_is_no_connection(&error))
                        )
                    })?;
                    if count == 0 {
                        break;
                    }
                    file.write_all(&buffer[..count])
                        .map_err(|error| format!("Component download write failed: {error}"))?;
                    downloaded_bytes = downloaded_bytes.saturating_add(count as u64);
                    progress(downloaded_bytes);
                }
                file.sync_all()
                    .map_err(|error| format!("Could not finish component download: {error}"))?;
                Ok(())
            })();

            match result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 3 {
                        thread::sleep(Duration::from_millis(500 * attempt));
                    }
                }
            }
        }

        return Err(format!(
            "Could not download component asset from {url}: {}",
            last_error.unwrap_or_else(|| "the HTTPS request failed".to_string())
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/usr/bin/curl");

        command
            .args([
                "--fail",
                "--location",
                "--retry",
                "2",
                "--max-time",
                "900",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(destination)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not launch component downloader for {url}: {error}"))?;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if expected_size > 0 {
                        let downloaded_bytes = fs::metadata(destination)
                            .map(|metadata| metadata.len())
                            .unwrap_or(0);
                        progress(downloaded_bytes);
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "Could not monitor component downloader for {url}: {error}"
                    ));
                }
            }
        };
        if expected_size > 0 {
            let downloaded_bytes = fs::metadata(destination)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            progress(downloaded_bytes);
        }
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "Could not download component asset from {url}: downloader exited with {status}"
            ))
        }
    }
}

pub(crate) fn verify_file(
    path: &Path,
    expected_sha256: &str,
    expected_size: u64,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "Could not inspect downloaded component asset {}: {error}",
            path.display()
        )
    })?;
    if metadata.len() != expected_size {
        return Err(format!(
            "Downloaded component asset size mismatch: expected {expected_size} bytes, received {} bytes",
            metadata.len()
        ));
    }
    let mut file = File::open(path).map_err(|error| {
        format!(
            "Could not open downloaded component asset {}: {error}",
            path.display()
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            format!(
                "Could not hash downloaded component asset {}: {error}",
                path.display()
            )
        })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let actual = hex::encode(digest.finalize());
    if actual != expected_sha256 {
        return Err("Downloaded component asset failed SHA-256 verification against the app's sealed manifest".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn a_download_that_never_reached_the_server_is_no_connection() {
        // What 10.41.0 printed in the AppImage catalog's offline test: getaddrinfo found no DNS.
        let dns = ureq::Error::Io(IoError::other(
            "failed to lookup address information: Temporary failure in name resolution",
        ));
        assert!(ureq_error_is_no_connection(&dns));
        for error in [
            ureq::Error::HostNotFound,
            ureq::Error::ConnectionFailed,
            ureq::Error::BodyStalled,
            ureq::Error::Io(IoError::from(ErrorKind::NetworkUnreachable)),
            ureq::Error::Io(IoError::from(ErrorKind::ConnectionRefused)),
            ureq::Error::Io(IoError::from(ErrorKind::TimedOut)),
        ] {
            assert!(ureq_error_is_no_connection(&error), "{error}");
        }
        // A connection lost while reading the body: ureq hands its error back inside an io error.
        assert!(io_error_is_no_connection(&IoError::other(
            ureq::Error::ConnectionFailed
        )));
        assert!(io_error_is_no_connection(&IoError::from(
            ErrorKind::ConnectionReset
        )));
    }

    #[test]
    fn a_server_answer_a_bad_download_or_a_full_disk_is_not_no_connection() {
        for error in [
            ureq::Error::StatusCode(404),
            ureq::Error::StatusCode(503),
            ureq::Error::BadUri("nope".to_string()),
            ureq::Error::Tls("certificate rejected"),
            ureq::Error::Io(IoError::from(ErrorKind::StorageFull)),
            ureq::Error::Io(IoError::from(ErrorKind::PermissionDenied)),
        ] {
            assert!(!ureq_error_is_no_connection(&error), "{error}");
        }
        assert!(!io_error_is_no_connection(&IoError::from(
            ErrorKind::UnexpectedEof
        )));
    }

    #[test]
    fn the_mark_survives_into_the_message_the_window_reads() {
        let error = ureq::Error::Io(IoError::other(
            "failed to lookup address information: Temporary failure in name resolution",
        ));
        let message = format!(
            "Could not download component asset from https://github.com/x: HTTPS request failed{}: {error}",
            no_connection_mark(ureq_error_is_no_connection(&error))
        );
        assert!(is_no_connection_failure(&message), "{message}");
        let not_found = format!(
            "HTTPS request failed{}: {}",
            no_connection_mark(ureq_error_is_no_connection(&ureq::Error::StatusCode(404))),
            ureq::Error::StatusCode(404)
        );
        assert!(!is_no_connection_failure(&not_found), "{not_found}");
        assert!(!is_no_connection_failure(
            "Component download write failed: No space left on device (os error 28)"
        ));
        assert!(!is_no_connection_failure(
            "Downloaded component asset failed SHA-256 verification against the app's sealed manifest"
        ));
    }
}
