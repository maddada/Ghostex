//! The macOS code-signing identity local starts sign with.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{output, Res};

/// CDXC:Build 2026-09-22 WHY:
/// macOS keys folder permissions (Documents, removable volumes, and the rest of TCC) to the app's designated requirement. A certificate-signed build keeps one requirement across rebuilds; an ad-hoc build's requirement is its cdhash, so every rebuild is a new app to TCC and the permission prompts come back on each restart and session switch.
/// A start whose shell could not use the keychain used to warn once and install an ad-hoc build over the certificate-signed one, wiping the grants; and once the install was ad-hoc, every later keychain-less start stayed ad-hoc without a word because nothing recorded that this computer has a certificate. The identity of every successful certificate start is now remembered per computer, and ad-hoc is only for a computer that has never signed with a certificate, or an explicit GHOSTEX_GPUI_SIGN_IDENTITY=-. Every other case stops here, before the build, and prints codesign's and security's own output so the keychain problem gets fixed instead of hidden. Supersedes the 2026-08-25 fall-back-to-ad-hoc behaviour.
pub fn resolve_identity(installed_app_path: &Path) -> Res<String> {
    if let Some(value) = std::env::var_os("GHOSTEX_GPUI_SIGN_IDENTITY") {
        return Ok(value.to_string_lossy().into_owned());
    }
    let (identities, listing_output) = list_identities();
    let mut probe_failures = Vec::new();
    for identity in preferred(&identities) {
        match signing_failure(&identity) {
            None => {
                remember(&identity)?;
                return Ok(identity);
            }
            Some(failure) => probe_failures.push(format!("  {identity}: {failure}")),
        }
    }
    let installed_authority = read_certificate_authority(installed_app_path);
    let remembered = remembered();
    if !probe_failures.is_empty() || installed_authority.is_some() || remembered.is_some() {
        let mut lines = vec![if !probe_failures.is_empty() {
            format!(
                "This shell lists code-signing identities but cannot sign with any of them:\n{}",
                probe_failures.join("\n")
            )
        } else if let Some(authority) = installed_authority {
            format!(
                "{} is signed with \"{authority}\", but this shell lists no code-signing identity.",
                installed_app_path.display()
            )
        } else {
            format!(
                "This computer signs local Ghostex builds with \"{}\" (remembered in {}), but this shell lists no code-signing identity.",
                remembered.unwrap_or_default(),
                identity_record_path().display()
            )
        }];
        if !listing_output.is_empty() {
            lines.push(format!(
                "security find-identity -v -p codesigning said:\n{}",
                indent(&listing_output)
            ));
        }
        lines.push(format!(
            "security list-keychains said:\n{}",
            indent(&keychain_search_list())
        ));
        lines.push(
            "An ad-hoc build would make macOS forget the app's folder permissions and ask again after every rebuild.".into(),
        );
        lines.push(
            "Fix keychain access for this shell (unlock the login keychain, or run outside a sandboxed agent shell), or set GHOSTEX_GPUI_SIGN_IDENTITY=- to install an ad-hoc build on purpose.".into(),
        );
        return Err(lines.join("\n").into());
    }
    eprintln!(
        "No Apple code-signing identity was found; using ad-hoc GPUI signing. macOS will ask for permissions again after GPUI rebuilds."
    );
    Ok("-".into())
}

pub fn timestamp_flag() -> String {
    match std::env::var_os("GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG") {
        Some(value) => value.to_string_lossy().into_owned(),
        None => "--timestamp=none".into(),
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn identity_record_path() -> PathBuf {
    crate::util::xdg_state_root()
        .join("ghostex")
        .join("local-start")
        .join("macos-code-sign-identity")
}

pub(crate) fn remembered() -> Option<String> {
    let text = fs::read_to_string(identity_record_path()).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn remember(identity: &str) -> Res {
    let path = identity_record_path();
    fs::create_dir_all(path.parent().expect("identity record has a parent"))?;
    fs::write(path, format!("{identity}\n"))?;
    Ok(())
}

fn keychain_search_list() -> String {
    match output(Command::new("security").arg("list-keychains")) {
        Ok(out) => format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .trim()
        .to_string(),
        Err(error) => error.to_string(),
    }
}

/// The first `Authority=` line of a signature, when the code is certificate-signed.
pub fn read_certificate_authority(code_path: &Path) -> Option<String> {
    let details = signature_details(code_path)?;
    details
        .lines()
        .find_map(|line| line.strip_prefix("Authority=").map(str::to_string))
}

/// `codesign -dv --verbose=4` output (stderr then stdout), or None when the path is not signed code.
pub fn signature_details(code_path: &Path) -> Option<String> {
    let out = output(
        Command::new("codesign")
            .args(["-dv", "--verbose=4"])
            .arg(code_path),
    )
    .ok()?;
    out.status.success().then(|| {
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

fn preferred(identities: &[String]) -> Vec<String> {
    let mut picked: Vec<String> = Vec::new();
    for prefix in [
        "Apple Development: ",
        "Mac Developer: ",
        "Developer ID Application: ",
        "Apple Distribution: ",
    ] {
        for name in identities {
            if name.starts_with(prefix) && !picked.contains(name) {
                picked.push(name.clone());
            }
        }
    }
    picked
}

/// CDXC:Build 2026-08-25 WHY:
/// `security find-identity -v -p codesigning` can list certificates that `codesign --sign` then rejects (missing private key, locked keychain, or a stale listing), so each identity is probed on a throwaway file before the build instead of failing after the full rebuild.
/// Returns None when the identity signs, otherwise codesign's error text.
pub(crate) fn signing_failure(identity: &str) -> Option<String> {
    let probe_dir = std::env::temp_dir().join(format!(
        "ghostex-gpui-sign-{}-{}",
        std::process::id(),
        crate::util::unix_millis()
    ));
    let probe = probe_dir.join("probe");
    let result = (|| -> Res<Option<String>> {
        fs::create_dir_all(&probe_dir)?;
        fs::write(&probe, "ghostex-gpui-codesign-probe\n")?;
        let out = output(
            Command::new("codesign")
                .args(["--force", "--sign", identity, "--timestamp=none"])
                .arg(&probe),
        )?;
        if out.status.success() {
            return Ok(None);
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Ok(Some(if stderr.is_empty() {
            format!(
                "codesign exited with status {}",
                out.status.code().unwrap_or(1)
            )
        } else {
            stderr
        }))
    })();
    let _ = fs::remove_dir_all(&probe_dir);
    result.unwrap_or_else(|error| Some(error.to_string()))
}

fn list_identities() -> (Vec<String>, String) {
    let out =
        match output(Command::new("security").args(["find-identity", "-v", "-p", "codesigning"])) {
            Ok(out) => out,
            Err(error) => return (Vec::new(), error.to_string()),
        };
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let combined = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr))
        .trim()
        .to_string();
    if !out.status.success() {
        return (Vec::new(), combined);
    }
    // Lines look like `  1) 0123ABCD... "Apple Development: Name (TEAM)"`.
    let identities = stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (number, rest) = line.split_once(')')?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let rest = rest.trim_start();
            let (hash, rest) = rest.split_once(char::is_whitespace)?;
            if hash.is_empty() || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let rest = rest.trim_start().strip_prefix('"')?;
            let name = &rest[..rest.find('"')?];
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect();
    (identities, combined)
}
