//! Finding and installing Hex: the release feed's download is resolved and checked, an older Hex
//! (no local API) is told apart from Hex 3, and a download that isn't Hex's (wrong checksum, no
//! Hex signature) is refused before anything is put in place. Every install here goes into a temp
//! folder from a fake release feed; nothing touches the Hex installed on this computer.

use serde_json::json;

use super::hex::{manifest_target, Target};

fn manifest(url: &str) -> serde_json::Value {
    json!({
        "schema": 1,
        "app": "hex",
        "channel": "stable",
        "release": {
            "version": "3.3.0",
            "targets": {
                "macos-arm64": {"url": url, "size": 27081045, "sha256": "1C28D6A1E790323EF199B481BC9231E6AD8E70A1785BE84913E0A7BF0A6EBB39"}
            }
        }
    })
}

#[test]
fn the_feed_download_is_resolved_against_the_feed() {
    let target = manifest_target(
        &manifest("/release/hex/files/1c28/Hex.zip"),
        "macos-arm64",
        "https://anoma.ly",
    )
    .expect("target");
    assert_eq!(
        target,
        Target {
            version: "3.3.0".into(),
            url: "https://anoma.ly/release/hex/files/1c28/Hex.zip".into(),
            size: 27081045,
            sha256: "1c28d6a1e790323ef199b481bc9231e6ad8e70a1785be84913e0a7bf0a6ebb39".into(),
        }
    );
}

#[test]
fn a_feed_without_https_a_size_or_a_checksum_is_refused() {
    assert!(manifest_target(
        &manifest("http://example.com/Hex.zip"),
        "macos-arm64",
        "https://anoma.ly"
    )
    .is_err());
    assert!(manifest_target(&manifest("/Hex.zip"), "windows-x64", "https://anoma.ly").is_err());
    let mut no_hash = manifest("/Hex.zip");
    no_hash["release"]["targets"]["macos-arm64"]["sha256"] = json!("abc");
    assert!(manifest_target(&no_hash, "macos-arm64", "https://anoma.ly").is_err());
    let mut no_size = manifest("/Hex.zip");
    no_size["release"]["targets"]["macos-arm64"]["size"] = json!(0);
    assert!(manifest_target(&no_size, "macos-arm64", "https://anoma.ly").is_err());
}

#[cfg(unix)]
#[test]
fn a_direct_linux_install_says_its_version_through_its_current_link() {
    if super::platform::os() == super::platform::Os::Windows {
        return;
    }
    let support = tempfile::tempdir().expect("temp");
    std::fs::create_dir_all(support.path().join("versions/3.3.0")).unwrap();
    std::os::unix::fs::symlink("versions/3.3.0", support.path().join("current")).unwrap();
    assert_eq!(
        super::hex::direct_version(support.path()).as_deref(),
        Some("3.3.0")
    );
}

#[cfg(target_os = "macos")]
mod mac {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::super::hex::{find_mac, install_mac, Found};
    use super::super::jobs::Log;

    /// A minimal `Hex.app` with `bundle_id`, as a folder inside `dir`.
    fn fake_app(dir: &Path, bundle_id: &str, version: &str) -> PathBuf {
        let contents = dir.join("Hex.app").join("Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{bundle_id}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
<key>CFBundleExecutable</key><string>hex</string>
</dict></plist>"#
            ),
        )
        .unwrap();
        std::fs::write(contents.join("MacOS").join("hex"), "#!/bin/sh\n").unwrap();
        dir.join("Hex.app")
    }

    #[test]
    fn hex_3_and_the_older_hex_are_told_apart_by_bundle_id() {
        let new = tempfile::tempdir().unwrap();
        let old = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let new_app = fake_app(new.path(), "anomaly.hex", "3.3.0");
        let old_app = fake_app(old.path(), "com.kitlangton.Hex", "0.6.2");
        assert_eq!(
            find_mac(&[empty.path().to_path_buf(), new.path().to_path_buf()]),
            Some(Found::Current {
                version: Some("3.3.0".into()),
                path: new_app,
                packaged: false
            })
        );
        assert_eq!(
            find_mac(&[old.path().to_path_buf()]),
            Some(Found::Older {
                version: Some("0.6.2".into()),
                path: old_app
            })
        );
        assert_eq!(find_mac(&[empty.path().to_path_buf()]), None);
    }

    /// A release feed on a local port serving `stable.json` and one zip.
    fn fake_feed(zip: Vec<u8>, sha256: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let size = zip.len();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).is_err()
                        || header == "\r\n"
                        || header.is_empty()
                    {
                        break;
                    }
                }
                let body: Vec<u8> = if line.contains("/release/hex/desktop/stable.json") {
                    serde_json::to_vec(&serde_json::json!({
                        "release": {"version": "3.3.0", "targets": {"macos-arm64": {
                            "url": "/release/hex/files/x/Hex.zip", "size": size, "sha256": sha256
                        }}}
                    }))
                    .unwrap()
                } else {
                    zip.clone()
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
                let _ = reader.read(&mut [0u8; 1]);
            }
        });
        origin
    }

    fn zipped_fake_hex() -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let app = fake_app(dir.path(), "anomaly.hex", "3.3.0");
        let zip = dir.path().join("Hex.zip");
        let status = Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(&app)
            .arg(&zip)
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::read(zip).unwrap()
    }

    fn sha256(bytes: &[u8]) -> String {
        use sha2::Digest as _;
        hex::encode(sha2::Sha256::digest(bytes))
    }

    #[test]
    fn an_app_without_hex_signature_is_refused_and_nothing_is_installed() {
        let zip = zipped_fake_hex();
        let origin = fake_feed(zip.clone(), sha256(&zip));
        let apps = tempfile::tempdir().unwrap();
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let sink = lines.clone();
        let log = Log::new(move |line| sink.lock().unwrap().push(line));
        let error = install_mac(&origin, apps.path(), false, &log).expect_err("unsigned");
        assert!(error.contains("isn't signed by Hex's developer"), "{error}");
        assert!(!apps.path().join("Hex.app").exists());
        assert_eq!(std::fs::read_dir(apps.path()).unwrap().count(), 0);
        assert!(lines
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("Checksum verified")));
    }

    #[test]
    fn a_download_that_does_not_match_its_checksum_is_refused() {
        let zip = zipped_fake_hex();
        let origin = fake_feed(zip, "0".repeat(64));
        let apps = tempfile::tempdir().unwrap();
        let error =
            install_mac(&origin, apps.path(), false, &Log::new(|_| {})).expect_err("mismatch");
        assert!(
            error.contains("did not match its published checksum"),
            "{error}"
        );
        assert_eq!(std::fs::read_dir(apps.path()).unwrap().count(), 0);
    }
}
