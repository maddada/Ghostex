//! HTTPS. Browsers only decode video on secure pages, so a viewer on another machine needs
//! voxl over HTTPS. With `--tls-cert` and `--tls-key` voxl serves the given PEM certificate
//! chain and key; with `--tls` alone it makes a self-signed certificate once, keeps it (so a
//! browser's exception for it lasts across restarts) and serves that.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_rustls::rustls;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// The TLS settings the flags ask for, or `None` to serve plain HTTP.
/// `names` go into a newly made self-signed certificate.
pub fn from_args(
    a: &HashMap<String, String>,
    names: Vec<String>,
) -> Option<Arc<rustls::ServerConfig>> {
    let (cert, key) = match (a.get("tls-cert"), a.get("tls-key")) {
        (Some(c), Some(k)) => (PathBuf::from(c), PathBuf::from(k)),
        (Some(_), None) | (None, Some(_)) => {
            panic!("--tls-cert and --tls-key go together: give both, or --tls for a self-signed certificate")
        }
        (None, None) if crate::truthy(a.get("tls"), false) => {
            // One file holds both the certificate and its key.
            let pem = self_signed(&tls_dir(), names);
            (pem.clone(), pem)
        }
        (None, None) => return None,
    };
    let config = load(&cert, &key);
    log::info!("HTTPS with the certificate {}", cert.display());
    Some(config)
}

/// The server config for a PEM certificate chain and key (which may share a file).
fn load(cert: &Path, key: &Path) -> Arc<rustls::ServerConfig> {
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
        .and_then(|it| it.collect())
        .unwrap_or_else(|e| panic!("reading the certificate {}: {e}", cert.display()));
    let key = PrivateKeyDer::from_pem_file(key)
        .unwrap_or_else(|e| panic!("reading the key {}: {e}", key.display()));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .unwrap_or_else(|e| panic!("the certificate and key don't work together: {e}"));
    // HTTP/1.1 only: the stream is a WebSocket, which browsers open over HTTP/1.1.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

/// Where voxl keeps the certificate it makes: %LOCALAPPDATA%\voxl on Windows,
/// $XDG_CONFIG_HOME/voxl or ~/.config/voxl elsewhere, or VOXL_TLS_DIR.
fn tls_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VOXL_TLS_DIR") {
        return d.into();
    }
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.unwrap_or_else(|| PathBuf::from(".")).join("voxl")
}

/// The kept self-signed certificate and key, as one PEM file in `dir`, made on first use.
///
/// Instances starting together each write a whole pair to a file of their own, then
/// publish it under the shared name only if nothing is there yet (a hard link, which
/// doesn't replace). The first one wins and every instance uses its file, so none can
/// read a partly written pair or a certificate from one instance with another's key.
fn self_signed(dir: &Path, names: Vec<String>) -> PathBuf {
    let pem = dir.join("voxl-tls.pem");
    if pem.exists() {
        return pem;
    }
    std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("creating {}: {e}", dir.display()));
    let made = rcgen::generate_simple_self_signed(names.clone()).expect("making a certificate");
    let pair = made.cert.pem() + &made.key_pair.serialize_pem();
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).expect("random");
    let own = dir.join(format!(".voxl-tls-{:016x}.tmp", u64::from_le_bytes(nonce)));
    write_private(&own, pair.as_bytes())
        .unwrap_or_else(|e| panic!("writing {}: {e}", own.display()));
    let published = match std::fs::hard_link(&own, &pem) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
        // A file system without hard links: an atomic rename still leaves one whole pair.
        Err(_) => std::fs::rename(&own, &pem).is_ok(),
    };
    let _ = std::fs::remove_file(&own);
    if published {
        log::info!(
            "made a self-signed certificate for {} in {}; browsers warn about it once",
            names.join(", "),
            pem.display()
        );
    }
    pem
}

/// Writes a new file only its owner can read (on Unix; on Windows the file inherits the
/// per-user %LOCALAPPDATA% permissions) and flushes it to disk.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    std::io::Write::write_all(&mut file, data)?;
    file.sync_all()
}

/// Serves `app` over TLS on `listener` until the process ends.
pub async fn serve(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    config: Arc<rustls::ServerConfig>,
) {
    let acceptor = tokio_rustls::TlsAcceptor::from(config);
    loop {
        let tcp = match listener.accept().await {
            Ok((tcp, _)) => tcp,
            Err(e) => {
                log::warn!("accept: {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let _ = tcp.set_nodelay(true);
        let acceptor = acceptor.clone();
        let app = app.clone();
        tokio::spawn(async move {
            // A client that never finishes the handshake doesn't hold the task forever.
            let stream =
                match tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => return log::debug!("TLS handshake: {e}"),
                    Err(_) => return,
                };
            let service = hyper_util::service::TowerToHyperService::new(app);
            let _ =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                    .serve_connection_with_upgrades(hyper_util::rt::TokioIo::new(stream), service)
                    .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Instances starting together all end up with the same, working certificate and key.
    #[test]
    fn concurrent_first_starts_share_one_pair() {
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let dir =
            std::env::temp_dir().join(format!("voxl-tls-test-{:016x}", u64::from_le_bytes(nonce)));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let paths: Vec<PathBuf> = (0..8)
            .map(|_| {
                let (dir, barrier) = (dir.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    let pem = self_signed(&dir, vec!["localhost".into()]);
                    load(&pem, &pem);
                    pem
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect();
        assert!(paths.iter().all(|p| p == &paths[0]));
        // Only the shared file is left: every instance removed its own.
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("voxl-tls.pem")]);
        // A later start reuses it.
        load(&self_signed(&dir, vec![]), &paths[0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
